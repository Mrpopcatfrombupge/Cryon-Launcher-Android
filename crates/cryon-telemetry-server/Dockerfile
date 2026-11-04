# Built from the repository root: docker build -f crates/cryon-telemetry-server/Dockerfile .
FROM rust:1-slim AS build
WORKDIR /src
# The root manifest only for the shared crate's workspace settings (edition, lints).
COPY Cargo.toml Cargo.toml
COPY crates/cryon-telemetry crates/cryon-telemetry
COPY crates/cryon-telemetry-server crates/cryon-telemetry-server
RUN cargo build --release --locked --manifest-path crates/cryon-telemetry-server/Cargo.toml

FROM debian:stable-slim
COPY --from=build /src/crates/cryon-telemetry-server/target/release/cryon-telemetry-server /usr/local/bin/
ENV HORIZON_TELEMETRY_DB=/data/telemetry.sqlite3 HORIZON_TELEMETRY_TRUST_PROXY=1
CMD ["cryon-telemetry-server"]
