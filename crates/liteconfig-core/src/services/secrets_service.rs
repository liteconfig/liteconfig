//! Secret storage lives outside the SQLite DB (so the DB is always safe to
//! push to GitHub). Secrets are written to `~/.liteconfig/secrets.local.json`
//! with mode 0600 and never leave the device.
//!
//! Profile configs reference secrets via `"@secret:<name>"` placeholders;
//! this module resolves those placeholders into concrete values at write
//! time, right before the bytes land on disk.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fs_util::{atomic_write_private, read_to_string};
use crate::paths::liteconfig_secrets_path;
use crate::{Error, Result};

pub const SECRET_PREFIX: &str = "@secret:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub value: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub created_at: i64,
}

fn default_kind() -> String {
    "api_key".to_string()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretStore {
    pub entries: BTreeMap<String, Secret>,
}

impl SecretStore {
    pub fn load_or_default() -> Result<Self> {
        let path = liteconfig_secrets_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = read_to_string(&path)?;
        let store: SecretStore = serde_json::from_str(&text).map_err(|source| Error::Json {
            path: path.clone(),
            source,
        })?;
        Ok(store)
    }

    pub fn save(&self) -> Result<()> {
        let path = liteconfig_secrets_path()?;
        let bytes = serde_json::to_vec_pretty(self)?;
        atomic_write_private(&path, &bytes)
    }

    pub fn get(&self, name: &str) -> Option<&Secret> {
        self.entries.get(name)
    }

    pub fn put(&mut self, name: impl Into<String>, value: impl Into<String>, kind: &str) {
        self.entries.insert(
            name.into(),
            Secret {
                value: value.into(),
                kind: kind.to_string(),
                created_at: chrono::Utc::now().timestamp_millis(),
            },
        );
    }

    pub fn remove(&mut self, name: &str) -> Option<Secret> {
        self.entries.remove(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

/// Walk `value` and replace every string `"@secret:<name>"` with the
/// corresponding value from `store`. Returns an error listing every
/// unresolved reference.
pub fn resolve(value: &Value, store: &SecretStore) -> Result<Value> {
    let mut unresolved = Vec::new();
    let resolved = resolve_inner(value, store, &mut unresolved);
    if !unresolved.is_empty() {
        return Err(Error::UnresolvedSecret(unresolved.join(", ")));
    }
    Ok(resolved)
}

/// Walk `value` and replace every string matching an existing secret name
/// with `"@secret:<name>"`. Used when importing an existing live config so
/// that the DB never captures raw credentials.
pub fn redact(value: &Value, store: &SecretStore) -> Value {
    match value {
        Value::String(s) => {
            if let Some(name) = find_name_for_value(s, store) {
                Value::String(format!("{SECRET_PREFIX}{name}"))
            } else {
                value.clone()
            }
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| redact(v, store)).collect()),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), redact(v, store));
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

/// Return every unresolved `@secret:*` reference found in `value`. Useful for
/// the restore-to-new-device flow that surfaces a "re-enter these" prompt.
pub fn unresolved_refs(value: &Value, store: &SecretStore) -> Vec<String> {
    let mut out = Vec::new();
    collect_unresolved(value, store, &mut out);
    out.sort();
    out.dedup();
    out
}

fn resolve_inner(value: &Value, store: &SecretStore, unresolved: &mut Vec<String>) -> Value {
    match value {
        Value::String(s) => {
            if let Some(name) = s.strip_prefix(SECRET_PREFIX) {
                match store.get(name) {
                    Some(secret) => Value::String(secret.value.clone()),
                    None => {
                        unresolved.push(name.to_string());
                        Value::String(String::new())
                    }
                }
            } else {
                value.clone()
            }
        }
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| resolve_inner(v, store, unresolved))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                out.insert(k.clone(), resolve_inner(v, store, unresolved));
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

fn collect_unresolved(value: &Value, store: &SecretStore, out: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if let Some(name) = s.strip_prefix(SECRET_PREFIX) {
                if store.get(name).is_none() {
                    out.push(name.to_string());
                }
            }
        }
        Value::Array(arr) => arr.iter().for_each(|v| collect_unresolved(v, store, out)),
        Value::Object(map) => map.values().for_each(|v| collect_unresolved(v, store, out)),
        _ => {}
    }
}

fn find_name_for_value<'a>(raw: &str, store: &'a SecretStore) -> Option<&'a str> {
    // Only redact meaningful, long-ish strings — avoid matching e.g. "1" or "".
    if raw.len() < 8 {
        return None;
    }
    store
        .entries
        .iter()
        .find(|(_, s)| s.value == raw)
        .map(|(name, _)| name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store_with(name: &str, value: &str) -> SecretStore {
        let mut s = SecretStore::default();
        s.put(name, value, "api_key");
        s
    }

    #[test]
    fn resolve_replaces_known_secrets() {
        let store = store_with("claude-primary", "sk-ant-test-xyz");
        let config = json!({ "env": { "ANTHROPIC_API_KEY": "@secret:claude-primary" } });
        let out = resolve(&config, &store).unwrap();
        assert_eq!(out["env"]["ANTHROPIC_API_KEY"], "sk-ant-test-xyz");
    }

    #[test]
    fn resolve_errors_on_missing() {
        let store = SecretStore::default();
        let config = json!({ "key": "@secret:missing" });
        let err = resolve(&config, &store).unwrap_err();
        assert!(matches!(err, Error::UnresolvedSecret(_)));
    }

    #[test]
    fn redact_replaces_values_with_refs() {
        let store = store_with("claude-primary", "sk-ant-test-xyz-1234");
        let config =
            json!({ "apiKey": "sk-ant-test-xyz-1234", "nested": { "k": "sk-ant-test-xyz-1234" } });
        let out = redact(&config, &store);
        assert_eq!(out["apiKey"], "@secret:claude-primary");
        assert_eq!(out["nested"]["k"], "@secret:claude-primary");
    }

    #[test]
    fn unresolved_refs_listed() {
        let store = store_with("present", "sk-xyz-123456");
        let config = json!({
            "a": "@secret:missing-1",
            "b": "@secret:present",
            "c": ["@secret:missing-2"]
        });
        let refs = unresolved_refs(&config, &store);
        assert_eq!(refs, vec!["missing-1".to_string(), "missing-2".to_string()]);
    }
}
