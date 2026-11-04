# cryon-server in a container (docs/server.md, section 3). Build context: the repository root:
#   docker build -f packaging/server/Dockerfile -t cryon-server .
#
# The image holds none of Minecraft's files. Prepare them on the host from your own Minecraft 26.3
# (cryon-prepare --prepare, see packaging/PLAY.txt) and mount the folder it writes, read-only:
#   docker run -d --name horizon -p 25565:25565 --stop-timeout 180 \
#     -v cryon-data:/data \
#     -v /path/to/.local/cryon-client/26.3:/opt/horizon/.local/cryon-client/26.3:ro \
#     -e HORIZON_EULA=true -e HORIZON_WHITELIST=true -e HORIZON_PLAYERS=Alex,Steve -e HORIZON_OP=Alex \
#     cryon-server
#
# HORIZON_EULA=true accepts the Minecraft EULA (https://aka.ms/MinecraftEULA) for this server. The first
# start writes /data/server.toml from HORIZON_* (EULA, WORLD, SEED, PORT, WHITELIST, PLAYERS, OP); later
# starts use the file. Manage it with `docker exec horizon cryon-server status|cmd|backup|logs ...`.
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
# The server build of the game (no sound, no controllers: it needs neither ALSA nor udev) as cryon-engine.
RUN cargo build --release --locked -p cryon-client --no-default-features --bin cryon-client \
    && cargo build --release --locked -p cryon-server --bin cryon-server \
    && mkdir /out && cp target/release/cryon-client /out/cryon-engine && cp target/release/cryon-server /out/

FROM debian:bookworm-slim
RUN useradd --system --home-dir /data --shell /usr/sbin/nologin horizon \
    && mkdir -p /data /opt/horizon/.local/cryon-client \
    && chown horizon:horizon /data
COPY --from=build /out/ /opt/horizon/
ENV PATH=/opt/horizon:$PATH \
    HORIZON_NO_AUDIO=1
WORKDIR /data
VOLUME /data
EXPOSE 25565
USER horizon
# SIGTERM makes cryon-server stop the game cleanly (it saves first): give it --stop-timeout 180.
CMD ["sh", "-c", "test -f server.toml || cryon-server init --yes || exit 1; exec cryon-server run"]
