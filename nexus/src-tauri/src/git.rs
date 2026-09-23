//! Auto-commit on save when the vault root is a git repository. Uses git2
//! only — never shells out (spec §8 rule 9). Saves are coalesced over a short
//! window so a burst of autosaves becomes one commit.

use crate::error::{NexusError, Result};
use git2::{IndexAddOption, Repository, Signature};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

pub const COALESCE: Duration = Duration::from_millis(1500);

enum Msg {
    Changed(String),
    Flush(Sender<Result<Option<String>, String>>),
    Shutdown,
}

pub struct GitSync {
    tx: Sender<Msg>,
    handle: parking_lot::Mutex<Option<std::thread::JoinHandle<()>>>,
}

pub fn is_repo(root: &Path) -> bool {
    Repository::open(root).is_ok()
}

pub fn init(root: &Path) -> Result<()> {
    Repository::init(root).map_err(git_err)?;
    Ok(())
}

fn git_err(e: git2::Error) -> NexusError {
    NexusError::Other(format!("git: {}", e.message()))
}

impl GitSync {
    /// Start the committer if `root` itself is a repository's work tree.
    pub fn open(root: &Path) -> Option<GitSync> {
        let repo = Repository::open(root).ok()?;
        if repo.is_bare() {
            return None;
        }
        let workdir = repo.workdir()?.to_path_buf();
        drop(repo);
        let (tx, rx) = mpsc::channel::<Msg>();
        let root = root.to_path_buf();
        let handle = std::thread::Builder::new()
            .name("nexus-git".into())
            .spawn(move || {
                let mut pending: BTreeSet<String> = BTreeSet::new();
                let mut deadline: Option<Instant> = None;
                loop {
                    let msg = match deadline {
                        Some(d) => rx.recv_timeout(d.saturating_duration_since(Instant::now())),
                        None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    };
                    match msg {
                        Ok(Msg::Changed(p)) => {
                            pending.insert(p);
                            deadline = Some(Instant::now() + COALESCE);
                        }
                        Ok(Msg::Flush(ack)) => {
                            let r = commit(&root, &workdir, &pending).map_err(|e| e.to_string());
                            pending.clear();
                            deadline = None;
                            let _ = ack.send(r);
                        }
                        Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                            if !pending.is_empty() {
                                if let Err(e) = commit(&root, &workdir, &pending) {
                                    log::warn!("git commit on shutdown: {e}");
                                }
                            }
                            return;
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if let Err(e) = commit(&root, &workdir, &pending) {
                                log::warn!("git auto-commit: {e}");
                            }
                            pending.clear();
                            deadline = None;
                        }
                    }
                }
            })
            .ok()?;
        Some(GitSync { tx, handle: parking_lot::Mutex::new(Some(handle)) })
    }

    pub fn changed(&self, rel: &str) {
        let _ = self.tx.send(Msg::Changed(rel.to_owned()));
    }

    /// Commit anything pending now; returns the new commit id, if any.
    pub fn flush(&self) -> Result<Option<String>> {
        let (ack, done) = mpsc::channel();
        self.tx.send(Msg::Flush(ack)).map_err(|_| NexusError::Other("git worker stopped".into()))?;
        done.recv().map_err(|_| NexusError::Other("git worker stopped".into()))?.map_err(NexusError::Other)
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
        if let Some(h) = self.handle.lock().take() {
            let _ = h.join();
        }
    }
}

impl Drop for GitSync {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn commit(root: &Path, workdir: &Path, paths: &BTreeSet<String>) -> Result<Option<String>> {
    if paths.is_empty() {
        return Ok(None);
    }
    let repo = Repository::open(root).map_err(git_err)?;
    let mut index = repo.index().map_err(git_err)?;
    let mut touched = Vec::new();
    for rel in paths {
        let abs: PathBuf = crate::fs::resolve(root, rel)?;
        let Ok(in_repo) = abs.strip_prefix(workdir) else { continue };
        if repo.is_path_ignored(in_repo).unwrap_or(false) {
            continue;
        }
        if abs.is_file() {
            index.add_path(in_repo).map_err(git_err)?;
        } else if abs.is_dir() {
            index.add_all([in_repo], IndexAddOption::DEFAULT, None).map_err(git_err)?;
        } else {
            // Deleted: drop the file (or everything below a deleted dir).
            let _ = index.remove_path(in_repo);
            let _ = index.remove_dir(in_repo, 0);
        }
        touched.push(rel.as_str());
    }
    if touched.is_empty() {
        return Ok(None);
    }
    index.write().map_err(git_err)?;
    let tree_id = index.write_tree().map_err(git_err)?;
    let tree = repo.find_tree(tree_id).map_err(git_err)?;
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    if let Some(p) = &parent {
        if p.tree_id() == tree_id {
            return Ok(None); // nothing actually changed
        }
    }
    let sig = repo
        .signature()
        .or_else(|_| Signature::now("Nexus", "nexus@localhost"))
        .map_err(git_err)?;
    let msg = if touched.len() == 1 {
        format!("nexus: update {}", touched[0])
    } else {
        format!("nexus: update {} files\n\n{}", touched.len(), touched.iter().map(|p| format!("- {p}")).collect::<Vec<_>>().join("\n"))
    };
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    let oid = repo.commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parents).map_err(git_err)?;
    Ok(Some(oid.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_changes_and_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        init(root).unwrap();
        std::fs::write(root.join(".gitignore"), ".nexus/\n").unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::create_dir_all(root.join(".nexus")).unwrap();
        std::fs::write(root.join("notes/a.md"), "a").unwrap();
        std::fs::write(root.join(".nexus/index.db"), "x").unwrap();
        let g = GitSync::open(root).unwrap();
        g.changed("notes/a.md");
        g.changed(".nexus/index.db");
        let first = g.flush().unwrap();
        assert!(first.is_some());

        let repo = Repository::open(root).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "nexus: update notes/a.md");
        assert!(head.tree().unwrap().get_path(Path::new(".nexus/index.db")).is_err(), "ignored files never committed");

        // No-op save produces no commit.
        g.changed("notes/a.md");
        assert!(g.flush().unwrap().is_none());

        std::fs::remove_file(root.join("notes/a.md")).unwrap();
        g.changed("notes/a.md");
        assert!(g.flush().unwrap().is_some());
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert!(head.tree().unwrap().get_path(Path::new("notes/a.md")).is_err());
    }
}
