# GitLab Runner TUI

A fast, beautiful Terminal User Interface (TUI) for querying and inspecting GitLab Runners.

## Overview

GitLab Runner TUI provides DevOps engineers and GitLab administrators with an intuitive command-line interface to monitor and manage GitLab Runner infrastructure. Query runners by tags—all from your terminal.

## Features

- 🚀 **Interactive TUI** - Beautiful, keyboard-driven interface built with [ratatui](https://ratatui.rs/)
- 🔍 **Multiple Query Commands** - Six specialized commands for different runner queries
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

[[runner_targets]]
kind = "group"
id = "my-org/platform"
label = "Platform"

[[runner_targets]]
kind = "project"
id = "my-org/app"
```

The legacy path `~/.config/igor/config.toml` is still read as a fallback for backward compatibility.

If you launch the interactive TUI without a configured token, with a stale/invalid token, or without runner targets, the app now runs a setup flow and writes the canonical config file for you before entering the dashboard.

Runner discovery now comes from configured group/project targets instead of instance-wide runner listing. This is what makes normal GitLab.com usage possible.

Optional `config.toml` file locations (checked in this order):

1. `./config.toml` (current working directory)
2. `~/.config/gitlab-runner-tui/config.toml` (canonical)
3. `~/.config/igor/config.toml` (legacy fallback)

Example:

```toml
poll_interval_secs = 30
poll_timeout_secs = 1800
gitlab_host = "https://gitlab.com"
gitlab_token = "glpat-xxxxxxxxxxxxxxxxxxxx"

[[runner_targets]]
kind = "group"
id = "my-org/platform"
label = "Platform"

[[runner_targets]]
kind = "project"
id = "12345"
label = "App Project"
```

`runner_targets` is required for both interactive and headless mode. Supported target kinds:

- `group`
- `project`

Each target `id` may be either a numeric GitLab ID or a group/project path.

During onboarding, targets are entered as a single comma-separated prompt using explicit prefixes, for example:

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

## Commands

| Command   | Description                                           |
|-----------|-------------------------------------------------------|
| `fetch`   | Fetch all GitLab Runner details with optional filters |
| `lights`  | Health check - verify all tagged runners are online   |
| `switch`  | List runners with no online managers                  |
| `workers` | Show detailed list of Runner Managers                 |
| `flames`  | Find runners not contacted recently (default: 1 hour) |
| `empty`   | List runners with no managers                         |

## Keyboard Navigation

- `Tab` / `Shift+Tab` - Switch dashboard tabs
- `1`-`7` - Jump directly to a tab
- `↑`/`↓` or `k`/`j` - Move table selection
- `/` or `f` - Focus tag filter input
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

`runner_targets` are not currently configurable through environment variables; use onboarding or `config.toml`.

### CLI Flags

```bash
gitlab-runner-tui --host <URL>     # Override GITLAB_HOST
gitlab-runner-tui --token <TOKEN>  # Override GITLAB_TOKEN
```

## Examples

### Find all production runners

1. Configure at least one `group` or `project` runner target
2. Select `fetch` command
3. Enter tags: `production`
4. View results

### Check runner health

1. Configure at least one runner target
2. Select `lights` command
3. Enter tags: `production,linux`
4. View health summary and runner statuses

### List offline runners

1. Configure at least one runner target
2. Select `switch` command
3. Enter tags: `alm`
4. View runners with offline managers

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
