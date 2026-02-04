# Rust Best Practices (rust-besty)

## Error Handling

- Use `anyhow::Result` for application code (propagates errors with context)
- Use `thiserror` for library code (custom error types with `#[derive(Error)]`)
- Add context with `.context("what failed")` or `.with_context(|| format!("..."))`
- Prefer `?` operator over `.unwrap()` or `.expect()` in production code
- Reserve `.expect("reason")` for cases that truly indicate programmer error

```rust
// Good: context explains what failed
let config = load_config().context("Failed to load config")?;

// Avoid in production
let config = load_config().unwrap(); // panics without context
```

## Naming Conventions (RFC 430)

- Types/Traits: `PascalCase` (e.g., `RunnerManager`, `GitLabClient`)
- Functions/Methods: `snake_case` (e.g., `fetch_runners`, `get_status`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_RETRIES`)
- Conversion methods: `as_` (borrowed), `to_` (expensive/new), `into_` (consumes self)
- Getters: no `get_` prefix (e.g., `runner.status()` not `runner.get_status()`)
- Iterators: `iter()`, `iter_mut()`, `into_iter()`

## Derive Common Traits

Eagerly implement standard traits on public types:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MyType { ... }

// For serialization (with serde)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ApiResponse { ... }
```

Essential traits to consider: `Debug`, `Clone`, `Default`, `PartialEq`, `Eq`, `Hash`, `Serialize`, `Deserialize`

## Struct Construction

- Use `Default` trait for types with sensible defaults
- Use Builder pattern for complex construction with many optional fields
- Structs should have private fields for future-proofing

```rust
// Derive Default when all fields have sensible defaults
#[derive(Default)]
pub struct Config {
    timeout: Duration,  // defaults to zero
    retries: u32,       // defaults to 0
}

// Use partial initialization
let config = Config {
    timeout: Duration::from_secs(30),
    ..Default::default()
};
```

## Async Patterns

- Use `tokio` as the async runtime
- Prefer `async fn` over manual `Future` implementations
- Use `tokio::select!` for concurrent operations with cancellation
- Avoid blocking operations in async contexts

```rust
tokio::select! {
    result = async_operation() => handle(result),
    _ = tokio::time::sleep(timeout) => handle_timeout(),
}
```

## Option and Result Handling

- Use `?` for early returns on errors
- Use `.map()`, `.and_then()`, `.unwrap_or_default()` for transformations
- Prefer `if let` or `match` over `.is_some()` + `.unwrap()`
- Use `Option::take()` to replace with `None` and get owned value

```rust
// Good: pattern matching
if let Some(value) = option {
    use_value(value);
}

// Good: chaining
let result = option.map(|v| v.process()).unwrap_or_default();

// Avoid
if option.is_some() {
    let value = option.unwrap();  // redundant check
}
```

## Memory Efficiency

- Use `mem::take()` to move owned values out while leaving defaults
- Avoid unnecessary `.clone()` - the borrow checker guides you
- Use `Cow<str>` when ownership is conditional
- Prefer `&str` over `String` in function parameters when possible

## Documentation

- Add `///` doc comments on all public items
- Include examples in doc comments (they become tests!)
- Document panics, errors, and safety considerations
- Use `#[must_use]` on functions where ignoring the result is likely a bug

```rust
/// Fetches runners from the GitLab API.
///
/// # Errors
/// Returns an error if the API request fails or response is invalid.
///
/// # Example
/// ```
/// let runners = client.fetch_runners(&filters).await?;
/// ```
pub async fn fetch_runners(&self, filters: &Filters) -> Result<Vec<Runner>> {
    // ...
}
```

## Testing

- Co-locate unit tests in the same file with `#[cfg(test)]`
- Use `mockito` or similar for HTTP mocking
- Test deserialization with realistic JSON fixtures
- Use `#[tokio::test]` for async tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsing() {
        let json = r#"{"id": 1, "status": "online"}"#;
        let runner: Runner = serde_json::from_str(json).unwrap();
        assert_eq!(runner.id, 1);
    }

    #[tokio::test]
    async fn test_async_operation() {
        // ...
    }
}
```

## References

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/)
- [The Rust Book](https://doc.rust-lang.org/book/)
