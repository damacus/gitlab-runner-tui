# TODO - GitLab Runner TUI Development Roadmap

## High Priority Features

### 1. Advanced Filter Input UI ⏳

**Status:** Not Started
**Description:** Enhance FilterInput mode with multi-field support (Status, Version, Type, Paused)

- [ ] Add separate input fields for additional filter types
- [ ] Support command-specific filter availability

**Files to modify:** `src/tui/app.rs`, `src/tui/ui.rs`

---

### 2. Streaming Pagination ⏳

**Status:** Not Started
**Description:** Display first page immediately while loading remaining pages in background

- [x] Fetch all pages with pagination
- [x] Enrich runners with detail + managers in parallel (buffer_unordered)
- [ ] Display first page immediately for "snappy" feedback
- [ ] Add "Loading more..." visual indicator

**Files to modify:** `src/conductor/mod.rs`, `src/tui/app.rs`

---

### 3. Enhanced Keyboard Navigation ⏳

**Status:** Not Started

- [ ] PageUp/PageDown (`u`/`d`) for page scrolling
- [ ] Home/End (`g`/`G`) for jump to top/bottom
- [ ] Update help text with new shortcuts

**Files to modify:** `src/tui/app.rs`, `src/tui/ui.rs`

---

### 4. SSL/TLS & Proxy Configuration ⏳

**Status:** Not Started

- [ ] Support self-signed certificates
- [ ] Auto-detect `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`
- [ ] Configurable timeouts

**Files to modify:** `src/client/mod.rs`, `README.md`

---

## Testing & Quality

### 5. Test Coverage 🚀

**Status:** Ongoing (36 tests)

- [x] Client tests (fetch_runners, fetch_runner_detail, fetch_runner_managers, error cases)
- [x] Conductor tests (enrichment pipeline, offline filtering, no-managers filtering)
- [x] Model deserialization tests
- [ ] Achieve ≥80% code coverage
- [ ] Add UI rendering tests
- [ ] Test pagination edge cases (>100 runners)

---

## Build & Deployment

### 6. CI/CD Pipeline ⏳

**Status:** Not Started

- [ ] Set up GitHub Actions workflow
- [ ] Build multi-platform binaries (Linux, macOS)
- [ ] Publish to GitHub Releases

**Files to create:** `.github/workflows/release.yml`

---

## Future Enhancements

- [ ] Export results to CSV/JSON
- [ ] Saved filter presets
- [ ] Full-text search within results
- [ ] Dynamic column sorting

---

## Legend

- ✅ Complete
- 🚀 In Progress
- ⏳ Not Started
