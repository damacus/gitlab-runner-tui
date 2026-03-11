## 2024-05-24 - API Aggregation Latency
**Learning:** Sequential API calls within asynchronous streams multiply latency unacceptably. While we limited concurrency using `buffer_unordered`, the underlying logic for enriching each runner serially awaited detail then managers. By using `tokio::join!`, both independent calls resolve concurrently. The total duration falls to `max(detail_duration, managers_duration)`.
**Action:** When gathering independent pieces of state across network bounds for a single entity, default to executing them concurrently (`tokio::join!` or `futures::join!`) before combining the result.

## 2024-05-18 - Remove redundant clone in worker map

**Learning:** Iterating over an owned collection (`Vec<Runner>`) by reference using `.iter()` and then cloning each element to create new structs (`ManagerRow`) results in unnecessary allocations and memory copying, slowing down the mapping process.
**Action:** Changed `runners.iter()` to `runners.into_iter()` to take ownership of the iterator, allowing elements to be moved into the new structs without cloning, resulting in a performance improvement.

## 2024-05-25 - Avoid cloning and sorting in TUI render loop
**Learning:** When computing values within a hot render loop (like TUI rendering), avoid performing allocations or complex calculations (like `clone()`, `sort_by()` etc.).
**Action:** We can find min and max values by scanning the slice without cloning in O(N) using `Iterator::min_by_key` and `Iterator::max_by_key`. Pre-compute these where possible or avoid allocations to ensure a smooth framerate.
## 2024-11-20 - Pre-compute string joins in TUI render loop
**Learning:** Calling `.join(", ")` on vector fields directly inside the ratatui `render` loop causes string allocations on every single frame. When rendering lists of runners, this results in significant garbage collection/memory overhead and framerate drops.
**Action:** Always pre-compute concatenated strings during the data fetch phase or state update logic and store them as owned `String` fields on the models, passing only references (`&String` or `&str`) to the TUI components.
