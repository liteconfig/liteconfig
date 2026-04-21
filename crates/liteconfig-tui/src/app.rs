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
use liteconfig_core::model::skill::{Skill, SyncMethod};
use liteconfig_core::paths::liteconfig_dir;
use liteconfig_core::services::backup_service;
use liteconfig_core::services::backup_service::Snapshot;
use liteconfig_core::services::mcp_service;
use liteconfig_core::services::profile_service;
use liteconfig_core::services::rule_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::services::skill_service;
use liteconfig_core::settings::Settings;

use crate::tasks::{TaskLogEntry, TaskRunner, TaskStatus};
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
    /// Case-insensitive substring match applied to `name` and `description`.
    /// Empty string = show all.
    pub filter: String,
    /// True while the user is typing into the filter input.
    pub filter_editing: bool,
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

/// Fixed display order for the per-skill sync-method picker. The popup cursor
/// indexes into this; `SyncMethod::cycle` uses the same order.
pub const METHOD_POPUP_CHOICES: [SyncMethod; 4] = [
    SyncMethod::Auto,
    SyncMethod::Symlink,
    SyncMethod::Copy,
    SyncMethod::Inherit,
];

/// Modal state for the per-skill sync-method picker. `current` is the
/// committed method on the skill when the popup opened; `cursor` is the
/// highlighted row in the picker, which becomes `current` on Enter.
#[derive(Debug, Clone)]
pub struct MethodPopup {
    pub row_id: String,
    pub row_name: String,
    pub cursor: usize,
    pub current: SyncMethod,
}

/// View-level state for the MCP tab.
#[derive(Debug, Clone, Default)]
pub struct McpView {
    pub servers: Vec<McpServer>,
    pub selected_ids: HashSet<String>,
    pub focused_idx: usize,
    pub filter: String,
    pub filter_editing: bool,
}

/// View-level state for the Backup tab.
#[derive(Debug, Clone, Default)]
pub struct BackupView {
    pub snapshots: Vec<Snapshot>,
    pub focused_idx: usize,
}

/// Focusable rows in the Settings tab. Order matches the render order —
/// `move_settings_focus` walks this enum linearly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    GhEnabled,
    GhRepoUrl,
    GhBranch,
    GhAutoSync,
}

impl SettingsRow {
    pub const ALL: &'static [SettingsRow] = &[
        SettingsRow::GhEnabled,
        SettingsRow::GhRepoUrl,
        SettingsRow::GhBranch,
        SettingsRow::GhAutoSync,
    ];

    pub fn is_text(self) -> bool {
        matches!(self, SettingsRow::GhRepoUrl | SettingsRow::GhBranch)
    }

    pub fn label(self) -> &'static str {
        match self {
            SettingsRow::GhEnabled => "Enabled",
            SettingsRow::GhRepoUrl => "Repo",
            SettingsRow::GhBranch => "Branch",
            SettingsRow::GhAutoSync => "Auto-sync",
        }
    }
}

/// Settings tab state: which row the user has focused and, if editing a
/// text row, the working copy of the value.
#[derive(Debug, Clone)]
pub struct SettingsView {
    pub focused_row: Option<SettingsRow>,
    /// `Some` while the user is editing a text row (Enter → commit, Esc → discard).
    pub input_buf: Option<String>,
}

impl Default for SettingsView {
    fn default() -> Self {
        // Highlight the first editable row on startup so ↑/↓ has an obvious
        // starting point and the user doesn't have to "acquire focus" first.
        Self {
            focused_row: Some(SettingsRow::GhEnabled),
            input_buf: None,
        }
    }
}

/// View-level state for the Rules tab.
#[derive(Debug, Clone, Default)]
pub struct RulesView {
    pub rules: Vec<Rule>,
    pub selected_ids: HashSet<String>,
    pub focused_idx: usize,
    pub filter: String,
    pub filter_editing: bool,
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
    pub settings_view: SettingsView,
    /// When `Some`, the active tab yields input to drive the popup.
    pub agent_popup: Option<AgentPopup>,
    /// When `Some`, the Skills tab yields input to the sync-method picker.
    pub method_popup: Option<MethodPopup>,

    /// Ordered list of available theme slugs (builtin + user). Populated once
    /// on startup; used to cycle themes in the Settings tab.
    pub available_themes: Vec<String>,

    pub toasts: Vec<Toast>,

    /// Background worker for long operations (sync-all, GitHub push, …).
    /// Drained each `tick` so finished jobs surface as toasts and the
    /// affected view reloads.
    pub tasks: TaskRunner,
    /// Toggles the Activity overlay (bound to `L`). The log is always live
    /// on `tasks` regardless of this flag — this just controls visibility.
    pub show_activity: bool,
    /// Toggles the per-tab Help overlay (bound to `?`).
    pub show_help: bool,
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
            settings_view: SettingsView::default(),
            agent_popup: None,
            method_popup: None,
            available_themes,
            toasts: Vec::new(),
            tasks: TaskRunner::new(),
            show_activity: false,
            show_help: false,
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
        self.drain_completed_tasks();
    }

    /// Move any just-finished background tasks out of the runner: push a
    /// toast for each, and reload whichever view the task affected. The
    /// task's `name` is the routing key — keep the names stable.
    fn drain_completed_tasks(&mut self) {
        let completed = self.tasks.drain_completed();
        for entry in completed {
            let (level, msg) = match &entry.status {
                TaskStatus::Ok(s) if s.is_empty() => (ToastLevel::Success, entry.name.clone()),
                TaskStatus::Ok(s) => (ToastLevel::Success, format!("{}: {}", entry.name, s)),
                TaskStatus::Err(e) => (ToastLevel::Error, format!("{} failed: {}", entry.name, e)),
                TaskStatus::Running => continue, // drain_completed only returns finished
            };
            self.push_toast(msg, level);
            self.post_task_reload(&entry);
        }
    }

    fn post_task_reload(&mut self, entry: &TaskLogEntry) {
        match entry.name.as_str() {
            "Sync all skills" => {
                let _ = self.reload_skills();
            }
            "Sync all MCP" => {
                let _ = self.reload_mcp();
            }
            "Sync all rules" => {
                let _ = self.reload_rules();
            }
            "Push backup" => self.reload_backups(),
            _ => {}
        }
    }

    pub fn toggle_activity(&mut self) {
        self.show_activity = !self.show_activity;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
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
        let filter = std::mem::take(&mut self.skills_view.filter);
        let filter_editing = self.skills_view.filter_editing;
        self.skills_view = SkillsView {
            skills,
            selected_ids: selected,
            focused_idx: focused,
            filter,
            filter_editing,
        };
        Ok(())
    }

    pub fn move_skill_focus(&mut self, delta: i32) {
        let n = self.filtered_skill_indices().len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.skills_view.focused_idx as i32 + delta) % len + len) % len;
        self.skills_view.focused_idx = next as usize;
    }

    /// Indices into `skills_view.skills` that match the current filter.
    /// Empty filter → all indices.
    pub fn filtered_skill_indices(&self) -> Vec<usize> {
        let needle = self.skills_view.filter.trim().to_lowercase();
        if needle.is_empty() {
            return (0..self.skills_view.skills.len()).collect();
        }
        self.skills_view
            .skills
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.name.to_lowercase().contains(&needle)
                    || s.description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&needle))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn focused_skill(&self) -> Option<&Skill> {
        let idx = *self
            .filtered_skill_indices()
            .get(self.skills_view.focused_idx)?;
        self.skills_view.skills.get(idx)
    }

    pub fn skills_filter_push(&mut self, c: char) {
        self.skills_view.filter.push(c);
        self.skills_view.focused_idx = 0;
    }

    pub fn skills_filter_pop(&mut self) {
        self.skills_view.filter.pop();
        self.skills_view.focused_idx = 0;
    }

    pub fn skills_filter_open(&mut self) {
        self.skills_view.filter_editing = true;
    }

    pub fn skills_filter_close_keep(&mut self) {
        self.skills_view.filter_editing = false;
    }

    pub fn skills_filter_clear(&mut self) {
        self.skills_view.filter.clear();
        self.skills_view.filter_editing = false;
        self.skills_view.focused_idx = 0;
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

    pub fn open_method_popup_for_focused(&mut self) {
        let Some(skill) = self.focused_skill() else {
            return;
        };
        let current = skill.sync_method;
        let cursor = METHOD_POPUP_CHOICES
            .iter()
            .position(|m| *m == current)
            .unwrap_or(0);
        self.method_popup = Some(MethodPopup {
            row_id: skill.id.clone(),
            row_name: skill.name.clone(),
            cursor,
            current,
        });
    }

    pub fn method_popup_move(&mut self, delta: i32) {
        let Some(p) = self.method_popup.as_mut() else {
            return;
        };
        let n = METHOD_POPUP_CHOICES.len() as i32;
        p.cursor = (((p.cursor as i32 + delta) % n + n) % n) as usize;
    }

    pub fn method_popup_cancel(&mut self) {
        self.method_popup = None;
    }

    pub fn method_popup_commit(&mut self) {
        let Some(p) = self.method_popup.take() else {
            return;
        };
        let Some(method) = METHOD_POPUP_CHOICES.get(p.cursor).copied() else {
            return;
        };
        match skill_service::set_sync_method(&self.db, &p.row_id, method) {
            Ok(_) => {
                let _ = self.reload_skills();
                if let Err(e) = skill_service::sync_one(&self.db, &self.settings, &p.row_id) {
                    self.push_toast(format!("Resync failed: {e}"), ToastLevel::Error);
                    return;
                }
                self.push_toast(
                    format!("{} → {}", p.row_name, method.as_str()),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Change failed: {e}"), ToastLevel::Error),
        }
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
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let settings = self.settings.clone();
            self.tasks.submit("Sync all skills", move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                skill_service::sync_all(&db, &settings).map_err(|e| e.to_string())?;
                Ok(format!("{count} skill(s)"))
            });
            self.push_toast(
                format!("Syncing {count} skills in background…"),
                ToastLevel::Info,
            );
        } else {
            // In-memory DB: no path to reopen from. Run inline.
            match skill_service::sync_all(&self.db, &self.settings) {
                Ok(()) => {
                    self.push_toast(format!("Synced all {count} skills"), ToastLevel::Success)
                }
                Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
            }
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
        let filter = std::mem::take(&mut self.mcp_view.filter);
        let filter_editing = self.mcp_view.filter_editing;
        self.mcp_view = McpView {
            servers,
            selected_ids: selected,
            focused_idx: focused,
            filter,
            filter_editing,
        };
        Ok(())
    }

    pub fn move_mcp_focus(&mut self, delta: i32) {
        let n = self.filtered_mcp_indices().len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.mcp_view.focused_idx as i32 + delta) % len + len) % len;
        self.mcp_view.focused_idx = next as usize;
    }

    /// Indices into `mcp_view.servers` matching the case-insensitive filter on
    /// `name` and command text. Empty filter → all indices.
    pub fn filtered_mcp_indices(&self) -> Vec<usize> {
        let needle = self.mcp_view.filter.trim().to_lowercase();
        if needle.is_empty() {
            return (0..self.mcp_view.servers.len()).collect();
        }
        self.mcp_view
            .servers
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                if s.name.to_lowercase().contains(&needle) {
                    return true;
                }
                let cmd = s
                    .config
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                cmd.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn focused_mcp(&self) -> Option<&McpServer> {
        let idx = *self.filtered_mcp_indices().get(self.mcp_view.focused_idx)?;
        self.mcp_view.servers.get(idx)
    }

    pub fn mcp_filter_open(&mut self) {
        self.mcp_view.filter_editing = true;
    }

    pub fn mcp_filter_close_keep(&mut self) {
        self.mcp_view.filter_editing = false;
    }

    pub fn mcp_filter_clear(&mut self) {
        self.mcp_view.filter.clear();
        self.mcp_view.filter_editing = false;
        self.mcp_view.focused_idx = 0;
    }

    pub fn mcp_filter_push(&mut self, c: char) {
        self.mcp_view.filter.push(c);
        self.mcp_view.focused_idx = 0;
    }

    pub fn mcp_filter_pop(&mut self) {
        self.mcp_view.filter.pop();
        self.mcp_view.focused_idx = 0;
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
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let settings = self.settings.clone();
            self.tasks.submit("Sync all MCP", move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                mcp_service::sync_all(&db, &settings).map_err(|e| e.to_string())?;
                Ok(String::new())
            });
            self.push_toast("Syncing MCP servers in background…", ToastLevel::Info);
        } else {
            match mcp_service::sync_all(&self.db, &self.settings) {
                Ok(()) => self.push_toast("Synced MCP to all agents", ToastLevel::Success),
                Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
            }
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
        let filter = std::mem::take(&mut self.rules_view.filter);
        let filter_editing = self.rules_view.filter_editing;
        self.rules_view = RulesView {
            rules,
            selected_ids: selected,
            focused_idx: focused,
            filter,
            filter_editing,
        };
        Ok(())
    }

    pub fn move_rules_focus(&mut self, delta: i32) {
        let n = self.filtered_rules_indices().len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.rules_view.focused_idx as i32 + delta) % len + len) % len;
        self.rules_view.focused_idx = next as usize;
    }

    /// Indices into `rules_view.rules` matching the case-insensitive filter on
    /// `name` and `body`. Empty filter → all indices.
    pub fn filtered_rules_indices(&self) -> Vec<usize> {
        let needle = self.rules_view.filter.trim().to_lowercase();
        if needle.is_empty() {
            return (0..self.rules_view.rules.len()).collect();
        }
        self.rules_view
            .rules
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.name.to_lowercase().contains(&needle) || r.body.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn focused_rule(&self) -> Option<&Rule> {
        let idx = *self
            .filtered_rules_indices()
            .get(self.rules_view.focused_idx)?;
        self.rules_view.rules.get(idx)
    }

    pub fn rules_filter_open(&mut self) {
        self.rules_view.filter_editing = true;
    }

    pub fn rules_filter_close_keep(&mut self) {
        self.rules_view.filter_editing = false;
    }

    pub fn rules_filter_clear(&mut self) {
        self.rules_view.filter.clear();
        self.rules_view.filter_editing = false;
        self.rules_view.focused_idx = 0;
    }

    pub fn rules_filter_push(&mut self, c: char) {
        self.rules_view.filter.push(c);
        self.rules_view.focused_idx = 0;
    }

    pub fn rules_filter_pop(&mut self) {
        self.rules_view.filter.pop();
        self.rules_view.focused_idx = 0;
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
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let settings = self.settings.clone();
            self.tasks.submit("Sync all rules", move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                rule_service::sync_all(&db, &settings).map_err(|e| e.to_string())?;
                Ok(String::new())
            });
            self.push_toast("Syncing rules in background…", ToastLevel::Info);
        } else {
            match rule_service::sync_all(&self.db, &self.settings) {
                Ok(()) => self.push_toast("Synced rules to all agents", ToastLevel::Success),
                Err(e) => self.push_toast(format!("Sync failed: {e}"), ToastLevel::Error),
            }
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
        let gh = self.settings.github_backup.clone();
        if !gh.enabled || gh.repo_url.is_empty() {
            self.push_toast(
                "GitHub backup is disabled — enable it and set a repo URL in Settings",
                ToastLevel::Warning,
            );
            return;
        }
        self.tasks.submit("Push backup", move || {
            let oid = backup_service::push_to_github(&gh).map_err(|e| e.to_string())?;
            Ok(format!("commit {}", &oid[..oid.len().min(8)]))
        });
        self.push_toast("Pushing backup in background…", ToastLevel::Info);
    }

    // ---------- Settings tab actions ----------

    pub fn move_settings_focus(&mut self, delta: i32) {
        let rows = SettingsRow::ALL;
        let cur = self
            .settings_view
            .focused_row
            .and_then(|r| rows.iter().position(|x| *x == r))
            .unwrap_or(0) as i32;
        let n = rows.len() as i32;
        let next = ((cur + delta) % n + n) % n;
        self.settings_view.focused_row = Some(rows[next as usize]);
        // Any focus move cancels an in-progress text edit.
        self.settings_view.input_buf = None;
    }

    /// Toggle the focused boolean row (Enabled / Auto-sync). Text rows do
    /// nothing here — they use `settings_begin_edit`.
    pub fn settings_toggle_focused(&mut self) {
        let Some(row) = self.settings_view.focused_row else {
            return;
        };
        let flipped = match row {
            SettingsRow::GhEnabled => {
                self.settings.github_backup.enabled = !self.settings.github_backup.enabled;
                Some(("GitHub backup", self.settings.github_backup.enabled))
            }
            SettingsRow::GhAutoSync => {
                self.settings.github_backup.auto_sync = !self.settings.github_backup.auto_sync;
                Some(("Auto-sync", self.settings.github_backup.auto_sync))
            }
            _ => None,
        };
        let Some((label, new_val)) = flipped else {
            return;
        };
        if let Err(e) = self.settings.save() {
            self.push_toast(format!("Save failed: {e}"), ToastLevel::Error);
            return;
        }
        self.push_toast(
            format!("{label}: {}", if new_val { "on" } else { "off" }),
            ToastLevel::Success,
        );
    }

    /// Open the inline text input for the focused text row. No-op on bool rows.
    pub fn settings_begin_edit(&mut self) {
        let Some(row) = self.settings_view.focused_row else {
            return;
        };
        if !row.is_text() {
            return;
        }
        let current = match row {
            SettingsRow::GhRepoUrl => self.settings.github_backup.repo_url.clone(),
            SettingsRow::GhBranch => self.settings.github_backup.branch.clone(),
            _ => return,
        };
        self.settings_view.input_buf = Some(current);
    }

    pub fn settings_input_push(&mut self, c: char) {
        if let Some(buf) = self.settings_view.input_buf.as_mut() {
            buf.push(c);
        }
    }

    pub fn settings_input_pop(&mut self) {
        if let Some(buf) = self.settings_view.input_buf.as_mut() {
            buf.pop();
        }
    }

    pub fn settings_input_cancel(&mut self) {
        self.settings_view.input_buf = None;
    }

    pub fn settings_input_commit(&mut self) {
        let Some(buf) = self.settings_view.input_buf.take() else {
            return;
        };
        let Some(row) = self.settings_view.focused_row else {
            return;
        };
        let (field, new_val) = match row {
            SettingsRow::GhRepoUrl => {
                self.settings.github_backup.repo_url = buf.clone();
                ("Repo URL", buf)
            }
            SettingsRow::GhBranch => {
                // Blank branch falls back to "main" so the push path never
                // hits an empty refspec.
                let val = if buf.trim().is_empty() {
                    "main".to_string()
                } else {
                    buf
                };
                self.settings.github_backup.branch = val.clone();
                ("Branch", val)
            }
            _ => return,
        };
        if let Err(e) = self.settings.save() {
            self.push_toast(format!("Save failed: {e}"), ToastLevel::Error);
            return;
        }
        self.push_toast(format!("{field}: {new_val}"), ToastLevel::Success);
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
