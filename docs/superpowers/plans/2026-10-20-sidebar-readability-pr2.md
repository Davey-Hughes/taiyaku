# Sidebar and readability, PR 2 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A version label (the short commit) at the sidebar's foot, an About page fed by `GET /api/about`, a licence page with texts bundled from their files, and a copy button for the OPDS catalog address on About and Account.

**Architecture:** The server reads `option_env!("TAIYAKU_COMMIT")` at compile time and answers `GET /api/about` from the constants that write each format. The Dockerfile and CI pass the commit in as a build argument. The web client asks `/api/about` once per page load (`about.ts`); the label, the About page and the catalog address all read that one answer. The licence page imports its texts with `?raw` behind a dynamic `import()`, so they ship in a chunk of their own.

**Tech Stack:** Rust (axum, `taiyaku-serve`), React 19 + react-router, vitest (`node`, `jsdom`, `browser` projects in `web/vite.config.ts`), Docker buildx, Forgejo Actions.

**Spec:** `docs/superpowers/specs/2026-10-18-sidebar-readability-about-design.md` §4, §6 (About), §7. **Branch:** `about-licence` in the main checkout `/home/davey/projects/taiyaku`, cut from `main` at `df7bfafb`. No O-number is open for this; the status entry is the Later one, "Next: a version label, an About page and a licence page".

## Global Constraints

- The commit is the full 40-character lowercase hex hash. The label shows its first seven characters. CI tags the image with the same seven, as `sha-<7>`. Any other value of `TAIYAKU_COMMIT` (unset, empty, short, not hex) counts as no commit, and the build reports none.
- Nothing runs git during a build. `.git` stays excluded from the image build (`.dockerignore:4`).
- The repository URL is `https://github.com/Davey-Hughes/taiyaku`. It has one source, `[workspace.package] repository` in the root `Cargo.toml` (`:17`, today `https://git.daveynet.xyz/davey/taiyaku`), which reaches the server as `env!("CARGO_PKG_REPOSITORY")`. The links are built even though the URL answered 404 anonymously on 2026-10-18.
- `/api/about` is behind the session guard like the rest of `/api`.
- Muted text keeps the 14px floor (`--text-meta`, `--muted`; `web/src/styles/tokens.css:96, 119`). The reader looks as it does on `main`.
- Every new test is watched failing first, against the defect it names, hand-applied where the code already passes. The report says how each test was made to fail.
- A whole-page `toContain` cannot see which text belongs to which section, so assert per element. Real-key browser tests focus the page first, and are also run alone with `-t`.
- Comments say why: the reason, the invariant, what breaks. No history.
- Gates run whole projects. For Rust: `NEXTEST_TEST_THREADS=8 cargo nextest run --workspace >| <log> 2>&1`, then clippy and fmt. For web: `pnpm -C web exec vitest run --project <p> --reporter=default >| <log> 2>&1`. Read each summary with `grep -a`, and never pipe a gate through `tail`. A browser run with import/connection errors and no failed assertion is the headless Chromium crash: check `coredumpctl list --since -1h` and rerun the whole project once.
- Lint (`pnpm -C web run lint`) adds no warning to `main`'s count. Run typecheck with the script in `web/package.json`. Coverage: aggregate and per-file floors (`scripts/coverage-floor.mjs`) for both languages. Report any floor that moves.
- Never `cd` (it is zoxide here). Use absolute paths, `git -C` and `pnpm -C web`. Logs and scratch go in the session's scratchpad, never the working tree.
- Commits: one or more per task. The message says what the code now does. No attribution lines.

## Readings of the spec (each confirmed by the user, 2026-10-20)

1. **The label with no commit.** §4 says the label is "the short commit (or "unknown build")" and is "the way to About for every account". §6 says "the label and the commit link are drawn only with a commit". This plan reads §6 as being about the commit text and the About page's commit link. The label is drawn whenever `/api/about` has answered, and reads "unknown build" when the commit is null.
2. **The notes file's name.** The spec writes `taiyaku.notes.json`. The status row for `taiyaku-serve` writes `<stem>.notes.json`. The format table uses the names the code writes. The Later entry "A version field for `taiyaku.notes.json`" is corrected the same way if the code says so.
3. **The catalog path comes from the server.** `/api/about` returns `catalog`, built from the OPDS module's version constant. The web prepends `location.origin`, so the web never writes `/opds/v1.2/catalog` itself.

## Before Task 1

- [ ] Cut the branch. Run `scripts/check-all.sh` on it into a log, and record its `all green` line and exit status. This baseline is how a later red is called new or pre-existing. If it is red, stop and report.

---

### Task 1: `GET /api/about` (spec §4)

**Files:**
- Create: `crates/taiyaku-serve/src/about.rs`, with its tests laid out the way `session/tests.rs` is
- Modify: `crates/taiyaku-serve/src/lib.rs` (the module); `routes.rs` (register at `guarded_routes()` `:2097`, beside `/api/me` at `:2126`; a `GrantIndependent` posture in `grant_postures()` `:2426`, shaped like `/api/me`'s at `:2489-2494`); `auth/tests.rs:61` (the hand-typed route list); `opds.rs` (a version constant; its `routes()` is at `:866`); `crates/taiyaku-library/src/progressdoc.rs:44` (`VERSION` becomes `pub`); root `Cargo.toml:17` (`repository`)
- Read first: `session.rs:1198` (`whoami`, a guarded `json!` handler) and `session/tests.rs:413` (how it is driven). `test_support.rs:819` (`router_over_a_library`, a real session cookie) and `:1927` (`get_json`).

**Interfaces:**
- Produces: `opds::VERSION: &str = "1.2"`; `about::build_commit(raw: Option<&'static str>) -> Option<&'static str>`; and the response below. This plan is the first place the shape exists. Every field is always present.

```json
{
  "commit": "<40 lowercase hex>" | null,
  "repository": "https://github.com/Davey-Hughes/taiyaku",
  "catalog": "/opds/v1.2/catalog",
  "formats": [
    { "name": "taiyaku.json",                           "version": "1" },
    { "name": "<as written>.library.json",              "version": "1" },
    { "name": "<as written>.progress.json",             "version": "1" },
    { "name": "<as written>.truth.json",                "version": "1" },
    { "name": "<as written>.regions.json",              "version": "1" },
    { "name": "Database schema",                        "version": "17" },
    { "name": "OPDS, with the Page Streaming Extension", "version": "1.2" },
    { "name": "<as written>.notes.json",                "version": null },
    { "name": "HTTP API",                               "version": null },
    { "name": "wasm module",                            "version": null }
  ]
}
```

The rows come from these sources, in this order: `taiyaku_core::VERSION`, `taiyaku_library::librarydoc::VERSION`, `taiyaku_library::progressdoc::VERSION`, `taiyaku_core::TRUTH_VERSION`, `taiyaku_core::REGIONS_VERSION`, `taiyaku_library::schema::SCHEMA_VERSION` and `opds::VERSION`. Each version is serialised as a string. "As written" means the file's name the way its writer builds it, with the varying prefix shown as the writer names it (e.g. `{stem}.truth.json` is shown as `<stem>.truth.json`). Read each writer to get the name, and list the names in the report. `catalog` is `format!("/opds/v{}/catalog", opds::VERSION)`.

- [ ] **Step 1:** `build_commit` tests. `None`, `Some("")`, a 7-character hex, a 40-character string with one non-hex character, and a 40-character uppercase hash each give `None`. A 40-character lowercase hex gives itself. Watch each test fail with the filter removed.
- [ ] **Step 2:** The route test. It goes through `router_over_a_library` and `get_json`, and asserts the whole body exactly: `commit` equals `build_commit(option_env!("TAIYAKU_COMMIT"))`, `repository` equals the URL above, `catalog` and every row are as listed, and each version equals its constant's `to_string()`. String literals and `json!` keys get no mutant (see the mutation memory), so only an exact assertion holds them. Add the route to `auth/tests.rs:61` and to `grant_postures()`. Watch `every_route_refuses_an_anonymous_request` fail when the route is registered outside the guard, and `every_route_declares_its_grant_posture` fail with the posture missing.
- [ ] **Step 3:** An `opds` test that every path in `opds::routes()` starts with `/opds/v{VERSION}/`, and that `catalog`'s path is one of them. Watch it fail with `VERSION` set to `"1.3"`.
- [ ] **Step 4:** Grep the tree for the old `git.daveynet.xyz` URL and report each hit. Change only `Cargo.toml`. Docs that name it as history stay as they are.
- [ ] **Step 5:** Run the Rust gate whole (nextest, clippy, fmt), then commit. Run `scripts/mutants.sh --since main` after the commit, because it reads commits, not the worktree. Report caught, missed and unviable.

**Known blind spot (state it in the report):** the five file formats are all at version 1, so a swap between two of their constants gives the same body. No value test can see it. The reviewer checks the pairing by reading.

### Task 2: The label, `about.ts` and `/about` (spec §4)

**Files:**
- Create: `web/src/about.ts`, `web/src/VersionLabel.tsx`, `web/src/About.tsx`, with tests beside each in the project that fits (jsdom, or browser for the drawer)
- Modify: `web/src/Shell.tsx:359-362` (the label goes in `.sidebar-foot` after `<AccountMenu>`; the column and the drawer are the same element, `:339-363`), `web/src/Routes.tsx:67-130` (`/about` as `<Shell><AboutScreen/></Shell>` with `useDocumentTitle('About')`, shaped like `AccountScreen` at `:60-64`), `web/src/styles/shell.css` (the label, near `.sidebar-foot` `:45`), and the CSS that styles a page like Account's
- Read first: `api.ts:68, 105-109` (`request`, `getJson`, 401 handling); `Shell.tsx:115-135` (the cancelled-guard fetch pattern); `AccountMenu.tsx:49, 58-69` (`INSIDE`, and `data-here` on a `NavLink`); `classNames.test.ts:94-101` (every class needs a rule)

**Interfaces:**
- Consumes: Task 1's response.
- Produces: `type About = { commit: string | null; repository: string; catalog: string; formats: { name: string; version: string | null }[] }`. `about(): Promise<About>` makes one request per page load once it succeeds; a failure is not remembered, so the next caller asks again. `forgetAbout()` exists for tests. `shortCommit(commit: string): string` returns the first seven characters. Tasks 3 and 4 use these.

- [ ] **Step 1:** `about.ts` tests. A second call after a success makes no request. A call after a failure makes a new one. A 401 goes through `request`'s `onUnauthenticated`. Watch each test fail against its defect (no memo; a memoised rejection).
- [ ] **Step 2:** `VersionLabel` tests. Nothing is drawn before the answer, or after a failure. Once the answer arrives it is a link to `/about`: the text is `shortCommit(commit)` with accessible name `About, build <short>`, or "unknown build" with `About, unknown build`. It is current (`aria-current`, drawn in the accent like More's `data-here`) on `/about` and `/about/licence` and nowhere else. More is not marked on `/about`, because `INSIDE` does not list it. In the drawer, clicking the label navigates and closes the drawer, the same as a library link. Shell's tests mock modules, so mock `about.ts` the same way. Reserve the label's line in CSS so the foot does not shift when the answer arrives.
- [ ] **Step 3:** `About` page tests. With a commit, the short commit links to `<repository>/commit/<full hash>`. Without one, "unknown build" is plain text and there is no commit link. Source links to `<repository>` and Issues to `<repository>/issues`. The format table has one row per `formats` entry, in order, and a null version reads "unversioned". Assert each row's cells. A failed answer shows the error the way other pages do. The catalog address is shown as text here; Task 3 adds the button.
- [ ] **Step 4:** All three web projects whole, lint, typecheck, coverage. Commit.

### Task 3: The catalog address and its copy button (spec §4)

**Files:**
- Create: `web/src/CatalogAddress.tsx`, with a jsdom test
- Modify: `web/src/About.tsx`, `web/src/Account.tsx:295-298` (beside the OPDS line; Account renders as the page and as the reader's panel through `LOOKS` `:69`, and the address appears in both), `web/src/Account.test.tsx` (its local `server(routes)` at `:20-28` answers 404 to anything unlisted)

**Interfaces:**
- Consumes: `about()`, and `About.catalog`.
- Produces: `<CatalogAddress catalog={string} className?={string} />`.

- [ ] **Step 1:** Tests. The address `location.origin + catalog` is always drawn as selectable text. The Copy button is drawn only when `window.isSecureContext` is true and `navigator.clipboard` exists; test each half missing separately. A click calls `writeText` with exactly the address, and a polite live region says "Copied". A refused write says "Not copied" and throws nothing. Watch each test fail with its guard removed.
- [ ] **Step 2:** About and Account use it. Account's existing tests stay green, with `/api/about` added to their server where a test needs the address. Account with no answer from `/api/about` draws the OPDS line without an address.
- [ ] **Step 3:** All three web projects whole, lint, typecheck, coverage. Commit.

### Task 4: `/about/licence` (spec §4)

**Files:**
- Create: `web/src/licenceTexts.ts` (four `?raw` imports: `../../LICENSE`, `@fontsource/shippori-mincho-b1/LICENSE`, `@fontsource/zen-kaku-gothic-new/LICENSE`, `./styles/fonts/OFL.txt`; both packages export `./LICENSE`), `web/src/Licence.tsx`, with a test
- Modify: `web/src/Routes.tsx` (`/about/licence`, title 'Licence'), `web/src/About.tsx` (a link to it), `web/vite.config.ts:229-232` (`server.fs.allow` gains the repo-root `LICENSE` file, and nothing wider; the dev server and the browser project serve through it), `Dockerfile` web stage `:36-46` (`COPY LICENSE /src/LICENSE` after `COPY web/ ./`; the stage's WORKDIR is `/src/web`, so `../LICENSE` resolves there)

- [ ] **Step 1:** `Licence.tsx` loads `licenceTexts.ts` by dynamic `import()`, so the texts stay out of the entry chunk. It shows: taiyaku is GPL-3.0-or-later, then the full text; then the fonts, each OFL-1.1 with its own text. The three fonts are Shippori Mincho B1, Zen Kaku Gothic New, and the marks subset (cut from Zen Kaku Gothic New, `tokens.css:52-62`). There is a link up to About, written the way the Libraries folder page writes its way up. Long lines wrap: no horizontal page scroll at 390px.
- [ ] **Step 2:** Tests, per section. The GPL section holds "GNU GENERAL PUBLIC LICENSE" and "Version 3, 29 June 2007". Shippori's holds "Shippori Mincho Project Authors". Each Zen section holds "The Zen Project Authors". Watch each test fail with its import pointed at another file. **Known blind spot:** `OFL.txt` is byte-identical to Zen Kaku Gothic New's `LICENSE`, so a swap of those two renders the same. State it in the report.
- [ ] **Step 3:** `pnpm -C web run build`. "29 June 2007" is in exactly one file under `web/dist/assets`, and that file is not the entry chunk named in `web/dist/index.html`. Quote both greps.
- [ ] **Step 4:** All three web projects whole, lint, typecheck, coverage. Commit.

### Task 5: The commit through the build (spec §4)

**Files:**
- Modify: `Dockerfile` server stage `:49-60` (`ARG TAIYAKU_COMMIT` immediately before the `RUN … cargo build`, so the layers above keep their cache; a why-comment saying `option_env!` reads it at compile time and nothing runs git); `.forgejo/workflows/ci.yml` docker job `:501-622`

- [ ] **Step 1:** CI. Both `docker buildx build` calls, the probed `--load` one at `:562` and the `--push` one at `:621`, get `--build-arg TAIYAKU_COMMIT=<full sha>`. Take the sha from `${{ github.sha }}` in each step's `env`. The arg must be on both calls: the push rebuilds, and a push without it would publish a different image from the one probed. In the probe step (near `:583`), assert that the image's binary holds the full sha: `docker run --rm --entrypoint grep taiyaku-ci:$SHORT -qa "$SHA" <binary>`. Read the binary's path from the runtime stage. The docker job only runs on a push to main, so nothing on the branch exercises these lines; Step 2 is their only check before merge.
- [ ] **Step 2:** Build the image locally: `docker buildx build --load --build-arg TAIYAKU_COMMIT=$(git -C /home/davey/projects/taiyaku rev-parse HEAD) -t taiyaku-about:check .`. The binary holds the sha (the same grep as CI). The image's web assets hold "29 June 2007" in a non-entry chunk. If docker or buildx is missing, stop and report. Do not skip it. Remove the `taiyaku-about:check` image afterwards.
- [ ] **Step 3:** Commit.

### Task 6: Docs, the built app, and the gate (spec §6, §7)

**Files:**
- Modify: `README.md` (`:160-161` OPDS address; `:306` the account screen; `:316-323` the browser routes; `:409-428` running on a server; `:650-652` Licence), `docs/status.md` (the Later entry at `:5545-5549`, and the notes-file name if Reading 2 holds)

- [ ] **Step 1:** README. The catalog address is on About and Account, with a copy button where the browser allows it. Add `/about` and `/about/licence`. The image's label is its commit. A local `docker buildx build` passes `--build-arg TAIYAKU_COMMIT=$(git rev-parse HEAD)` and a local `cargo build` sets `TAIYAKU_COMMIT` the same way; without it the label reads "unknown build". The licence section names the fonts' OFL-1.1 and the page.
- [ ] **Step 2:** `docs/status.md`. The entry "Next: a version label, an About page and a licence page" becomes what shipped, shaped like PR 1's entry above it. It records the repository URL change and that the links work once the GitHub repository is public.
- [ ] **Step 3:** Run `scripts/check-all.sh` into a log. Quote its `all green` line and its exit status.
- [ ] **Step 4:** By eye, on the built app (production CSS differs from dev, O218), served from a scratch copy per the live-server memory. Build it twice: once with `TAIYAKU_COMMIT` set and once without. Check both themes at desktop width and at 390px: the sidebar foot with the label in the column and in the drawer, beside a `main` build; `/about` with a commit and with none; `/about/licence` (wrapping, no sideways scroll); and Account's address. Then open the page over the machine's LAN address on plain http, which is not a secure context: there is no Copy button, and the address is selectable. Screenshots go in the scratchpad, and the report names what each one shows.
- [ ] **Step 5:** Commit.

The controller then runs a whole-branch review that also reviews its own fixes, and the claim check over the status entry, the README and the PR body.
