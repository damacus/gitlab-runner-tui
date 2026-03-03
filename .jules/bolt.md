
## 2024-03-03 - [Concurrent GitLab API Requests]
**Learning:** Sequential API calls within a single iterator processing chain can introduce unnecessary latency. In `Conductor::fetch_runners`, calling `fetch_runner_detail` followed by `fetch_runner_managers` sequentially resulted in delayed runner augmentation.
**Action:** Used `tokio::join!` to execute independent asynchronous API requests concurrently for each runner. This pattern halves the individual runner fetching latency while preserving the concurrency limits enforced by `buffer_unordered`. Always check if successive `await` calls in an asynchronous block depend on each other, and if not, execute them concurrently.
