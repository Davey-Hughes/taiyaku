# The web tests get their own tree: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every web test file and test-only helper moves out of `web/src`
into a `web/tests/` tree that mirrors it, and the same tests run against the
same code afterwards.

**Architecture:** A mechanical move (`git mv` plus a scripted rewrite of each
moved file's relative paths), the config that points the runners, lint and
the type-check at `tests/`, one guard test that keeps `src/` free of tests,
and the live docs. Spec:
`docs/superpowers/specs/2026-10-28-web-tests-tree-design.md`.

**Tech Stack:** Vite, Vitest 4 (projects `node`, `jsdom`, `browser` with two
instances), TypeScript `tsc -b`, Biome, pnpm.

## Global Constraints

- Only path lines change inside a moved file: import and export specifiers,
  dynamic `import()`, `vi.mock` / `vi.doMock` / `vi.importActual` /
  `vi.importMock` paths, `?raw` / `?url` imports, `new URL(…, import.meta.url)`,
  and file-read paths. No test body, assertion or name changes.
- Nothing under `web/src` changes except the files that leave it and
  comments that name a moved path.
- Every move is a `git mv`.
- The mapping: `web/src/X.test.ts(x)` → `web/tests/X.test.ts(x)` (same
  relative place, `modules/` and `styles/` included);
  `web/src/testing/*` → `web/tests/support/*`;
  `web/src/browser-setup.ts` → `web/tests/support/browser-setup.ts`.
- Dated specs and plans under `docs/superpowers/` are not edited.
- A comment says why, never history or a measurement.
- Evidence (baselines, reports, scripts written for the checks, logs) goes in
  `.superpowers/sdd/web-tests-tree-evidence/`, never in `target/` and never
  elsewhere in the working tree.
- Run long commands into a log file and read the summary lines from it;
  never pipe a gate or test command through `tail` (that masks its exit
  status). The shell is zsh with `noclobber`: start a log with `>|`.
- Under Claude Code vitest picks its `agent` reporter; pass `--reporter=…`
  explicitly whenever the output is read.
- A browser project that goes red with import or connection errors and no
  failed assertion is the headless Chromium crashing: check
  `coredumpctl list` and rerun the whole project before suspecting the
  branch.
- Work on branch `web-tests-tree` in the main checkout. No worktrees.

---

### Task 1: Move the tests and helpers, rewrite their paths, point the config at `tests/`

**Files:**
- Move: every `web/src/**/*.test.ts` and `web/src/**/*.test.tsx`;
  `web/src/testing/*`; `web/src/browser-setup.ts` (mapping in Global
  Constraints)
- Modify: `web/vite.config.ts` (the three projects' `include` / `exclude`,
  `SHOWN_SCROLLBARS`, both `setupFiles`, coverage `exclude`, and comments in
  the file that name a moved path); `web/tsconfig.app.json` (`include`);
  `web/package.json` (`lint`)
- Evidence: `.superpowers/sdd/web-tests-tree-evidence/`

**Interfaces:**
- Produces: the `web/tests/` tree and `web/tests/support/` that Task 2's
  guard and docs describe; the baselines Task 2 compares against
  (`names-main-<project>.txt`, `coverage-main/`); the comparison scripts
  `names.mjs` and `coverage-diff.mjs` in the evidence directory.

- [ ] **Step 1: Confirm the starting point**

Run: `git -C /home/davey/projects/taiyaku diff --quiet d0c4dad2 HEAD -- web && echo same`
Expected: `same` (the branch differs from main only in docs, so a baseline
taken here is main's).

- [ ] **Step 2: Take the test-name baselines**

For each of the four runs, run vitest with the JSON reporter into the
evidence directory: `--project node`, `--project jsdom`, and the two browser
instances, each by its own name. Read the instance names from
`web/vite.config.ts` (the second is
`browser (chromium, scrollbars shown)`; confirm the first's resolved name with
`pnpm -C web exec vitest list --project 'browser*' --json` or the reporter's
output before relying on it). Example for one run:

`pnpm -C web exec vitest run --project node --reporter=json --outputFile=/home/davey/projects/taiyaku/.superpowers/sdd/web-tests-tree-evidence/report-main-node.json >| /home/davey/projects/taiyaku/.superpowers/sdd/web-tests-tree-evidence/run-main-node.log 2>&1`

Each run must pass. Then write `names.mjs` in the evidence directory: it
reads one JSON report and prints one line per test,
`<file> :: <fullName> :: <status>`, sorted, where `<file>` is the test file's
path with the leading `…/web/src/` or `…/web/tests/` removed and a leading
`testing/` rewritten to `support/`, so a moved file's lines compare equal to
its old ones. Write `names-main-<project>.txt` for each run.

Expected: four non-empty name lists; every status `passed` or the same
`skipped`/`todo` it has on main.

- [ ] **Step 3: Take the coverage baseline**

Run: `pnpm -C web exec vitest run --project node --project jsdom --coverage --coverage.reporter=json --coverage.reporter=json-summary --coverage.reportsDirectory=/home/davey/projects/taiyaku/.superpowers/sdd/web-tests-tree-evidence/coverage-main --reporter=default >| …/run-main-coverage.log 2>&1`

Then write `coverage-diff.mjs`: given two `coverage-final.json` files, for
every source file it compares the set of statement ids with a hit count
above 0, the set of `(branch id, arm index)` pairs above 0 and the set of
function ids above 0, and prints each file whose sets differ (and any file
present in one report and not the other). It compares covered sets, not hit
counts. Exit status 1 when anything differs. Run it baseline-against-itself
as a control: it must print nothing and exit 0.

- [ ] **Step 4: Write and run the move script**

Write the script in the evidence directory, not in the working tree. It:
1. builds the move map from `git ls-files web/src` using the mapping in
   Global Constraints;
2. `git mv`s each file, creating `web/tests/…` directories as needed;
3. in every moved file, finds each relative specifier (starting `./` or
   `../`) in the path-line forms listed in Global Constraints, strips any
   `?query`, resolves it against the file's **old** directory, maps the
   result through the move map if the target itself moved, makes it
   relative to the file's **new** directory (prefixing `./` when needed),
   re-appends the query and writes it back;
4. prints every rewrite as `file: old -> new` into a log in the evidence
   directory, and every relative specifier it saw but could not resolve to
   an existing file (with the extensions `.ts`, `.tsx`, `.js`, `/index.ts`
   tried).

Expected: no unresolved specifier left unexplained in the log.

- [ ] **Step 5: Fix the file reads by hand**

List the moved files that read a file at run time:
`git -C /home/davey/projects/taiyaku grep -lE "readFileSync|readdirSync|import\.meta\.url|import\.meta\.glob|\?raw|\?url|__dirname" -- web/tests`

For each, check every path it reads resolves, from the new location, to the
same file it read from the old one (a `join(dirname, '…')` or a string
handed to a helper is not caught by Step 4). Record each file and what it
reads in the evidence directory.

- [ ] **Step 6: Point the config at `tests/`**

- `web/vite.config.ts`: `src/**/*.test.*` patterns become `tests/**/…` in
  each project's `include` and `exclude` and in `SHOWN_SCROLLBARS`;
  `./src/browser-setup.ts` becomes `./tests/support/browser-setup.ts` in both
  `setupFiles`; coverage keeps `include: ['src/**/*.ts', 'src/**/*.tsx']` and
  drops the excludes for `src/**/*.test.ts`, `src/**/*.test.tsx`,
  `src/browser-setup.ts` and `src/testing/**` together with the comments
  that explain only those excludes; `css.include` stays.
- `web/tsconfig.app.json`: `"include": ["src", "tests"]`.
- `web/package.json`: `"lint": "biome check ./src ./tests"`.
- Confirm, and record in the evidence directory, that `scripts/check-all.sh`
  and `.forgejo/workflows/*` name no moved path:
  `git -C /home/davey/projects/taiyaku grep -nE "src/[A-Za-z/]*\.test\.|src/testing|browser-setup|src/\*\*" -- scripts .forgejo Dockerfile`
  (a hit that only names coverage's `src/**` include or a source path is
  fine; say so for each).

- [ ] **Step 7: Run lint, type-check and the four runs**

Run each into a log: `pnpm -C web run lint`, `pnpm -C web run typecheck`,
then the four JSON-reporter runs of Step 2 with `report-branch-<project>.json`
names, then the coverage run of Step 3 into `coverage-branch`.

Expected: all pass.

- [ ] **Step 8: Watch lint and the type-check read `tests/`**

In one moved `.test.ts` file, add `const unused: number = 'x'`; run
`pnpm -C web run typecheck` (expect it red, naming that file) and
`pnpm -C web run lint` (expect a diagnostic in that file). Remove the line;
both green again. Record both red outputs' relevant lines.

- [ ] **Step 9: Compare**

1. `names-branch-<project>.txt` equals `names-main-<project>.txt` for each
   of the four runs (`diff` prints nothing).
2. `node coverage-diff.mjs coverage-main/coverage-final.json coverage-branch/coverage-final.json`
   prints nothing and exits 0.
3. `git -C /home/davey/projects/taiyaku diff -M --name-status main -- web`
   shows `R` for every moved file and `M` only for the three config files.
   No `A`/`D` pair for a moved file.
4. `git -C /home/davey/projects/taiyaku diff -M main -- web/tests` read in
   full: every `-`/`+` line is a path line. Record a count of changed lines
   by kind (import, mock, URL/read) in the evidence directory.

- [ ] **Step 10: Commit**

```bash
git -C /home/davey/projects/taiyaku add -A web
git -C /home/davey/projects/taiyaku commit -m "The web tests and their helpers move to web/tests/, mirroring web/src"
```

Then `git status --short` shows a clean tree outside `.superpowers/`.

---

### Task 2: The guard against a test in `src`, and the live docs

**Files:**
- Create: `web/tests/layout.test.ts`
- Modify: `docs/ci.md`, `web/README.md`, `docs/status.md`, `docs/notes.md`,
  and any comment in `web/`, `scripts/` or `.forgejo/` that names a moved
  path (found by the grep in Step 4)

**Interfaces:**
- Consumes: Task 1's `web/tests/` tree, the `node` project (which runs
  `tests/**/*.test.ts`), `names.mjs` and `names-main-node.txt`.

- [ ] **Step 1: Write the guard**

The plan is the first place this exists. Shape it like its neighbours in
`web/tests/` (read two of them that use `node:fs` first):

```ts
import { readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, it } from 'vitest'

// The vitest projects look only in `tests/`, so a test file under `src/`
// would never run and nothing else would say so.
const SRC = fileURLToPath(new URL('../src', import.meta.url))

it('keeps every test file out of src', () => {
  const strays = readdirSync(SRC, { recursive: true, encoding: 'utf8' })
    .filter((name) => /\.test\.tsx?$/.test(name))
    .sort()
  expect(strays).toEqual([])
})
```

- [ ] **Step 2: Watch it fail, at the top and nested**

Create `web/src/stray.test.ts` and `web/src/modules/stray.test.tsx` (any
content). Run
`pnpm -C web exec vitest run --project node --reporter=default tests/layout.test.ts`.
Expected: FAIL, the received list naming both files. Delete both; rerun;
expected PASS. Record the red output's relevant lines.

- [ ] **Step 3: Run the node project and compare names**

Run the `node` JSON-reporter run into `report-branch2-node.json` and its
names list. Expected: `diff names-main-node.txt names-branch2-node.txt`
shows exactly one added line, the guard, and nothing removed.

- [ ] **Step 4: Bring the live docs and comments up to date**

Find every live mention of a moved path:
`git -C /home/davey/projects/taiyaku grep -nE "web/src/[A-Za-z/]*\.test\.|src/[A-Za-z/]*\.test\.tsx?|src/testing|src/browser-setup|src/\*\*/\*\.(browser\.|css\.)?test" -- ':!docs/superpowers'`

Update each to the new path or glob. A mention that gives a bare file name
(`reads.test.ts`) stays. Specifically:
- `docs/ci.md`: its account of the vitest projects and their globs.
- `web/README.md`: wherever it says where tests live or how they are found.
- `docs/status.md`: each path-qualified mention, and wherever it describes
  where the web tests live; in the loading entry (the bullet opening
  "**The loading work, in two PRs.**"), one sentence that the user ruled on
  2026-10-28 that the web test move comes before the second loading PR.
- `docs/notes.md`: each path-qualified mention.

Rerun the grep. Expected: no hits, or only hits you have recorded as right
(for example a sentence about the guard that names `src/` on purpose).

- [ ] **Step 5: Lint, type-check and commit**

Run `pnpm -C web run lint` and `pnpm -C web run typecheck` into logs;
both pass.

```bash
git -C /home/davey/projects/taiyaku add web/tests/layout.test.ts docs web scripts .forgejo
git -C /home/davey/projects/taiyaku commit -m "A test keeps web/src free of test files; the live docs name web/tests/"
```

---

### Task 3: Re-derive the web coverage floor's `minFiles` (spec ruling 5)

**Files:**
- Modify: `scripts/coverage-floor.mjs` (the web format's `minFiles`, ~:221,
  and the arithmetic in the comment above it, ~:208)
- Modify: `docs/status.md` (the `minFiles` bullet, ~:881–905: the stated
  value and measured count, and the list of re-derivations, which gains this
  one as the fourth, in the form the third is written)
- Modify: `docs/ci.md` (the section "Why `minFiles` is 108, and was 59 for
  too long — three times now", ~:827, its pointer at ~:704, and the table at
  ~:1034; ~:385 only if it states the current value)
- The arithmetic goes where the third re-derivation's went: find where the
  whole-branch review of style pass ③ (2026-09-19) recorded its own, and
  follow that. Dated notes sections (O98's) stay as written.

**Interfaces:**
- Consumes: Task 1 and 2's tree; `web/coverage/coverage-summary.json` from
  `pnpm -C web run test:coverage`; `node scripts/coverage-floor.mjs`.

- [ ] **Step 1: Measure.** Run `pnpm -C web run test:coverage` into a log,
  then `node scripts/coverage-floor.mjs` into a log. Count the report's
  files and how many are under `src/modules/`. The new value is the file
  count less seven.
- [ ] **Step 2: Watch the near miss pass the stale floor.** In
  `web/vite.config.ts`, change `coverage.include` to
  `['src/*.ts', 'src/*.tsx']`, rerun both commands, and record that
  vitest and the floor at the old value both exit 0 with the smaller count.
- [ ] **Step 3: Set the new value**, keeping the near-miss include in place,
  rerun the floor alone, and record it exiting non-zero with its
  `fewer than the … this gate` message. Restore `coverage.include`, rerun
  both commands, and record `… files (floor requires <new>)` and exit 0.
  `git diff web/vite.config.ts` prints nothing.
- [ ] **Step 4: Bring the comment and the docs to the new value.** Write as
  few figures as the sentences need. Each one written is a claim checked
  against Step 1–3's logs. The Rust `minFiles` is not touched.
- [ ] **Step 5: Commit**, then run lint and typecheck into logs.

---

### After the tasks (controller)

Whole-branch review, then the full gate
(`scripts/check-all.sh >| <evidence>/gate.log 2>&1`, quoting its
`all green` line), the PR body, the claim check over status.md, notes.md,
ci.md and the body, then the push and PR asks, each separately.
