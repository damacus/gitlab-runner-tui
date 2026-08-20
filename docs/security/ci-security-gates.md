# CI security gates

The `Security` workflow prevents newly introduced Rust dependency advisories
and secrets from entering maintained branches. It deliberately separates
required new-change checks from the full-history cleanup tracked by issue #155.

## Dependency advisories

The `Dependency advisories` job uses the official cargo-deny action pinned to
commit `3c6349835b2b7b196a839186cb8b78e02f7b5f25` (`v2.1.1`). That action's
digest-pinned image embeds cargo-deny 0.20.2. It runs:

```fish
cargo deny --all-features check advisories
```

`deny.toml` checks the RustSec advisory database with all Cargo features
enabled. Vulnerable, unsound, and yanked dependencies fail the job. An
unmaintained advisory fails when it affects a workspace crate. There are no
approved exceptions.

An exception may be added only when all of these are present in the same pull
request:

1. The exact advisory ID and a reason in `deny.toml`.
2. A linked repository issue describing exposure and compensating controls.
3. A named owner and removal condition or date.
4. Evidence that a patched compatible dependency is not available.

`unused-ignored-advisory = "deny"` makes stale exceptions fail after the
dependency is fixed.

## Secret checks required on changes

The workflow installs gitleaks 8.30.1 from its official release archive after
verifying the archive's pinned SHA-256 digest. It performs two required scans:

- A current tracked-tree scan. The workflow archives `HEAD` and scans inside
  that archive, avoiding both untracked runner files and `.git` traversal.
- A commit-range scan. The workflow computes the merge base of the event's base
  and head commits and scans every introduced commit through the event head.

Both scans use 100 percent redaction. No credential values should appear in CI
logs or reports.

`.gitleaksignore` contains one exact commit/path/rule/line fingerprint for the
non-secret UI rendering sentinel that was added and later replaced on this
feature branch. It is not a regex or path-wide exception, and it does not match
the historical `.env` finding. Remove the entry after issue #155 rewrites the
synthetic marker from reachable history.

These gates prevent a new secret from entering through the current file tree or
through a commit that adds and then removes a secret within one pull request.
They do not assert that old repository history is clean.

## Full-history transition after issue #155

The manual `Full history (blocked by issue #155)` job runs:

```fish
gitleaks git --log-opts=--all --redact=100 --no-banner --no-color .
```

It is opt-in through `workflow_dispatch` and is not a required green check.
Until issue #155 is completed, it is expected to fail on the historical `.env`
credential. The already-pushed synthetic test fixture is suppressed only by
the exact fingerprint described above. The historical credential is not ignored.

After the owner-coordinated rewrite in issue #155:

1. Run the manual full-history job and require an exit status of 0.
2. Remove `Full history (blocked by issue #155)` from the job name.
3. Move the full-history scan into the required `secret-changes` job, or make it
   a separate required job on pushes to `main` and scheduled runs.
4. Add the resulting check name to default-branch ruleset `12695880`.
5. Keep the current-tree and merge-base range scans; they provide faster and
   more precise pull request feedback than a full scan alone.

Do not add an ignore for the historical `.env` finding to make the manual job
green. A history gate is enabled only when the rewritten remote and a fresh
clone both pass without that suppression.

## Local verification

Use the versions pinned by the workflow. To scan tracked working-tree content
without traversing `.git`:

```fish
set scan_dir (mktemp -d /tmp/gitlab-runner-tui-tree.XXXXXX)
git ls-files -z | tar --null -T - -cf - | tar -xf - -C $scan_dir
gitleaks dir --redact=100 --no-banner $scan_dir
```

To scan only commits introduced on the current branch:

```fish
set merge_base (git merge-base origin/main HEAD)
gitleaks git --log-opts="$merge_base..HEAD" --redact=100 --no-banner .
```
