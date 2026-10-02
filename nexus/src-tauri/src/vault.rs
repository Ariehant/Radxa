//! The open vault: canonical Markdown + JSON Canvas files on disk.

use crate::error::{NexusError, Result};
use crate::fs as vfs;
use crate::config::Config;
use crate::git::GitSync;
use crate::index::Index;
use crate::state::EventSink;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const VAULT_DIRS: &[&str] = &[
    "notes",
    "databases",
    "views",
    "flows",
    "templates/nodes",
    "runs",
    "attachments",
    ".nexus",
];

#[derive(Debug, Clone, Serialize)]
pub struct TreeEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct VaultInfo {
    pub root: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Saved {
    pub path: String,
    pub hash: String,
}

pub struct Vault {
    pub root: PathBuf,
    pub events: Arc<dyn EventSink>,
    pub index: Index,
    pub git: Option<GitSync>,
    pub engine: crate::flow::engine::Engine,
    config: parking_lot::RwLock<Config>,
}

pub fn content_hash(s: &str) -> String {
    format!("{:016x}", xxhash_rust::xxh3::xxh3_64(s.as_bytes()))
}

/// `"Fix Auth Bug!"` → `"fix-auth-bug"`.
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut dash = false;
    for c in title.trim().chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}

impl Vault {
    pub fn open(root: &Path, events: Arc<dyn EventSink>) -> Result<Arc<Vault>> {
        if !root.is_dir() {
            return Err(NexusError::NotFound(root.display().to_string()));
        }
        let root = dunce(root.canonicalize()?);
        std::fs::create_dir_all(root.join(".nexus"))?;
        let config = Config::load(&root);
        let index = Index::open(&root, events.clone(), kind_indexers(), true)?;
        let git = if config.git.auto_commit { GitSync::open(&root) } else { None };
        Ok(Arc::new(Vault { root, events, index, git, engine: Default::default(), config: parking_lot::RwLock::new(config) }))
    }

    /// Create the standard layout (idempotent) and open it. Optionally
    /// `git init` so every save is versioned.
    pub fn create(root: &Path, events: Arc<dyn EventSink>, git: bool) -> Result<Arc<Vault>> {
        std::fs::create_dir_all(root)?;
        for d in VAULT_DIRS {
            std::fs::create_dir_all(root.join(d))?;
        }
        crate::scaffold::write_defaults(root)?;
        let fresh_repo = git && !crate::git::is_repo(root);
        if fresh_repo {
            crate::git::init(root)?;
        }
        let v = Vault::open(root, events)?;
        if fresh_repo {
            if let Some(g) = &v.git {
                g.changed("");
                g.flush()?;
            }
        }
        Ok(v)
    }

    pub fn info(&self) -> VaultInfo {
        VaultInfo {
            root: self.root.to_string_lossy().replace('\\', "/"),
            name: self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().clone()
    }

    pub fn set_config(&self, c: Config) -> Result<()> {
        c.save(&self.root)?;
        *self.config.write() = c;
        Ok(())
    }

    pub fn abs(&self, rel: &str) -> Result<PathBuf> {
        vfs::resolve(&self.root, rel)
    }

    pub fn rel(&self, abs: &Path) -> Option<String> {
        vfs::to_rel(&self.root, abs)
    }

    /// Flat, depth-annotated, directories-first listing. The UI builds the
    /// expandable tree and virtualises it.
    pub fn tree(&self) -> Result<Vec<TreeEntry>> {
        let mut out = Vec::new();
        let walker = walkdir::WalkDir::new(&self.root)
            .min_depth(1)
            .sort_by(|a, b| {
                let ad = a.file_type().is_dir();
                let bd = b.file_type().is_dir();
                bd.cmp(&ad).then_with(|| {
                    a.file_name().to_string_lossy().to_lowercase().cmp(&b.file_name().to_string_lossy().to_lowercase())
                })
            })
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'));
        for entry in walker {
            let entry = entry.map_err(|e| NexusError::Other(e.to_string()))?;
            let Some(rel) = self.rel(entry.path()) else { continue };
            if rel.ends_with(".tmp") {
                continue;
            }
            out.push(TreeEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: entry.file_type().is_dir(),
                depth: entry.depth() - 1,
                path: rel,
            });
        }
        Ok(out)
    }

    /// Read through the hot tier of the index (validated by mtime + size).
    pub fn read_text(&self, rel: &str) -> Result<Arc<String>> {
        self.index.read_text(&vfs::normalize_rel(rel))
    }

    /// Atomic save → index (synchronously, < 200 ms) → git (coalesced).
    /// If `base_hash` is given and the file on disk no longer matches it,
    /// the save is refused so an external edit is never silently clobbered.
    pub fn write_text(&self, rel: &str, content: &str, base_hash: Option<&str>) -> Result<Saved> {
        let rel = vfs::normalize_rel(rel);
        if rel.is_empty() || vfs::is_ignored_rel(&rel) {
            return Err(NexusError::invalid(format!("cannot write {rel}")));
        }
        let abs = self.abs(&rel)?;
        if let Some(base) = base_hash {
            if abs.is_file() {
                let current = vfs::read_text(&abs)?;
                if content_hash(&current) != base {
                    return Err(NexusError::Conflict(rel));
                }
            }
        }
        let content = vfs::normalize_newlines(content);
        vfs::atomic_write(&abs, content.as_bytes())?;
        self.after_change(&rel)?;
        Ok(Saved { hash: content_hash(&content), path: rel })
    }

    /// Create a new note from a title, never overwriting: `notes/fix-auth-bug-2.md`.
    pub fn create_note(&self, dir: &str, title: &str, body: &str) -> Result<Saved> {
        let dir = vfs::normalize_rel(dir);
        let base = slugify(title);
        let mut n = 1;
        let rel = loop {
            let name = if n == 1 { format!("{base}.md") } else { format!("{base}-{n}.md") };
            let rel = if dir.is_empty() { name } else { format!("{dir}/{name}") };
            if !self.abs(&rel)?.exists() {
                break rel;
            }
            n += 1;
        };
        let content = if body.starts_with("---\n") {
            body.to_owned()
        } else {
            // A JSON string literal is a valid YAML double-quoted scalar.
            let t = serde_json::to_string(title)?;
            format!("---\ntitle: {t}\ncreated: {}\n---\n\n{body}", crate::time::now_rfc3339())
        };
        self.write_text(&rel, &content, None)
    }

    pub fn delete(&self, rel: &str) -> Result<()> {
        let rel = vfs::normalize_rel(rel);
        let abs = self.abs(&rel)?;
        if abs.is_dir() {
            std::fs::remove_dir_all(&abs)?;
        } else {
            vfs::remove_file(&abs)?;
        }
        self.after_change(&rel)
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<String> {
        let (from, to) = (vfs::normalize_rel(from), vfs::normalize_rel(to));
        let (a, b) = (self.abs(&from)?, self.abs(&to)?);
        if b.exists() && from.to_lowercase() != to.to_lowercase() {
            return Err(NexusError::invalid(format!("{to} already exists")));
        }
        if let Some(p) = b.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::rename(&a, &b)?;
        self.after_change(&from)?;
        self.after_change(&to)?;
        Ok(to)
    }

    fn after_change(&self, rel: &str) -> Result<()> {
        self.index.index_now(rel)?;
        if let Some(g) = &self.git {
            g.changed(rel);
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.index.shutdown();
        if let Some(g) = &self.git {
            g.shutdown();
        }
    }
}

/// Kind-specific indexing hooks (flows, canvases, ...).
fn kind_indexers() -> Vec<(crate::index::FileKind, crate::index::worker::KindIndexer)> {
    use crate::index::FileKind;
    vec![(FileKind::Flow, crate::flow::index_flow), (FileKind::Canvas, crate::flow::index_canvas)]
}

/// Strip the Windows `\\?\` verbatim prefix that `canonicalize` adds, so
/// paths stay usable by libraries and readable in the UI.
fn dunce(p: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            if !rest.starts_with("UNC\\") {
                return PathBuf::from(rest.to_string());
            }
        }
    }
    p
}
