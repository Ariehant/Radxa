//! Derived, disposable index over the vault (spec §3 rule 1: delete
//! `.nexus/index.db`, rebuild, all data returns).

pub mod bloom;
pub mod cache;
pub mod parser;
pub mod query;
pub mod schema;
pub mod watcher;
pub mod worker;

use crate::error::{NexusError, Result};
use crate::fs as vfs;
use crate::state::EventSink;
use cache::{HotCache, HotEntry, HOT_CAPACITY};
use parking_lot::{Mutex, RwLock};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::Duration;
use worker::{Job, KindIndexer, Shared, Worker};

pub use parser::FileKind;

/// Indexing priority, highest first (spec §9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Open,
    Viewport,
    Recent,
    Idle,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const READERS: usize = 4;

/// Small pool of read-only connections (spec §9.1: separate read/write).
pub struct ReadPool {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
}

pub struct ReadConn<'a> {
    pool: &'a ReadPool,
    conn: Option<Connection>,
}

impl std::ops::Deref for ReadConn<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl Drop for ReadConn<'_> {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            let mut idle = self.pool.idle.lock();
            if idle.len() < READERS {
                idle.push(c);
            }
        }
    }
}

impl ReadPool {
    fn open_conn(&self) -> rusqlite::Result<Connection> {
        let c = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
        )?;
        c.execute_batch("PRAGMA mmap_size = 268435456; PRAGMA busy_timeout = 5000; PRAGMA query_only = ON;")?;
        Ok(c)
    }

    pub fn get(&self) -> Result<ReadConn<'_>> {
        let c = match self.idle.lock().pop() {
            Some(c) => c,
            None => self.open_conn().map_err(sql)?,
        };
        Ok(ReadConn { pool: self, conn: Some(c) })
    }
}

pub fn sql(e: rusqlite::Error) -> NexusError {
    NexusError::Sqlite(e)
}

pub struct Index {
    root: PathBuf,
    jobs: Sender<Job>,
    reads: ReadPool,
    shared: Arc<Shared>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    watcher: Mutex<Option<watcher::Watcher>>,
}

#[derive(Debug, serde::Serialize)]
pub struct IndexStatus {
    pub pending: usize,
    pub scanning: bool,
    pub hot: usize,
    pub stats: query::Stats,
}

impl Index {
    pub fn db_path(root: &Path) -> PathBuf {
        root.join(".nexus").join("index.db")
    }

    /// Open (creating/migrating) the index, start the worker + watcher, and
    /// kick off a background scan. Returns immediately: first paint never
    /// waits for indexing.
    pub fn open(
        root: &Path,
        events: Arc<dyn EventSink>,
        kind_indexers: Vec<(FileKind, KindIndexer)>,
        watch: bool,
    ) -> Result<Index> {
        let db = Self::db_path(root);
        std::fs::create_dir_all(db.parent().expect("db has parent"))?;
        let mut conn = match Connection::open(&db) {
            Ok(c) => c,
            Err(_) => {
                // Corrupt index: it's derived, so just throw it away.
                let _ = std::fs::remove_file(&db);
                Connection::open(&db).map_err(sql)?
            }
        };
        if schema::migrate(&mut conn).is_err() {
            drop(conn);
            for suffix in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
            }
            conn = Connection::open(&db).map_err(sql)?;
            schema::migrate(&mut conn).map_err(sql)?;
        }

        let shared = Arc::new(Shared {
            bloom: RwLock::new(bloom::Bloom::with_capacity(16 * 1024)),
            hot: Mutex::new(HotCache::new(HOT_CAPACITY)),
            pending: AtomicUsize::new(0),
            scanning: AtomicBool::new(true),
        });
        let (tx, rx) = mpsc::channel();
        let w = Worker::new(root.to_path_buf(), conn, rx, events, shared.clone(), kind_indexers);
        let handle = std::thread::Builder::new().name("nexus-index".into()).spawn(move || w.run())?;
        tx.send(Job::Scan).map_err(|_| NexusError::Other("index worker gone".into()))?;

        let watcher = if watch {
            match watcher::start(root, tx.clone()) {
                Ok(w) => Some(w),
                Err(e) => {
                    log::warn!("file watcher unavailable: {e}");
                    None
                }
            }
        } else {
            None
        };

        Ok(Index {
            root: root.to_path_buf(),
            jobs: tx,
            reads: ReadPool { path: db, idle: Mutex::new(Vec::new()) },
            shared,
            worker: Mutex::new(Some(handle)),
            watcher: Mutex::new(watcher),
        })
    }

    fn send(&self, job: Job) -> Result<()> {
        self.jobs.send(job).map_err(|_| NexusError::Other("index worker stopped".into()))
    }

    pub fn touch(&self, rel: &str, prio: Priority) {
        let _ = self.send(Job::Touch { rel: rel.to_owned(), prio });
    }

    /// Index `rel` ahead of everything else and wait for it (save path).
    pub fn index_now(&self, rel: &str) -> Result<()> {
        let (ack, done) = mpsc::channel();
        self.send(Job::Now { rel: rel.to_owned(), ack })?;
        done.recv_timeout(Duration::from_secs(10)).map_err(|_| NexusError::Other("index timeout".into()))
    }

    /// Wait until the queue is drained.
    pub fn flush(&self) -> Result<()> {
        let (ack, done) = mpsc::channel();
        self.send(Job::Barrier(ack))?;
        done.recv().map_err(|_| NexusError::Other("index worker stopped".into()))
    }

    pub fn flush_fts(&self) -> Result<()> {
        let (ack, done) = mpsc::channel();
        self.send(Job::FlushFts(ack))?;
        done.recv_timeout(Duration::from_secs(30)).map_err(|_| NexusError::Other("fts flush timeout".into()))
    }

    pub fn rescan(&self) -> Result<()> {
        self.send(Job::Scan)
    }

    pub fn rebuild(&self) -> Result<()> {
        self.shared.scanning.store(true, Ordering::Relaxed);
        self.send(Job::Rebuild)
    }

    pub fn read(&self) -> Result<ReadConn<'_>> {
        self.reads.get()
    }

    /// Definitive "no" without touching SQLite; "maybe" is confirmed.
    pub fn may_exist(&self, key: &str) -> bool {
        self.shared.bloom.read().may_contain(key)
    }

    pub fn exists(&self, target: &str) -> Result<bool> {
        let key = parser::link_key(target);
        if !self.shared.scanning.load(Ordering::Relaxed) && !self.may_exist(&key) && !self.may_exist(target.trim()) {
            return Ok(false);
        }
        let c = self.read()?;
        query::exists(&c, target).map_err(sql)
    }

    /// Read a file through the hot tier.
    pub fn read_text(&self, rel: &str) -> Result<Arc<String>> {
        let abs = vfs::resolve(&self.root, rel)?;
        let meta = std::fs::metadata(&abs).map_err(|_| NexusError::NotFound(rel.to_owned()))?;
        if !meta.is_file() {
            return Err(NexusError::NotFound(rel.to_owned()));
        }
        let mtime = worker::mtime_ms(&meta);
        if let Some(c) = self.shared.hot.lock().get(rel, mtime, meta.len()) {
            return Ok(c);
        }
        let content = Arc::new(vfs::read_text(&abs)?);
        self.shared.hot.lock().put(rel, HotEntry { content: content.clone(), mtime, len: meta.len() });
        Ok(content)
    }

    pub fn status(&self) -> Result<IndexStatus> {
        Ok(IndexStatus {
            pending: self.shared.pending.load(Ordering::Relaxed),
            scanning: self.shared.scanning.load(Ordering::Relaxed),
            hot: self.shared.hot.lock().len(),
            stats: {
                let c = self.read()?;
                query::stats(&c).map_err(sql)?
            },
        })
    }

    pub fn shutdown(&self) {
        self.watcher.lock().take();
        let _ = self.jobs.send(Job::Shutdown);
        if let Some(h) = self.worker.lock().take() {
            let _ = h.join();
        }
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
