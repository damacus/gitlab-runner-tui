# IGOR - Copilot Instructions

## Project Overview

**IGOR** (Interactive GitLab Object Retriever) is a Rust TUI application for querying GitLab Runners via the GitLab API v4. Built with `ratatui` + `crossterm` for the terminal UI and `tokio`/`reqwest` for async HTTP.

**Specification**: [app_spec.txt](../app_spec.txt) is the authoritative specification for all features and behavior.

## Architecture (Three-Layer Design)

```
TUI (View) → Conductor (Business Logic) → GitLabClient (API)
```

1. **TUI Layer** ([src/tui/](src/tui/)): Pure presentation - handles keyboard events, renders UI, manages app modes (`CommandSelection`, `FilterInput`, `ResultsView`, `Help`)
2. **Conductor** ([src/conductor/mod.rs](src/conductor/mod.rs)): Orchestrates operations, applies client-side filtering, fetches runners + their managers
3. **GitLabClient** ([src/client/mod.rs](src/client/mod.rs)): HTTP communication with GitLab API, pagination, authentication via `PRIVATE-TOKEN` header

## Key Data Models

Located in [src/models/](src/models/):

- `Runner`: GitLab runner with nested `Vec<RunnerManager>` (managers fetched separately and attached)
- `RunnerManager`: Individual runner process with status/contact info
- `RunnerFilters`: Query filters (some API-side, some client-side)

**Important**: `tag_list` and `version_prefix` filters are applied **client-side** in the Conductor, not via API query params.

## Development Commands

```bash
# Build
cargo build --release    # Binary output: target/release/igor

# Run (requires GitLab credentials)
GITLAB_HOST=https://gitlab.example.com GITLAB_TOKEN=xxx cargo run

# Tests
cargo test
```

## Configuration

Environment variables (also supports `.env` file via `dotenvy`):

- `GITLAB_HOST`: GitLab instance URL (required)
- `GITLAB_TOKEN`: Personal access token with `read_api` scope (required)

CLI args (`--host`, `--token`) override env vars.

## Code Patterns

### Adding New Commands

Commands are runner-focused. Current commands:

- `fetch`: Fetch all runners with optional filters
- `lights`: Health check - verify all tagged runners are online
- `switch`: List runners with no online managers
- `workers`: Show detailed list of Runner Managers
- `flames`: Find runners not contacted recently
- `empty`: List runners with no managers

1. Add command name to `App.commands` vector in [src/tui/app.rs](src/tui/app.rs#L35)
2. Handle the command in `execute_search()` match block
3. Add corresponding Conductor method if needed

### API Endpoints Pattern

```rust
// In GitLabClient: build request with auth header
fn request(&self, method: Method, endpoint: &str) -> RequestBuilder {
    self.client.request(method, &url).header("PRIVATE-TOKEN", &self.token)
}
```

### Error Handling

Use `anyhow::Result` throughout. Context errors with `.context("message")` for better error chains.

### Async Pattern

All API calls are async. The TUI uses a tick-based event loop with `tokio::select!` for concurrent event/timer handling in [src/tui/event.rs](src/tui/event.rs).

## Testing

- Unit tests use `mockito` for HTTP mocking
- Tests are co-located in modules (e.g., `#[cfg(test)] mod tests` in each file)
- Model deserialization tests verify JSON parsing matches GitLab API responses

## Git Workflow

Follow **GitHub Flow**:

1. Create feature branch from `main`
2. Make changes with descriptive commits
3. Open PR for review
4. Merge to `main` after approval

## Deployment

- Binary published to **GitHub Releases**
- Build: `cargo build --release` → `target/release/igor`

## Logging

Logs go to `logs/igor.log` (rolling daily) via `tracing-appender`. Logs are disabled in terminal to avoid TUI interference.

## Keyboard Navigation

- `j/k` or `↑/↓`: Navigate lists/tables
- `Enter`: Select/confirm
- `Esc`: Go back/cancel
- `q`: Quit
- `?`: Toggle help
