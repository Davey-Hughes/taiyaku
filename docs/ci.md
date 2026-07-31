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

- a label matching `runs-on: docker`
- Docker support, since every job specifies a `container:`

The label was `ubuntu-latest` until it was measured. git.daveynet.xyz's runner
advertises `docker`, so every job asked for a label nothing provided and sat in
`waiting` until a later push superseded and cancelled it — sixteen runs, none
of which ever recorded a start. Nothing in the workflow reports this: a job
matching no runner is queued, not failed.

If the runner advertises a different label, change `runs-on:` in `ci.yml` to
match. Check the instance's registered runners under repository settings if
unsure; note that an instance-wide runner shows in neither the repository's nor
the user's `actions/runners` API listing, so an empty list there is not evidence
that no runner exists.

## Why there are no actions at all

Every job clones with `git` in a `run:` step. No `uses:` anywhere, which is
unusual enough to explain.

`actions/checkout` is a JavaScript action, and a JS action needs a node inside
the job's container to execute it. `rust:1-bookworm` carries no node —
`command -v node` exits non-zero — and this runner does not inject one. A job
combining `container:` with any JS action therefore dies before its first step.

That was measured rather than assumed, because the symptom names none of it:
all three jobs failed inside twenty-five seconds with no output worth reading.
Probe jobs settled it — a container with no action passes, an action with no
container passes, and both together is the only failing combination.

The consequence to keep in mind: **no action can be added to these workflows
while the image is `rust:1-bookworm`**. Caching, artifact upload and the rest
are all JavaScript actions. The escape hatch, when one is wanted, is an image
carrying both toolchains; two are measured working below.

## Caching, and why there is none

Every job compiles from cold. `Swatinem/rust-cache` was in the first version of
this workflow and was removed with the rest of the actions, and no replacement
was added, because there is no such thing as a cache action that is not
JavaScript — `actions/cache` and every Rust-specific wrapper alike.

The measurement says not to care yet. On a green run the three jobs take 51s,
39s and 17s, and they run in parallel, so the pipeline is under a minute
wall-clock. Caching would save perhaps twenty or thirty seconds of dependency
compilation against real added complexity. Revisit when a single job passes a
couple of minutes, or when artifact upload is wanted for the `verify` reports.

Two options exist that do not need an action at all, neither yet tested:

- **Named Docker volumes** on the job container, mounting `/usr/local/cargo` and
  the target directory across runs. Pure workflow YAML. The risk is that
  `act_runner` gates mounts behind `container.valid_volumes`, which denies by
  default, so it may need a runner config change.
- **`apt-get install -y nodejs` as the first step**, after which JavaScript
  actions work for the rest of the job, since the runner only needs a node when
  it reaches the action. The apt install costs most of what caching would save.

## Images carrying both Rust and node

Measured on this runner, each by running `actions/checkout` — the step that
fails on `rust:1-bookworm` — followed by `node --version`, `cargo --version` and
a real `cargo fmt --check`:

| Image | Compressed | Result |
| ----- | ---------- | ------ |
| `cimg/rust:1.97-node` | 0.77 GB | works |
| `catthehacker/ubuntu:rust-latest` | 1.05 GB | works |
| `mcr.microsoft.com/devcontainers/rust:1-bookworm` | — | no node |
| `rust:1-bookworm` (control) | 0.57 GB | fails, as predicted |

The control is the point of the table: it failed the same way the real jobs did,
which is what makes the two passes mean something rather than being a guess that
happened to work.

**Not switched, deliberately.** The only thing an image change buys today is the
caching described above, and that is not worth paying for yet. The costs are
real: `cimg/rust` adds 200 MB and runs as a non-root user, and the probe run
spent five minutes pulling these images before any job started, which is what a
cold layer cache costs on this runner. `catthehacker/ubuntu` is the closer match
to a GitHub-style runner if the deciding factor ever becomes action
compatibility rather than size.

Note that neither image removes the toolchain download described below, since
`rust-toolchain.toml` pins `stable` regardless of what the image ships.

## Why the clone is authenticated

The instance requires sign-in for all content, so an anonymous fetch is refused
even though this repository is public — the same setting that makes the REST API
reject unauthenticated calls. Without credentials the clone fails with
`could not read Username for 'http://forgejo:3000'`.

The runner supplies `FORGEJO_TOKEN` for this. Note that `FORGEJO_SERVER_URL` is
the runner's own view of the forge, an address on its container network rather
than the public hostname — an unauthenticated clone fails against
`http://forgejo:3000`, not against `git.daveynet.xyz`. The workflow derives the
clone URL from the variable instead of hard-coding either name.

## Why the variables say Forgejo and not GitHub

Forgejo Actions implements GitHub's interface, so its runner exports every value
twice: once under `GITHUB_*`, which is what lets actions written for GitHub work
unchanged, and once under `FORGEJO_*`. Both were measured working on this
instance, and the values are identical — `GITHUB_SERVER_URL` here is
`http://forgejo:3000`, not anything on github.com.

The `FORGEJO_*` set is used because nothing in this project is hosted on or
fetched from GitHub, and a workflow that reads as though it were invites the
question every time someone opens it.

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
