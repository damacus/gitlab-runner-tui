# Agent Instructions

This project uses **GitHub Issues** for issue tracking.

## Local Tooling

- Use `mise install` to install the Rust toolchain and local development tools.
- Use `mise run <task>` for local project commands, including builds, tests, formatting, linting, security checks, and container builds.
- Do not run `cargo`, `rustup`, or locally installed Rust tools directly.
- If `mise.toml` does not provide a required local command, add a reusable task before running it.
- Use `mise tasks` to list the available commands.

## Quick Reference

```bash
gh issue list --state open                  # Find available work
gh issue view <id>                          # View issue details
gh issue create --title "<title>" --body "<body>"  # File follow-up work
gh issue close <id> --comment "<summary>"   # Close finished work
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
