//! Input event dispatch: keyboard and mouse.
//!
//! Mouse handling is hit-test-based: every interactive widget records its
//! last-rendered `Rect` into `App`'s hit registry; on `MouseEvent` we resolve
//! the topmost rect containing the click point and dispatch its action.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::app::{App, Tab};

/// Returns `true` if the event was consumed and the app should re-render.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if key.kind != KeyEventKind::Press {
        return false;
    }

    // Modal popups consume input first so global shortcuts can't hijack them.
    if app.agent_popup.is_some() {
        return handle_agent_popup_key(app, key);
    }
    if app.method_popup.is_some() {
        return handle_method_popup_key(app, key);
    }
    if app.presets_popup.is_some() {
        return handle_presets_popup_key(app, key);
    }
    if app.search_popup.is_some() {
        return handle_search_popup_key(app, key);
    }
    if app.show_activity {
        if matches!(
            (key.code, key.modifiers),
            (KeyCode::Char('L') | KeyCode::Char('l') | KeyCode::Esc, _)
        ) {
            app.toggle_activity();
            return true;
        }
        // Swallow other keys while the overlay is up so tab switches etc.
        // don't quietly happen behind the panel.
        return true;
    }
    if app.show_help {
        // Any keypress closes help so the user is never trapped in it.
        app.show_help = false;
        return true;
    }

    // Global bindings (work on every tab).
    match (key.code, key.modifiers) {
        (KeyCode::Char('q'), KeyModifiers::NONE)
        | (KeyCode::Char('c') | KeyCode::Char('q'), KeyModifiers::CONTROL) => {
            app.should_quit = true;
            return true;
        }
        (KeyCode::Tab, KeyModifiers::NONE) | (KeyCode::Right, KeyModifiers::CONTROL) => {
            app.next_tab();
            return true;
        }
        (KeyCode::BackTab, _) | (KeyCode::Left, KeyModifiers::CONTROL) => {
            app.prev_tab();
            return true;
        }
        (KeyCode::Char('L'), KeyModifiers::SHIFT) | (KeyCode::Char('L'), KeyModifiers::NONE) => {
            app.toggle_activity();
            return true;
        }
        (KeyCode::Char('?'), _) => {
            app.toggle_help();
            return true;
        }
        (KeyCode::Char(c @ '1'..='9'), KeyModifiers::NONE) => {
            let idx = (c as u8 - b'1') as usize;
            if let Some(tab) = Tab::from_index(idx) {
                app.set_active_tab(tab);
                return true;
            }
        }
        _ => {}
    }

    // Per-tab bindings.
    match app.active_tab {
        Tab::Profiles => handle_profiles_key(app, key),
        Tab::Skills => handle_skills_key(app, key),
        Tab::Mcp => handle_mcp_key(app, key),
        Tab::Rules => handle_rules_key(app, key),
        Tab::Backup => handle_backup_key(app, key),
        Tab::Settings => handle_settings_key(app, key),
        _ => false,
    }
}

fn handle_settings_key(app: &mut App, key: KeyEvent) -> bool {
    // Text-input mode has priority — swallow typing + editing keys.
    if app.settings_view.input_buf.is_some() {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                app.settings_input_cancel();
                return true;
            }
            (KeyCode::Enter, _) => {
                app.settings_input_commit();
                return true;
            }
            (KeyCode::Backspace, _) => {
                app.settings_input_pop();
                return true;
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                app.settings_input_push(c);
                return true;
            }
            _ => return false,
        }
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('t'), KeyModifiers::NONE) => {
            app.cycle_theme();
            true
        }
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.move_settings_focus(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.move_settings_focus(1);
            true
        }
        (KeyCode::Char(' '), KeyModifiers::NONE) => {
            app.settings_toggle_focused();
            true
        }
        (KeyCode::Enter, _) => {
            app.settings_begin_edit();
            true
        }
        _ => false,
    }
}

fn handle_backup_key(app: &mut App, key: KeyEvent) -> bool {
    // Two-phase delete: `d` twice deletes; any other key clears the arm so
    // the prompt doesn't silently outlive the user's attention.
    if !matches!(key.code, KeyCode::Char('d')) {
        app.clear_backup_delete_arm();
    }
    match (key.code, key.modifiers) {
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.move_backup_focus(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.move_backup_focus(1);
            true
        }
        (KeyCode::Char('n'), KeyModifiers::NONE) => {
            app.create_snapshot();
            true
        }
        (KeyCode::Char('r'), KeyModifiers::NONE) => {
            app.restore_focused_snapshot();
            true
        }
        (KeyCode::Char('p'), KeyModifiers::NONE) => {
            app.push_github_backup();
            true
        }
        (KeyCode::Char('d'), KeyModifiers::NONE) => {
            app.delete_focused_snapshot();
            true
        }
        _ => false,
    }
}

fn handle_rules_key(app: &mut App, key: KeyEvent) -> bool {
    if app.rules_view.filter_editing {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                app.rules_filter_clear();
                return true;
            }
            (KeyCode::Enter, _) => {
                app.rules_filter_close_keep();
                return true;
            }
            (KeyCode::Backspace, _) => {
                app.rules_filter_pop();
                return true;
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                app.rules_filter_push(c);
                return true;
            }
            _ => return false,
        }
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('/'), KeyModifiers::NONE) => {
            app.rules_filter_open();
            true
        }
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.move_rules_focus(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.move_rules_focus(1);
            true
        }
        (KeyCode::Char(' '), KeyModifiers::NONE) => {
            app.toggle_focused_rule_selection();
            true
        }
        (KeyCode::Char('a'), KeyModifiers::NONE) => {
            app.open_agent_popup_for_focused_rule();
            true
        }
        (KeyCode::Char('d'), KeyModifiers::NONE) => {
            app.delete_focused_rule();
            true
        }
        (KeyCode::Char('S'), KeyModifiers::SHIFT) | (KeyCode::Char('s'), KeyModifiers::SHIFT) => {
            app.sync_all_rules();
            true
        }
        (KeyCode::Char('i'), KeyModifiers::NONE) => {
            app.import_rules_from_live();
            true
        }
        _ => false,
    }
}

fn handle_mcp_key(app: &mut App, key: KeyEvent) -> bool {
    if app.mcp_view.filter_editing {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                app.mcp_filter_clear();
                return true;
            }
            (KeyCode::Enter, _) => {
                app.mcp_filter_close_keep();
                return true;
            }
            (KeyCode::Backspace, _) => {
                app.mcp_filter_pop();
                return true;
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                app.mcp_filter_push(c);
                return true;
            }
            _ => return false,
        }
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('/'), KeyModifiers::NONE) => {
            app.mcp_filter_open();
            true
        }
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.move_mcp_focus(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.move_mcp_focus(1);
            true
        }
        (KeyCode::Char(' '), KeyModifiers::NONE) => {
            app.toggle_focused_mcp_selection();
            true
        }
        (KeyCode::Char('a'), KeyModifiers::NONE) => {
            app.open_agent_popup_for_focused_mcp();
            true
        }
        (KeyCode::Char('i'), KeyModifiers::NONE) => {
            app.import_mcp_from_live();
            true
        }
        (KeyCode::Char('d'), KeyModifiers::NONE) => {
            app.delete_focused_mcp();
            true
        }
        (KeyCode::Char('S'), KeyModifiers::SHIFT) | (KeyCode::Char('s'), KeyModifiers::SHIFT) => {
            app.sync_all_mcp();
            true
        }
        (KeyCode::Char('n'), KeyModifiers::NONE) => {
            app.open_new_mcp_menu();
            true
        }
        _ => false,
    }
}

fn handle_profiles_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.move_profile_selection(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.move_profile_selection(1);
            true
        }
        (KeyCode::Left | KeyCode::Char('h'), KeyModifiers::NONE) => {
            app.move_agent_focus(-1);
            true
        }
        (KeyCode::Right | KeyCode::Char('l'), KeyModifiers::NONE) => {
            app.move_agent_focus(1);
            true
        }
        (KeyCode::Enter, KeyModifiers::NONE) | (KeyCode::Char('s'), KeyModifiers::NONE) => {
            app.switch_focused_profile();
            true
        }
        (KeyCode::Char('i'), KeyModifiers::NONE) => {
            app.import_profiles_from_live();
            true
        }
        _ => false,
    }
}

fn handle_skills_key(app: &mut App, key: KeyEvent) -> bool {
    // Filter input mode: absorb typed chars and editing keys before the
    // normal per-tab keymap sees them.
    if app.skills_view.filter_editing {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                app.skills_filter_clear();
                return true;
            }
            (KeyCode::Enter, _) => {
                app.skills_filter_close_keep();
                return true;
            }
            (KeyCode::Backspace, _) => {
                app.skills_filter_pop();
                return true;
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                app.skills_filter_push(c);
                return true;
            }
            _ => return false,
        }
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('/'), KeyModifiers::NONE) => {
            app.skills_filter_open();
            true
        }
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.move_skill_focus(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.move_skill_focus(1);
            true
        }
        (KeyCode::Char(' '), KeyModifiers::NONE) => {
            app.toggle_focused_skill_selection();
            true
        }
        (KeyCode::Char('a'), KeyModifiers::NONE) => {
            app.open_agent_popup_for_focused();
            true
        }
        (KeyCode::Char('m'), KeyModifiers::NONE) => {
            app.cycle_focused_skill_method();
            true
        }
        (KeyCode::Char('M'), KeyModifiers::SHIFT) | (KeyCode::Char('M'), KeyModifiers::NONE) => {
            app.open_method_popup_for_focused();
            true
        }
        (KeyCode::Char('s'), KeyModifiers::NONE) => {
            app.sync_selected_skills();
            true
        }
        (KeyCode::Char('S'), KeyModifiers::SHIFT) | (KeyCode::Char('s'), KeyModifiers::SHIFT) => {
            app.sync_all_skills();
            true
        }
        (KeyCode::Enter, KeyModifiers::NONE) => {
            app.sync_focused_skill();
            true
        }
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
            app.select_all_skills();
            true
        }
        (KeyCode::Char('A'), KeyModifiers::CONTROL | KeyModifiers::SHIFT) => {
            app.clear_skill_selection();
            true
        }
        (KeyCode::Char('i'), KeyModifiers::NONE) => {
            app.import_skills_from_live();
            true
        }
        (KeyCode::Char('n'), KeyModifiers::NONE) => {
            app.open_new_skill_menu();
            true
        }
        (KeyCode::Char('f'), KeyModifiers::CONTROL) => {
            app.open_search_skills();
            true
        }
        _ => false,
    }
}

fn handle_agent_popup_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.agent_popup_move(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.agent_popup_move(1);
            true
        }
        (KeyCode::Char(' '), KeyModifiers::NONE) => {
            app.agent_popup_toggle();
            true
        }
        (KeyCode::Char('a') | KeyCode::Char('A'), _) => {
            app.agent_popup_set_all(true);
            true
        }
        (KeyCode::Char('n') | KeyCode::Char('N'), _) => {
            app.agent_popup_set_all(false);
            true
        }
        (KeyCode::Enter, _) => {
            app.agent_popup_commit();
            true
        }
        (KeyCode::Esc, _) => {
            app.agent_popup_cancel();
            true
        }
        _ => false,
    }
}

fn handle_search_popup_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.search_popup_cancel();
            true
        }
        (KeyCode::Enter, _) => {
            app.search_popup_enter();
            true
        }
        (KeyCode::Tab, _) => {
            app.search_popup_toggle_focus();
            true
        }
        (KeyCode::Backspace, _) => {
            app.search_popup_pop();
            true
        }
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.search_popup_move(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.search_popup_move(1);
            true
        }
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            app.search_popup_push(c);
            true
        }
        _ => false,
    }
}

fn handle_presets_popup_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.presets_popup_move(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.presets_popup_move(1);
            true
        }
        (KeyCode::Enter, _) => {
            app.presets_popup_commit();
            true
        }
        (KeyCode::Esc, _) => {
            app.presets_popup_cancel();
            true
        }
        _ => false,
    }
}

fn handle_method_popup_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
            app.method_popup_move(-1);
            true
        }
        (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
            app.method_popup_move(1);
            true
        }
        (KeyCode::Enter, _) => {
            app.method_popup_commit();
            true
        }
        (KeyCode::Esc, _) => {
            app.method_popup_cancel();
            true
        }
        _ => false,
    }
}

/// Mouse events: left-click walks button hits first, then tab bar hits. More
/// hit-testing lands as the UI grows.
pub fn handle_mouse(
    app: &mut App,
    event: MouseEvent,
    tab_bar_hits: &[TabHit],
    button_hits: &[ButtonHit],
) -> bool {
    if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        return false;
    }
    for hit in button_hits {
        if hit.contains(event.column, event.row) {
            dispatch_button_action(app, hit.action);
            return true;
        }
    }
    for hit in tab_bar_hits {
        if hit.contains(event.column, event.row) {
            app.set_active_tab(hit.tab);
            return true;
        }
    }
    false
}

fn dispatch_button_action(app: &mut App, action: ButtonAction) {
    match action {
        ButtonAction::SkillsNew => app.open_new_skill_menu(),
        ButtonAction::SkillsDiscover => app.open_search_skills(),
        ButtonAction::SkillsImport => app.import_skills_from_live(),
        ButtonAction::SkillsSyncSelected => app.sync_selected_skills(),
        ButtonAction::SkillsSyncAll => app.sync_all_skills(),
        ButtonAction::BackupSnapshot => app.create_snapshot(),
        ButtonAction::BackupRestore => app.restore_focused_snapshot(),
        ButtonAction::BackupPush => app.push_github_backup(),
        ButtonAction::BackupDelete => app.delete_focused_snapshot(),
        ButtonAction::McpNew => app.open_new_mcp_menu(),
        ButtonAction::McpSyncAll => app.sync_all_mcp(),
        ButtonAction::McpImport => app.import_mcp_from_live(),
        ButtonAction::McpDelete => app.delete_focused_mcp(),
        ButtonAction::RulesSyncAll => app.sync_all_rules(),
        ButtonAction::RulesImport => app.import_rules_from_live(),
        ButtonAction::RulesDelete => app.delete_focused_rule(),
    }
}

/// One tab label's rendered rect + the tab it activates.
#[derive(Debug, Clone, Copy)]
pub struct TabHit {
    pub tab: Tab,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl TabHit {
    pub fn contains(self, col: u16, row: u16) -> bool {
        col >= self.x && col < self.x + self.w && row >= self.y && row < self.y + self.h
    }
}

/// An action triggered by clicking a toolbar button on one of the tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonAction {
    SkillsNew,
    SkillsDiscover,
    SkillsImport,
    SkillsSyncSelected,
    SkillsSyncAll,
    BackupSnapshot,
    BackupRestore,
    BackupPush,
    BackupDelete,
    McpNew,
    McpSyncAll,
    McpImport,
    McpDelete,
    RulesSyncAll,
    RulesImport,
    RulesDelete,
}

/// One toolbar button's rendered rect + the action it fires.
#[derive(Debug, Clone, Copy)]
pub struct ButtonHit {
    pub action: ButtonAction,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl ButtonHit {
    pub fn contains(self, col: u16, row: u16) -> bool {
        col >= self.x && col < self.x + self.w && row >= self.y && row < self.y + self.h
    }
}
