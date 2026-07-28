# Continuous integration

Workflows live in `.forgejo/workflows/`. Forgejo looks there first and falls back
to `.github/workflows/`; using the Forgejo path keeps the intent unambiguous.

## Jobs

| Job | Runs |
| --- | ---- |
| `lint` | `cargo fmt --check`, `cargo clippy -D warnings` |
| `test` | `cargo test --workspace` |
| `wasm` | builds `taiyaku-core` for `wasm32-unknown-unknown` |

The `wasm` job is the one worth keeping even when it feels redundant. The design
depends on the core being shareable between the browser reader and a future
native shell, and that property is easy to destroy accidentally — a single
dependency that assumes threads or links a native library will do it. Without a
gate, the breakage stays invisible until the reader is being built, by which
point the offending dependency is load-bearing.

## Runner requirements

A registered `forgejo-runner` with:

- a label matching `runs-on: ubuntu-latest`
- Docker support, since every job specifies a `container:`

If the runner advertises a different label — `docker` is the other common
default — change `runs-on:` in `ci.yml` to match. Check the instance's
registered runners under repository settings if unsure.

## Why `uses:` carries full URLs

Forgejo resolves a bare `owner/repo` in `uses:` against the instance's
`DEFAULT_ACTIONS_URL`, which varies between deployments and versions. Writing
`https://code.forgejo.org/actions/checkout@v4` in full removes that ambiguity, so
these workflows behave the same on any instance.

The consequence is that the runner needs outbound access to `code.forgejo.org`
and `github.com`. If that is not available, vendor the actions or drop the
caching step — `Swatinem/rust-cache` is a build-time optimisation, not a
correctness requirement, and removing it costs only rebuild time.

## Toolchain note

`rust-toolchain.toml` pins `channel = "stable"` and requests the `wasm32`
target plus `rustfmt` and `clippy`, which makes a fresh local checkout work with
no setup.

The tradeoff is in CI: the `rust:1-bookworm` image installs its toolchain under a
version-specific name rather than as `stable`, so rustup fetches `stable`
separately on each job. It works, but costs a download per job. If that becomes
annoying, either pin `channel` to the image's exact version and bump both
together, or delete `rust-toolchain.toml` and let CI use the image toolchain as-is
at the cost of local convenience.
