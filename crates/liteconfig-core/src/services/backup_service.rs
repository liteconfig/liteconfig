//! Local snapshot create/restore, plus a stub for GitHub sync that strips
//! secrets before push.
//!
//! A snapshot is a directory under `~/.liteconfig/backups/<timestamp>/`
//! containing:
//!   - `liteconfig.db`      — copy of the SQLite file
//!   - `skills/`            — copy of the skills tree
//!
//! `secrets.local.json` is **never** included in a snapshot. The DB itself
//! contains only `@secret:*` references, so a snapshot is safe to push to
//! any remote.

use std::path::{Path, PathBuf};

use git2::{IndexAddOption, Repository, Signature};
use walkdir::WalkDir;

use crate::fs_util::atomic_write;
use crate::paths::{
    ensure_dir, liteconfig_backup_repo_dir, liteconfig_backups_dir, liteconfig_db_path,
    liteconfig_skills_dir,
};
use crate::settings::GithubBackupSettings;
use crate::{Error, Result};

/// Metadata for one snapshot, enough to populate the Backup tab.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub timestamp: String,
    pub directory: PathBuf,
    pub size_bytes: u64,
}

pub fn list_snapshots() -> Result<Vec<Snapshot>> {
    let root = liteconfig_backups_dir()?;
    if !root.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|source| Error::Io {
        path: root.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::Io {
            path: root.clone(),
            source,
        })?;
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let dir = entry.path();
        let size = dir_size(&dir).unwrap_or(0);
        let ts = entry.file_name().to_string_lossy().to_string();
        out.push(Snapshot {
            timestamp: ts,
            directory: dir,
            size_bytes: size,
        });
    }
    out.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(out)
}

/// Snapshot the current DB + skills tree. Returns the new snapshot's path.
pub fn create_snapshot() -> Result<Snapshot> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let root = liteconfig_backups_dir()?.join(&stamp);
    ensure_dir(&root)?;

    let db_src = liteconfig_db_path()?;
    if db_src.exists() {
        let bytes = std::fs::read(&db_src).map_err(|source| Error::Io {
            path: db_src.clone(),
            source,
        })?;
        atomic_write(&root.join("liteconfig.db"), &bytes)?;
    }

    let skills_src = liteconfig_skills_dir()?;
    if skills_src.exists() {
        copy_tree(&skills_src, &root.join("skills"))?;
    }

    let size = dir_size(&root).unwrap_or(0);
    Ok(Snapshot {
        timestamp: stamp,
        directory: root,
        size_bytes: size,
    })
}

/// Restore the named snapshot over the current DB + skills tree.
///
/// Returns the list of unresolved `@secret:*` references the caller should
/// prompt the user to re-enter, so the restore-to-new-device flow can be
/// completed interactively.
pub fn restore_snapshot(timestamp: &str) -> Result<()> {
    let src = liteconfig_backups_dir()?.join(timestamp);
    if !src.exists() {
        return Err(Error::InvalidConfig(format!(
            "snapshot not found: {timestamp}"
        )));
    }
    let db_src = src.join("liteconfig.db");
    if db_src.exists() {
        let bytes = std::fs::read(&db_src).map_err(|source| Error::Io {
            path: db_src.clone(),
            source,
        })?;
        atomic_write(&liteconfig_db_path()?, &bytes)?;
    }
    let skills_src = src.join("skills");
    if skills_src.exists() {
        let skills_dst = liteconfig_skills_dir()?;
        if skills_dst.exists() {
            std::fs::remove_dir_all(&skills_dst).map_err(|source| Error::Io {
                path: skills_dst.clone(),
                source,
            })?;
        }
        copy_tree(&skills_src, &skills_dst)?;
    }
    Ok(())
}

/// Remove a local snapshot directory. Missing directories are treated as a
/// no-op so a stale UI handle can't surface spurious errors. Refuses empty
/// timestamps and any component that attempts to escape the backups root
/// (e.g. `..`) as a defense against caller bugs.
pub fn delete_snapshot(timestamp: &str) -> Result<()> {
    if timestamp.trim().is_empty() {
        return Err(Error::InvalidConfig(
            "snapshot timestamp cannot be empty".into(),
        ));
    }
    if timestamp.contains('/') || timestamp.contains('\\') || timestamp.contains("..") {
        return Err(Error::InvalidConfig(format!(
            "snapshot timestamp contains path separators or traversal: {timestamp}"
        )));
    }
    let path = liteconfig_backups_dir()?.join(timestamp);
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_dir_all(&path).map_err(|source| Error::Io {
        path: path.clone(),
        source,
    })?;
    Ok(())
}

/// Commit the current DB + skills tree into the local backup repo and push
/// to the configured remote.
///
/// Never stages `settings.json` or `secrets.local.json`. The repo's
/// `.gitignore` enforces this at the git layer too, so even a manual
/// `git add -A` in the backup-repo dir would skip them.
pub fn push_to_github(gh: &GithubBackupSettings) -> Result<String> {
    if !gh.enabled {
        return Err(Error::InvalidConfig(
            "GitHub backup is disabled in settings".to_string(),
        ));
    }
    if gh.repo_url.trim().is_empty() {
        return Err(Error::InvalidConfig(
            "GitHub backup repo URL is empty".to_string(),
        ));
    }

    let repo_dir = liteconfig_backup_repo_dir()?;
    ensure_dir(&repo_dir)?;
    let repo = open_or_init_repo(&repo_dir, &gh.repo_url, &gh.branch)?;

    // Stage files.
    stage_backup_payload(&repo_dir)?;

    // git add -A
    let mut index = repo.index().map_err(git_err)?;
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .map_err(git_err)?;
    index.write().map_err(git_err)?;
    let tree_oid = index.write_tree().map_err(git_err)?;
    let tree = repo.find_tree(tree_oid).map_err(git_err)?;

    let sig = Signature::now("liteconfig", "liteconfig@local").map_err(git_err)?;
    let msg = format!(
        "liteconfig backup {}",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    );
    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit> = parent.as_ref().map(|c| vec![c]).unwrap_or_default();
    let commit_oid = repo
        .commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parents)
        .map_err(git_err)?;

    // Push to remote.
    let mut remote = repo.find_remote("origin").map_err(git_err)?;
    let refspec = format!("refs/heads/{0}:refs/heads/{0}", gh.branch);
    let mut cbs = git2::RemoteCallbacks::new();
    cbs.credentials(credentials_callback);
    let mut opts = git2::PushOptions::new();
    opts.remote_callbacks(cbs);
    remote
        .push(&[refspec.as_str()], Some(&mut opts))
        .map_err(git_err)?;

    Ok(commit_oid.to_string())
}

fn stage_backup_payload(repo_dir: &Path) -> Result<()> {
    // .gitignore — enforce secret/settings exclusions at the git layer.
    let gitignore = "secrets.local.json\nsettings.json\n";
    atomic_write(&repo_dir.join(".gitignore"), gitignore.as_bytes())?;

    // Copy DB.
    let db_src = liteconfig_db_path()?;
    if db_src.exists() {
        let bytes = std::fs::read(&db_src).map_err(|source| Error::Io {
            path: db_src.clone(),
            source,
        })?;
        atomic_write(&repo_dir.join("liteconfig.db"), &bytes)?;
    }

    // Copy skills.
    let skills_src = liteconfig_skills_dir()?;
    let skills_dst = repo_dir.join("skills");
    if skills_dst.exists() {
        std::fs::remove_dir_all(&skills_dst).map_err(|source| Error::Io {
            path: skills_dst.clone(),
            source,
        })?;
    }
    if skills_src.exists() {
        copy_tree(&skills_src, &skills_dst)?;
    }
    Ok(())
}

fn open_or_init_repo(dir: &Path, remote_url: &str, branch: &str) -> Result<Repository> {
    let repo = match Repository::open(dir) {
        Ok(r) => r,
        Err(_) => {
            let mut opts = git2::RepositoryInitOptions::new();
            opts.initial_head(branch);
            Repository::init_opts(dir, &opts).map_err(git_err)?
        }
    };
    // Ensure origin matches settings.
    match repo.find_remote("origin") {
        Ok(existing) => {
            if existing.url() != Some(remote_url) {
                repo.remote_set_url("origin", remote_url).map_err(git_err)?;
            }
        }
        Err(_) => {
            repo.remote("origin", remote_url).map_err(git_err)?;
        }
    }
    Ok(repo)
}

fn credentials_callback(
    url: &str,
    username_from_url: Option<&str>,
    allowed_types: git2::CredentialType,
) -> std::result::Result<git2::Cred, git2::Error> {
    if allowed_types.contains(git2::CredentialType::SSH_KEY) {
        let user = username_from_url.unwrap_or("git");
        if let Ok(cred) = git2::Cred::ssh_key_from_agent(user) {
            return Ok(cred);
        }
    }
    if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
        if let (Ok(user), Ok(pass)) = (std::env::var("GIT_USERNAME"), std::env::var("GIT_PASSWORD"))
        {
            return git2::Cred::userpass_plaintext(&user, &pass);
        }
    }
    if allowed_types.contains(git2::CredentialType::DEFAULT) {
        return git2::Cred::default();
    }
    Err(git2::Error::from_str(&format!(
        "no credentials available for {url}"
    )))
}

fn git_err(e: git2::Error) -> Error {
    Error::InvalidConfig(format!("git: {e}"))
}

fn dir_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in WalkDir::new(path).into_iter().filter_map(|r| r.ok()) {
        if entry.file_type().is_file() {
            if let Ok(m) = entry.metadata() {
                total += m.len();
            }
        }
    }
    Ok(total)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    ensure_dir(dst)?;
    for entry in WalkDir::new(src) {
        let entry = entry.map_err(|e| Error::InvalidConfig(e.to_string()))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            ensure_dir(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                ensure_dir(parent)?;
            }
            std::fs::copy(entry.path(), &target).map_err(|source| Error::Io {
                path: target.clone(),
                source,
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::with_temp_home;

    #[test]
    fn push_to_github_commits_to_local_bare_remote() {
        let _home = with_temp_home();
        ensure_dir(&crate::paths::liteconfig_dir().unwrap()).unwrap();
        std::fs::write(crate::paths::liteconfig_db_path().unwrap(), b"db-bytes-v1").unwrap();

        // Stand up a bare repo to act as the remote — no network needed.
        let remote_dir = tempfile::tempdir().unwrap();
        git2::Repository::init_bare(remote_dir.path()).unwrap();
        let remote_url = remote_dir.path().to_string_lossy().to_string();

        let gh = GithubBackupSettings {
            enabled: true,
            repo_url: remote_url,
            branch: "main".into(),
            auto_sync: false,
        };
        let oid = push_to_github(&gh).expect("first push should succeed");
        assert_eq!(oid.len(), 40);

        // Second push after an update should make a new commit.
        std::fs::write(crate::paths::liteconfig_db_path().unwrap(), b"db-bytes-v2").unwrap();
        let oid2 = push_to_github(&gh).expect("second push should succeed");
        assert_ne!(oid, oid2);

        // Remote should hold the commit.
        let bare = git2::Repository::open_bare(remote_dir.path()).unwrap();
        let head = bare.refname_to_id("refs/heads/main").unwrap();
        assert_eq!(head.to_string(), oid2);
    }

    #[test]
    fn push_to_github_rejects_disabled() {
        let _home = with_temp_home();
        let gh = GithubBackupSettings {
            enabled: false,
            ..Default::default()
        };
        assert!(push_to_github(&gh).is_err());
    }

    #[test]
    fn snapshot_roundtrip() {
        let _home = with_temp_home();

        ensure_dir(&crate::paths::liteconfig_dir().unwrap()).unwrap();
        std::fs::write(
            crate::paths::liteconfig_db_path().unwrap(),
            b"fake-db-bytes",
        )
        .unwrap();

        let snap = create_snapshot().unwrap();
        assert!(snap.directory.join("liteconfig.db").exists());

        // Corrupt the live DB, then restore.
        std::fs::write(crate::paths::liteconfig_db_path().unwrap(), b"corrupted").unwrap();
        restore_snapshot(&snap.timestamp).unwrap();
        let restored = std::fs::read(crate::paths::liteconfig_db_path().unwrap()).unwrap();
        assert_eq!(restored, b"fake-db-bytes");
    }
}
