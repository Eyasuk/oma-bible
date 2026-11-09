---
name: oma-design
description: >
  Design rules for Oma Bible's desktop (GPUI) and terminal front ends: an
  Omawrite-simple reading surface with professional study tools layered on
  quietly. Use before adding or restyling any UI in crates/gpui or
  crates/tui — new tools, overlays, panels, the dock, the wheel, the notes
  stack, settings rows, study features, colours, spacing, or typography.
  Triggers: design, redesign, looks bad, make it nicer, new panel, new tool,
  overlay, popup, dock, wheel, notes, study feature, layout, spacing, theme.
---

# Oma Bible design

The page is scripture. Everything else is chrome, and chrome earns its place
by staying quiet until it is wanted. Think Omawrite: one centred column, big
calm type, no toolbars fighting for attention. Study power is real but it is
*reached for* — by key, right-click, or hover — never parked on screen.

## The five rules

1. **One loud thing.** Only the reading column is full-contrast. Chrome uses
   `muted_foreground` icons, `popover` surfaces, `border` hairlines. Accent
   colour marks *state* (bookmarked, selected, hovered tool), never decoration.
2. **Quiet until reached.** Chrome is compact or collapsed at rest and opens on
   intent: the dock is a small pill, the notes stack is a pile that fans out
   on hover, the wheel only exists after right-click/Space.
3. **Keyboard first, mouse equal.** Every action has a Vim-style key *and* a
   pointer path. Show the key as a hint inside the UI (wheel hub, footers).
   Add the key to `handle_key_down` and the README table in the same change.
4. **Follow the desktop.** Colours come only from the active theme
   (`cx.theme()` in GPUI, `app.theme` in the TUI). On Omarchy the theme is
   Omarchy's; never hard-code a colour except the scrim and highlight swatches.
5. **Opaque surfaces.** Floating chrome sits over text, so its background is
   opaque. Dim with muted icon colour, not element `opacity()` — opacity lets
   verse text bleed through.

## Tokens (GPUI)

| Token | Value |
|---|---|
| Reading column | `max_w 760`, `px 40`, line height `font × 1.62` |
| Chrome text | 10–11 px labels, 12–13 px body, 18 px sheet titles |
| Icons | 16 px in the dock, 19 px in the wheel, 14 px inline in rows |
| Radius | 10 rows/cards, 12 small cards, 16–18 sheets, `rounded_full` pills/wheel |
| Edge gap | 14 px from the window edge for floating chrome |
| Scrim | `rgba(0x00000059)` for the wheel, `0x00000066` for sheets |
| Shadow | `shadow_md` resting chrome, `shadow_xl` sheets and the wheel |

## Where things live

- `crates/gpui/src/tools.rs` — the `Tool` list. **Add a new tool here**: icon,
  label, key, then its action in `OmaBibleApp::run_tool` and state in
  `tool_active`. Put it in `Tool::WHEEL` (room for ~8) and only in
  `Tool::DOCK` if it is used on most verses.
- `crates/gpui/src/chrome.rs` — dock, wheel, notes stack. Constants at the
  top of each section; keep geometry there, not inline.
- `crates/gpui/src/reader.rs` — the column. Keep rows to: number, text, small
  state icons. Longer content (notes, cross-refs) goes in the stack or a sheet,
  not under the verse.
- `app.rs` `overlay_shell` — use it for any new sheet so scrim, width and
  click-outside behave the same.

## Study features: how to add them without clutter

Professional study means depth on demand. Prefer, in order:

1. **A tool** (wheel + key) that acts on the selected verse or visual range.
2. **A stack** at an edge that lists per-chapter items and fans out on hover
   (notes today; cross-references, word studies, tags later). Stacks hide
   when empty and move away from the dock's edge.
3. **A sheet** (`overlay_shell`) for focused work: note detail, highlight
   picker, compare translations.
4. **A screen** only for whole-library views (study list, downloads).

Never add a permanent sidebar or toolbar row to the reader.

Ideas that fit: verse compare across installed translations (sheet), tags on
bookmarks (tool + study screen filter), cross-reference stack, reading plans
(screen), note export to Markdown, search across the whole book.

## Checklist before finishing UI work

- Looks right in a light and a dark theme, and at the 940 px minimum width.
- Nothing translucent sits over verse text.
- New action has a key, a pointer path, a README row, and a hint in the UI.
- Shared state (`Settings`, `Bookmarks`, `Highlights`) changes go in
  `crates/core` with serde defaults so old files still load.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` all pass.
- Look at it: run the app and take a screenshot of the window (`grim -g`
  with the geometry from `hyprctl clients -j`) instead of trusting the code.
