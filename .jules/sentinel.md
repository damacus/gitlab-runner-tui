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
