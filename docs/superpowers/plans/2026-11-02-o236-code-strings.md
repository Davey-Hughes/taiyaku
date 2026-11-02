# O236: code strings that contradict the code — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close O236 by making the refusal messages, test strings, an `#[expect]` reason and a test's `reserved` set say what the code does.

**Architecture:** String and test edits only, plus one experiment: walk a symlink cycle with no depth ceiling to settle which paragraph of `MAX_DEPTH`'s doc is right before the test that names the outcome is renamed. No behaviour changes except the text of two refusals.

**Tech Stack:** Rust (taiyaku-library, taiyaku-serve, taiyaku-cli, taiyaku-core), TypeScript tests under `web/` (vitest).

**Spec:** O236's row and bullet in `docs/status.md`. Locations are by symbol. The line numbers are hints taken at main 4a214fb7.

## Global Constraints

- The new refusal text, verbatim (the user chose it on 2026-11-02):
  - `credential::hash_with_salt`, `session.rs`'s `change_password` and `user.rs`'s `accept`: `an empty password would let anybody who presses Enter sign in`
  - `session.rs`'s `claim` (inside its JSON `{"error": ...}` body): `an empty password would let anybody who presses Enter sign in as this server's administrator`
- `004_auth.sql` is not touched (schema.rs's append-never-edit rule).
- A comment, assertion message or `#[expect]` reason says why: the reason, the invariant, what breaks. No history, no measurements (CLAUDE.md; `scripts/check-all.sh`'s comment guard enforces part of this).
- No AI attribution anywhere.
- Gate: run the repo's own `scripts/check-all.sh`, never a copy, with `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8`, into a log file. Quote its `all green` line. Do not pipe it through `tail`.
- Web tests: `./node_modules/.bin/vitest run --reporter=default` from `web/`. Do not use `pnpm exec`.
- Every changed assertion gets a watched mutation: put the old string or behaviour back, see the test fail, restore it, and check `git diff` shows the restore. Record each one in the task report.
- Branch `o236-code-strings` from main 4a214fb7, in the main checkout. Evidence and reports go in `.superpowers/sdd/o236-evidence/`.

---

### Task 1: The empty-password refusals give the real reason

**Files:**
- Modify: `crates/taiyaku-library/src/credential.rs` — `hash_with_salt`'s refusal (~465)
- Modify: `crates/taiyaku-serve/src/session.rs` — `claim`'s JSON refusal (~715) and `change_password`'s refusal (~1077)
- Modify: `crates/taiyaku-cli/src/user.rs` — `accept`'s refusal (~164)
- Test: `crates/taiyaku-library/src/credential/tests.rs` (~262: `why.contains("empty password")`, which the old and new text both pass)
- Test: `crates/taiyaku-serve/src/session/tests.rs` — the `change(FIXTURE_PASSWORD, "")` assertion (~1441, whose message is the old text) and `a_server_cannot_be_claimed_with_an_empty_password` (~1505, which asserts the status only)
- Test: `crates/taiyaku-cli/src/user/tests.rs` (~270-275: asserts `contains("lock")`, which pins the wrong reason)
- Test: `web/tests/Account.test.tsx` — `refusesInWords`'s `PUT /api/password` mock (~3534) and the expectation that reads it back (~3589)

- [ ] **Step 1: Make the tests pin the reason.** In each Rust test above, assert that the refusal text contains `presses Enter`:
  - `credential/tests.rs`: next to the existing `contains("empty password")`.
  - `user/tests.rs`: replace `contains("lock")`. Keep the assertion's message, "the refusal should say what an empty password would do".
  - `session/tests.rs`: for `change_password`, check the 400's body as well as its status. The `change` helper returns only a `StatusCode`, so either widen it or send that one request inline. Pick whichever reads like its neighbours. For the claim, add a body check to `a_server_cannot_be_claimed_with_an_empty_password` that the `error` field contains `presses Enter` and `administrator`. Replace the old-text assertion message with one that says why the request is refused.
- [ ] **Step 2: Run them and watch them fail.** Run `cargo nextest run -p taiyaku-library credential` and the same for `-p taiyaku-serve session` and `-p taiyaku-cli user`. Expected: the four new assertions FAIL on the old text.
- [ ] **Step 3: Change the four strings** to the Global Constraints text. In `Account.test.tsx`, put the shared text in the mock and in the expectation; they must match each other.
- [ ] **Step 4: Run the tests and watch them pass.** Rerun the three Rust filters from Step 2, then run the whole packages (`cargo nextest run -p taiyaku-library -p taiyaku-serve -p taiyaku-cli`) and the web `jsdom` project. Expected: PASS.
- [ ] **Step 5: Watched mutations.** One site at a time, put back the old string, run that site's test and see it fail, then restore it. Do this for `hash_with_salt`, `change_password`, `claim` and `accept`. Expected: four FAILs, then `git diff` shows only the intended changes.
- [ ] **Step 6: Check nothing else uses the old wording.** Run `grep -rn -a "lock the account rather than open\|only administrator with no" crates web/src web/tests docs --exclude=status.md`. Expected: no hits. `docs/status.md` keeps its quotes until Task 3.
- [ ] **Step 7: Commit:** "The empty-password refusals say anybody could sign in, which is why they refuse (O236)"

### Task 2: The test strings, `merge_spoke`'s reason and zoom's `reserved` set

**Files:**
- Modify: `crates/taiyaku-cli/src/inventory.rs` — `MAX_DEPTH`'s doc (~128-150)
- Modify: `crates/taiyaku-cli/src/inventory/tests.rs` — `a_symlink_cycle_errors_naming_the_depth_instead_of_overflowing` (~158)
- Modify: `crates/taiyaku-serve/src/auth/tests.rs` — `an_api_key_authenticates_without_reaching_argon2`'s assertion message "...whose password is locked" (~405)
- Modify: `crates/taiyaku-serve/src/metadata/tests.rs` — the assertion messages in `patching_a_work_out_of_a_series_saves_the_sibling_left_behind` (~1211, "its root is read-only") and `patching_a_work_out_of_a_series_reaches_its_real_sibling_not_series_one` (~1286, "root_a, read-only")
- Modify: `crates/taiyaku-core/src/sidecar.rs` — `Sidecar::merge_spoke`'s `#[expect(clippy::too_many_lines, reason = ...)]` (~639-649)
- Modify: `web/tests/modules/zoom.test.ts` — "claims nothing intentOf answers before modules" (~38-52)

- [ ] **Step 1: The cycle experiment.** It decides between the two accounts in `MAX_DEPTH`'s doc. Paragraphs 1-2 say that without the ceiling the walk recurses forever and overflows the stack. Paragraph 3 says Linux's 40-hop `ELOOP` makes `is_dir()` answer `false`, so the walk returns `Ok` with the cycle cut short. Temporarily set `MAX_DEPTH` to `u32::MAX` and run `cargo nextest run -p taiyaku-cli a_symlink_cycle --no-capture` into `.superpowers/sdd/o236-evidence/cycle-no-ceiling.log`. The test does `unwrap_err()`, so a panic saying it was called on `Ok` means paragraph 3 is right. A stack-overflow abort means paragraphs 1-2 are right. Also print the walked tree's depth so the log shows where it stopped. Restore the constant and confirm with `git diff --exit-code crates/taiyaku-cli/src/inventory.rs`. Nothing from this step is committed.
- [ ] **Step 2: Act on the result.**
  - If paragraph 3 is right: rename the test so its last words name the truncated `Ok` it rules out, not an overflow. Then rewrite the sentences in paragraphs 1-2 that claim a stack overflow or unbounded recursion so they agree with paragraph 3.
  - If paragraphs 1-2 are right: keep the name and rewrite paragraph 3.
  - Either way, grep `docs/` and `crates/` for the old test name and update every hit except `docs/status.md`, which Task 3 handles.
- [ ] **Step 3: `auth/tests.rs`.** The test sets a hash nothing verifies, not `LOCKED`, and `account::by_api_key` refuses a locked account. Reword the assertion message to say the key authenticates against an account whose password nothing verifies.
- [ ] **Step 4: `metadata/tests.rs`.** Both tests plant a file at `root_a/.taiyaku` instead of making the root read-only, because CI runs as root. The comment near ~594 says why. Reword both messages to describe that file. Do not call the root read-only.
- [ ] **Step 5: `merge_spoke`'s reason.** Read the function and count its guards against the reason's list. The credits-drop branch refuses nothing: it returns `Merged::DroppedCredits`. Rewrite the reason so its count and its verbs ("refuse" versus "report") match what each branch does. Keep the argument for keeping them in one function.
- [ ] **Step 6: zoom's `reserved` set.** `intentOf` (`web/src/keys.ts`) walks `READER_KEYS` before any module, and `READER_KEYS` answers `A` and `L`, which the hand-written set leaves out. Build the set from the table so it cannot drift again:
  ```ts
  const reserved = new Set(READER_KEYS.flatMap((binding) => binding.keys))
  ```
  Import `READER_KEYS` from `../../src/keys`. Reword the comment above the test so it gives that reason and no longer lists keys.
- [ ] **Step 7: Watched mutation for zoom.** Temporarily add a binding for `L` to `zoomModule.keys` in `web/src/modules/zoom.ts`. Run the zoom test and see it FAIL. The old hand-written set passes against this mutation, and that is the defect. Restore the file and check `git diff --exit-code web/src/modules/zoom.ts`. The Rust message edits in Steps 3-5 change no assertion's condition, so they need no mutation. Step 2's rename does not either.
- [ ] **Step 8: Run** `cargo nextest run -p taiyaku-cli -p taiyaku-serve -p taiyaku-core`, `cargo clippy --workspace --all-targets` (it checks `#[expect]`) and the web `node` and `jsdom` projects. Expected: PASS, and the Rust test count is unchanged from main.
- [ ] **Step 9: Commit:** "Test strings, merge_spoke's reason and zoom's reserved set say what their code does (O236)"

### Task 3: Close O236 in `docs/status.md`

**Files:**
- Modify: `docs/status.md`

- [ ] **Step 1: The five-part close.** Make each edit:
  - Tick the row's checkbox. Update its site list to give the renamed test's new name.
  - Rewrite the bullet to say what was done. Record the cycle experiment's outcome, and keep the `004_auth.sql` paragraph, which stays.
  - Leave the test counts (the "Five crates, N tests" figure and "Nine of the N") unless Tasks 1-2 changed the count. If they did, read the new figures off a run; do not add to the old ones.
  - Update the "N are now closed — leaving M" sentence and the "What remains … M open items" line. The count word wraps across a line break, so run `grep -n -A1 '^What remains:'`.
- [ ] **Step 2: Sweep.** Run the four greps from the open-items ritual. They count the rows and the closed rows, and find the test-count lines and the "What remains" line. Then grep the bare `O236` across `docs/` and `web/src/` and read every hit.
- [ ] **Step 3: Commit:** "O236 is closed"

### After the tasks (controller)

- Gate on the branch head: `scripts/check-all.sh` (Global Constraints), run by a subagent.
- Whole-branch review of `main...HEAD`, then the fixes.
- PR body in `.superpowers/sdd/o236-evidence/pr-body.md`, with its title on the first line. Run the claim check over the body and the status bullet.
- Push, open the PR and merge only on separate yeses from the user.
