FROM rust:1-alpine AS builder
WORKDIR /app

RUN apk add --no-cache musl-dev

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src

COPY src/ src/
RUN touch src/main.rs && cargo build --release && strip target/release/gitlab-runner-tui

FROM alpine:3.24 AS ci
RUN apk add --no-cache bash ca-certificates grep jq
ENV GITLAB_RUNNER_TUI_CONTAINER=1
COPY --from=builder /app/target/release/gitlab-runner-tui /usr/local/bin/gitlab-runner-tui

FROM scratch AS runtime
ENV GITLAB_RUNNER_TUI_CONTAINER=1
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/target/release/gitlab-runner-tui /usr/local/bin/gitlab-runner-tui

ENTRYPOINT ["gitlab-runner-tui"]
