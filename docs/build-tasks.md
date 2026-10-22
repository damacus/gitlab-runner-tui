# Local and CI commands

Run `mise install`, then use the same tasks as GitHub Actions. Tool versions and
command flags live in `mise.toml`.

| Task | Purpose |
| --- | --- |
| `mise run build` | Locked debug build |
| `mise run build:release` | Locked release build |
| `mise run build:release -- --target <triple>` | Locked release build for one target |
| `mise run toolchain:target -- <triple>` | Install one target for the pinned Rust compiler |
| `mise run test` | Full Rust test suite |
| `mise run fmt` | Check formatting |
| `mise run lint` | Clippy with CI flags |
| `mise run security` | RustSec advisory checks with cargo-deny |
| `mise run security:secrets` | Scan the source tree committed at HEAD |
| `mise run security:changes` | Scan commits introduced since origin/main |
| `mise run security:changes -- <base> <head>` | Scan an explicit commit range |
| `mise run workflow:lint` | Validate Actions syntax and shell snippets |
| `mise run workflow:security` | Check Actions security configuration |

The tree scan deliberately reads the committed snapshot, matching CI. It does
not scan uncommitted edits or ignored local files. The range scan accepts
`BASE_SHA` and `HEAD_SHA` from CI, or positional refs locally. A missing or
invalid ref fails before scanning. Both scans redact all findings.

`mise run security:history` scans all reachable Git history. It remains separate
from `mise run check` while historical credential cleanup is tracked in #155.

`mise run publish -- --dry-run` verifies Cargo packaging without uploading.
`mise run publish` uploads to crates.io and requires registry credentials.
Publishing is never a dependency of the local check task.

GitHub Actions still owns job scheduling, token permissions, network policies,
artifact transfer and publication conditions. Calling a mise task does not
replace these controls. Docker compilation remains in the Dockerfile; container
build and publication isolation is tracked separately in #307.
