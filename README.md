# GitLab Runner TUI

GitLab Runner TUI is a read-only terminal dashboard for inspecting GitLab runners and their
managers. Use the interactive dashboard for day-to-day checks, or run one-shot commands that
return JSON for scripts and continuous integration (CI) jobs.

## What it shows

- Runner status, type, version, tags, and pause state
- Manager status, version, system ID, and last contact time
- Fleet health and runners with no online manager
- Stale runners that have stopped contacting GitLab
- Runners with no managers
- Runners with overlapping managers during rotation
- Query timing and GitLab API request counts in command output

The application only reads runner data from GitLab. It does not register, pause, delete, or edit
runners.

## Try the demo

If Docker is installed, open the dashboard without a GitLab account or token:

```sh
docker run --rm -it ghcr.io/damacus/gitlab-runner-tui:latest --demo
```

The demo uses built-in fixture data and does not make network requests.

## Install

### Download a native binary

[GitHub Releases](https://github.com/damacus/gitlab-runner-tui/releases/latest) provides these
archives:

| Operating system | Architecture | Archive |
| --- | --- | --- |
| Linux | x86-64 | `gitlab-runner-tui-linux-amd64.tar.gz` |
| Linux | ARM64 | `gitlab-runner-tui-linux-arm64.tar.gz` |
| macOS | Intel | `gitlab-runner-tui-macos-amd64.tar.gz` |
| macOS | Apple silicon | `gitlab-runner-tui-macos-arm64.tar.gz` |

Download the matching archive and `checksums-sha256.txt`. Then extract the binary and put it on
your `PATH`. For example, on Linux x86-64:

```sh
tar -xzf gitlab-runner-tui-linux-amd64.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 0755 gitlab-runner-tui "$HOME/.local/bin/gitlab-runner-tui"
gitlab-runner-tui --help
```

If `$HOME/.local/bin` is not on your `PATH`, add it to your shell configuration or install the
binary in another directory that is on your `PATH`.

### Use Docker

The published image supports Linux on AMD64 and ARM64:

```sh
docker pull ghcr.io/damacus/gitlab-runner-tui:latest
```

The `latest` tag follows the newest release. Use a full version tag for a repeatable deployment.

### Build from source

The repository uses [mise](https://mise.jdx.dev/) to install the pinned Rust toolchain and build
tools:

```sh
git clone https://github.com/damacus/gitlab-runner-tui.git
cd gitlab-runner-tui
mise trust
mise install
mise run build:release
```

The binary is written to `target/release/gitlab-runner-tui`.

## Connect to GitLab

### 1. Create a token

For GitLab 17.1 and later, start with a personal access token that has:

```text
read_api, manage_runner
```

Add `read_user` if GitLab rejects the initial user validation request. GitLab versions older than
17.1 can use the broader `api` scope.

Your GitLab role must also permit access to the runners you want to inspect. See
[GitLab token permissions](docs/security/gitlab-token-scopes.md) for endpoint and fine-grained
token details.

### 2. Run the application

```sh
gitlab-runner-tui
```

On first run, the setup asks for:

- the GitLab URL;
- the personal access token;
- a runner discovery mode; and
- optional group or project targets.

The token prompt is masked. Outside Docker, the application stores the token in macOS Keychain,
Windows Credential Manager, or Secret Service on Linux and other Unix desktops. The configuration
file stores only non-secret settings.

### 3. Choose a discovery mode

| Mode | Use it when | GitLab endpoint |
| --- | --- | --- |
| `all` | You are an instance administrator or auditor | `/runners/all` |
| `visible` | You want runners visible to the current user | `/runners` |
| `targets` | You want runners from specific groups or projects | Group and project runner endpoints |

The `all` mode falls back to `visible` if GitLab returns `403 Forbidden`. The `targets` mode is
usually the clearest choice for GitLab.com or a token with limited scope. Enter targets with an
explicit prefix:

```text
group:my-org/platform,project:my-org/application
```

A target can use a numeric GitLab ID or a group or project path.

## Use the dashboard

The dashboard has seven views:

| Key | View | Purpose |
| ---: | --- | --- |
| `1` | Runners | Browse every discovered runner |
| `2` | Health | See how many runners have an online manager |
| `3` | Offline | Find runners with managers but no online manager |
| `4` | Stale | Find runners whose managers have not contacted GitLab recently |
| `5` | Idle | Find runners with no registered managers |
| `6` | Rotating | Find runners with more than one manager |
| `7` | Workers | Inspect each manager as an individual row |

Useful keys:

| Key | Action |
| --- | --- |
| `Tab` / `Shift+Tab` | Change view |
| `1`–`7` | Open a view directly |
| `Up` / `Down` or `k` / `j` | Move the selection |
| `Enter` | Open the selected runner in GitLab |
| `r` | Refresh the current view |
| `p` | Start or stop automatic polling |
| `f` or `/` | Open tag, status, and version filters |
| `t` | Edit the comma-separated tag filter |
| `a` | Edit the contact cutoff on the Stale view |
| `s` | Change the sort order |
| `c` | Open settings and diagnostics |
| `?` | Open the built-in help |
| `q` or `Ctrl-C` | Quit |

## Run one-shot commands

Add a command to skip the dashboard and return machine-readable output:

| Command | Result |
| --- | --- |
| `fetch` | All discovered runners with details and managers |
| `fetch --summary` | Fast inventory from list endpoints only |
| `switch` | Runners with no online manager |
| `flames` | Runners whose managers have not contacted GitLab recently |
| `empty` | Runners with no managers |
| `rotating` | Runners with more than one manager |
| `auth status` | Active GitLab host and credential source |
| `auth logout` | Remove saved credentials and plaintext config tokens |

Examples:

```sh
# Fast inventory
gitlab-runner-tui fetch --summary

# Full data for production runners
gitlab-runner-tui fetch --tags production

# Offline runners on a specific runner version
gitlab-runner-tui switch --tags production --version 17.11

# Runners that have not contacted GitLab since 11:00 local time
gitlab-runner-tui flames --stale-cutoff 11:00

# Check whether matching runners have overlapping managers
gitlab-runner-tui rotating --tags production
```

Normal query commands write one JSON document. The document contains `runners` and `metrics`:

```sh
gitlab-runner-tui fetch --summary | jq '.runners[] | {id, status}'
gitlab-runner-tui fetch | jq '.metrics.request_counts'
```

`fetch --summary` omits runner details and managers. It cannot be combined with `--version`,
because version filtering needs enriched runner data.

### Wait for runner rotation

The rotation waiter polls until every eligible matching runner has rotated:

```sh
gitlab-runner-tui rotating --wait --tags production
```

It writes newline-delimited JSON events while it runs. It exits with status `0` when rotation
completes and status `2` when the configured timeout expires. See the
[CLI and automation guide](docs/CLI.md) for the event fields and waiter configuration.

## Configuration

The default configuration file is:

| Platform | Path |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/gitlab-runner-tui/config.toml`, or `~/.config/gitlab-runner-tui/config.toml` |
| macOS | `~/Library/Application Support/gitlab-runner-tui/config.toml` |
| Windows | `%APPDATA%\gitlab-runner-tui\config.toml` |

The settings screen edits the same file. A minimal target-based configuration looks like this:

```toml
gitlab_host = "https://gitlab.example.com"
discovery_mode = "configured_targets"
poll_interval_secs = 30
poll_timeout_secs = 1800
max_enrichment_requests = 10

[[runner_targets]]
kind = "group"
id = "my-org/platform"
label = "Platform"

[[runner_targets]]
kind = "project"
id = "my-org/application"
```

`max_enrichment_requests` controls concurrent runner-detail and manager requests. The default is
`10`; accepted values are `2` through `64`.

The application does not discover `config.toml` or `.env` files from the current directory. Select
another file explicitly:

```sh
gitlab-runner-tui --config /trusted/path/config.toml
gitlab-runner-tui --dotenv /trusted/path/runtime.env
```

Only select files you trust. A selected file can change the GitLab host that receives your token.

## Credentials

Use these commands to inspect or remove saved credentials without showing the token:

```sh
gitlab-runner-tui auth status
gitlab-runner-tui auth logout
```

For a temporary override, set environment variables for the process:

```sh
GITLAB_HOST=https://gitlab.example.com \
GITLAB_TOKEN=glpat-example \
gitlab-runner-tui fetch --summary
```

`GITLAB_TOKEN` takes priority over the saved credential. It is not written to the operating system
credential store or configuration file. The application does not accept tokens as command-line
arguments because process listings and shell history can expose them.

## Docker with persistent configuration

Run the dashboard with a named volume:

```sh
docker run --rm -it \
  --env XDG_CONFIG_HOME=/config \
  --mount type=volume,source=gitlab-runner-tui-config,target=/config \
  ghcr.io/damacus/gitlab-runner-tui:latest
```

The first run starts setup and writes
`/config/gitlab-runner-tui/config.toml`. The official container has no system credential store, so
the token is saved in that configuration volume. Protect access to the Docker daemon and volume.

For automation, pass a protected environment file instead of putting the token in the command:

```env
GITLAB_HOST=https://gitlab.example.com
GITLAB_TOKEN=glpat-example
```

```sh
chmod 600 /trusted/path/gitlab-runner-tui.env
docker run --rm \
  --env-file /trusted/path/gitlab-runner-tui.env \
  ghcr.io/damacus/gitlab-runner-tui:latest fetch --summary
```

Users with access to the Docker daemon can inspect container environment variables while the
container exists. Use a trusted read-only bind mount with `--dotenv` if the token must stay out of
the Docker-managed environment.

Each release also publishes `ghcr.io/damacus/gitlab-runner-tui-ci`. That image includes `sh`,
`bash`, `grep`, and `jq`, and has no application entrypoint so GitLab Runner can execute job
scripts normally.

## Troubleshooting

### Authentication fails

Run `gitlab-runner-tui auth status`, then check:

- the token has not expired;
- the token has the required runner permissions;
- the token owner can access the selected groups, projects, or instance runners; and
- `GITLAB_HOST` is the expected GitLab instance.

If `GITLAB_TOKEN` is set, it overrides the saved credential.

### GitLab returns `403 Forbidden`

The `all` discovery mode needs administrator or auditor access. Use `visible` or `targets` for a
more limited account, or grant the required GitLab role and token permissions.

### No runners appear

- Clear active tag, status, and version filters.
- Check the selected discovery mode in settings.
- In `targets` mode, confirm each group or project ID or path.
- Confirm that the token owner can see the runners in GitLab.

### The system credential store is unavailable

Unlock the desktop credential store before starting the application. For a temporary session, set
`GITLAB_TOKEN`. The native application does not fall back to saving a plaintext token.

### A self-signed GitLab certificate is rejected

Custom certificate authorities and insecure TLS are not currently supported.

## Contributing

Issues and pull requests are welcome. This repository uses mise for its pinned development tools.
Run `mise install` and `mise run check` before submitting a change.

## Support

- [Open an issue](https://github.com/damacus/gitlab-runner-tui/issues)
- [Start a discussion](https://github.com/damacus/gitlab-runner-tui/discussions)

## Licence

GitLab Runner TUI is available under the [MIT Licence](LICENSE).
