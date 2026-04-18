//! Filesystem utilities used by every live-config writer: atomic writes,
//! symlink-or-copy, and recursive hashing.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{Error, Result};

/// Atomically write `data` to `path`. Writes to a sibling `*.tmp` file, fsyncs
/// it, then renames over the target. On any error the temp file is cleaned up.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }

    let tmp = tmp_sibling(path);
    let mut file = fs::File::create(&tmp).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    file.write_all(data).map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    file.sync_all().map_err(|source| Error::Io {
        path: tmp.clone(),
        source,
    })?;
    drop(file);

    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(Error::Io {
            path: path.to_path_buf(),
            source: e,
        });
    }
    Ok(())
}

/// Like `atomic_write` but also sets mode 0600 on the final file. Used for
/// `secrets.local.json`.
pub fn atomic_write_private(path: &Path, data: &[u8]) -> Result<()> {
    atomic_write(path, data)?;
    let mut perm = fs::metadata(path)
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?
        .permissions();
    perm.set_mode(0o600);
    fs::set_permissions(path, perm).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn tmp_sibling(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".tmp");
    PathBuf::from(s)
}

/// Read a whole file, wrapping errors with the file's path.
pub fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// SHA-256 of the concatenated (relative-path, file-bytes) of every regular
/// file under `root`, in sorted order. Used by skills to detect upstream
/// updates.
pub fn hash_directory(root: &Path) -> Result<String> {
    let mut entries: Vec<PathBuf> = WalkDir::new(root)
        .sort_by_file_name()
        .into_iter()
        .filter_map(|r| r.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();
    entries.sort();

    let mut hasher = Sha256::new();
    for path in entries {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(b"\0");
        let bytes = fs::read(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        hasher.update(&bytes);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(TABLE[(b >> 4) as usize] as char);
        s.push(TABLE[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_then_renames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        atomic_write(&path, b"{\"k\":1}").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"k\":1}");
        // temp sibling should be gone
        let tmp = path.with_extension("json.tmp");
        assert!(!tmp.exists());
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("out.txt");
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
    }

    #[test]
    fn hash_directory_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("sub/b.txt");
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::write(&a, "alpha").unwrap();
        std::fs::write(&b, "beta").unwrap();
        let h1 = hash_directory(dir.path()).unwrap();
        let h2 = hash_directory(dir.path()).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn hash_directory_changes_on_edit() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        std::fs::write(&a, "one").unwrap();
        let h1 = hash_directory(dir.path()).unwrap();
        std::fs::write(&a, "two").unwrap();
        let h2 = hash_directory(dir.path()).unwrap();
        assert_ne!(h1, h2);
    }
}
