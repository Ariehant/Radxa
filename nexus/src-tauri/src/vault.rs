//! The open vault: canonical Markdown + JSON Canvas files on disk.

use crate::error::{NexusError, Result};
use crate::fs as vfs;
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

pub struct Vault {
    pub root: PathBuf,
    pub events: Arc<dyn EventSink>,
    pub index: Index,
}

impl Vault {
    pub fn open(root: &Path, events: Arc<dyn EventSink>) -> Result<Arc<Vault>> {
        if !root.is_dir() {
            return Err(NexusError::NotFound(root.display().to_string()));
        }
        let root = dunce(root.canonicalize()?);
        std::fs::create_dir_all(root.join(".nexus"))?;
        let index = Index::open(&root, events.clone(), kind_indexers(), true)?;
        Ok(Arc::new(Vault { root, events, index }))
    }

    /// Create the standard layout (idempotent) and open it.
    pub fn create(root: &Path, events: Arc<dyn EventSink>) -> Result<Arc<Vault>> {
        std::fs::create_dir_all(root)?;
        for d in VAULT_DIRS {
            std::fs::create_dir_all(root.join(d))?;
        }
        crate::scaffold::write_defaults(root)?;
        Vault::open(root, events)
    }

    pub fn info(&self) -> VaultInfo {
        VaultInfo {
            root: self.root.to_string_lossy().replace('\\', "/"),
            name: self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        }
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

    pub fn shutdown(&self) {
        self.index.shutdown();
    }
}

/// Kind-specific indexing hooks (flows, canvases, ...).
fn kind_indexers() -> Vec<(crate::index::FileKind, crate::index::worker::KindIndexer)> {
    Vec::new()
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
