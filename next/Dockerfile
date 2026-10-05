# syntax=docker/dockerfile:1.7
#
# RE-KORD hub (next) in un container.
#
#   docker build -t rekord:5.0.0 .            (dalla cartella next/)
#   docker compose up -d                      (vedi docker-compose.yml)
#
# Stadi: ui (client + pannello admin con pnpm) → server (rekord-server in
# release) → tools (yt-dlp e cloudflared scaricati a versione fissata e
# verificati con SHA-256, vedi scripts/fetch-*.sh) → runtime (debian slim con
# ffmpeg, utente non root, /data e /music come volumi).

ARG NODE_VERSION=22
ARG RUST_VERSION=1
ARG DEBIAN_RELEASE=bookworm

# ---------------------------------------------------------------- ui
FROM node:${NODE_VERSION}-${DEBIAN_RELEASE}-slim AS ui
ENV PNPM_HOME=/pnpm PATH=/pnpm:$PATH CI=1
RUN corepack enable
WORKDIR /src
# Prima solo i manifest: lo strato delle dipendenze resta in cache finche'
# lockfile e package.json non cambiano.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/client-ui/package.json apps/client-ui/
COPY apps/server-ui/package.json apps/server-ui/
COPY apps/client-shell/package.json apps/client-shell/
COPY packages/ui/package.json packages/ui/
RUN --mount=type=cache,id=pnpm-store,target=/pnpm/store \
    pnpm install --frozen-lockfile --filter "@rekord/client-ui..." --filter "@rekord/server-ui..."
COPY packages/ui packages/ui
COPY apps/client-ui apps/client-ui
COPY apps/server-ui apps/server-ui
RUN pnpm build:ui

# ---------------------------------------------------------------- server
FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY apps/server apps/server
# Il guscio Tauri e' membro del workspace ma qui non si compila: basta il suo
# manifest (e una lib vuota) perche' cargo legga il workspace.
COPY apps/client-shell/src-tauri/Cargo.toml apps/client-shell/src-tauri/Cargo.toml
RUN mkdir -p apps/client-shell/src-tauri/src && : > apps/client-shell/src-tauri/src/lib.rs
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=rekord-target,target=/src/target \
    cargo build -p rekord-server --release --locked \
 && install -m 0755 target/release/rekord-server /rekord-server

# ---------------------------------------------------------------- tools
FROM node:${NODE_VERSION}-${DEBIAN_RELEASE}-slim AS tools
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY scripts/lib/fetch-common.sh scripts/lib/
COPY scripts/fetch-ytdlp.sh scripts/fetch-cloudflared.sh scripts/third-party.sha256 scripts/
ARG TARGETARCH
# Vuoti = versioni fissate negli script.
ARG YTDLP_VERSION=
ARG CLOUDFLARED_VERSION=
RUN set -eu; \
    case "${TARGETARCH:-amd64}" in arm64) plat=linux-arm64 ;; *) plat=linux-x64 ;; esac; \
    YTDLP_VERSION="${YTDLP_VERSION}" bash scripts/fetch-ytdlp.sh "$plat" /out; \
    CLOUDFLARED_VERSION="${CLOUDFLARED_VERSION}" bash scripts/fetch-cloudflared.sh "$plat" /out

# ---------------------------------------------------------------- runtime
FROM debian:${DEBIAN_RELEASE}-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl ffmpeg tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --user-group --home-dir /data --shell /usr/sbin/nologin rekord \
 && mkdir -p /data /music /app/bin \
 && chown rekord:rekord /data /music

COPY --from=server /rekord-server /app/rekord-server
COPY --from=ui /src/apps/client-ui/dist /app/client-ui
COPY --from=ui /src/apps/server-ui/dist /app/admin-ui
COPY --from=tools /out/ /app/bin/

# Stesse variabili della riga di comando: rekord-server --help.
ENV REKORD_BIND=0.0.0.0:7420 \
    REKORD_DATA_DIR=/data \
    REKORD_MUSIC_ROOT=/music \
    REKORD_CLIENT_UI=/app/client-ui \
    REKORD_ADMIN_UI=/app/admin-ui \
    YTDLP_PATH=/app/bin/yt-dlp \
    REKORD_CLOUDFLARED_BIN=/app/bin/cloudflared \
    RUST_LOG=info,tower_http=info

USER rekord
WORKDIR /data
VOLUME ["/data", "/music"]
EXPOSE 7420

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD ["sh", "-c", "curl -fsS \"http://127.0.0.1:${REKORD_BIND##*:}/api/v1/health\" >/dev/null || exit 1"]

# tini inoltra SIGTERM all'hub (chiusura ordinata del DB) e raccoglie i processi
# figli (ffmpeg, yt-dlp, cloudflared).
ENTRYPOINT ["/usr/bin/tini", "--", "/app/rekord-server"]
