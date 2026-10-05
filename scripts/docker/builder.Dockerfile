# Ambiente di build dei pacchetti desktop RE-KORD (usato da scripts/pack.sh).
#
# Contiene tutto quello che serve a Tauri per Linux (webkit2gtk-4.1 e compagnia)
# e per l'installer Windows in cross-compilazione (cargo-xwin + LLVM + NSIS), cosi'
# la macchina di chi impacchetta non ha bisogno di nessuno di questi pacchetti.
#
# Ubuntu 24.04: WebKitGTK recente (piu' veloce, meno bug) e GLib/GStreamer uguali a
# quelli delle distribuzioni attuali, cosi' l'AppImage non mescola librerie sue con
# moduli di sistema incompatibili. Serve glibc 2.39: Ubuntu 24.04+, Mint 22+,
# Debian 13+, Fedora 40+.
#
# I plugin GStreamer installati qui finiscono nell'AppImage (bundleMediaFramework):
# senza, WebKit non trova decoder e uscita audio e il processo web si chiude al primo
# brano. base/good: ogg, opus, vorbis, flac, mp3, wav, webm, uscita audio; libav: AAC/M4A.
#
# cargo-xwin scarica al primo uso la CRT e il Windows SDK di Microsoft, accettando
# la loro licenza: e' il prezzo per produrre l'installer .exe da Linux.
FROM ubuntu:24.04

ARG NODE_VERSION=22.16.0
ARG PNPM_VERSION=9.15.0
ARG CARGO_XWIN_VERSION=0.23.1

ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:/opt/node/bin:/usr/local/bin:/usr/bin:/bin

RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl wget ca-certificates git file xz-utils unzip zip \
      libwebkit2gtk-4.1-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev \
      libdbus-1-dev libxdo-dev patchelf xdg-utils \
      gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-libav \
      gstreamer1.0-pulseaudio gstreamer1.0-alsa \
      nsis clang lld llvm \
 && rm -rf /var/lib/apt/lists/* \
 # cargo-xwin cerca i nomi senza suffisso di versione.
 && for t in clang-cl llvm-lib lld-link llvm-rc llvm-ar; do \
      src="$(ls /usr/bin/${t}-[0-9]* 2>/dev/null | sort -V | tail -n1)"; \
      [ -n "$src" ] && [ ! -e "/usr/local/bin/$t" ] && ln -s "$src" "/usr/local/bin/$t" || true; \
    done

RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ -C /opt && mv "/opt/node-v${NODE_VERSION}-linux-x64" /opt/node \
 && npm install -g "pnpm@${PNPM_VERSION}"

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \
 && rustup target add x86_64-pc-windows-msvc \
 && cargo install --locked "cargo-xwin@${CARGO_XWIN_VERSION}" \
 && rm -rf /opt/cargo/registry /opt/cargo/git \
 # Il container gira con l'utente di chi lancia la build (file in release/ suoi,
 # non di root): toolchain leggibile e scrivibile da chiunque.
 && chmod -R a+rwX /opt/rustup /opt/cargo

# Cache persistenti montate da scripts/pack.sh (registry cargo, CRT/SDK di xwin,
# strumenti scaricati da Tauri come linuxdeploy e i plugin NSIS).
ENV CARGO_HOME=/cache/cargo \
    XWIN_CACHE_DIR=/cache/xwin \
    HOME=/cache/home \
    APPIMAGE_EXTRACT_AND_RUN=1 \
    REKORD_BUILDER=1
