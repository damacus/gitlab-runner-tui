# GitLab Runner TUI

A fast, beautiful Terminal User Interface (TUI) for querying and inspecting GitLab Runners.

## Overview

GitLab Runner TUI provides DevOps engineers and GitLab administrators with a command-line interface to monitor and manage GitLab Runner infrastructure. Query runners by tags from your terminal.

## Preview

![Overview of the seven dashboard tabs](docs/screenshots/overview.gif)

*Seven dashboard tabs. Navigate with `1` to `7` or `Tab`.*

![Runner detail pane](docs/screenshots/detail.gif)

*Runner detail pane. Use the arrow keys to inspect runner status, version, managers, and tags.*

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

- GitLab personal access token with the required runner permissions. On GitLab 17.1 and later,
  start with `read_api` and `manage_runner`. Add `read_user` if `/user` validation fails. Older
  GitLab versions can use the broader `api` compatibility fallback. See
  [GitLab token permissions](docs/security/gitlab-token-scopes.md).
- GitLab instance URL (defaults to gitlab.com)

### Installation

#### From source

```bash
git clone https://github.com/damacus/gitlab-runner-tui.git
cd gitlab-runner-tui
cargo build --release
./target/release/gitlab-runner-tui
```

#### Docker

The published image supports Linux on AMD64 and ARM64:

```bash
docker pull ghcr.io/damacus/gitlab-runner-tui:latest
```

The `latest` tag follows the newest release. For repeatable deployments, replace it with a full
version tag from the [published packages](https://github.com/damacus/gitlab-runner-tui/pkgs/container/gitlab-runner-tui).

Run the interactive TUI with a named volume for its configuration:

```bash
docker run --rm -it \
  --env XDG_CONFIG_HOME=/config \
  --mount type=volume,source=gitlab-runner-tui-config,target=/config \
  ghcr.io/damacus/gitlab-runner-tui:latest
```

The first run starts the masked setup flow. It stores the configuration at
`/config/gitlab-runner-tui/config.toml`. Docker creates the named volume automatically, and the
same command reuses it after `--rm` removes the previous container.

The volume persists configuration settings and the saved token. It does not persist fetched
runner data, the selected dashboard tab, or active filters. Protect access to the Docker daemon
and the volume because the configuration contains credentials.

##### Docker credentials

For interactive use, prefer the masked setup flow above. It keeps the token out of the Docker
command and shell history.

For automation, create an environment file outside the repository:

```env
GITLAB_TOKEN=glpat-xxxxxxxxxxxxxxxxxxxx
GITLAB_HOST=https://gitlab.com
```

Restrict the file, then pass its path to Docker:

```bash
chmod 600 /trusted/path/gitlab-runner-tui.env

docker run --rm \
  --env-file /trusted/path/gitlab-runner-tui.env \
  --env XDG_CONFIG_HOME=/config \
  --mount type=volume,source=gitlab-runner-tui-config,target=/config \
  ghcr.io/damacus/gitlab-runner-tui:latest fetch --summary
```

Do not put a literal token in `docker run --env GITLAB_TOKEN=...`. An environment file keeps the
token out of the command and shell history, but Docker still stores environment variables as plain
text in the container configuration. Users with access to the Docker daemon can inspect them while
the container exists.

To keep the token out of the Docker-managed environment, mount the trusted file read-only and let
the application load it explicitly:

```bash
docker run --rm \
  --env XDG_CONFIG_HOME=/config \
  --mount type=volume,source=gitlab-runner-tui-config,target=/config \
  --mount type=bind,source=/trusted/path/gitlab-runner-tui.env,target=/run/secrets/gitlab-runner-tui.env,readonly \
  ghcr.io/damacus/gitlab-runner-tui:latest \
  --dotenv /run/secrets/gitlab-runner-tui.env fetch --summary
```

Only mount a dotenv file that you trust. Its host and token values control where credentials are
sent. See Docker's documentation for [`docker run`](https://docs.docker.com/reference/cli/docker/container/run/),
[named volumes](https://docs.docker.com/engine/storage/volumes/), and
[read-only bind mounts](https://docs.docker.com/engine/storage/bind-mounts/).

##### GitLab CI image

Each release also publishes `ghcr.io/damacus/gitlab-runner-tui-ci`. This image supports Linux on
AMD64 and ARM64 and includes `sh`, `bash`, `grep`, and `jq`. It has no application entrypoint, so
GitLab Runner can execute job scripts through the image shell. The CI image is published only when
a release is created. Replace `latest` with a full release tag for a repeatable pipeline.

Store `GITLAB_TOKEN` as a masked and hidden GitLab CI/CD variable. Protect the variable if only
protected branches or tags need it. The environment token is used for the current process and is
not written to the system credential store or `config.toml`.

This job writes the polling events to an artifact and uses `jq` to require a final `complete`
event:

```yaml
runner-rotation:
  image: ghcr.io/damacus/gitlab-runner-tui-ci:latest
  variables:
    GITLAB_HOST: "https://gitlab.example.com"
    RUNNER_TAGS: "production"
  script:
    - gitlab-runner-tui rotating --wait --tags "$RUNNER_TAGS" | tee rotation.ndjson
    - jq -e 'select(.event == "complete")' rotation.ndjson > /dev/null
  artifacts:
    when: always
    paths:
      - rotation.ndjson
```

`rotating --wait` exits with status `0` after rotation completes and status `2` after the configured
timeout. It writes newline-delimited JSON events while it polls. Other commands write one JSON
document, so they can be piped directly to `jq`:

```yaml
runner-inventory:
  image: ghcr.io/damacus/gitlab-runner-tui-ci:latest
  script:
    - gitlab-runner-tui fetch --summary | jq '.metrics.request_counts'
```

### Configuration

#### First run outside Docker

Start the interactive TUI without `GITLAB_TOKEN`, `--dotenv`, or a command such as `fetch`:

```bash
gitlab-runner-tui
```

The setup asks for your GitLab host, personal access token, discovery mode, and runner targets. The
token prompt is masked. For most users, choose `targets` and enter one or more groups or projects,
for example `group:my-org/platform,project:my-org/app`.

On success, the app stores the token in the operating system credential store:

- macOS Keychain
- Windows Credential Manager
- Secret Service on Linux and other Unix desktops

The canonical `config.toml` stores only the GitLab host and non-secret settings. Start the app again
normally, or run a command such as `gitlab-runner-tui fetch --summary`. The app reads the saved token
automatically.

Unlock the operating system credential store before you start the app. If the credential store is
unavailable, set `GITLAB_TOKEN` for the current process or use the Docker workflow. The app does not
fall back to writing the token in plaintext outside Docker.

If `GITLAB_TOKEN` is set or you pass `--dotenv`, that token is a temporary override and is not saved
to the credential store. If an existing canonical config contains `gitlab_token`, the next normal
local launch moves it to the credential store and rewrites the config without the token.

Check which credential source is active without displaying the token:

```bash
gitlab-runner-tui auth status
```

The status command warns when `GITLAB_TOKEN` overrides saved credentials or when `config.toml`
contains a plaintext token. To remove saved credentials and rewrite `config.toml` without the token,
run:

```bash
gitlab-runner-tui auth logout
```

Then run `gitlab-runner-tui` and complete setup to store a new token in the operating system
credential store.

#### Automation credentials

For automation, set environment variables:

```bash
export GITLAB_TOKEN="glpat-xxxxxxxxxxxxxxxxxxxx"
export GITLAB_HOST="https://gitlab.com"  # Optional, defaults to gitlab.com
```

Or load an explicitly trusted dotenv file:

```env
GITLAB_TOKEN=glpat-xxxxxxxxxxxxxxxxxxxx
GITLAB_HOST=https://gitlab.com
```

```bash
gitlab-runner-tui --dotenv /trusted/path/gitlab-runner-tui.env
```

Dotenv files are never discovered from the current working directory. Only use `--dotenv` with a
file you trust. Its host and token values control where credentials are sent.

You can keep non-secret settings in `~/.config/gitlab-runner-tui/config.toml`:

```toml
gitlab_host = "https://gitlab.com"
poll_interval_secs = 30
poll_timeout_secs = 1800
# Bounds concurrent runner-detail and manager API requests (valid range: 2-64).
max_enrichment_requests = 10
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

If you launch the interactive TUI without a configured token, with a stale or invalid token, or
without runner targets, the app runs setup before entering the dashboard. Local setup stores the
token in the system credential store. The official Docker image has no system credential store, so
its setup flow stores the token in the mounted configuration volume instead.

Runner discovery now comes from configured group/project targets instead of instance-wide runner listing. This is what makes normal GitLab.com usage possible.

By default, configuration is loaded only from the canonical user path:

- `~/.config/gitlab-runner-tui/config.toml` on Linux
- the equivalent platform user-config directory on macOS and Windows

The application does not discover `./config.toml`. To use another file, select it explicitly with
`--config /trusted/path/config.toml`. Treat explicitly selected config and dotenv files as trusted
because they can choose the GitLab host that receives your token.

Example:

```toml
poll_interval_secs = 30
poll_timeout_secs = 1800
max_enrichment_requests = 10
gitlab_host = "https://gitlab.com"
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

GitLab GET requests retry HTTP `429`, `502`, `503`, and `504` responses up to four total
attempts. A valid `Retry-After` header controls the delay. Otherwise, the client uses capped
exponential backoff with jitter. Authentication failures and other HTTP statuses are not retried.
Aborting or replacing an interactive search also cancels a pending retry delay.

`max_enrichment_requests` is config-only and defaults to `10`. It is the combined in-flight
request budget for runner detail and manager lookups, including the two lookups started for each
runner. Values from `2` through `64` are accepted.

Queries now use explicit enrichment profiles instead of always issuing both follow-up requests.
The full Runners, Health, and Workers views still fetch detail and manager data because their
tables and detail panes display both. Offline, Stale, Idle, and Rotating queries fetch managers for
all candidates, filter the result, and fetch runner details only for matching rows. Rotation wait
polls use manager data only unless a version filter requires runner detail. Status-only summary
queries use list data without per-runner enrichment.

The interactive Runners tab renders list-endpoint summaries as soon as discovery completes, then
updates each row as its detail and manager requests finish. The selected runner is tracked by ID,
so sorting changes during enrichment do not move the user's selection to another runner. Filters
that require detail or manager data are applied to the final enriched snapshot. Starting another
search, changing tabs, or saving settings cancels the previous stream and its pending retry delays.
Specialized tabs remain correctness-first and publish their results only after their required
manager/detail filtering is complete.

Within an interactive Runners session, successful detail and manager enrichment is cached for the
current discovery scope. The cache compares the stable runner ID and the complete list-endpoint
runner record. An unchanged poll reuses enrichment, while a changed or new runner fetches only its
missing data. Runners absent from the next result in the same scope are evicted. Changing the host,
token, discovery mode, targets, or saved settings clears the cache. Cache updates are committed only
after the current search completes, so a canceled refresh cannot replace known-good data.
`metrics.request_counts` continues to report HTTP calls only, while
`metrics.reused_enrichments.detail_enrichments` and `manager_enrichments` report cache reuse.

The TUI keeps each enriched runner only once in its canonical session store, using lightweight
indices for filtered and sorted runner tables and runner/manager index pairs for worker tables, so
rebuilding a view does not clone runner, manager, tag, or string payloads.
Selection is restored by runner ID whenever streamed updates rebuild or reorder those projections.
The settings benchmark includes 1,000- and 10,000-runner samples and reports the deep Runner clone
count alongside filtering, sorting, and worker-flattening latency.

`runner_targets` are required when `discovery_mode = "configured_targets"`. Set `discovery_mode = "visible_runners"` to use the current user's visible `/runners` endpoint instead. Supported target kinds:

- `group`
- `project`

Each target `id` may be either a numeric GitLab ID or a group/project path.

During onboarding, targets are entered as a comma-separated prompt using explicit prefixes. In the interactive TUI you can also edit the discovery mode, token, targets, and poll settings from the settings modal.

```text
group:my-org/platform,project:my-org/app
```

### Running

```bash
# Using environment variables
gitlab-runner-tui

# Or override only the host through the CLI. Tokens are never accepted through argv.
GITLAB_TOKEN=glpat-xxx gitlab-runner-tui --host https://gitlab.example.com
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
| `GITLAB_TOKEN` | No       | System credential store | Personal access token override with the required [runner permissions](docs/security/gitlab-token-scopes.md) |
| `GITLAB_HOST`  | No       | `https://gitlab.com` | GitLab instance URL                            |

`runner_targets`, `discovery_mode`, and polling settings can be edited in the TUI settings modal
and are persisted back to the canonical config file. Outside Docker, token changes are saved to the
system credential store instead of `config.toml`.

## CLI & Automation

`gitlab-runner-tui` also provides a CLI for automation and LLM-based workflows.

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
# Override the host (provide tokens through GITLAB_TOKEN or secure interactive setup)
gitlab-runner-tui --host <URL>

# Explicitly load trusted local configuration
gitlab-runner-tui --config /trusted/path/config.toml
gitlab-runner-tui --dotenv /trusted/path/runtime.env

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
# Install the pinned Rust toolchain and development tools
mise install

# Development build
mise run build

# Release build (optimized)
mise run build:release

# Run tests
mise run test

# Run with debug logging
RUST_LOG=debug mise run dev
```

### Git Hooks

This repo uses `lefthook` for local git hooks.

```bash
mise run hooks:install
```

The hook process must be able to find `mise` on `PATH`. Configure GUI Git clients to use the
same `PATH` as your mise-enabled shell.

Configured hooks:

- `pre-commit`: `mise run fmt` and `mise run lint`
- `pre-push`: `mise run test`

### Testing

```bash
# Run all tests
mise run test

# Run with output
mise run test:nocapture

# Run specific test
mise run test test_name
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
