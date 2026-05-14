//! Central app state. Every user action is funneled into `App` via `handle_*`
//! methods; the UI layer only reads state.

use std::collections::{BTreeMap, HashSet};

use color_eyre::eyre::Result;
use liteconfig_core::agents::for_kind as agent_for_kind;
use liteconfig_core::db::Database;
use liteconfig_core::model::agent::{AgentKind, ALL_AGENT_KINDS};
use liteconfig_core::model::mcp::McpServer;
use liteconfig_core::model::plugin::PluginSource;
use liteconfig_core::model::profile::Profile;
use liteconfig_core::model::rule::Rule;
use liteconfig_core::model::skill::{Skill, SyncMethod};
use liteconfig_core::paths::liteconfig_dir;
use liteconfig_core::presets::{MCP_PRESETS, SKILL_REPO_PRESETS};
use liteconfig_core::services::backup_service;
use liteconfig_core::services::backup_service::Snapshot;
use liteconfig_core::services::mcp_index_service::{self, ExternalMcp};
use liteconfig_core::services::mcp_service;
use liteconfig_core::services::plugin_service;
use liteconfig_core::services::profile_service;
use liteconfig_core::services::rule_service;
use liteconfig_core::services::secrets_service::SecretStore;
use liteconfig_core::services::skill_cli_service::{self, CommandStream, InstallMethod, RunStatus};
use liteconfig_core::services::skill_index_service::{self, ExternalSkill};
use liteconfig_core::services::skill_repo_service;
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
    Plugins,
    Backup,
    #[allow(dead_code)]
    Sessions,
    Settings,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[
        Tab::Profiles,
        Tab::Skills,
        Tab::Mcp,
        Tab::Rules,
        Tab::Plugins,
        Tab::Backup,
        Tab::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Profiles => "Profiles",
            Tab::Skills => "Skills",
            Tab::Mcp => "MCP",
            Tab::Rules => "Rules",
            Tab::Plugins => "Plugins",
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

/// Preset chooser popup. Either offers curated skill-repo URLs or
/// curated MCP servers — the user picks one with ↑/↓ and installs with
/// Enter, or Esc closes without side effects.
#[derive(Debug, Clone)]
pub struct PresetsPopup {
    pub kind: PresetsKind,
    pub cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetsKind {
    SkillRepo,
    Mcp,
    Plugin,
}

/// Curated Claude Code plugin repo. Lives here (not liteconfig-core) because
/// the catalog is TUI-facing and may reshuffle with UX iterations.
#[derive(Debug, Clone, Copy)]
pub struct PluginPreset {
    pub owner: &'static str,
    pub name: &'static str,
    pub branch: &'static str,
    pub description: &'static str,
}

pub const PLUGIN_PRESETS: &[PluginPreset] = &[
    PluginPreset {
        owner: "anthropic-experimental",
        name: "cc-essentials",
        branch: "main",
        description: "Official starter plugin bundle (skills + commands)",
    },
    PluginPreset {
        owner: "davila7",
        name: "claude-code-templates",
        branch: "main",
        description: "Language-specific workflow templates",
    },
    PluginPreset {
        owner: "sumeetdas",
        name: "claude-code-power-pack",
        branch: "main",
        description: "Productivity commands + subagents",
    },
    PluginPreset {
        owner: "jondot",
        name: "awesome-claude-code",
        branch: "main",
        description: "Community-curated plugin bundle",
    },
    PluginPreset {
        owner: "vivekvells",
        name: "claude-dev-toolkit",
        branch: "main",
        description: "Dev workflow helpers + subagents",
    },
];

impl PresetsPopup {
    pub fn len(&self) -> usize {
        match self.kind {
            PresetsKind::SkillRepo => SKILL_REPO_PRESETS.len(),
            PresetsKind::Mcp => MCP_PRESETS.len(),
            PresetsKind::Plugin => PLUGIN_PRESETS.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Keyboard focus within the skills.sh search popup. Tab cycles between the
/// two zones; Enter's effect depends on which one owns focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchFocus {
    Query,
    Results,
}

/// Current state of the skills.sh search. Drives both the result list and
/// the status line inside the popup.
#[derive(Debug, Clone)]
pub enum SearchStatus {
    Idle,
    Loading,
    Error(String),
    Loaded,
}

/// Worker thread hands the final result through here; UI thread drains on
/// each tick. `None` = still in flight, `Some(Ok/Err)` = done.
pub type SearchInbox = std::sync::Arc<std::sync::Mutex<Option<Result<Vec<ExternalSkill>, String>>>>;

/// Live skills.sh search popup. The HTTP call runs on a worker thread that
/// drops the result into `inbox`; the UI thread drains that on each tick.
#[derive(Clone)]
pub struct SearchSkillsPopup {
    pub query: String,
    pub results: Vec<ExternalSkill>,
    pub cursor: usize,
    pub status: SearchStatus,
    pub focus: SearchFocus,
    pub inbox: SearchInbox,
}

impl std::fmt::Debug for SearchSkillsPopup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchSkillsPopup")
            .field("query", &self.query)
            .field("results_len", &self.results.len())
            .field("cursor", &self.cursor)
            .field("status", &self.status)
            .field("focus", &self.focus)
            .finish()
    }
}

/// Popup that tails installer output. Wraps `CommandStream` so the same
/// popup can drive either `pnpx skills add <owner/repo>` or a nested
/// pnpm-bootstrap step.
#[derive(Clone)]
pub struct InstallLogPopup {
    pub mode: InstallLogMode,
    /// `owner/repo` the user wanted to install. Carried through so when the
    /// pnpm-installer finishes we can automatically kick off the skill
    /// install.
    pub pending_skill: Option<String>,
}

#[derive(Clone)]
pub enum InstallLogMode {
    /// We need pnpm but don't have it — prompt for confirmation.
    ConfirmPnpm { owner_repo: String },
    /// Child process is streaming or just finished. Status lives on the
    /// `CommandStream` itself.
    Streaming(CommandStream),
}

impl std::fmt::Debug for InstallLogPopup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstallLogPopup")
            .field(
                "mode",
                match &self.mode {
                    InstallLogMode::ConfirmPnpm { owner_repo } => owner_repo,
                    InstallLogMode::Streaming(s) => &s.title,
                },
            )
            .field("pending_skill", &self.pending_skill)
            .finish()
    }
}

/// Inbox for the MCP live-search popup — same pattern as `SearchInbox` but
/// carrying `ExternalMcp` results from Smithery.
pub type McpSearchInbox =
    std::sync::Arc<std::sync::Mutex<Option<Result<Vec<ExternalMcp>, String>>>>;

/// Live Smithery MCP search popup. Mirrors `SearchSkillsPopup`.
#[derive(Clone)]
pub struct SearchMcpPopup {
    pub query: String,
    pub results: Vec<ExternalMcp>,
    pub cursor: usize,
    pub status: SearchStatus,
    pub focus: SearchFocus,
    pub inbox: McpSearchInbox,
}

impl std::fmt::Debug for SearchMcpPopup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SearchMcpPopup")
            .field("query", &self.query)
            .field("results_len", &self.results.len())
            .field("cursor", &self.cursor)
            .field("status", &self.status)
            .field("focus", &self.focus)
            .finish()
    }
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

/// View-level state for the Plugins tab.
#[derive(Debug, Clone, Default)]
pub struct PluginsView {
    pub plugins: Vec<liteconfig_core::model::plugin::Plugin>,
    pub focused_idx: usize,
    /// Two-phase delete, same pattern as Backup's snapshot delete.
    pub delete_armed_at: Option<std::time::Instant>,
}

/// View-level state for the Backup tab.
#[derive(Debug, Clone, Default)]
pub struct BackupView {
    pub snapshots: Vec<Snapshot>,
    pub focused_idx: usize,
    /// Two-phase delete: first `d` stamps this, second `d` within
    /// [`DELETE_CONFIRM_WINDOW`] runs the actual delete. Cleared on any
    /// non-`d` key and on successful delete.
    pub delete_armed_at: Option<std::time::Instant>,
}

/// How long a pending delete stays armed after the first `d`.
pub const DELETE_CONFIRM_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

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
    pub plugins_view: PluginsView,
    pub settings_view: SettingsView,
    /// When `Some`, the active tab yields input to drive the popup.
    pub agent_popup: Option<AgentPopup>,
    /// When `Some`, the Skills tab yields input to the sync-method picker.
    pub method_popup: Option<MethodPopup>,
    /// When `Some`, a presets chooser (skill-repo or MCP) is open.
    pub presets_popup: Option<PresetsPopup>,
    /// When `Some`, the skills.sh search popup is open.
    pub search_popup: Option<SearchSkillsPopup>,
    /// When `Some`, the MCP Smithery live-search popup is open.
    pub mcp_search_popup: Option<SearchMcpPopup>,
    /// When `Some`, the pnpx installer log (or pre-install confirm) is up.
    pub install_log_popup: Option<InstallLogPopup>,

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
    /// Monotonic tick counter used to drive UI animation (spinner).
    /// Wraps at 255 — callers should modulo by the frame count of their
    /// animation.
    pub tick_idx: u8,
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
            plugins_view: PluginsView::default(),
            settings_view: SettingsView::default(),
            agent_popup: None,
            method_popup: None,
            presets_popup: None,
            search_popup: None,
            mcp_search_popup: None,
            install_log_popup: None,
            available_themes,
            toasts: Vec::new(),
            tasks: TaskRunner::new(),
            show_activity: false,
            show_help: false,
            tick_idx: 0,
        };
        app.reload_profiles()?;
        app.reload_skills()?;
        app.reload_mcp()?;
        app.reload_rules()?;
        app.reload_backups();
        app.reload_plugins();
        app.auto_import_if_empty();
        app.rescan_live_skills_async();
        app.recompute_skill_hashes_async();
        Ok(app)
    }

    // ---------- pnpx install-log popup ----------

    /// Entry point for the Skills-tab `p` shortcut. If pnpm is on PATH,
    /// spawn `pnpx skills add <owner/repo>` and open the tailing popup.
    /// Otherwise open the confirm-pnpm prompt first.
    pub fn install_skill_via_pnpx(&mut self, owner_repo: &str) {
        match skill_cli_service::detect() {
            InstallMethod::None => {
                self.install_log_popup = Some(InstallLogPopup {
                    mode: InstallLogMode::ConfirmPnpm {
                        owner_repo: owner_repo.to_string(),
                    },
                    pending_skill: Some(owner_repo.to_string()),
                });
            }
            InstallMethod::NodeOnly => {
                // Node but no package manager — treat like missing so the
                // confirm prompt still offers to install pnpm.
                self.install_log_popup = Some(InstallLogPopup {
                    mode: InstallLogMode::ConfirmPnpm {
                        owner_repo: owner_repo.to_string(),
                    },
                    pending_skill: Some(owner_repo.to_string()),
                });
            }
            InstallMethod::Pnpm | InstallMethod::Npm => {
                let stream = skill_cli_service::install_via_pnpx(owner_repo);
                self.install_log_popup = Some(InstallLogPopup {
                    mode: InstallLogMode::Streaming(stream),
                    pending_skill: Some(owner_repo.to_string()),
                });
            }
        }
    }

    /// Confirm popup → yes: start the pnpm installer, stay in popup.
    pub fn install_log_confirm_pnpm(&mut self) {
        let Some(popup) = self.install_log_popup.as_mut() else {
            return;
        };
        let InstallLogMode::ConfirmPnpm { .. } = &popup.mode else {
            return;
        };
        let stream = skill_cli_service::install_pnpm_via_curl();
        popup.mode = InstallLogMode::Streaming(stream);
    }

    /// Confirm popup → no: fall back to git-clone for the pending skill.
    pub fn install_log_decline_pnpm(&mut self) {
        let Some(popup) = self.install_log_popup.take() else {
            return;
        };
        let Some(owner_repo) = popup.pending_skill else {
            return;
        };
        self.push_toast(
            format!("Falling back to git clone for {owner_repo}"),
            ToastLevel::Info,
        );
        let repo = match skill_repo_service::add(&self.db, &owner_repo, None) {
            Ok(r) => r,
            Err(e) => {
                self.push_toast(format!("Add failed: {e}"), ToastLevel::Error);
                return;
            }
        };
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let repo_id = repo.id.clone();
            self.tasks.submit(format!("Clone {owner_repo}"), move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                let r = skill_repo_service::sync(&db, &repo_id).map_err(|e| e.to_string())?;
                Ok(format!("{} skill(s)", r.skill_count))
            });
        } else {
            let _ = skill_repo_service::sync(&self.db, &repo.id);
            let _ = self.reload_skills();
        }
    }

    /// User pressed Enter/Esc on the streaming popup. If the pnpm installer
    /// just finished successfully and a skill is pending, auto-chain into
    /// `pnpx skills add <pending>`. Otherwise close + trigger a rescan so
    /// freshly installed files land in the Skills tab.
    pub fn install_log_close(&mut self) {
        let Some(popup) = self.install_log_popup.take() else {
            return;
        };
        let was_ok = matches!(
            &popup.mode,
            InstallLogMode::Streaming(s) if matches!(s.status(), RunStatus::Ok)
        );
        // If pnpm just landed and we have a pending skill, kick off the
        // actual skill install now.
        if was_ok {
            if let (Some(pending), true) = (
                popup.pending_skill.as_ref(),
                matches!(
                    &popup.mode,
                    InstallLogMode::Streaming(s) if s.title == "install pnpm"
                ),
            ) {
                let owner_repo = pending.clone();
                self.install_skill_via_pnpx(&owner_repo);
                return;
            }
            self.rescan_live_skills_async();
        }
    }

    pub fn recompute_skill_hashes_async(&mut self) {
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            self.tasks.submit("Recompute skill hashes", move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                let n = skill_service::recompute_missing_hashes(&db).map_err(|e| e.to_string())?;
                if n == 0 {
                    Ok(String::new())
                } else {
                    Ok(format!("{n} hashed"))
                }
            });
        } else if let Ok(n) = skill_service::recompute_missing_hashes(&self.db) {
            if n > 0 {
                let _ = self.reload_skills();
            }
        }
    }

    /// Every launch, kick off a background scan of the user's live skills
    /// directories. Picks up skills that were installed externally since
    /// the last run (e.g. via `claude` CLI). Silent when nothing new turns
    /// up so it doesn't nag on every cold start.
    pub fn rescan_live_skills_async(&mut self) {
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let settings = self.settings.clone();
            self.tasks.submit("Rescan live skills", move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                let v = skill_service::scan_from_live(&db, &settings).map_err(|e| e.to_string())?;
                if v.is_empty() {
                    Ok(String::new())
                } else {
                    Ok(format!("{} new", v.len()))
                }
            });
        } else {
            // In-memory DB (tests): run inline.
            if let Ok(v) = skill_service::scan_from_live(&self.db, &self.settings) {
                if !v.is_empty() {
                    self.push_toast(
                        format!("Rescan: {} new skill(s)", v.len()),
                        ToastLevel::Info,
                    );
                }
                let _ = self.reload_skills();
            }
        }
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
        self.tick_idx = self.tick_idx.wrapping_add(1);
        self.toasts.retain(|t| t.created_at.elapsed().as_secs() < 5);
        self.drain_completed_tasks();
        self.drain_search_inbox();
        self.drain_mcp_search_inbox();
    }

    /// Move any just-finished background tasks out of the runner: push a
    /// toast for each, and reload whichever view the task affected. The
    /// task's `name` is the routing key — keep the names stable.
    fn drain_completed_tasks(&mut self) {
        let completed = self.tasks.drain_completed();
        for entry in completed {
            self.post_task_reload(&entry);
            if let Some((level, msg)) = Self::toast_for_task(&entry) {
                self.push_toast(msg, level);
            }
        }
    }

    /// Map a completed task to a toast. Returning `None` suppresses the
    /// toast entirely — used for low-value background chores like the
    /// startup rescan when nothing new was found.
    fn toast_for_task(entry: &TaskLogEntry) -> Option<(ToastLevel, String)> {
        match &entry.status {
            TaskStatus::Ok(s) => {
                if (entry.name == "Rescan live skills" || entry.name == "Recompute skill hashes")
                    && s.is_empty()
                {
                    return None;
                }
                if s.is_empty() {
                    Some((ToastLevel::Success, entry.name.clone()))
                } else {
                    Some((ToastLevel::Success, format!("{}: {}", entry.name, s)))
                }
            }
            TaskStatus::Err(e) => {
                Some((ToastLevel::Error, format!("{} failed: {}", entry.name, e)))
            }
            TaskStatus::Running => None,
        }
    }

    fn post_task_reload(&mut self, entry: &TaskLogEntry) {
        match entry.name.as_str() {
            "Sync all skills" | "Rescan live skills" | "Recompute skill hashes" => {
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

    /// Open the curated skill-repo presets chooser.
    pub fn open_new_skill_menu(&mut self) {
        self.presets_popup = Some(PresetsPopup {
            kind: PresetsKind::SkillRepo,
            cursor: 0,
        });
    }

    /// Open the curated MCP-server presets chooser.
    pub fn open_new_mcp_menu(&mut self) {
        self.presets_popup = Some(PresetsPopup {
            kind: PresetsKind::Mcp,
            cursor: 0,
        });
    }

    pub fn presets_popup_move(&mut self, delta: i32) {
        let Some(p) = self.presets_popup.as_mut() else {
            return;
        };
        let n = p.len() as i32;
        if n == 0 {
            return;
        }
        p.cursor = (((p.cursor as i32 + delta) % n + n) % n) as usize;
    }

    pub fn presets_popup_cancel(&mut self) {
        self.presets_popup = None;
    }

    pub fn presets_popup_commit(&mut self) {
        let Some(p) = self.presets_popup.take() else {
            return;
        };
        match p.kind {
            PresetsKind::SkillRepo => self.install_skill_repo_preset(p.cursor),
            PresetsKind::Mcp => self.install_mcp_preset(p.cursor),
            PresetsKind::Plugin => self.install_plugin_preset(p.cursor),
        }
    }

    /// Register a curated skill-repo and kick off its clone+scan in the
    /// background. The registration itself is synchronous (just a DB write);
    /// the heavy network fetch runs through the TaskRunner so the UI stays
    /// responsive for large repos like `anthropics/skills`.
    pub fn install_skill_repo_preset(&mut self, idx: usize) {
        let Some(preset) = SKILL_REPO_PRESETS.get(idx).copied() else {
            return;
        };
        let arg = preset.add_arg();
        let repo = match skill_repo_service::add(&self.db, &arg, Some(preset.branch)) {
            Ok(r) => r,
            Err(e) => {
                self.push_toast(format!("Add failed: {e}"), ToastLevel::Error);
                return;
            }
        };

        let label = preset.display_name();
        let task_name = format!("Clone {label}");
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let repo_id = repo.id.clone();
            self.tasks.submit(task_name, move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                let r = skill_repo_service::sync(&db, &repo_id).map_err(|e| e.to_string())?;
                Ok(format!("{} skill(s)", r.skill_count))
            });
            self.push_toast(
                format!("Added {label} — cloning in background…"),
                ToastLevel::Info,
            );
        } else {
            // In-memory DB: run inline so tests see the repo synced.
            match skill_repo_service::sync(&self.db, &repo.id) {
                Ok(r) => {
                    let _ = self.reload_skills();
                    self.push_toast(
                        format!("Added {label} — {} skill(s)", r.skill_count),
                        ToastLevel::Success,
                    );
                }
                Err(e) => self.push_toast(format!("Clone failed: {e}"), ToastLevel::Error),
            }
        }
    }

    /// Upsert a curated MCP server row with the cross-platform resolved
    /// command/args. Stays disabled for every agent — user flips enablement
    /// per-agent via the `a` popup, matching cc-switch's behaviour.
    pub fn install_mcp_preset(&mut self, idx: usize) {
        let Some(preset) = MCP_PRESETS.get(idx).copied() else {
            return;
        };
        let (command, args) = preset.resolved();
        let now = chrono::Utc::now().timestamp_millis();
        let mut enabled = BTreeMap::new();
        for agent in ALL_AGENT_KINDS {
            enabled.insert(*agent, false);
        }
        let server = McpServer {
            id: uuid::Uuid::new_v4().to_string(),
            name: preset.name.to_string(),
            config: serde_json::json!({
                "command": command,
                "args": args,
                "homepage": preset.homepage,
                "description": preset.description,
            }),
            enabled,
            created_at: now,
            updated_at: now,
        };
        match mcp_service::upsert(&self.db, server) {
            Ok(s) => {
                let _ = self.reload_mcp();
                self.push_toast(
                    format!(
                        "Added MCP preset \"{}\" (disabled — enable per agent via a)",
                        s.name
                    ),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Add failed: {e}"), ToastLevel::Error),
        }
    }

    // ---------- skills.sh search popup ----------

    pub fn open_search_skills(&mut self) {
        self.search_popup = Some(SearchSkillsPopup {
            query: String::new(),
            results: Vec::new(),
            cursor: 0,
            status: SearchStatus::Idle,
            focus: SearchFocus::Query,
            inbox: std::sync::Arc::new(std::sync::Mutex::new(None)),
        });
    }

    pub fn search_popup_cancel(&mut self) {
        self.search_popup = None;
    }

    pub fn search_popup_push(&mut self, c: char) {
        if let Some(p) = self.search_popup.as_mut() {
            p.query.push(c);
            p.focus = SearchFocus::Query;
        }
    }

    pub fn search_popup_pop(&mut self) {
        if let Some(p) = self.search_popup.as_mut() {
            p.query.pop();
            p.focus = SearchFocus::Query;
        }
    }

    pub fn search_popup_toggle_focus(&mut self) {
        if let Some(p) = self.search_popup.as_mut() {
            p.focus = match p.focus {
                SearchFocus::Query => SearchFocus::Results,
                SearchFocus::Results => SearchFocus::Query,
            };
        }
    }

    pub fn search_popup_move(&mut self, delta: i32) {
        let Some(p) = self.search_popup.as_mut() else {
            return;
        };
        let n = p.results.len() as i32;
        if n == 0 {
            return;
        }
        p.cursor = (((p.cursor as i32 + delta) % n + n) % n) as usize;
    }

    /// Enter's meaning depends on focus: typing in the query zone runs the
    /// search; highlighting a hit in the result list installs it.
    pub fn search_popup_enter(&mut self) {
        let Some(p) = self.search_popup.as_ref() else {
            return;
        };
        match p.focus {
            SearchFocus::Query => self.run_skills_search(),
            SearchFocus::Results => self.install_focused_search_result(),
        }
    }

    fn run_skills_search(&mut self) {
        let Some(p) = self.search_popup.as_mut() else {
            return;
        };
        let query = p.query.trim().to_string();
        if query.is_empty() {
            p.status = SearchStatus::Error("type a query first".into());
            return;
        }
        p.status = SearchStatus::Loading;
        p.results.clear();
        p.cursor = 0;
        let inbox = p.inbox.clone();
        std::thread::spawn(move || {
            let result = skill_index_service::search(&query, 20, 0).map_err(|e| e.to_string());
            if let Ok(mut slot) = inbox.lock() {
                *slot = Some(result);
            }
        });
    }

    /// Drains the inbox from any pending skills.sh search completion.
    /// Called once per tick from [`Self::tick`] so results flow in without
    /// the user having to press a key.
    pub fn drain_search_inbox(&mut self) {
        let Some(p) = self.search_popup.as_mut() else {
            return;
        };
        let delivered = { p.inbox.lock().ok().and_then(|mut s| s.take()) };
        let Some(outcome) = delivered else {
            return;
        };
        match outcome {
            Ok(hits) => {
                p.results = hits;
                p.cursor = 0;
                p.status = SearchStatus::Loaded;
                p.focus = SearchFocus::Results;
            }
            Err(e) => {
                p.status = SearchStatus::Error(e);
            }
        }
    }

    fn install_focused_search_result(&mut self) {
        let Some(p) = self.search_popup.as_ref() else {
            return;
        };
        let Some(hit) = p.results.get(p.cursor).cloned() else {
            return;
        };
        self.search_popup = None;
        let arg = hit.add_arg();
        let label = hit.name.clone();
        let branch = hit.repo_branch.clone();
        let repo = match skill_repo_service::add(&self.db, &arg, Some(&branch)) {
            Ok(r) => r,
            Err(e) => {
                self.push_toast(format!("Add failed: {e}"), ToastLevel::Error);
                return;
            }
        };
        if let Some(path) = self.db.path().map(std::path::Path::to_path_buf) {
            let repo_id = repo.id.clone();
            self.tasks.submit(format!("Clone {label}"), move || {
                let db = Database::open(&path).map_err(|e| e.to_string())?;
                let r = skill_repo_service::sync(&db, &repo_id).map_err(|e| e.to_string())?;
                Ok(format!("{} skill(s)", r.skill_count))
            });
            self.push_toast(
                format!("Adding {label} — cloning in background…"),
                ToastLevel::Info,
            );
        } else {
            match skill_repo_service::sync(&self.db, &repo.id) {
                Ok(r) => {
                    let _ = self.reload_skills();
                    self.push_toast(
                        format!("Added {label} — {} skill(s)", r.skill_count),
                        ToastLevel::Success,
                    );
                }
                Err(e) => self.push_toast(format!("Clone failed: {e}"), ToastLevel::Error),
            }
        }
    }

    // ---------- Smithery MCP search popup ----------

    pub fn open_search_mcp(&mut self) {
        self.mcp_search_popup = Some(SearchMcpPopup {
            query: String::new(),
            results: Vec::new(),
            cursor: 0,
            status: SearchStatus::Idle,
            focus: SearchFocus::Query,
            inbox: std::sync::Arc::new(std::sync::Mutex::new(None)),
        });
    }

    pub fn mcp_search_popup_cancel(&mut self) {
        self.mcp_search_popup = None;
    }

    pub fn mcp_search_popup_push(&mut self, c: char) {
        if let Some(p) = self.mcp_search_popup.as_mut() {
            p.query.push(c);
            p.focus = SearchFocus::Query;
        }
    }

    pub fn mcp_search_popup_pop(&mut self) {
        if let Some(p) = self.mcp_search_popup.as_mut() {
            p.query.pop();
            p.focus = SearchFocus::Query;
        }
    }

    pub fn mcp_search_popup_toggle_focus(&mut self) {
        if let Some(p) = self.mcp_search_popup.as_mut() {
            p.focus = match p.focus {
                SearchFocus::Query => SearchFocus::Results,
                SearchFocus::Results => SearchFocus::Query,
            };
        }
    }

    pub fn mcp_search_popup_move(&mut self, delta: i32) {
        let Some(p) = self.mcp_search_popup.as_mut() else {
            return;
        };
        let n = p.results.len() as i32;
        if n == 0 {
            return;
        }
        p.cursor = (((p.cursor as i32 + delta) % n + n) % n) as usize;
    }

    pub fn mcp_search_popup_enter(&mut self) {
        let Some(p) = self.mcp_search_popup.as_ref() else {
            return;
        };
        match p.focus {
            SearchFocus::Query => self.run_mcp_search(),
            SearchFocus::Results => self.install_focused_mcp_search_result(),
        }
    }

    fn run_mcp_search(&mut self) {
        let Some(p) = self.mcp_search_popup.as_mut() else {
            return;
        };
        let query = p.query.trim().to_string();
        if query.is_empty() {
            p.status = SearchStatus::Error("type a query first".into());
            return;
        }
        p.status = SearchStatus::Loading;
        p.results.clear();
        p.cursor = 0;
        let inbox = p.inbox.clone();
        std::thread::spawn(move || {
            let result = mcp_index_service::search(&query, 1, 20).map_err(|e| e.to_string());
            if let Ok(mut slot) = inbox.lock() {
                *slot = Some(result);
            }
        });
    }

    pub fn drain_mcp_search_inbox(&mut self) {
        let Some(p) = self.mcp_search_popup.as_mut() else {
            return;
        };
        let delivered = { p.inbox.lock().ok().and_then(|mut s| s.take()) };
        let Some(outcome) = delivered else {
            return;
        };
        match outcome {
            Ok(hits) => {
                p.results = hits;
                p.cursor = 0;
                p.status = SearchStatus::Loaded;
                p.focus = SearchFocus::Results;
            }
            Err(e) => {
                p.status = SearchStatus::Error(e);
            }
        }
    }

    fn install_focused_mcp_search_result(&mut self) {
        let Some(p) = self.mcp_search_popup.as_ref() else {
            return;
        };
        let Some(hit) = p.results.get(p.cursor).cloned() else {
            return;
        };
        self.mcp_search_popup = None;
        let mut enabled = BTreeMap::new();
        for agent in ALL_AGENT_KINDS {
            enabled.insert(*agent, false);
        }
        let now = chrono::Utc::now().timestamp_millis();
        let server = McpServer {
            id: uuid::Uuid::new_v4().to_string(),
            name: hit.qualified_name.clone(),
            config: hit.install_config(),
            enabled,
            created_at: now,
            updated_at: now,
        };
        match mcp_service::upsert(&self.db, server) {
            Ok(s) => {
                let _ = self.reload_mcp();
                self.push_toast(
                    format!(
                        "Added MCP \"{}\" (disabled — enable per agent via a)",
                        s.name
                    ),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Add failed: {e}"), ToastLevel::Error),
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

    // ---------- Plugins tab actions ----------

    pub fn reload_plugins(&mut self) {
        let plugins = plugin_service::list(&self.db).unwrap_or_default();
        let max_idx = plugins.len().saturating_sub(1);
        let focused = self.plugins_view.focused_idx.min(max_idx);
        let prev_armed = self.plugins_view.delete_armed_at;
        self.plugins_view = PluginsView {
            plugins,
            focused_idx: focused,
            delete_armed_at: prev_armed,
        };
    }

    pub fn move_plugin_focus(&mut self, delta: i32) {
        let n = self.plugins_view.plugins.len();
        if n == 0 {
            return;
        }
        let len = n as i32;
        let next = ((self.plugins_view.focused_idx as i32 + delta) % len + len) % len;
        self.plugins_view.focused_idx = next as usize;
    }

    pub fn clear_plugin_delete_arm(&mut self) {
        self.plugins_view.delete_armed_at = None;
    }

    pub fn install_plugin_preset(&mut self, idx: usize) {
        let Some(preset) = PLUGIN_PRESETS.get(idx).copied() else {
            return;
        };
        let source = PluginSource::Git {
            url: format!("https://github.com/{}/{}.git", preset.owner, preset.name),
            branch: preset.branch.to_string(),
        };
        let name_hint = format!("{}/{}", preset.owner, preset.name);
        match plugin_service::install(&self.db, source, Some(&name_hint)) {
            Ok(p) => {
                self.reload_plugins();
                let _ = self.reload_skills();
                self.push_toast(
                    format!(
                        "Installed {} — {} skill(s), {} cmd(s), {} agent(s)",
                        p.name, p.contents.skills, p.contents.commands, p.contents.agents
                    ),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Plugin install failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn open_new_plugin_menu(&mut self) {
        self.presets_popup = Some(PresetsPopup {
            kind: PresetsKind::Plugin,
            cursor: 0,
        });
    }

    pub fn delete_focused_plugin(&mut self) {
        let Some(plugin) = self
            .plugins_view
            .plugins
            .get(self.plugins_view.focused_idx)
            .cloned()
        else {
            self.push_toast("No plugin to delete", ToastLevel::Warning);
            return;
        };
        let now = std::time::Instant::now();
        let armed = self
            .plugins_view
            .delete_armed_at
            .map(|t| now.duration_since(t) <= DELETE_CONFIRM_WINDOW)
            .unwrap_or(false);
        if !armed {
            self.plugins_view.delete_armed_at = Some(now);
            self.push_toast(
                format!("Press d again to remove plugin {}", plugin.name),
                ToastLevel::Warning,
            );
            return;
        }
        self.plugins_view.delete_armed_at = None;
        match plugin_service::uninstall(&self.db, &plugin.id) {
            Ok(()) => {
                self.reload_plugins();
                self.push_toast(
                    format!("Uninstalled plugin {}", plugin.name),
                    ToastLevel::Success,
                );
            }
            Err(e) => self.push_toast(format!("Uninstall failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn reload_backups(&mut self) {
        let snapshots = backup_service::list_snapshots().unwrap_or_default();
        let max_idx = snapshots.len().saturating_sub(1);
        let focused = self.backup_view.focused_idx.min(max_idx);
        let prev_armed = self.backup_view.delete_armed_at;
        self.backup_view = BackupView {
            snapshots,
            focused_idx: focused,
            delete_armed_at: prev_armed,
        };
    }

    /// Two-phase delete of the focused snapshot. First call arms the
    /// delete + shows a "press d again" toast; second call within
    /// [`DELETE_CONFIRM_WINDOW`] runs the delete. Pressing any other key
    /// clears the arm (see `clear_backup_delete_arm`).
    pub fn delete_focused_snapshot(&mut self) {
        let Some(snap) = self
            .backup_view
            .snapshots
            .get(self.backup_view.focused_idx)
            .cloned()
        else {
            self.push_toast("No snapshot to delete", ToastLevel::Warning);
            return;
        };
        let now = std::time::Instant::now();
        let armed = self
            .backup_view
            .delete_armed_at
            .map(|t| now.duration_since(t) <= DELETE_CONFIRM_WINDOW)
            .unwrap_or(false);
        if !armed {
            self.backup_view.delete_armed_at = Some(now);
            self.push_toast(
                format!("Press d again to delete snapshot {}", snap.timestamp),
                ToastLevel::Warning,
            );
            return;
        }
        self.backup_view.delete_armed_at = None;
        let ts = snap.timestamp.clone();
        match backup_service::delete_snapshot(&ts) {
            Ok(()) => {
                self.reload_backups();
                self.push_toast(format!("Deleted snapshot {ts}"), ToastLevel::Success);
            }
            Err(e) => self.push_toast(format!("Delete failed: {e}"), ToastLevel::Error),
        }
    }

    pub fn clear_backup_delete_arm(&mut self) {
        self.backup_view.delete_armed_at = None;
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
