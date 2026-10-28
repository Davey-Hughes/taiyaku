# The web tests get their own tree: design

2026-10-28. The user asked, before the second loading PR, for the tests to
stop sitting next to the code files. This spec is how, for the web side.

## 1. The user's rulings (2026-10-28)

1. **Web only.** The Rust crates keep their `foo.rs` + `foo/tests.rs` layout:
   it is Rust's usual one, the tests already sit in a subfolder, and moving
   them out of `src/` would need a `#[path]` on every `mod tests;` or would
   cost them their access to private items.
2. **`web/tests/` mirrors `web/src/`.** Each test keeps its file name and its
   place relative to the tree: `src/reads.test.ts` becomes
   `tests/reads.test.ts`, `src/modules/x.test.ts` becomes
   `tests/modules/x.test.ts`, `src/styles/x.css.test.ts` becomes
   `tests/styles/x.css.test.ts`.
3. **Test-only helpers move too.** `src/testing/` becomes `tests/support/`,
   and `src/browser-setup.ts` and `src/fixtures.ts` (a fake resolver that
   only tests import) move into it. Afterwards `src/` holds no code that
   only tests use.
4. **The source scans read the code they check from `src/` only.**
   (`classNames` also reads the tests' text, as it did on main, for the
   classes a test selects by.) `classNames` and
   `colourLiterals` read a few helpers on main only because those helpers
   sat in `src`; those reads go with the helpers, so each scan reads what
   its own doc comment says it reads. This is the one exception to §2's
   rule that a test keeps reading what it read on main.
5. **The web coverage floor's `minFiles` is re-derived in this PR.** Task
   2's review found it stale on main: with `src/modules` dropped the report
   would still clear it. It goes back to the report's file count less the
   seven-file margin `docs/ci.md` sets, and the near miss it exists for (one
   `*` deleted from `coverage.include`) is watched failing.

## 2. The move

- Every `*.test.ts` and `*.test.tsx` under `web/src` moves with `git mv`, so
  git reads each one as a rename.
- The only edits inside a moved file are the paths that the move breaks:
  - import specifiers (`./reads` becomes `../src/reads`, and from
    `tests/modules/` it becomes `../../src/modules/…`);
  - `vi.mock`, `vi.doMock` and `vi.importActual` paths, which resolve against
    the test file exactly as an import does;
  - reads of a neighbouring file: `?raw` and `?url` imports,
    `new URL(…, import.meta.url)`, `readFileSync` and `readdirSync` paths.
    A path in this group that is wrong can fail in silence (an empty glob, or
    a read that returns a different file), so each one is checked by hand, and
    a test that reads a file must still read the file it read on main.
- Imports of the helpers follow them: `./testing/mount` becomes
  `./support/mount` inside `tests/`. A helper that imports production code
  points at `../../src/…`.
- No path alias is added: none exists today, and relative imports keep the
  change mechanical.

## 3. Config

- `web/vite.config.ts`:
  - each vitest project's `include` and `exclude` read `tests/**` where they
    read `src/**`, and so does the `SHOWN_SCROLLBARS` glob;
  - `setupFiles` points at `./tests/support/browser-setup.ts`;
  - coverage still measures `src/**/*.ts` and `src/**/*.tsx`. Its excludes
    for test files, `src/testing/**` and `src/browser-setup.ts` are dropped,
    because nothing of that kind is left under `src`. The excludes for entry
    points (`main.tsx`, `label.tsx`) and `*.d.ts` stay;
  - `css.include` is unchanged: it names the stylesheets, which stay in
    `src`.
- `web/tsconfig.app.json`: `include` becomes `["src", "tests"]`, so the
  type-check covers the same files as today.
- `web/package.json`: `lint` becomes `biome check ./src ./tests`, and
  `format` names `./tests` beside `./src` the same way. Each names `./src`
  alone today, so without this the tests would stop being linted and
  nothing would say so.
- `.gitignore`: the line for the browser runs' failure screenshots names
  `web/tests/__screenshots__/`, where vitest now writes them, beside the
  test file.
- Unchanged in what they do, and the plan confirms each: the per-file
  floors in `scripts/coverage-floor.mjs` (keyed on `web/src` source files,
  which do not move; ruling 5 re-derives its `minFiles` separately), the
  Dockerfile (it copies the whole of `web/`), `web/biome.jsonc` (it has no
  include list), and `scripts/check-all.sh` and the Forgejo workflows (they
  call the `package.json` scripts; `check-all.sh` changes only in
  comments).

## 4. A guard against a test left in `src`

Once the projects look only in `tests/`, a new `src/foo.test.ts` written from
habit would never run, and nothing would say so. One test in `tests/` fails
if any `*.test.ts` or `*.test.tsx` exists under `web/src`, naming each one it
finds. It runs wherever the suite runs, so CI needs no new step. Its own
proof is a watched failure: put a test file in `src`, see the guard go red
and name it, then take the file away.

## 5. Docs

- Live docs are brought up to date: `docs/ci.md`'s account of the vitest
  projects and their globs, `web/README.md`, the path-qualified mentions in
  `docs/status.md` and `docs/notes.md`, and path-qualified mentions in code
  comments and scripts. A mention that gives a bare file name is still right
  and stays as it is.
- Dated specs and plans under `docs/superpowers/` are left as written: they
  record the tree as it was when they were written.
- `docs/status.md` gets the line this PR's work needs under the
  open-items ritual.

## 6. How we know nothing changed

1. **The same tests run.** For each of the four vitest runs (node, jsdom,
   browser, browser with scrollbars shown), the sorted list of test names
   (file name, then the full test name) on the branch equals main's, with the
   guard of §4 as the one addition. A list is compared, not a count.
2. **The same code is reached.** Per-file line and branch coverage of
   `web/src` on the branch equals main's, file by file (the lcov diff of
   #149), except that `fixtures.ts` leaves the report: on main it was
   counted as source only because it sat in `src`. A test that moved but
   stopped reaching its code shows here.
3. **The moves are moves.** `git diff -M main` reports every moved file as a
   rename, and a moved file's changed lines are path lines only.
4. **Lint and the type-check read `tests/`.** A deliberate type error and a
   deliberate lint error in a moved test each turn their check red, and are
   then taken out.
5. **The gate is green**: `scripts/check-all.sh` ends `all green`.

## 7. Out of scope

- Reorganising `web/src` itself into folders.
- The Rust test layout (ruling 1).
- Renaming test files, or changing how a test picks its vitest project (the
  filename suffix still does that).
