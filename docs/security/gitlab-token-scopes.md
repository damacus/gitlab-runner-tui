# GitLab token permissions

GitLab Runner TUI sends the configured token in the `PRIVATE-TOKEN` header. The
permissions that you need depend on your GitLab version, discovery mode, and the
types of runners that you inspect.

## Requests made by the app

The app validates the token first:

- `GET /api/v4/user`

It then uses one or more runner discovery endpoints:

- `GET /api/v4/runners`
- `GET /api/v4/runners/all`
- `GET /api/v4/groups/:id/runners`
- `GET /api/v4/projects/:id/runners`

The normal full view enriches each discovered runner:

- `GET /api/v4/runners/:id`
- `GET /api/v4/runners/:id/managers`

In `all_runners` discovery mode, the app tries `/runners/all` first. If GitLab
returns `403 Forbidden`, it falls back to `/runners`. In `configured_targets`
mode, the app calls the configured group or project endpoints directly.

## Legacy personal access tokens

For GitLab 17.1 and later, start with these scopes:

```text
read_api, manage_runner
```

Test the token against the target GitLab instance. If the app rejects it during
the `/user` validation request, add `read_user`:

```text
read_user, read_api, manage_runner
```

`read_api` provides broad read-only API access, but it does not replace
endpoint-specific scopes. GitLab documents `manage_runner` for runner
operations, including the runner detail and manager `GET` requests. The scope
describes the runner capability, not whether an HTTP request writes data.

GitLab introduced `manage_runner` in GitLab 17.1. For older GitLab versions,
use this compatibility fallback:

```text
api
```

The `api` scope grants complete read and write API access. It works, but it is
broader than the recommended scopes for current GitLab versions.

## Fine-grained personal access tokens

Grant `Read` access at each boundary that the app must query:

| Request | Resource | Permission | Boundary |
| --- | --- | --- | --- |
| `GET /user` | User | Read | User |
| `GET /runners` | Runner | Read | User |
| `GET /groups/:id/runners` | Runner | Read | Group |
| `GET /projects/:id/runners` | Runner | Read | Project |
| `GET /runners/all` | Runner | Read | Instance |
| `GET /runners/:id` | Runner | Read | Runner's owning Project, Group, or Instance |
| `GET /runners/:id/managers` | Runner | Read | Runner's owning Project, Group, or Instance |

A token that reads a mixture of project, group, and instance runners needs the
matching `Runner` / `Read` boundaries. A token restricted to one project or
group can omit boundaries that the app will not query.

## GitLab roles still apply

Token permissions do not override the token owner's GitLab role. Both the role
and token permissions must allow each request.

For example, `/runners/all` requires administrator or auditor access on GitLab
Self-Managed or GitLab Dedicated. Group, project, and runner-detail endpoints
also enforce the roles documented for the runner type and your GitLab version.
If a request returns `403 Forbidden`, check both the token permissions and the
token owner's role.

## Official GitLab documentation

- [Personal access token scopes](https://docs.gitlab.com/security/tokens/access_token_scopes/)
- [Runners API](https://docs.gitlab.com/api/runners/)
- [Fine-grained personal access token permissions](https://docs.gitlab.com/auth/tokens/fine_grained_access_tokens_rest/)
