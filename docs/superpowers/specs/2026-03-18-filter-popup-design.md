# Filter Popup Design

**Date:** 2026-03-18
**Status:** Approved

## Summary

Replace the separate tag-text filter (`f`/`/`) and version modal (`v`) with a unified filter popup opened by `f`. Move the tag text input to `t`. Add a multi-select tag section to the popup, sourced from discovered runner tags.

## Key Binding Changes

| Key | Old behaviour | New behaviour |
|-----|--------------|---------------|
| `f` | Opens tag text input (`FilterInput` mode) | Opens filter popup (`FilterPopup` mode) |
| `/` | Opens tag text input | Opens filter popup |
| `t` | — | Opens tag text input (renamed from `f`/`/`) |
| `v` | Opens version modal (`VersionFilter` mode) | Removed; merged into `f` popup |

### Key conflict notes

- `t` in Dashboard mode is safe: the existing `t`/`T` bindings that set `discovery_mode = ConfiguredTargets` live exclusively in the `Settings` modal handler. Because the Dashboard key handler only runs when `mode == AppMode::Dashboard`, there is no collision at runtime.
- `a` inside `FilterPopup` clears the focused section. The existing Dashboard `a` binding that opens the age filter only runs when `mode == AppMode::Dashboard`, so it is also safe.

## Modes

### Removed
- `AppMode::VersionFilter`

### Added
- `AppMode::FilterPopup`

### Unchanged
- `AppMode::FilterInput` (now opened by `t`)

## New Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterPopupSection {
    #[default]
    Tags,
    Versions,
}
```

## App State Changes

### Added fields
```rust
pub filter_popup_section: FilterPopupSection,
pub tag_options: Vec<String>,       // discovered tags, sorted alphabetically, deduplicated
pub selected_tags: Vec<String>,     // multi-select state for popup tag section
pub tag_list_state: ListState,      // list cursor for tags section in popup
```

### Removed fields
None. `version_options`, `selected_versions`, and `version_list_state` remain, now used inside the unified popup.

## Tag Discovery

Add `extract_runner_tags()` in `src/models/runner.rs` alongside `extract_runner_versions()`:
- Collects all unique values from `runner.tag_list` across `raw_runners` only (not manager-level tags)
- Each tag is trimmed and filtered for empty strings (consistent with `extract_runner_versions()`)
- Sorted alphabetically
- Deduplicated
- Populated whenever `version_options` is populated (after a successful fetch)

## Popup Layout

```
┌─ Filter [f] ──────────────────────────────┐
│ ▶ Tags                                     │
│   [ ] docker                               │
│   [x] kubernetes                           │
│   [ ] linux                                │
│                                            │
│   Versions                                 │
│   [x] 17.5.0                               │
│   [ ] 17.4.1                               │
│                                            │
│ space:toggle  a:clear section  esc:close   │
└────────────────────────────────────────────┘
```

`▶` marks the focused section. Focused section header is highlighted; unfocused is muted.

When a section has no items, show a single non-interactive placeholder row: `"No tags discovered yet."` / `"No versions loaded yet."`. When a section is empty, its `ListState` must be set to `select(None)` and `Space` must be a no-op for that section.

## Navigation Within Popup

| Key | Action |
|-----|--------|
| `j` / `↓` | Move cursor down within current section; wraps to top |
| `k` / `↑` | Move cursor up within current section; wraps to bottom |
| `Tab` | Switch focus to Versions (if on Tags) or Tags (if on Versions) |
| `Shift-Tab` | Same cycle in reverse (equivalent with two sections; written to handle N sections) |
| `Space` | Toggle selection on highlighted item; no-op if section is empty |
| `a` | Clear all selections in the focused section only |
| `Backspace` | Clear all selections in the focused section (same as `a`; preserves behaviour from old version modal) |
| `Esc` / `f` | Close popup, return to Dashboard |

Cursor position is preserved per section when switching between them.

All unrecognised keys inside `FilterPopup` mode are consumed and must **not** fall through to the Dashboard handler. In particular, `/` pressed while the popup is open must be a no-op (not attempt to open `FilterInput` mode).

### `open_filter_popup()` cursor initialisation

When the popup is opened:
- For each section, if the corresponding list is non-empty and `ListState` has no selection, select index 0.
- If the list is empty, force `ListState::select(None)`.

## Filter Integration

`build_filters()` merges `selected_tags` (popup) with parsed `filter_input` (text, `t` mode):

```rust
fn build_filters(&self) -> RunnerFilters {
    let text_tags = self.filter_tags(); // parses filter_input comma-separated
    let popup_tags = (!self.selected_tags.is_empty()).then_some(self.selected_tags.clone());

    let tag_list = match (text_tags, popup_tags) {
        (Some(mut a), Some(b)) => { a.extend(b); Some(a) }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    RunnerFilters {
        tag_list,
        selected_versions: (!self.selected_versions.is_empty())
            .then_some(self.selected_versions.clone()),
        older_than_secs: self.age_filter_secs(),
        ..RunnerFilters::default()
    }
}
```

Both sources are AND-combined. Duplicates between the two sources are harmless: `runner_matches_filters` uses `all()` over the merged tag list, and each tag is checked with `any()` against `runner.tag_list`, so a repeated tag does not change the result.

## Filter Bar Hint Line and Active Summary

### Hint line (no active filters)
```
Press t to edit tags. a: age. f: filter. s: sort. c: settings.
```

### Active summary line (when any filter is active)
Show all active tag sources:
```
{filter_input_value} +{N} popup tags | age {X} | versions {Y} | sort {Z}
```

Rules:
- If `filter_input` is non-empty and `selected_tags` is non-empty: show both (text value + `+N popup tags`)
- If only `filter_input` is non-empty: show text value as before
- If only `selected_tags` is non-empty: show `{N} popup tags` using `styles::accent_style()`
- If neither: show hint line

## Status Bar

Add an arm for `AppMode::FilterPopup` in `render_status_bar()`:
```
Filter  [f/esc close]  [tab switch section]  [space toggle]  [a clear]
```

## Polling while popup is open

`AppMode::FilterPopup` suppresses background polling, consistent with the existing behaviour of `AppMode::VersionFilter`. The `should_poll_now()` guard already rejects any mode other than `AppMode::Dashboard`, so no code change is needed here. This is intentional: it prevents `apply_view_state` from firing `selected_tags.retain()` while the user is mid-interaction, which could deselect items they just toggled.

## `apply_view_state` — stale selection cleanup

After populating `tag_options` and `version_options` in `apply_view_state`, retain only selections still present in the current options:

```rust
self.selected_tags.retain(|t| self.tag_options.contains(t));
self.selected_versions.retain(|v| self.version_options.contains(v));
```

This prevents stale popup selections from silently filtering out all runners after a data refresh.

## Reset on settings save and fetch error

`save_settings()` (on success) must clear: `tag_options`, `selected_tags`, `version_options`, `selected_versions`. The GitLab host may change, making previously discovered tags/versions stale.

On fetch error, clear `tag_options` and `version_options` (same as the existing `version_options` clear). `selected_tags` and `selected_versions` should also be cleared to avoid active filters that reference options the user can no longer see.

## Rendering

Replace `render_version_filter_modal()` with `render_filter_popup()`:
- Centered modal, ~55% width × 65% height
- Two visually separated sections stacked vertically
- Active section header uses `focused_block`-style highlight; inactive uses `styles::muted_style()`
- Each section renders its list using `render_stateful_widget`; only the active section's `ListState` responds to key input
- Footer row shows key hint line

## Files Affected

- `src/tui/app.rs` — new fields, `FilterPopupSection` type, `AppMode::FilterPopup`, updated `handle_key()`, updated `build_filters()`, new `extract_runner_tags()` call in `apply_view_state`, `open_filter_popup()` method, `toggle_selected_tag()` method, stale-selection retain calls, settings-save and error-path resets
- `src/tui/ui.rs` — replace `render_version_filter_modal()` with `render_filter_popup()`, update `render_filter_bar()` hint and active summary (including title string from `[/]` to `[t]`), update `render()` dispatch, add `AppMode::FilterPopup` arm in `render_status_bar()`, update `render_help_view()` to show new bindings (`f` opens popup, `t` opens tag text input, `v` removed)
- `src/models/runner.rs` — add `extract_runner_tags()`

## Test Plan

- `t` key opens tag text input (`FilterInput` mode)
- `f` and `/` keys open filter popup (`FilterPopup` mode)
- `v` key does nothing (unbound)
- Tab switches section focus Tags → Versions → Tags; Shift-Tab reverses
- `j`/`k` navigate within focused section only, not across sections
- Space toggles selection and immediately re-applies filters; Space is no-op when section is empty
- `a` and `Backspace` clear selections in focused section only, leave other section unchanged
- `open_filter_popup()` selects index 0 for non-empty sections; forces `select(None)` for empty sections
- `extract_runner_tags()` returns deduplicated, alphabetically sorted tags from `raw_runners` only; trims and filters empty strings
- `build_filters()` AND-combines text tags and popup-selected tags
- Runner with tags `["docker","linux"]` matches `filter_input="linux"` + `selected_tags=["docker"]`
- Runner with tags `["docker"]` does not match `filter_input="linux"` + `selected_tags=["docker"]`
- Runner with tags `["docker"]` matches `filter_input="docker"` + `selected_tags=["docker"]` (duplicate tag is harmless)
- `selected_tags` and `tag_options` are cleared on settings save and fetch error
- `selected_tags.retain()` removes stale tags after a data refresh
- Filter bar shows popup-only active summary when `filter_input` is empty but `selected_tags` is non-empty
- Status bar shows `FilterPopup` hint string when popup is open
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` all pass
