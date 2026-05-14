//! Semantic color palette. All tabs + widgets pull colors through this module.
//!
//! Built-in themes live in `themes/*.json` at the workspace root and are
//! compiled in via `include_str!`. Users can add or override themes by
//! dropping `.json` files into `~/.liteconfig/themes/`.

use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

/// One concrete theme — a bundle of named semantic colors.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct Theme {
    pub surface: Color,
    pub surface_alt: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub muted: Color,
    pub border: Color,
    pub border_focus: Color,
    pub text: Color,
    pub text_dim: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
}

/// JSON-deserializable representation. Each field is `[R, G, B]`.
#[derive(Debug, Deserialize)]
struct ThemeJson {
    surface: [u8; 3],
    surface_alt: [u8; 3],
    primary: [u8; 3],
    secondary: [u8; 3],
    accent: [u8; 3],
    muted: [u8; 3],
    border: [u8; 3],
    border_focus: [u8; 3],
    text: [u8; 3],
    text_dim: [u8; 3],
    success: [u8; 3],
    warning: [u8; 3],
    danger: [u8; 3],
    selection_bg: [u8; 3],
    selection_fg: [u8; 3],
}

fn rgb([r, g, b]: [u8; 3]) -> Color {
    Color::Rgb(r, g, b)
}

impl From<ThemeJson> for Theme {
    fn from(j: ThemeJson) -> Self {
        Self {
            surface: rgb(j.surface),
            surface_alt: rgb(j.surface_alt),
            primary: rgb(j.primary),
            secondary: rgb(j.secondary),
            accent: rgb(j.accent),
            muted: rgb(j.muted),
            border: rgb(j.border),
            border_focus: rgb(j.border_focus),
            text: rgb(j.text),
            text_dim: rgb(j.text_dim),
            success: rgb(j.success),
            warning: rgb(j.warning),
            danger: rgb(j.danger),
            selection_bg: rgb(j.selection_bg),
            selection_fg: rgb(j.selection_fg),
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in themes compiled into the binary at build time.
// Paths are relative to this source file (crates/liteconfig-tui/src/theme.rs).
// ---------------------------------------------------------------------------
const BUILTIN_THEMES: &[(&str, &str)] = &[
    ("dark", include_str!("../../../themes/dark.json")),
    ("light", include_str!("../../../themes/light.json")),
    ("dracula", include_str!("../../../themes/dracula.json")),
    ("nord", include_str!("../../../themes/nord.json")),
    ("gruvbox", include_str!("../../../themes/gruvbox.json")),
    ("monokai", include_str!("../../../themes/monokai.json")),
    ("solarized", include_str!("../../../themes/solarized.json")),
    (
        "catppuccin-mocha",
        include_str!("../../../themes/catppuccin-mocha.json"),
    ),
    (
        "catppuccin-macchiato",
        include_str!("../../../themes/catppuccin-macchiato.json"),
    ),
    (
        "catppuccin-frappe",
        include_str!("../../../themes/catppuccin-frappe.json"),
    ),
    (
        "catppuccin-latte",
        include_str!("../../../themes/catppuccin-latte.json"),
    ),
    ("vesper", include_str!("../../../themes/vesper.json")),
    (
        "noir-berry",
        include_str!("../../../themes/noir-berry.json"),
    ),
    (
        "velvet-ember",
        include_str!("../../../themes/velvet-ember.json"),
    ),
    ("slater", include_str!("../../../themes/slater.json")),
    ("cardinal", include_str!("../../../themes/cardinal.json")),
    ("abyss", include_str!("../../../themes/abyss.json")),
];

impl Theme {
    /// All built-in theme slugs in their defined order.
    pub fn all_builtin_names() -> Vec<&'static str> {
        BUILTIN_THEMES.iter().map(|(slug, _)| *slug).collect()
    }

    /// Load a theme by slug. Falls back to the built-in `dark` theme on any error.
    pub fn by_name(name: &str) -> Self {
        for (slug, raw) in BUILTIN_THEMES {
            if *slug == name {
                if let Ok(j) = serde_json::from_str::<ThemeJson>(raw) {
                    return j.into();
                }
            }
        }
        // Fall back to dark.
        let raw = BUILTIN_THEMES
            .iter()
            .find(|(s, _)| *s == "dark")
            .map(|(_, r)| *r)
            .unwrap_or("{}");
        serde_json::from_str::<ThemeJson>(raw)
            .map(Into::into)
            .unwrap_or(Self::dark_fallback())
    }

    /// Scan a directory for user-contributed theme JSONs. Returns `(slug, Theme)` pairs
    /// sorted by slug. Invalid files are silently skipped.
    pub fn load_user_themes(dir: &Path) -> Vec<(String, Self)> {
        let Ok(read) = std::fs::read_dir(dir) else {
            return vec![];
        };
        let mut out: Vec<(String, Self)> = read
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("json"))
            })
            .filter_map(|e| {
                let slug = e.path().file_stem()?.to_string_lossy().into_owned();
                let raw = std::fs::read_to_string(e.path()).ok()?;
                let j: ThemeJson = serde_json::from_str(&raw).ok()?;
                Some((slug, j.into()))
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Hardcoded fallback in case the JSON is somehow missing (should never happen
    /// in a correctly built binary, but prevents a panic).
    const fn dark_fallback() -> Self {
        Self {
            surface: Color::Rgb(18, 18, 24),
            surface_alt: Color::Rgb(30, 30, 40),
            primary: Color::Rgb(135, 180, 255),
            secondary: Color::Rgb(200, 170, 255),
            accent: Color::Rgb(255, 200, 120),
            muted: Color::Rgb(130, 130, 140),
            border: Color::Rgb(60, 60, 75),
            border_focus: Color::Rgb(135, 180, 255),
            text: Color::Rgb(230, 230, 240),
            text_dim: Color::Rgb(170, 170, 180),
            success: Color::Rgb(130, 210, 160),
            warning: Color::Rgb(255, 200, 120),
            danger: Color::Rgb(255, 120, 140),
            selection_bg: Color::Rgb(50, 60, 90),
            selection_fg: Color::Rgb(255, 255, 255),
        }
    }

    pub fn default_style(self) -> Style {
        Style::default().fg(self.text).bg(self.surface)
    }

    pub fn border_style(self, focused: bool) -> Style {
        let color = if focused {
            self.border_focus
        } else {
            self.border
        };
        Style::default().fg(color)
    }

    pub fn selection_style(self) -> Style {
        Style::default()
            .fg(self.selection_fg)
            .bg(self.selection_bg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn muted_style(self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn accent_style(self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    /// Sweep `primary → accent` across `text` as per-char RGB-interpolated
    /// spans. `tick_idx` rotates the starting offset so the gradient drifts
    /// slightly over time. Falls back to solid `primary` if either anchor
    /// is not an RGB color (named / indexed colors carry no components).
    pub fn gradient_title<'a>(self, text: &'a str, tick_idx: u8) -> Vec<ratatui::text::Span<'a>> {
        let (a, b) = match (self.primary, self.accent) {
            (Color::Rgb(r0, g0, b0), Color::Rgb(r1, g1, b1)) => ((r0, g0, b0), (r1, g1, b1)),
            _ => {
                return vec![ratatui::text::Span::styled(
                    text,
                    Style::default()
                        .fg(self.primary)
                        .add_modifier(Modifier::BOLD),
                )]
            }
        };
        let n = text.chars().count().max(1);
        let offset = tick_idx as usize;
        text.chars()
            .enumerate()
            .map(|(i, ch)| {
                // 0..=1 progress, slow drift via offset.
                let t = (((i + offset / 4) % (n * 2)) as f32 / (n * 2) as f32) * 2.0;
                let t = if t > 1.0 { 2.0 - t } else { t };
                let r = (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8;
                let g = (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8;
                let bch = (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8;
                ratatui::text::Span::styled(
                    ch.to_string(),
                    Style::default()
                        .fg(Color::Rgb(r, g, bch))
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect()
    }
}

/// Platform-aware modifier label — `⌘` on macOS, `Ctrl+` elsewhere.
pub fn mod_key_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    }
}

/// Named keyboard shortcuts used in the hint bar.
pub fn key_label(action: KeyAction) -> String {
    match action {
        KeyAction::CommandPalette => format!("{}K", mod_key_label()),
        KeyAction::Help => "F1".to_string(),
        KeyAction::Quit => "q".to_string(),
        KeyAction::SelectAll => format!("{}A", mod_key_label()),
        KeyAction::Duplicate => format!("{}D", mod_key_label()),
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum KeyAction {
    CommandPalette,
    Help,
    Quit,
    SelectAll,
    Duplicate,
}
