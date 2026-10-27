# A local build names its commit: design

2026-10-26. The `docs/status.md` Later entry "Next after PR 3: a local build
names its commit" asks for it; this spec is how.

## 1. What changes for the reader

A build made without `TAIYAKU_COMMIT` says "unknown build" in the version
label and on About today. After this, it names git's HEAD there, and marks a
build whose tracked files differed from HEAD when it was built:

| Build | Label | Label's name | About's Commit row |
| --- | --- | --- | --- |
| `TAIYAKU_COMMIT` a full hash | `abc1234` | About, build abc1234 | `abc1234`, linked |
| HEAD, tree clean | `abc1234` | About, build abc1234 | `abc1234`, linked |
| HEAD, tracked files changed | `abc1234*` | About, build abc1234, with uncommitted changes | `abc1234`, linked, then " with uncommitted changes" |
| neither | `unknown build` | About, unknown build | `unknown build`, plain text |

The link goes to `<repository>/commit/<hash>` as now; for a changed tree it
names the commit the changes were made on.

## 2. The user's rulings (2026-10-25 and 2026-10-26)

1. **The order:** `TAIYAKU_COMMIT` when it is a full hash, then HEAD read by a
   build script in `taiyaku-serve`, then "unknown build" (the status.md entry).
2. **The mark:** `*` where tracked files differ from HEAD, as
   `git describe --dirty` marks them; untracked files do not count.
3. **When it is worked out again:** when git's state changes (a commit, a
   checkout, a stage) or a Rust input changes (§3.3), not on every build and
   not on git's state alone.
4. **How it reads:** the star on the label, with a name that says it in words;
   the words on About (the table in §1).

## 3. The build script, `crates/taiyaku-serve/build.rs`

It gathers facts and decides nothing about which source wins; that is §4's,
where a unit test reaches it.

### 3.1 What it asks git

- It runs `git -C <workspace root>`, the root being `CARGO_MANIFEST_DIR`'s
  grandparent, with `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and
  `GIT_COMMON_DIR` removed from the child's environment (a hook's environment
  would point git at another index or repository) and `GIT_OPTIONAL_LOCKS=0`.
- **Is this workspace a repository:** `git rev-parse --show-toplevel` must be
  the workspace root (both canonicalised). Otherwise there are no facts and
  nothing of git's is watched: a source tree unpacked inside some other
  repository must not report that repository's HEAD, nor rerun on its state.
- **HEAD:** `git rev-parse HEAD`, emitted as
  `cargo:rustc-env=TAIYAKU_GIT_HEAD=<hash>`.
- **Changed:** `git status --porcelain --untracked-files=no`; any output emits
  `cargo:rustc-env=TAIYAKU_GIT_DIRTY=1`.
- **Any failure is no facts.** A missing `git`, an unborn HEAD, a refused
  repository (git's "dubious ownership"), or a status that fails: no
  `TAIYAKU_GIT_*` line is emitted, so the build reports no commit. Where the
  workspace is still found to be the repository's top (an unborn HEAD, a
  status that fails), §3.3's git paths are watched all the same, so a first
  commit, or the next change to them, runs the script again; where
  `--show-toplevel` itself fails (a missing `git`, a refused repository,
  no repository yet),
  they are not (§3.4). A commit reported without its mark
  would be a wrong label, and no label is better than a wrong one (the reason
  `full_hash` gives).

### 3.2 It never writes to what it watches

`git status` refreshes the index as a side effect unless
`GIT_OPTIONAL_LOCKS=0` stops it. The index is one of the watched paths, so a
script that wrote it would mark itself stale and run again on the next build.
The script runs only read-only commands.

### 3.3 What it watches

Each through `cargo:rerun-if-changed`, and **each only where the path
exists**: cargo treats a missing path as changed, which would rerun the script
on every build (a branch whose ref is only in `packed-refs` has no loose ref
file, for one).

- Git's state, watched whenever `--show-toplevel` is the workspace root,
  whether or not HEAD and status gave facts: `<git dir>/HEAD`,
  `<git dir>/index` and `<git dir>/logs/HEAD`; when HEAD is a branch,
  `<common dir>/<its ref>`, or where that ref has no file, the nearest
  directory under `<common dir>/refs/heads` that will hold it;
  `<common dir>/packed-refs`; and `reftable/tables.list` in the git dir and in
  the common dir. The git dir and the common dir come from
  `git rev-parse --git-dir` and `--git-common-dir`, so a git worktree, whose
  `.git` is a file, is read where its HEAD, index and reflog are.
- **A branch with no ref file.** A branch with no commit yet, or whose ref is
  only in `packed-refs` (`git gc` packs every branch ref, so any branch can be
  in that state), has no loose ref file. Its next commit writes one and need
  not write the index (`commit --amend -m`, `commit --allow-empty`,
  `reset --soft`, a first commit). Cargo scans a watched directory whole, so
  the directory that will hold the ref sees that commit, and a commit on any
  branch under it, a sibling worktree's included, reruns the script too.
  `logs/HEAD`, to which every move of HEAD appends, sees the same commits
  where reflogs are on, but a first commit has no reflog before it. Never
  `refs/` or the git dir whole: a fetch would rerun the script, and every
  build would rescan the objects.
- **`reftable`.** In git's `reftable` ref format `.git/HEAD` is a fixed stub,
  and there are no loose refs, no `packed-refs` and no `logs/`: every ref and
  reflog update rewrites `tables.list`, the worktree's for HEAD and the shared
  one for branches.
- The Rust inputs: the workspace's `Cargo.toml` and `Cargo.lock`, and each
  `crates/*/Cargo.toml` and `crates/*/src`. These are watched where there is no
  repository too.
- **Not `crates/` whole.** Test runs write `crates/taiyaku-cli/.taiyaku-cache`
  and a wasm build writes `crates/taiyaku-wasm/pkg`, both ignored by git; under
  a watch on `crates/` either would rerun the script and recompile
  `taiyaku-serve`.

### 3.4 What it misses, by the ruling

The mark is right as of the script's last run. An unstaged edit outside §3.3's
inputs (under `web/`, `docs/`, a crate's `tests/` or `fixtures/`) made after
that run carries no `*` until the script's next run, on a change to one of
§3.3's paths. An edit undone after a run keeps its `*` until the same.

Where `--show-toplevel` fails (a missing `git`, a repository git refuses, no
repository yet) or names another directory, none of git's state is watched.
Once git can answer (git installed, the repository marked safe or made), the
label reads "unknown build"
until a Rust input changes.

Each run recompiles `taiyaku-serve` and what depends on it; the plan measures
what a run and a build with nothing changed cost.

## 4. The server, `crates/taiyaku-serve/src/about.rs`

- `build_label` is a function of the three values, read through
  `option_env!`: `TAIYAKU_COMMIT`, `TAIYAKU_GIT_HEAD`, `TAIYAKU_GIT_DIRTY`. It
  answers the commit and whether the tree had changed:
  - a valid `TAIYAKU_COMMIT`: that commit, not changed;
  - otherwise a valid `TAIYAKU_GIT_HEAD`: that commit, changed when
    `TAIYAKU_GIT_DIRTY` is set;
  - otherwise no commit, not changed.

  Valid is what it is now: a full 40-character lowercase hex hash, for both.
  An invalid `TAIYAKU_COMMIT` falls through to HEAD.
- `GET /api/about` adds `"dirty"`, a boolean beside `"commit"`, true only with
  a commit.
- Cargo goes on rebuilding the crate when `TAIYAKU_COMMIT` changes, because
  rustc records an `option_env!` read; the build script does not read it.

## 5. The web

- The about answer's type gains `dirty: boolean`.
- `VersionLabel.tsx` draws `<short>*` and names the link "About, build
  <short>, with uncommitted changes" when `dirty`.
- `About.tsx`'s Commit row follows the commit's link with " with uncommitted
  changes" when `dirty`.
- **The sidebar's foot reads the label, then More, then Theme** (the user's
  ruling of 2026-10-27, after looking at the two builds): a browser draws a
  hovered link's address over the window's bottom-left corner, where it
  covered the label, so Theme, which holds no link, takes that corner, and
  the label and More stand above it. `Shell.tsx` draws them in that order,
  so Tab reaches them in it too.

## 6. The docs

Each says what is true after this:

- `about.rs`: its doc says a build script runs git, and that the image's build
  context holds no `.git`, so there the argument is the only source.
- `Dockerfile` (the comment above `ARG TAIYAKU_COMMIT`): the build script finds
  no `.git` in the context.
- `README.md` (the build label's section): a local `cargo build` names HEAD
  itself; `TAIYAKU_COMMIT` still wins, and an image build still needs it.
- `.forgejo/workflows/ci.yml` (the `test` job's comment) and `docs/ci.md`
  (under the jobs table, and "The image job"): unset, the route test compares
  with git's HEAD rather than `null`; the image's build runs no git that finds
  a repository.
- `docs/status.md`: the Later entry becomes what shipped, in the About entry.

## 7. Checks

- **Unit:** the function in §4 over each branch: a valid `TAIYAKU_COMMIT` wins
  over HEAD and is never marked; an invalid one falls through; HEAD with and
  without the dirty fact; neither; an invalid HEAD.
- **Route:** the route test as now, with `dirty` read from the same function.
- **Web:** the label's text and name, and About's row, each with `dirty` true
  and false.
- **The build script, by hand, recorded in the PR body**, each run as the
  compiled `build-script-build` binary with its environment set where a full
  build is not needed:
  1. A second `cargo build` with nothing changed leaves the script fresh, and
     `.git/index`'s mtime is unchanged by the first.
  2. An unstaged edit under `crates/*/src` gives the `*`; a commit takes it away.
  3. In a git worktree, HEAD and the mark are the worktree's.
  4. The workspace copied without `.git` into another repository: no commit.
  5. `TAIYAKU_COMMIT` set to a full hash: that commit, no `*`, on a changed
     tree.
  6. A watched mutation: drop `GIT_OPTIONAL_LOCKS=0`, touch a tracked file
     without changing it (so the index's stat data is stale and `git status`
     has something to refresh), and see check 1 fail; or record that git did
     not write the index here and why.
  7. A commit that writes no index on a branch whose ref is only in
     `packed-refs` (`reset --soft`, `commit --amend -m`, `--allow-empty`)
     reruns the script, on HEAD's reflog or on the directory that will hold
     the branch's ref, and the build after it is
     fresh; a watched mutation drops both watches and sees the label go stale
     (either one alone still sees it). The same in a reftable repository
     rerun it on `tables.list`.
  8. A first commit after `git init` (`git archive` unpacked, HEAD unborn)
     reruns the script on the directory that will hold the branch's ref, the
     build names that commit, and the build after it is fresh; a watched
     mutation drops that watch and the label stays "unknown build". A
     repository git refuses, and a tree inside another repository, watch none
     of git's state.
- **Mutation harness:** whether it mutates `build.rs` is checked, not
  assumed, and if it does not, the hand checks are that file's only test,
  which the PR body says.

## 8. Not in this

- The web bundle's own commit: the label is the server binary's, so a rebuild
  of the web alone does not change it (the status.md entry).
- Untracked files (ruling 2).
