# Demo Mode Design

**Date:** 2026-03-19
**Status:** Approved

## Problem

The TUI requires a live GitLab instance with valid credentials to run. This makes local development, UI iteration, and showing the app to others unnecessarily difficult.

## Goal

A `--demo` flag that launches the full TUI pre-populated with realistic static fixture data — no credentials, no network, no configuration required.

## Approach

Direct state injection. Demo mode bypasses the conductor and credential bootstrap entirely, populates `App` state with a static `Vec<Runner>` from a new `src/fixtures.rs` module, and marks all tabs as loaded.

## Components

### 1. CLI flag (`src/main.rs`)

Add `--demo` to the `Args` struct via clap:

```rust
#[arg(long, help = "Run with demo fixture data (no GitLab credentials required)")]
demo: bool,
```

In `main()`, branch before credential validation:

```rust
if args.demo {
    run_demo().await?;
    return Ok(());
}
```

### 2. Fixture data (`src/fixtures.rs`)

Public function `demo_runners() -> Vec<Runner>` returning ~15 hardcoded runners covering:

| Status | Count | Notes |
|--------|-------|-------|
| online | 6 | mix of versions, tags, manager counts |
| offline | 3 | some with stale managers |
| stale | 3 | last contact 2–10 days ago |
| never_contacted | 2 | no managers, no contact |

Additional variety:
- Runner types: `group_type`, `instance_type`, `project_type`
- Versions: mix of current (18.x) and outdated (16.x, 17.x)
- Tags: realistic sets (`docker`, `linux`, `prod`, `k8s`, `platform`, `dps:core:runner:type:local`)
- Some runners paused
- Some with groups assigned
- Managers: 0–3 per runner, mix of online/offline/never_contacted
- `created_at` spread over last 3 years
- `ip_address` for some, absent for others

### 3. Demo entry point (`src/main.rs`)

```rust
async fn run_demo() -> Result<()> {
    let config = AppConfig {
        gitlab_host: Some("https://demo.gitlab.example.com".to_string()),
        ..AppConfig::default()
    };
    let conductor = Conductor::new_noop();
    let mut app = App::new(conductor, config);

    // Inject fixture data — simulate a completed poll on the Runners tab
    app.runners = fixtures::demo_runners();
    app.loaded_tab = Some(Tab::Runners);
    app.is_loading = false;

    run_tui(app).await
}
```

The existing `run_tui()` helper (or equivalent terminal setup + event loop) is reused so demo mode is identical to real mode at the render layer.

### 4. Noop Conductor

Add `Conductor::new_noop()` — a constructor that creates a conductor with no client and no targets. Subsequent background polls triggered by `p` or `r` will fail gracefully (existing error handling already displays errors in the status bar) or return empty results. No special-casing needed in the TUI.

Alternatively, if the conductor's fetch path panics without a real client, wrap the poll in a check: if `app.config.gitlab_host == demo host`, skip background polls. This is a minor implementation detail to resolve during coding.

## Data Flow

```
$ gitlab-runner-tui --demo
         │
         ▼
   args.demo == true
         │
         ▼
   run_demo()
   ├─ AppConfig { host: "demo.gitlab.example.com" }
   ├─ Conductor::new_noop()
   ├─ App::new(conductor, config)
   ├─ app.runners = fixtures::demo_runners()
   └─ app.loaded_tab = Some(Tab::Runners)
         │
         ▼
   run_tui(app)   ← identical to normal interactive path
```

## Error Handling

- If the user presses `r` (refresh) or `p` (poll) in demo mode, the conductor fetch will return an error or empty result. The TUI already handles this gracefully with a status bar message. No special demo-mode guards needed in the UI layer.
- `--demo` combined with `--host` or `--token` is allowed; the extra args are simply ignored.

## Testing

- `fixtures::demo_runners()` is `pub` so existing snapshot tests can use it directly, reducing duplication with inline test builders.
- No new test infrastructure needed; the function is pure and has no side effects.

## Out of Scope

- Simulated polling / dynamic data changes over time
- Configurable fixture sets via flags or files
- Demo mode in headless (`--watch`) mode
