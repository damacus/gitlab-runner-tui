# Filter Popup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the separate tag text filter (`f`/`/`) and version modal (`v`) with a unified filter popup (`f`) containing multi-select Tags and Versions sections; move the tag text input to `t`.

**Architecture:** Three files change. `src/models/runner.rs` gets a new `extract_runner_tags()` function. `src/tui/app.rs` gets new state fields, a `FilterPopupSection` type, key handler logic, and updated filter-building. `src/tui/ui.rs` gets the new popup renderer and updated filter bar, status bar, and help view strings.

**Tech Stack:** Rust, ratatui (TUI), tokio (async), crossterm (key events)

---

## Conventions used in this plan

- **Test helper:** All `app.rs` tests use the private `test_app()` function already defined in the `#[cfg(test)]` block (line ~1366). Do not write `App::new_test()`.
- **Async tests:** `handle_key` is `async`. All tests that call it must use `#[tokio::test]` and `.await`. See existing tests around line 1551 for the exact pattern.
- **Runner construction:** `Runner` does not derive `Default`. Use the private `create_test_runner(id, status, manager_status)` helper already defined in `runner.rs` tests (line ~501), then mutate fields as needed.

---

## File Map

| File | Change |
|------|--------|
| `src/models/runner.rs` | Add `extract_runner_tags()` after `extract_runner_versions()` (line 115) |
| `src/tui/app.rs` | Add `FilterPopupSection` enum; add 4 fields to `App`; update `App::new()`; add `open_filter_popup()`, `toggle_selected_tag()`, `selected_tags_summary()`; update `build_filters()`, `apply_view_state()`, `save_settings()`, error path in `execute_search()`, `handle_key()`; update existing `test_slash_focuses_filter_mode` test |
| `src/tui/ui.rs` | Add `FilterPopupSection` to `use crate::tui::app` import; replace `render_version_filter_modal()` with `render_filter_popup()`; update `render_filter_bar()` titles and hint/active text; update Dashboard arm in `render_status_bar()`; add `FilterPopup` arm in `render_status_bar()`; update `render_help_view()`; update `render()` dispatch |

---

## Task 1: `extract_runner_tags()` in model layer

**Files:**
- Modify: `src/models/runner.rs:115` (insert after `extract_runner_versions`)

- [ ] **Write failing tests**

  Add in the `#[cfg(test)]` block at the bottom of `src/models/runner.rs`. Use `create_test_runner` and mutate `tag_list`:

  ```rust
  #[test]
  fn test_extract_runner_tags_deduplicates_and_sorts_alpha() {
      let mut r1 = create_test_runner(1, "online", None);
      r1.tag_list = vec!["linux".to_owned(), "docker".to_owned()];
      let mut r2 = create_test_runner(2, "online", None);
      r2.tag_list = vec!["docker".to_owned(), "prod".to_owned()];
      let tags = extract_runner_tags(&[r1, r2]);
      assert_eq!(tags, vec!["docker", "linux", "prod"]);
  }

  #[test]
  fn test_extract_runner_tags_trims_and_drops_empty() {
      let mut r = create_test_runner(1, "online", None);
      r.tag_list = vec!["  linux  ".to_owned(), "".to_owned(), " ".to_owned()];
      let tags = extract_runner_tags(&[r]);
      assert_eq!(tags, vec!["linux"]);
  }

  #[test]
  fn test_extract_runner_tags_empty_runners() {
      let tags = extract_runner_tags(&[]);
      assert!(tags.is_empty());
  }
  ```

- [ ] **Run to confirm they fail**

  ```bash
  cargo test test_extract_runner_tags 2>&1 | tail -20
  ```

  Expected: compile error — `cannot find function extract_runner_tags`

- [ ] **Implement `extract_runner_tags()`**

  Insert after the closing brace of `extract_runner_versions` at line 115:

  ```rust
  pub fn extract_runner_tags(runners: &[Runner]) -> Vec<String> {
      let mut tags: Vec<String> = runners
          .iter()
          .flat_map(|runner| runner.tag_list.iter())
          .map(|tag| tag.trim())
          .filter(|tag| !tag.is_empty())
          .map(ToOwned::to_owned)
          .collect();

      tags.sort();
      tags.dedup();
      tags
  }
  ```

- [ ] **Run tests to confirm pass**

  ```bash
  cargo test test_extract_runner_tags 2>&1 | tail -10
  ```

  Expected: 3 tests pass, 0 failures.

- [ ] **Commit**

  ```bash
  git add src/models/runner.rs
  git commit -m "feat: add extract_runner_tags function"
  ```

---

## Task 2: New app state — `FilterPopupSection`, fields, constructor

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Write failing test**

  Add in the `#[cfg(test)]` block at the bottom of `src/tui/app.rs`:

  ```rust
  #[test]
  fn test_app_initial_filter_popup_fields() {
      let app = test_app();
      assert_eq!(app.filter_popup_section, FilterPopupSection::Tags);
      assert!(app.tag_options.is_empty());
      assert!(app.selected_tags.is_empty());
      assert_eq!(app.tag_list_state.selected(), None);
  }
  ```

- [ ] **Run to confirm failure**

  ```bash
  cargo test test_app_initial_filter_popup_fields 2>&1 | tail -10
  ```

  Expected: compile error referencing missing field and type.

- [ ] **Add `FilterPopupSection` enum**

  In `src/tui/app.rs`, insert after the `AppMode` enum definition (after line ~138):

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub enum FilterPopupSection {
      #[default]
      Tags,
      Versions,
  }
  ```

- [ ] **Replace `AppMode::VersionFilter` with `AppMode::FilterPopup`**

  In the `AppMode` enum (lines 130–138), replace `VersionFilter` with `FilterPopup`:

  ```rust
  pub enum AppMode {
      #[default]
      Dashboard,
      FilterInput,
      AgeInput,
      FilterPopup,
      Settings,
      Help,
  }
  ```

- [ ] **Add new fields to `App` struct**

  In the `App` struct definition, after `version_list_state: ListState,` (line ~330), add:

  ```rust
  pub filter_popup_section: FilterPopupSection,
  pub tag_options: Vec<String>,
  pub selected_tags: Vec<String>,
  pub tag_list_state: ListState,
  ```

- [ ] **Initialise new fields in `App::new()`**

  In the `Self { ... }` constructor block (lines ~356–388), after `version_list_state: ListState::default(),` add:

  ```rust
  filter_popup_section: FilterPopupSection::default(),
  tag_options: Vec::new(),
  selected_tags: Vec::new(),
  tag_list_state: ListState::default(),
  ```

- [ ] **Run tests**

  ```bash
  cargo test test_app_initial_filter_popup_fields 2>&1 | tail -10
  ```

  Expected: 1 test passes. Then run the full suite:

  ```bash
  cargo test 2>&1 | tail -20
  ```

  Some tests referencing `VersionFilter` or `open_version_filter` may now fail — that is expected and will be resolved in Task 5.

- [ ] **Commit**

  ```bash
  git add src/tui/app.rs
  git commit -m "feat: add FilterPopupSection type and filter popup state fields to App"
  ```

---

## Task 3: `open_filter_popup()`, `toggle_selected_tag()`, `selected_tags_summary()`

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Write failing tests**

  ```rust
  #[test]
  fn test_open_filter_popup_selects_first_item_when_tags_available() {
      let mut app = test_app();
      app.tag_options = vec!["docker".to_owned(), "linux".to_owned()];
      app.tag_list_state.select(None);
      app.open_filter_popup();
      assert_eq!(app.mode, AppMode::FilterPopup);
      assert_eq!(app.tag_list_state.selected(), Some(0));
  }

  #[test]
  fn test_open_filter_popup_none_when_tags_empty() {
      let mut app = test_app();
      app.tag_options = vec![];
      app.open_filter_popup();
      assert_eq!(app.tag_list_state.selected(), None);
  }

  #[test]
  fn test_toggle_selected_tag_adds_and_removes() {
      let mut app = test_app();
      app.tag_options = vec!["docker".to_owned(), "linux".to_owned()];
      app.tag_list_state.select(Some(0));
      app.toggle_selected_tag();
      assert_eq!(app.selected_tags, vec!["docker"]);
      app.toggle_selected_tag(); // toggle off
      assert!(app.selected_tags.is_empty());
  }

  #[test]
  fn test_selected_tags_summary_empty_and_non_empty() {
      let mut app = test_app();
      assert_eq!(app.selected_tags_summary(), "all tags");
      app.selected_tags = vec!["docker".to_owned(), "linux".to_owned()];
      assert_eq!(app.selected_tags_summary(), "2 selected");
  }
  ```

- [ ] **Run to confirm failure**

  ```bash
  cargo test "test_open_filter_popup|test_toggle_selected_tag|test_selected_tags_summary" 2>&1 | tail -15
  ```

- [ ] **Add `open_filter_popup()` — delete `open_version_filter()` in the same step**

  In `src/tui/app.rs`, find `open_version_filter()` (lines ~474–482). Replace it entirely with `open_filter_popup()`:

  ```rust
  pub fn open_filter_popup(&mut self) {
      self.mode = AppMode::FilterPopup;
      // Tags section cursor
      if self.tag_options.is_empty() {
          self.tag_list_state.select(None);
      } else if self.tag_list_state.selected().is_none() {
          self.tag_list_state.select(Some(0));
      }
      // Versions section cursor
      if self.version_options.is_empty() {
          self.version_list_state.select(None);
      } else if self.version_list_state.selected().is_none() {
          self.version_list_state.select(Some(0));
      }
      self.error_message = None;
  }
  ```

- [ ] **Add `toggle_selected_tag()` method**

  After `toggle_selected_version()` (line ~880), add:

  ```rust
  pub fn toggle_selected_tag(&mut self) {
      let Some(index) = self.tag_list_state.selected() else {
          return;
      };
      let Some(tag) = self.tag_options.get(index).cloned() else {
          return;
      };

      if let Some(existing_index) = self.selected_tags.iter().position(|t| t == &tag) {
          self.selected_tags.remove(existing_index);
      } else {
          self.selected_tags.push(tag);
          self.selected_tags.sort();
      }

      if self.has_loaded_active_tab() {
          self.apply_view_state(self.active_tab());
      }
  }
  ```

- [ ] **Add `selected_tags_summary()` method**

  After `selected_versions_summary()` (line ~517), add:

  ```rust
  pub fn selected_tags_summary(&self) -> String {
      if self.selected_tags.is_empty() {
          "all tags".to_string()
      } else {
          format!("{} selected", self.selected_tags.len())
      }
  }
  ```

- [ ] **Run tests**

  ```bash
  cargo test "test_open_filter_popup|test_toggle_selected_tag|test_selected_tags_summary" 2>&1 | tail -15
  ```

  Expected: all 4 pass.

- [ ] **Commit**

  ```bash
  git add src/tui/app.rs
  git commit -m "feat: add open_filter_popup, toggle_selected_tag, selected_tags_summary"
  ```

---

## Task 4: Update `build_filters()` and `apply_view_state()` retain

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Write failing tests**

  ```rust
  #[test]
  fn test_build_filters_merges_text_and_popup_tags() {
      let mut app = test_app();
      app.filter_input = "linux".to_owned();
      app.selected_tags = vec!["docker".to_owned()];
      let filters = app.build_filters();
      let tags = filters.tag_list.unwrap();
      assert!(tags.contains(&"linux".to_owned()));
      assert!(tags.contains(&"docker".to_owned()));
  }

  #[test]
  fn test_build_filters_popup_tags_only() {
      let mut app = test_app();
      app.selected_tags = vec!["docker".to_owned()];
      let filters = app.build_filters();
      assert_eq!(filters.tag_list, Some(vec!["docker".to_owned()]));
  }

  #[test]
  fn test_build_filters_text_tags_only() {
      let mut app = test_app();
      app.filter_input = "linux".to_owned();
      let filters = app.build_filters();
      assert_eq!(filters.tag_list, Some(vec!["linux".to_owned()]));
  }

  #[test]
  fn test_build_filters_no_tags_returns_none() {
      let app = test_app();
      let filters = app.build_filters();
      assert!(filters.tag_list.is_none());
  }

  #[test]
  fn test_build_filters_duplicate_tag_is_harmless() {
      let mut app = test_app();
      app.filter_input = "docker".to_owned();
      app.selected_tags = vec!["docker".to_owned()];
      let filters = app.build_filters();
      // duplicates from both sources are passed through; runner_matches_filters handles them
      let tags = filters.tag_list.unwrap();
      assert_eq!(tags.iter().filter(|t| t.as_str() == "docker").count(), 2);
  }
  ```

- [ ] **Run to confirm failure**

  ```bash
  cargo test "test_build_filters" 2>&1 | tail -15
  ```

- [ ] **Replace `build_filters()` body**

  Find `build_filters()` at line ~496 and replace its body:

  ```rust
  fn build_filters(&self) -> RunnerFilters {
      let text_tags = self.filter_tags();
      let popup_tags = (!self.selected_tags.is_empty()).then_some(self.selected_tags.clone());

      let tag_list = match (text_tags, popup_tags) {
          (Some(mut a), Some(b)) => {
              a.extend(b);
              Some(a)
          }
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

- [ ] **Add `tag_options` population and `selected_tags` retain in `apply_view_state()`**

  First confirm the import: find the `use crate::models::runner::` line at the top of `app.rs` and add `extract_runner_tags` alongside `extract_runner_versions`.

  In `apply_view_state()` (line ~738), the existing code is:

  ```rust
  self.version_options = extract_runner_versions(&self.raw_runners);
  self.selected_versions
      .retain(|version| self.version_options.contains(version));
  ```

  Extend it:

  ```rust
  self.version_options = extract_runner_versions(&self.raw_runners);
  self.selected_versions
      .retain(|version| self.version_options.contains(version));
  self.tag_options = extract_runner_tags(&self.raw_runners);
  self.selected_tags
      .retain(|tag| self.tag_options.contains(tag));
  ```

- [ ] **Run tests**

  ```bash
  cargo test "test_build_filters" 2>&1 | tail -15
  ```

  Expected: all 5 pass. Then full suite:

  ```bash
  cargo test 2>&1 | tail -20
  ```

- [ ] **Commit**

  ```bash
  git add src/tui/app.rs
  git commit -m "feat: merge popup and text tags in build_filters, retain selected_tags in apply_view_state"
  ```

---

## Task 5: Reset paths and key handler

**Files:**
- Modify: `src/tui/app.rs`

- [ ] **Write failing tests**

  All key handler tests are async — use `#[tokio::test]` and `.await`:

  ```rust
  #[tokio::test]
  async fn test_f_key_opens_filter_popup() {
      let mut app = test_app();
      app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::FilterPopup);
  }

  #[tokio::test]
  async fn test_slash_key_opens_filter_popup() {
      let mut app = test_app();
      app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::FilterPopup);
  }

  #[tokio::test]
  async fn test_t_key_opens_filter_input() {
      let mut app = test_app();
      app.handle_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::FilterInput);
  }

  #[tokio::test]
  async fn test_v_key_does_nothing_in_dashboard() {
      let mut app = test_app();
      app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::Dashboard);
  }

  #[tokio::test]
  async fn test_esc_closes_filter_popup() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::Dashboard);
  }

  #[tokio::test]
  async fn test_f_key_closes_filter_popup() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.handle_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::Dashboard);
  }

  #[tokio::test]
  async fn test_slash_inside_filter_popup_is_noop() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)).await;
      assert_eq!(app.mode, AppMode::FilterPopup);
  }

  #[tokio::test]
  async fn test_tab_switches_section_tags_to_versions() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.filter_popup_section = FilterPopupSection::Tags;
      app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)).await;
      assert_eq!(app.filter_popup_section, FilterPopupSection::Versions);
  }

  #[tokio::test]
  async fn test_tab_switches_section_versions_to_tags() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.filter_popup_section = FilterPopupSection::Versions;
      app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)).await;
      assert_eq!(app.filter_popup_section, FilterPopupSection::Tags);
  }

  #[tokio::test]
  async fn test_a_clears_focused_tags_section_only() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.filter_popup_section = FilterPopupSection::Tags;
      app.selected_tags = vec!["docker".to_owned()];
      app.selected_versions = vec!["17.5.0".to_owned()];
      app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)).await;
      assert!(app.selected_tags.is_empty());
      assert_eq!(app.selected_versions, vec!["17.5.0".to_owned()]);
  }

  #[tokio::test]
  async fn test_backspace_clears_focused_versions_section_only() {
      let mut app = test_app();
      app.mode = AppMode::FilterPopup;
      app.filter_popup_section = FilterPopupSection::Versions;
      app.selected_tags = vec!["docker".to_owned()];
      app.selected_versions = vec!["17.5.0".to_owned()];
      app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)).await;
      assert!(app.selected_versions.is_empty());
      assert_eq!(app.selected_tags, vec!["docker".to_owned()]);
  }
  ```

- [ ] **Update the existing `test_slash_focuses_filter_mode` test**

  Find `test_slash_focuses_filter_mode` (line ~1551). It currently asserts `AppMode::FilterInput`. Update it to assert `AppMode::FilterPopup`:

  ```rust
  // Before:
  assert_eq!(app.mode, AppMode::FilterInput);
  // After:
  assert_eq!(app.mode, AppMode::FilterPopup);
  ```

- [ ] **Run to confirm the new tests fail (and the updated test now matches expected behaviour)**

  ```bash
  cargo test "test_f_key|test_slash|test_t_key|test_v_key|test_esc_closes|test_tab_switches|test_a_clears|test_backspace_clears|test_slash_inside" 2>&1 | tail -20
  ```

- [ ] **Remove the `VersionFilter` key handler block**

  Find `if self.mode == AppMode::VersionFilter { ... }` (lines ~1064–1106) and delete it entirely.

- [ ] **Add the `FilterPopup` key handler block**

  Insert where the `VersionFilter` block was, **before** the Dashboard handler, so it returns early:

  ```rust
  if self.mode == AppMode::FilterPopup {
      match key.code {
          KeyCode::Esc | KeyCode::Char('f') => {
              self.mode = AppMode::Dashboard;
          }
          KeyCode::Tab | KeyCode::BackTab => {
              self.filter_popup_section = match self.filter_popup_section {
                  FilterPopupSection::Tags => FilterPopupSection::Versions,
                  FilterPopupSection::Versions => FilterPopupSection::Tags,
              };
          }
          KeyCode::Up | KeyCode::Char('k') => match self.filter_popup_section {
              FilterPopupSection::Tags => {
                  let len = self.tag_options.len();
                  if len > 0 {
                      let next = match self.tag_list_state.selected() {
                          Some(0) | None => len - 1,
                          Some(i) => i - 1,
                      };
                      self.tag_list_state.select(Some(next));
                  }
              }
              FilterPopupSection::Versions => {
                  let len = self.version_options.len();
                  if len > 0 {
                      let next = match self.version_list_state.selected() {
                          Some(0) | None => len - 1,
                          Some(i) => i - 1,
                      };
                      self.version_list_state.select(Some(next));
                  }
              }
          },
          KeyCode::Down | KeyCode::Char('j') => match self.filter_popup_section {
              FilterPopupSection::Tags => {
                  let len = self.tag_options.len();
                  if len > 0 {
                      let next = match self.tag_list_state.selected() {
                          Some(i) if i + 1 < len => i + 1,
                          _ => 0,
                      };
                      self.tag_list_state.select(Some(next));
                  }
              }
              FilterPopupSection::Versions => {
                  let len = self.version_options.len();
                  if len > 0 {
                      let next = match self.version_list_state.selected() {
                          Some(i) if i + 1 < len => i + 1,
                          _ => 0,
                      };
                      self.version_list_state.select(Some(next));
                  }
              }
          },
          KeyCode::Char(' ') => match self.filter_popup_section {
              FilterPopupSection::Tags => self.toggle_selected_tag(),
              FilterPopupSection::Versions => self.toggle_selected_version(),
          },
          KeyCode::Char('a') | KeyCode::Backspace => match self.filter_popup_section {
              FilterPopupSection::Tags => {
                  self.selected_tags.clear();
                  if self.has_loaded_active_tab() {
                      self.apply_view_state(self.active_tab());
                  }
              }
              FilterPopupSection::Versions => {
                  self.selected_versions.clear();
                  if self.has_loaded_active_tab() {
                      self.apply_view_state(self.active_tab());
                  }
              }
          },
          _ => {} // all other keys consumed; no fall-through to Dashboard
      }
      return;
  }
  ```

- [ ] **Update Dashboard key bindings**

  In the Dashboard `match key.code { ... }` block:

  1. Change `KeyCode::Char('/') | KeyCode::Char('f')` (line ~1249) to call `open_filter_popup()`:
     ```rust
     KeyCode::Char('/') | KeyCode::Char('f') => {
         self.open_filter_popup();
     }
     ```
  2. Add a `t` arm **before** the `/`/`f` arm:
     ```rust
     KeyCode::Char('t') => {
         self.focus_filter();
     }
     ```
  3. **Delete** the `KeyCode::Char('v')` arm (lines ~1211–1213).

- [ ] **Clear tag state in `save_settings()` success path**

  In `save_settings()`, in the `Ok(...)` arm (line ~938–954), after `self.selected_versions.clear();` add:

  ```rust
  self.tag_options.clear();
  self.selected_tags.clear();
  ```

- [ ] **Clear tag state in `execute_search()` error path**

  In the `Err(error)` branch (line ~707–724), after `self.version_options.clear();` add:

  ```rust
  self.tag_options.clear();
  self.selected_tags.clear();
  self.selected_versions.clear();
  ```

- [ ] **Run tests**

  ```bash
  cargo test "test_f_key|test_slash|test_t_key|test_v_key|test_esc_closes|test_tab_switches|test_a_clears|test_backspace_clears|test_slash_inside" 2>&1 | tail -20
  ```

  Expected: all pass. Then full suite:

  ```bash
  cargo test 2>&1 | tail -20
  ```

- [ ] **Commit**

  ```bash
  git add src/tui/app.rs
  git commit -m "feat: add FilterPopup key handler, update dashboard bindings (f popup, t text, remove v)"
  ```

---

## Task 6: Rendering — filter popup, filter bar, status bar, help

**Files:**
- Modify: `src/tui/ui.rs`

- [ ] **Add `FilterPopupSection` to the `use crate::tui::app` import**

  Near the top of `src/tui/ui.rs`, find the line that imports from `crate::tui::app` (imports `App`, `AppMode`, etc.) and add `FilterPopupSection` to it. Example:

  ```rust
  use crate::tui::app::{App, AppMode, FilterPopupSection, /* ...existing items... */};
  ```

- [ ] **Replace `render_version_filter_modal()` with `render_filter_popup()`**

  Delete `render_version_filter_modal()` entirely (lines 780–807) and replace with:

  ```rust
  fn render_filter_popup(app: &mut App, frame: &mut Frame) {
      let area = centered_rect(55, 65, frame.size());
      frame.render_widget(Clear, area);

      let sections = Layout::default()
          .direction(Direction::Vertical)
          .constraints([
              Constraint::Percentage(47),
              Constraint::Percentage(47),
              Constraint::Length(1),
          ])
          .split(area);

      // --- Tags section ---
      let tags_focused = app.filter_popup_section == FilterPopupSection::Tags;
      let tags_title = if tags_focused { "▶ Tags" } else { "  Tags" };
      let tag_items: Vec<ListItem> = if app.tag_options.is_empty() {
          vec![ListItem::new("No tags discovered yet.")]
      } else {
          app.tag_options
              .iter()
              .map(|tag| {
                  let marker = if app.selected_tags.contains(tag) { "[x]" } else { "[ ]" };
                  ListItem::new(format!("{marker} {tag}"))
              })
              .collect()
      };
      let tags_list = List::new(tag_items)
          .highlight_style(if tags_focused { styles::selected_row_style() } else { styles::muted_style() })
          .block(if tags_focused { styles::focused_block(tags_title) } else { styles::block(tags_title) });
      if tags_focused {
          frame.render_stateful_widget(tags_list, sections[0], &mut app.tag_list_state);
      } else {
          frame.render_widget(tags_list, sections[0]);
      }

      // --- Versions section ---
      let versions_focused = app.filter_popup_section == FilterPopupSection::Versions;
      let versions_title = if versions_focused { "▶ Versions" } else { "  Versions" };
      let version_items: Vec<ListItem> = if app.version_options.is_empty() {
          vec![ListItem::new("No versions loaded yet.")]
      } else {
          app.version_options
              .iter()
              .map(|version| {
                  let marker = if app.selected_versions.contains(version) { "[x]" } else { "[ ]" };
                  ListItem::new(format!("{marker} {version}"))
              })
              .collect()
      };
      let versions_list = List::new(version_items)
          .highlight_style(if versions_focused { styles::selected_row_style() } else { styles::muted_style() })
          .block(if versions_focused { styles::focused_block(versions_title) } else { styles::block(versions_title) });
      if versions_focused {
          frame.render_stateful_widget(versions_list, sections[1], &mut app.version_list_state);
      } else {
          frame.render_widget(versions_list, sections[1]);
      }

      // --- Footer ---
      let footer = Paragraph::new("space:toggle  tab:switch section  a:clear section  esc:close")
          .style(styles::muted_style());
      frame.render_widget(footer, sections[2]);
  }
  ```

  > **Note on `render_stateful_widget` vs `render_widget`:** Only the focused section's `ListState` should respond to highlight rendering. The unfocused section renders as a plain widget to avoid the highlight style being applied to a stale cursor position.

- [ ] **Update `render()` dispatch**

  In the `render()` function (around line 58–60), change:

  ```rust
  AppMode::VersionFilter => render_version_filter_modal(app, frame),
  ```

  to:

  ```rust
  AppMode::FilterPopup => render_filter_popup(app, frame),
  ```

- [ ] **Update `render_filter_bar()` titles and hint/active text**

  Replace the function body (lines 183–235) with:

  ```rust
  fn render_filter_bar(app: &App, frame: &mut Frame, area: Rect) {
      let title = if app.mode == AppMode::FilterInput {
          "Filter Tags [t] (focused)"
      } else if app.mode == AppMode::AgeInput {
          "Age Filter [a] (focused)"
      } else {
          "Filter Tags [t]"
      };

      let has_text = !app.filter_input.is_empty();
      let has_popup_tags = !app.selected_tags.is_empty();

      let (text, style) = if !has_text && !has_popup_tags {
          (
              format!(
                  "Press t to edit tags. a: age {}. f: filter. s: sort {}. c: settings.",
                  app.age_filter_summary(),
                  app.sort_label()
              ),
              styles::muted_style(),
          )
      } else {
          let tag_part = match (has_text, has_popup_tags) {
              (true, true) => format!(
                  "{} +{} popup tags",
                  app.filter_input,
                  app.selected_tags.len()
              ),
              (true, false) => app.filter_input.clone(),
              (false, true) => format!("{} popup tags", app.selected_tags.len()),
              (false, false) => unreachable!(),
          };
          (
              format!(
                  "{} | age {} | versions {} | sort {}",
                  tag_part,
                  app.age_filter_summary(),
                  app.selected_versions_summary(),
                  app.sort_label()
              ),
              styles::accent_style(),
          )
      };

      let block = if app.mode == AppMode::FilterInput {
          styles::focused_block(title)
      } else {
          styles::block(title)
      };

      let paragraph = Paragraph::new(text).style(style).block(block);
      frame.render_widget(paragraph, area);

      if app.mode == AppMode::FilterInput {
          frame.set_cursor(
              area.x + app.filter_input.chars().count() as u16 + 1,
              area.y + 1,
          );
      } else if app.mode == AppMode::AgeInput {
          frame.set_cursor(
              area.x + app.age_filter_input.chars().count() as u16 + 1,
              area.y + 1,
          );
      }
  }
  ```

- [ ] **Update `render_status_bar()` — replace `VersionFilter` arm, add `FilterPopup` arm, update Dashboard hint string**

  In `render_status_bar()` (line ~1002):

  1. Replace the `VersionFilter` arm:
     ```rust
     // Remove this:
     AppMode::VersionFilter => {
         "Version filter | ↑/↓ move | Space toggle | a/Backspace clear | Enter/Esc close"
             .to_string()
     }
     // Add this:
     AppMode::FilterPopup => {
         "Filter | f/esc close | tab switch section | space toggle | a clear section"
             .to_string()
     }
     ```

  2. In the `AppMode::Dashboard` arm, find the hint string on line ~1050:
     ```
     "... | / tags | a age | v versions | s sort | c settings | ..."
     ```
     Update it to:
     ```
     "... | f filter | t tags | a age | s sort | c settings | ..."
     ```
     (Keep `r refresh | p poll | ?: help | q/Ctrl-C quit` as-is.)

- [ ] **Update `render_help_view()` keybinding text**

  Replace the Actions and Filtering sections (lines ~1083–1097):

  ```rust
  Line::from("Actions"),
  Line::from("  Enter            Apply the current filter"),
  Line::from("  r                Refresh the active tab"),
  Line::from("  p                Toggle polling / auto-refresh"),
  Line::from("  a                Edit age filter (24h, 7d, 90m)"),
  Line::from("  f or /           Open filter popup (tags + versions multi-select)"),
  Line::from("  t                Edit tag text filter (comma-separated)"),
  Line::from("  s                Cycle sort mode"),
  Line::from("  c                Open settings and diagnostics"),
  Line::from("  q or Ctrl-C      Quit"),
  Line::from(""),
  Line::from("Filtering"),
  Line::from("  t                Focus the tag text filter bar"),
  Line::from("  Type tags        Edit comma-separated tag filters"),
  Line::from("  Enter            Apply tag filter to the active tab"),
  Line::from("  Esc              Exit filter editing"),
  ```

- [ ] **Build to catch compile errors**

  ```bash
  cargo build 2>&1 | head -40
  ```

  Fix any remaining `VersionFilter` references:

  ```bash
  grep -rn "VersionFilter\|render_version_filter\|open_version_filter" src/
  ```

  Expected: no output.

- [ ] **Run full test suite**

  ```bash
  cargo test 2>&1 | tail -20
  ```

  Expected: all tests pass.

- [ ] **Run lints and format**

  ```bash
  cargo fmt
  cargo clippy --all-targets -- -D warnings 2>&1
  ```

  Fix any warnings.

- [ ] **Commit**

  ```bash
  git add src/tui/ui.rs
  git commit -m "feat: add render_filter_popup, update filter bar, status bar, help view"
  ```

---

## Task 7: Final verification

- [ ] **Confirm no `VersionFilter` references remain**

  ```bash
  grep -rn "VersionFilter\|render_version_filter\|open_version_filter" src/
  ```

  Expected: no output.

- [ ] **Confirm `v` is unbound from Dashboard**

  ```bash
  grep -n "Char('v')" src/tui/app.rs
  ```

  Expected: only in the `Settings` modal handler (where `t`/`T`/`v`/`V` toggle discovery mode) — no standalone Dashboard binding.

- [ ] **Run full suite one final time**

  ```bash
  cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
  ```

  Expected: clean output, all tests pass.

- [ ] **Final commit**

  ```bash
  git add src/tui/app.rs src/tui/ui.rs src/models/runner.rs
  git commit -m "chore: final cleanup for filter popup feature"
  ```
