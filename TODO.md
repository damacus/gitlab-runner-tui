# TODO - IGOR Development Roadmap

## High Priority Features

### 1. Advanced Filter Input UI ⏳

**Status:** Not Started
**Description:** Enhance FilterInput mode with multi-field support
**Tasks:**

- [ ] Add separate input fields for each filter type (Tags, Status, Version, Type, Paused)
- [ ] Implement Tab/Shift+Tab navigation between fields
- [ ] Add field-specific hints and validation
- [ ] Update UI to show all available filter fields per command
- [ ] Support command-specific filter availability

**Files to modify:** `src/tui/app.rs`, `src/tui/ui.rs`

---

### 2. Pagination & Asynchronous Loading ✅

**Status:** To Verify
**Description:** Ensure efficient loading of large datasets
**Tasks:**

- [x] Fetch all pages (currently implemented)
- [ ] Display first page immediately for "snappy" feedback
- [ ] Load additional pages in background
- [ ] Add "Loading more..." visual indicator
- [ ] Optimize for datasets >1000 runners

**Files to modify:** `src/conductor/mod.rs`, `src/client/mod.rs`, `src/tui/app.rs`

---

### 3. Enhanced Keyboard Navigation ⏳

**Status:** Not Started
**Description:** Add advanced navigation shortcuts
**Tasks:**

- [ ] Implement PageUp/PageDown (`u`/`d`) for page scrolling
- [ ] Implement Home/End (`g`/`G`) for jump to top/bottom
- [ ] Apply to all table views (Runners, Workers, HealthCheck)
- [ ] Add smooth scrolling behavior
- [ ] Update help text with new shortcuts

**Files to modify:** `src/main.rs`, `src/tui/app.rs`, `src/tui/ui.rs`

---

### 4. SSL/TLS & Proxy Configuration ⏳

**Status:** Not Started
**Description:** Support enterprise deployment scenarios
**Tasks:**

- [ ] Respect `DISABLE_SSL_WARNINGS` environment variable
- [ ] Support self-signed certificates
- [ ] Auto-detect `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY`
- [ ] Configurable connection/read/total timeouts
- [ ] Display warning when SSL verification disabled
- [ ] Add proxy configuration to README

**Files to modify:** `src/client/mod.rs`, `README.md`

---

## Testing & Quality

### 5. Test Coverage ⏳

**Status:** Ongoing
**Tasks:**

- [ ] Achieve ≥80% code coverage
- [ ] Add integration tests for all commands
- [ ] Add UI event handling tests
- [ ] Test error scenarios (network failures, auth errors)
- [ ] Test pagination edge cases

---

## Documentation

### 6. Documentation Improvements ⏳

**Status:** In Progress
**Tasks:**

- [x] Create README.md
- [x] Create TODO.md
- [ ] Add CONTRIBUTING.md
- [ ] Add detailed troubleshooting guide
- [ ] Add screenshots/demo GIF
- [ ] Document all environment variables
- [ ] Add usage examples

---

## Build & Deployment

### 7. CI/CD Pipeline 🚀

**Status:** In Progress
**Tasks:**

- [ ] Set up GitHub Actions workflow
- [ ] Build multi-platform binaries (Linux, macOS, Windows)
- [ ] Publish to GitHub Releases
- [ ] Optional: Docker image build and publish
- [ ] Optional: Homebrew formula
- [ ] Add versioning/release notes automation

**Files to create:** `.github/workflows/release.yml`

---

## Future Enhancements (Post-MVP)

### 8. Export Results ⏳

- [ ] Export to CSV format
- [ ] Export to JSON format
- [ ] Export to Markdown tables

### 9. Saved Filters ⏳

- [ ] Save frequently used filter combinations
- [ ] Quick filter presets
- [ ] Configuration file support (`.igor.toml`)

### 10. Search & Sort ⏳

- [ ] Full-text search within results
- [ ] Dynamic column sorting
- [ ] Filter results post-fetch

---

## Legend

- ✅ Complete
- 🚀 In Progress
- ⏳ Not Started
- ❌ Blocked
