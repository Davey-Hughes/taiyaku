# syntax=docker/dockerfile:1
#
# taiyaku: the server, the web app it serves, and nothing else. Four stages:
# the WebAssembly package the app imports, the app, the server binary, and a
# slim runtime holding the last two. See
# docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md §3.2.
#
# Versions follow what CI already pins: `rust:1-bookworm` and
# `node:26-bookworm` as its jobs' images, wasm-pack 0.15.0 by checksum as its
# `wasm` job, pnpm 11.3.0 as its `web` job. The `rust:1-bookworm` tag floats,
# but `rust-toolchain.toml` pins the toolchain the build uses; its install is a
# layer of its own, so a build reuses the toolchain it last installed until
# that file changes.
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
# The licence page bundles the repository's licence from `../LICENSE`
# (`web/src/licenceTexts.ts`), which from this WORKDIR is this path: without
# it the build below fails to resolve the import.
COPY LICENSE /src/LICENSE
COPY --from=wasm /src/crates/taiyaku-wasm/pkg /src/crates/taiyaku-wasm/pkg
RUN pnpm run build

########################  3. The server  ########################################
FROM rust:1-bookworm AS server
WORKDIR /src
COPY rust-toolchain.toml ./
RUN rustup toolchain install
# The `rar-libarchive` build below compiles `compress-tools`, which finds
# libarchive's headers through `pkg-config`: this image has `pkg-config` but
# not the headers. A layer of its own above the sources, so a source change
# does not reinstall it.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libarchive-dev
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# The commit `/api/about` reports, which `taiyaku-serve` reads at compile time
# through `option_env!`. Its build script asks git for HEAD, but `.git` is not
# in the context, so here this argument is the only source. A `RUN` after an
# `ARG` misses the cache whenever its value changes, so it is declared here and
# not higher, where every commit would reinstall the toolchain. Unset, or
# anything but a full 40-character lowercase hash, and the server reports no
# commit.
ARG TAIYAKU_COMMIT
# `rusqlite` compiles SQLite into the binary (its `bundled` feature). The
# binary still links this image's glibc and, through `rar-libarchive`, the
# system libarchive (installed above), so this stage and the runtime one below
# must be the same distribution.
#
# SQLite's default allows 32766 bound parameters and this image allows 250000:
# `editions_by_work`, `opens_by_work`, `missing_by_edition` and `books_among` in
# `browse.rs` bind one parameter per work or edition of a listing, so at the
# default a large enough library answers its listings with an error.
ENV LIBSQLITE3_FLAGS=-DSQLITE_MAX_VARIABLE_NUMBER=250000
RUN --mount=type=cache,id=taiyaku-cargo-registry,sharing=locked,target=/usr/local/cargo/registry \
    --mount=type=cache,id=taiyaku-server-target,target=/src/target \
    cargo build --release --locked -p taiyaku-cli --features rar-libarchive \
 && install -m 0755 target/release/taiyaku /usr/local/bin/taiyaku

########################  4. Runtime  ###########################################
FROM debian:bookworm-slim AS runtime
# `libarchive13` is the one library the binary links that this image lacks.
# SQLite is compiled into the binary, so its security fixes arrive through
# `libsqlite3-sys` in `Cargo.lock` and never through `apt`, and `ureq`'s TLS
# roots are compiled in, so no `ca-certificates`. `apt-get upgrade` runs so that
# a rebuild ships Debian's security fixes before `debian:bookworm-slim` is
# republished with them.
RUN apt-get update \
 && DEBIAN_FRONTEND=noninteractive apt-get upgrade -y \
 && apt-get install -y --no-install-recommends libarchive13 \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /config /cache \
 && chown 99:100 /config /cache
COPY --from=server /usr/local/bin/taiyaku /usr/local/bin/taiyaku
COPY --from=web /src/web/dist /app/web
COPY --chmod=0755 deploy/probe.sh /usr/local/bin/taiyaku-probe
# The database lands at /config/taiyaku/library.db and its backups beside it;
# covers and tiles under /cache/taiyaku. Map /config somewhere backed up and
# /cache somewhere that is not: everything in it is regenerable. /media is
# where the Libraries screen's directory browser starts: map the share that
# holds every library's directories there.
ENV XDG_DATA_HOME=/config \
    XDG_CACHE_HOME=/cache \
    TAIYAKU_DIRS=/media
# Unraid's `nobody:users`, which owns its shares.
USER 99:100
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --start-interval=2s --retries=3 \
  CMD ["taiyaku-probe", "/", "200"]
ENTRYPOINT ["taiyaku"]
CMD ["serve", "--addr", "0.0.0.0:8484", "--web", "/app/web"]
