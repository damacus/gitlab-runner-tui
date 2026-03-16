## 2024-05-24 - API Aggregation Latency
**Learning:** Sequential API calls within asynchronous streams multiply latency unacceptably. While we limited concurrency using `buffer_unordered`, the underlying logic for enriching each runner serially awaited detail then managers. By using `tokio::join!`, both independent calls resolve concurrently. The total duration falls to `max(detail_duration, managers_duration)`.
**Action:** When gathering independent pieces of state across network bounds for a single entity, default to executing them concurrently (`tokio::join!` or `futures::join!`) before combining the result.

## 2024-05-18 - Remove redundant clone in worker map

**Learning:** Iterating over an owned collection (`Vec<Runner>`) by reference using `.iter()` and then cloning each element to create new structs (`ManagerRow`) results in unnecessary allocations and memory copying, slowing down the mapping process.
**Action:** Changed `runners.iter()` to `runners.into_iter()` to take ownership of the iterator, allowing elements to be moved into the new structs without cloning, resulting in a performance improvement.

## 2024-05-25 - Avoid cloning and sorting in TUI render loop
**Learning:** When computing values within a hot render loop (like TUI rendering), avoid performing allocations or complex calculations (like `clone()`, `sort_by()` etc.).
**Action:** We can find min and max values by scanning the slice without cloning in O(N) using `Iterator::min_by_key` and `Iterator::max_by_key`. Pre-compute these where possible or avoid allocations to ensure a smooth framerate.

## 2024-05-25 - Avoid polluting API data models with UI state
**Learning:** To cache expensive UI-specific string operations (like pre-computed formatting), do not leak presentation logic by adding `#[serde(default, skip)]` fields to core API data models. If the data is deserialized elsewhere or loaded from cache, these fields will silently initialize as empty strings, causing subtle data loss bugs in the UI.
**Action:** Instead, wrap the API models in a UI-specific struct (e.g., `UIRunner { runner, tags_csv }`) within the application state layer to safely hold the pre-computed strings.
