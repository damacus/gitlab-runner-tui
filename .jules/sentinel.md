## 2024-05-20 - Prevent Credential Leakage in Debug Logs
**Vulnerability:** The `AppConfig` struct derived the `Debug` trait automatically, which would print the plaintext `gitlab_token` (a sensitive personal access token) if the configuration was ever logged or formatted with `{:?}`.
**Learning:** In Rust applications, automatically deriving `Debug` for structs containing secrets is a common anti-pattern that can lead to credential leakage in logs or console output.
**Prevention:** Manually implement `std::fmt::Debug` for structs containing sensitive data and explicitly redact those fields (e.g., using `[REDACTED]`). Include unit tests to verify the redaction logic.
