# The Play with friends relay, for anyone to run (docs/play-with-friends.md, "Running your own relay").
# Build context: the repository root (the workspace's manifests and lockfile):
#   docker build -f crates/cryon-relay/Dockerfile -t cryon-relay .
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p cryon-relay \
    && cp target/release/cryon-relay /cryon-relay

FROM debian:bookworm-slim
COPY --from=build /cryon-relay /usr/local/bin/cryon-relay
ENV PORT=7000
EXPOSE 7000
USER nobody
CMD ["cryon-relay"]
