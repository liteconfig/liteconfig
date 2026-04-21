//! Live search against skills.sh. cc-switch queries the same endpoint, so
//! result shapes should stay compatible.
//!
//! The service is deliberately synchronous — callers that need to keep the
//! UI responsive submit this through the TUI's [`TaskRunner`] so the HTTP
//! round-trip happens off the main thread.

use std::time::Duration;

use serde::Deserialize;

use crate::{Error, Result};

/// A single hit returned from skills.sh. Only the fields we actually surface
/// in the popup are pulled out — extra keys in the response are ignored.
#[derive(Debug, Clone)]
pub struct ExternalSkill {
    pub skill_id: String,
    pub name: String,
    pub source: String,
    pub installs: Option<u64>,
    pub repo_owner: Option<String>,
    pub repo_name: Option<String>,
    pub repo_branch: String,
}

impl ExternalSkill {
    /// What the user would pass to [`skill_repo_service::add`] to install
    /// this result. For GitHub-backed entries that's the shorthand
    /// `owner/name`; for anything else it's the raw `source` string.
    pub fn add_arg(&self) -> String {
        match (&self.repo_owner, &self.repo_name) {
            (Some(o), Some(n)) => format!("{o}/{n}"),
            _ => self.source.clone(),
        }
    }
}

/// Build the search URL without running it. Split out so the test suite can
/// assert the URL shape without opening a socket.
pub fn search_url(query: &str, limit: u32, offset: u32) -> String {
    format!(
        "https://skills.sh/api/search?q={}&limit={}&offset={}",
        urlencode(query),
        limit,
        offset
    )
}

/// Hit skills.sh's `/api/search` endpoint synchronously. 10s timeout matches
/// cc-switch; non-`github.com/...` sources are filtered out since we can
/// only act on git-cloneable hits.
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
            let source = hit.source.clone();
            if !source.starts_with("github.com/") {
                return None;
            }
            let rest = source.strip_prefix("github.com/").unwrap_or(&source);
            let mut it = rest.split('/');
            let owner = it.next().map(str::to_string);
            let name = it.next().map(str::to_string);
            Some(ExternalSkill {
                skill_id: hit.skill_id.unwrap_or_else(|| hit.name.clone()),
                name: hit.name,
                source,
                installs: hit.installs,
                repo_owner: owner,
                repo_name: name,
                repo_branch: hit.branch.unwrap_or_else(|| "main".to_string()),
            })
        })
        .collect())
}

fn urlencode(s: &str) -> String {
    // skills.sh cares about `+` / spaces / `&` — the standard library has no
    // encoder so we do the minimal percent-escape we need here.
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

#[derive(Debug, Deserialize)]
struct SearchBody {
    #[serde(default)]
    skills: Vec<HitRaw>,
}

#[derive(Debug, Deserialize)]
struct HitRaw {
    #[serde(default, rename = "skillId")]
    skill_id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    installs: Option<u64>,
    #[serde(default)]
    branch: Option<String>,
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
    fn parse_filters_non_github_sources() {
        let body = r#"{
            "skills": [
                {"skillId":"a","name":"A","source":"github.com/foo/bar","branch":"main"},
                {"skillId":"b","name":"B","source":"https://example.com/b"},
                {"skillId":"c","name":"C","source":"github.com/baz/qux","installs":42}
            ]
        }"#;
        let out = parse_response(body).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].repo_owner.as_deref(), Some("foo"));
        assert_eq!(out[0].repo_name.as_deref(), Some("bar"));
        assert_eq!(out[1].installs, Some(42));
    }

    #[test]
    fn add_arg_prefers_owner_name_shorthand() {
        let s = ExternalSkill {
            skill_id: "x".into(),
            name: "x".into(),
            source: "github.com/foo/bar".into(),
            installs: None,
            repo_owner: Some("foo".into()),
            repo_name: Some("bar".into()),
            repo_branch: "main".into(),
        };
        assert_eq!(s.add_arg(), "foo/bar");
    }

    #[test]
    #[ignore = "network-dependent; run manually with --ignored"]
    fn live_hit_skills_sh() {
        let hits = search("anthropic", 5, 0).unwrap();
        assert!(!hits.is_empty());
    }
}
