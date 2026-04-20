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
    match (key.code, key.modifiers) {
        (KeyCode::Char('t'), KeyModifiers::NONE) => {
            app.cycle_theme();
            true
        }
        _ => false,
    }
}

fn handle_backup_key(app: &mut App, key: KeyEvent) -> bool {
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
        _ => false,
    }
}

fn handle_rules_key(app: &mut App, key: KeyEvent) -> bool {
    match (key.code, key.modifiers) {
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
    match (key.code, key.modifiers) {
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
    match (key.code, key.modifiers) {
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

/// Mouse events: for now we only react to left-click inside the tab bar.
/// More hit-testing lands as the UI grows.
pub fn handle_mouse(app: &mut App, event: MouseEvent, tab_bar_hits: &[TabHit]) -> bool {
    if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
        return false;
    }
    for hit in tab_bar_hits {
        if hit.contains(event.column, event.row) {
            app.set_active_tab(hit.tab);
            return true;
        }
    }
    false
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
