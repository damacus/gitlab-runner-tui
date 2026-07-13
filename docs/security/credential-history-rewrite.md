# Historical credential response and Git history rewrite

This runbook covers GitHub issue #155. It intentionally contains no credential
value. Treat the rewrite as an owner-coordinated incident operation, not a
normal development change.

Rotating or revoking the exposed credential is the first and most important
step. Rewriting Git history does not make an active credential safe, and it
does not remove copies from existing clones, forks, pull request refs, or
caches.

## Verified scope on 2026-07-13

A fully redacted gitleaks 8.30.1 scan of all reachable local history exits with
status 1. It reports the historical `.env` finding under two detection rules.
The finding originates in the repository's initial history and is therefore an
ancestor of every currently published branch and tag.

The affected GitHub branch refs are:

- `bolt/avoid-string-join-sorting-1502315767318017684`
- `bolt/optimize-tag-sort-3172190013451509846`
- `bolt-optimize-runner-detail-tags-allocation-14820102985510543750`
- `bolt-optimize-tag-sort-13309248021822007449`
- `bolt-optimize-tag-sorting-17858758804670742803`
- `codex/top-20-improvements`
- `main`
- `release-please--branches--main--components--gitlab-runner-tui`

The affected GitHub tag refs are:

- `0.1.1`
- `gitlab-runner-tui-v0.1.1`
- `gitlab-runner-tui-v0.1.2`
- `gitlab-runner-tui-v0.1.3`
- `v0.1.0`
- `v0.1.1`
- `v0.1.10`
- `v0.1.11`
- `v0.1.12`
- `v0.1.13`
- `v0.1.14`
- `v0.1.15`
- `v0.1.16`
- `v0.1.17`
- `v0.1.2`
- `v0.1.4`
- `v0.1.5`
- `v0.1.6`
- `v0.1.7`
- `v0.1.8`
- `v0.1.9`
- `v0.2.0`

This is 30 currently published branch and tag refs. Do not use stale local
remote-tracking refs as the source of truth. Refresh and re-inventory the remote
immediately before the maintenance window.

GitHub reports `main` as protected by active repository ruleset `12695880`.
The ruleset blocks deletion and non-fast-forward updates and requires `Lint`,
`Build`, and the stable macOS, Ubuntu, and Windows test jobs. The legacy branch
protection endpoint returns 404 because the repository ruleset is the active
protection mechanism. An authorized repository owner must arrange a temporary
bypass or disable the ruleset before the force-push, then restore and verify it.

The scan also detects the already-pushed historical form of a non-secret,
token-shaped test sentinel in `src/tui/ui.rs`. Issue #174 replaces the current
fixture so current-tree and new-commit scans can pass, but the old commit still
has one exact fingerprint entry in `.gitleaksignore`. During the rewrite,
remove that historical form too and then delete the stale ignore entry. Never
ignore the `.env` finding.

## Required authorizations and external actions

The repository owner or incident lead must explicitly authorize and perform
these actions:

1. Revoke the exposed GitLab token and confirm it is unusable.
2. If the application still needs a token, issue a replacement with the least
   required scopes, update the authorized secret store, and verify the
   replacement before removing any temporary fallback.
3. Review GitLab audit events for use of the exposed credential and escalate
   any unexplained access.
4. Announce a push freeze, identify active collaborators and forks, and choose
   a maintenance window.
5. Merge or close open pull requests, or explicitly accept that rewritten
   commit IDs can invalidate their diffs and comments.
6. Temporarily bypass or disable the `main` ruleset's non-fast-forward rule.
7. Approve the mirror force-push of every branch and tag.
8. Restore the ruleset immediately after the push.
9. Open a GitHub Support request to remove affected read-only pull request refs
   and cached views and to run server-side cleanup where available.
10. Coordinate cleanup with fork owners; GitHub cannot remove the object from
    another user's clone or fork.

Do not begin the rewrite until steps 1 through 7 have named owners and the push
freeze is active.

## Rotation and revocation checklist

- Record the incident owner, start time, token owner, and affected integration
  in a private incident record. Do not paste the token into the record.
- Revoke the old credential in GitLab. Record only the token identifier or a
  non-reversible fingerprint supplied by the provider.
- Confirm the old credential is rejected using the provider's UI or an
  approved secret-aware verification process. Do not put it on a command line,
  in shell history, or in CI output.
- Create a replacement only if required. Prefer a narrowly scoped project or
  group access token over a broadly privileged personal access token.
- Update GitHub Actions, deployment systems, and local operator secret stores
  through their secret-management interfaces.
- Verify the replacement, remove superseded values, and review GitLab audit
  events.

## Collaborator notice

Send this notice before the push freeze:

> We are rotating a credential and rewriting this repository's history during
> the announced maintenance window. Stop pushes and do not merge or update
> branches until the all-clear. Commit IDs will change. Afterward, discard old
> clones and re-clone, or follow the recovery steps below. Do not merge an old
> branch into rewritten history because that can reintroduce the removed
> objects.

Send a second notice after verification with the new default-branch commit ID,
the ruleset-restoration confirmation, and the contributor recovery deadline.

## Prepare an isolated clone

The following preparation and inspection commands are read-only against the
remote. Run them in a new incident-only directory, not in a developer's normal
clone.

```fish
git clone --mirror git@github.com:damacus/gitlab-runner-tui.git gitlab-runner-tui-clean.git
cd gitlab-runner-tui-clean.git
git fetch --all --prune
git show-ref --heads --tags
git filter-repo --version
gitleaks version
```

Record branch and tag names and commit IDs in the private incident log. Do not
record blob contents. Verify that `git-filter-repo` supports
`--sensitive-data-removal` (version 2.47 or later).

With `--sensitive-data-removal`, `git-filter-repo` fetches additional refs that
may include affected `refs/pull/*` references in addition to branches and tags.
These refs are read-only on GitHub and cannot be fixed by an owner force-push.
Keep the list from `.git/filter-repo/changed-refs` for the GitHub Support
request.

## Owner-only destructive rewrite

The commands in this section rewrite commit IDs and force-update the remote.
Run them only after the explicit approvals, credential revocation, push freeze,
backup, and protection bypass described above.

Remove the historical environment file from every rewritten ref:

```fish
git filter-repo --sensitive-data-removal --path .env --invert-paths --force
```

If the token-shaped test sentinel remains in rewritten history, resolve it in
the same isolated rewrite before pushing. Do not create a replacement file that
contains the real credential. If a text-replacement rule is required, prepare
it in a mode-0600 file through an approved secret-handling process and securely
dispose of it after verification.

`git-filter-repo` may remove `origin` as a safety measure. Re-add only the
verified repository URL:

```fish
git remote add origin git@github.com:damacus/gitlab-runner-tui.git
git remote -v
```

Verify the rewritten clone before any push:

```fish
git log --all -- .env
git rev-list --objects --all | awk '$2 == ".env" { print $1, $2 }'
gitleaks git --log-opts=--all --redact=100 --no-banner .
git push --force --mirror --dry-run origin
```

The first two commands must produce no output. Gitleaks must exit 0 without an
ignore for the incident finding. Review the dry-run ref list against the frozen
inventory. The incident clone must be a mirror clone as shown above; do not run
`git push --mirror` from a normal working clone, which may contain local-only
refs. Stop if any unexpected ref would be created, changed, or deleted.

After the incident owner signs off on that evidence, perform the irreversible
push:

```fish
git push --force --mirror origin
```

Failures for `refs/pull/*` are expected because GitHub owns those refs. Any
other failure is a stop condition, commonly an active ruleset or a ref that
changed after the freeze. Do not partially improvise around a failed push;
reconcile the inventory and repeat the rewrite from a fresh mirror if needed.

## Post-rewrite verification

1. Restore ruleset `12695880` and verify its deletion,
   non-fast-forward, and five required-check rules are active.
2. Fresh-clone the repository into a separate directory.
3. Confirm `.env` is absent from all reachable objects and the full-history
   gitleaks scan exits 0:

   ```fish
   git log --all -- .env
   git rev-list --objects --all | awk '$2 == ".env" { print $1, $2 }'
   gitleaks git --log-opts=--all --redact=100 --no-banner .
   ```

4. Scan the tracked working tree without traversing `.git`:

   ```fish
   set scan_dir (mktemp -d /tmp/gitlab-runner-tui-tree.XXXXXX)
   git ls-files -z | tar --null -T - -cf - | tar -xf - -C $scan_dir
   gitleaks dir --redact=100 --no-banner $scan_dir
   ```

5. Compare all GitHub branch and tag names with the frozen inventory and verify
   each now resolves to rewritten history.
6. Verify the default branch and release tags still build from a fresh clone.
7. Provide GitHub Support with the repository, affected pull request refs from
   `.git/filter-repo/changed-refs`, the first changed commit metadata, and the
   statement that the credential was revoked. Never include the credential.
8. Ask fork owners to purge or delete affected forks.
9. Keep the push freeze until Support coordination and collaborator notices are
   complete.

## Contributor recovery

The safest recovery is to discard the old clone and clone the repository again:

```fish
cd ..
mv gitlab-runner-tui gitlab-runner-tui.pre-rewrite
git clone git@github.com:damacus/gitlab-runner-tui.git
```

Do not push from the preserved clone. Keep it offline only long enough to
recover unpushed work, then delete it according to the incident lead's cleanup
instructions.

For unpushed work, create patches containing only the intended changes, inspect
them for secrets, and apply them to a fresh branch based on rewritten `main`.
Do not merge or rebase the old branch wholesale, and do not push old tags. A
single merge from tainted history can restore the removed objects.

## Completion criteria

- The old GitLab credential is confirmed unusable.
- Every published branch and tag points only to rewritten history.
- `.env` is absent from every reachable Git object.
- Full-history and tracked-tree gitleaks scans exit 0 without suppressing the
  incident finding.
- The `main` ruleset is restored with its original constraints.
- GitHub Support has handled affected pull request refs and cached views, or has
  documented why no further server-side action is available.
- Fork owners and collaborators have received recovery instructions.
- Fresh-clone tests, lint, and builds pass.

## References

- [GitHub: Removing sensitive data from a repository](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository)
- [Gitleaks command-line documentation](https://github.com/gitleaks/gitleaks#usage)
