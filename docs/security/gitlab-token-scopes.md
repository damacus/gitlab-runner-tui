# GitLab token scopes

`gitlab-runner-tui` uses one token for token validation and runner reads. The
minimum scope depends on the token type and on the discovery mode.

## Legacy personal access token

For current GitLab versions, use these two scopes:

- `read_user` — allows the startup check against `GET /user`.
- `manage_runner` — allows the runner API reads used by the TUI.

`read_api` is not enough for the normal full view. The TUI normally enriches
each runner with these calls:

- `GET /runners/:id`
- `GET /runners/:id/managers`

GitLab protects those runner calls with `manage_runner`. The broad `api`
scope also works, but grants read and write access to the whole API and is not
the minimum recommended scope.

`manage_runner` was introduced in GitLab 17.1. On older GitLab versions,
`api` may be required because the narrower scope does not exist there.

The user must still have a suitable GitLab role. A token scope does not grant
access that the user does not already have.

## Fine-grained personal access token

Fine-grained tokens use a resource, an action, and an access boundary. Select
`Read` for the `Runner` resource at each boundary the token must read:

| TUI request | Fine-grained permission |
| --- | --- |
| `GET /user` | `User` → `Read` |
| `GET /runners` | `Runner` → `Read` → `User` |
| `GET /groups/:id/runners` | `Runner` → `Read` → `Group` |
| `GET /projects/:id/runners` | `Runner` → `Read` → `Project` |
| `GET /runners/all` | `Runner` → `Read` → `Instance` |
| `GET /runners/:id` | `Runner` → `Read` at the runner's owning boundary |
| `GET /runners/:id/managers` | `Runner` → `Read` at the runner's owning boundary |

The TUI can use different requests depending on its discovery mode:

- **Visible runners** uses `GET /runners`.
- **All runners** tries `GET /runners/all` and falls back to `GET /runners`
  when GitLab returns `403 Forbidden`.
- **Configured targets** uses the group or project endpoint for each target.

Therefore, a token that must handle a mixture of project, group, and instance
runners needs the matching `Runner` → `Read` boundaries. A token limited to
one project or group can omit the others.

## GitLab role requirements

The API role requirements still apply:

- `/runners/all` requires administrator or auditor access on GitLab
  Self-Managed or GitLab Dedicated.
- Group runner listings require the documented group Owner/Auditor or
  administrator access.
- Project runner listings require the documented project Maintainer/Auditor
  or administrator access.

The exact requirements vary by runner type and GitLab version. The token scope
and the user's role must both allow the request.

## References

- [Access token scopes](https://docs.gitlab.com/security/tokens/access_token_scopes/)
- [Runners API](https://docs.gitlab.com/api/runners/)
- [Fine-grained REST API permissions](https://docs.gitlab.com/auth/tokens/fine_grained_access_tokens_rest/)
