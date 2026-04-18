# Contributing a theme

Drop a `.json` file in this directory. The filename (without `.json`) becomes the theme slug — the name used in `settings.json` and the Settings tab.

## Schema

All 15 color keys are required. Each value is a `[R, G, B]` array (0–255).

```json
{
  "name":         "My Theme",
  "author":       "Your Name",
  "surface":      [18,  18,  24],
  "surface_alt":  [30,  30,  40],
  "primary":      [135, 180, 255],
  "secondary":    [200, 170, 255],
  "accent":       [255, 200, 120],
  "muted":        [130, 130, 140],
  "border":       [60,  60,  75],
  "border_focus": [135, 180, 255],
  "text":         [230, 230, 240],
  "text_dim":     [170, 170, 180],
  "success":      [130, 210, 160],
  "warning":      [255, 200, 120],
  "danger":       [255, 120, 140],
  "selection_bg": [50,  60,  90],
  "selection_fg": [255, 255, 255]
}
```

## Color token guide

| Token | Used for |
|-------|----------|
| `surface` | Main background |
| `surface_alt` | Subtle secondary background (popups, alternate rows) |
| `primary` | Primary accent — tab titles, focused borders, key labels |
| `secondary` | Secondary accent — rarely used, currently reserved |
| `accent` | Warm accent — buttons, method labels, KB chips |
| `muted` | Dim text — hints, labels, non-focused elements |
| `border` | Unfocused widget border |
| `border_focus` | Focused widget border |
| `text` | Main body text |
| `text_dim` | Secondary body text |
| `success` | Status indicators (synced, enabled dots ●) |
| `warning` | Warning indicators (⬆ update) |
| `danger` | Destructive action buttons, error toasts |
| `selection_bg` | Highlighted row background |
| `selection_fg` | Highlighted row foreground |

## User themes directory

You can also drop themes into `~/.liteconfig/themes/` for personal use without modifying this repo. User themes override built-in themes with the same filename slug.

## Submitting

1. Fork the repo, add your `themes/<slug>.json`.
2. Launch liteconfig, navigate to the Settings tab, press `t` to confirm it looks right.
3. Open a pull request — one JSON file per PR, please.
