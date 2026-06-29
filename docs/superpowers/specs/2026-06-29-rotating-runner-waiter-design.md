# Rotating Runner Waiter Design

**Date:** 2026-06-29
**Status:** Implemented

## Problem

GitLab runner rotations can take longer than a single CLI query. During a GitLab deployment, runner managers may be replaced, runners may disappear, and new matching runners may appear. Automation needs a blocking command that waits until the current matching fleet has rotated without relying on a fixed expected runner count.

## Goal

Extend the current subcommand CLI so `gitlab-runner-tui rotating --wait --tags <tags>` polls GitLab until all currently eligible matching runners have rotated. Plain `gitlab-runner-tui rotating --tags <tags>` remains a one-shot JSON snapshot.

## CLI Contract

Command mode emits JSON by default. The removed `--once`, `--json`, and `--command` flags are not reintroduced.

```bash
gitlab-runner-tui rotating --tags production
gitlab-runner-tui rotating --wait --tags production
```

`rotating --wait` emits newline-delimited JSON, one event per line, so callers can stream progress:

```json
{"event":"baseline","eligible_count":200,"completed_count":0,"pending_count":200,"stable_polls":0,"stale_excluded_count":3,"added_runner_ids":[123],"rotated_runner_ids":[],"missing_runner_ids":[],"removed_runner_ids":[],"pending_runner_ids":[123],"is_complete":false}
{"event":"progress","eligible_count":200,"completed_count":1,"pending_count":199,"stable_polls":0,"stale_excluded_count":0,"added_runner_ids":[],"rotated_runner_ids":[123],"missing_runner_ids":[],"removed_runner_ids":[],"pending_runner_ids":[456],"is_complete":false}
{"event":"complete","eligible_count":198,"completed_count":198,"pending_count":0,"stable_polls":2,"stale_excluded_count":0,"added_runner_ids":[],"rotated_runner_ids":[],"missing_runner_ids":[],"removed_runner_ids":[],"pending_runner_ids":[],"is_complete":true}
```

## Configuration

The waiter uses existing `poll_interval_secs` and `poll_timeout_secs`. Rotation-specific settings are optional:

```toml
[rotation_wait]
timezone = "Europe/London"          # optional; defaults to the system timezone
rotation_window_start = "00:00"     # optional; defaults to command start time
active_contacted_within_secs = 3600
missing_runner_grace_polls = 2
completion_stability_polls = 2
```

If `rotation_window_start` is omitted, the command start time becomes the rotation window start. If `timezone` is omitted, configured wall-clock start times are interpreted in the system timezone.

## Rotation Semantics

- Each poll discovers runners using the existing configured discovery mode, targets, tags, and version filters.
- Runners with no recently contacted manager are excluded from the blocking fleet and reported as stale.
- Newly discovered eligible runners join the tracked fleet immediately.
- Missing runners remain blocking until they have been absent for `missing_runner_grace_polls`, then they are removed from the tracked fleet.
- A runner counts as rotated when any manager was created at or after the effective rotation window start, or when a later poll observes a manager `system_id` not present in that runner's baseline.
- Completion requires a non-empty eligible fleet, zero pending runners, and `completion_stability_polls` consecutive complete polls.

## Testing

Coverage includes parser tests for `rotating --wait`, config parsing/defaults, timezone window resolution, pure state-machine tests for stale exclusion and fleet churn, a mock GitLab waiter test, and the existing command JSON snapshot tests.
