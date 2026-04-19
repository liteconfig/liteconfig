//! Central app state. Every user action is funneled into `App` via `handle_*`
//! methods; the UI layer only reads state.

use std::collections::{BTreeMap, HashSet};

use color_eyre::eyre::Result;
use liteconfig_core::agents::for_kind as agent_for_kind;
use liteconfig_core::db::Database;
use liteconfig_core::model::agent::{AgentKind, ALL_AGENT_KINDS};
use liteconfig_core::model::mcp::McpServer;
use liteconfig_core::model::profile::Profile;
use liteconfig_core::model::rule::Rule;
use liteconfig_core::model::skill::Skill;
use liteconfig_core::paths::liteconfig_dir;
use liteconfig_core::services::backup_service;
use liteconfig_core::services::backup_service::Snapshot;
use liteconfig_core::services::mcp_service;
use liteconfig_core::services::profile_service;
use liteconfig_core::services::rule_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::services::skill_service;
use liteconfig_core::settings::Settings;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Profiles,
    Skills,
    Mcp,
    Rules,
    Backup,
    Sessions,
    Settings,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[
        Tab::Profiles,
        Tab::Skills,
        Tab::Mcp,
        Tab::Rules,
        Tab::Backup,
        Tab::Sessions,
        Tab::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Profiles => "Profiles",
            Tab::Skills => "Skills",
            Tab::Mcp => "MCP",
            Tab::Rules => "Rules",
            Tab::Backup => "Backup",
            Tab::Sessions => "Sessions",
            Tab::Settings => "Settings",
        }
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }

    pub fn from_index(i: usize) -> Option<Self> {
        Self::ALL.get(i).copied()
    }
}

/// Short-lived user-visible message. The UI pops it after a few frames.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: std::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Per-agent row of profiles shown in the Profiles tab. We keep a view-level
/// struct so the UI never re-queries the DB mid-paint.
#[derive(Debug, Clone)]
pub struct ProfileView {
    #[allow(dead_code)]
    pub agent: AgentKind,
    pub profiles: Vec<Profile>,
    pub selected: usize,
}

/// View-level state for the Skills tab: the full list + multi-selection +
/// current focus. Rebuilt from the DB by `reload_skills`.
#[derive(Debug, Clone, Default)]
pub struct SkillsView {
    pub skills: Vec<Skill>,
    pub selected_ids: HashSet<String>,
    pub focused_idx: usize,
}

/// Modal state for "which agents should this row sync to?" popup. Used by
/// the Skills tab and the MCP tab — the `target` tag tells `commit` which
/// service to write back to.
#[derive(Debug, Clone)]
pub struct AgentPopup {
    pub target: AgentPopupTarget,
    pub row_id: String,
    pub row_name: String,
    /// Index into `ALL_AGENT_KINDS` for the popup cursor.
    pub cursor: usize,
    /// Working copy of enable flags — committed on OK, discarded on Cancel.
    pub enabled: BTreeMap<AgentKind, bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentPopupTarget {
    Skill,
    Mcp,
    Rule,
}

/// View-level state for the MCP tab.
#[derive(Debug, Clone, Default)]
pub struct McpView {
    pub servers: Vec<McpServer>,
    pub selected_ids: HashSet<String>,
    pub focused_idx: usize,
}

/// View-level state for the Backup tab.
#[derive(Debug, Clone, Default)]
pub struct BackupView {
    pub snapshots: Vec<Snapshot>,
    pub focused_idx: usize,
}

/// View-level state for the Rules tab.
#[derive(Debug, Clone, Default)]
pub struct RulesView {
    pub rules: Vec<Rule>,
    pub selected_ids: HashSet<String>,
    pub focused_idx: usize,
}

pub struct App {
    pub db: Database,
    pub settings: Settings,
    pub secrets: SecretStore,
    pub theme: Theme,

    pub active_tab: Tab,
    pub should_quit: bool,

    pub profile_views: BTreeMap<AgentKind, ProfileView>,
    /// Index into `ALL_AGENT_KINDS` for the focused row in the Profiles tab.
    pub focused_agent_idx: usize,

    pub skills_view: SkillsView,
    pub mcp_view: McpView,
    pub rules_view: RulesView,
    pub backup_view: BackupView,
    /// When `Some`, the active tab yields input to drive the popup.
    pub agent_popup: Option<AgentPopup>,

    /// Ordered list of available theme slugs (builtin + user). Populated once
    /// on startup; used to cycle themes in the Settings tab.
    pub available_themes: Vec<String>,

    pub toasts: Vec<Toast>,
}

impl App {
    pub fn new(db: Database, settings: Settings, secrets: SecretStore) -> Result<Self> {
        let theme = Theme::by_name(&settings.theme);
        let available_themes = Self::build_theme_list();
        let mut app = Self {
            db,
            settings,
            secrets,
            theme,
            active_tab: Tab::Profiles,
            should_quit: false,
            profile_views: BTreeMap::new(),
            focused_agent_idx: 0,
            skills_view: SkillsView::default(),
            mcp_view: McpView::default(),
            rules_view: RulesView::default(),
            backup_view: BackupView::default(),
            agent_popup: None,
            available_themes,
            toasts: Vec::new(),
        };
        app.reload_profiles()?;
        app.reload_skills()?;
        app.reload_mcp()?;
        app.reload_rules()?;
        app.reload_backups();
        app.auto_import_if_empty();
        Ok(app)
    }

    /// On first launch (all tables empty), pull in whatever profiles / skills /
    /// rules / MCP servers already exist in the user's live configs. Silent —
    /// only the resulting toast announces counts.
    fn auto_import_if_empty(&mut self) {
        let profiles_empty = self.profile_views.values().all(|v| v.profiles.is_empty());
        let skills_empty = self.skills_view.skills.is_empty();
        let rules_empty = self.rules_view.rules.is_empty();
        let mcp_empty = self.mcp_view.servers.is_empty();
        if !(profiles_empty && skills_empty && rules_empty && mcp_empty) {
            return;
        }

        let mut np = 0usize;
        let mut ns = 0usize;
        let mut nr = 0usize;
        let mut nm = 0usize;

        if let Ok(v) = profile_service::import_from_live(&self.db, &self.settings, &self.secrets) {
            np = v.len();
        }
        if let Ok(v) = skill_service::scan_from_live(&self.db, &self.settings) {
            ns = v.len();
        }
        if let Ok(v) = rule_service::import_from_live(&self.db, &self.settings) {
            nr = v.len();
        }
        if let Ok(v) = mcp_service::import_from_live(&self.db, &self.settings) {
            nm = v.len();
        }

        let _ = self.reload_profiles();
        let _ = self.reload_skills();
        let _ = self.reload_rules();
        let _ = self.reload_mcp();

        if np + ns + nr + nm > 0 {
            self.push_toast(
                format!(
                    "Imported {np} profile(s), {ns} skill(s), {nr} rule(s), {nm} MCP server(s)"
                ),
                ToastLevel::Info,
            );
        }
    }

    pub fn reload_profiles(&mut self) -> Result<()> {
        self.profile_views.clear();
        for agent in ALL_AGENT_KINDS {
            let profiles = profile_service::list(&self.db, *agent)?;
            let selected = profiles
                .iter()
                .position(|p| self.settings.current_profile_for(*agent) == Some(p.id.as_str()))
                .unwrap_or(0);
            self.profile_views.insert(
                *agent,
                ProfileView {
                    agent: *agent,
                    profiles,
                    selected,
                },
            );
        }
        Ok(())
    }

    /// Agents that have a profile concept; drives the Profiles tab layout.
    pub fn profile_agents() -> Vec<AgentKind> {
        ALL_AGENT_KINDS
            .iter()
            .copied()
            .filter(|a| a.supports_profiles())
            .collect()
    }

    pub fn focused_agent(&self) -> AgentKind {
        let list = Self::profile_agents();
        list.get(self.focused_agent_idx)
            .copied()
            .unwrap_or(AgentKind::Claude)
    }

    pub fn set_active_tab(&mut self, tab: Tab) {
        self.active_tab = tab;
    }

    pub fn next_tab(&mut self) {
        let i = self.active_tab.index();
        let n = Tab::ALL.len();
        self.active_tab = Tab::from_index((i + 1) % n).unwrap_or(self.active_tab);
    }

    pub fn prev_tab(&mut self) {
        let i = self.active_tab.index();
        let n = Tab::ALL.len();
        self.active_tab = Tab::from_index((i + n - 1) % n).unwrap_or(self.active_tab);
    }

    pub fn push_toast(&mut self, message: impl Into<String>, level: ToastLevel) {
        self.toasts.push(Toast {
            message: message.into(),
            level,
            created_at: std::time::Instant::now(),
        });
        // Keep at most three visible.
        if self.toasts.len() > 3 {
            self.toasts.drain(..self.toasts.len() - 3);
        }
    }

    pub fn tick(&mut self) {
        self.toasts.retain(|t| t.created_at.elapsed().as_secs() < 5);
    }

    // ---------- Profiles tab actions ----------

    pub fn move_profile_selection(&mut self, delta: i32) {
        let agent = self.focused_agent();
        if let Some(view) = self.profile_views.get_mut(&agent) {
            if view.profiles.is_empty() {
                return;
            }
            let len = view.profiles.len() as i32;
            let next = ((view.selected as i32 + delta) % len + len) % len;
            view.selected = next as usize;
        }
    }

    pub fn move_agent_focus(&mut self, delta: i32) {
        let n = Self::profile_agents().len().max(1) as i32;
        let next = ((self.focused_agent_idx as i32 + delta) % n + n) % n;
        self.focused_agent_idx = next as usize;
    }

    /// Switch to the currently-selected profile in the focused agent row.
    pub fn switch_focused_profile(&mut self) {
        let agent = self.focused_agent();
        let Some(view) = self.profile_views.get(&agent) else {
            return;
        };
        let Some(profile) = view.profiles.get(view.selected) else {
            self.push_toast(
                format!("No profile to switch to for {}", agent.display_name()),
                ToastLevel::Warning,
            );
            return;
        };
        let id = profile.id.clone();
        let name = profile.name.clone();
        match profile_service::switch(&self.db, &mut self.settings, &self.secrets, agent, &id) {
            Ok(()) => self.push_toast(
                format!("Switched {} to {}", agent.display_name(), name),
                ToastLevel::Success,
            ),
            Err(e) => self.push_toast(format!("Switch failed: {e}"), ToastLevel::Error),
        }
        // Re-load so the UI reflects the new pointer immediately.
        let _ = self.reload_profiles();
    }

    // ---------- Skills tab actions ----------

    pub fn reload_skills(&mut self) -> Result<()> {
        let skills = skill_service::list(&self.db)?;
        let max_idx = skills.len().saturating_sub(1);
        let focused = self.skills_view.focused_idx.min(max_idx);
        let selected = self
            .skills_view
            .selected_ids
            .iter()
            .filter(|id| skills.iter().any(|s| &s.id == *id))
            .cloned()
            .collect();
        self.skills_view = SkillsView {
            skills,
            selected_ids: selected,
            focused_idx: focused,
        };
        Ok(())
    }

    pub fn move_skill_focus(&mut self, delta: i32) {
        let n = self.skills_view.skills.len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.skills_view.focused_idx as i32 + delta) % len + len) % len;
        self.skills_view.focused_idx = next as usize;
    }

    pub fn focused_skill(&self) -> Option<&Skill> {
        self.skills_view.skills.get(self.skills_view.focused_idx)
    }

    pub fn toggle_focused_skill_selection(&mut self) {
        let Some(skill) = self.focused_skill() else {
            return;
        };
        let id = skill.id.clone();
        if !self.skills_view.selected_ids.insert(id.clone()) {
            self.skills_view.selected_ids.remove(&id);
        }
    }

    pub fn select_all_skills(&mut self) {
        self.skills_view.selected_ids = self
            .skills_view
            .skills
            .iter()
            .map(|s| s.id.clone())
            .collect();
    }

    pub fn clear_skill_selection(&mut self) {
        self.skills_view.selected_ids.clear();
    }

    pub fn cycle_focused_skill_method(&mut self) {
        let Some(skill) = self.focused_skill() else {
            return;
        };
        let id = skill.id.clone();
        let next = skill.sync_method.cycle();
        match skill_service::set_sync_method(&self.db, &id, next) {
            Ok(_) => {
                let _ = self.reload_skills();
                self.push_toast(
                    format!("Sync method → {}", next.as_str()),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Change failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn sync_focused_skill(&mut self) {
        let Some(skill) = self.focused_skill() else {
            self.push_toast("No skill to sync", ToastLevel::Warning);
            return;
        };
        let id = skill.id.clone();
        let name = skill.name.clone();
        match skill_service::sync_one(&self.db, &self.settings, &id) {
            Ok(()) => self.push_toast(format!("Synced {name}"), ToastLevel::Success),
            Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn sync_selected_skills(&mut self) {
        let ids: Vec<String> = self.skills_view.selected_ids.iter().cloned().collect();
        if ids.is_empty() {
            self.push_toast("No skills selected", ToastLevel::Warning);
            return;
        }
        let count = ids.len();
        match skill_service::sync_many(&self.db, &self.settings, &ids) {
            Ok(()) => self.push_toast(format!("Synced {count} skills"), ToastLevel::Success),
            Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn import_skills_from_live(&mut self) {
        match skill_service::scan_from_live(&self.db, &self.settings) {
            Ok(v) => {
                let n = v.len();
                let _ = self.reload_skills();
                self.push_toast(
                    format!("Imported {n} skill(s) from live configs"),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Import failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn import_rules_from_live(&mut self) {
        match rule_service::import_from_live(&self.db, &self.settings) {
            Ok(v) => {
                let n = v.len();
                let _ = self.reload_rules();
                self.push_toast(
                    format!("Imported {n} rule(s) from live configs"),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Import failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn import_profiles_from_live(&mut self) {
        match profile_service::import_from_live(&self.db, &self.settings, &self.secrets) {
            Ok(v) => {
                let n = v.len();
                let _ = self.reload_profiles();
                self.push_toast(
                    format!("Imported {n} profile(s) from live configs"),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Import failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn sync_all_skills(&mut self) {
        let count = self.skills_view.skills.len();
        if count == 0 {
            self.push_toast("No skills to sync", ToastLevel::Warning);
            return;
        }
        match skill_service::sync_all(&self.db, &self.settings) {
            Ok(()) => self.push_toast(format!("Synced all {count} skills"), ToastLevel::Success),
            Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn open_agent_popup_for_focused(&mut self) {
        let Some(skill) = self.focused_skill() else {
            return;
        };
        let mut enabled = BTreeMap::new();
        for agent in ALL_AGENT_KINDS {
            enabled.insert(*agent, skill.is_enabled_for(*agent));
        }
        self.agent_popup = Some(AgentPopup {
            target: AgentPopupTarget::Skill,
            row_id: skill.id.clone(),
            row_name: skill.name.clone(),
            cursor: 0,
            enabled,
        });
    }

    pub fn agent_popup_move(&mut self, delta: i32) {
        let Some(p) = self.agent_popup.as_mut() else {
            return;
        };
        let n = ALL_AGENT_KINDS.len() as i32;
        p.cursor = (((p.cursor as i32 + delta) % n + n) % n) as usize;
    }

    pub fn agent_popup_toggle(&mut self) {
        let Some(p) = self.agent_popup.as_mut() else {
            return;
        };
        let Some(agent) = ALL_AGENT_KINDS.get(p.cursor) else {
            return;
        };
        let entry = p.enabled.entry(*agent).or_insert(false);
        *entry = !*entry;
    }

    pub fn agent_popup_set_all(&mut self, value: bool) {
        let Some(p) = self.agent_popup.as_mut() else {
            return;
        };
        for agent in ALL_AGENT_KINDS {
            p.enabled.insert(*agent, value);
        }
    }

    pub fn agent_popup_cancel(&mut self) {
        self.agent_popup = None;
    }

    pub fn agent_popup_commit(&mut self) {
        let Some(p) = self.agent_popup.take() else {
            return;
        };
        match p.target {
            AgentPopupTarget::Skill => {
                for (agent, enabled) in &p.enabled {
                    if let Err(e) =
                        skill_service::set_enabled(&self.db, &p.row_id, *agent, *enabled)
                    {
                        self.push_toast(format!("Update failed: {e}"), ToastLevel::Error);
                        return;
                    }
                }
                if let Err(e) = skill_service::sync_one(&self.db, &self.settings, &p.row_id) {
                    self.push_toast(format!("Resync failed: {e}"), ToastLevel::Error);
                }
                let _ = self.reload_skills();
            }
            AgentPopupTarget::Mcp => {
                for (agent, enabled) in &p.enabled {
                    if let Err(e) = mcp_service::set_enabled(&self.db, &p.row_id, *agent, *enabled)
                    {
                        self.push_toast(format!("Update failed: {e}"), ToastLevel::Error);
                        return;
                    }
                }
                if let Err(e) = mcp_service::sync_all(&self.db, &self.settings) {
                    self.push_toast(format!("Resync failed: {e}"), ToastLevel::Error);
                }
                let _ = self.reload_mcp();
            }
            AgentPopupTarget::Rule => {
                for (agent, enabled) in &p.enabled {
                    if let Err(e) = rule_service::set_enabled(&self.db, &p.row_id, *agent, *enabled)
                    {
                        self.push_toast(format!("Update failed: {e}"), ToastLevel::Error);
                        return;
                    }
                }
                if let Err(e) = rule_service::sync_all(&self.db, &self.settings) {
                    self.push_toast(format!("Resync failed: {e}"), ToastLevel::Error);
                }
                let _ = self.reload_rules();
            }
        }
        self.push_toast(
            format!("Updated agents for {}", p.row_name),
            ToastLevel::Success,
        );
    }

    /// Helper for hint/status renderers: which sync method the focused skill resolves to.
    pub fn focused_sync_method_label(&self) -> &'static str {
        self.focused_skill()
            .map(|s| s.sync_method.as_str())
            .unwrap_or("-")
    }

    // ---------- MCP tab actions ----------

    pub fn reload_mcp(&mut self) -> Result<()> {
        let servers = mcp_service::list(&self.db)?;
        let max_idx = servers.len().saturating_sub(1);
        let focused = self.mcp_view.focused_idx.min(max_idx);
        let selected = self
            .mcp_view
            .selected_ids
            .iter()
            .filter(|id| servers.iter().any(|s| &s.id == *id))
            .cloned()
            .collect();
        self.mcp_view = McpView {
            servers,
            selected_ids: selected,
            focused_idx: focused,
        };
        Ok(())
    }

    pub fn move_mcp_focus(&mut self, delta: i32) {
        let n = self.mcp_view.servers.len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.mcp_view.focused_idx as i32 + delta) % len + len) % len;
        self.mcp_view.focused_idx = next as usize;
    }

    pub fn focused_mcp(&self) -> Option<&McpServer> {
        self.mcp_view.servers.get(self.mcp_view.focused_idx)
    }

    pub fn toggle_focused_mcp_selection(&mut self) {
        let Some(server) = self.focused_mcp() else {
            return;
        };
        let id = server.id.clone();
        if !self.mcp_view.selected_ids.insert(id.clone()) {
            self.mcp_view.selected_ids.remove(&id);
        }
    }

    pub fn open_agent_popup_for_focused_mcp(&mut self) {
        let Some(server) = self.focused_mcp() else {
            return;
        };
        let mut enabled = BTreeMap::new();
        for agent in ALL_AGENT_KINDS {
            enabled.insert(*agent, server.is_enabled_for(*agent));
        }
        self.agent_popup = Some(AgentPopup {
            target: AgentPopupTarget::Mcp,
            row_id: server.id.clone(),
            row_name: server.name.clone(),
            cursor: 0,
            enabled,
        });
    }

    pub fn sync_all_mcp(&mut self) {
        match mcp_service::sync_all(&self.db, &self.settings) {
            Ok(()) => self.push_toast("Synced MCP to all agents", ToastLevel::Success),
            Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn import_mcp_from_live(&mut self) {
        match mcp_service::import_from_live(&self.db, &self.settings) {
            Ok(servers) => {
                let n = servers.len();
                let _ = self.reload_mcp();
                self.push_toast(
                    format!("Imported {n} MCP server(s) from live configs"),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Import failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn delete_focused_mcp(&mut self) {
        let Some(server) = self.focused_mcp() else {
            return;
        };
        let id = server.id.clone();
        let name = server.name.clone();
        match mcp_service::delete(&self.db, &id) {
            Ok(()) => {
                let _ = self.reload_mcp();
                self.push_toast(format!("Deleted MCP server {name}"), ToastLevel::Success);
            }
            Err(e) => self.push_toast(format!("Delete failed: {e}"), ToastLevel::Error),
        }
    }

    // ---------- Rules tab actions ----------

    pub fn reload_rules(&mut self) -> Result<()> {
        let mut rules = rule_service::list(&self.db)?;
        rules.sort_by_key(|r| r.name.to_lowercase());
        let max_idx = rules.len().saturating_sub(1);
        let focused = self.rules_view.focused_idx.min(max_idx);
        let selected = self
            .rules_view
            .selected_ids
            .iter()
            .filter(|id| rules.iter().any(|r| &r.id == *id))
            .cloned()
            .collect();
        self.rules_view = RulesView {
            rules,
            selected_ids: selected,
            focused_idx: focused,
        };
        Ok(())
    }

    pub fn move_rules_focus(&mut self, delta: i32) {
        let n = self.rules_view.rules.len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.rules_view.focused_idx as i32 + delta) % len + len) % len;
        self.rules_view.focused_idx = next as usize;
    }

    pub fn focused_rule(&self) -> Option<&Rule> {
        self.rules_view.rules.get(self.rules_view.focused_idx)
    }

    pub fn toggle_focused_rule_selection(&mut self) {
        let Some(rule) = self.focused_rule() else {
            return;
        };
        let id = rule.id.clone();
        if !self.rules_view.selected_ids.insert(id.clone()) {
            self.rules_view.selected_ids.remove(&id);
        }
    }

    pub fn open_agent_popup_for_focused_rule(&mut self) {
        let Some(rule) = self.focused_rule() else {
            return;
        };
        let mut enabled = BTreeMap::new();
        for agent in ALL_AGENT_KINDS {
            enabled.insert(*agent, *rule.enabled.get(agent).unwrap_or(&false));
        }
        self.agent_popup = Some(AgentPopup {
            target: AgentPopupTarget::Rule,
            row_id: rule.id.clone(),
            row_name: rule.name.clone(),
            cursor: 0,
            enabled,
        });
    }

    pub fn sync_all_rules(&mut self) {
        match rule_service::sync_all(&self.db, &self.settings) {
            Ok(()) => self.push_toast("Synced rules to all agents", ToastLevel::Success),
            Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn delete_focused_rule(&mut self) {
        let Some(rule) = self.focused_rule() else {
            return;
        };
        let id = rule.id.clone();
        let name = rule.name.clone();
        match rule_service::delete(&self.db, &id) {
            Ok(()) => {
                let _ = self.reload_rules();
                let _ = rule_service::sync_all(&self.db, &self.settings);
                self.push_toast(format!("Deleted rule {name}"), ToastLevel::Success);
            }
            Err(e) => self.push_toast(format!("Delete failed: {e}"), ToastLevel::Error),
        }
    }

    // ---------- Backup tab actions ----------

    pub fn reload_backups(&mut self) {
        let snapshots = backup_service::list_snapshots().unwrap_or_default();
        let max_idx = snapshots.len().saturating_sub(1);
        let focused = self.backup_view.focused_idx.min(max_idx);
        self.backup_view = BackupView {
            snapshots,
            focused_idx: focused,
        };
    }

    pub fn move_backup_focus(&mut self, delta: i32) {
        let n = self.backup_view.snapshots.len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.backup_view.focused_idx as i32 + delta) % len + len) % len;
        self.backup_view.focused_idx = next as usize;
    }

    pub fn create_snapshot(&mut self) {
        match backup_service::create_snapshot() {
            Ok(snap) => {
                self.reload_backups();
                self.push_toast(
                    format!("Snapshot created: {}", snap.timestamp),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Snapshot failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn restore_focused_snapshot(&mut self) {
        let Some(snap) = self.backup_view.snapshots.get(self.backup_view.focused_idx) else {
            self.push_toast("No snapshot selected", ToastLevel::Warning);
            return;
        };
        let ts = snap.timestamp.clone();
        match backup_service::restore_snapshot(&ts) {
            Ok(()) => self.push_toast(format!("Restored {ts}"), ToastLevel::Success),
            Err(e) => self.push_toast(format!("Restore failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn push_github_backup(&mut self) {
        match backup_service::push_to_github(&self.settings.github_backup) {
            Ok(oid) => self.push_toast(
                format!("Pushed backup commit {}", &oid[..oid.len().min(8)]),
                ToastLevel::Success,
            ),
            Err(e) => self.push_toast(format!("Push failed: {e}"), ToastLevel::Error),
        }
    }

    // ---------- Theme actions ----------

    fn build_theme_list() -> Vec<String> {
        let mut names: Vec<String> = Theme::all_builtin_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        // Overlay user themes: if same slug exists, it takes the same slot; new
        // slugs are appended and sorted at the end.
        if let Ok(dir) = liteconfig_dir() {
            let user_dir = dir.join("themes");
            for (slug, _) in Theme::load_user_themes(&user_dir) {
                if !names.contains(&slug) {
                    names.push(slug);
                }
            }
        }
        names
    }

    /// Advance to the next theme in `available_themes`, apply it live, and
    /// persist the change to `settings.json`.
    pub fn cycle_theme(&mut self) {
        if self.available_themes.is_empty() {
            return;
        }
        let current = &self.settings.theme;
        let idx = self
            .available_themes
            .iter()
            .position(|s| s == current)
            .unwrap_or(0);
        let next_idx = (idx + 1) % self.available_themes.len();
        let next = self.available_themes[next_idx].clone();

        // Load from user dir first (same slug overrides builtin).
        let loaded = if let Ok(dir) = liteconfig_dir() {
            let user_dir = dir.join("themes");
            Theme::load_user_themes(&user_dir)
                .into_iter()
                .find(|(s, _)| s == &next)
                .map(|(_, t)| t)
        } else {
            None
        };
        let loaded = loaded.unwrap_or_else(|| Theme::by_name(&next));

        self.theme = loaded;
        self.settings.theme = next.clone();
        if let Err(e) = self.settings.save() {
            self.push_toast(format!("Could not save theme: {e}"), ToastLevel::Warning);
        } else {
            self.push_toast(format!("Theme: {next}"), ToastLevel::Info);
        }
    }

    pub fn current_profile_name(&self, agent: AgentKind) -> Option<&str> {
        let id = self.settings.current_profile_for(agent)?;
        self.profile_views
            .get(&agent)?
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.as_str())
    }

    /// Label for the status bar: which adapter file is the live config.
    pub fn live_config_hint(&self) -> String {
        let agent = self.focused_agent();
        let adapter = agent_for_kind(agent).ok();
        let path = adapter
            .and_then(|a| a.paths(&self.settings).ok())
            .map(|p| p.live_settings.display().to_string())
            .unwrap_or_else(|| "?".into());
        format!("{}: {path}", agent.display_name())
    }
}
