# CLI and Automation Guide

The `gitlab-runner-tui` can be used as a CLI tool for automation and integration with other tools (like LLMs or monitoring scripts) by passing a command. Command mode runs once and emits JSON by default.

## Basic Usage

To fetch runners once and output JSON:

```bash
gitlab-runner-tui fetch
```

### Filtering by Tags

```bash
gitlab-runner-tui fetch --tags linux,docker
```

### Specialized Commands

You can use the same specialized commands available in the TUI:

- `fetch`: List all discovered runners.
- `switch`: List offline runners (all managers are offline).
- `flames`: List uncontacted runners (haven't checked in for > 1 hour, or after `--stale-cutoff` when provided).
- `empty`: List runners with no registered managers.
- `rotating`: List runners with more than one manager (potential overlapping process). Requires `--tags`.

Example:
```bash
gitlab-runner-tui rotating --tags production
```

Use a maintenance cutoff for stale runner checks:

```bash
gitlab-runner-tui flames --stale-cutoff 11:00
gitlab-runner-tui flames --stale-cutoff 2026-05-12T11:00:00+01:00
```

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
