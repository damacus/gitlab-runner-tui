# Changelog

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
