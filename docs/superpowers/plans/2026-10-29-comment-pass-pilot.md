# Comment pass pilot: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut every comment in `crates/taiyaku-core`, `crates/taiyaku-read` and `crates/taiyaku-wasm` down to its why, with Sonnet doing the cutting. Measure two things: the defects Sonnet leaves, found by an Opus reader of every block, and the cost of each model per block. The user then chooses the scope and the model for the rest of the repo from taiyaku's own numbers.

**Architecture:** This is redextape's comment pass (its plan `2026-10-17-comments-and-repeats.md`, its Part D method) run on three small crates. The tools live in `.superpowers/sdd/comment-pass/` (gitignored, never committed): `worklist.py` cuts every comment block into chunks, an implementer writes one decisions file per chunk, `apply.py` applies it, and `check_comment_only.py` proves the code stream is byte-identical. One chunk is also done independently by Opus as a control. Every decisions file is reviewed blind by Opus, using one prompt.

**Tech Stack:** Python 3 (tools), Rust (cargo clippy pedantic, rustdoc), the Gitea MCP for the PR.

## Global Constraints

- **The comment rule (the user, 2026-10-17):** a comment holds the why: the reason, the invariant, what breaks if it changes. History (what the code used to do, how a defect was found, a struck-out earlier reason, a PR's story) and measurements come out of comments, moved to the docs or dropped. A comment may point at a notes section or an O-number; it does not retell it.
- **Trim level:** "drop history, keep why" (the user's example: `derive.rs`'s O210 block, 9 lines to 6).
- **Comment-only:** every commit on this branch leaves `python3 .superpowers/sdd/comment-pass/check_comment_only.py aca6dc6e` at exit 0. `aca6dc6e` is `main` when the branch was cut; the plan's own commit changes only `docs/`.
- **Moves default to DROP.** A removed sentence is moved only when it passes `d-brief.md`'s (a)/(b)/(c) test. Moved text is verbatim: same case, punctuation and paragraph breaks. It goes to `docs/superpowers/notes/2026-10-29-code-history-core-read-wasm.md`, and `apply.py` writes the headings.
- **Kept intact:** fenced code in comments, the `# Errors`, `# Panics`, `# Safety` and `# Examples` sections, and the first sentence of every public item's doc.
- **Replacement text** adds no fact the original did not state, reads as true of the code today, backticks every identifier (`clippy::doc_markdown`), and is rewrapped to the comment width around it (rules 8–12 of `redextape-ref/c-constraints.md`).
- **False at base:** a comment found untrue of the code at `aca6dc6e` is listed in the report (file, sentence, why), not fixed. The user decides at the end.
- **Models:** implementers run `model: "sonnet"`; the control implementer, every reviewer, Task 0 and the end checks run `model: "opus"`. A reviewer is never told which model wrote what it reviews.
- **This machine:** the Bash tool is zsh with `noclobber` (start a log with `>|`), `grep` is a function (`grep -a`), and `cd` is zoxide (use absolute paths). Cargo runs with `--jobs 8` and test threads at 8. Never two committing agents in one checkout.
- **Size:** one subagent task finishes in under ~150 calls; a chunk is at most 150 blocks.
- **The controller stays thin:** it dispatches, reads the few-line replies, keeps `.superpowers/sdd/progress.md`, and talks to the user.

## Files

- Create: `docs/superpowers/notes/2026-10-29-code-history-core-read-wasm.md` (written by `apply.py`).
- Modify: comments only, in `crates/taiyaku-core/**`, `crates/taiyaku-read/**` and `crates/taiyaku-wasm/**`.
- Scratch, not committed: `.superpowers/sdd/comment-pass/` (tools, `check/`, briefs under `p/`, chunks `p/pilot-NN.{md,json}`, decisions, reviews, reports, `p/result.md`).

---

### Task 0: Tools and briefs for taiyaku (Opus)

**Files:** `.superpowers/sdd/comment-pass/{check_comment_only,apply}.py`, `check/{check_moves,partial_cuts,id_mismatch}.py`, `check/revert_check.sh`; create `p/p-brief.md`, `p/p-review-brief.md`, `p/chunks.tsv`.

- [ ] **Step 1: Strip redextape from the tools.** `check_comment_only.py`: empty `ALLOWED_FENCE_EDITS` and drop the self-test's pinned-fence check (it reads redextape commit `630248f`). `check/*`: repoint `REPO`, `W`, `GLOB`, `NOTES_RE` and every notes path to taiyaku and `2026-10-29-code-history-*`. Self-tests that read redextape commits are replaced by Step 2's trial.
- [ ] **Step 2: Prove each check on taiyaku, on a throwaway branch.** Cut it from `comments-pilot` and run a two-block trial there (one replace with a move, one delete) on a file in `crates/taiyaku-read`. Each check must pass on the clean trial and fail on its planted defect:
  - `check_comment_only.py`: a one-character code change.
  - `check_moves.py`: a lowercased word in a moved paragraph.
  - `partial_cuts.py`: a trimmed sentence inside a moved paragraph.
  - `id_mismatch.py`: a decisions section under the wrong block id.

  Then delete the branch and confirm `git status` on `comments-pilot` is clean. Also run `check_comment_only.py --self-test` and require exit 0.
- [ ] **Step 3: Write the briefs.** `p/p-brief.md` is the implementer brief: `redextape-ref/d-brief.md` adapted to Rust and taiyaku, plus `c-constraints.md` rules 8–12 and this plan's Global Constraints. Its per-chunk checks:
  - `check_comment_only.py aca6dc6e`, `check/check_moves.py HEAD`, `check/partial_cuts.py`, `check/id_mismatch.py`.
  - `cargo clippy -p taiyaku-core -p taiyaku-read -p taiyaku-wasm --all-targets --all-features -- -D warnings`.
  - The rustdoc line from `scripts/check-all.sh` (`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"`), limited to the three crates.

  `p/p-review-brief.md` adapts `redextape-ref/d-review-brief.md`. The reviewer reads the chunk's `.md` (every block), a decisions file, and the source at `aca6dc6e` (`git show aca6dc6e:<path>`). It is read-only and ends with a TSV of findings: `block  severity  class  note`.
  - **Severity:** Critical (now false or a safety argument lost), Important, Minor.
  - **Class:** `false-now`, `lost-why`, `lost-referent`, `widened`, `added-fact`, `move-not-verbatim`, `over-move`, `missed-trim` (a kept block still holding history or a measurement), or `other`.
  - The last line gives the counts of changed blocks and kept blocks read.
- [ ] **Step 4: Cut the chunks.** Run `python3 .superpowers/sdd/comment-pass/worklist.py .superpowers/sdd/comment-pass/p/pilot crates/taiyaku-core crates/taiyaku-read crates/taiyaku-wasm --all --max-blocks 150 --first-id 1`. Expect 716 blocks in six chunks. Write `p/chunks.tsv` (chunk, blocks, comment lines, files) and name the **control chunk**: the one with the most comment lines.
- [ ] **Step 5:** Reply in a few lines: check exits (clean and planted), chunk table, control chunk.

### Task 1: Opus control on the control chunk (Opus, read-only on source)

Run it before Task 2 reaches the control chunk; it may run alongside Task 2's other chunks.

- [ ] Follow `p/p-brief.md` exactly as an implementer would, but write `p/<X>-decisions-b.md` and stop at `apply.py … --dry-run`. Never apply, never commit, never edit source.
- [ ] Reply: blocks kept/deleted/replaced/moved, unsure calls, false-at-base list.

### Task 2: One Sonnet chunk (repeat for each chunk, the control chunk last)

- [ ] Dispatch with `model: "sonnet"`. Brief: `p/p-brief.md`; inputs: the chunk name, the checkout, `HEAD`. The implementer reads the chunk's `.md` and every file it names whole, writes `p/<chunk>-decisions-a.md`, applies it, runs every check in the brief, and commits: `Comments in <crates> keep their why; history and measurements are in docs/superpowers/notes/2026-10-29-code-history-core-read-wasm.md or gone`.
- [ ] Reply: SHA, counts, check exits, unsure calls, false-at-base list. The controller records the agent id: fixes go back to it by SendMessage.

### Task 3: Blind Opus review of every decisions file

- [ ] Before each review, copy the decisions file to `p/review/<chunk>-<n>.md` with a neutral `<n>`. Record which file is which in `p/mapping.md`, which no reviewer reads.
- [ ] Dispatch one Opus reviewer per file: six Sonnet chunks plus the control, so seven. A chunk over 1,000 comment lines gets two reviewers, split by file.
- [ ] Reviews are read-only and may run while Task 2 commits other chunks.
- [ ] Each writes `p/review/<chunk>-<n>-review.md` and replies with its counts.

### Task 4: Fix round for the Sonnet chunks

- [ ] Send each Critical, Important and `missed-trim` finding to that chunk's Sonnet implementer by SendMessage, one line per finding (block, what is wrong, "draft in the review").
- [ ] It commits the fixes and reruns the brief's checks.
- [ ] The same reviewer, by SendMessage, checks only that fix commit: each finding fixed or left with a reason, and no new defect inside the fix.
- [ ] One round. Anything still open goes into `p/result.md` as left.

### Task 5: Pointers into what was cut (Opus, read-only)

- [ ] Enumerate every place outside the changed blocks that names text this branch deleted or moved: other comments in the repo, `docs/`, test names and messages. Search for each removed sentence's distinctive phrases (`grep -a -i`), case-insensitively.
- [ ] Report each pointer as CONFIRMED (it now points at nothing), MOVED (the text is in the notes; repoint it) or FINE.
- [ ] Fix the CONFIRMED and MOVED ones in one comment-only commit, by the Task 2 implementer of the chunk that cut the text. The Task 3 reviewer checks it.

### Task 6: The measurement

- [ ] **Defects** (from the review TSVs and `p/mapping.md`):
  - On the control chunk, compare Sonnet and Opus: Critical and Important per changed block, and `missed-trim` per kept block.
  - Over all six Sonnet chunks: the same two rates, before the fix round and after it.
- [ ] **Cost:**
  - This session's last `cost-state` record (`modelUsage`, by model).
  - Per-agent shares from `~/temp/claude-cost-audit/` (its README). Subagent output tokens are under-recorded, so say so.
  - Give $ per block for: Sonnet implementing, Opus implementing (control), Opus reviewing, and the fix round.
- [ ] **Projection** to the repo's 15,418 blocks (`worklist.py --all`, all areas, at `aca6dc6e`) for two setups: Sonnet implements, then Opus reviews every block and fixes; or Opus implements, then Opus reviews and fixes. State the caveat: three Rust crates may not predict `web/`.
- [ ] Write `p/result.md` and give the user the table. The user chooses A, B or C and the model.

### Task 7: Gate and PR

- [ ] A subagent runs `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| <log> 2>&1` and quotes the `all green` line. A pipe would mask the exit status, so read the log.
- [ ] Draft the PR body. Short; one line per paragraph; the measurement goes in as a few figures at most.
- [ ] Claim-check the body against the branch.
- [ ] Open the PR with the Gitea MCP on the user's yes. The user merges.
