# Tiksu Clipper UI redesign

Approved by the user in chat on 2026-11-13 ("Kyllä", approach 2).

## Intent
The current UI (teal brand, gradients, glow, card grid) is replaced by the neutral IT-Veljekset design
system used by ProjectHub (`IT-Veljekset Group/DESIGN.md`). Users get clear guidance on every page, and new
pages can be added cheaply.

## Goals
- ProjectHub look: near-black neutral surfaces, 1px borders instead of shadows, one blue accent, colour only as
  meaning (green running, amber warning, red error).
- ProjectHub layout: 240 px sidebar with grouped navigation and settings/version in the footer, breadcrumb bar on
  top, page head (title, one-line description, one primary action), one scroll area, panels with header row and
  40 px rows.
- Guidance: every page has a description, every setting a hint, empty states say what to do, Overview has a
  three-step "Aloitus" panel until the first clip exists, a new Ohje page answers common questions.
- Extensible: one file per page, a single `Page` registry (label, icon, sidebar group), a small widget kit in
  `tiksu.rs`. Adding a page = new file + enum variant + registry rows.

## Non-goals
- No light theme (tokens are structured so it can be added), no search, no English, no custom window chrome.
- No behaviour change: capture, hotkeys, updates, library and trim logic stay as they are.

## Anti-goals
- Gradients, glow, eyebrow text, card-in-card, primary actions behind hover, same number twice on a page.

## Approaches considered
1. Re-skin only: fast, but no guidance and no better structure.
2. **Re-skin + new frame + guidance (chosen).**
3. One long Settings page: fewer clicks, hard to scan.

## Decisions (my calls)
- Native title bar kept, but coloured dark through DWM (`DWMWA_CAPTION_COLOR`, dark mode attribute); a custom
  title bar would need resize/drag code for little gain.
- Fonts: Segoe UI from `%WINDIR%\Fonts` (as ProjectHub), bundled Instrument Sans as fallback; JetBrains Mono only
  for key caps and file names. Outfit is removed.
- Icons stay on Phosphor (already bundled); ProjectHub uses Lucide, but the glyph set is not the point.
- Scales from DESIGN.md: radius 6/10/14, font 11/12/13/15/20, heights 32/28/40, one shadow for floating things.
- Clips page becomes a list (thumbnail, name, meta, visible actions), not a card grid.
- Settings pages are a single column, max width 880.

## Structure
```
src/tiksu.rs            tokens, fonts, widgets (button, switch, segmented, panel, row, list_row, badge, ...)
src/ui/mod.rs           app state, frame (sidebar, topbar, page head), toast, save bar, Page registry
src/ui/pages/*.rs       overview, clips, source, audio, quality, hotkeys, profiles, app, help
```
