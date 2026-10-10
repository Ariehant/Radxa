//! Atomic writes: `<file>.tmp` → fsync → rename (spec §8 rule 2).

use crate::error::Result;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

fn tmp_path(target: &Path) -> PathBuf {
    let mut name = target.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".tmp");
    target.with_file_name(name)
}

/// Atomically replace `target` with `contents`. Parent directories are created.
pub fn atomic_write(target: &Path, contents: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = tmp_path(target);
    {
        let mut f = OpenOptions::new().write(true).create(true).truncate(true).open(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    // Windows: rename onto an existing (possibly briefly locked) file can fail.
    // Delete the target and retry a few times before giving up.
    let mut result = fs::rename(&tmp, target);
    let mut attempt = 0u64;
    while result.is_err() && attempt < 5 {
        attempt += 1;
        let _ = fs::remove_file(target);
        std::thread::sleep(std::time::Duration::from_millis(10 * attempt));
        result = fs::rename(&tmp, target);
    }
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    sync_parent(target);
    Ok(())
}

/// Best-effort directory fsync so the rename itself is durable (no-op on Windows).
fn sync_parent(target: &Path) {
    #[cfg(unix)]
    if let Some(parent) = target.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = target;
}

pub fn remove_file(target: &Path) -> Result<()> {
    match fs::remove_file(target) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces() {
        let dir = std::env::temp_dir().join(format!("nexus-atomic-{}", std::process::id()));
        let target = dir.join("sub").join("a.md");
        atomic_write(&target, b"one").unwrap();
        atomic_write(&target, b"two").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "two");
        assert!(!tmp_path(&target).exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
