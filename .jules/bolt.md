## 2024-05-24 - API Aggregation Latency
**Learning:** Sequential API calls within asynchronous streams multiply latency unacceptably. While we limited concurrency using `buffer_unordered`, the underlying logic for enriching each runner serially awaited detail then managers. By using `tokio::join!`, both independent calls resolve concurrently. The total duration falls to `max(detail_duration, managers_duration)`.
**Action:** When gathering independent pieces of state across network bounds for a single entity, default to executing them concurrently (`tokio::join!` or `futures::join!`) before combining the result.

## 2024-05-18 - Remove redundant clone in worker map

**Learning:** Iterating over an owned collection (`Vec<Runner>`) by reference using `.iter()` and then cloning each element to create new structs (`ManagerRow`) results in unnecessary allocations and memory copying, slowing down the mapping process.
**Action:** Changed `runners.iter()` to `runners.into_iter()` to take ownership of the iterator, allowing elements to be moved into the new structs without cloning, resulting in a performance improvement.

## 2024-05-25 - Avoid cloning and sorting in TUI render loop
**Learning:** When computing values within a hot render loop (like TUI rendering), avoid performing allocations or complex calculations (like `clone()`, `sort_by()` etc.).
**Action:** We can find min and max values by scanning the slice without cloning in O(N) using `Iterator::min_by_key` and `Iterator::max_by_key`. Pre-compute these where possible or avoid allocations to ensure a smooth framerate.

## 2024-05-27 - O(N log N) Heap Allocations in Sorting Closures
**Learning:** Comparing collections (like `Vec<String>`) by converting them to strings via `.join(", ")` inside a `sort_by` closure results in severe performance degradation. Because the closure is called $O(N \log N)$ times, this causes continuous memory allocation and deallocation.
**Action:** Always compare vectors or slices directly using `.cmp()`, which delegates to lexicographical comparison of the elements without any heap allocations.

## 2024-05-27 - CI Lint Failures and Clippy Enforcement
**Learning:** The GitHub CI pipeline is configured to enforce Clippy linting strictly by treating warnings as errors. The CI failure encountered (`clippy::derivable_impls`) was caused by a manual implementation of the `Default` trait that could be auto-derived.
**Action:** Always run `mise run lint` locally before considering a change complete. This task checks all targets and features and treats warnings as errors.

## 2024-05-30 - O(N) String allocations in Hot TUI Loop via format!
**Learning:** During text-wrapping computations within `render_runner_detail`, constructing candidate strings dynamically using `format!` and immediately calling `.clone()` if line limits are exceeded creates multiple throwaway string allocations per tag *per rendering tick*.
**Action:** When computing wrapped text or dynamic strings inside hot TUI render loops, avoid using `format!` and `.clone()`. Instead, use `String::with_capacity()`, in-place concatenation (`push_str()`), and `std::mem::take()` or `std::mem::replace()` to reuse string buffers and drastically reduce heap allocations per frame.

## 2024-05-30 - O(N log N) Heap Allocations in Version Sorting
**Learning:** Parsing versions by converting segments into `Vec<u32>` and comparing strings `(Vec<u32>, String)` inside a `sort_by` closure results in severe performance degradation. Because the closure is called $O(N \log N)$ times, this causes continuous memory allocation and deallocation during TUI rendering.
**Action:** Always compare version components using lazily evaluated split iterators (e.g., `left.split('.').map(...)`). Comparing iterators natively via `.cmp()` delegating to lexicographical element comparisons avoids heap allocations completely.
