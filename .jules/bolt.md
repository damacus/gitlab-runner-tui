## 2024-05-24 - API Aggregation Latency
**Learning:** Sequential API calls within asynchronous streams multiply latency unacceptably. While we limited concurrency using `buffer_unordered`, the underlying logic for enriching each runner serially awaited detail then managers. By using `tokio::join!`, both independent calls resolve concurrently. The total duration falls to `max(detail_duration, managers_duration)`.
**Action:** When gathering independent pieces of state across network bounds for a single entity, default to executing them concurrently (`tokio::join!` or `futures::join!`) before combining the result.

## 2024-05-18 - Remove redundant clone in worker map

**Learning:** Iterating over an owned collection (`Vec<Runner>`) by reference using `.iter()` and then cloning each element to create new structs (`ManagerRow`) results in unnecessary allocations and memory copying, slowing down the mapping process.
**Action:** Changed `runners.iter()` to `runners.into_iter()` to take ownership of the iterator, allowing elements to be moved into the new structs without cloning, resulting in a performance improvement.
