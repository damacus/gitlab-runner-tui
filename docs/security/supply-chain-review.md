# Supply chain review, 22 October 2026

This pass covers the checked-in workflows, build tasks, dependencies, credentials
and obsolete agent files. It does not establish that maintainer accounts or
previously published releases are uncompromised.

## Threat and priorities

The [LiteLLM incident report](https://docs.litellm.ai/blog/security-incident-oct-21-2026#how-it-was-undermined)
describes stolen developer credentials overriding branch protection, changing
the release source, approving publication and deleting workflow records. Signed
provenance alone cannot establish that a privileged workflow used approved code.

The live GitHub settings inspected during this review had an administrator
bypass, no required PR, only five older CI checks, Actions PR approval enabled
and no platform-enforced action SHA pinning. Repository files cannot enforce
these settings against a compromised administrator.

Recommended settings are mandatory PRs, the existing CI checks plus dependency,
secret and workflow checks, no administrator bypass, read-only default tokens,
disabled Actions PR approvals and mandatory full action SHAs. A second-person
approval requirement needs another available maintainer; CODEOWNERS naming the
sole author does not provide independent review.

## Changes in this pass

- The GitLab client rejects HTTP redirects. A loopback regression reproduced
  forwarding the custom PRIVATE-TOKEN header to another server before the fix.
  Configure the canonical GitLab host rather than a redirecting URL.
- Cargo.lock updates rustls to 0.23.46, rustls-webpki to 0.103.16 and
  webpki-root-certs to 1.0.10, addressing the advisory check's TLS findings.
- Builds and tests use locked resolution. The follow-up build stack shares
  security and release commands through mise tasks and reads the pinned Rust
  compiler version from `mise.toml` in both local and CI runs.
- Release preflight verifies that the version tag resolves to the triggering
  commit and waits for successful CI and Security push runs on that exact SHA.
  Source checkouts use the event SHA. Release builds restore no compiled Cargo
  cache. The asset upload job retains its checkout and pinned upload action;
  their removal or replacement is deferred. The tag is rechecked before upload.
- Renovate changes need review and have a seven-day release-age delay. This
  reduces exposure to newly compromised versions; it is not proof of safety.
- Actionlint and offline zizmor check workflows. CI runs gitleaks directly;
  no Python wrapper or Python test suite is part of this repository.
- Daily dependency and current-tree secret checks supplement PR checks.
- Obsolete .claude and .jules tracked files and local .gemini, .serena and
  .superpowers state were removed. Generic AGENTS.md, .github/skills and design
  records under docs remain useful. The two pre-existing stashes are preserved.

## Harden-Runner assessment

[Harden-Runner](https://github.com/step-security/harden-runner) is useful here.
The public repository and GitHub-hosted runners qualify for the free Community
tier. The pinned action is first in every job. Audited Linux CI, secret,
advisory, workflow-lint and PR-title jobs now use block mode with per-job
allowlists; macOS, Windows, release and manual history jobs remain in audit mode.
It records runner network activity and can surface unexpected destinations.
This sends security telemetry to StepSecurity. Windows and macOS support audit
mode; Linux also supports endpoint blocking.

The policies come from CI run 39765402589, Security run 39765402754 and
PR-title run 39767094701. The agent logs identify the destination, port and
calling process. GitHub Actions cache/result storage and StepSecurity control
traffic already have built-in agent allowances; no broad new wildcard or
unexplained domain was added. All explicit entries use HTTPS port 443.

CI permits GitHub checkout and release assets, mise version lookup, the Rust
distribution server, and the Cargo index and package CDN. Secret scans permit
only GitHub and its release asset server. Advisory checks permit GitHub and
the Cargo index and CDN. Workflow lint permits GitHub, release assets and mise
version lookup. The existing PR-title action installs Python dependencies and
verifies GitHub attestations; its audited destinations are PyPI, its package
CDN, the GitHub API and the specific Sigstore and attestation storage hosts.
That third-party implementation does not add Python files to this repository.

The first enforced run, 39767872665, blocked
`tuf-repo-cdn.sigstore.dev` during mise download verification. The cached audit
had not exercised this metadata refresh. This exact HTTPS host was added to
the mise-installing jobs; signature verification remains enabled.

Release publication and manual full-history scans need their own audits before
blocking is enabled. One observed run cannot establish every future download
or publishing path; new blocked destinations require investigation, not an
automatic wider allowlist.

The action itself adds privileged code and a vendor dependency. Pinning its
wrapper does not pin every service or runtime component it fetches. It cannot
prevent an administrator from deleting the action, changing repository rules,
or publishing through another path. Retained external audit records and strong
maintainer authentication are separate controls.

## Deferred housekeeping and build efficiency

The owner narrowed this PR to security fixes and the requested agent-folder
cleanup. Existing CI provisioning, cache layout, debug builds, test invocation,
local check scheduling, artifact compression and retention are preserved.
Removing the asset-upload checkout, replacing its pinned upload action and
extracting shared scanner setup are separate changes.
These optional changes are tracked in
[issue #310](https://github.com/damacus/gitlab-runner-tui/issues/310).

No matched CPU, peak RSS, disk-use or end-to-end timing benchmark has established
the optimal runner or job arrangement. Build optimisations, including Docker
compiling both architectures through QEMU separately from downloadable releases,
are deferred. See issue #307 for container build and publication isolation.

## Remaining controls and limits

1. Historical secret cleanup remains unresolved. Redacted scans of fetched
   origin/main and all local refs each found three findings: the historical
   .env credential matches two rules and a known UI sentinel matches one. Issue
   #155 was closed and has now been reopened with this evidence. Keep the full-history job opt-in and
   failing until rotation and coordinated history cleanup are proved. No ignore
   for the historical credential is approved.
2. [Issue #307](https://github.com/damacus/gitlab-runner-tui/issues/307):
   container publication still compiles with packages:write available. It uses
   QEMU, mutable base image tags and caches. The follow-up should separate build,
   test and publish, pin image digests and emit an SBOM and verified provenance.
3. [Issue #308](https://github.com/damacus/gitlab-runner-tui/issues/308) tracks
   the remaining runner and release controls. Crate publication still exposes a long-lived registry token to Cargo build
   scripts during verification. Move to crates.io trusted publishing after
   confirming compatibility with the current unpublished Ratatui git patch.
   Preserve Cargo's package verification; do not use --no-verify as a shortcut.
4. Protect release tags against updates and deletion and consider immutable
   GitHub releases. Account for release-please's need to create releases before
   adding an environment gate or tag rules. A separate reviewer provides more
   protection than an approval from the same compromised account.
5. Review maintainer OAuth grants, use phishing-resistant authentication and
   avoid mounting or forwarding gh credentials into services or agent sessions.
   Export audit logs to storage a repository administrator cannot delete.
6. Consider dependency review, a locked-source allowlist in cargo-deny,
   cargo-vet for reviewed dependency changes, and a pinned secret scanner for
   container layers. Add each when it has an owner and an actionable gate.

No new composite action is needed to fix the identified security issues.
Reimplementing checkout, cryptographic signing or egress monitoring would add
security-sensitive code without a clear benefit.
