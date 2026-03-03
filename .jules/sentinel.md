## 2024-03-03 - Secure Request Headers
**Vulnerability:** GitLab API token (`PRIVATE-TOKEN`) was stored as a raw String in the client struct and manually added to each request. If HTTP tracing/logging was enabled, this token would be logged in plain text.
**Learning:** `reqwest` does not automatically redact custom authentication headers like `PRIVATE-TOKEN`.
**Prevention:** Always add authentication tokens as default headers to the `reqwest::Client` builder, and explicitly mark the `HeaderValue` as sensitive using `.set_sensitive(true)` to prevent it from leaking into debug logs or error traces.
