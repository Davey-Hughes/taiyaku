#!/usr/bin/env bash
# Mutation testing: does this suite's *assertions* see a defect, or only run the line?
#
#   scripts/mutants.sh                       # the whole workspace — 3 h 12 m, measured 2026-08-19
#   scripts/mutants.sh --since main          # only code this branch touched — the usable one
#   scripts/mutants.sh --crate taiyaku-serve # one crate
#   scripts/mutants.sh --file 'crates/**/auth.rs'   # one file, repeatable
#   scripts/mutants.sh --baseline            # rewrite docs/mutants-baseline.md from this run
#
# **This is not a gate and must not become one.** `check-all.sh` mirrors CI and
# is meant to be runnable before every push; the workspace sweep is **3,191
# mutants at 3 h 12 m** (2026-08-19), three orders of magnitude past that
# budget. It runs weekly in `.forgejo/workflows/mutants.yml`, gating nothing —
# one job per crate, because a three-hour job is a bound nobody can set
# honestly and a failure in it names nothing.
#
# The figure was 1,660 mutants at 43 minutes until 2026-08-19, and that was
# August's, taken before spec B's scanner and spec A2's auth nearly doubled the
# workspace. Carrying it forward set the workflow's timeout at 1.35x the real
# cost while its comment claimed 6x. **Re-measure rather than quote this line.**
#
# --- What the baseline is for -------------------------------------------------
#
# A survivor list is only actionable if you can tell a new one from an old one.
# 45 mutants in `detect.rs` and `regions.rs` are known survivors deferred until
# L2 redesigns the detector, and a run that reports them every time trains you to
# skim the output — which is the same failure as a CI job that is red by default.
#
# So `docs/mutants-baseline.md` records the survivors that are known and
# accepted, each with the reason, and this script diffs against it. **A survivor
# not in the baseline is the entire output worth reading.** That also makes the
# harness a ratchet rather than a snapshot: the thing four whole-branch reviews
# could not do was remember what had already been checked, and a committed
# baseline is that memory.
#
# The diff is on the mutant's own `file:line:col: description` line, so moving a
# function invalidates its baseline entry and the survivor reappears. That is
# deliberate and it is the annoying half: a line that moved is a line that
# changed, and the alternative — matching on description alone — would carry an
# accepted survivor across a rewrite that made it a real defect.
#
# --- Traps, all of them measured ---------------------------------------------
#
# **`--gitignore=true` is not optional here.** cargo-mutants builds each mutant
# in a copy of this tree, and this tree has 28 GB of gitignored `corpus/` in it.
# Without the flag every build directory copies all of it and the run dies with
# `No space left on device` — an error naming neither the corpus nor `.gitignore`.
# It also cannot be combined with `--copy-target`; the two are mutually exclusive.
#
# **The scratch directory goes on disk, not in `/tmp`.** cargo-mutants builds
# each mutant in a copy of the tree, under `TMPDIR`, and every copy carries its
# own `target/`. On this machine `/tmp` is a 31 GB tmpfs — RAM — and a
# five-file run filled it and died at mutant 140 of 255 with `No space left on
# device`, which is the same error the corpus produces and a different cause.
# `$PWD/target/mutants-scratch` is on the 3.6 TB disk and is gitignored with the
# rest of `target/`. Overridable: set `TAIYAKU_MUTANTS_TMP` if that disk is the
# small one on some other host.
#
# **`TAIYAKU_CORPUS` is unset for the run.** With it set, the standing corpus
# checks execute inside every one of hundreds of mutant builds. See `check-all.sh`,
# which learned the same lesson the expensive way.
#
# **A non-terminating mutant is reported TIMEOUT, not MISSED** — `left += 1`
# becoming `left *= 1` never finishes. Those are not survivors and the summary
# counts them separately. `.config/nextest.toml` bounds a single test at 60 s,
# which is what keeps a timeout from being a hang.
#
# **A time bound is not a memory bound, and that is what `ulimit -v` below is
# for.** A mutant that removes an allocation guard does not spin — it allocates.
# Measured 2026-08-19: two sweeps were killed by the kernel's OOM killer, one
# test binary at **36.6 GB** anon-rss and another at **43.8 GB**, on a machine
# with 60 GB. Neither came near the 60 s timeout; forty gigabytes takes seconds.
# The kernel then picks a victim by score, so what dies may be the desktop
# session rather than the runaway — the first of those two kills was invoked by
# `kwin_wayland`, which was simply the process that asked for memory next.
#
# `ulimit -v` is per-process and inherited by every child, so each test binary
# gets its own ceiling and a runaway hits an allocation failure — the test
# aborts, the suite fails, and cargo-mutants records the mutant as caught, which
# is the right answer: a mutation that makes the code allocate forty gigabytes
# has changed observable behaviour and the suite did notice.
#
# **A cgroup would have been the wrong tool**, though `systemd-run --user
# --scope -p MemoryMax=` was tried first: a ceiling on the whole run lets one
# runaway consume the budget and take innocent concurrent mutants down with it,
# which reports them as caught when nothing tested them. False *caught* is the
# same failure as false green.
#
# 16 GB verified both ways on 2026-08-19: `cargo build --workspace
# --all-targets` completes under it, and a 4 GB allocation under a 2 GB cap
# fails with an allocation error rather than an OOM kill. Overridable with
# `TAIYAKU_MUTANTS_VM_KB` for a host where rustc needs more address space.
#
# **`const { assert!(...) }` makes a mutant unviable rather than caught**, so
# clippy's suggested fix for `assertions_on_constants` silently improves the
# score without improving anything. Unviable is not a win; the summary prints it.
#
# **cargo-mutants does not mutate every expression.** It generates no mutant at
# all for some `match` arms — O38's defect was invisible to it before and after
# the fix. "0 missed" on a file is not "this file is verified", and this script
# cannot say otherwise.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BASELINE_FILE="docs/mutants-baseline.md"
# `--output DIR` means "put a `mutants.out` directory inside DIR", so this is
# `target` and the results land in `target/mutants.out`. Written as
# `target/mutants.out` first, which produced `target/mutants.out/mutants.out`
# and an empty survivor list the diff below read as "nothing survived" — a
# harness reporting a clean run because it was looking in the wrong place is the
# one failure it must not have.
OUT_PARENT="target"
OUT_DIR="$OUT_PARENT/mutants.out"

mode=workspace
since_ref=""
crate=""
files=()
write_baseline=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --since)
      shift
      [ "$#" -gt 0 ] || { echo "error: --since needs a ref (e.g. --since main)" >&2; exit 2; }
      mode=since; since_ref="$1"
      ;;
    --crate)
      shift
      [ "$#" -gt 0 ] || { echo "error: --crate needs a package name" >&2; exit 2; }
      mode=crate; crate="$1"
      ;;
    --file)
      shift
      [ "$#" -gt 0 ] || { echo "error: --file needs a glob" >&2; exit 2; }
      mode=files; files+=("$1")
      ;;
    --baseline) write_baseline=1 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

# See the TMPDIR note above. Created before the run rather than relied on,
# because cargo-mutants reports a missing TMPDIR as a build failure per mutant
# rather than as one error at the top.
scratch="${TAIYAKU_MUTANTS_TMP:-$PWD/target/mutants-scratch}"
mkdir -p "$scratch"
export TMPDIR="$scratch"

args=(--gitignore=true --output "$OUT_PARENT")

case "$mode" in
  since)
    # `--in-diff` wants a diff file rather than a ref. Three dots so the diff is
    # against the merge base: `main..HEAD` would also show what main gained
    # since this branch forked, and mutating code this branch never touched is
    # the opposite of what --since is for.
    diff_file="$(mktemp)"
    trap 'rm -f "$diff_file"' EXIT
    git diff "${since_ref}...HEAD" > "$diff_file"
    if [ ! -s "$diff_file" ]; then
      echo "nothing changed against ${since_ref} — no mutants to run"
      exit 0
    fi
    args+=(--in-diff "$diff_file")
    echo "==> mutants in code changed since ${since_ref}"
    ;;
  crate)
    args+=(--package "$crate")
    echo "==> mutants in ${crate}"
    ;;
  files)
    for f in "${files[@]}"; do args+=(--file "$f"); done
    echo "==> mutants in ${files[*]}"
    ;;
  workspace)
    echo "==> mutants across the workspace — 3,191 mutants, 3 h 12 m on 2026-08-19"
    ;;
esac

# See the `ulimit -v` note at the top. Set here rather than exported, so it
# bounds cargo-mutants and every rustc and test binary below it. `|| true`
# because a host that does not enforce RLIMIT_AS should still run the sweep —
# it says so rather than failing, since the alternative is refusing to measure
# anything on a machine that may not have the problem.
vm_kb="${TAIYAKU_MUTANTS_VM_KB:-16000000}"
if ! ulimit -v "$vm_kb" 2>/dev/null; then
  echo "warning: could not set a per-process address-space limit of ${vm_kb} KB."
  echo "         A mutant that removes an allocation guard can then take the machine"
  echo "         out through the OOM killer rather than failing its own test."
fi

# `|| true` because cargo-mutants exits non-zero when mutants survive, and a
# survivor is the thing this script exists to report rather than an error in
# running it. The exit status is decided below, against the baseline.
set +e
env -u TAIYAKU_CORPUS cargo mutants "${args[@]}"
mutants_status=$?
set -e

if [ ! -f "$OUT_DIR/missed.txt" ] && [ "$mutants_status" -gt 1 ]; then
  echo "error: cargo mutants failed before it could report (exit ${mutants_status})" >&2
  exit "$mutants_status"
fi

# **Absent is not empty.** This used to create the file when it was missing,
# which meant a run that crashed, or one whose output landed somewhere else,
# was read as "nothing survived" and reported a clean sweep. A harness that
# says green because it could not find the answer is worse than no harness.
missed_file="$OUT_DIR/missed.txt"
if [ ! -f "$missed_file" ]; then
  echo "error: ${missed_file} does not exist, so this run produced no survivor list." >&2
  echo "       cargo mutants exited ${mutants_status}. Its log is under ${OUT_DIR}/." >&2
  exit 1
fi

if [ "$write_baseline" = 1 ]; then
  echo
  echo "==> rewriting ${BASELINE_FILE} from this run"
  node scripts/mutants-baseline.mjs --write "$missed_file" "$BASELINE_FILE" "$mode"
  exit 0
fi

echo
node scripts/mutants-baseline.mjs "$missed_file" "$BASELINE_FILE"
