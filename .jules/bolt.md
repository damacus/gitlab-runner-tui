## 2024-05-18 - Remove redundant clone in worker map

**Learning:** Iterating over an owned collection (`Vec<Runner>`) by reference using `.iter()` and then cloning each element to create new structs (`ManagerRow`) results in unnecessary allocations and memory copying, slowing down the mapping process.
**Action:** Changed `runners.iter()` to `runners.into_iter()` to take ownership of the iterator, allowing elements to be moved into the new structs without cloning, resulting in a performance improvement.
