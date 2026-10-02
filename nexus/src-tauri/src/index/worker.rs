//! The indexing worker: a dedicated thread that owns the only write
//! connection. Jobs are prioritised (open → viewport → recent → idle),
//! deduplicated, and applied in batched transactions.

use super::bloom::Bloom;
use super::cache::HotCache;
use super::parser::{self, FileKind, Parsed};
use super::{now_ms, Priority};
use crate::error::Result;
use crate::fs as vfs;
use crate::state::EventSink;
use parking_lot::{Mutex, RwLock};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde_json::json;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const BATCH: usize = 512;
const IDLE_FTS_AFTER: Duration = Duration::from_millis(1500);
const FTS_BATCH: usize = 500;

pub enum Job {
    /// (Re)index or remove `rel` depending on what is on disk now.
    Touch { rel: String, prio: Priority },
    /// Index immediately, ahead of the queue, then ack (used on save).
    Now { rel: String, ack: Sender<()> },
    /// Walk the vault, enqueue changed files, drop vanished ones.
    Scan,
    /// Wipe the index and rescan: SQLite is never truth.
    Rebuild,
    /// Ack once the queue is empty.
    Barrier(Sender<()>),
    /// Push all pending bodies into FTS, then ack (called before search).
    FlushFts(Sender<()>),
    Shutdown,
}

/// Hook for kind-specific indexing (flows, canvases, databases...).
pub type KindIndexer = fn(&Transaction, &IndexCtx) -> rusqlite::Result<()>;

pub struct IndexCtx<'a> {
    pub file_id: i64,
    pub rel: &'a str,
    pub node_id: &'a str,
    pub src: &'a str,
    pub parsed: &'a Parsed,
}

pub struct Shared {
    pub bloom: RwLock<Bloom>,
    pub hot: Mutex<HotCache>,
    pub pending: std::sync::atomic::AtomicUsize,
    pub scanning: std::sync::atomic::AtomicBool,
}

pub struct Worker {
    root: PathBuf,
    conn: Connection,
    rx: Receiver<Job>,
    events: Arc<dyn EventSink>,
    shared: Arc<Shared>,
    kind_indexers: Vec<(FileKind, KindIndexer)>,
    heap: BinaryHeap<Reverse<(Priority, u64, String)>>,
    queued: HashMap<String, Priority>,
    seq: u64,
    fts_pending: HashSet<String>,
    rebuild_bloom_when_idle: bool,
}

#[derive(Default)]
struct BatchResult {
    changed: Vec<String>,
    removed: Vec<String>,
    added: bool,
}

impl Worker {
    pub fn new(
        root: PathBuf,
        conn: Connection,
        rx: Receiver<Job>,
        events: Arc<dyn EventSink>,
        shared: Arc<Shared>,
        kind_indexers: Vec<(FileKind, KindIndexer)>,
    ) -> Self {
        Worker {
            root,
            conn,
            rx,
            events,
            shared,
            kind_indexers,
            heap: BinaryHeap::new(),
            queued: HashMap::new(),
            seq: 0,
            fts_pending: HashSet::new(),
            rebuild_bloom_when_idle: false,
        }
    }

    pub fn run(mut self) {
        loop {
            let first = if !self.heap.is_empty() {
                None
            } else if !self.fts_pending.is_empty() || self.rebuild_bloom_when_idle {
                match self.rx.recv_timeout(IDLE_FTS_AFTER) {
                    Ok(j) => Some(j),
                    Err(RecvTimeoutError::Timeout) => {
                        self.idle_work();
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            } else {
                match self.rx.recv() {
                    Ok(j) => Some(j),
                    Err(_) => return,
                }
            };
            if let Some(j) = first {
                if !self.handle(j) {
                    return;
                }
            }
            while let Ok(j) = self.rx.try_recv() {
                if !self.handle(j) {
                    return;
                }
            }
            if !self.heap.is_empty() {
                self.process_batch(BATCH);
            }
        }
    }

    /// Returns false on shutdown.
    fn handle(&mut self, job: Job) -> bool {
        match job {
            Job::Touch { rel, prio } => self.push(rel, prio),
            Job::Now { rel, ack } => {
                let rel = vfs::normalize_rel(&rel);
                self.queued.remove(&rel.to_lowercase());
                self.apply(&[rel]);
                let _ = ack.send(());
            }
            Job::Scan => self.scan(),
            Job::Rebuild => {
                if let Err(e) = self.wipe() {
                    log::error!("rebuild wipe failed: {e}");
                }
                self.scan();
            }
            Job::Barrier(ack) => {
                while !self.heap.is_empty() {
                    self.process_batch(BATCH);
                }
                let _ = ack.send(());
            }
            Job::FlushFts(ack) => {
                while !self.fts_pending.is_empty() {
                    self.flush_fts(usize::MAX);
                }
                let _ = ack.send(());
            }
            Job::Shutdown => return false,
        }
        true
    }

    fn push(&mut self, rel: String, prio: Priority) {
        let rel = vfs::normalize_rel(&rel);
        if rel.is_empty() || vfs::is_ignored_rel(&rel) || rel.ends_with(".tmp") {
            return;
        }
        let key = rel.to_lowercase();
        if let Some(existing) = self.queued.get(&key) {
            if *existing <= prio {
                return;
            }
        }
        self.queued.insert(key, prio);
        self.seq += 1;
        self.heap.push(Reverse((prio, self.seq, rel)));
        self.shared.pending.store(self.queued.len(), std::sync::atomic::Ordering::Relaxed);
    }

    fn process_batch(&mut self, max: usize) {
        let mut rels = Vec::with_capacity(max.min(self.heap.len()));
        while rels.len() < max {
            let Some(Reverse((prio, _, rel))) = self.heap.pop() else { break };
            // Skip stale heap entries superseded by a higher-priority push.
            if self.queued.get(&rel.to_lowercase()) == Some(&prio) {
                self.queued.remove(&rel.to_lowercase());
                rels.push(rel);
            }
        }
        self.shared.pending.store(self.queued.len(), std::sync::atomic::Ordering::Relaxed);
        if !rels.is_empty() {
            self.apply(&rels);
        }
        if self.heap.is_empty() && self.shared.scanning.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.rebuild_bloom_when_idle = true;
            self.events.emit("index:ready", json!({}));
        }
    }

    fn apply(&mut self, rels: &[String]) {
        let started = Instant::now();
        let mut res = BatchResult::default();
        let tx = match self.conn.transaction() {
            Ok(t) => t,
            Err(e) => {
                log::error!("index tx: {e}");
                return;
            }
        };
        let mut expand_dirs = Vec::new();
        let mut fts_new = Vec::new();
        for rel in rels {
            match index_path(&tx, &self.root, rel, &self.kind_indexers, &self.shared) {
                Ok(Outcome::Unchanged) => {}
                Ok(Outcome::Updated { added }) => {
                    res.added |= added;
                    fts_new.push(rel.clone());
                    res.changed.push(rel.clone());
                }
                Ok(Outcome::Removed(paths)) => {
                    for p in paths {
                        self.fts_pending.remove(&p);
                        res.removed.push(p);
                    }
                }
                Ok(Outcome::Directory) => expand_dirs.push(rel.clone()),
                Err(e) => log::warn!("index {rel}: {e}"),
            }
        }
        if let Err(e) = tx.commit() {
            log::error!("index commit: {e}");
            return;
        }
        self.fts_pending.extend(fts_new);
        for d in expand_dirs {
            self.enqueue_dir(&d);
        }
        if !res.changed.is_empty() || !res.removed.is_empty() {
            self.events.emit(
                "index:updated",
                json!({
                    "changed": res.changed,
                    "removed": res.removed,
                    "structural": res.added || !res.removed.is_empty(),
                    "pending": self.queued.len(),
                    "ms": started.elapsed().as_millis() as u64,
                }),
            );
        }
    }

    fn enqueue_dir(&mut self, rel_dir: &str) {
        let Ok(abs) = vfs::resolve(&self.root, rel_dir) else { return };
        for entry in walkdir::WalkDir::new(abs).min_depth(1).into_iter().filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.')).flatten() {
            if entry.file_type().is_file() {
                if let Some(rel) = vfs::to_rel(&self.root, entry.path()) {
                    self.push(rel, Priority::Recent);
                }
            }
        }
    }

    fn scan(&mut self) {
        self.shared.scanning.store(true, std::sync::atomic::Ordering::Relaxed);
        let known: HashMap<String, i64> = {
            let mut stmt = match self.conn.prepare("SELECT path, mtime FROM files") {
                Ok(s) => s,
                Err(e) => {
                    log::error!("scan: {e}");
                    return;
                }
            };
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?.to_lowercase(), r.get::<_, i64>(1)?)));
            rows.map(|it| it.flatten().collect()).unwrap_or_default()
        };
        let mut seen = HashSet::with_capacity(known.len());
        let mut candidates: Vec<(i64, String)> = Vec::new();
        let walker = walkdir::WalkDir::new(&self.root)
            .min_depth(1)
            .into_iter()
            .filter_entry(|e| {
                let n = e.file_name().to_string_lossy();
                !n.starts_with('.') && n != "node_modules"
            });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let Some(rel) = vfs::to_rel(&self.root, entry.path()) else { continue };
            if rel.ends_with(".tmp") {
                continue;
            }
            let mtime = entry.metadata().ok().map(|m| mtime_ms(&m)).unwrap_or(0);
            let key = rel.to_lowercase();
            if known.get(&key) != Some(&mtime) {
                candidates.push((mtime, rel));
            }
            seen.insert(key);
        }
        // The most recently modified files are indexed first.
        candidates.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        for (i, (_, rel)) in candidates.into_iter().enumerate() {
            let prio = if i < super::cache::HOT_CAPACITY { Priority::Recent } else { Priority::Idle };
            self.push(rel, prio);
        }
        for path in known.keys() {
            if !seen.contains(path) {
                self.push(path.clone(), Priority::Idle);
            }
        }
        if self.heap.is_empty() {
            self.shared.scanning.store(false, std::sync::atomic::Ordering::Relaxed);
            self.rebuild_bloom_when_idle = true;
            self.events.emit("index:ready", json!({}));
        }
    }

    fn wipe(&mut self) -> rusqlite::Result<()> {
        self.fts_pending.clear();
        self.shared.hot.lock().clear_all();
        self.conn.execute_batch(
            "BEGIN; DELETE FROM aliases; DELETE FROM edges; DELETE FROM nodes; DELETE FROM files;
             DELETE FROM fts; DELETE FROM canvas_layout; DELETE FROM rollup_cache; COMMIT;",
        )
    }

    fn idle_work(&mut self) {
        if !self.fts_pending.is_empty() {
            self.flush_fts(FTS_BATCH);
        } else if self.rebuild_bloom_when_idle {
            self.rebuild_bloom_when_idle = false;
            if let Err(e) = self.rebuild_bloom() {
                log::warn!("bloom rebuild: {e}");
            }
        }
    }

    /// Frontmatter-only parsing during indexing; bodies land in FTS lazily,
    /// when idle or right before a search.
    fn flush_fts(&mut self, max: usize) {
        let batch: Vec<String> = self.fts_pending.iter().take(max).cloned().collect();
        for p in &batch {
            self.fts_pending.remove(p);
        }
        let Ok(tx) = self.conn.transaction() else { return };
        for rel in &batch {
            // fts.rowid == files.id, so replacement is an O(log n) rowid op.
            let Ok(Some((id, path))) = tx
                .query_row("SELECT id, path FROM files WHERE path = ?1", params![rel], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
                .optional()
            else {
                continue;
            };
            let _ = tx.execute("DELETE FROM fts WHERE rowid = ?1", params![id]);
            if matches!(FileKind::from_path(rel), FileKind::Attachment | FileKind::Canvas) {
                continue;
            }
            let Ok(abs) = vfs::resolve(&self.root, rel) else { continue };
            let Ok(src) = vfs::read_text(&abs) else { continue };
            let (_, body) = parser::split_frontmatter(&src);
            let _ = tx.execute("INSERT INTO fts(rowid, path, body) VALUES (?1, ?2, ?3)", params![id, path, body]);
        }
        if let Err(e) = tx.commit() {
            log::warn!("fts commit: {e}");
        }
    }

    fn rebuild_bloom(&mut self) -> rusqlite::Result<()> {
        let n: i64 = self.conn.query_row("SELECT (SELECT count(*) FROM files) + (SELECT count(*) FROM aliases)", [], |r| r.get(0))?;
        let mut b = Bloom::with_capacity((n as usize) * 2);
        let mut stmt = self.conn.prepare("SELECT path FROM files UNION ALL SELECT key FROM aliases")?;
        let mut rows = stmt.query([])?;
        while let Some(r) = rows.next()? {
            b.insert(&r.get::<_, String>(0)?);
        }
        *self.shared.bloom.write() = b;
        Ok(())
    }
}

enum Outcome {
    Unchanged,
    Updated { added: bool },
    Removed(Vec<String>),
    Directory,
}

pub fn mtime_ms(m: &std::fs::Metadata) -> i64 {
    m.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Path-derived node id for notes without a frontmatter `id`.
pub fn path_node_id(rel: &str) -> String {
    format!("path:{}", rel.to_lowercase())
}

fn index_path(
    tx: &Transaction,
    root: &Path,
    rel: &str,
    kind_indexers: &[(FileKind, KindIndexer)],
    shared: &Shared,
) -> Result<Outcome> {
    let abs = vfs::resolve(root, rel)?;
    let meta = match std::fs::metadata(&abs) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Outcome::Removed(remove_path(tx, rel, shared)?));
        }
        Err(e) => return Err(e.into()),
    };
    if meta.is_dir() {
        return Ok(Outcome::Directory);
    }
    let mtime = mtime_ms(&meta);
    let existing: Option<(i64, Vec<u8>, i64)> = tx
        .query_row("SELECT id, hash, mtime FROM files WHERE path = ?1", params![rel], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    if let Some((_, _, old_mtime)) = &existing {
        if *old_mtime == mtime {
            return Ok(Outcome::Unchanged);
        }
    }

    let kind = FileKind::from_path(rel);
    let (hash, src) = if kind == FileKind::Attachment {
        // Don't read binaries: size + mtime is a sufficient change signature.
        let mut h = Vec::with_capacity(16);
        h.extend_from_slice(&meta.len().to_le_bytes());
        h.extend_from_slice(&mtime.to_le_bytes());
        (h, None)
    } else {
        let bytes = std::fs::read(&abs)?;
        let h = xxhash_rust::xxh3::xxh3_64(&bytes).to_le_bytes().to_vec();
        (h, Some(vfs::normalize_newlines(&String::from_utf8_lossy(&bytes))))
    };

    if let Some((id, old_hash, _)) = &existing {
        if *old_hash == hash {
            tx.execute("UPDATE files SET mtime = ?1 WHERE id = ?2", params![mtime, id])?;
            return Ok(Outcome::Unchanged);
        }
    }

    shared.hot.lock().invalidate(rel);
    let now = now_ms();
    let file_id: i64 = match &existing {
        Some((id, _, _)) => {
            tx.execute(
                "UPDATE files SET path = ?1, hash = ?2, mtime = ?3, kind = ?4, indexed_at = ?5 WHERE id = ?6",
                params![rel, hash, mtime, kind.as_str(), now, id],
            )?;
            clear_file_nodes(tx, *id)?;
            *id
        }
        None => {
            tx.execute(
                "INSERT INTO files(path, hash, mtime, kind, indexed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![rel, hash, mtime, kind.as_str(), now],
            )?;
            tx.last_insert_rowid()
        }
    };

    let mut bloom = shared.bloom.write();
    bloom.insert(rel);

    let Some(src) = src else {
        return Ok(Outcome::Updated { added: existing.is_none() });
    };
    let parsed = parser::parse(rel, &src);

    let mut node_id = parsed.id.clone().unwrap_or_else(|| path_node_id(rel));
    let taken: Option<i64> = tx.query_row("SELECT file_id FROM nodes WHERE id = ?1", params![node_id], |r| r.get(0)).optional()?;
    if taken.is_some_and(|f| f != file_id) {
        log::warn!("duplicate id {node_id} in {rel}; using path id");
        node_id = path_node_id(rel);
    }

    let node_kind = match parsed.kind {
        FileKind::Note => parsed.frontmatter.get("type").and_then(|v| v.as_str()).unwrap_or("note").to_owned(),
        k => k.as_str().to_owned(),
    };
    let data = json!({
        "path": rel,
        "title": parsed.title,
        "file_kind": parsed.kind.as_str(),
        "frontmatter": parsed.frontmatter,
        "error": parsed.frontmatter_error,
    });
    tx.execute(
        "INSERT INTO nodes(id, file_id, kind, data) VALUES (?1, ?2, ?3, ?4)",
        params![node_id, file_id, node_kind, data.to_string()],
    )?;

    {
        let mut ins = tx.prepare_cached("INSERT OR IGNORE INTO aliases(key, node_id) VALUES (?1, ?2)")?;
        for key in parser::alias_keys(rel, &parsed.title, &parsed.frontmatter) {
            bloom.insert(&key);
            ins.execute(params![key, node_id])?;
        }
    }
    drop(bloom);
    {
        let mut ins = tx.prepare_cached(
            "INSERT OR IGNORE INTO edges(from_id, to_id, kind, port, data_type) VALUES (?1, ?2, ?3, ?4, NULL)",
        )?;
        for l in &parsed.links {
            ins.execute(params![node_id, parser::link_key(&l.target), if l.embed { "embed" } else { "link" }, ""])?;
        }
        for (prop, target) in &parsed.relations {
            ins.execute(params![node_id, parser::link_key(target), "relation", prop])?;
        }
    }

    let ctx = IndexCtx { file_id, rel, node_id: &node_id, src: &src, parsed: &parsed };
    for (k, f) in kind_indexers {
        if *k == parsed.kind {
            f(tx, &ctx)?;
        }
    }
    Ok(Outcome::Updated { added: existing.is_none() })
}

/// Edges owned by a node: `from_id = id` or `from_id` starting with `id/`
/// (sub-nodes such as flow steps). Range query keeps the index usable.
fn delete_owned_edges(tx: &Transaction, node_id: &str) -> rusqlite::Result<()> {
    let lo = format!("{node_id}/");
    let hi = format!("{node_id}0"); // '0' is the byte after '/'
    tx.execute("DELETE FROM edges WHERE from_id = ?1 OR (from_id >= ?2 AND from_id < ?3)", params![node_id, lo, hi])?;
    tx.execute("DELETE FROM canvas_layout WHERE flow_id = ?1", params![node_id])?;
    tx.execute("DELETE FROM rollup_cache WHERE node_id = ?1", params![node_id])?;
    Ok(())
}

fn clear_file_nodes(tx: &Transaction, file_id: i64) -> rusqlite::Result<()> {
    let ids: Vec<String> = {
        let mut stmt = tx.prepare_cached("SELECT id FROM nodes WHERE file_id = ?1")?;
        let rows = stmt.query_map(params![file_id], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for id in &ids {
        delete_owned_edges(tx, id)?;
    }
    tx.execute("DELETE FROM nodes WHERE file_id = ?1", params![file_id])?;
    Ok(())
}

/// Remove a file, or every file below a vanished directory.
fn remove_path(tx: &Transaction, rel: &str, shared: &Shared) -> rusqlite::Result<Vec<String>> {
    let prefix = format!("{}/", rel.to_lowercase());
    let rows: Vec<(i64, String)> = {
        let mut stmt = tx.prepare_cached("SELECT id, path FROM files WHERE path = ?1 OR lower(substr(path, 1, ?3)) = ?2")?;
        let rows = stmt.query_map(params![rel, prefix, prefix.len() as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut hot = shared.hot.lock();
    hot.invalidate(rel);
    hot.invalidate_prefix(rel);
    drop(hot);
    let mut removed = Vec::with_capacity(rows.len());
    for (id, path) in rows {
        clear_file_nodes(tx, id)?;
        tx.execute("DELETE FROM files WHERE id = ?1", params![id])?;
        tx.execute("DELETE FROM fts WHERE rowid = ?1", params![id])?;
        removed.push(path);
    }
    Ok(removed)
}
