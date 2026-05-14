//! External skill repositories: register a source (GitHub `owner/name`, a
//! full `https://github.com/...` URL, or a local filesystem path), clone or
//! re-read it, and upsert each skill subdir into the `skills` table with
//! `SkillSource::Github { owner, name, branch }`.
//!
//! Dedup rule for the upsert: key by `(skill_name, content_hash)`.
//! - Same name + same hash → flip this repo's per-agent enabled bit, no new
//!   row.
//! - Same name + different hash → insert a new row with the name suffixed by
//!   ` (owner/repo)` so both variants coexist.

use std::path::{Path, PathBuf};

use git2::Repository;

use crate::db::Database;
use crate::fs_util::hash_directory;
use crate::model::skill::{Skill, SkillSource, SyncMethod};
use crate::model::skill_repo::SkillRepo;
use crate::paths::{ensure_dir, liteconfig_repos_dir};
use crate::{Error, Result};

/// How the caller identified the repo. Parsed from the raw string passed to
/// [`add`]; each variant resolves to a different clone/scan strategy in
/// [`sync`].
#[derive(Debug, Clone)]
enum RepoKind {
    /// A GitHub repo identified by `owner/name`, optionally with a non-main
    /// default branch. `url` is the https git URL we'll use for cloning.
    Github {
        owner: String,
        name: String,
        branch: String,
        url: String,
    },
    /// A plain directory on disk. `url` is the absolute path; scans read
    /// directly from it with no cloning.
    Local { path: PathBuf },
}

/// Parse the caller's `owner_or_url` string and register a new repo in the
/// DB. The clone itself happens later in [`sync`] so adding many repos stays
/// cheap and the UI can show them before the network round-trips.
///
/// `branch_override` lets the caller specify the git branch explicitly —
/// critical for presets like `ComposioHQ/awesome-claude-skills` whose
/// default branch is `master`, not `main`. If `None`, the parser either
/// picks up a `#branch` suffix from the input (e.g. `owner/name#dev`) or
/// falls back to `main`.
pub fn add(db: &Database, owner_or_url: &str, branch_override: Option<&str>) -> Result<SkillRepo> {
    let input = owner_or_url.trim();
    if input.is_empty() {
        return Err(Error::InvalidConfig("empty repo identifier".into()));
    }

    let kind = parse_repo_kind(input)?;
    let kind = apply_branch_override(kind, branch_override);
    let id = uuid::Uuid::new_v4().to_string();
    let repo = match kind {
        RepoKind::Github {
            owner,
            name,
            branch,
            url,
        } => SkillRepo {
            id,
            name: format!("{owner}/{name}"),
            owner: Some(owner),
            repo: name,
            branch,
            url,
            last_synced_at: None,
            skill_count: 0,
        },
        RepoKind::Local { path } => {
            let basename = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("repo")
                .to_string();
            SkillRepo {
                id,
                name: basename.clone(),
                owner: None,
                repo: basename,
                branch: "main".into(),
                url: path.to_string_lossy().into_owned(),
                last_synced_at: None,
                skill_count: 0,
            }
        }
    };
    db.upsert_skill_repo(&repo)?;
    Ok(repo)
}

/// Clone (GitHub) or re-read (local) the named repo and upsert every skill
/// subdirectory found. Returns the updated `SkillRepo` with `skill_count` +
/// `last_synced_at` refreshed.
pub fn sync(db: &Database, id: &str) -> Result<SkillRepo> {
    let mut record = db
        .get_skill_repo(id)?
        .ok_or_else(|| Error::InvalidConfig(format!("skill repo not found: {id}")))?;

    let scan_root = materialize(&record)?;
    let count = ingest_skills(db, &record, &scan_root)?;

    record.skill_count = count as u32;
    record.last_synced_at = Some(chrono::Utc::now().timestamp_millis());
    db.upsert_skill_repo(&record)?;
    Ok(record)
}

/// Convenience over [`sync`]: sweep every registered repo.
pub fn sync_all(db: &Database) -> Result<Vec<SkillRepo>> {
    let mut out = Vec::new();
    for r in db.list_skill_repos()? {
        out.push(sync(db, &r.id)?);
    }
    Ok(out)
}

/// Remove a repo record. Skills previously scanned from it stay in the DB —
/// the skills aren't coupled to the repo row and removing skills is a
/// separate user decision.
pub fn remove(db: &Database, id: &str) -> Result<()> {
    db.delete_skill_repo(id)
}

// ---------- parsing ----------

/// Split an input like `owner/name#branch` into `(owner/name, Some("branch"))`.
/// No `#` → `(input, None)`. Used by every parser branch below so the syntax
/// works uniformly for shorthand, https, and ssh URLs.
fn split_branch_suffix(input: &str) -> (&str, Option<&str>) {
    match input.rsplit_once('#') {
        Some((base, branch)) if !branch.is_empty() => (base, Some(branch)),
        _ => (input, None),
    }
}

/// Overlay an explicit branch onto a parsed `RepoKind`. Local repos ignore
/// the override (the path IS the source of truth); GitHub repos get their
/// branch replaced.
fn apply_branch_override(kind: RepoKind, override_branch: Option<&str>) -> RepoKind {
    let Some(b) = override_branch else {
        return kind;
    };
    match kind {
        RepoKind::Github {
            owner, name, url, ..
        } => RepoKind::Github {
            owner,
            name,
            branch: b.to_string(),
            url,
        },
        other => other,
    }
}

fn parse_repo_kind(input: &str) -> Result<RepoKind> {
    let path = Path::new(input);
    if path.is_absolute() && path.exists() {
        return Ok(RepoKind::Local {
            path: path.to_path_buf(),
        });
    }

    let (base, branch_suffix) = split_branch_suffix(input);
    let branch = branch_suffix.unwrap_or("main").to_string();

    if let Some(rest) = base
        .strip_prefix("https://github.com/")
        .or_else(|| base.strip_prefix("http://github.com/"))
    {
        let rest = rest.trim_end_matches('/').trim_end_matches(".git");
        let mut it = rest.split('/');
        let owner = it.next().unwrap_or("").to_string();
        let name = it.next().unwrap_or("").to_string();
        if owner.is_empty() || name.is_empty() {
            return Err(Error::InvalidConfig(format!(
                "unrecognised github url: {input}"
            )));
        }
        return Ok(RepoKind::Github {
            owner: owner.clone(),
            name: name.clone(),
            branch,
            url: format!("https://github.com/{owner}/{name}.git"),
        });
    }

    if let Some(rest) = base.strip_prefix("git@github.com:") {
        let rest = rest.trim_end_matches('/').trim_end_matches(".git");
        let mut it = rest.split('/');
        let owner = it.next().unwrap_or("").to_string();
        let name = it.next().unwrap_or("").to_string();
        if owner.is_empty() || name.is_empty() {
            return Err(Error::InvalidConfig(format!(
                "unrecognised github ssh url: {input}"
            )));
        }
        return Ok(RepoKind::Github {
            owner,
            name,
            branch,
            url: base.to_string(),
        });
    }

    // `owner/name` shorthand — no slashes beyond one, and both halves non-empty.
    let parts: Vec<&str> = base.split('/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        let owner = parts[0].to_string();
        let name = parts[1].to_string();
        return Ok(RepoKind::Github {
            owner: owner.clone(),
            name: name.clone(),
            branch,
            url: format!("https://github.com/{owner}/{name}.git"),
        });
    }

    Err(Error::InvalidConfig(format!(
        "could not parse repo identifier: {input}"
    )))
}

// ---------- materialization ----------

/// Resolve the on-disk directory that contains this repo's skills. For
/// GitHub repos: clone into `~/.liteconfig/repos/<id>/` (or pull the
/// existing clone). For local-path repos: return the path as-is.
fn materialize(repo: &SkillRepo) -> Result<PathBuf> {
    // Local path.
    let p = Path::new(&repo.url);
    if p.is_absolute() && p.exists() {
        return Ok(p.to_path_buf());
    }

    // GitHub clone-or-fetch.
    let dest = liteconfig_repos_dir()?.join(&repo.id);
    ensure_dir(dest.parent().unwrap_or(Path::new("/")))?;

    match Repository::open(&dest) {
        Ok(r) => {
            let mut remote = r.find_remote("origin").map_err(git_err)?;
            remote.fetch(&[&repo.branch], None, None).map_err(git_err)?;
            let fetch_head = r
                .find_reference("FETCH_HEAD")
                .and_then(|f| f.peel_to_commit())
                .map_err(git_err)?;
            r.reset(fetch_head.as_object(), git2::ResetType::Hard, None)
                .map_err(git_err)?;
        }
        Err(_) => {
            // `clone` honors HEAD's default branch; set the requested branch
            // explicitly so non-`main` presets (e.g. `master`) work.
            let mut builder = git2::build::RepoBuilder::new();
            builder.branch(&repo.branch);
            builder.clone(&repo.url, &dest).map_err(git_err)?;
        }
    }
    Ok(dest)
}

// ---------- scanning ----------

/// Walk `scan_root` one level deep; every subdir with a `SKILL.md` becomes a
/// skill candidate. Returns the number of skills upserted.
fn ingest_skills(db: &Database, repo: &SkillRepo, scan_root: &Path) -> Result<usize> {
    use std::collections::BTreeMap;
    let mut existing_by_name: BTreeMap<String, Skill> = db
        .list_skills()?
        .into_iter()
        .map(|s| (s.name.clone(), s))
        .collect();

    let now = chrono::Utc::now().timestamp_millis();
    let mut count = 0usize;

    let entries = match std::fs::read_dir(scan_root) {
        Ok(it) => it,
        Err(_) => return Ok(0),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("SKILL.md").exists() {
            continue;
        }
        let Some(dir_name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };

        let hash = hash_directory(&path).ok();
        let description = read_skill_description(&path);
        let source = SkillSource::Github {
            owner: repo.owner.clone().unwrap_or_else(|| repo.name.clone()),
            name: repo.repo.clone(),
            branch: repo.branch.clone(),
        };

        // Dedup: same name + same hash → already installed, no new row.
        if let Some(existing) = existing_by_name.get(&dir_name) {
            if existing.content_hash.is_some() && existing.content_hash == hash {
                count += 1;
                continue;
            }
        }

        // Same name + different hash → suffix with repo tag so they coexist.
        let name = if existing_by_name.contains_key(&dir_name) {
            format!("{dir_name} ({})", repo.name)
        } else {
            dir_name.clone()
        };

        let skill = Skill {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.clone(),
            description,
            directory: path.clone(),
            source,
            sync_method: SyncMethod::Inherit,
            enabled: Default::default(),
            content_hash: hash,
            last_synced_hash: None,
            installed_at: now,
            updated_at: now,
        };
        db.upsert_skill(&skill)?;
        existing_by_name.insert(name, skill);
        count += 1;
    }

    Ok(count)
}

fn read_skill_description(dir: &Path) -> Option<String> {
    for candidate in ["SKILL.md", "README.md", "readme.md"] {
        let p = dir.join(candidate);
        if let Ok(text) = std::fs::read_to_string(&p) {
            let first = text
                .lines()
                .map(|l| l.trim_start_matches('#').trim())
                .find(|l| !l.is_empty())?;
            return Some(first.to_string());
        }
    }
    None
}

fn git_err(e: git2::Error) -> Error {
    Error::InvalidConfig(format!("git: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_shorthand_and_urls() {
        assert!(matches!(
            parse_repo_kind("anthropic/cookbook").unwrap(),
            RepoKind::Github { .. }
        ));
        assert!(matches!(
            parse_repo_kind("https://github.com/anthropic/cookbook.git").unwrap(),
            RepoKind::Github { .. }
        ));
        assert!(matches!(
            parse_repo_kind("git@github.com:anthropic/cookbook.git").unwrap(),
            RepoKind::Github { .. }
        ));
        assert!(parse_repo_kind("").is_err());
        assert!(parse_repo_kind("just-a-name").is_err());
    }
}
