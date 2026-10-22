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
- Builds and tests use locked resolution. CI and release tools share the
  repository's pinned Rust version through mise.
- Release preflight verifies that the version tag resolves to the triggering
  commit and waits for successful CI and Security push runs on that exact SHA.
  Source checkouts use the event SHA. Release builds restore no compiled Cargo
  cache, and the asset upload job does not check out or compile source.
- GitHub CLI replaces the third-party release asset upload action. The CLI
  rechecks the tag before uploading and refuses to overwrite existing assets.
- Renovate changes need review and have a seven-day release-age delay. This
  reduces exposure to newly compromised versions; it is not proof of safety.
- Actionlint and offline zizmor check workflows and the local composite action.
  Release gate tests exercise success, wrong source, failed checks, pending
  checks, API errors and invalid tags without contacting GitHub.
- A small local composite action centralises checksum-verified gitleaks
  installation. It avoids duplicate download logic and sudo. Its narrow PATH
  exception is documented next to the runner-owned temporary directory write.
- Daily dependency and current-tree secret checks supplement PR checks.
- Job timeouts and cancellation of superseded CI runs bound wasted resources.
- Obsolete .claude and .jules tracked files and local .gemini, .serena and
  .superpowers state were removed. Generic AGENTS.md, .github/skills and design
  records under docs remain useful. The two pre-existing stashes are preserved.

## Harden-Runner assessment

[Harden-Runner](https://github.com/step-security/harden-runner) is useful here.
The public repository and GitHub-hosted runners qualify for the free Community
tier. The pinned action is first in every job and initially uses audit mode.
It records runner network activity and can surface unexpected destinations.
This sends security telemetry to StepSecurity. Windows and macOS support audit
mode; Linux also supports endpoint blocking.

Audit mode is not an outbound allowlist. After successful cold and warm runs,
review telemetry separately for builds, dependency scans, release uploads and
container publishing, then trial Linux block mode with observed destinations.
Do not guess the Cargo, GitHub, tool download and registry endpoint list.

The action itself adds privileged code and a vendor dependency. Pinning its
wrapper does not pin every service or runtime component it fetches. It cannot
prevent an administrator from deleting the action, changing repository rules,
or publishing through another path. Retained external audit records and strong
maintainer authentication are separate controls.

## Build resource assessment

The existing setup repeated debug compilation in a separate Build job even
though tests already compile the application. Only Test cached Rust output,
with three caches whose keys omitted the Rust toolchain. The local check task
started competing Cargo processes, and an integration test spawned cargo run.

This pass removes the duplicate debug build, uses toolchain-aware dependency
caches for each CI job, permits cache saves only on main pushes, disables CI
incremental output and keeps line-table debug information rather than full
debug information. Local incremental compilation remains available. The local
check task runs serially and the integration test executes Cargo's built binary.

These remove known duplicate work and limit CI disk use. No matched CPU, peak
RSS, disk-use or end-to-end timing benchmark has established the optimal runner
or job arrangement. Clean release builds deliberately trade cache speed for a
smaller trust boundary. Do not call this configuration maximally efficient.

The largest remaining waste is Docker compiling both architectures through
QEMU, separately from downloadable releases, for runtime and CI images with
separate caches. The owner deferred that redesign. Package already-tested musl
binaries in both images, then smoke-test before registry publication. Keep this
separate from the Linux compatibility work in PR #306.

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

Prefer existing small tools and short reviewed scripts. A custom action is
justified for shared repository-specific behaviour such as the verified scanner
installer. Reimplementing checkout, cryptographic signing or egress monitoring
would add security-sensitive code without a clear benefit.
