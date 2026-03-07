## 2024-05-20 - Prevent Credential Leakage in Debug Logs
**Vulnerability:** The `AppConfig` struct derived the `Debug` trait automatically, which would print the plaintext `gitlab_token` (a sensitive personal access token) if the configuration was ever logged or formatted with `{:?}`.
**Learning:** In Rust applications, automatically deriving `Debug` for structs containing secrets is a common anti-pattern that can lead to credential leakage in logs or console output.
**Prevention:** Manually implement `std::fmt::Debug` for structs containing sensitive data and explicitly redact those fields (e.g., using `[REDACTED]`). Include unit tests to verify the redaction logic.

## 2024-03-03 - Secure Request Headers
**Vulnerability:** GitLab API token (`PRIVATE-TOKEN`) was stored as a raw String in the client struct and manually added to each request. If HTTP tracing/logging was enabled, this token would be logged in plain text.
**Learning:** `reqwest` does not automatically redact custom authentication headers like `PRIVATE-TOKEN`.
**Prevention:** Always add authentication tokens as default headers to the `reqwest::Client` builder, and explicitly mark the `HeaderValue` as sensitive using `.set_sensitive(true)` to prevent it from leaking into debug logs or error traces.

## 2025-03-04 - Insecure Error Handling (Panic on Missing Token)
**Vulnerability:** The application panics using `.expect()` when a GitLab token is not provided via CLI args, env variables, or config. This abrupt termination can lead to Denial of Service and potentially expose internal stack traces depending on the environment.
**Learning:** Using `expect` or `unwrap` on critical initialization paths in a Rust application is a poor practice that bypasses graceful error handling and cleanup logic.
**Prevention:** Always use idiomatic error propagation mechanisms like `anyhow::Context` and the `?` operator to return descriptive errors to the `main` function for a controlled exit.

## 2024-05-24 - Exposing Sensitive Data via `Debug` traits

**Vulnerability:**
Sensitive fields, such as GitLab tokens, were exposed via the `Debug` trait when structs holding those values (like `Args` and `AppConfig`) used `#[derive(Debug)]`. This can lead to tokens being unintentionally leaked in logs or console output if the application ever prints or logs debug information about these structs.

**Learning:**
Using `#[derive(Debug)]` on structs containing sensitive credentials is an easy way to inadvertently introduce a security vulnerability. The auto-generated `fmt` implementation will print out all fields, including secrets.

**Prevention:**
Remove the `Debug` trait from the `#[derive(...)]` attribute of any structs containing sensitive fields. Instead, provide a manual implementation of `std::fmt::Debug` where sensitive fields are explicitly redacted (e.g., `.field("gitlab_token", &self.gitlab_token.as_ref().map(|_| "[REDACTED]"))`).
## 2026-03-07 - Remove `.env` and `config.toml` loading from CWD
**Vulnerability:** The application was loading `dotenvy::dotenv().ok()` and `config.toml` from the current working directory (`CWD`), which could allow a malicious actor to place a compromised configuration file containing rogue credentials (credential hijacking) if a user executed the binary from an untrusted directory.
**Learning:** Loading environment variables or configurations from the current working directory introduces unexpected and insecure behaviors, leading to potential hijacking or execution of unauthorized commands if secrets or critical endpoints are overridden.
**Prevention:** Do not load configuration files or environment variables (such as via `dotenvy`) from the current working directory. Always load user-specific configuration securely from dedicated home directories (e.g., `~/.config/igor/config.toml`).
