//! Path handling. The frontend only ever sees vault-relative paths with forward
//! slashes; the backend only ever touches `PathBuf`s (spec §8 rule 1).

pub mod atomic;

use crate::error::{NexusError, Result};
use std::path::{Component, Path, PathBuf};

pub use atomic::{atomic_write, remove_file};

/// Resolve a vault-relative path (forward or back slashes) to an absolute path
/// inside `root`. Rejects absolute paths and any `..` escape.
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf> {
    let mut out = root.to_path_buf();
    for part in rel.split(['/', '\\']) {
        match part {
            "" | "." => continue,
            ".." => return Err(NexusError::invalid(format!("path escapes vault: {rel}"))),
            p => {
                let comp = Path::new(p);
                if comp.is_absolute() || comp.components().any(|c| matches!(c, Component::Prefix(_))) {
                    return Err(NexusError::invalid(format!("absolute path not allowed: {rel}")));
                }
                out.push(p);
            }
        }
    }
    Ok(out)
}

/// Absolute path → vault-relative path with forward slashes.
pub fn to_rel(root: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(root).ok()?;
    let parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

/// Normalise a relative path string: forward slashes, no leading `./` or `/`.
pub fn normalize_rel(rel: &str) -> String {
    rel.split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Directories inside a vault that are never shown or indexed.
pub fn is_ignored_rel(rel: &str) -> bool {
    rel.split('/').any(|seg| seg.starts_with('.') || seg == "node_modules")
}

/// Read a text file, normalising line endings to `\n` (spec §8 rule 4).
pub fn read_text(path: &Path) -> Result<String> {
    let raw = std::fs::read_to_string(path)?;
    Ok(normalize_newlines(&raw))
}

pub fn normalize_newlines(s: &str) -> String {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_rejects_escape() {
        let root = PathBuf::from("/vault");
        assert!(resolve(&root, "../etc/passwd").is_err());
        assert!(resolve(&root, "notes/../../x").is_err());
        assert_eq!(resolve(&root, "notes/a.md").unwrap(), root.join("notes").join("a.md"));
        assert_eq!(resolve(&root, "notes\\a.md").unwrap(), root.join("notes").join("a.md"));
    }

    #[test]
    fn rel_roundtrip() {
        let root = PathBuf::from("/vault");
        let abs = root.join("notes").join("a.md");
        assert_eq!(to_rel(&root, &abs).unwrap(), "notes/a.md");
        assert_eq!(normalize_rel("./notes\\x//y.md"), "notes/x/y.md");
    }

    #[test]
    fn ignored() {
        assert!(is_ignored_rel(".nexus/index.db"));
        assert!(is_ignored_rel("notes/.git/x"));
        assert!(!is_ignored_rel("notes/a.md"));
    }
}
