# GitLab Runner TUI

A fast, beautiful Terminal User Interface (TUI) for querying and inspecting GitLab Runners.

## Overview

GitLab Runner TUI provides DevOps engineers and GitLab administrators with an intuitive command-line interface to monitor and manage GitLab Runner infrastructure. Query runners by tags—all from your terminal.

## Preview

![Overview — seven dashboard tabs](docs/screenshots/overview.gif)

*Seven dashboard tabs — navigate with `1`–`7` or `Tab`*

![Runner detail pane](docs/screenshots/detail.gif)

*Runner detail pane — arrow through runners to inspect status, version, managers, and tags*

## Features

- 🚀 **Interactive TUI** - Beautiful, keyboard-driven interface built with [ratatui](https://ratatui.rs/)
- 📊 **Seven Dashboard Tabs** - Specialized views for different runner and infrastructure metrics
- 🔍 **Command Mode** - Run one-shot JSON queries for CI/CD or automation
- 🏷️ **Tag Filtering** - Filter runners by comma-separated tags
- ⚡ **Real-time API Queries** - Direct integration with GitLab REST API v4
- 📊 **Detailed Results** - Tabular display of runners and managers with color highlighting
- 🔐 **Secure** - Token-based authentication

## Quick Start

### Prerequisites

- GitLab personal access token accepted by the GitLab user API
- GitLab instance URL (defaults to gitlab.com)

### Installation

**From source:**

```bash
git clone https://github.com/damacus/gitlab-runner-tui.git
cd gitlab-runner-tui
cargo build --release
./target/release/gitlab-runner-tui
```

### Configuration

Set required environment variables:

```bash
export GITLAB_TOKEN="glpat-xxxxxxxxxxxxxxxxxxxx"
export GITLAB_HOST="https://gitlab.com"  # Optional, defaults to gitlab.com
```

Or create a `.env` file:

```env
GITLAB_TOKEN=glpat-xxxxxxxxxxxxxxxxxxxx
GITLAB_HOST=https://gitlab.com
```

Or create a config file at `~/.config/gitlab-runner-tui/config.toml`:

```toml
gitlab_host = "https://gitlab.com"
gitlab_token = "glpat-xxxxxxxxxxxxxxxxxxxx"
poll_interval_secs = 30
poll_timeout_secs = 1800
discovery_mode = "configured_targets"

[[runner_targets]]
kind = "group"
id = "my-org/platform"
label = "Platform"

[[runner_targets]]
kind = "project"
id = "my-org/app"

[rotation_wait]
# Optional: defaults to the system timezone.
timezone = "Europe/London"
# Optional: defaults to the command start time.
rotation_window_start = "00:00"
active_contacted_within_secs = 3600
missing_runner_grace_polls = 2
completion_stability_polls = 2
```

If you launch the interactive TUI without a configured token, with a stale/invalid token, or without runner targets, the app now runs a setup flow and writes the canonical config file for you before entering the dashboard.

Runner discovery now comes from configured group/project targets instead of instance-wide runner listing. This is what makes normal GitLab.com usage possible.

Optional `config.toml` file locations (checked in this order):

1. `./config.toml` (current working directory)
2. `~/.config/gitlab-runner-tui/config.toml` (canonical)

Example:

```toml
poll_interval_secs = 30
poll_timeout_secs = 1800
gitlab_host = "https://gitlab.com"
gitlab_token = "glpat-xxxxxxxxxxxxxxxxxxxx"
discovery_mode = "configured_targets"

[[runner_targets]]
kind = "group"
id = "my-org/platform"
label = "Platform"

[[runner_targets]]
kind = "project"
id = "12345"
label = "App Project"

[rotation_wait]
active_contacted_within_secs = 3600
missing_runner_grace_polls = 2
completion_stability_polls = 2
```

`runner_targets` are required when `discovery_mode = "configured_targets"`. Set `discovery_mode = "visible_runners"` to use the current user's visible `/runners` endpoint instead. Supported target kinds:

- `group`
- `project`

Each target `id` may be either a numeric GitLab ID or a group/project path.

During onboarding, targets are entered as a single comma-separated prompt using explicit prefixes. In the interactive TUI you can also edit the discovery mode, token, targets, and poll settings from the settings modal.

```text
group:my-org/platform,project:my-org/app
```

### Running

```bash
# Using environment variables
gitlab-runner-tui

# Or specify via CLI flags
gitlab-runner-tui --host https://gitlab.example.com --token glpat-xxx
```

## Dashboard Tabs

| Tab           | Description                                             |
|---------------|---------------------------------------------------------|
| `Runners`     | Fetch all GitLab Runner details with optional filters   |
| `Health`      | Health check - verify all tagged runners are online     |
| `Offline`     | List runners with no online managers                    |
| `Uncontacted` | Find runners not contacted recently (default: 1 hour)   |
| `Empty`       | List runners with no managers                           |
| `Rotating`    | Detect runners currently in rotation (multiple managers)|
| `Workers`     | Show detailed list of all individual Runner Managers    |

## Keyboard Navigation

- `Tab` / `Shift+Tab` - Switch dashboard tabs
- `1`-`7` - Jump directly to a tab
- `↑`/`↓` or `k`/`j` - Move table selection
- `/` or `f` - Focus tag filter input
- `a` on Stale - Edit the last-contact cutoff (`HH:MM`, `HH:MM:SS`, or RFC3339)
- `v` - Open version multi-select
- `o` - Cycle sort mode
- `c` - Open settings + diagnostics
- `Enter` or `r` - Refresh the active tab
- `p` - Toggle polling / auto-refresh
- `Esc` - Exit filter editing or dismiss errors
- `?` - Toggle help
- `q` or `Ctrl-C` - Quit

## Configuration Options

### Environment Variables

| Variable       | Required | Default              | Description                                    |
|----------------|----------|----------------------|------------------------------------------------|
| `GITLAB_TOKEN` | Yes      | -                    | Personal access token accepted by `GET /user`  |
| `GITLAB_HOST`  | No       | `https://gitlab.com` | GitLab instance URL                            |

`runner_targets`, `discovery_mode`, and polling settings can be edited in the TUI settings modal and are persisted back to the canonical config file.

## CLI & Automation

In addition to the interactive TUI, `gitlab-runner-tui` can function as a powerful CLI tool for automation and LLM-based workflows.

### Command JSON Output

Pass a command to fetch data once and exit with a JSON response:

```bash
# Fetch all runners as JSON
gitlab-runner-tui fetch

# Fetch list endpoint data only, without per-runner enrichment
gitlab-runner-tui fetch --summary

# List only rotating runners as JSON
gitlab-runner-tui rotating --tags production

# Block until matching runners have rotated
gitlab-runner-tui rotating --wait --tags production
```

### Integration with `jq`

The JSON output includes both the `runners` data and `metrics` about the query. You can easily process this with `jq`:

**List all runner IDs:**
```bash
gitlab-runner-tui fetch | jq '.runners[].id'
```

**Find runners with offline managers:**
```bash
gitlab-runner-tui fetch | \
  jq '.runners[] | select(.managers[].status == "offline") | {id: .id, description: .description}'
```

**Get query performance metrics:**
```bash
gitlab-runner-tui fetch | jq '.metrics'
```

**Fast inventory for large estates:**
```bash
gitlab-runner-tui fetch --summary | jq '.metrics.request_counts'
```

`fetch --summary` keeps the normal JSON envelope but skips per-runner detail and manager requests. It is useful for quick inventory, IDs, status counts, and checking discovery scope. It does not include manager rows, manager contact timestamps, manager versions, or full runner detail.

### CLI Flags

```bash
# Override host and token
gitlab-runner-tui --host <URL> --token <TOKEN>

# Command mode (non-interactive, JSON output)
gitlab-runner-tui fetch --tags production
gitlab-runner-tui fetch --summary
gitlab-runner-tui rotating --tags production
gitlab-runner-tui rotating --wait --tags production

# Use demo data for any mode (no credentials required)
gitlab-runner-tui --demo
gitlab-runner-tui --demo fetch
```

- Commands: `fetch`, `switch`, `flames`, `empty`, `rotating`.
- `--tags <TAGS>`: Comma-separated list of tags for filtering.
- `fetch --summary`: List-only output for fast inventory. For 390 runners this avoids 390 detail requests and 390 manager requests.
- `fetch --summary` cannot be combined with `--version` because version filtering is local-only and requires enriched runner data.
- `--stale-cutoff <TIME>`: For `flames`, use a last-contact cutoff (`HH:MM`, `HH:MM:SS`, or RFC3339) instead of the default 1 hour.
- `rotating` requires `--tags` to avoid estate-wide rotation scans.
- `rotating --wait` polls until all currently eligible matching runners have rotated, emitting newline-delimited JSON progress events.

## Examples

### Find all production runners

1. Configure at least one `group` or `project` runner target
2. Select **Runners** tab (`1`)
3. Press `/` to enter tags: `production`
4. View filtered results

### Check runner health

1. Configure at least one runner target
2. Select **Health** tab (`2`)
3. Press `/` to enter tags: `production,linux`
4. View health summary and runner statuses

### List offline runners

1. Configure at least one runner target
2. Select **Offline** tab (`3`)
3. Press `/` to enter tags: `alm`
4. View runners with offline managers

### Check for runner rotation

Run a non-interactive check for runners that have multiple managers (e.g. during a migration):

```bash
gitlab-runner-tui rotating --tags prod
```

Block until matching runners have completed rotation:

```bash
gitlab-runner-tui rotating --wait --tags prod
```

### Check maintenance recovery after a cutoff

List runners whose managers have not contacted GitLab after 11:00 local time:

```bash
gitlab-runner-tui flames --stale-cutoff 11:00
```

## Development

### Building

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Run with debug logging
RUST_LOG=debug cargo run
```

### Git Hooks

This repo uses `lefthook` for local git hooks.

```bash
brew install lefthook
lefthook install
```

Configured hooks:

- `pre-commit`: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
- `pre-push`: `cargo test`

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

## Troubleshooting

### Connection Issues

**Error:** "Connection timeout"

- Check `GITLAB_HOST` is correct and accessible
- Verify network connectivity: `ping gitlab.com`
- Check proxy settings if behind corporate firewall

### Authentication Issues

**Error:** "Authentication failed"

- Verify `GITLAB_TOKEN` is correct
- Verify the token can authenticate against the GitLab user API
- Check token hasn't expired

### Runner Target Issues

**Error:** "At least one runner target must be configured"

- Add one or more `[[runner_targets]]` entries to `config.toml`
- Or rerun onboarding and enter targets like `group:my-org/platform,project:my-org/app`
- Or leave runner targets blank and use the current user's visible runners
- Confirm the configured group/project IDs or paths are valid for the current GitLab host

### SSL Certificate Issues

**Error:** "SSL certificate verify failed"

- Self-signed certificate support is not currently implemented

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Architecture

GitLab Runner TUI follows a three-layer architecture:

```text
TUI (View) → Conductor (Business Logic) → GitLabClient (API)
```

- **TUI Layer**: User interface, event handling, rendering
- **Conductor Layer**: Orchestrates operations, applies filters, formats results
- **GitLabClient Layer**: HTTP communication with GitLab API

See [app_spec.txt](app_spec.txt) for detailed specification.

## License

[Add your license here]

## Support

- **Issues**: [GitHub Issues](https://github.com/damacus/gitlab-runner-tui/issues)
- **Discussions**: [GitHub Discussions](https://github.com/damacus/gitlab-runner-tui/discussions)

## Acknowledgments

Built with [ratatui](https://ratatui.rs/) - A Rust library for building terminal user interfaces.
