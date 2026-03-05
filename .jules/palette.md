## 2024-05-24 - Add empty states to TUI data tables
**Learning:** Empty tables in TUIs without explicit empty states can leave users wondering if the app is still loading, if it failed silently, or if the query simply returned zero results. Providing explicit empty state messages with actionable keyboard hints significantly reduces ambiguity and improves keyboard navigability.
**Action:** Always implement explicit empty state views for list/table components in terminal applications rather than rendering blank headers or empty grids.
