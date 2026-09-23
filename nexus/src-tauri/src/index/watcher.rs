//! Filesystem watcher with a 400 ms trailing debounce. Handles Modify *and*
//! Rename events (editors save atomically via rename). Backends: inotify on
//! Linux, ReadDirectoryChangesW on Windows — both via `notify`.

use super::worker::Job;
use super::Priority;
use crate::fs as vfs;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub const DEBOUNCE: Duration = Duration::from_millis(400);

pub struct Watcher {
    _inner: RecommendedWatcher,
    stop: Sender<()>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

pub fn start(root: &Path, jobs: Sender<Job>) -> notify::Result<Watcher> {
    let (ev_tx, ev_rx) = mpsc::channel::<notify::Result<Event>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let mut inner = notify::recommended_watcher(move |res| {
        let _ = ev_tx.send(res);
    })?;
    inner.watch(root, RecursiveMode::Recursive)?;

    let root: PathBuf = root.to_path_buf();
    std::thread::Builder::new()
        .name("nexus-watch".into())
        .spawn(move || {
            let mut pending: HashSet<String> = HashSet::new();
            let mut deadline: Option<Instant> = None;
            loop {
                if stop_rx.try_recv().is_ok() {
                    return;
                }
                let timeout = deadline.map(|d| d.saturating_duration_since(Instant::now())).unwrap_or(Duration::from_millis(250));
                match ev_rx.recv_timeout(timeout) {
                    Ok(Ok(ev)) => {
                        if relevant(&ev.kind) {
                            for p in &ev.paths {
                                if let Some(rel) = vfs::to_rel(&root, p) {
                                    if !vfs::is_ignored_rel(&rel) && !rel.ends_with(".tmp") {
                                        pending.insert(rel);
                                    }
                                }
                            }
                            if !pending.is_empty() {
                                deadline = Some(Instant::now() + DEBOUNCE);
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        log::warn!("watch error: {e}; rescanning");
                        let _ = jobs.send(Job::Scan);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                if deadline.is_some_and(|d| Instant::now() >= d) {
                    deadline = None;
                    for rel in pending.drain() {
                        if jobs.send(Job::Touch { rel, prio: Priority::Recent }).is_err() {
                            return;
                        }
                    }
                }
            }
        })
        .map_err(|e| notify::Error::generic(&e.to_string()))?;
    Ok(Watcher { _inner: inner, stop: stop_tx })
}

fn relevant(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any)
}
