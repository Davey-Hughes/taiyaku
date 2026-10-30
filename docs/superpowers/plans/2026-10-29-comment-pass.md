# Comment pass, the rest of the repository: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut every comment the pilot did not reach down to its why, in five PRs: `web/src`, `web/tests`, `taiyaku-library`, `taiyaku-serve`, then `taiyaku-cli` with the scripts and CI files.

**Architecture:** The pilot's method (`2026-10-29-comment-pass-pilot.md`, merged as #216) in the setup the user chose on 2026-10-29 (setup A): per chunk, a Sonnet implementer writes a decisions file and applies it with `apply.py`, an Opus reviewer reads every block blind, and one fix round goes back to the same implementer. Each PR then has one false-at-base commit and one pointer pass. The tools stay in `.superpowers/sdd/comment-pass/` (gitignored). The pilot's measurement is `.superpowers/sdd/comment-pass/p/result.md`, and its ledger is `.superpowers/sdd/comments-pilot-evidence/progress.md`.

**Tech Stack:** Python 3 (tools), Rust (rustfmt, clippy pedantic, rustdoc), TypeScript, TSX and CSS (biome, `tsc -b`), node (the comment guard), the Gitea MCP and REST API for PRs.

## The PRs

| PR | Branch | Scope | Guard area | Blocks / comment lines at aca6dc6e | Chunk prefix |
|---|---|---|---|---|---|
| 1 | `comments-web-src` | `web/src`, `web/vite.config.ts` | `web/src` | 3,094 / 22,189 | `ws` |
| 2 | `comments-web-tests` | `web/tests` | `web/tests` | 4,620 / 16,509 | `wt` |
| 3 | `comments-library` | `crates/taiyaku-library` | `crates/taiyaku-library` | 2,871 / 22,326 | `lib` |
| 4 | `comments-serve` | `crates/taiyaku-serve` | `crates/taiyaku-serve` | 2,488 / 16,038 | `srv` |
| 5 | `comments-cli-scripts` | `crates/taiyaku-cli`, `scripts`, `.forgejo`, root files, and whatever PR 1's inventory finds outside 1–4 | `crates/taiyaku-cli`, `scripts` | 1,436 + 193 / 8,580 + 2,174 | `cli` |

- The block counts come from `.superpowers/sdd/comment-pass/size/*-all.md` at aca6dc6e. Task 1 cuts each PR again at its own base.
- **Web goes first.** The pilot read only Rust, so PR 1 tests whether its rates hold for TypeScript (`result.md` §3, caveat 1) before the rest is committed to. The checkpoint after PR 1's sixth review is where that is decided.
- Each PR is cut from `main` after the one before it merges. It gets its own fresh ledger, `.superpowers/sdd/progress.md`, and its own notes file, `docs/superpowers/notes/<cut date>-code-history-<scope>.md`.
- **A PR may span sessions, but a session hands off only between chunks.** A chunk's implementation, review, fix and fix check finish in the session that started it, so that no fix round depends on an agent from an earlier session.
- **Cost.** The pilot projects setup A, all in, at about $0.065 per block. That covers implementing, review, the fix round, the controller and the pointer pass (`result.md` §3: $622 + $161 + $218 over 15,418 blocks). By PR that is about $200, $300, $190, $160 and $105. `taiyaku-library`'s blocks run longer than the pilot's: 7.8 comment lines a block, against 6.0. After each PR the ledger records its real cost beside its figure. **If PR 1 costs more than twice its figure, the user decides before PR 2.**

## Global Constraints

- **The comment rule (the user, 2026-10-17):** a comment holds the why: the reason, the invariant, what breaks if it changes. History (what the code used to do, how a defect was found, a struck-out earlier reason, a PR's story) and measurements come out of comments, moved to the docs or dropped. A comment may point at a notes section or an O-number; it does not retell it.
- **Trim level:** "drop history, keep why" (the user's example: `derive.rs`'s O210 block, 9 lines to 6).
- **Comment-only:** every commit on a PR's branch leaves `python3 .superpowers/sdd/comment-pass/check_comment_only.py <BASE>` at exit 0, where `<BASE>` is `main` when the branch was cut. The plan's own commit changes only `docs/`.
- **Directive comments are code**, kept byte-identical, and `check_comment_only.py` enforces it from Task 0 on: `biome-ignore`, `@ts-expect-error`, `@ts-ignore`, `@ts-nocheck`, `/// <reference …>`, `eslint-disable…`, `/*#__PURE__*/`, `@vite-ignore`, `@vitest-environment`, a shebang, `# shellcheck …`, a Dockerfile `# syntax=`, and anything else Task 0's grep finds.
- **Moves default to DROP.** A removed sentence is moved only when it passes `brief.md`'s (a)/(b)/(c) test. Moved text is verbatim: same case, punctuation and paragraph breaks. It goes to the PR's notes file, and `apply.py` writes the headings.
- **Kept intact:** fenced code in comments; the `# Errors`, `# Panics`, `# Safety` and `# Examples` sections; the first sentence of every public Rust item's doc; and the first sentence of every exported TypeScript item's JSDoc.
- **Replacement text** adds no fact the original did not state, reads as true of the code today, and backticks every identifier. It is rewrapped to the comment width around it: 80 columns for both rustfmt and biome (`web/biome.json` sets `lineWidth` to 80), with no line over 80 and no one-word orphan line. `check_moves.py`'s width check runs after every commit, fix rounds included.
- **The comment guard:** `node scripts/check-comments.mjs --base <BASE>` shows no rule-1 or rule-2 finding after every commit, so no added comment line carries a history marker and no block is new or grown past 8 lines with text. Rule 3's stale arm fails for the PR's own area until Task 7 lowers its budget, and that is the one expected failure.
- **False at base:** a comment found untrue of the code at `<BASE>` is listed per chunk and fixed in Task 5 of the same PR (the pilot's ruling (b), carried forward). Anything that needs a code change is listed for the user instead.
- **Code strings with history** (assert messages, `reason =` strings, test names) are listed as follow-ups and never changed (the pilot's ruling 2).
- **A code defect found while reading** is listed. The controller asks the user before it becomes an O-number (as O231 did).
- **Line cites:** a `path:line` cite that this PR's cuts shifted is rewritten to name the item, not a new line number (the pilot's ruling 4). Dated records (closed O-rows' narratives, dated specs, plans and notes, measurement logs) stay as written, as the pilot's Task 5 left them (`p/task5-pointers.md`).
- **`docs/mutants-baseline.md` keys** into a changed Rust file are re-checked after every commit that moves lines, including a rebase. In the pilot, 12 keys slipped past two checks.
- **Migration SQL comments (PR 3)** are in the pass, as `001_initial.sql` was in the pilot (it is `include_str!`'d, with no checksum), unless the user rules otherwise before PR 3.
- **Models:** implementers run `model: "sonnet"`. Every reviewer, Task 0, the brief review, Tasks 6 and 7's checks and the claim check run `model: "opus"`. A reviewer is never told which model wrote what it reviews.
- **One committing agent at a time** in the checkout. Reviews and fix checks are read-only and run alongside it. The controller sends a fix round only when no implementer is running.
- **The gate** is the repository's own `scripts/check-all.sh`, never a modified copy.
- **This machine:** the Bash tool is zsh with `noclobber` (start a log with `>|`), `grep` is a function (use `grep -a`), and `cd` is zoxide (use absolute paths and `env -C`). Cargo runs with `--jobs 8`, test threads 8. In a harness worktree, copy `web/node_modules` in and never symlink it.
- **Size:** a subagent task finishes in under ~150 calls. A chunk is at most 150 blocks and 1,300 comment lines (the pilot's largest chunk), unless one file alone is larger.
- **The controller stays thin:** it dispatches, reads the few-line replies, keeps the ledger and `<prefix>/tally.md`, and talks to the user.

## Files

- Per PR, committed: the notes file (written by `apply.py`); comments in the PR's scope; the PR's areas in `scripts/comment-budget.json`; re-keyed keys in `docs/mutants-baseline.md` (Rust PRs); the live docs whose cites the PR shifted; and the comments clean-up entry in `docs/status.md`'s Later list.
- Scratch, never committed: `.superpowers/sdd/comment-pass/`, which holds the tools, `check/`, `brief.md`, `review-brief.md`, and one `<prefix>/` per PR (`params.md`, chunks, decisions, logs, reports, `review/`, `mapping.md`, `tally.md`, `false-at-base.md`, `pointers.md`).

---

### Task 0: Tools and briefs for TypeScript (PR 1 only, Opus)

**Files:** `.superpowers/sdd/comment-pass/{worklist,check_comment_only,lexer}.py` and `check/*`; create `brief.md`, `review-brief.md` and `check/rekey.py`.

- [ ] **Step 1: Directives as code.** `git grep` the whole tree at `<BASE>` for directive comments (the Global Constraints' list, plus any `-ignore`, `-disable` or `-expect-error` form). Make `check_comment_only.py` put each directive's text into the code stream, so that changing one fails. Record the forms found in `brief.md`.
- [ ] **Step 2: `worklist.py --max-lines N`.** A chunk closes before a file that would take it past N comment lines, and a file over N alone is its own chunk. Prove it: in a detached scratch worktree at `aca6dc6e`, re-cut the pilot's three crates with `--all --max-blocks 150 --max-lines 1300`. The result must be the same six chunks as `p/chunks.tsv`.
- [ ] **Step 3: Lexer agreement.** For every file under `web/src` and `web/tests`, plus `web/vite.config.ts`, at `<BASE>`, compare the comment lines `lexer.py` finds with those the guard's lexer (`scripts/check-comments.mjs`) finds. Write the per-file differences to `ws/lexer-agreement.md`, and explain or fix every one. A TSX `{/* … */}`, a regex literal holding `//`, and a template string holding `/*` are the cases to look for.
- [ ] **Step 4: Prove each check on TypeScript and CSS, on a throwaway branch** cut from `comments-web-src`. Run a two-block trial in a `.tsx` file and another in a `.css` file: one replace with a move, one delete. Each check must pass on the clean trial and fail on its planted defect:
  - `check_comment_only.py`: a one-character code change, and an edited `biome-ignore` comment.
  - `check_moves.py`: a lowercased word in a moved paragraph, and an 81-column replacement line.
  - `partial_cuts.py`: a trimmed sentence inside a moved paragraph.
  - `id_mismatch.py`: a decisions section under the wrong block id.
  - The guard: an added comment line carrying a date.

  Delete the branch, then confirm `git status` on `comments-web-src` is clean.
- [ ] **Step 5: `check/rekey.py BASE [HEAD]`.** For each `path:line[:col]` key in `docs/mutants-baseline.md`, map its line through `git diff -U0 BASE HEAD`. Print each key whose line moved, with its new key, or `gone` if the line itself changed. In a second section, list every `path:line` cite in `docs/` and in code comments that points into a file changed since BASE, with the same mapping. Prove it on the pilot: take the baseline file at `af694710`, map it through `af694710..20f25cdb`, and compare with the baseline at `20f25cdb`. Its output must match the re-keys the pilot made. The one allowed difference is `format.rs`'s key, which was stale at base (O226).
- [ ] **Step 6: The briefs.** Build `brief.md` from `p/p-brief.md` and `review-brief.md` from `p/p-review-brief.md`. Drop the control run (§12) and the pilot's names. Each dispatch names its `<prefix>/params.md` (scope paths, BASE, notes file, chunk prefix, checks), and the brief reads it first. The per-chunk checks:
  - Every chunk: `decisions_check`, `id_mismatch`, `apply`, `check_comment_only.py <BASE>`, `check_moves.py` with width, `partial_cuts.py`, and the guard with `--base <BASE>`.
  - Rust: `cargo fmt -p … -- --check`, `cargo clippy --jobs 8 -p … --all-targets --all-features -- -D warnings`, rustdoc with `-D rustdoc::broken_intra_doc_links`, the advisory doc lints from `p-brief.md` §9, and `check/rekey.py <BASE>`.
  - TypeScript, TSX and CSS: `pnpm -C web run lint` and `pnpm -C web run typecheck`.

  Add the pilot's lessons to the brief's "what reviews keep finding" section (`result.md` §1 and the pilot ledger):
  - The commonest Importants were `widened` and `lost-why`.
  - Cutting a sentence that a quote elsewhere names leaves a `lost-referent`.
  - `missed-trim` turns up on changed blocks too.
  - Fix rounds leave orphan words and over-width lines.
  - A finding left in a fix round carries its reason to the reviewer.
  - Read every file whole: pilot-03 decided from the `.md` alone and still had 2 Important and 3 `missed-trim`.

  In `review-brief.md`, add the double-backtick hand-edit form that `brief.md` allows (pilot Task 0 P1). Also add, for TypeScript: JSDoc tags belong to their comment, and a JSX `{/* … */}` is a comment.
- [ ] **Step 7: Brief review.** A fresh Opus agent, read-only, reviews `brief.md` and `review-brief.md` against this plan's Global Constraints. Its Critical and Important findings go back to the Task 0 agent by SendMessage.
- [ ] **Step 8:** Reply in a few lines: the directive forms found, the re-cut's match, the lexer differences, each check's exits (clean and planted), rekey's match, and the brief review's verdict.

### Task 1: Cut the PR (every PR, Opus)

- [ ] Cut the branch from `main` (PR 1's branch already holds this plan) and record `<BASE>` in a fresh `.superpowers/sdd/progress.md`.
- [ ] **PR 1 only, the inventory:** assign every tracked file that `lexer.lang_of` knows to exactly one PR's scope. Write the files that fall in none into `ws/inventory.md`, for PR 5's `params.md`.
- [ ] **PR 5 only:** Task 0's Steps 1, 3 and 4 for the languages the scripts and CI files use (sh, YAML, TOML, a Dockerfile, which has no extension). Measure their comment width at `<BASE>` and put it in `params.md`.
- [ ] Write `<prefix>/params.md`. Run `python3 .superpowers/sdd/comment-pass/worklist.py .superpowers/sdd/comment-pass/<prefix>/<prefix> <scope paths> --all --max-blocks 150 --max-lines 1300 --first-id 1`. Write `<prefix>/chunks.tsv` (chunk, blocks, comment lines, files). If the block count differs from this plan's table by more than 5%, report it.
- [ ] Reply with the chunk table.

### Task 2: One chunk (Sonnet, every chunk in order)

- [ ] Dispatch with `model: "sonnet"`: "Follow `brief.md` exactly as written; reading every file the chunk names whole and the full report are required, and the budget is ample. Params: `<prefix>/params.md`. Chunk: `<chunk>`. HEAD: `<sha>`."
- [ ] The commit message is `Comments in <scope> keep their why; history and measurements are in <notes file> or gone`.
- [ ] Reply: SHA, counts, check exits, unsure calls, false-at-base list. The controller records the agent id in the ledger.

### Task 3: Blind review (Opus, per chunk, as soon as its commit lands)

- [ ] Copy the decisions file to `<prefix>/review/<chunk>-<n>.md` with `p/strip_for_review.py` and a neutral `<n>`, and record the mapping in `<prefix>/mapping.md`.
- [ ] Dispatch one Opus reviewer with `review-brief.md`. A chunk over 1,000 comment lines gets two reviewers, split by file. The reviewer writes `<prefix>/review/<chunk>-<n>-review.md` and replies with its counts.
- [ ] From the review's TSV rows, never its summary line, the controller adds the chunk's row to `<prefix>/tally.md`: changed, kept, Critical, Important, Minor, `missed-trim`.

### Task 4: Fix round (per chunk)

- [ ] Send each Critical, Important and `missed-trim` finding to the chunk's implementer by SendMessage, one line each (block, what is wrong, "draft in the review"), while no other implementer is running. The implementer follows `brief.md`'s fix-round section, reruns every check including the width check, the guard and rekey, and commits.
- [ ] The same reviewer, by SendMessage, checks only the fix commit. Each finding is fixed, or left with the implementer's reason, which the controller relays. The reviewer also checks that the fix adds no new defect.
- [ ] One round only. Whatever is still open goes into the ledger and the PR body.

### Checkpoint (PR 1 only, after six chunks are reviewed)

- [ ] Compute two rates over the first six chunks from `ws/tally.md`: (Critical + Important) per changed block, and `missed-trim` per kept block. Set them beside the pilot's 0.122 and 0.019, and tell the user.
- [ ] Carry on unless a rate is over twice the pilot's. If one is, stop and ask the user whether to carry on, switch the implementer to Opus, or stop the pass.

### Task 5: False at base (every PR, after the last fix check)

- [ ] Deduplicate every chunk's false-at-base items, from both the reports and the reviews, into `<prefix>/false-at-base.md`.
- [ ] A fresh Sonnet implementer fixes them in one comment-only commit under `brief.md`'s rules. An Opus agent checks only that commit, and one fix round follows. Items that need a code change go to the user as a list.

### Task 6: Pointers, cites and baseline keys (Opus read-only, then one fix commit)

- [ ] As the pilot's Task 5 did, reduce the PR's removed comment lines to distinctive phrases and `git grep -i -F` each across the tree. Mark each hit CONFIRMED (it now points at nothing), MOVED (the text is in the notes, so repoint it) or FINE. Write the results to `<prefix>/pointers.md`.
- [ ] Run `check/rekey.py <BASE>`. Every moved baseline key gets its new key, and every live cite into shifted lines is rewritten to name the item.
- [ ] A Sonnet implementer makes the fixes in one commit, and an Opus agent checks it. From here on, rerun `rekey.py` after every commit that moves lines, including a rebase.

### Task 7: Budget, gate, PR

- [ ] Lower the PR's areas in `scripts/comment-budget.json` to the shares the guard prints. The guard must then exit 0.
- [ ] Update `docs/status.md`'s Later entry on the comments clean-up: name this plan and the areas done. PR 5 closes the entry.
- [ ] If `main` has moved, rebase onto it, then rerun the guard and `rekey.py`.
- [ ] A subagent runs `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| <log> 2>&1` and quotes the `all green` line. If a browser project fails with no failed assertion, check the Chromium crash log and rerun that project before suspecting the branch.
- [ ] Write the PR body with one line per paragraph. It covers the scope, the method in one sentence, and the follow-ups: findings left open, code strings with history, code observations, and false-at-base items that need code. Keep it to a few figures.
- [ ] Claim-check the body against the branch. Pass 1 includes the body; later passes cover only the correction commit and the body.
- [ ] Push on the user's yes, and open the PR on a separate yes. The user merges.

### Task 8: After the merge

- [ ] Verify the squash: one parent, which is `main` at the merge; tree equal to the PR head's; body once; "(#N)"; no attribution. Then fast-forward `main`.
- [ ] On the user's yes, delete the local branch and move the ledger and `<prefix>/` to `.superpowers/sdd/comments-<prefix>-evidence/`.
- [ ] In `.superpowers/sdd/comment-pass/summary.md`, add a row for the PR: blocks, the two rates before and after the fix round, and its cost (from `cost-state`, or priced from transcripts as `result.md` §2.1 did) beside its figure in this plan.
- [ ] Start the next PR at Task 1, from the new `main`, in a fresh session.
