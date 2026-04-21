//! Live search against skills.sh. API shape verified against cc-switch's
//! implementation in `cc-switch/src-tauri/src/services/skill.rs`.
//!
//! The service is deliberately synchronous — callers that need to keep the
//! UI responsive submit this through the TUI's [`TaskRunner`] so the HTTP
//! round-trip happens off the main thread.

use std::time::Duration;

use serde::Deserialize;

use crate::{Error, Result};

/// A single hit returned from skills.sh.
#[derive(Debug, Clone)]
pub struct ExternalSkill {
    /// The specific skill's directory name inside the repo (e.g. "find-skills").
    pub skill_id: String,
    /// Human-readable display name from `SKILL.md`.
    pub name: String,
    /// Raw `owner/repo` string from the API.
    pub source: String,
    pub installs: u64,
    pub repo_owner: String,
    pub repo_name: String,
    pub repo_branch: String,
    /// Direct link to the repo on GitHub for the detail view.
    pub readme_url: String,
}

impl ExternalSkill {
    /// Shorthand passed to `skill_repo_service::add`.
    pub fn add_arg(&self) -> String {
        format!("{}/{}", self.repo_owner, self.repo_name)
    }
}

/// Build the search URL without running it — extracted so tests can assert
/// the URL shape without opening a socket.
pub fn search_url(query: &str, limit: u32, offset: u32) -> String {
    format!(
        "https://skills.sh/api/search?q={}&limit={}&offset={}",
        urlencode(query),
        limit,
        offset
    )
}

/// Hit skills.sh's `/api/search` endpoint synchronously. 10s timeout matches
/// cc-switch. Entries where `source` contains a dot in either the owner or
/// repo segment (e.g. `skills.volces.com`) are filtered — they are not
/// plain GitHub `owner/repo` handles and cannot be cloned by `git2`.
pub fn search(query: &str, limit: u32, offset: u32) -> Result<Vec<ExternalSkill>> {
    let url = search_url(query, limit, offset);
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .build();
    let raw = agent
        .get(&url)
        .call()
        .map_err(|e| Error::InvalidConfig(format!("skills.sh: {e}")))?
        .into_string()
        .map_err(|e| Error::InvalidConfig(format!("skills.sh body: {e}")))?;
    parse_response(&raw)
}

fn parse_response(raw: &str) -> Result<Vec<ExternalSkill>> {
    let body: SearchBody = serde_json::from_str(raw)
        .map_err(|e| Error::InvalidConfig(format!("skills.sh: bad json: {e}")))?;
    Ok(body
        .skills
        .into_iter()
        .filter_map(|hit| {
            // source is "owner/repo" — split and reject anything with dots
            // (non-GitHub domains like "skills.volces.com/path").
            let mut parts = hit.source.splitn(2, '/');
            let owner = parts.next().unwrap_or("").to_string();
            let repo = parts.next().unwrap_or("").to_string();
            if owner.is_empty() || repo.is_empty() || owner.contains('.') || repo.contains('.') {
                return None;
            }
            let readme_url = format!("https://github.com/{owner}/{repo}");
            Some(ExternalSkill {
                skill_id: hit.skill_id,
                name: hit.name,
                source: hit.source,
                installs: hit.installs,
                repo_owner: owner,
                repo_name: repo,
                repo_branch: "main".to_string(),
                readme_url,
            })
        })
        .collect())
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Top-level response wrapper. cc-switch also reads `query`, `count`,
/// `duration_ms`, and `searchType` but we only need `skills`.
#[derive(Debug, Deserialize)]
struct SearchBody {
    #[serde(default)]
    skills: Vec<HitRaw>,
}

/// Single hit as returned by the API. Field names match the wire format
/// exactly (cc-switch: `SkillsShApiSkill`).
#[derive(Debug, Deserialize)]
struct HitRaw {
    /// UUID-style record id (ignored after parsing).
    #[allow(dead_code)]
    id: String,
    /// The skill's directory name inside the repo ("skillId" in the API).
    #[serde(rename = "skillId")]
    skill_id: String,
    name: String,
    installs: u64,
    /// `owner/repo` shorthand — no `github.com/` prefix.
    source: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_shape_matches_cc_switch() {
        assert_eq!(
            search_url("seo", 10, 0),
            "https://skills.sh/api/search?q=seo&limit=10&offset=0"
        );
        assert_eq!(
            search_url("hello world", 5, 20),
            "https://skills.sh/api/search?q=hello+world&limit=5&offset=20"
        );
        assert_eq!(
            search_url("a&b", 1, 0),
            "https://skills.sh/api/search?q=a%26b&limit=1&offset=0"
        );
    }

    #[test]
    fn parse_uses_owner_slash_repo_source_format() {
        let body = r#"{
            "skills": [
                {"id":"u1","skillId":"find-skills","name":"Find Skills","installs":10,"source":"anthropics/skills"},
                {"id":"u2","skillId":"seo","name":"SEO","installs":5,"source":"skills.volces.com/seo"},
                {"id":"u3","skillId":"ci","name":"CI","installs":2,"source":"acme/devops"}
            ]
        }"#;
        let out = parse_response(body).unwrap();
        // "skills.volces.com/seo" rejected because owner contains '.'
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].repo_owner, "anthropics");
        assert_eq!(out[0].repo_name, "skills");
        assert_eq!(out[0].skill_id, "find-skills");
        assert_eq!(out[0].installs, 10);
        assert_eq!(out[0].readme_url, "https://github.com/anthropics/skills");
        assert_eq!(out[1].repo_owner, "acme");
    }

    #[test]
    fn parse_rejects_empty_or_missing_parts() {
        let body = r#"{
            "skills": [
                {"id":"u1","skillId":"x","name":"X","installs":0,"source":"noslash"},
                {"id":"u2","skillId":"y","name":"Y","installs":0,"source":"/leadingslash"},
                {"id":"u3","skillId":"z","name":"Z","installs":0,"source":"ok/repo"}
            ]
        }"#;
        let out = parse_response(body).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].repo_name, "repo");
    }

    #[test]
    fn add_arg_returns_owner_name_shorthand() {
        let s = ExternalSkill {
            skill_id: "x".into(),
            name: "x".into(),
            source: "foo/bar".into(),
            installs: 0,
            repo_owner: "foo".into(),
            repo_name: "bar".into(),
            repo_branch: "main".into(),
            readme_url: "https://github.com/foo/bar".into(),
        };
        assert_eq!(s.add_arg(), "foo/bar");
    }

    #[test]
    #[ignore = "network-dependent; run manually with --ignored"]
    fn live_hit_skills_sh() {
        let hits = search("fastapi", 5, 0).unwrap();
        assert!(!hits.is_empty());
    }
}
