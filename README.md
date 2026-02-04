# IGOR - Interactive GitLab Object Retriever

A fast, beautiful Terminal User Interface (TUI) for querying and inspecting GitLab Runners.

## Overview

IGOR provides DevOps engineers and GitLab administrators with an intuitive command-line interface to monitor and manage GitLab Runner infrastructure. Query runners by tags, status, version, and more—all from your terminal.

## Features

- 🚀 **Interactive TUI** - Beautiful, keyboard-driven interface built with [ratatui](https://ratatui.rs/)
- 🔍 **Multiple Query Commands** - Six specialized commands for different runner queries
- 🏷️ **Flexible Filtering** - Filter by tags, status, version, type, and pause state
- ⚡ **Real-time API Queries** - Direct integration with GitLab REST API v4
- 📊 **Detailed Results** - Tabular display of runners and managers with color highlighting
- 🔐 **Secure** - Token-based authentication with proxy and SSL support

## Quick Start

### Prerequisites

- GitLab personal access token with `read_api` scope
- GitLab instance URL (defaults to gitlab.com)

### Installation

**From source:**

```bash
git clone https://github.com/damacus/gitlab-runner-tui.git
cd gitlab-runner-tui
cargo build --release
./target/release/igor
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

### Running

```bash
# Using environment variables
igor

# Or specify via CLI flags
igor --host https://gitlab.example.com --token glpat-xxx
```

## Commands

| Command | Description |
|---------|-------------|
| `fetch` | Fetch all GitLab Runner details with optional filters |
| `lights` | Health check - verify all tagged runners are online |
| `switch` | List runners with no online managers |
| `workers` | Show detailed list of Runner Managers |
| `flames` | Find runners not contacted recently (default: 1 hour) |
| `empty` | List runners with no managers |

## Keyboard Navigation

### Command Selection

- `↑`/`↓` or `k`/`j` - Navigate commands
- `Enter` - Select command
- `?` - Toggle help
- `q` - Quit

### Filter Input

- Type to enter filter tags (comma-separated)
- `Enter` - Execute search
- `Esc` - Back to command selection

### Results View

- `↑`/`↓` or `k`/`j` - Scroll results
- `Esc` - Back to command selection
- `q` - Quit

## Configuration Options

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `GITLAB_TOKEN` | Yes | - | Personal access token (needs `read_api` scope) |
| `GITLAB_HOST` | No | `https://gitlab.com` | GitLab instance URL |
| `LOG_LEVEL` | No | `info` | Logging level: `debug`, `info`, `warn`, `error` |
| `DISABLE_SSL_WARNINGS` | No | `false` | Disable SSL verification (for self-signed certs) |
| `HTTP_PROXY` | No | - | HTTP proxy URL |
| `HTTPS_PROXY` | No | - | HTTPS proxy URL |
| `NO_PROXY` | No | - | Comma-separated hosts to bypass proxy |

### CLI Flags

```bash
igor --host <URL>     # Override GITLAB_HOST
igor --token <TOKEN>  # Override GITLAB_TOKEN
```

## Examples

### Find all production runners

1. Select `fetch` command
2. Enter tags: `production`
3. View results

### Check runner health

1. Select `lights` command
2. Enter tags: `production,linux`
3. View health summary and runner statuses

### List offline runners

1. Select `switch` command
2. Enter tags: `alm`
3. View runners with offline managers

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
- Ensure token has `read_api` scope
- Check token hasn't expired

### SSL Certificate Issues

**Error:** "SSL certificate verify failed"

- For self-signed certificates, set: `DISABLE_SSL_WARNINGS=true`
- Not recommended for production use

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Architecture

IGOR follows a three-layer architecture:

```
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
