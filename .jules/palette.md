## 2024-05-24 - Add empty states to TUI data tables
**Learning:** Empty tables in TUIs without explicit empty states can leave users wondering if the app is still loading, if it failed silently, or if the query simply returned zero results. Providing explicit empty state messages with actionable keyboard hints significantly reduces ambiguity and improves keyboard navigability.
**Action:** Always implement explicit empty state views for list/table components in terminal applications rather than rendering blank headers or empty grids.
## 2024-05-14 - TUI Input Cursor Placement
**Learning:** In Rust TUI applications, when using `frame.set_cursor()` to manually position the hardware cursor after input text, using `String::len()` will cause the cursor to jump ahead incorrectly if the user types multi-byte non-ASCII characters (e.g., emojis or accents). `String::len()` returns the byte length, not the character count.
**Action:** Always use `.chars().count()` (or a visual width crate like `unicode-width` if full width calculation is required) when calculating cursor coordinates based on string lengths to ensure correct alignment for all inputs.
## 2025-03-14 - Inline Command Descriptions
**Learning:** Providing inline descriptions for bespoke subcommands directly in the selection UI significantly reduces cognitive load by eliminating the need for users to open a separate help menu to understand what each command does.
**Action:** Include static inline descriptions for command/menu selections in terminal applications instead of rendering just the command names.
