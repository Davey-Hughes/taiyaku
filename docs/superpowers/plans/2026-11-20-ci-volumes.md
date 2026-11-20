# CI build volumes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The four Rust jobs of `.forgejo/workflows/ci.yml` (`lint`, `test`, `default-features`, `wasm`) keep cargo's downloads and built dependencies in named Docker volumes between runs, give the same verdicts warm as cold, and build as today when the volumes are absent.

**Architecture:** Each of the four jobs mounts two shared volumes (cargo's registry and git checkouts) and one target volume of its own at `/ci/target`. A new script, `scripts/ci-volumes.sh`, runs straight after the clone: it fetches under a lock, takes a build directory named for the pinned toolchain for the whole job, prunes other toolchains' directories unused for two weeks, and exports `CARGO_TARGET_DIR` and `CARGO_INCREMENTAL=0` through `$GITHUB_ENV`. Every file a later step reads by name stays in the fresh workspace.

**Tech Stack:** Forgejo Actions YAML (forgejo-runner v12.13.2, `container.volumes`), bash, util-linux `flock`/`mountpoint`/`setsid` (all present in `rust:1-trixie`), cargo 1.100.0, cargo-llvm-cov 0.9.1, cargo-nextest, Docker for the local trial.

**Spec:** `docs/superpowers/specs/2026-11-20-ci-volumes-design.md` (approved and settled; read it first, it is short). Background: `/home/davey/projects/taiyaku/.superpowers/sdd/2026-11-20-ci-volumes-handoff.md`.

**Worktree:** `/home/davey/projects/taiyaku/.claude/worktrees/ci-volumes` (below, `$W`), branch `ci-volumes`. **Evidence:** `/home/davey/projects/taiyaku/.superpowers/sdd/ci-volumes-evidence/` (below, `$EV`), in the main checkout, never committed and never under `target/`. `$EV/plan-probe/output.txt` holds the five probes this plan rests on; the fifth ran the script below, as written here, through cases S1 to S7 in reduced form. Line numbers throughout are `origin/main`'s. `CLAUDE.md` is not in the worktree (it is excluded locally): read `/home/davey/projects/taiyaku/CLAUDE.md`.

## Two corrections to the design, and the builder's decisions

The design is settled; these two of its sentences are false as written, and the plan carries the smallest change that keeps its Goals and Anti-goals. **The controller confirms both with the owner before dispatching Task 1.**

- **C1. "Two runs of the same job at once do wait" does not make them safe.** Cargo's build lock ends with the build. The tests then run from the directory, and `cargo llvm-cov report` reads it, with no lock held; a second run's `cargo llvm-cov` cleans and rebuilds under the first. Worse, cargo compares a source file's time with its last output's: a checkout older than another run's build is taken as already built (probe 3: changed source with an older time is `Fresh`), so one run can pass on another commit's code. That is the design's first Anti-goal. **Change:** the script holds a lock on its build directory for the whole job (a background `sleep` keeps it after the step ends; probe 1), there are two directories per toolchain (`<channel>-a`, `<channel>-b`) because the runner's capacity is 2, a third concurrent run builds in `./target` as today, and after taking the lock the script sets every checked-out file's time to now. The simpler alternative, one directory and a cold build for the second run, costs the second run a from-nothing build exactly when the runner is busy; say so if the owner prefers it, and drop the `for slot` loop.
- **C2. "Cargo's package-cache lock serializes downloads" is false for these mounts.** That lock is `.package-cache` in cargo's home directory itself (probe 4), which each container has to itself when only `registry/` and `git/` are volumes. Two jobs unpacking one crate into the shared registry collide. **Change:** the script runs one `cargo fetch --locked` under a `flock` on a file inside the registry volume, after which no cargo call in the job has anything to download. The alternative, one volume as `CARGO_HOME`, changes the design's volumes and every tool's environment.

**Changed in review, after this plan was written.** In `lint` the `Build volumes` step follows `Formatting`, not the clone: `cargo fmt --check` compiles nothing, so a formatting failure costs no download and no wait on another job's fetch. Where this plan says the step is "straight after the clone" or `steps[1]`, that holds for `test`, `default-features` and `wasm` only.

**Changed by the whole-branch review.** The script in Task 1 is the script as that review left it. Beside `CARGO_TARGET_DIR` it writes `CARGO_INCREMENTAL=0`, because the dev profile is incremental and a kept directory would hand one run's incremental state to the next. It makes `./target`, which `cargo llvm-cov report` does not make for its output file. It opens its lock files to append, so a lock file that is a link empties nothing. It renames a directory to `<name>.removing` before removing it and removes one it finds, so a removal cut short leaves nothing under a name a run builds in. And it never removes the pinned toolchain's two directories for age. Where Task 1's case table or its list of output lines says less than that, `$EV/trial/script-checks.sh` is what holds.

Decisions the design left to the builder:

- **`CARGO_TARGET_DIR` at the volume, not a mount at `target/`.** `target/nextest/default/junit.xml` and `target/rust-coverage.json` are written under the workspace whatever `CARGO_TARGET_DIR` is (probe 2), so with the build elsewhere both are always this run's and no step's path changes.
- **Mount path `/ci/target`:** outside the workspace, `/usr/local/cargo` and `/tmp`, so nothing else writes there.
- **A script under `scripts/`, run as the step after Checkout**, not inline YAML: four jobs would repeat it, and the local trial runs the same file CI does. It cannot be the first step: nothing from the repository is on disk before the clone.
- **`$GITHUB_ENV`** carries `CARGO_TARGET_DIR` (and `CARGO_INCREMENTAL=0`, since the whole-branch review) to later steps; `advisories.yml` already relies on it. With no line written, the job builds in `./target`.
- **CI takes nothing from `scripts/coverage-build-guard.sh`.** Its hazard is two compilers' coverage builds in one folder; CI's folder is named for the channel, and the script refuses a channel that is not an exact version.
- **The "134 s" sizing figure stays** at `ci.yml:53`, `docs/ci.md:268-269` and in O39's closed record. It is stale by measurement, and rewriting the bound's reasoning belongs with the owner's decision on `timeout-minutes`, which the design defers. Task 7 opens one open-items row for that decision, naming the two live sentences.
- **Figures:** one table in `docs/ci.md`, four jobs by two named runs (volumes empty, volumes filled), plus the six sizes one job's first step printed. Single runs, named by run number, no averages and no ratios.

## Global Constraints

- Volumes are named `taiyaku-ci-*` and nothing else; the runner allows that one glob. A workflow volume is `name:/path` with no `:ro` suffix.
- `timeout-minutes` stays 10. The `docker`, `web` jobs and `advisories.yml`, `mutants.yml` get no volume; rustup's toolchain download and the tool tarballs stay uncached.
- No existing `run:` line changes. `ci.yml`'s six coverage lines and `check-all.sh`'s stay identical (`ci.yml:337-340` says why).
- No step or script reads a file an earlier run left. A missing volume or a failed script never fails a job; what the deployed runner does with a volume it refuses is Task 6's to find, and nothing says it before then.
- Every delete in a script names its path through `"${VAR:?}"`. Every `docker volume rm` and `docker rm` you run names its targets literally, in a Bash call of its own.
- Comments say why only: no history, no measurements, no dates, no PR numbers, no block over 8 lines, and the `scripts` area's comment share must not rise above `scripts/comment-budget.json` (the script below passes as written; if an edit tips it, trim comments, do not raise the budget). `node scripts/check-comments.mjs --base origin/main` checks all of it, YAML and shell included.
- Shell: no `cd` (it is zoxide); `git -C`, `env -C`, absolute paths. zsh: `grep -a`, never a variable named `path`, start a log with `>|`.
- Local runs are throttled: `--cpus 8 -e CARGO_BUILD_JOBS=8 -e NEXTEST_TEST_THREADS=8 -e RUST_TEST_THREADS=8` on every trial container. No mutation sweep. Local timings are not CI's (32 cores) and are never written into the docs.
- Trial volumes and containers are named `taiyaku-trial-*` and are removed when the task ends.
- Tasks commit locally and never push; the push, the pull request and anything on the NAS are the controller's. No attribution lines in commits. Docs name no branch commit; dates are the owner's Pacific day.

## Review Focus

1. **Two runs of one job at once.** The second takes the other directory, a third builds in `./target`, none shares. Task 1, cases S2 and S3.
2. **A job killed at the time limit mid-build.** The next run finds a half-written directory and a released lock and must go green. Task 3, the interrupted fill.
3. **A warm run after a different commit.** Nothing of the earlier commit's build or profile data reaches the report. Task 3 fills with a planted change for this reason; a fill from the same commit cannot show it. (The whole-branch review found that Task 3's comparison shows the build half only: every run ran the same tests, so merged profile data would have read the same. Its own check on a scratch crate, `$EV/review/coverage-scratch-3-watched.log`, shows the profile-data half.)
4. **A volume that is missing, read-only or on another host.** The job builds as today. Task 1, cases S1 and S7; Task 6 for the deployed runner.
5. **A toolchain file the script cannot name a directory from** (`stable`, a two-part version, a path). No volume is used. Task 1, case S4.

## File Structure

| File | Change |
| --- | --- |
| `scripts/ci-volumes.sh` | new, mode 755: the whole of the cache preparation |
| `.forgejo/workflows/ci.yml` | `container.volumes` and one step in four jobs; comments at `:572` |
| `docs/ci.md` | the caching section rewritten; six other sites corrected; runner prerequisite; figures |
| `scripts/check-all.sh` | three comments (`:11`, `:139-142`, `:497`) |
| `crates/taiyaku-cli/Cargo.toml` | one comment (`:43-47`) |
| `docs/status.md` | one sentence (`:254`); one open-items row (Task 7) |

---

### Task 1: `scripts/ci-volumes.sh`, checked case by case in the image

**Files:**
- Create: `scripts/ci-volumes.sh` (commit it executable, as `scripts/build-wasm.sh` is)
- Evidence: `$EV/trial/script-checks.sh`, `$EV/trial/script-checks.log`

**Interfaces:**
- Produces: the script below. Mount points `/usr/local/cargo/registry`, `/usr/local/cargo/git`, `/ci/target`. Build directory `/ci/target/<channel>-a` or `-b`, lock `<directory>.lock`, mark `<directory>/.last-used`. Output lines later tasks grep: `volume: <path>`, `no volume: <path>`, `not using a build volume: <reason>`, `removing <dir>: not used for 14 days`, `build directory: <dir>`.

- [ ] **Step 1: Write the check script first.** `$EV/trial/script-checks.sh` starts `rust:1-trixie` containers (named `taiyaku-trial-s*`, throttled as the constraints say), each with `/home/davey/projects/taiyaku/.git` mounted read-only and a checkout made the way `ci.yml:68-77` makes one (`git init`, `fetch --depth 1` of `ci-volumes` from the mount, `checkout FETCH_HEAD`; mark the mount as a `safe.directory`), at `/workspace/taiyaku`. It copies `$W/scripts/ci-volumes.sh` in with `docker cp`, runs it by `docker exec` exactly as the workflow step will (`./scripts/ci-volumes.sh || echo "not using a build volume: scripts/ci-volumes.sh failed"`) with `GITHUB_ENV` naming an empty file, and prints one `PASS` or `FAIL` line per case. Volume: `taiyaku-trial-target-script`; the registry is left unmounted except in S8. The cases:

  | Case | Setup | Expected |
  | --- | --- | --- |
  | S1 | no volume | three `no volume:` lines; `not using a build volume: none is mounted at /ci/target`; exit 0; `GITHUB_ENV` file empty |
  | S2 | target volume | `build directory: /ci/target/1.100.0-a`; the env file holds that `CARGO_TARGET_DIR`; a second `docker exec` in the same container finds `1.100.0-a.lock` held (`flock -n` fails) |
  | S3 | S2's container still alive | a second container gets `-b`; a third prints `both directories for 1.100.0 are in use` and writes nothing; once S2's container is removed, a new one gets `-a` |
  | S4 | `rust-toolchain.toml`'s channel edited in the container | `stable`, `1.100`, `../x` and an empty value each print `is not an exact version` and write nothing; `1.91.0` gives `/ci/target/1.91.0-a` and leaves `1.100.0-a/.last-used`'s time unchanged |
  | S5 | planted directories `0.0.1-a` (mark 15 days old), `0.0.2-a` (13 days), `0.0.3-a` (no mark), `0.0.4-a` (15 days, its lock held by another container) | `0.0.1-a` removed with its `removing` line; the other three kept; `0.0.3-a` now has a mark |
  | S6 | a reference file made after the checkout, a second's wait, then the script | `find . -type f ! -newer <reference>` prints nothing; `git status --porcelain --untracked-files=no` prints nothing; `git diff-files --quiet` exits 0 |
  | S7 | target volume mounted read-only (the trial's `docker run` may add `:ro`) | the `scripts/ci-volumes.sh failed` line; exit 0; env file empty |
  | S8 | registry and git volumes (`taiyaku-trial-cargo-registry`, `taiyaku-trial-cargo-git`), empty; two containers start the script at once | both exit 0; exactly one log holds `Downloaded` lines; `cargo fetch --locked --offline` then succeeds in both |

- [ ] **Step 2: Run it before the script exists.** Expected: every case `FAIL`.

- [ ] **Step 3: Write `scripts/ci-volumes.sh`.** This plan is the first place it exists:

```bash
#!/usr/bin/env bash
# Points this job's cargo at a build directory kept between runs, when the
# runner mounted the volume for one, and prints what it found. It only ever
# adds: with no volume it leaves CARGO_TARGET_DIR unset, and the job builds in
# ./target from nothing. docs/ci.md, "Build volumes, and what a run keeps".
set -euo pipefail

root=/ci/target
home=/usr/local/cargo
keep_days=14

for mount in "${home}/registry" "${home}/git" "${root}"; do
  if mountpoint -q "${mount}"; then echo "volume: ${mount}"; else echo "no volume: ${mount}"; fi
done

# cargo's download lock is a file in its home directory, which every job's
# container has to itself, so two jobs would unpack one crate into the shared
# registry at once. After one fetch under a lock in the volume, no later cargo
# call in this job downloads anything.
if mountpoint -q "${home}/registry"; then
  # A job's first cargo call installs the toolchain; where none has been made,
  # this one is it, so that no install holds the lock.
  cargo --version >/dev/null || true
  flock -w 300 "${home}/registry/.ci-volumes.lock" cargo fetch --locked \
    || echo "cargo fetch did not finish; the job downloads as it builds"
fi

if ! mountpoint -q "${root}"; then
  echo "not using a build volume: none is mounted at ${root}"
  exit 0
fi

# An exact version only: a floating channel would put two compilers' coverage
# builds in one directory, which scripts/coverage-build-guard.sh refuses.
channel="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml)"
if ! [[ "${channel}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "not using a build volume: rust-toolchain.toml's channel '${channel}' is not an exact version"
  exit 0
fi

# One run per directory for the whole job. cargo's build lock ends with the
# build, while the tests and the coverage report go on reading the directory.
# A lock file is opened to append, here and below, so that one planted as a
# link to another file empties nothing.
dir=
for slot in a b; do
  exec 9>>"${root}/${channel}-${slot}.lock"
  if flock -n 9; then
    dir="${root}/${channel}-${slot}"
    break
  fi
done
if [ -z "${dir}" ]; then
  echo "not using a build volume: both directories for ${channel} are in use by other runs"
  exit 0
fi
mkdir -p "${dir}"
touch "${dir}/.last-used"

# Other toolchains' directories only: the pinned one's second directory is for
# a second run at once, which would otherwise find it gone when it is needed.
# A directory's lock is taken before it is removed, so a run building in it
# keeps it, and it is renamed first, so a removal cut short leaves nothing
# under a name a run builds in; a later run removes what is left. A directory
# with no mark gets a mark, as its age is unknown.
for other in "${root}"/*/; do
  other="${other%/}"
  if [ ! -d "${other}" ] || [ "${other%-[ab]}" = "${root}/${channel}" ]; then continue; fi
  if [[ "${other}" = *.removing ]]; then
    echo "removing ${other}: an earlier removal did not finish"
    rm -rf "${other:?}" || echo "could not remove ${other}"
  elif [ ! -e "${other}/.last-used" ]; then
    touch "${other}/.last-used" || true
  elif [ -n "$(find "${other}/.last-used" -mmin "+$((keep_days * 24 * 60))")" ]; then
    (
      exec 8>>"${other}.lock" || exit 0
      flock -n 8 || exit 0
      echo "removing ${other}: not used for ${keep_days} days"
      mv "${other:?}" "${other:?}.removing" && rm -rf "${other:?}.removing"
    ) || echo "could not remove ${other}"
  fi
done

# The clone was made before the lock was held, and cargo takes outputs newer
# than a source file as built from it. The refresh records the new times in
# the index, or git diff-files would report every file as changed.
find . -type f -exec touch -c {} +
git update-index -q --refresh

du -sh "${root}"/*/ "${home}/registry" "${home}/git" 2>/dev/null || true

# Written before the lock is kept: a write that fails ends the script with the
# lock released, not held all job beside a build in ./target. Incremental
# compilation is off because its state in a kept directory is an earlier
# run's, which this run would build from. ./target is made because the
# coverage report is written into it and no cargo build makes it now.
mkdir -p target
printf 'CARGO_TARGET_DIR=%s\nCARGO_INCREMENTAL=0\n' "${dir}" >> "${GITHUB_ENV}"

# Keeps the lock once this step has ended: the sleeper inherits descriptor 9
# and lives until the job's container is removed.
setsid sleep infinity </dev/null >/dev/null 2>&1 &

echo "build directory: ${dir}"
```

- [ ] **Step 4: Run the checks.** Expected: every case `PASS`. Fix the script, not the case, unless the case contradicts this plan; if it does, stop and report.

- [ ] **Step 5: Watch each check fail.** Apply each change to the copy inside the container only, run the case, see it `FAIL`, restore:
  - the prune's `+` before `$((keep_days` becomes `-`: S5 fails (`0.0.1-a` kept, `0.0.2-a` removed);
  - the line `flock -n 8 || exit 0` deleted: S5 fails (`0.0.4-a` removed);
  - the `find . -type f -exec touch` line deleted: S6 fails;
  - the `git update-index -q --refresh` line deleted: S6 fails (`git diff-files --quiet` exits 1);
  - `flock -n 9` replaced by `true`: S3 fails (the second container also gets `-a`);
  - the `flock -w 300 ...lock` prefix deleted, leaving `cargo fetch --locked`: S8 fails (both logs download, or one errors).

  The task review added cases S9 to S13 (a re-used directory's old mark made new; no delete without the directory's lock; a no-mark directory that cannot be marked; an unwritable `GITHUB_ENV`; the toolchain installed outside the registry lock) and a watched failure for each; `$EV/trial/script-checks.sh` holds all thirteen cases and eleven failures. Record the eleven failures and the final all-`PASS` run in `$EV/trial/script-checks.log`. The whole-branch review's fix round added cases S14 (lock files that are links) and S15 (a removal stopped part way), changed S1 to S5, S9 and S11, and added a watched failure for each change to the script; the same log records them.

- [ ] **Step 6: Lint and commit.** `node scripts/check-comments.mjs --self-test` and `node scripts/check-comments.mjs --base origin/main` pass; `bash -n scripts/ci-volumes.sh` passes. Remove the trial containers by name and, in its own call, `docker volume rm taiyaku-trial-target-script taiyaku-trial-cargo-registry taiyaku-trial-cargo-git`. Commit `scripts/ci-volumes.sh`: "ci: a script that points a job's cargo at a kept build directory".

### Task 2: the four jobs mount the volumes and run the script

**Files:**
- Modify: `.forgejo/workflows/ci.yml`: `lint` (`:62-78`), `test` (`:163-179`), `default-features` (`:399-415`), `wasm` (`:464-480`)

**Interfaces:**
- Consumes: `scripts/ci-volumes.sh` and its mount points from Task 1.
- Produces: step name `Build volumes`; target volumes `taiyaku-ci-target-lint`, `-test`, `-default-features`, `-wasm`.

- [ ] **Step 1: Add the block and the step to each of the four jobs.** The shape, for `lint`; the other three differ only in the target volume's name:

```yaml
    container:
      image: rust:1-trixie
      volumes:
        - taiyaku-ci-cargo-registry:/usr/local/cargo/registry
        - taiyaku-ci-cargo-git:/usr/local/cargo/git
        - taiyaku-ci-target-lint:/ci/target
    steps:
      - name: Checkout
        # unchanged
      - name: Build volumes
        run: |
          ./scripts/ci-volumes.sh || echo "not using a build volume: scripts/ci-volumes.sh failed"
```

  One comment block, on `lint`'s `volumes:` only, of at most 8 lines: the names must match the runner's `container.valid_volumes` glob; the step sits after the clone because the script comes from it and before `Formatting` because every cargo call after it builds in the kept directory; a pointer to `docs/ci.md`, "Build volumes, and what a run keeps". The other three jobs get one line pointing at `lint`'s. Say nothing yet about what the runner does with a volume it does not allow: Task 6 finds that out.

- [ ] **Step 2: Prove the shape.** With whichever YAML parser is installed (`python3 -c 'import yaml'`, else `ruby -ryaml`), assert for each of the four jobs that `container.volumes` is exactly its three entries and `steps[1].name` is `Build volumes`; that `web` and `docker` have no `volumes`; that every `timeout-minutes` is what `origin/main` has. Then `git -C $W diff origin/main -- .forgejo/workflows/ci.yml` shows no removed line and no added line inside an existing step. Keep the output in `$EV/task2-shape.log`.

- [ ] **Step 3: Comment guard, commit.** `node scripts/check-comments.mjs --base origin/main` passes. Commit: "ci: the four Rust jobs mount build volumes".

### Task 3: the `test` and `wasm` jobs, cold and warm, in the image

Evidence only; it changes a repository file only if it finds a defect, and then it stops and reports before fixing.

**Files:**
- Evidence: `$EV/trial/job.sh`, `$EV/trial/compare-lines.mjs`, logs and reports under `$EV/trial/`

**Interfaces:**
- Consumes: the branch as committed after Task 2.

- [ ] **Step 1: Write the driver.** `$EV/trial/job.sh` runs one job's steps in one new container (`taiyaku-trial-j*`, throttled, workspace `/workspace/taiyaku`, the checkout made as in Task 1, never from `git archive`: it keeps commit times and cargo then skips the workspace's crates). One `docker exec` per workflow step, in the job's order, each `run:` text taken from the branch's `ci.yml`; lines written to the `GITHUB_ENV` file by one step are passed as environment to every later one; `TAIYAKU_COMMIT` is the commit checked out. Confirm with `grep -aF` that the coverage command, the report command and the two node steps in the driver are character for character `ci.yml`'s. Volumes for `test`: `taiyaku-trial-cargo-registry`, `taiyaku-trial-cargo-git`, `taiyaku-trial-target-test`; for `wasm` the same two and `taiyaku-trial-target-wasm`.

- [ ] **Step 2: Write the comparison.** `$EV/trial/compare-lines.mjs <a.json> <b.json>` reads two `cargo llvm-cov report --json` files and prints every file whose `summary.lines.count` or `summary.lines.covered` differs, or that only one report holds; exit 1 if it printed any.

- [ ] **Step 3: Fill, with a planted change, interrupted once (run F).** In the container's checkout only, append a small uncalled public function to one `taiyaku-core` source file that the report lists. Start the `test` job with the three volumes, empty; once `Compiling` lines are scrolling and before the build ends, `docker rm -f` the container by name. Start F again in a new container. Expected: it runs to the end; every step exits 0. Keep `F.log` and `F.json`.

- [ ] **Step 4: Warm, on the branch as committed (run W).** New container, same volumes, no planted change. Expected, each recorded in `$EV/trial/W-checks.log`:
  - the first step prints three `volume:` lines and `build directory: /ci/target/1.100.0-a`;
  - every step exits 0, the coverage floor and the test-count step included;
  - `W.log` has a `Compiling` line for each of the six workspace crates and for no other crate; none names `tikv-jemalloc-sys`;
  - `find /ci/target -name junit.xml -o -name rust-coverage.json` prints nothing, and both files exist under the workspace's `target/`.

- [ ] **Step 5: Cold, with no volume (run C).** New container, no volumes, the branch as committed. Expected: three `no volume:` lines and `not using a build volume: none is mounted at /ci/target`; every step exits 0; the build lands in the workspace's `target/`.

- [ ] **Step 6: Compare.** `compare-lines.mjs W.json C.json` prints nothing and exits 0. `compare-lines.mjs F.json C.json` prints the planted file and nothing else, which is what shows the comparison can fail. If W and C differ, run W once more and compare the two warm reports before deciding: a difference between two warm runs is the suite's own, and is reported as such, not blamed on the volume.

- [ ] **Step 7: The `wasm` job twice.** Fresh container each time, volumes `taiyaku-trial-target-wasm` and the two shared ones. Expected both times: every step exits 0, the smoke run included, and `crates/taiyaku-wasm/pkg/taiyaku_wasm_bg.wasm` exists in the workspace; the second log compiles no crate outside the workspace. If `wasm-pack` builds `wasm-bindgen-cli` itself, record it and go on.

- [ ] **Step 8: A warm `lint` does not replay an earlier pass.** Volume `taiyaku-trial-target-lint` and the two shared ones. Run `lint`'s steps up to and including `Clippy` once on the branch as committed: exit 0. Then, in a new container, plant in the checkout one construct `-D warnings` refuses (an unused variable in a `taiyaku-core` function) and run the same steps. Expected: `Clippy` fails and names the planted line, having compiled no crate outside the workspace.

- [ ] **Step 9: A toolchain change starts a new directory.** In a new container on `taiyaku-trial-target-lint`, set `rust-toolchain.toml`'s channel to the version `Cargo.toml`'s `rust-version` declares, in the checkout only, and run the script and `cargo check --workspace --all-features --locked`. Expected: `build directory: /ci/target/<that version>-a`; the log compiles dependencies from nothing; `find /ci/target/1.100.0-a -newer <a file made before this step>` prints nothing.

- [ ] **Step 10: Remove what the trial made.** The containers by name; then, in its own call, `docker volume rm taiyaku-trial-cargo-registry taiyaku-trial-cargo-git taiyaku-trial-target-test taiyaku-trial-target-wasm taiyaku-trial-target-lint`. `docker volume ls --filter name=taiyaku-trial` prints no volume. Write `$EV/trial/summary.md`: each expectation above with the log line that met it. No commit.

### Task 4: every sentence the volumes make false, and the section that replaces "there is none"

Nothing here states a figure from the forge or what the deployed runner does with a refused volume; Task 7 adds those.

**Files:**
- Modify: `docs/ci.md`, `scripts/check-all.sh`, `.forgejo/workflows/ci.yml:572`, `crates/taiyaku-cli/Cargo.toml:43-47`, `docs/status.md:254`

- [ ] **Step 1: Rewrite `docs/ci.md:1935-1972`** under the heading `## Build volumes, and what a run keeps`. It says, from the design and Tasks 1 to 3 and with no number that is not a name or a setting:
  - what is kept and where: the six volumes, their mount points, which jobs share which;
  - what is not: the workspace's own crates (a fresh clone's files are newer than any output, so cargo rebuilds them), rustup's toolchain download, the tool tarballs, and the `web` and `docker` jobs, `advisories.yml` and `mutants.yml`;
  - the layout inside a target volume: a directory per pinned toolchain, two of them, the lock held for the whole job and why cargo's own lock is not enough (C1), what a third run does, the mark, the two weeks, and that the lint job's check at the declared Rust version builds in the same directory;
  - the fetch under a lock and why cargo's own download lock does not cover it (C2);
  - why nothing stale is read: the two files steps read by name are written in the workspace; `cargo llvm-cov` clears the workspace's artifacts and profile data before each run; the script sets the clone's file times after taking the lock;
  - trust: any run, a pull request's included, writes volumes that main's gate jobs later read; the forge is sign-in only; the image the `docker` job ships never comes from them;
  - no size cap and why (a wipe under a running build fails it); clearing by hand with the runner idle, as one `docker volume rm` naming the six volumes literally;
  - one host: a job that lands on a runner on another host does not find these volumes;
  - what was checked locally (Task 3's W against C, the interrupted fill, Task 1's cases), in words, pointing at `.superpowers/sdd/ci-volumes-evidence/` and saying it is not committed;
  - the two approaches not taken, kept from the old text: a cache action after a throwaway node install, and an image with dependencies built in.

- [ ] **Step 2: Correct each site.** Read the range, change only what is now false:

  | Site | Now false | Becomes |
  | --- | --- | --- |
  | `docs/ci.md:189-191` | "There is no cache ... compiles the workspace from nothing" | dependencies are kept; the workspace's own crates still compile from nothing each run, which is the part a release profile slows |
  | `docs/ci.md:199-200` | "which is CI's condition on every run" | it was CI's condition when measured; the release figures were not re-measured with the volumes |
  | `docs/ci.md:1265-1267` | "this CI has no cache" and the old heading | the `web` job has none; the new heading |
  | `docs/ci.md:1930-1933` | reads as "caching needs an action" | cache *actions* and artifact upload need one; the volumes do not |
  | `docs/ci.md:1991-1992` | "the only thing an image change buys today is the caching" | it buys actions; caching no longer needs it |
  | `docs/ci.md:1147-1156` | silent on CI | one sentence: CI's coverage build sits in a directory named for the pin, so CI runs no such guard, and a floating channel gets no volume |
  | `docs/ci.md:110-116` | "reproduces all five jobs ... same order" omits the new step | one sentence: the `Build volumes` step is CI's alone, as a local target directory already persists |
  | `scripts/check-all.sh:11`, `:497` | "a cacheless CI", "CI has no cache" | the `web` job has no cache |
  | `scripts/check-all.sh:139-142` | "CI starts from an empty target directory" | local only because CI's coverage build is in a directory named for its toolchain |
  | `.forgejo/workflows/ci.yml:572` | "this cacheless CI" | this job has no cache |
  | `crates/taiyaku-cli/Cargo.toml:43-47` | "CI has no cargo cache ... from scratch every run" | the reason the cost is accepted, without the claim that it is paid every run |
  | `docs/status.md:254` | "a cacheless CI" | the `web` job has no cache |

  These stay, and the task confirms each by reading it: `advisories.yml:6` and `:79-81` (that job mounts nothing, so "nothing is cached" holds); `docs/ci.md:2143-2146` (rustup's download is not cached); `docs/ci.md:447`; `docs/status.md:191` and `docs/notes.md:8508` (records of a past choice and a past measurement, kept as written); `ci.yml:53`, `docs/ci.md:268-269` and O39's record (the 134 s decision above).

- [ ] **Step 3: Sweep for what the table missed.** `grep -rn -a` over `docs/*.md`, `scripts/`, `.forgejo/`, `crates/`, `web/src`, `README.md` and `Dockerfile` (not `docs/superpowers/`) for `cacheless`, `no cache`, `no cargo cache`, `nothing is cached`, `from cold`, `compiles from`, `empty target`, `from scratch every run` and the old heading `Caching, and why there is none`. Read every hit; each is either corrected or listed in the report with why it stays.

- [ ] **Step 4: The local gate.** `web/node_modules` must be a real install (`pnpm -C $W/web install --frozen-lockfile` if absent, never a symlink). Run, as one command and not through a pipe: `env -C $W env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 CARGO_TARGET_DIR=$W/target scripts/check-all.sh >| $EV/gate-task4.log 2>&1`. Its exit status is 0, and `grep -a -n 'all green' $EV/gate-task4.log` gives the closing line; quote it with any `NOT MEASURED` entries under it. A red browser project with no failed assertion, or book search's two timing tests (O413), under load: read `/proc/loadavg` and run the gate again before suspecting the branch.

- [ ] **Step 5: Commit.** "docs: what the build volumes keep, and the sentences they make false".

**Controller, after Task 4:** confirm the runner restarted with `container.valid_volumes: ["taiyaku-ci-*"]`; on the owner's word push the branch and open the pull request. Do not start Task 5 before the first run has been queued. A first run the runner cancels at its time limit leaves its volume partly filled and its lock released: run it again.

### Task 5: the probe commit (temporary)

**Files:**
- Modify: `.forgejo/workflows/ci.yml`, the `wasm` job only

- [ ] **Step 1: Add a fourth volume and one step to `wasm`.** The volume `taiyaku-refused-probe:/ci/refused`, a name outside the runner's glob, and straight after `Build volumes`:

```yaml
      - name: Probe (temporary)
        run: |
          if mountpoint -q /ci/refused; then echo "probe: /ci/refused IS mounted"; else echo "probe: /ci/refused is not mounted"; fi
          for lock in /ci/target/*.lock; do
            [ -e "$lock" ] || continue
            if flock -n "$lock" true; then echo "probe: $lock is free"; else echo "probe: $lock is held"; fi
          done
```

- [ ] **Step 2: Commit** it alone: "probe: a volume the runner does not allow (temporary)". Report the commit to the controller, who pushes it.

### Task 6: read what the deployed runner did, then take the probe out

**Inputs from the controller:** the number of the run that carried the probe commit, and whether any earlier run on this branch had already filled the volumes.

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (the revert)
- Evidence: `$EV/forge/`

- [ ] **Step 1: Read the four jobs' `Build volumes` step** through the gitea MCP or the REST API (token in `~/.config/tea/config.yml`; never `gh`). Save each step's output. Expected in all four: three `volume:` lines and a `build directory:` line. If any job prints `no volume:` for a `taiyaku-ci-*` mount, stop: the runner's setting is not in effect, and that is the controller's to fix.
- [ ] **Step 2: Read the `wasm` job's probe step.** Expected: `probe: /ci/refused is not mounted`, exactly one `.lock` reported `held`, and the job went on to its later steps. Save it.
- [ ] **Step 3: Read the runner's log for that job**, the one place a task uses the NAS: `ssh -o BatchMode=yes root@192.168.1.69`, read-only, the runner container's log around the job's start, searched for `taiyaku-refused-probe` and `volume`. Save the lines. Change nothing there.
- [ ] **Step 4: Write `$EV/forge/refused-volume.md`:** what the deployed v12.13.2 did, in the log's own words. If it differs from the design's reading of the source (the job failed, or the volume was mounted, or a held lock read as free), stop and report: the docs then say what was seen, and the design's "fails soft" or correction C1 does not hold as written.
- [ ] **Step 5: Revert** the probe commit with `git -C $W revert --no-edit <it>` (a new commit; never amend). `git -C $W diff <the commit before the probe> -- .forgejo/workflows/ci.yml` is empty. Report to the controller, who pushes.

**Controller, after Task 6:** let at least two runs of the reverted branch finish with no other run of this repository in flight, then start two runs close enough together that the same job of both is running at once (a `workflow_dispatch` on the branch while a push's run is in its `test` job). Supply Task 7 with: the run that found each volume empty, two later warm runs, the overlapping pair, and the open-items number to use (above main's highest and above every number an unmerged lane has claimed).

### Task 7: the figures, the runner prerequisite, the open item, the gate

**Inputs from the controller:** as above.

**Files:**
- Modify: `docs/ci.md` (the new section; `:1894-1913`), `.forgejo/workflows/ci.yml` (`lint`'s volume comment), `docs/status.md`
- Evidence: `$EV/forge/`

- [ ] **Step 1: Collect.** For the empty-volume run and the first warm run, each of the four jobs' durations from the forge's tasks API, saved as the API returned them; from the second warm run's `Build volumes` steps, the six sizes `du` printed; from the warm runs' logs, the `Compiling` lines of the `test` job (workspace crates only, or the plan's claim is wrong: stop and report); from the overlapping pair, the two `Build volumes` outputs of the job that overlapped. A job the runner cancelled at its time limit has no duration to quote: the docs say it was cancelled, in words.
- [ ] **Step 2: Add to the new section of `docs/ci.md`:** one table, four jobs by the two named runs, with each job's target volume size, and the registry and git sizes in a sentence; that each is a single run on a runner whose load swings, named by run number so it can be re-read; no average, no ratio, no "about". What the overlapping pair showed (the second took `-b`). That the bound under "Bounding a test's runtime" was sized before these figures and is the open item's to revisit.
- [ ] **Step 3: The runner prerequisite**, in `docs/ci.md`'s "Runner requirements": `container.valid_volumes: ["taiyaku-ci-*"]` in the runner's config, and what a runner without it does, as `$EV/forge/refused-volume.md` records it for the deployed version. Add the same fact in one line to `lint`'s volume comment in `ci.yml`.
- [ ] **Step 4: Open one item in `docs/status.md`** with the number the controller gave: `timeout-minutes: 10` is justified at `ci.yml`'s head and in `docs/ci.md`'s "Bounding a test's runtime" by a worst job of 134 s, which no longer holds; the owner decides the bound from the figures in "Build volumes, and what a run keeps". Site by name (`ci.yml`'s `timeout-minutes`), dated by the Pacific day. Opening is five edits; `git -C $W show ccbf74ff -- docs/status.md` shows all five done for O413: the count word at the head of "Open items", the "leaving" count, the table row (exactly six `|`), the prose bullet, and "What remains". Then the sweep: `grep -c '^| O[0-9]' docs/status.md` against the count word, `grep -c '^| O[0-9]* | \[x\]'` against the closed count, `grep -n -A1 '^What remains:'`.
- [ ] **Step 5: Check every figure against its source.** Each number in the table and the sentence, against the saved API output or log line; write the pairs to `$EV/forge/figures-checked.md`.
- [ ] **Step 6: The gate again**, as Task 4's Step 4, into `$EV/gate-task7.log`; quote its `all green` line.
- [ ] **Step 7: Commit.** "docs: the build volumes measured on the forge, and the job bound filed as open". Report the open item's number and the counts to the controller for the pull request's body.
