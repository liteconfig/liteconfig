//! External skill repository record. Each row represents a remote (or local
//! filesystem path) that liteconfig syncs skills from. Skills scanned out of
//! the clone land in the `skills` table with `SkillSource::Github`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRepo {
    pub id: String,
    /// Display name — `owner/name` for GitHub, or the basename for local paths.
    pub name: String,
    /// Parsed `owner` segment for GitHub repos. `None` for local-path repos.
    #[serde(default)]
    pub owner: Option<String>,
    /// Parsed `repo` segment for GitHub repos, or the directory basename for
    /// local-path repos.
    pub repo: String,
    pub branch: String,
    /// Canonical URL (git remote) or absolute local path.
    pub url: String,
    #[serde(default)]
    pub last_synced_at: Option<i64>,
    #[serde(default)]
    pub skill_count: u32,
}
