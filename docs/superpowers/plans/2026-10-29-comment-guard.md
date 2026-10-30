# Comment guard: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A gate that stops comments growing back. It fails a branch whose new or changed comments tell history, whose new or grown comment blocks run past 8 lines, or that raises an area's comment share above a committed budget.

**Architecture:** One script, `scripts/check-comments.mjs`. It lexes tracked source into comment, string and code spans, then applies three rules:
- Rules 1 and 2 read only what the branch adds, diffed against its base.
- Rule 3 reads the whole tree against `scripts/comment-budget.json`.

It runs from `scripts/check-all.sh` and from CI, each run preceded by `--self-test`, in the same shape as `scripts/coverage-floor.mjs` (the closest precedent: a committed floor, a message that names the value to write).

**Tech Stack:** Node (no new dependency; CI and check-all already run `node scripts/*.mjs`). Prior art for the lexer: `.superpowers/sdd/comment-pass/lexer.py` (Python, proven by redextape's comment pass). Port its logic; do not call Python from the gate unless every CI job that runs the gate already has `python3`.

## Global Constraints

- **The rules (the user, 2026-10-29):** fail on history markers, on any comment block a branch adds or grows past **8** lines, and on any area's comment share rising above its committed baseline. taiyaku only; redextape gets it later, with what taiyaku's clean-up teaches.
- **The comment rule it enforces (the user, 2026-10-17):** a comment holds the why. History and measurements go to the docs or are dropped. A comment may point at a notes section or an O-number.
- **Existing comments are not failed** by rules 1 and 2. Only lines the branch adds or changes are; rule 3 compares totals.
- **Languages:** `.rs`, `.ts`, `.tsx`, `.mjs`, `.js`, `.css`, `.sh`, and the YAML under `.forgejo/` (whole-line `#` only). Rust nested block comments, raw strings and `'a` lifetimes vs char literals; TS template literals, regex literals and JSX `{/* */}`.
- **A code comment in this gate's own code says why** (the reason, the invariant, what breaks if it changes). The gate's own comments must pass the gate.
- **This machine:** the Bash tool is zsh with `noclobber` (start a log with `>|`), `grep` is a function (`grep -a`), `cd` is zoxide (absolute paths). Cargo at `--jobs 8`, `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8`. Never pipe the gate through `tail`; read the log.
- **Size:** each task under ~150 calls.

## The rules

1. **History markers** in comment text on lines the branch adds. Start from `worklist.py`'s `HIST` and narrow it:
   - Fail on: an ISO date that is not part of a file name (`2026-10-28-loading-library-design.md` is a pointer); a 7–40-character hex run holding at least one digit and one letter; `#` followed by digits (a PR number); `\bPR\b`; `previously`; `originally`; `formerly`; `was|were (changed|replaced|removed|added|renamed|moved|tried)`; `before (this|the) (fix|change)`; `~~`; and `used to` unless preceded by `is|are|was|were|be|been|being|get|gets|got`.
   - Allowed: O-numbers (`O148`) and `no longer` (its invariant use is common).
   - **Tune it on the tree.** Run the rule over every comment at the base and classify a sample of 60 hits. Narrow or drop any pattern whose sample is more than 1 in 5 false. Report the before and after counts; they go in the report, not the docs.
2. **Block cap.** A block is a maximal run of whole-line comments; one doc comment is one block. A block fails when two things hold:
   - It has more than 8 counted lines in the new file.
   - The branch grew it: the comment lines added inside it outnumber the comment lines removed in the hunks that overlap it.

   So a new long block fails; a trimmed block, or an edited line in an old long block, passes.

   Not counted: fenced code inside a doc comment; `# Errors`, `# Panics`, `# Safety` and `# Examples` sections; Rust `//!` module docs; a file's leading header block (before its first import or statement).
3. **Comment share.** For each area — `crates/taiyaku-cli`, `crates/taiyaku-core`, `crates/taiyaku-library`, `crates/taiyaku-read`, `crates/taiyaku-serve`, `crates/taiyaku-wasm`, `web/src`, `web/tests`, `scripts` — compute comment lines ÷ (comment lines + code lines). Blank lines count as neither. The baseline lives in `scripts/comment-budget.json` to 4 decimals.
   - Fail when an area is above its baseline. The message says the PR may raise the budget in the same diff, deliberately.
   - Fail too when an area is more than 0.5 points below its baseline, so a trimming PR locks in its gain. The message gives the value to write.
- **Base:** locally `git merge-base HEAD origin/main`. In CI, the PR's target branch. On a push to `main`, `HEAD^`. Check the CI checkout's `fetch-depth` gives the gate that history; deepen it if not.

---

### Task 1: Lexer, comment share, budget, wiring (Opus)

**Files:**
- Create: `scripts/check-comments.mjs`, `scripts/comment-budget.json`, `docs/superpowers/plans/2026-10-29-comment-guard.md` (this plan, committed first).
- Modify: `scripts/check-all.sh`, `.forgejo/workflows/ci.yml`.

- [ ] **Setup.** In your worktree, run `git switch -c comment-guard origin/main` (or `git reset --hard origin/main` on the branch the harness made). Check `git log -1` is `aca6dc6e` or later on `main`. Copy this plan to `docs/superpowers/plans/2026-10-29-comment-guard.md` and commit it.
- [ ] **The lexer**, ported from `lexer.py`, with a `--self-test` table: per language, one comment, one string holding `//` or `/*`, and one code line, each classified right. Rust raw strings, nested block comments, lifetimes; TS template literals with `${}`, regex literals, JSX comments.
- [ ] **Rule 3** and `scripts/comment-budget.json`, written from the tree at the base. The gate passes on the unchanged tree.
- [ ] **Prove rule 3 by hand on a scratch branch.** Add 30 comment lines to one `web/src` file: it fails, naming `web/src`. Delete 1,000 comment lines' worth from one area: it fails as stale, giving the value. Delete the scratch branch.
- [ ] **Wire it.** Add `node scripts/check-comments.mjs --self-test` and `node scripts/check-comments.mjs` to `check-all.sh` (beside `status-test-count.mjs`'s step) and to CI, in a job that already has node. Commit.
- [ ] Reply in a few lines: SHA, self-test count, the hand proofs' exits, which CI job.

### Task 2: History markers and the block cap (Opus)

**Files:** Modify `scripts/check-comments.mjs`.

- [ ] **Rule 1:** its self-test cases (each pattern fires, each allowed form stays quiet), then the tuning run (rule 1 above). Report the sample, the classifications and the patterns changed.
- [ ] **Rule 2:** its self-test cases: a new 9-line block fails; a new 8-line block passes; a 20-line block trimmed to 12 passes; a 20-line block with one line edited passes; a 20-line block grown by one line fails; a 30-line `//!` module doc passes; a doc comment with a 12-line fence and 4 prose lines passes.
- [ ] **Prove both by hand on a scratch branch.** Add a 9-line comment to a `.rs` file and a `.tsx` file: rule 2 fails on each. Add `// this used to read the cache first` to a `.ts` file: rule 1 fails. Add `// O148 explains why` to the same file: it passes. Delete the scratch branch.
- [ ] Commit. Reply: SHA, self-test count, tuning figures, the hand proofs' exits.

### Task 3: Docs (Opus, may be the Task 2 agent)

- [ ] `docs/ci.md`: a section for the gate covering why it exists, the three rules, what is exempt, how to raise a budget deliberately, and how the base is chosen. Follow the file's existing section shape.
- [ ] If `docs/status.md` lists the CI gates anywhere, add this one there.
- [ ] Commit.

### Task 4: Review, gate, PR

- [ ] One Opus whole-branch review (fresh agent). The code goes in the brief; no conclusions do.
- [ ] Fix round by the implementer via SendMessage; a narrow review of the fix commit.
- [ ] A subagent runs `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| <log> 2>&1` and quotes the `all green` line.
- [ ] Draft the PR body (one line per paragraph), claim-check it, and open the PR with the Gitea MCP on the user's yes.
- [ ] **Interaction with the comment pass:** `comments-pilot` lowers its crates' shares. Whichever PR merges second rebases and, if the guard is in, lowers those budgets in `comment-budget.json`.
