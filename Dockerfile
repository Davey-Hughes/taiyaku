# syntax=docker/dockerfile:1
#
# taiyaku: the server, the web app it serves, and nothing else. Four stages:
# the WebAssembly package the app imports, the app, the server binary, and a
# slim runtime holding the last two. See
# docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md §3.2.
#
# Versions follow what CI already pins: `rust:1-bookworm` and
# `node:26-bookworm` as its jobs' images, wasm-pack 0.15.0 by checksum as its
# `wasm` job, pnpm 11.3.0 as its `web` job. `rust-toolchain.toml` floats on
# `stable`; its install is a layer of its own, so a build reuses the
# toolchain it last installed until that file changes.
#
# The two Rust stages build in parallel and share one cargo registry cache, so
# that mount is `sharing=locked`: with the default, both unpack the same crate
# at once and one fails with "File exists" on `.cargo-ok`.

########################  1. The WebAssembly package  ###########################
FROM rust:1-bookworm AS wasm
WORKDIR /src
RUN curl -fsSL -o /tmp/wasm-pack.tar.gz \
      https://github.com/wasm-bindgen/wasm-pack/releases/download/v0.15.0/wasm-pack-v0.15.0-x86_64-unknown-linux-musl.tar.gz \
 && echo "c09f971ecaed9a2efc80fdcea7a00ef6b53c7fadc8c57d1f61b53a6aa66b668a  /tmp/wasm-pack.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/wasm-pack.tar.gz -C /tmp \
 && install -m 0755 /tmp/wasm-pack-v0.15.0-x86_64-unknown-linux-musl/wasm-pack /usr/local/bin/wasm-pack
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY scripts/build-wasm.sh scripts/
RUN --mount=type=cache,id=taiyaku-cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=taiyaku-wasm-target,target=/src/target \
    ./scripts/build-wasm.sh

########################  2. The web app  #######################################
FROM node:26-bookworm AS web
RUN npm i -g pnpm@11.3.0
WORKDIR /src/web
# Installed before the package exists, as CI's `web` job does: the
# `link:../crates/taiyaku-wasm/pkg` dependency needs no target to install, so
# this layer survives every Rust change.
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
COPY --from=wasm /src/crates/taiyaku-wasm/pkg /src/crates/taiyaku-wasm/pkg
RUN pnpm run build

########################  3. The server  ########################################
FROM rust:1-bookworm AS server
WORKDIR /src
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# `rusqlite` links the system SQLite (`libsqlite3-dev` ships in this image),
# so this stage and the runtime one below must be the same distribution.
RUN --mount=type=cache,id=taiyaku-cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=taiyaku-server-target,target=/src/target \
    cargo build --release --locked -p taiyaku-cli \
 && install -m 0755 target/release/taiyaku /usr/local/bin/taiyaku

########################  4. Runtime  ###########################################
FROM debian:bookworm-slim AS runtime
# `libsqlite3-0` is the one library the binary links that this image lacks;
# `ureq`'s TLS roots are compiled in, so no `ca-certificates`.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libsqlite3-0 \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /config /cache \
 && chown 99:100 /config /cache
COPY --from=server /usr/local/bin/taiyaku /usr/local/bin/taiyaku
COPY --from=web /src/web/dist /app/web
COPY --chmod=0755 deploy/probe.sh /usr/local/bin/taiyaku-probe
# The database lands at /config/taiyaku/library.db and its backups beside it;
# covers and tiles under /cache/taiyaku. Map /config somewhere backed up and
# /cache somewhere that is not: everything in it is regenerable.
ENV XDG_DATA_HOME=/config \
    XDG_CACHE_HOME=/cache
# Unraid's `nobody:users`, which owns its shares.
USER 99:100
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --start-interval=2s --retries=3 \
  CMD ["taiyaku-probe", "/", "200"]
ENTRYPOINT ["taiyaku"]
CMD ["serve", "--addr", "0.0.0.0:8484", "--web", "/app/web"]
