# Changelog

## [0.1.10](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.9...v0.1.10) (2026-04-23)


### Performance Improvements

* ⚡ Bolt: Optimize tag sorting by comparing slices directly ([#114](https://github.com/damacus/gitlab-runner-tui/issues/114)) ([21527f8](https://github.com/damacus/gitlab-runner-tui/commit/21527f82877929425cde0698b0072101b5bfbc7a))

## [0.1.9](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.8...v0.1.9) (2026-03-24)


### Features

* add --demo flag, extract run_tui(), wire up run_demo() ([b8455f2](https://github.com/damacus/gitlab-runner-tui/commit/b8455f28f62d9effe12d0dce5451df9e43bdc516))
* add AND/OR toggle for popup tag filtering, rename filter bar ([0192961](https://github.com/damacus/gitlab-runner-tui/commit/01929616530680fd20bb1b739db55a9b622a77dd))
* add App::seed_demo_data() for injecting fixture runners ([cf7aad5](https://github.com/damacus/gitlab-runner-tui/commit/cf7aad5268c0068690bd9f7e8f55a9f3ea072b6b))
* add CLI JSON output, one-shot mode, and fix build warnings ([db38359](https://github.com/damacus/gitlab-runner-tui/commit/db383597bb96994398e55c597495ce5868e5aee2))
* Add colours ([7d199d1](https://github.com/damacus/gitlab-runner-tui/commit/7d199d1de70634045cc238fe289683c8a397c9d9))
* add demo_mode flag to App, guard start_search ([de1f54d](https://github.com/damacus/gitlab-runner-tui/commit/de1f54d2134af36ba98d4f56c440b40fc781b7d4))
* add fixtures module with demo_runners() ([fc8a457](https://github.com/damacus/gitlab-runner-tui/commit/fc8a45719859f9dcfdd5631facd024289910455a))
* add screenshot recording script and README Preview section ([8c8e06d](https://github.com/damacus/gitlab-runner-tui/commit/8c8e06dab719d59ee1ebc27dc469455a13587965))
* add tag search filter to filter popup, drop igor compat ([8d1528f](https://github.com/damacus/gitlab-runner-tui/commit/8d1528f506c539ac3d3a7bde7f166f2716b30170))
* adopt ratatui 0.28-0.30 API improvements ([d5702f1](https://github.com/damacus/gitlab-runner-tui/commit/d5702f1b78f25b61f5d2f9856073a87cc9f7a270))
* compact top bar with semantic tab chips and subtitle ([0e8ce21](https://github.com/damacus/gitlab-runner-tui/commit/0e8ce2181c55928da07b1caa6cb15485a3282883))
* enrich runner detail panel with group, URL, paused state, and registration date ([03d7536](https://github.com/damacus/gitlab-runner-tui/commit/03d7536c509c466338f535df55ff1b947cab8465))
* Enter expands runner detail drawer on all tabs ([a1c8d46](https://github.com/damacus/gitlab-runner-tui/commit/a1c8d46ec0f6035d67c358fa3940a20f5211b60c))
* open selected runner in browser on Enter (↵ open in status bar) ([b23258c](https://github.com/damacus/gitlab-runner-tui/commit/b23258c8463c43861ea48720c13bd40dfd20b95f))
* **tui:** add highlight symbol and spacing to tables to prevent layout jump and improve row selection visibility ([9fbfba8](https://github.com/damacus/gitlab-runner-tui/commit/9fbfba8d573b6e33db03703ceebc16db49fb837b))
* **tui:** add visual status symbols to tables for better signal density ([7b1335e](https://github.com/damacus/gitlab-runner-tui/commit/7b1335ef1990631c34d2bb950026d35925c3bb94))
* **tui:** elevate UI with rounded borders, styled titles, and Gemini-style gradients ([29cd11a](https://github.com/damacus/gitlab-runner-tui/commit/29cd11ad68ca416bcf47a5880d51a36b4be2d3d9))
* **tui:** elevate UI with rounded borders, styled titles, and Gemini-style gradients ([2fd1671](https://github.com/damacus/gitlab-runner-tui/commit/2fd1671932746607bb3310664e528b259259442b))
* **tui:** improve status visibility and standardize mode-specific help hints ([83cbce2](https://github.com/damacus/gitlab-runner-tui/commit/83cbce2af4a53480d1cfba6938a3576b0105bcd8))


### Bug Fixes

* **security:** hide GITLAB_TOKEN value in help output and add regression test ([2c9b5ef](https://github.com/damacus/gitlab-runner-tui/commit/2c9b5ef581c15038ab397551975bdfc2621c0a21))
* seed_demo_data resets fetch state fields, test asserts runners populated ([e856485](https://github.com/damacus/gitlab-runner-tui/commit/e85648580d80dfa07e5d23d74d3e6fda1d47191a))

## [0.1.8](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.7...v0.1.8) (2026-03-18)


### Features

* 🎨 Palette: Add cursor to filter input ([#58](https://github.com/damacus/gitlab-runner-tui/issues/58)) ([4517a32](https://github.com/damacus/gitlab-runner-tui/commit/4517a322a7e862fde14cc202433cd332f5c3ff62))
* 🎨 Palette: Add empty states for TUI tables ([#53](https://github.com/damacus/gitlab-runner-tui/issues/53)) ([c1b57e2](https://github.com/damacus/gitlab-runner-tui/commit/c1b57e227feccbc30c33547347a79c3d7d0a0bae))
* 🎨 Palette: Improve filter input and rotation empty state UX ([#61](https://github.com/damacus/gitlab-runner-tui/issues/61)) ([12822ac](https://github.com/damacus/gitlab-runner-tui/commit/12822ac7d82a939e08c2924a5f86cc195e930103))
* add AllRunners discovery mode using /runners/all with fallback to /runners ([a10beeb](https://github.com/damacus/gitlab-runner-tui/commit/a10beebafcc656b26f37a689b58b865d53003b2e))
* add extract_runner_tags function ([b17f142](https://github.com/damacus/gitlab-runner-tui/commit/b17f142970fdaa65929e809aee7194aa7019d567))
* add FilterPopup key handler, update dashboard bindings (f popup, t text, remove v) ([7d93c86](https://github.com/damacus/gitlab-runner-tui/commit/7d93c8696a171546a208eb3d78404002f801438a))
* add FilterPopupSection type and filter popup state fields to App ([1b6a028](https://github.com/damacus/gitlab-runner-tui/commit/1b6a028d89a81b90f62233378528ab1666cf2bf2))
* add open_filter_popup, toggle_selected_tag, selected_tags_summary ([a9cc283](https://github.com/damacus/gitlab-runner-tui/commit/a9cc283290ac3c4a20e49a1c9c1a14b53d7fb0f1))
* add render_filter_popup, update filter bar, status bar, help view ([1be7edc](https://github.com/damacus/gitlab-runner-tui/commit/1be7edcae68032e761fc54ab9829c1aaafcec85c))
* add Tags: to runner detail section ([978967c](https://github.com/damacus/gitlab-runner-tui/commit/978967cee14f732e9de93661459358e398c57d82))
* improve token masking in settings modal ([897848b](https://github.com/damacus/gitlab-runner-tui/commit/897848b18faf0e3ea9fde4b02927a305a0777122))
* merge popup and text tags in build_filters, retain selected_tags in apply_view_state ([f432b79](https://github.com/damacus/gitlab-runner-tui/commit/f432b796d6186b23ed06c4bbbb10bdf60c878a57))
* non-blocking background search keeps TUI responsive during large queries ([d520f13](https://github.com/damacus/gitlab-runner-tui/commit/d520f136dd192b8478fa32e132e6320e1e8ba449))
* overhaul sorting to cycle through columns from left to right ([a441cce](https://github.com/damacus/gitlab-runner-tui/commit/a441cce1484a67592cfc0213f16f93289a7fdc77))
* polish runner detail pane, remove age filter, improve status bar ([8a5ca03](https://github.com/damacus/gitlab-runner-tui/commit/8a5ca03c55fdb0a898140c83a56945e27b72ffa4))
* **security:** Fix credential leakage in debug output ([#38](https://github.com/damacus/gitlab-runner-tui/issues/38)) ([ed3ece1](https://github.com/damacus/gitlab-runner-tui/commit/ed3ece1629285397b4284443b44662e93f6ce57a))


### Bug Fixes

* 🔒 prevent panic on missing token by using anyhow Context ([#45](https://github.com/damacus/gitlab-runner-tui/issues/45)) ([737aedb](https://github.com/damacus/gitlab-runner-tui/commit/737aedb382d167a345f7f5ab5631993bf09d9c39))
* correct 403 fallback detection and show actual endpoint in settings ([eac16f5](https://github.com/damacus/gitlab-runner-tui/commit/eac16f5c1e4c465a0148ef1462e30e4672161748))
* correct sort shortcut label and add header indicators ([3ea2d61](https://github.com/damacus/gitlab-runner-tui/commit/3ea2d612ad818409e1f14034224d0b536921bb8f))
* default to all-runners discovery, restore 403 fallback, show active endpoint ([f5352eb](https://github.com/damacus/gitlab-runner-tui/commit/f5352eb09224ea1058a33e63ed7cf82f5122ab94))
* display human-readable runner type in detail panel (project/group/instance) ([08b72e1](https://github.com/damacus/gitlab-runner-tui/commit/08b72e1ab62b2ef8cafaa65650ec7cb69cbaf25e))
* remove silent fallback, add d-key endpoint cycle, always show description ([e15132b](https://github.com/damacus/gitlab-runner-tui/commit/e15132b2ad0c68d3525570e292c9da5aa302850d))
* **ui:** Add visible cursor to filter input ([#40](https://github.com/damacus/gitlab-runner-tui/issues/40)) ([aec3a02](https://github.com/damacus/gitlab-runner-tui/commit/aec3a0220fbd1457491e84016673e6791da0803e))


### Performance Improvements

* ⚡ Bolt: concurrent runner detail and manager fetch ([#51](https://github.com/damacus/gitlab-runner-tui/issues/51)) ([0396e87](https://github.com/damacus/gitlab-runner-tui/commit/0396e8730fe0926362d5beaaf4f2b6012b3a858b))
* ⚡ Bolt: Remove redundant clone in worker map ([#47](https://github.com/damacus/gitlab-runner-tui/issues/47)) ([c6ac665](https://github.com/damacus/gitlab-runner-tui/commit/c6ac66515453afb992ce559fbcc55d607b7cb349))
* ⚡ Bolt: Remove redundant string cloning in TUI render loop ([#62](https://github.com/damacus/gitlab-runner-tui/issues/62)) ([25cb6bf](https://github.com/damacus/gitlab-runner-tui/commit/25cb6bfec3e5e80e19adb9dcada1a11ec93ed4d2))
* ⚡ Bolt: replace clone and sort in TUI render loop with min/max_by_key ([#57](https://github.com/damacus/gitlab-runner-tui/issues/57)) ([582bc1a](https://github.com/damacus/gitlab-runner-tui/commit/582bc1a85da17ea361344dcf844a95abb0d5861a))
* ⚡ Bolt: Use const styles in render loop ([#43](https://github.com/damacus/gitlab-runner-tui/issues/43)) ([897c441](https://github.com/damacus/gitlab-runner-tui/commit/897c441423b5768fc8d7888816d842bd7b4ff390))

## [0.1.7](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.6...v0.1.7) (2026-03-03)


### Bug Fixes

* **deps:** update rust crate dirs to v6 ([#30](https://github.com/damacus/gitlab-runner-tui/issues/30)) ([aebde2f](https://github.com/damacus/gitlab-runner-tui/commit/aebde2f81443a1de47b326d190c63fcaf3f0b631))

## [0.1.6](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.5...v0.1.6) (2026-03-03)


### Bug Fixes

* **deps:** update rust crate crossterm to 0.29 ([#22](https://github.com/damacus/gitlab-runner-tui/issues/22)) ([561b653](https://github.com/damacus/gitlab-runner-tui/commit/561b653a6356a13832d063f1fc25cd97d43fabeb))
* **deps:** update rust crate toml to v1 ([#31](https://github.com/damacus/gitlab-runner-tui/issues/31)) ([22b4ed9](https://github.com/damacus/gitlab-runner-tui/commit/22b4ed93887ca1372317b20f5ec0076d3555e2f8))

## [0.1.5](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.4...v0.1.5) (2026-02-11)


### Bug Fixes

* add crates.io metadata and LICENSE for publishing ([#13](https://github.com/damacus/gitlab-runner-tui/issues/13)) ([2d0cbc5](https://github.com/damacus/gitlab-runner-tui/commit/2d0cbc559114094ce0e974e93dc3613c920a7a18))

## [0.1.4](https://github.com/damacus/gitlab-runner-tui/compare/v0.1.3...v0.1.4) (2026-02-11)


### Features

* runner rotation detection with polling and CI/headless mode ([#3](https://github.com/damacus/gitlab-runner-tui/issues/3)) ([bed52f6](https://github.com/damacus/gitlab-runner-tui/commit/bed52f606e7418126955a71df7276ddb74f2f546))


### Bug Fixes

* 'q' and '?' keys now work correctly during FilterInput mode ([9ef2967](https://github.com/damacus/gitlab-runner-tui/commit/9ef2967b97a31a05e5f653dccae96909f6280358))
* add /api/v4/ prefix to API URLs and error_for_status check ([54b61ac](https://github.com/damacus/gitlab-runner-tui/commit/54b61ac8cdf91f946493d98f2db8eed77b7eaadd))
* add clean shutdown for event handler task ([9e28221](https://github.com/damacus/gitlab-runner-tui/commit/9e2822191aaa6474925688b564028d2943789726))
* chain docker build from release-please workflow ([#10](https://github.com/damacus/gitlab-runner-tui/issues/10)) ([dcaaa23](https://github.com/damacus/gitlab-runner-tui/commit/dcaaa235ebe3e40586d9e76f6ce682b49ac4acb1))
* check all managers for offline/health detection, not just first ([9e75906](https://github.com/damacus/gitlab-runner-tui/commit/9e75906bfc636a8f2129ba103f5141e7df4d4aae))
* clear stale data when switching between commands ([c193c2b](https://github.com/damacus/gitlab-runner-tui/commit/c193c2bc8e74861d94ba8513ba0ae790180a792f))
* improve tag enrichment reliability ([7cf4b2c](https://github.com/damacus/gitlab-runner-tui/commit/7cf4b2cfcd7b48154a3ad641a2957de9bf11605f))
* remove .env with real token from tracking, add .env.example ([a59d2e5](https://github.com/damacus/gitlab-runner-tui/commit/a59d2e55ef5369c0c88b984845b488dc6ba0cb95))
* remove non-functional Tab key reference from help text ([75f0181](https://github.com/damacus/gitlab-runner-tui/commit/75f0181ed70c3b97f3ba9febf70cd9af08815b77))
* resolve clippy type_complexity in test helper ([72785d9](https://github.com/damacus/gitlab-runner-tui/commit/72785d9d668e5804639acf943764ba88aa54b5f6))
* server-side tag filtering, runner detail enrichment, parallel fetches ([5d31374](https://github.com/damacus/gitlab-runner-tui/commit/5d31374b0459419c6bed3755f1ff21a5ccb56c76))
* switch Docker image to Alpine builder and scratch runtime ([#6](https://github.com/damacus/gitlab-runner-tui/issues/6)) ([758bba2](https://github.com/damacus/gitlab-runner-tui/commit/758bba2de050d9511d12be88a8a46e62d66c275a))
* trigger docker and release workflows on release-please tags ([#8](https://github.com/damacus/gitlab-runner-tui/issues/8)) ([ff15d30](https://github.com/damacus/gitlab-runner-tui/commit/ff15d3050dee17df1b8dc9d248ad80b6a3b0fc13))

## [0.1.3](https://github.com/damacus/gitlab-runner-tui/compare/gitlab-runner-tui-v0.1.2...gitlab-runner-tui-v0.1.3) (2026-02-11)


### Bug Fixes

* trigger docker and release workflows on release-please tags ([#8](https://github.com/damacus/gitlab-runner-tui/issues/8)) ([ff15d30](https://github.com/damacus/gitlab-runner-tui/commit/ff15d3050dee17df1b8dc9d248ad80b6a3b0fc13))

## [0.1.2](https://github.com/damacus/gitlab-runner-tui/compare/gitlab-runner-tui-v0.1.1...gitlab-runner-tui-v0.1.2) (2026-02-11)


### Bug Fixes

* switch Docker image to Alpine builder and scratch runtime ([#6](https://github.com/damacus/gitlab-runner-tui/issues/6)) ([758bba2](https://github.com/damacus/gitlab-runner-tui/commit/758bba2de050d9511d12be88a8a46e62d66c275a))

## [0.1.1](https://github.com/damacus/gitlab-runner-tui/compare/gitlab-runner-tui-v0.1.0...gitlab-runner-tui-v0.1.1) (2026-02-11)


### Features

* runner rotation detection with polling and CI/headless mode ([#3](https://github.com/damacus/gitlab-runner-tui/issues/3)) ([bed52f6](https://github.com/damacus/gitlab-runner-tui/commit/bed52f606e7418126955a71df7276ddb74f2f546))


### Bug Fixes

* 'q' and '?' keys now work correctly during FilterInput mode ([9ef2967](https://github.com/damacus/gitlab-runner-tui/commit/9ef2967b97a31a05e5f653dccae96909f6280358))
* add /api/v4/ prefix to API URLs and error_for_status check ([54b61ac](https://github.com/damacus/gitlab-runner-tui/commit/54b61ac8cdf91f946493d98f2db8eed77b7eaadd))
* add clean shutdown for event handler task ([9e28221](https://github.com/damacus/gitlab-runner-tui/commit/9e2822191aaa6474925688b564028d2943789726))
* check all managers for offline/health detection, not just first ([9e75906](https://github.com/damacus/gitlab-runner-tui/commit/9e75906bfc636a8f2129ba103f5141e7df4d4aae))
* clear stale data when switching between commands ([c193c2b](https://github.com/damacus/gitlab-runner-tui/commit/c193c2bc8e74861d94ba8513ba0ae790180a792f))
* improve tag enrichment reliability ([7cf4b2c](https://github.com/damacus/gitlab-runner-tui/commit/7cf4b2cfcd7b48154a3ad641a2957de9bf11605f))
* remove .env with real token from tracking, add .env.example ([a59d2e5](https://github.com/damacus/gitlab-runner-tui/commit/a59d2e55ef5369c0c88b984845b488dc6ba0cb95))
* remove non-functional Tab key reference from help text ([75f0181](https://github.com/damacus/gitlab-runner-tui/commit/75f0181ed70c3b97f3ba9febf70cd9af08815b77))
* resolve clippy type_complexity in test helper ([72785d9](https://github.com/damacus/gitlab-runner-tui/commit/72785d9d668e5804639acf943764ba88aa54b5f6))
* server-side tag filtering, runner detail enrichment, parallel fetches ([5d31374](https://github.com/damacus/gitlab-runner-tui/commit/5d31374b0459419c6bed3755f1ff21a5ccb56c76))
