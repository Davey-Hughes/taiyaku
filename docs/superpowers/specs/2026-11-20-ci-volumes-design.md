# CI build volumes: design

2026-11-20. Approved by the owner in conversation the same day ("you do it": the design below and the runner step). Background and the measurements that prompted it: `.superpowers/sdd/2026-11-20-ci-volumes-handoff.md`.

## Intent

Every Rust job in `.forgejo/workflows/ci.yml` compiles its third-party dependencies from nothing in a fresh `rust:1-trixie` container, and since jemalloc that includes jemalloc's C. The `test` and `default-features` jobs now reach the ten-minute job limit on a busy runner. Keeping cargo's downloads and built dependencies between runs removes the dependency build from every run after the first.

## Goals

- The four Rust jobs of `ci.yml` (`lint`, `test`, `default-features`, `wasm`) reuse downloaded crates and compiled dependencies from earlier runs.
- A run without the volumes behaves as CI does today: slower, and otherwise the same.
- The gates give the same verdicts warm as cold. The coverage gate in particular reports the same lines.
- `docs/ci.md` says what is cached, where, how it is cleared and what it costs, with figures measured on real runs.

## Non-goals

- The `docker` job. It builds the deployed image with buildx and its own BuildKit cache; nothing it ships comes from a job's target dir, and that stays so.
- `web`, `advisories.yml` and `mutants.yml`. They can follow once this is measured.
- The rustup download of the pinned toolchain, and the per-job tool tarballs.
- `timeout-minutes`. It stays at 10; whether to raise it is the owner's decision, to be made from the warm figures.
- Workspace crates. A fresh checkout gives every source file a new mtime, so cargo rebuilds the six workspace crates each run whatever is cached. That holds only with incremental compilation off: the dev profile is incremental, and its state for the workspace crates would otherwise stay in the kept directory for the next run to build from. The script sets `CARGO_INCREMENTAL=0` wherever it sets `CARGO_TARGET_DIR` (added by the whole-branch review).

## Anti-goals

- A gate that passes because of something an earlier run left behind: a stale report, a stale test binary, a stale junit file.
- A cache that has to be present for CI to be green.

## Constraints found

- The NAS runner is forgejo-runner v12.13.2 in a container on the NAS, capacity 2, `docker_host: automount`, `--cpus 4` per job. Its config had no `container.valid_volumes`, and the default is an empty list.
- By the runner's source (read at v13.2.2), a volume a job asks for that the list does not allow is dropped with a warning and the job carries on. This has to be confirmed on the deployed version by a real run before the docs state it.
- Docker's root on the NAS is a btrfs NVMe with over a terabyte free, so named volumes are not short of room.
- The `docker` label is shared by two runners: the NAS runner and one on a desktop machine (forgejo-runner v12.13.2 in rootless Docker, capacity 2, `--cpus 12` per job), and either can take any job. Each host keeps its own set of volumes, so a job's first run on a host builds from nothing (corrected after the runs on the forge).
- An empty named volume mounted over a non-empty image directory is filled from the image once and hides the image's directory afterwards. Mounting all of `/usr/local/cargo` would freeze the image's cargo and rustup binaries at the first run's.
- cargo-llvm-cov, run without `--no-clean` as CI runs it, removes the workspace crates' artifacts and old profile data before each run and keeps the dependencies'.
- Cargo holds one lock per build directory, for the build only.

## Approaches

1. **Named Docker volumes mounted by the workflow (chosen).** No action, no node, nothing to upload or download; the price is one setting on each runner and a set of volumes on each runner's host.
2. **A cache action after a throwaway node install.** Needs node in every Rust job and moves gigabytes through the forge's cache store each run.
3. **A prebuilt image with dependencies compiled in.** Needs an image build, a registry and a rebuild on every `Cargo.lock` change; `docs/ci.md` already judged an image change not worth its cost.

The owner asked for volumes; 2 and 3 are recorded as considered.

## The design

**Volumes.** All named `taiyaku-ci-*`, which is the one glob each runner allows.

- `taiyaku-ci-cargo-registry` on `/usr/local/cargo/registry` and `taiyaku-ci-cargo-git` on `/usr/local/cargo/git`, shared by the four jobs. Cargo's own download lock is a file in its home directory, which these mounts leave private to each container, so each job downloads once under a lock held in the registry volume (corrected by the plan, C2).
- One target volume per job: `taiyaku-ci-target-lint`, `-test`, `-default-features`, `-wasm`. Jobs of one run never wait on each other's build lock. Two runs of the same job at once must not share a directory at all: cargo's lock ends with the build while the tests and the coverage report go on reading it, and a checkout older than another run's outputs is taken as already built. A run holds its directory for the whole job, each toolchain has two directories because each runner's capacity is 2, and a third run builds without a volume (corrected by the plan, C1).

**Where cargo builds.** Inside the job's target volume, in a directory named for the pinned toolchain (`rust-toolchain.toml`'s channel), so a toolchain bump starts a new directory instead of deleting one another run may be using. Each run marks its directory as used; a directory of another toolchain not used for two weeks is removed at job start. The pinned toolchain's own directories are never removed for age: the second is used only when two runs overlap, and removing it would make the next overlapping run build from nothing. A directory is renamed before it is removed, so a removal cut short leaves nothing under a name a run builds in (both settled by the whole-branch review). Every delete names its path through `"${VAR:?}"`.

**No automatic size cap (my call, a change from what the owner was first shown).** A wipe under a build another run is in the middle of fails that run. A toolchain's directory grows by what a `Cargo.lock` change leaves behind and, in `lint`'s volume, by what the check at the declared Rust version built before `rust-version` moved; the registry and git volumes are never pruned, because cargo's own cache collection keeps its record in cargo's home directory, outside them. Each job prints its volume's size instead, and the docs give the by-hand clear (`docker volume rm` on each host, with its runner idle).

**Nothing stale is read.** No step or script may read a file an earlier run left, cargo's incremental state included (see Non-goals). The builder traces every `target/` path the four jobs' steps and scripts name and decides between pointing `CARGO_TARGET_DIR` at the volume (my lean: files the steps name explicitly then stay in the fresh workspace) and mounting the volume at the workspace's `target/` with the known outputs removed first.

**Visibility.** Each of the four jobs has a step, straight after the clone (in `lint`, straight after `Formatting`, which compiles nothing), that prints whether its volumes are mounted, and the size and toolchain directory in use. It does not fail the job.

**The runner (done by me on the owner's word, 2026-11-20).** `container.valid_volumes: ["taiyaku-ci-*"]` added to `/mnt/user/appdata/forgejo/runner/config.yml` (the previous file kept beside it as `config.yml.bak-2026-11-20`), and the runner restarted with a drain once idle. The owner gave the second runner the same setting the same day, in `/home/cirunner/.config/forgejo-runner/config.yml`, the file it reads.

**Trust.** Any run, a pull request's included, writes the volumes that main's gate jobs later read. Those jobs execute what the volumes hold: kept build-script outputs and proc-macro libraries are loaded by rustc, as root in the job's container. On the NAS that is no wider than what a job has already, since that runner mounts the host's Docker socket into every job container. On the desktop the volumes belong to the rootless Docker of the user `cirunner`, whose socket the `docker` job's default container reached in main's run 1025; whether a job that sets a `container:` gets that socket there too has not been looked at (corrected after the runs on the forge). The forge is sign-in only and the deployed image never comes from them. The docs say this.

**Checks before the PR.**

- Locally, in `rust:1-trixie` with scratch volumes: the `test` job's commands cold, then warm on a fresh checkout. The coverage report's per-file line counts are equal, the dependency crates are not recompiled, and the workspace crates are.
- The same for a toolchain-directory change and for the pruning step, including a watched failure: a planted stale directory is removed, a fresh one is kept.
- On the forge: the PR's own runs, first filling the volumes and then warm, give the figures for `docs/ci.md`; the runner's log or the job's `Build volumes` step confirms what the deployed runner does with a volume it does not allow (checked before the runner restart if a run can be had then, otherwise from a volume name outside the glob on a scratch branch).

**Docs.** Rewrite "Caching, and why there is none" in `docs/ci.md` and correct each sentence elsewhere that a cache makes false (`docs/ci.md`, `scripts/check-all.sh`, the workflow comments). Document the runner setting as a prerequisite that fails soft. Follow the open-items ritual in `docs/status.md` if a row opens or closes.

## Left to the builder

The mount path and the `CARGO_TARGET_DIR` decision above; the exact form of that step and whether it is a script under `scripts/`; how the docs figures are laid out.
