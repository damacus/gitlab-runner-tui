# CLI and Automation Guide

The `gitlab-runner-tui` can be used as a CLI tool for automation and integration with other tools (like LLMs or monitoring scripts) by passing a command. Command mode runs once and emits JSON by default.

## Basic Usage

To fetch runners once and output JSON:

```bash
gitlab-runner-tui fetch
```

To fetch only list endpoint data without per-runner detail or manager requests:

```bash
gitlab-runner-tui fetch --summary
```

### Filtering by Tags

```bash
gitlab-runner-tui fetch --tags linux,docker
gitlab-runner-tui fetch --summary --tags linux,docker
```

### Specialized Commands

You can use the same specialized commands available in the TUI:

- `fetch`: List all discovered runners.
- `switch`: List offline runners (all managers are offline).
- `flames`: List uncontacted runners (haven't checked in for > 1 hour, or after `--stale-cutoff` when provided).
- `empty`: List runners with no registered managers.
- `rotating`: List runners with more than one manager (potential overlapping process). Requires `--tags`.

`fetch` keeps full enriched JSON by default for compatibility. Use `fetch --summary` for fast inventory, IDs, status counts, or discovery checks. Summary mode keeps the normal JSON envelope but does not include manager rows, manager contact timestamps, manager versions, or full runner detail. It cannot be combined with `--version` because version filtering is local-only and needs enriched runner data.

Example:
```bash
gitlab-runner-tui rotating --tags production
```

Block until matching runners have rotated:

```bash
gitlab-runner-tui rotating --wait --tags production
```

`rotating --wait` emits newline-delimited JSON progress events instead of one pretty-printed JSON document:

```json
{"event":"baseline","eligible_count":200,"completed_count":0,"pending_count":200,"stable_polls":0,"stale_excluded_count":3,"added_runner_ids":[123],"rotated_runner_ids":[],"missing_runner_ids":[],"removed_runner_ids":[],"pending_runner_ids":[123],"is_complete":false}
{"event":"complete","eligible_count":198,"completed_count":198,"pending_count":0,"stable_polls":2,"stale_excluded_count":0,"added_runner_ids":[],"rotated_runner_ids":[],"missing_runner_ids":[],"removed_runner_ids":[],"pending_runner_ids":[],"is_complete":true}
```

Use a maintenance cutoff for stale runner checks:

```bash
gitlab-runner-tui flames --stale-cutoff 11:00
gitlab-runner-tui flames --stale-cutoff 2026-05-12T11:00:00+01:00
```

## Rotation Wait Configuration

The waiter uses the existing `poll_interval_secs` and `poll_timeout_secs` settings. Optional rotation-specific settings can be added to `config.toml`:

```toml
[rotation_wait]
# Optional: defaults to the system timezone.
timezone = "Europe/London"
# Optional: defaults to the command start time.
rotation_window_start = "00:00"
active_contacted_within_secs = 3600
missing_runner_grace_polls = 2
completion_stability_polls = 2
```

A runner counts as rotated when a manager was created at or after the effective rotation window start, or when a later poll observes a manager `system_id` that was not present in that runner's baseline. Runners that stop appearing during deployment stop blocking after `missing_runner_grace_polls`.

## API Retry and Enrichment Limits

All idempotent GitLab GET requests retry only HTTP `429`, `502`, `503`, and `504`, with four
total attempts. The client honors a valid `Retry-After` value and otherwise uses capped
exponential backoff with jitter. Authentication failures and all other statuses fail without a
retry. Cancelling a TUI search interrupts any pending backoff sleep.

Runner detail and manager enrichment share a configurable in-flight HTTP request budget:

```toml
# Default: 10; valid range: 2-64.
max_enrichment_requests = 10
```

This config-only value counts each detail request and each manager request separately, so the two
lookups started for one runner cannot bypass the limit.

### Query profiles and request counts

Let `N` be the number of runners returned by discovery and `M` the number left after a specialized
manager-based filter:

| Query | Detail requests | Manager requests |
| --- | ---: | ---: |
| `fetch` and full TUI views | `N` | `N` |
| `switch`, `flames`, `empty`, `rotating` | `M` | `N` |
| `rotating --wait` | `0` | `N` |
| Summary with status filtering | `0` | `0` |
| Summary with a runner-version prefix | `N` | `0` |

The list request count still depends on discovery mode, configured targets, and pagination.
`rotating --wait --version ...` adds `N` detail requests to preserve runner-version filtering.
Filters are combined as one requirements set, so a query never repeats a detail or manager call
for the same runner in one pass. Full output rows remain fully enriched; the savings come from not
fetching detail data for runners that specialized commands will discard.

---

## Integration with `jq`

The JSON output includes both the `runners` list and `metrics` about the query.

### List all runner IDs

```bash
gitlab-runner-tui fetch | jq '.runners[].id'
```

### Filter by status in `jq`

Even though the app has built-in commands, you can do complex filtering with `jq`:

```bash
# Find runners that are paused
gitlab-runner-tui fetch | jq '.runners[] | select(.paused == true)'
```

### Get a summary count

```bash
gitlab-runner-tui fetch | jq '.metrics.result_count'
```

### Check request counts for large estates

```bash
gitlab-runner-tui fetch --summary | jq '.metrics.request_counts'
```

For 390 runners, summary mode avoids 390 detail requests and 390 manager requests. The expected `detail_requests` and `manager_requests` values are `0`.

### List managers for a specific runner

```bash
gitlab-runner-tui fetch | jq '.runners[] | select(.id == 12345) | .managers[].system_id'
```

### Identify "Stale" managers

```bash
# Managers that haven't contacted in over 5 minutes (300s)
# Note: Requires a recent version of jq for fromdateiso8601
gitlab-runner-tui fetch | jq '
  .runners[].managers[] 
  | select((now - (.contacted_at | fromdateiso8601)) > 300) 
  | {id: .id, system_id: .system_id, last_seen: .contacted_at}
'
```

---

## Environment Variables

For automation, it's recommended to use environment variables instead of flags:

```bash
export GITLAB_HOST="https://gitlab.example.com"
export GITLAB_TOKEN="your-token"

gitlab-runner-tui fetch
```

The token is not accepted as a command-line argument because process arguments and shell history
can expose secrets. Interactive setup reads the token without echoing it. The canonical user config
is loaded by default; current-directory `.env` and `config.toml` files are ignored. Use `--dotenv
/trusted/path/runtime.env` or `--config /trusted/path/config.toml` only for files you trust, because
they can select the GitLab host that receives the token.
