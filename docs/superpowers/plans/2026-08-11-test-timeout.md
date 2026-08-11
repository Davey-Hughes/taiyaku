# Bounding a test's runtime — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every test a runtime bound and every CI job a wall-clock bound, so a non-terminating loop fails the forge by name instead of hanging it — closing O39.

**Architecture:** Two independent bounds. `.config/nextest.toml` bounds each *test* (marked `SLOW` at 30 s, terminated at 60 s) with the corpus and one-off experiment modules exempted from termination; `timeout-minutes: 10` bounds each *job*. The `test` job's runner changes from `cargo llvm-cov` to `cargo llvm-cov nextest`, and `scripts/check-all.sh` changes identically so the local gate keeps mirroring CI.

**Tech Stack:** `cargo-nextest` 0.9.140, `cargo-llvm-cov` 0.8.7, Forgejo Actions, `rust:1-bookworm`.

**Design:** `docs/superpowers/specs/2026-08-11-test-timeout-design.md`. Read it before starting — every number below traces to a measurement recorded there.

## Global Constraints

- **Branch is `o39-test-timeout`, already created off `origin/main` and already carrying two commits — `e896a9b` (the spec) and `6ee2c4c` (this plan).** Do not create a new branch. Do not commit to `main` — it is protected by a pre-receive hook.
- **The remote is Gitea/Forgejo at `git.daveynet.xyz`, not GitHub.** Use the `gitea` MCP tools for PRs. `gh` is authenticated against the wrong host.
- **Merges are squash-only, and the PR description *is* the squash commit message.** Local commit messages are never read by the squash. Write the PR body as the commit you want on `main`.
- **PR bodies must not be hard-wrapped** — one line per paragraph, per list item, per blockquote. Gitea renders comments with GFM hard line breaks on, so a newline inside a paragraph becomes a `<br/>`.
- **Never run `scripts/check-all.sh` with `TAIYAKU_CORPUS` set.** It exports the variable into an unfiltered `cargo test --workspace`, which runs `geometry_experiment` in the debug profile — 40+ minutes; one run was killed by hand at 1 h 09 m. Always `env -u TAIYAKU_CORPUS scripts/check-all.sh`.
- **The Bash tool runs zsh with `noclobber`.** `>` onto an existing file silently no-ops. Always `rm -f` the target first when writing a file in a loop or a chain.
- **`cargo-nextest` pin: version `0.9.140`.** Asset: `https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.140/cargo-nextest-0.9.140-x86_64-unknown-linux-musl.tar.gz`. SHA-256: `bc2a998567816eef7b087b920036f49fb89aa2a8a603f6a3d84098be160b62c5` — computed from the artefact on 2026-08-11, 9,605,767 bytes, containing `cargo-nextest` at the tarball root.
- **Bound values, all measured (spec §3):** per-test warn 30 s, per-test kill 60 s, per-job 10 min.

## File Structure

| File | Status | Responsibility |
| ---- | ------ | -------------- |
| `.config/nextest.toml` | **create** | The per-test bound and the one exemption. The only file that names 30 s / 60 s. |
| `.forgejo/workflows/ci.yml` | modify | Adds `timeout-minutes` to four jobs; adds the nextest install step; switches the `test` job's runner. |
| `scripts/check-all.sh` | modify | Mirrors the runner switch and guards on `cargo-nextest` the way it already guards on `cargo-llvm-cov` and `node`. |
| `docs/notes.md` | append | The measurements. `status.md`'s preamble promises every claim there traces to one recorded here. |
| `docs/status.md` | modify | The five-part O39 close. |

**The cross-file invariant.** `scripts/check-all.sh` states, in its own comments, that its six coverage invocation lines and `ci.yml`'s hash identically once leading whitespace and the `run` / `run env -u TAIYAKU_CORPUS` prefixes are stripped. Task 2 changes one of those six lines and **must change it in both files in the same commit**, then re-prove the property.

---

### Task 1: The nextest config, and proof the bound fires

**Files:**
- Create: `.config/nextest.toml`
- Temporarily modify (reverted within this task): `crates/taiyaku-core/src/sidecar.rs`, `crates/taiyaku-cli/src/corpus.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `.config/nextest.toml` with a `default` profile and one override. Task 2 relies on `cargo nextest` picking this up automatically — nextest reads `.config/nextest.toml` from the workspace root with no flag.

- [ ] **Step 1: Create the config**

Create `.config/nextest.toml`:

```toml
# Nothing else in this repository bounds a test's runtime, and until this file
# existed nothing did at all: CI runs `cargo llvm-cov`, which wraps `cargo
# test`, which has no timeout. A regression that stopped a loop terminating
# would hang the forge until the runner's own limit rather than fail it. See
# docs/status.md, O39.
#
# 30 s is 40x the slowest test in the workspace and 60 s is 80x. Measured, not
# guessed: running the whole suite under a temporary short `slow-timeout` and
# raising the period, four tests are marked at 250 ms, two at 500 ms
# (`detect::tests::the_gap_does_not_grow_with_the_page` and
# `cover::tests::a_cover_is_a_bounded_jpeg_whatever_the_page_was`) and none at
# 750 ms. Nothing in 639 tests exceeds 0.75 s. The slack is for a shared docker
# runner, which is slower than the machine that measured this by a factor
# nobody has measured — the one guess here, and it says so.
[profile.default]
slow-timeout = { period = "30s", terminate-after = 2 }

# The exempt set is not a new list: it is exactly the four files
# `--ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'`
# already names in both coverage steps of ci.yml and check-all.sh. One exempt
# set, two gates, one reason — these are gated one-off measurements against
# 26 GB of real material, not unit tests.
#
# A global kill would destroy a deliberate measurement: `geometry_experiment`
# in the debug profile with TAIYAKU_CORPUS set runs 40+ minutes, and one run
# was killed by hand at 1 h 09 m. So these are marked SLOW and left alone —
# omitting `terminate-after` is what makes a slow-timeout advisory, not a
# separate key.
#
# `test(corpus::)` selects the same sixteen tests `cargo test corpus::` does:
# thirteen standing checks against real archives (ten in taiyaku-cli, three in
# taiyaku-serve) plus three `corpus::tests::` helpers that only assert how
# TAIYAKU_CORPUS is validated. Matching cargo test's existing selection exactly
# is the point — the exempt set must be the set the corpus step already runs,
# or the two disagree silently.
[[profile.default.overrides]]
filter = "test(corpus::) + test(geometry_experiment) + test(region_experiment)"
slow-timeout = { period = "60s" }
```

- [ ] **Step 2: Prove the filter selects exactly the intended set**

Run:

```bash
cargo nextest list --workspace --all-features --color never \
  -E 'test(corpus::) + test(geometry_experiment) + test(region_experiment)' | grep -c '::'
```

Expected: `30` — 16 corpus tests plus 14 in the two experiment modules. Both halves were measured on 2026-08-11; `cargo nextest list` prints one test per line as `<binary> <test::path>` with no header lines, so `grep -c '::'` counts tests and nothing else. Then confirm the corpus half matches `cargo test`'s selection exactly:

```bash
cargo nextest list --workspace --all-features --color never -E 'test(corpus::)' | grep -c '::'
env -u TAIYAKU_CORPUS cargo test --workspace --all-features corpus:: -- --list 2>/dev/null | grep -c ': test'
```

Expected: both print `16`.

**Do not count by grepping nextest's default coloured output.** It inserts ANSI escapes between `corpus` and `::`, so a `grep -c 'corpus::'` on the coloured stream undercounts to 3. `--color never` is required.

If the first command prints something other than 30, record the actual number and the actual test list in the task report rather than adjusting the filter to match it — the experiment modules' test count is pinned nowhere else and may legitimately have moved, but a *corpus* count other than 16 means the filter is wrong, not the codebase.

- [ ] **Step 3: Write a test that hangs, in a crate the default profile covers**

Add to `crates/taiyaku-core/src/sidecar.rs`, inside the existing `mod tests`, immediately before `fn the_stored_average_run_length_is_written_to_five_decimals`:

```rust
    #[test]
    fn tmp_hangs_outside_the_exempt_set() {
        loop {
            std::hint::spin_loop();
        }
    }
```

- [ ] **Step 4: Write a second hanging test, inside the exempt set**

Add to `crates/taiyaku-cli/src/corpus.rs`, at the end of the file, at module level (**not** inside `mod tests` — the standing checks live at module level, which is why `test(corpus::)` matches them):

```rust
#[test]
fn tmp_hangs_inside_the_exempt_set() {
    loop {
        std::hint::spin_loop();
    }
}
```

- [ ] **Step 5: Run both under a temporary short profile that terminates both**

The real config never kills the exempt set, so a run proving the override applied would not terminate. Use a temporary profile where **both** halves terminate but at *different* multiples — the differing kill times are the proof the override bound the right tests.

```bash
rm -f .config/nextest.toml.real && cp .config/nextest.toml .config/nextest.toml.real
rm -f .config/nextest.toml && cat > .config/nextest.toml <<'EOF'
[profile.default]
slow-timeout = { period = "5s", terminate-after = 1 }

[[profile.default.overrides]]
filter = "test(corpus::) + test(geometry_experiment) + test(region_experiment)"
slow-timeout = { period = "5s", terminate-after = 2 }
EOF
env -u TAIYAKU_CORPUS cargo nextest run --workspace --all-features --color never \
  -E 'test(tmp_hangs)' 2>&1 | grep -E 'TIMEOUT|SLOW|Summary'
```

Expected: two `TIMEOUT` lines, and the times differ by design —

```
        SLOW [>  5.000s] taiyaku-core sidecar::tests::tmp_hangs_outside_the_exempt_set
     TIMEOUT [   5.000s] taiyaku-core sidecar::tests::tmp_hangs_outside_the_exempt_set
        SLOW [>  5.000s] taiyaku-cli::bin/taiyaku corpus::tmp_hangs_inside_the_exempt_set
        SLOW [> 10.000s] taiyaku-cli::bin/taiyaku corpus::tmp_hangs_inside_the_exempt_set
     TIMEOUT [  10.000s] taiyaku-cli::bin/taiyaku corpus::tmp_hangs_inside_the_exempt_set
```

**The 5 s versus 10 s split is the whole point of this step.** If both terminate at 5 s the override did not match, and the filter in Step 1 is wrong. Record the actual output verbatim.

- [ ] **Step 6: Restore the real config and prove it kills at 60 s**

```bash
rm -f .config/nextest.toml && mv .config/nextest.toml.real .config/nextest.toml
rm -f /tmp/nextest-kill-check.txt
env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-core --all-features --color never \
  -E 'test(tmp_hangs_outside_the_exempt_set)' > /tmp/nextest-kill-check.txt 2>&1
echo "exit: $?"
grep -E 'TIMEOUT|SLOW|Summary' /tmp/nextest-kill-check.txt
```

Expected — this takes 60 seconds:

```
        SLOW [> 30.000s] taiyaku-core sidecar::tests::tmp_hangs_outside_the_exempt_set
     TIMEOUT [  60.000s] taiyaku-core sidecar::tests::tmp_hangs_outside_the_exempt_set
```

and a non-zero exit. **The exit code is captured before the grep**, so it is
`cargo nextest run`'s own status, not `grep`'s — piping straight to `grep` and
reading `$?` afterward would report the pipeline's last command instead. Do
not use `PIPESTATUS`; the Bash tool runs zsh. Record the exact lines — they
are quoted in the PR body.

- [ ] **Step 7: Remove both temporary tests**

Delete `tmp_hangs_outside_the_exempt_set` from `crates/taiyaku-core/src/sidecar.rs` and `tmp_hangs_inside_the_exempt_set` from `crates/taiyaku-cli/src/corpus.rs`. Verify nothing remains:

```bash
grep -rn "tmp_hangs" crates/ ; echo "hits above: none expected"
```

- [ ] **Step 8: Confirm the suite is untouched**

```bash
env -u TAIYAKU_CORPUS cargo nextest run --workspace --all-features --color never 2>&1 | tail -3
```

Expected: `Summary [ … ] 639 tests run: 639 passed`, zero `SLOW`, zero `TIMEOUT`.

- [ ] **Step 9: Commit**

```bash
git add .config/nextest.toml
git commit -m "nextest: bound a test's runtime at 60s, exempting the corpus and experiment modules"
```

---

### Task 2: Switch both gate files to nextest, in one commit

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (the `test` job — one new step, one changed line)
- Modify: `scripts/check-all.sh:182-186` (the tool guard and the changed line)

**Interfaces:**
- Consumes: `.config/nextest.toml` from Task 1.
- Produces: both gate files invoking `cargo llvm-cov nextest --workspace --all-features`. Task 3 adds `timeout-minutes` to the same file and must not disturb these lines.

**Why one commit:** `scripts/check-all.sh` documents that its six coverage invocation lines and `ci.yml`'s hash identically after normalisation. Splitting this across two commits leaves that invariant broken at a commit that could be checked out.

- [ ] **Step 1: Add the nextest install step to the `test` job**

In `.forgejo/workflows/ci.yml`, in the `test` job, insert immediately **after** the `Install cargo-llvm-cov` step and **before** `Tests, with coverage`:

```yaml
      - name: Install cargo-nextest
        # A prebuilt release binary, the pattern this file already uses three
        # times over — for cargo-llvm-cov above, and for node and wasm-pack in
        # the jobs below. The musl build is static, so it runs on the glibc
        # bookworm container without a matching libc.
        #
        # **The checksum is not optional and it is not upstream's.** nextest
        # publishes no checksum file for its release assets, and a GitHub
        # release asset can be replaced in place by whoever owns the
        # repository. The digest below was computed here, from the artefact
        # this workflow was written against. Bump it together with the version,
        # by downloading and re-computing — never by copying a value from an
        # issue or a README.
        run: |
          curl -fsSL -o /tmp/nextest.tar.gz \
            https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-0.9.140/cargo-nextest-0.9.140-x86_64-unknown-linux-musl.tar.gz
          echo "bc2a998567816eef7b087b920036f49fb89aa2a8a603f6a3d84098be160b62c5  /tmp/nextest.tar.gz" | sha256sum -c -
          tar -xzf /tmp/nextest.tar.gz -C /usr/local/bin
          cargo nextest --version
```

- [ ] **Step 2: Change the runner in `ci.yml`**

In the same job, in the `Tests, with coverage` step, change exactly one line:

```diff
-          cargo llvm-cov --workspace --all-features \
+          cargo llvm-cov nextest --workspace --all-features \
```

Then, in that step's comment block, replace the first sentence:

```diff
-        # `cargo llvm-cov` runs the tests — this is the `cargo test` step, not
-        # an addition to it. Four files are ignored: two gated one-off
+        # `cargo llvm-cov nextest` runs the tests — this is the test step, not
+        # an addition to it, and the `nextest` subcommand is what applies
+        # `.config/nextest.toml`'s per-test timeout. Four files are ignored:
+        # two gated one-off
```

- [ ] **Step 3: Change the runner and the guard in `check-all.sh`**

In `scripts/check-all.sh`, replace the tool guard and the first invocation:

```diff
-if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
-  note_skip "the Rust coverage gate on the test job (cargo-llvm-cov not on PATH — tests still ran, the threshold did not)"
+if ! command -v cargo-llvm-cov >/dev/null 2>&1 || ! command -v cargo-nextest >/dev/null 2>&1; then
+  command -v cargo-llvm-cov >/dev/null 2>&1 \
+    || note_skip "the Rust coverage gate on the test job (cargo-llvm-cov not on PATH — tests still ran, the threshold did not)"
+  command -v cargo-nextest >/dev/null 2>&1 \
+    || note_skip "the per-test timeout bound (cargo-nextest not on PATH — tests still ran, nothing bounded how long any of them could take)"
   run env -u TAIYAKU_CORPUS cargo test --workspace --all-features
 else
-  run env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features \
+  run env -u TAIYAKU_CORPUS cargo llvm-cov nextest --workspace --all-features \
     --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$' \
     --fail-under-lines 93
```

Two tools now guard one branch, and each names its own skip, because a machine missing only one of them should be told which. The fallback stays a plain `cargo test`: it is the one line with no CI counterpart, deliberately.

- [ ] **Step 4: Re-prove the cross-file invariant**

Run:

```bash
extract() {
  sed -E 's/^[[:space:]]+//; s/^run (env -u TAIYAKU_CORPUS )?//' "$1" | grep -v '^#' \
    | grep -E '^cargo llvm-cov( nextest)? --workspace|^--ignore-filename-regex|^--fail-under-lines 93|^cargo llvm-cov report --json|^node scripts/coverage-floor\.mjs target/rust-coverage\.json'
}
diff <(extract .forgejo/workflows/ci.yml) <(extract scripts/check-all.sh) && echo "byte-identical: OK (6 lines)"
```

Expected: `byte-identical: OK (6 lines)`, and the first extracted line reads `cargo llvm-cov nextest --workspace --all-features \`.

This check was confirmed passing against the pre-change files, so a failure here means the edit, not the check.

- [ ] **Step 5: Run the whole local gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | tail -12
```

Expected: `all green, except NOT MEASURED: - standing corpus checks (TAIYAKU_CORPUS unset)`. Specifically confirm the per-file floor still reports `38 files (floor requires 32), all above the per-file floor`, and that `--fail-under-lines 93` passed.

**Coverage under nextest is nondeterministic, by 2–5 lines out of 17,305.** Measured three times each on an unchanged tree: `cargo test` returns 399 uncovered every run with zero per-file variance; `cargo llvm-cov nextest` returns 402, 397, 399 — i.e. **97.68%–97.71%**. The variance is confined to `taiyaku-serve/src/lib.rs` and `taiyaku-serve/src/routes.rs`, and its cause is nextest's process-per-test isolation changing tokio scheduling in the async server crate.

So: anything in the range 97.6–97.8% is expected, and is not a finding. A figure below **97%** is a finding — report it, do not adjust anything to accommodate it.

- [ ] **Step 6: Commit**

```bash
git add .forgejo/workflows/ci.yml scripts/check-all.sh
git commit -m "ci: run the tests under nextest, in both gate files together"
```

---

### Task 3: A wall-clock bound on every job

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (four jobs)

**Interfaces:**
- Consumes: nothing from Task 2 beyond the file being in its post-Task-2 state.
- Produces: nothing later tasks read.

- [ ] **Step 1: Add `timeout-minutes: 10` to all four jobs**

For each of `lint`, `test`, `wasm` and `web`, add the key directly under `runs-on: docker`:

```yaml
  lint:
    runs-on: docker
    timeout-minutes: 10
    container:
      image: rust:1-bookworm
```

Apply the identical two-word change to `test`, `wasm` and `web`.

- [ ] **Step 2: Add the comment that explains the number, once**

Immediately above the `jobs:` key, append to the existing top-of-file comment block:

```yaml
# Every job carries `timeout-minutes: 10`, and it is the outer half of a bound
# that has two halves. `.config/nextest.toml` bounds a *test* — precisely, by
# name, at 60 s. This bounds a *job*, which is the only thing that can catch a
# hang outside a test: a stalled `curl` in one of the four tool-install steps
# below, or a `cargo build` that never finishes. Neither subsumes the other.
#
# 10 minutes is 4.5x the worst job this repository has ever run. Measured from
# the forge's own /actions/tasks API over the ten runs before this one: `test`
# 32-134 s, `lint` 21-85 s, `wasm` 33-84 s, `web` 9-25 s. Uniform across all
# four rather than tuned per job, because a per-job number has to be re-tuned
# as jobs shift and the only property this number needs is to sit between "any
# honest run" and "hours".
#
# **This key is untriggered rather than untested**, and deliberately so:
# deliberately hanging a job for ten minutes to watch it die costs a
# ten-minute CI run to verify one documented key, where the per-test bound
# beside it can be triggered in sixty seconds and was. See docs/status.md, O39.
```

- [ ] **Step 3: Verify the YAML still parses and nothing else moved**

```bash
python3 -c "import yaml,sys; d=yaml.safe_load(open('.forgejo/workflows/ci.yml')); print({k: v.get('timeout-minutes') for k,v in d['jobs'].items()})"
git diff --stat .forgejo/workflows/ci.yml
```

Expected: `{'lint': 10, 'test': 10, 'wasm': 10, 'web': 10}`, and a diff touching only the four `runs-on` neighbourhoods plus the comment block.

If `yaml` is not importable, `pip install pyyaml` is not worth it — instead confirm by eye that all four keys sit at the same indentation as `runs-on:` and re-read the diff.

- [ ] **Step 4: Commit**

```bash
git add .forgejo/workflows/ci.yml
git commit -m "ci: bound every job at ten minutes"
```

---

### Task 4: Trigger the bound on the forge

**Files:**
- Temporarily modify (reverted within this task): `crates/taiyaku-core/src/sidecar.rs`

**Interfaces:**
- Consumes: Tasks 1–3, committed.
- Produces: two CI run URLs and the verbatim failure text, both quoted in Task 7's PR body.

**This is the task the design exists for.** Nothing before it has exercised nextest under `cargo llvm-cov` inside `rust:1-bookworm` on this runner. Do not skip it and do not simulate it locally — the local proof already happened in Task 1, and it is not the same path.

- [ ] **Step 1: Add the hanging test back, alone**

Add to `crates/taiyaku-core/src/sidecar.rs`, inside `mod tests`:

```rust
    #[test]
    fn tmp_hangs_to_trigger_the_ci_bound() {
        loop {
            std::hint::spin_loop();
        }
    }
```

- [ ] **Step 2: Commit and push, then open the PR — CI will not run without it**

`ci.yml`'s triggers are `pull_request` and `push` to `main` only. **A push to this branch starts no job at all**: of every run this forge has recorded whose workflow is `ci.yml`, each one is either a `pull_request` or a push to `main`. (Runs 19–26 did push-trigger on `ci-probe` and `img-probe` branches, but those came from `probe.yml` and `imgprobe.yml`, throwaway workflow files deleted long since — they are not counter-examples to `ci.yml`'s triggers.) So the PR has to exist before anything can be observed, and it is opened here rather than at Task 7.

```bash
git add crates/taiyaku-core/src/sidecar.rs
git commit -m "temporary: a test that never terminates, to trigger the CI bound"
git push -u origin o39-test-timeout
```

Then open the PR with `mcp__gitea__pull_request_write`, `method: "create"`, `owner: "davey"`, `repo: "taiyaku"`, `base: "main"`, `head: "o39-test-timeout"`, title `WIP: bound a test's runtime (O39)`, and this provisional body — one line per paragraph, no hard wrapping:

```
Opened early and deliberately red: this branch's first CI run carries a test that never terminates, because `ci.yml` only triggers on `pull_request` and there is no other way to exercise the new per-test bound on this runner.

The body will be rewritten once that run is captured and the test is reverted. Do not merge until it is.
```

**Task 7 rewrites this body and title in place** — it does not create a second PR.

- [ ] **Step 3: Watch the `test` job fail, and capture it**

Poll the forge until the `test` job for this push reports `failure`:

```bash
TOKEN=$(python3 -c "import json;print(json.load(open('$HOME/.config/.claude/.claude.json'))['mcpServers']['gitea']['env']['GITEA_ACCESS_TOKEN'])")
curl -s -H "Authorization: token $TOKEN" \
  "https://forge.daveynet.xyz/api/v1/repos/davey/taiyaku/actions/tasks?limit=8" \
  | python3 -c "
import json,sys,datetime
d=json.load(sys.stdin)
def p(t): return datetime.datetime.fromisoformat(t)
for j in d['workflow_runs'][:8]:
    dur=(p(j['updated_at'])-p(j['created_at'])).total_seconds()
    print(f\"{j['run_number']:4} {j['name']:6} {j['status']:9} {dur:5.0f}s  {j['head_branch']}\")
"
```

Then read the failing job's log and capture the nextest failure block verbatim — the `TIMEOUT [ 60.000s] taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound` line, the summary line, and the job's wall clock. Use the `mcp__gitea__actions_run_read` tool with `method: "download_job_log"` or `get_job_log_preview`.

**Record three things:** the run URL, the verbatim `TIMEOUT` line, and the job duration. If the job instead dies at ten minutes with a `timeout-minutes` message, that means the per-test bound did **not** apply in CI — a finding that stops this task and sends you back to Task 2 to work out why `.config/nextest.toml` was not read.

- [ ] **Step 4: Remove the hanging test**

```bash
git revert --no-edit HEAD
grep -rn "tmp_hangs" crates/ ; echo "hits above: none expected"
```

`git revert` rather than a hand edit, so the branch history shows the deliberate red run rather than hiding it. The squash flattens it to nothing on `main`.

- [ ] **Step 5: Push, expecting green**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | tail -6
git push origin o39-test-timeout
```

Poll the same way. Expected: all four jobs `success`. **Record the run URL and the four job durations** — the `test` job now carries a nextest run plus one more tool download, and the design predicts it stays well under the 10-minute bound. If it moved materially past the 134 s historical worst, say so.

---

### Task 5: Record the measurements in `docs/notes.md`

**Files:**
- Modify: `docs/notes.md` (append at end)

**Interfaces:**
- Consumes: the figures captured in Tasks 1–4.
- Produces: the record Task 6's `status.md` claims trace to.

`status.md`'s preamble promises every claim there traces to a measurement recorded in `notes.md`. Task 6 asserts figures; this is where they live.

- [ ] **Step 1: Append the section**

Append to `docs/notes.md`, following the file's flat `## `-heading style:

```markdown
## O39: what the slowest test actually is, and the two bounds sized from it

Nothing bounded a test's runtime anywhere in this repository until this branch.
No `.config/nextest.toml`, no `timeout-minutes` in `.forgejo/workflows/ci.yml`,
and CI does not run nextest — the `test` job runs `cargo llvm-cov`, which wraps
`cargo test`, which has no timeout. The auto-timeout that made a hang legible
existed only inside `cargo-mutants`, which is how the gap was found: it reports
a mutant that stops a loop terminating as `TIMEOUT` rather than `MISSED`.

**The slowest test was bisected, not estimated.** Running the whole workspace
under a temporary `slow-timeout` and raising the period:

| period | tests marked SLOW |
| ------ | ----------------- |
| 250 ms | 4 |
| 500 ms | 2 |
| 750 ms | 0 |

The two at 500 ms are `detect::tests::the_gap_does_not_grow_with_the_page` and
`cover::tests::a_cover_is_a_bounded_jpeg_whatever_the_page_was`. Nothing in 639
tests exceeds 0.75 s, so the 30 s warn is 40x the slowest test that exists and
the 60 s kill is 80x.

**A caution for anyone re-running this.** An early count of the exempt set said
"3" and was wrong — nextest colours its output, so the literal bytes `corpus::`
do not appear on every line it prints, and `grep -c 'corpus::'` undercounts.
`--color never` is required. With it, `test(corpus::)` selects the same sixteen
tests `cargo test corpus::` does.

**Job durations, from the forge's own `/actions/tasks` API**, over the ten runs
before this branch:

| job | min | max |
| --- | --- | --- |
| `test` | 32 s | 134 s |
| `lint` | 21 s | 85 s |
| `wasm` | 33 s | 84 s |
| `web` | 9 s | 25 s |

`timeout-minutes: 10` is 4.5x the worst of those.

**Switching the runner to nextest was measured, not assumed — and the first
measurement was wrong.** llvm-cov runs each nextest test in its own process
where `cargo test` threads them in one, which can move coverage. A single pair
of runs suggested nextest *improved* coverage by two lines. Repeated three times
each on an unchanged tree, with the same `--ignore-filename-regex`, that turns
out to be noise:

| | `cargo llvm-cov` | `cargo llvm-cov nextest` |
| --- | --- | --- |
| uncovered lines, three runs | 399, 399, 399 | 402, 397, 399 |
| lines | 97.69% every run | 97.68%–97.71% |
| per-file variance | none | `taiyaku-serve/src/lib.rs` 44/42/44, `routes.rs` 55/52/52 |
| `--fail-under-lines 93` | pass | pass |
| `scripts/coverage-floor.mjs` | pass, 38 files | pass, 38 files |
| wall clock, warm | 1501 ms | 915 ms |

**Coverage is unchanged; what nextest adds is nondeterminism.** `cargo test` is
deterministic to the line. nextest is not, and both varying files are in the
async server crate — process-per-test isolation changes tokio scheduling, which
is the same property that makes the per-test timeout possible.

**Neither gate is at risk, and that was checked rather than assumed.** The
aggregate floor is 93 against 97.68–97.71: 4.7 points of margin against a
0.03-point swing. The per-file floor is 57% lines, and the two varying files sit
at 90.74% and 97.62% — a three-line swing moves the worse of them by about 0.6
points, leaving 33 points of headroom.

This is a cost, not a free win: the branch trades a deterministic coverage
number for a bounded test runtime. Cheap at these margins, but anyone later
tightening `--fail-under-lines` toward the measured figure needs to know the
last digit moves on its own.

The workspace has **zero doctests**, which nextest cannot run at all — free
today, and a constraint on whoever adds the first one: they must add a
`cargo test --doc` step beside it or CI will not run it.
```

Then append the CI trigger evidence captured in Task 4 — the two run URLs, the verbatim `TIMEOUT` line, and the job durations from both runs — under a short paragraph saying which run was the deliberate red one.

- [ ] **Step 2: Commit**

```bash
git add docs/notes.md
git commit -m "notes: the measurements behind the two timeout bounds"
```

---

### Task 6: Close O39 in `docs/status.md`

**Files:**
- Modify: `docs/status.md` (five sites plus a sweep)

**Interfaces:**
- Consumes: Task 5's `notes.md` section, which the prose points at.
- Produces: a document that does not contradict itself.

**Closing an O-number is a five-part edit, not a tick.** Miss any part and the document contradicts itself.

- [ ] **Step 1 — part 1: the table row's checkbox**

At `docs/status.md`, the row beginning `| O39 |`: change `[ ]` to `[x]` and append the closure annotation to the description cell, in the style the other closed rows use — state how it actually closed (e.g. `— *closed on the branch that found it (SHA): …*` if the closing branch is in fact the finding branch, checked rather than assumed; otherwise say so, e.g. `— *closed by (SHA): …*`).

- [ ] **Step 2 — part 2: the prose bullet**

Rewrite the `- **O39 — nothing bounds a test's runtime, so a hang is not a failure. Open.**` bullet to say what was done and what remains. It must state that the `timeout-minutes` half is untriggered rather than untested, and why — otherwise the document claims more verification than happened.

- [ ] **Step 3 — part 3: the test count, which appears twice**

Line ~78 reads `Five crates, 635 tests, plus 1,195 in web's TypeScript`, and line ~94 reads `Thirteen of the 635 are standing checks gated on`.

**Measure, do not increment.** The figure is already stale — the workspace measures **639** today:

```bash
env -u TAIYAKU_CORPUS cargo test --workspace --all-features 2>&1 \
  | grep -E '^test result: ok' | awk '{s+=$4} END {print s}'
```

Correct both occurrences to whatever that prints. Note in the task report that the pre-existing figure was stale by 4, so the drift is recorded rather than silently absorbed.

- [ ] **Step 4 — part 4: the closed count**

Line ~815 reads:

> **Twenty-nine are now closed — O1, O2, O3, O4, O5, O7, O8, O10, O12, O13, O14, O15, O16, O17, O18, O19, O20, O21, O22, O25, O26, O27, O28, O29, O30, O35, O36, O37 and O38 — leaving ten.**

Becomes **Thirty**, with `O38 and O39` in the list, **leaving nine**. Verify against the table itself:

```bash
echo "rows: $(grep -c '^| O[0-9]' docs/status.md)  closed: $(grep -c '^| O[0-9]* | \[x\]' docs/status.md)"
```

Expected after the edit: `rows: 39  closed: 30`.

- [ ] **Step 5 — part 5: the "What remains" sentence, and the paragraph under it**

Line ~2865 reads `What remains: the ten open items tabled above — seven of them found by a whole-branch review, now that O1 (…) … are all out of the count.` and the paragraph after it opens `Three are the exceptions: O23, …; O34, …; and O39, turned up on the mutation branch beside its survivors …`.

Two edits, and they must agree:

- `the ten open items` → `the nine open items`. **The count word wraps across a line break** — it renders as `the ten open\nitems tabled above` — so grepping for `ten open items` finds nothing and reads as "already updated". Grep the sentence opener `What remains:` and take the next line.
- `seven of them found by a whole-branch review` **stays seven**: nine open = seven from whole-branch reviews plus two exceptions. `Three are the exceptions` becomes `Two are the exceptions`, O39's clause moves out of that sentence, and O39 joins the `are all out of the count` list above with a parenthetical saying how it closed.

- [ ] **Step 6: The sweep the five parts do not cover**

The five parts are necessary, not sufficient — prose outside the table describes open items in wording no grep predicts.

```bash
grep -rn "O39" docs/ web/src/ 2>/dev/null
```

Read **every** hit, not only the ones matching a phrase you expected. Known hits at time of writing: `docs/status.md` lines ~711, ~869, ~2255, ~2323, ~2877, ~2902, ~2904, and `docs/notes.md` lines ~8110, ~8347, ~8671.

Historical narrative sections are **kept as written and marked** (`*O39 has since closed — see its entry above.*`) rather than rewritten. Note that line ~2904 already carries `*O38 has since closed — see its entry above. O39 stays open.*` — that second sentence is now false and must be corrected.

- [ ] **Step 7: Re-read the document against itself**

```bash
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -nE '[0-9]{3} tests|Thirteen of the' docs/status.md
grep -n -A1 'What remains:' docs/status.md
```

Confirm: 39 rows, 30 closed, both test counts identical and matching the measured figure, and the "What remains" count reading nine.

- [ ] **Step 8: Commit**

```bash
git add docs/status.md
git commit -m "status: close O39"
```

---

### Task 7: Open the PR

**Files:** none.

**Interfaces:**
- Consumes: everything above, and the two run URLs from Task 4.

- [ ] **Step 1: Final gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh 2>&1 | tail -8
git log --oneline origin/main..HEAD
```

Expected: green, and a history of **nine** commits — spec, plan, nextest config, gate files, job bound, the temporary hang, its revert, notes, status. The hang and its revert are both present deliberately: the branch history records that the red run happened rather than hiding it, and the squash flattens the pair to nothing on `main`.

- [ ] **Step 2: Write the body to a file and check it renders**

Write the PR body to `$CLAUDE_JOB_DIR/tmp/o39-body.md`, **one line per paragraph and per list item** — no hard wrapping. It must contain, at minimum: what the two bounds are and that they answer different questions; every number with what it was sized against; the exempt set and the `geometry_experiment` 40-minute evidence; the coverage comparison table; the verbatim CI `TIMEOUT` output with its run URL, and the green run URL; and an explicit statement that `timeout-minutes` is untriggered rather than untested.

Then verify by rendering, not by reading the source:

```bash
TOKEN=$(python3 -c "import json;print(json.load(open('$HOME/.config/.claude/.claude.json'))['mcpServers']['gitea']['env']['GITEA_ACCESS_TOKEN'])")
curl -s -H "Authorization: token $TOKEN" -H 'Content-Type: application/json' \
  -d "$(python3 -c 'import json,sys;json.dump({"text":open(sys.argv[1]).read(),"mode":"comment","context":"davey/taiyaku"},sys.stdout)' "$CLAUDE_JOB_DIR/tmp/o39-body.md")" \
  https://forge.daveynet.xyz/api/v1/markdown | grep -c '<br'
```

Expected: `0`. Any other number means the body is hard-wrapped.

- [ ] **Step 3: Rewrite the PR opened in Task 4**

The PR already exists — Task 4 had to open it before CI would run at all. **Update it in place; do not create a second one.**

Use `mcp__gitea__pull_request_write` with `method: "update"`, `owner: "davey"`, `repo: "taiyaku"`, `pull_number: <the number Task 4 reported>`, the verified body, and a title naming both bounds and the closed item — dropping the `WIP:` prefix Task 4 set.

**Do not merge.** Report the PR URL and stop.

---

## Self-Review

**Spec coverage.** §1 two bounds → Tasks 1 and 3. §2 the three files → Tasks 1, 2, 3. §3 the numbers → Task 1 Step 1 comment, Task 3 Step 2 comment, Task 5. §4 exempt set → Task 1 Steps 1, 2, 5. §5 coverage → Task 2 Step 5, Task 5. §6 what is not bounded → Task 3 Step 2's comment and Task 7's body requirement. §7 verification → Task 4. §8 closing O39 → Task 6. §9 risks → Task 4 Step 3's stop condition covers the "nextest config not read in CI" risk; the false-kill risk is carried by Task 4 Step 5's instruction to report a materially moved duration.

**Placeholder scan.** No TBD/TODO. Every figure was measured on 2026-08-11, including the exempt-set count in Task 1 Step 2 (30 = 16 corpus + 14 experiment) and the nextest digest. The 14 is the one figure nothing else pins, which is why that step distinguishes a moved experiment count (report it) from a wrong corpus count (fix the filter).

**Type consistency.** Three temporary test names, each used consistently: `tmp_hangs_outside_the_exempt_set` and `tmp_hangs_inside_the_exempt_set` in Task 1 (both removed in Step 7, both matched by the `tmp_hangs` grep), and `tmp_hangs_to_trigger_the_ci_bound` in Task 4. The `extract()` shell function is defined identically where used.
