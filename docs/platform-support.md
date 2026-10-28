# Platform support

## Release archives

| Archive | Platform | C library | Baseline |
| --- | --- | --- | --- |
| `gitlab-runner-tui-linux-amd64.tar.gz` | Linux x86-64 | glibc | glibc >= 2.31 |
| `gitlab-runner-tui-linux-arm64.tar.gz` | Linux ARM64 | glibc | glibc >= 2.31 |
| `gitlab-runner-tui-linux-amd64-musl.tar.gz` | Linux x86-64 | none (static musl) | no libc requirement |
| `gitlab-runner-tui-linux-arm64-musl.tar.gz` | Linux ARM64 | none (static musl) | no libc requirement |
| `gitlab-runner-tui-macos-amd64.tar.gz` | macOS Intel | — | — |
| `gitlab-runner-tui-macos-arm64.tar.gz` | macOS Apple silicon | — | — |

## glibc baseline

The GNU archives are built with `cargo zigbuild` against glibc **2.31**, the
version shipped by Debian 11. They run on Debian 11+, Ubuntu 20.04+,
Fedora 32+, and RHEL 9+. Distributions below the floor — RHEL 8 (glibc 2.28),
Ubuntu 18.04 (2.27) — should use a `-musl` archive or a container image.

The ELF may also list *weak* symbol versions newer than 2.31 (for optional
features such as `pidfd_*`). Weak references are skipped when the symbol is
absent; they do not prevent startup on the baseline.

Every release runs the GNU amd64 and arm64 archives on `debian:11` containers
before assets are published: `fetch --help` and `fetch --demo` must succeed,
and the binary must not import any mandatory `GLIBC_` symbol version above
2.31.

## musl static archives

The `-musl` archives are fully static: `readelf -d` shows no `NEEDED` entries.
Use them on distributions older than the glibc floor, on musl-based systems
such as Alpine, or in minimal containers. Release smoke tests run them on
`alpine` images and verify the static link.

## Kernel baseline

The verified floor is **Linux kernel 4.18**, tested on a RHEL 8.10 host
(`4.18.0-553.174.1.el8_10.x86_64`) running the released binaries and the
published container images.

Two caveats:

- **Containers share the host kernel.** Choosing the `-ci` or runtime image
  changes the userspace (shell, libc, tools), not the kernel requirement. A
  container test on a modern host proves nothing about an older kernel.
- **The ELF ABI note is not evidence.** The GNU binary's Linux ABI note
  records the toolchain's declared minimum, not a tested configuration. Its
  value can change with the toolchain.
  The 4.18 floor above is the only verified minimum; the musl binary carries
  no ABI note at all.

To re-verify the kernel floor, run `gitlab-runner-tui fetch --demo` and the
container smoke commands below on a VM or host running the oldest kernel you
intend to support.

## Container images

Each release publishes two images:

- `ghcr.io/damacus/gitlab-runner-tui` — a scratch image containing only the
  static musl binary and a CA bundle. Its entrypoint is the application and it
  has **no shell**. Use it for `docker run` invocations; it cannot execute a
  GitLab job script.
- `ghcr.io/damacus/gitlab-runner-tui-ci` — an Alpine image with `sh`, `bash`,
  `grep`, `jq`, and `ca-certificates`, and no application entrypoint. Use this
  image for GitLab CI jobs.

Both images set `GITLAB_RUNNER_TUI_CONTAINER=1` so the application detects
container mode.

### Use in GitLab CI

```yaml
runner-inventory:
  image: ghcr.io/damacus/gitlab-runner-tui-ci:0.1.21
  variables:
    GITLAB_HOST: https://gitlab.example.com
    # Set GITLAB_TOKEN as a masked CI/CD variable.
  script:
    - gitlab-runner-tui fetch --summary | jq '.runners | length'
```

Requirements:

- `GITLAB_TOKEN` as a masked CI/CD variable; the application never accepts a
  token as a command-line argument.
- A writable working directory. The application creates a `logs/` directory in
  its current working directory; a normal GitLab checkout provides this.
- No extra CA setup: the image ships `/etc/ssl/certs/ca-certificates.crt`.
- A runner host on kernel >= 4.18 (see above).

Do not use the runtime image as a GitLab job image: it has no shell for
GitLab Runner to execute the job script, and its entrypoint is the
application itself.

Note that `--demo` requires a CA bundle even though it makes no network
requests — the TLS client is constructed before fixture data is loaded.
