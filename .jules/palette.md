## 2024-05-24 - Add empty states to TUI data tables
**Learning:** Empty tables in TUIs without explicit empty states can leave users wondering if the app is still loading, if it failed silently, or if the query simply returned zero results. Providing explicit empty state messages with actionable keyboard hints significantly reduces ambiguity and improves keyboard navigability.
**Action:** Always implement explicit empty state views for list/table components in terminal applications rather than rendering blank headers or empty grids.
## 2024-05-14 - TUI Input Cursor Placement
**Learning:** In Rust TUI applications, when using `frame.set_cursor()` to manually position the hardware cursor after input text, using `String::len()` will cause the cursor to jump ahead incorrectly if the user types multi-byte non-ASCII characters (e.g., emojis or accents). `String::len()` returns the byte length, not the character count.
**Action:** Always use `.chars().count()` (or a visual width crate like `unicode-width` if full width calculation is required) when calculating cursor coordinates based on string lengths to ensure correct alignment for all inputs.
## 2024-03-07 - Inline Command Descriptions in TUI
**Learning:** Command-line interfaces and TUIs with bespoke subcommands (like `lights`, `flames`) can be opaque to new users. Forcing users to toggle a help menu or read external documentation interrupts their workflow. Providing inline descriptions directly in the selection menu significantly reduces cognitive load and improves discoverability.
**Action:** Always include succinct, inline descriptions for command or menu selections in TUI applications, rather than relying solely on separate help screens.
