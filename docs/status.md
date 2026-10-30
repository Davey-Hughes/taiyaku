# Status

Where the project is, what is settled, and what comes next. Updated 2026-08-13.

The detailed evidence for everything asserted here is in [`notes.md`](notes.md),
which is a working log rather than a summary — every claim below traces to a
measurement recorded there. The format is specified in
[`sidecar.md`](sidecar.md).

## Where it stands

**The server has accounts.** Spec A2 shipped on 2026-08-18, and **four
whole-branch reviews of it found fifteen things each** — a rate limit the spec
required on the `Authorization: Basic` path that had never been written, a claim
route that would take an empty password for the server's only administrator, an
enumeration test whose floor sat six routes below what shipped while reading
nothing at all of the exemption list it was credited with policing, and, in the
fourth round, a claim route open on a library its owner had already set up.
Every one is fixed on the branch; `notes.md` has the reproductions.

**The rate did not fall across the four rounds, and that is the finding about
the process rather than about the code.** Roughly a quarter of each round came
from the round before it — the clearest being the decoy hash added in round two
to close a timing oracle, which turned a documented and until-then-harmless
rate-limiter gap into an unauthenticated denial of service that round four
found. The rest was new ground each time, and the findings got deeper rather
than repeating: round one was structural (missing guards, wrong status codes),
round four semantic (two functions disagreeing about what "claimed" means).
Fifteen is what a round *reports*, not what exists. Nothing tracks which
invariants have been checked — coverage measures which lines ran — so each
reviewer re-samples the whole branch. A fifth round was not commissioned for
that reason; mutation testing is what measures whether a test can see a defect,
and it is the next thing on this branch's list rather than another reader.

What shipped: every `/api` and
`/opds` route resolves to a `Principal` before it runs, from a session cookie or
from `Authorization: Basic` — a password for a browser, a `tyk_`-prefixed API key
for an OPDS client, and the prefix is what keeps a 400-page volume from paying
argon2 four hundred times. Passwords hash with argon2id; session tokens and API
keys are 256-bit CSPRNG secrets stored as sha256 and never held raw. The two
hardcoded `LOCAL_USER = 1` constants are gone, so two readers of one work hold
two positions and two sets of notes, which the database could always express and
the server could not. `taiyaku library user` and `library key` administer it, and
promotion reaches the row spec A seeded rather than inserting beside it —
inserting would have orphaned every note recorded since, silently. What it
deliberately does not do is in §11 of its design; what it left open is O54
*(O54 has since closed — see its entry above)*, O55 *(also since closed, by
spec C2's second half: Panels does accept a `tyk_` key)* and O57 *(since
closed too, on 2026-10-12 — see its entry above)*.

**L0 works.** The aligner reads two editions, pairs their pages, and writes a
sidecar. It has been measured against eleven series and holds up on every case the
design anticipated, plus several it did not.

**`taiyaku serve` with no paths serves the whole library.** Spec C1 §5.3: the reader is handed sidecar paths on argv and a scanned work has none, so a browse UI over a derived library was a grid of covers that opened nothing. `works::from_library` projects every row instead, and a work with no `sidecar_path` is served from rows and corrected into rows — `patch_alignments` writes the file first *and only when there is one*, which for a scanned work means the correction's only copy is in the database until somebody runs `library export`. That cost is stated rather than solved; minting a `taiyaku.json` would have made the server a writer into a tree spec B never opens for writing.

**The reader now has a front end.** Milestones 1 through 3 shipped: page
reading is a crate of its own, `taiyaku serve` hands an HTTP client the
sidecar, any page by index, and where you were, and it can now read those pages
from a Stump instance as well as local disk and publish an OPDS feed grouping a
work's editions. M5a shipped the browser reader that draws it: single-page,
two-page spread and continuous-scroll modes, a swap between editions that
never writes position, a gap state that names an unpaired slot rather than
silently landing near it — *no longer the default since `gap-fallback`, marked
here rather than rewritten: another edition's page is drawn there and that gap
is one press of its mark away, per the "Fall back to another edition…" entry
below* — and a floating chrome — filmstrip and slot readout — that never
reflows the page underneath. M5d shipped the correction UI: a reader can point
at the right counterpart in the filmstrip and have the fix
written into the sidecar as `by: "hand"`, surviving a re-solve. `verify`'s
contact sheet remains read-only, but it is no longer the only way to see or
change what the aligner decided. A defect M5a had carried since it shipped
closed just after: an arrow steps by what is *drawn* — a group — rather than
by slot, so crossing a joined spread no longer takes two presses and a swap
always answers from the half a reader can see. **M5c shipped annotations, and
with them the one thing no other reader can offer: a note written on the raw
is readable while reading the English**, resolved through the same call the
swap makes, marked in the margin, and listed — including the notes that cannot
be reached from where the reader is standing, which are marked rather than
hidden.

**L1 shipped zoom, and with it the first thing the alignment does for a reader
that a side-by-side viewer cannot do at all.** A reader zoomed into a speech
balloon on the raw presses the swap and lands on the same balloon in English:
the rect they are looking at is mapped onto the other edition's page and the
viewport fitted around the result. A page turn carries the view and re-clamps;
continuous scroll is untouched and keeps fit-whole. **Which transform does the
mapping was measured rather than argued, and the measurement went against the
design**: of the two candidates it named, one turned out worse than doing
nothing, and what ships is a third neither had proposed. §5 below is the record.
Nothing in the sidecar, the aligner or the server changed to make it work.

**Three reader complaints landed on top of it, and the browser corrected two
designs on the way.** Pages fill their box now, the two halves of a spread meet
at the seam, and `?` or `h` lists every key the reader answers — generated from
the same table `intentOf` dispatches from, so there is no second copy to drift.
§6 below is the record, and the parts worth reading are the two failures: the
seam's design closed the gap on the wrong element and a measurement said so, and
closing it introduced a regression that put every annotation marker 376px from
its artwork until the same pass caught that too.

**Two more came from reading in two-page mode, and both were rules that were
right about one page and had never been asked about two.** A pair's halves each
appeared as its own decode settled — measured at a median 89.7 ms apart, every
turn tearing — and they now present together through one barrier. And the two
editions on either side of a swap drew pairs one page out of phase, because
pairing was anchored at slot 0 of each edition independently. **Pair boundaries
are now a property of the work rather than of an edition**: wherever any edition
joins a spread, that boundary is mapped into the hub's slot space, unioned with
every other edition's, and every edition — the hub included — takes its pairing
from the one grouping that results. Swaps that keep the pair intact go from 206
of 580 to 581 of 590. §8 below is the record, and it has two parts worth
reading. What the defect was *not*: the obvious diagnosis was an alignment
error, and a shift test over 12 alignments in 9 volumes says the aligner has
none — after the scan itself was corrected for the one-to-one constraint that
had made four volume tails look broken. And what the first fix missed: deriving
only the *spokes* from the hub scored 96% and left the hub unable to follow a
boundary only a spoke has, which a reader found at one press.

**It can be deployed.** `taiyaku serve --web DIR` serves the built web app
beside the API, so one process and one port serve both; a path no route
matched is the app's, except under `/api` and `/opds`, which keep their 404.
The repository builds an image (`Dockerfile`), CI pushes it to the forge's
registry on every merge to `main` whose pipeline passes, after running it,
and `README.md`'s "Running it on a server" is how it is meant to run on the
NAS. Spec
`2026-10-04-day-to-day-deployment-design.md`, Part B.

**The store keeps libraries.** A library is a name and one or more directories
(migration 15; migration 16 strips a name of all surrounding white space and makes names unique regardless of case), and a grant names a library rather than a directory. Derivation
and proposals stay inside one library: the same title in two libraries is two
series, and a byte-identical volume in two is two works. `taiyaku library
libraries` lists them, `library scan --add PATH --library NAME` files a directory
under one, and `library user grant-library` grants one. Configuration →
Libraries makes, renames, scans and removes them, and moves a directory between
them (Parts F2a and F2b).
A library can be removed, and its last directory removed or moved away, even
where it is some account's only grant; that account then sees no library
until an administrator grants one, and `library user every-library` lifts a
limit (O220).
Every account's sidebar lists the libraries it may read, each with a page of
its own that narrows the grid and the continue strip; search narrows to one;
and OPDS lists the libraries twice, by the directories each holds and by the
library the store files each work in (Part F3). Spec
`2026-10-05-libraries-design.md`.

## What is built

Six crates, 2807 tests, plus 4,337 in `web/`'s TypeScript — 1,667 in the `node`
project and 2,670 in the `jsdom` one — and one thousand and thirteen more in a
**browser** project of its own, no dependencies beyond
`image`, `zip`, `sha2`, `rayon`, `base64` and `serde`, plus `axum`, `tokio`,
`tower`, `tower-http` (feature `fs`, for `--web`) and `ureq` in the server, `rusqlite` (features `["backup"]`, against
the system libsqlite3 rather than bundled — measured at 1 m 43 s to build
bundled against 3.06 s on a 32-core machine, against a CI with no cargo cache
and a ten-minute job budget) in the new `taiyaku-library` crate, which
`taiyaku-serve` also names directly for the slug-to-row bridge, `argon2` for
spec A2's password hashing — the one genuinely new crate on that branch,
measured at 1.70 CPU-seconds for it and its five transitive dependencies
against 138.85 for a clean workspace build, with no inference drawn about CI's
budget — and `regex` in
the same crate for the scanner's rule layer's `regex`-kind rules (§4) — and
`zip` in it too since spec C3 §6's ComicInfo write-back, already in the tree
through `taiyaku-read` and named directly because this crate's own code now
rewrites an archive rather than only reading one — no
backtracking, so a pattern typed into C4's text box cannot produce
catastrophic behaviour, and already in `Cargo.lock` at 1.13.1 pulled in
transitively, so naming it here adds a dependency edge rather than a new
crate to build — and `ignore` in it since O92's scanner half, for applying
`root.exclude_globs` to the walk: 0.4.33, and **three new crates rather than
the ten `cargo add` suggests** — `ignore`, `globset` and `bstr` — measured
against this workspace's lock on 2026-09-09, since `notify` already supplies
`walkdir`, `same-file`, the `crossbeam-*` trio and `winapi-util`; `globset`
alone would have cost two of the same three and cannot express negation, and
a hand-rolled matcher would have been the second, drifting reimplementation
`rules.rs` already argues against — and `notify` in the same crate again, for §8 of the scanner
spec's watch probe: 8.2.0, verified with `cargo add notify --dry-run` on
2026-08-17, and unlike `regex` this one is genuinely new rather than already
in `Cargo.lock`, bringing a small subtree with it. **Four crates on Linux**,
re-measured 2026-08-18 against the merge base: `notify`, `notify-types`,
`inotify` and `inotify-sys`. The rest of its subtree — `bitflags`, `libc`,
`log`, `mio`, `walkdir`, `same-file` — was in the lockfile already. Seven new
package *names* appear in `Cargo.lock` in total, the other three being
`fsevent-sys`, `kqueue` and `kqueue-sys`, which no Linux build compiles; the
Windows entries in that diff are second versions of crates already present,
not new names. This said "sixteen crates on Linux" and no reading of the
lockfile or of `cargo tree` gives sixteen. Measured rather than assumed: a clean
`cargo build --workspace --all-targets --all-features` on this same 32-core
machine cost about 6% more CPU time with it than without (262 against 248
CPU-seconds) — not run through CI itself, and a 32-core box parallelises a
build differently than the runner CI actually uses, so that is what was
measured and nothing is inferred from it about the ten-minute budget —
`wasm-bindgen` in the binding, `react`, `react-router` and `vite` in `web/` —
`react-router` (8.3.0) **the first runtime dependency beyond React since the
project began**, kept over a fifty-line hand-rolled router that spec C1 §13
had also weighed, a decision closed 2026-08-20 rather than left open, for
scroll restoration, nested routes and navigation blocking, all things this UI
wants and all things tedious to get right twice. Measured on `pnpm build`:
the shipped bundle moved from 294.37 kB raw / 94.54 kB gzip to 337.22 kB raw /
109.30 kB gzip, **+42.85 kB raw / +14.76 kB gzip**, of which `react-router`
alone cost +37.59 kB raw / +13.16 kB gzip measured in isolation when it landed
(Task 7), and `tract-onnx` as a dev-dependency of `taiyaku-cli` alone, for
phase 3's gated measurement — it reaches no binary and no other crate. The
style pass's first PR added `@fontsource/shippori-mincho-b1` and
`@fontsource/zen-kaku-gothic-new`, which are fonts and CSS rather than code.
In apparent bytes, the built `web/dist` held 604,783 before them, its CSS
asset 16,306 bytes, and 34,294,658 with them: a 713.98 kB CSS asset (312.31 kB
gzip), JS essentially unmoved — `main` 191.45 kB (58.04 kB gzip), `label`
10.87 kB (4.29 kB gzip), the shared chunk 215.92 kB (68.92 kB gzip) — and the
growth 570 `.woff2` font files totalling 14,841,516 bytes and 574 `.woff`
fallbacks totalling 18,148,448. The same PR then strips the `.woff` fallbacks
from the build, since every browser that can draw the tokens loads woff2:
`web/dist` now holds 16,005,973 bytes, with no `.woff` file and a 573.60 kB
CSS asset (243.22 kB gzip). The thirty-two run under Vitest's browser mode
over Playwright's
Chromium, and cover only what no Node environment can answer: `jsdom` has no
layout engine and no real `HTMLImageElement.decode()`, and every defect the page
area has shipped was one or the other. They are **not** a CI job — a cacheless
CI would download a browser on every run — so `pnpm -C web run test:coverage`, which
is what CI and `check-all.sh` both run, stays the `node` and `jsdom` projects, and
`check-all.sh` runs the browser one separately and locally, naming it
unmeasured when the browser is absent. That adds `@vitest/browser`,
`@vitest/browser-playwright` and `playwright` to `web/`'s dev dependencies;
the runtime list above is unchanged. That list did not grow for L1: zoom is a CSS transform and
a page of arithmetic, and its corpus experiment fingerprints with `image` and
`rayon` exactly as `align` already did. Twenty-four of the 2807 are standing checks gated on
`TAIYAKU_CORPUS` — seventeen in `crates/taiyaku-cli/src/corpus.rs`, five in
`crates/taiyaku-serve/src/corpus.rs`, two in
`crates/taiyaku-library/src/corpus.rs` (this read "nineteen … ten … five"
until 2026-09-09, which was never arithmetic that worked and never named the
library's two; measured off the corpus run rather than recounted) — two of those added here, over spec C2's
own material: a real multi-member edition downloading as a readable zip, and a
real single-member edition served byte for byte — two in
`crates/taiyaku-cli/src/score_corpus.rs` and two in
`crates/taiyaku-library/src/corpus.rs` — the newest of those checking the
scanner itself, walk through derive, over the corpus's settled material
(whole-branch review's I9) — one of which also needs
`TAIYAKU_STUMP_URL` and `TAIYAKU_STUMP_KEY`; each skips cleanly when what it
needs is absent, so CI stays green without the corpus or a reachable Stump.
One measures composition
against a direct solve — 2,427 page comparisons, 0.91% loss, 0.00% disagreement.
One measures that the server and the aligner agree about what page N is — 617
pages across 3 editions, no disagreement. One measures that a note can be anchored to
every slot of every real edition — 629 anchors across three, two of which join
spreads and so have more slots than pages, which is the case a four-page
fixture cannot tell apart from a page check. One measures that the server and
Stump serve the same bytes for a member both hold — 187 pages compared byte-for-byte,
no disagreement, and 8 members this Stump instance does not hold recorded as
absent rather than silently skipped. Two pin credit-page classification against
both fan editions the corpus holds — Go-Toubun's four pages and Kaguya's single
known false positive, asserted as *sets* rather than counts, since four of six
is reachable by classifying the wrong four; see §4 below. The remaining six run
duplicate resolution over real duplicate sets and are listed in §2 below with
what each one measured. `check-all.sh` runs all twelve with `--nocapture`
against an absolute, canonicalised `TAIYAKU_CORPUS`, so a green gate shows what
each one measured, or says plainly that it did not run them at all. **It runs
them under `--release`**, and its `test` job runs with `TAIYAKU_CORPUS` removed
from the environment so they run once rather than twice — the whole gate is
about 15 seconds warm, against a run that was killed at 1h09m before that split.
The debug profile is the only one with overflow checks and `debug_assert!`, so
the unit tests are what exercise those and `--corpus-debug` puts the corpus
checks back under them; `docs/ci.md`, "Two profiles, and the gate that took an
hour", is the record.

**Seven further tests are gated on `TAIYAKU_CORPUS` and are deliberately not
among those twelve.** All seven are one-off measurements rather than checks, and
all seven are built the same way for the same reason: gated, skipping cleanly
when the variable is unset, calling the detector's own functions rather than
copies of them, and asserting only that they measured something — never a
quality figure, because a check that fails when the thing it measures improves
is not a measurement. **Three of the seven are phase 3's**, all in
`crates/taiyaku-cli/src/model_experiment.rs` and additionally gated on
`TAIYAKU_MODEL`: the head-to-head table against `detect_page`; — added
2026-08-12 — the measurement of whether the model groups a page's balloons the
same way on the raw and on the English edition, which is a pairing question
rather than a labelling one and reads no truth boxes at all; and, the same day,
a sweep of the model's confidence floor `BLK_CONF` over eight values, which is
`band_sweep.rs`'s shape applied to a constant taken verbatim from upstream and
never measured here. All three are described in the L2 entry below. This
sentence said "four" until the second landed, having missed the first one; the
count is not gate-checked, only the two totals above are. Two more are
described below; the remaining two are L2b's, and are
`crates/taiyaku-cli/src/truth_population.rs`, which measures what text is made
of from inside 478 hand-drawn truth boxes, and
`crates/taiyaku-cli/src/band_sweep.rs`, which scores the detector at eight
values of `BAND`'s `lo` over all 45 labelled pages and is what fixed it. Both
read their pages through `score::resolve_labelled`, so they measure exactly the
pages `taiyaku score` scores. `notes.md`, "What text is made of, measured
against 480 hand-drawn boxes" and "Where the band goes, measured at eight bands
over 478 hand-drawn boxes", are what each one printed. The band sweep is about
51 seconds under `--release`: it detects all eight bands inside one decode pass,
where re-entering the decode per band would be 360 full-resolution decodes to
score 45 pages.

`crates/taiyaku-cli/src/region_experiment.rs` is L2a's: why the text-block
detector fails, run against real pages. It prints, per sampled page, the
component count, the fraction of components under 5px, the scale each of three
candidate rules derives, the gap that follows, **how many blocks survive each of
the four filters in order**, the largest block `MAX_BLOCK_AREA` discarded, and
the axis flagged against the edition's true reading direction — and, since the
measurement below that asked what the one edition clearing the bar was actually
finding, **what every surviving block is made of**: its size, how many
components it holds, how many of those are under 5px against how many of the
page's are, and their median size, plus the page's own pixel dimensions, the
smear gap as a fraction of them, the largest component *before* any smearing,
and what each of the four filters rejected across the whole sample. It exists because
the first version of this diagnosis was a throwaway program, discarded once
`notes.md` was written — which left the survey's `regions:` lines reproducible
and the diagnosis behind them not. It calls `detect.rs`'s own functions rather
than copies of them, so nothing in it can drift from what `taiyaku regions`
does; the discarded program copied them and needed a cross-check against the
real command to show the copy was faithful. About seven seconds under
`--release`. See `notes.md`, "The text-block detector, measured against four
works, and why it does not work yet", "The scale rule, tried three ways, and
neither candidate clears the bar", and "The edition that clears the bar is the
one finding no text" — the last of which names the command that reprints every
figure in it.

`crates/taiyaku-cli/src/geometry_experiment.rs` is L1's one-off
measurement of which derived transform a swap should map through, and it stays a
measurement rather than becoming a check: it asserts only that it measured
something — a non-empty sample, a search that found a minimum in every volume,
every residual finite — so that a glob matching no volume cannot print an empty
table and pass, which is the one failure a measurement must not have.
`check-all.sh` reaches none of these four — its corpus job filters on
`corpus::`, and its `test` job now runs with `TAIYAKU_CORPUS` removed from the
environment — so the twelve above are still twelve. All four are excluded from
the per-file coverage floor by name, in the `--ignore-filename-regex` that
appears twice in `check-all.sh` and twice in `ci.yml`; a gated experiment that
skips in CI would otherwise report near-zero coverage and fail the floor. That second half is new:
until 2026-08-08 the `test` job inherited the variable and ran both of these in
**debug**, which is how a gate run came to be killed at 1h09m inside
`geometry_experiment` alone. This one also wants `--release`, under which it
takes about a minute against about forty without. Not promoting it was decided in
advance, by a rule fixed before the number existed (the L1 design's §4), and
the rule's first branch fired. It also could not have been promoted as written:
the transform it picked is TypeScript, so a Rust corpus check would re-measure
page dimensions no code in this repository can alter and go green while the
browser did something else. The unit tests beside `mapRect` are what guards
`mapRect`. See `notes.md`, "The exposure, and why there is no standing check".

`taiyaku-core` is the portable half — no I/O, so it compiles to WebAssembly:

| Module | Holds |
| ------ | ----- |
| `solve` | similarity, per-pair floor calibration, Needleman-Wunsch, run compression, confidence, verdict |
| `slot` | spread detection, slot expansion, reading direction |
| `sidecar` | the `taiyaku.json` format — parse, emit, judge, merge an edition in. `from_json` **refuses a document declaring a format version newer than this build reads**, so no command silently strips the fields it has no name for and writes the result back stamped version 1. The library's own import checked that at its two entry points and `align --write`, `credits --write` and `regions` did not; one check where every command parses is what makes the answer the same in all of them |
| `route` | which path a lookup walks between two editions, and the page it lands on |

Four of the other five touch the disk or the network. `taiyaku-wasm` touches
neither — it is portable like the core, and separate from it so the core's API
is not shaped by what can cross the JavaScript boundary.

| Crate | Holds |
| ----- | ----- |
| `taiyaku-read` | how a member becomes pages — archives, directories, natural order, page counts, content hashes, one page by index. It also holds what a *name* and what a container's *bytes* mean, both moved here on the scanner branch so `taiyaku-library` can call them without a second implementation: `volume_number` (marker or first digit run — `inventory`'s, since it is handed a directory that is a volume by construction), `volume_number_marked` and `chapter_number` (marker required, since a rule applies to every filename in a library), and `Format`/`sniff`, which answers what a container is from its first 512 bytes rather than its extension — `scan_entry.format` is sniffed, never derived from a suffix, because a zip named `.cbr` and a RAR named `.cbz` are both routine |
| `taiyaku-library` | the sixth crate, added between `taiyaku-read` and `taiyaku-serve` — a SQLite store: `work`, `edition`, `member`, `alignment` and `run` project a sidecar into rows, and `progress`, `note`, `truth_page`, `truth_box` and `truth_member` hold the per-user state that used to be sibling JSON files — `truth_member` being the digests a page's boxes were drawn *against*, stored rather than re-derived, so `TruthEdition::is_stale_for` can still refuse to score labels whose archives have changed (review 2's C2). `import` reconciles a sidecar against those rows by content identity first and by (canonicalised) `sidecar_path` second, calling `Sidecar::merge_spoke` rather than reimplementing its refusals; `export` projects rows back out to `taiyaku.json` and its siblings, `<stem>.library.json` among them (see `docs/sidecar.md`). A hand correction made through `serve` writes the `alignment` and `run` rows in the same request as the file, through `state::put_alignments` — the fourth `put_*`, beside progress, notes and truth (O43). Only `taiyaku-serve` and `taiyaku-cli` depend on it; `taiyaku-core` and `taiyaku-wasm`, which compile to WebAssembly, must not and do not, since SQLite does not run there. `browse.rs` is spec C1's read layer over all of it — the grid (`?library=` narrows it, and search and the home lists, to one library, by reading the directories that library and the grant share in place of the grant — `browse::library_scope`, `2026-10-05-libraries-design.md` §3), one series, one work, search over **both** FTS indexes — schema version 10, both `series_fts` and `work_fts` tokenised `trigram` since migration 8, with `LIKE` answering the under-three-character queries trigram cannot (`docs/notes.md`) — the three home lists, the series cover and the library status (`GET /api/library/status` → `{ libraries: [{ id, name, scanned }], restricted }`, read-only — the libraries the sidebar lists, per library the fact that tells two of spec C1 §8's first-run states apart, and whether the caller is limited to the libraries granted to it, which tells an account granted none from a server with none). A work's visibility under a grant is decided on `member.root_id` (migration 6), on the member rather than on the work because a work spans roots by design; an account that sees every library applies no filter at all, which is what stops the upgrade emptying every account's shelf. Since O220 that is the account's `every_library` setting (migration 17) rather than an empty grant, and an account limited to libraries and granted none sees nothing. **A hand-imported work has no series** — `import` never writes `series_id`, and cannot, since `taiyaku.json` has no field for it — so the grid and search both return `{ series, unfiled }`, without which every library predating spec B opened on an empty page. The schema also carries `collection`/`tag` (spec C1's browse UI, not built). The rest of what used to be listed beside them as unpopulated is spec B's scanner, and `derive.rs` is what populates it: the walk writes `scan_entry`/`dir_state` under a `root` and decides nothing about them, the rule engine maps a `rel_path` onto a series, an edition key, a volume and a chapter, and derivation turns those two into `series`, `series_title` (which is what the per-title FTS indexes, and what `librarydoc::find_series_by_title` looks a series up by, so a series row without one would be invisible to both), and — through `import::scanned`, so `work`/`edition`/`member` keep exactly one writer — the works themselves. `series_id` and `volume` on `work` are the two columns that import cannot write, because `taiyaku.json` has no field for either, so derivation sets them directly; they are also the pair the hub lookup reads back, since `content_key` cannot be computed until the hub is known and the hub must be fixed when the work is created. **A derive opens no file** except to hash a member nothing has hashed yet, and writes that digest back onto `scan_entry` — which is the whole of §2's claim: editing a rule re-derives the library with the tree deleted, and `derive/tests.rs` deletes it to prove there is no other route. Two rules keep that from doing damage. **A scan replaces editions and members and touches nothing else**: `import::scanned` leaves `alignment`, `run` and `page_flag` alone, because a walk cannot know how two editions correspond or which pages are a group's donation cards, and until 2026-08-17 it deleted all three from an empty document on every rescan — so a rescan after an `align --write` dropped hand-corrected runs, and a `library export` in the window wrote a sidecar with no alignments over the reader's own copy. The cost is stated rather than discovered: a rule edit that removes an edition an alignment names leaves that edge dangling, which `runs_of` reports and `quality_of` answers `None` to, so the work still projects and its hub still serves. **A directory is a member only if no ancestor of it is one**: the fact layer records a container and its children alike, so without that test a volume produced a member for itself *and* one per chapter. **The evidence for that used to be a citation of `docs/sidecar.md`'s "7 of 57 archives nesting pages in folders", and that document is measuring something else** — entry names *inside* a zip, which never become `scan_entry` rows at all and so cannot produce a member either way. The on-disk measurement that does support it, taken 2026-08-18 over `corpus/`: 121 directories hold page images directly, and 107 of them sit beneath another that also does — every one under `incoming/`, this corpus's untidied dump, which is exactly the shape a real library arrives in — its pages counted twice, its pieces made editions of nothing, and, where the chapters carry no volume marker, the volume they are plainly inside reported as needing an `attach-chapters` decision. **And only if nothing *inside* it is one either, unless a rule marked it a volume**: a container the rules gave no volume is a name in a path rather than something anybody released, so it stands aside for whatever holds the material — without that half, `Series/Extras/Art Book` counted five pages as ten and three levels gave three members, and taking the container instead (the first fix for it) turned 107 corpus members into 40, collapsing `incoming/bocchi`'s five editions into one 439-page sequence. A container the rules *did* mark as a volume still wins, which is the sentence above: `Series - v01/c001` is a piece of `v01`, not a work beside it. A **page-less** container wins nothing at all — a volume directory holding `.cbz` files takes `pages = Some(0)` from the walk, and it used to suppress the archive that held the pages and then hash to the digest of nothing, so every such directory collided on `content_key` and one row swallowed the rest. **Entries the rules gave neither a volume nor a chapter are keyed individually**, by `rel_path`: keyed on `(series, NULL)` they merged, so `ONE PIECE/001..004` — bare numbers, which `volume_number_marked` deliberately refuses — became one work of 18 concatenated pages, and a volume-less group is therefore addressed by its own member paths rather than by `(series_id, NULL)`, which no longer names one row. A scanned work with a volume also has a **stable address**, `(series_id, volume)`, probed after `content_key` exactly as a document-backed work's `sidecar_path` is: `content_key` hashes the hub edition's member digests, so adding a page, repacking an archive or fixing a scan moved it, the key lookup missed, and a second `work` row was inserted while the reader's progress, notes and history stayed on the first — measured on 2026-08-17, and triggered by the most ordinary events in this corpus. Content still decides first, so a volume whose bytes are unchanged follows them wherever it is renamed or renumbered to, and a duplicated download cannot turn one repeated file into a failed derive of a whole library. **What makes the address probe safe is that order and a counter, not any uniqueness of the address itself**: an address can be vacated and reoccupied — remove volume 1, renumber volume 2 into its place and repack it — and the probe then hands volume 1's row, and a reader's volume-1 position with it, to what was physically volume 2. Without the probe that same scan mints a third row and strands the position outright, so it is a trade made deliberately and `rekeyed` counts every time it fires. What that address probe reconciles is **counted** as `rekeyed`, and what the suppression drops is **counted** rather than dropped in silence: a rule set that accidentally made a series directory speak for its volumes would swallow every one of them, and a report with no number for that reads exactly like a library that happens to hold nothing — the same reason `check-all.sh` refuses to print a bare "all green" when something was skipped. **Any derivation outcome that changes what a person sees and cannot be inferred from the other counts gets a field**, which is the rule `suppressed` and `rekeyed` were built on and which `kept_editions`, `merged` and `unrecognised_kinds` follow too: `kept_editions`, editions standing on a work that this derive did not derive — a scan no longer deletes an edition it did not name, since "the scan never saw this" and "a rule edit removed it" are one statement from in there and a deleted hand-aligned edition is unrecoverable while an orphan is visible and fixable — `merged`, entries that joined an edition another entry had already opened, which is right for the chapters of a volume and wrong for anything else and which derivation cannot always tell apart, and `unrecognised_kinds`, editions whose rules named an `edition.kind` the column's closed set does not hold, so none was written. A scan is also barred from writing `work.title`, `work.reading`, `work.hub` and `edition.label` over a row that already has them: it derives a title from a filename and a direction from a constant, while a document carries what a person or the aligner established, and `import` + `scan` + `export --in-place` was measured rewriting a reader's own `taiyaku.json` from "Go-Toubun no Hanayome, volume 1" / ltr / "Japanese raw" to "Series - v01" / rtl / "raw". The cost, since nothing records whether a title came from a person or from an earlier derive: a work this scanner minted keeps the title it first derived, so a later rule edit that would rename it does not reach the row. **`edition.kind` and `edition.language` are deliberately not on that list**, and spec C3 is what settled where they belong: `rule.set_kind`/`set_language` reached no row at all until `derive::write_edition_metadata`, so two of the accept form's five controls decided nothing — measured on real archives, an accepted `edition-of` proposal producing an edition whose `kind` was NULL beside the rule that said `official`. What bars a scan from the four columns above is having no way to tell a hand-set value from a derived one; this pair has one, since `librarydoc::apply` is the only other writer of a value into them and `migrate::absorbed(…, LIBRARY)` answers per work whether it has run — so they sit under that guard beside `work.series_id` and `work.volume` instead, which is what lets a rule edit reach them a second time. A kind outside the closed set is refused rather than stored: `librarydoc::of` refuses an unrecognised stored kind, so a work that can no longer be described is a worse outcome than a typo that decides nothing, and the refusal is counted rather than silent. `librarydoc.rs`'s series/credit/chapter editing (spec C3, the metadata-editing screen) has no CLI command or HTTP route that lets someone *set* it, which is O44 — *closed by spec C3 PR 2, 2026-08-28: the four `PATCH /api/library/*` routes are the callers, and `derive`'s read-back applies a document a rebuilt database has never seen.* It read: `taiyaku.library.json` is the one document designed for hand-editing and there is no route that applies a second one. **A scanned work's copy of that document lives at a second address**, one this crate's own `hand.rs` finds and writes:
`<root>/.taiyaku/<content_key>.library.json` — a real file this branch writes into a reader's library, beside their archives, not only into the database. `<stem>.library.json` beside a sidecar already answered for an imported work; a scanned work has no sidecar to sit beside, so `hand::path_for` addresses it off the hub edition's own root instead, and `hand::save` writes it and arms the `LIBRARY` marker in the same call — for a work with no address at all (an imported work, whose document is `export::work`'s to write) the marker still arms, only the file does not, which is `Saved::persisted` answering `false` with no failure. Migration 010 is what makes `work.title` and `edition.label` — the two columns C3 §5 puts a PATCH route over and that were previously always-populated, never-cleared — safe to hand-edit at all: each gets a `_hand`/`_derived` pair, `title_hand`/`title_derived` and `label_hand`/`label_derived`, because both underlying columns are `NOT NULL` and a hand edit's own withdrawal has to restore something rather than leave a `NOT NULL` column with nothing to fall back to. `migrate.rs` (the whole-branch review's C3 fix — a distinct, coincidentally-numbered thing from the roadmap spec above) is the one caller that reaches `apply`: on a work's first import, if `.progress.json`, `.notes.json`, `.truth.json` or `.library.json` sit beside its sidecar and the database holds no state of that document's own kind yet, each is read back once — rows are current, files are history, so a document the database already holds state for is left alone even when its file still exists. **Once, is a stored fact**: a `migrated_document` row per work per document, written the first time each is considered, so a reader who deletes their last note does not get it back at the next `serve` start, which is what testing emptiness instead did (review 2's C3). That row carries a second fact, `absorbed` — whether this library has ever had the file's *content*, read in here or written out by an export — because "this document is settled" and "the database has authority over the file at that path" are different claims, and `export --in-place` was reading the first as the second (review 3's C1, and schema version 2). A malformed file is reported and skipped rather than failing the import, and is the one outcome that records nothing, so fixing it and importing again still works — and now survives an export in between, which it did not. The import summary gained a word for the same reason: a settled document the database holds nothing for reports `unread` rather than `kept`, whose documented meaning is that the database already held it. Every progress, notes and truth call funnels through one connection behind one mutex, so WAL buys nothing and two different works no longer read in parallel (O42, unmeasured — the file store it replaced had independent I/O per work). **Between processes it buys plenty**, and that is the deployment this exists for: `serve` and `library import` default to the same file, so every connection carries a five-second busy timeout and opens its writes `BEGIN IMMEDIATE` — SQLite runs no busy handler for a deferred transaction promoting to a writer, so before this an import started while the server ran exited 1 on "database is locked" (review 2's I2). Each migration applies inside one transaction that ends by writing the version, so a migration that fails leaves the file exactly as it was rather than half-applied and unopenable (review 2's I1). **The first open of a fresh library is the one that races**, and both ways it could fail are now waited out rather than reported: the version is read a second time inside the migration's own transaction, so two processes that both saw an empty file do not both apply migration 1 (the second used to fail with `table series already exists` — the very message the module names as unrecoverable), and `journal_mode = WAL`, the one statement SQLite runs no busy handler for, is retried for the same five seconds rather than failing on the spot with `database is locked`. Measured before: 14 failures in 48 concurrent first opens, and 12 in 30 runs of the four-way test that now pins it; after: none in 40 runs of the same (review 3's I1) |
| `taiyaku-serve` | the HTTP API: works, sidecar, page by index, progress, notes (`<stem>.notes.json`, read and written whole under `If-Match`), a download — a whole edition as one `STORE` zip or one member's own bytes, both with an exact `Content-Length` known before the first byte and working `Range` (spec C2, all three routes: the two above and the OPDS-namespace mirror at `/opds/v1.2/works/{work}/{edition}/download`, which is the *same handler* registered a second time rather than a second implementation — it exists because a 401 on `/opds` carries `WWW-Authenticate` and a 401 on `/api` deliberately does not, so a client following an acquisition link into `/api` cold would never prompt. The acquisition link in `opds.rs::work_feed` points at it, with the `type` `download::advertised_type` says the route will actually send rather than a constant, since a single-member edition can be a PDF and a feed advertising it as a comic archive would be a claim the response's own header contradicts. **A reader whose `can_download` is withheld keeps the page link**: OPDS 1.2 requires an acquisition link on every entry — the reason this one pointed at a page in the first place — so it becomes the fallback rather than being deleted, while the web UI's own control is *absent* instead, a feed having a conformance floor a screen does not) — an OPDS 1.2 + PSE feed and generated covers, all behind a `PageSource` trait with **two** implementations — local disk, and a composite that resolves each member to disk or, inline within the same type, to Stump (see below). A caching split runs the length of the file: the four routes whose bytes are fixed by a member's `sha256` carry a day's freshness — two of them an `ETag` as well, and the thumbnail and cover neither, since both find the member only inside `spawn_blocking`, too late for a validator to save the work it would be skipping — while every route answering a document this server rewrites carries no freshness at all, on the `304` as well as the `200`, which is the half that is easy to get wrong (O1) **Every route in `build_router` sits behind one guard since spec A2**, resolved from a session cookie or `Authorization: Basic` into a `Principal` — and `routes::guarded_routes` is the router's own source of truth so that `auth::tests::every_route_refuses_an_anonymous_request` walks the same list `build_router` folds, which is what makes a new unguarded route fail the suite on the commit that adds it rather than on a reviewer noticing. **Authentication had that enumeration from the start; the root grant did not, and six routes drifted from it before one existed** — four in `opds.rs` (`stream_page`, `cover`, `work_feed`, `works`), plus `routes::list_works`, plus a write route, `patch_alignments`, which returned a corrected sidecar in its response and wrote shared alignment rows for material the caller could not see. All six now refuse or filter, and `routes::grant_postures` classifies every guarded route as *Refuses*, *Filters*, *RefusesAndFilters* (`/api/works/{work}/alignments`, whose write is refused and whose response is separately filtered — a bare *Refuses* here left the response half outside the enumeration test's own *Filters* completeness check, which is structurally why that half went unproven until a verification pass caught it — and, since O60, `/api/works/{work}/progress`) or *GrantIndependent* (with a stated reason); `auth::tests::every_route_declares_its_grant_posture` asserts that classification is set-equal to `guarded_routes`, so an unclassified route fails the build, and drives a real request from a restricted account for **every** *Refuses*, *Filters* and *RefusesAndFilters* entry, whichever way the table splits between the three (a route has moved between them before), a split and a total the test pins with assertions of its own so this sentence does not have to. Driven against both a whole work outside the grant and, since a whole-branch review found the first version of this table blind to the shape of its own headline defect, a work split across roots — rather than merely declaring them, with the rest (*GrantIndependent*) declared because there is no grant behaviour to drive — **all three figures were stale before C4 PR 2 corrected them**: they read 38/39/23/15 while the assertions in `auth::tests::every_route_declares_its_grant_posture` had moved to 19 driven and 26 declared over PR 1's seven new routes, which is the prose-outside-the-table drift the open-items ritual warns the sweep cannot predict; **and they were stale again when O60 closed**, reading 51 routes, 19 driven and one deliberate *KnownGap* against a table of 58 entries, 22 of them driven, with no such posture left, **so they are deleted here rather than corrected a third time**, not because driving them was skipped — the guard's own shape, closed for the boundary a checklist had only ever guessed at. `route_layer` rather than `layer`, so a path that does not exist still answers 404 instead of telling a caller every URL they invent names something real. The two 401 shapes differ deliberately: `/opds` carries `WWW-Authenticate`, which is what makes Panels prompt, and `/api` must not, because that header raises the browser's own dialog over a reader that cannot style, pre-fill or recover from it. Resolution runs inside `spawn_blocking` and drops the library's mutex before argon2 — a guard is the one middleware every request passes through, and holding either would serialise the server behind 11.9 ms per password login (O42, O52). `session.rs` holds the cookie, the CSRF origin check, the account screen's six routes and §5's claim route; every one of the six is scoped to the principal rather than to an id in the URL, so another account's key or session id is a 404 — §9 ships no user administration, and a route taking any id would be one. **Spec C3's proposals inbox is `curation.rs`**: five admin-only paths over `taiyaku_library::proposals` — a list filtered by state and kind and carrying per-kind counts, one proposal's evidence, a `POST …/resolution` dry run answering what a candidate rule would move before anything is written, and the three writes (`POST …/accept`, `DELETE …/accept`, `POST …/dismiss`) — across six handlers, five of them id-addressed *(a fourth write and a seventh handler since O72: `DELETE …/dismiss` reopens a dismissal)*. Those five — six since O72, with `reopen` — resolve the row through `proposals::lookup`, which answers `Ok(None)` rather than an error for a row that is not there, so an unknown id is a real 404 instead of a 500 *(and since O72's fix round, `dismiss` and `reopen` also answer 404 for a row another process deleted after that check, through `curation::failed_unless_gone`)*. All six — seven since O72 — do their store work inside `spawn_blocking`, which is not decoration: measured with a canary task at 250,000 `scan_entry` rows, the dry run stalled the runtime 138.45 ms before and 0.02 ms after, and the handler's own duration did not move — so nothing but that instrument could have shown it. What the layer still cannot do is tell a refusal from a store fault, because the library hands it both as one `Err(String)`: a database failure during an accept reaches the client as a 409. That is O70. *(No longer: O70 closed on 2026-09-14. `accept_with`, `unaccept` and `dismiss` answer a refusal as `Offer::Refused` and a store failure as `Err`, so a store fault during any of the three writes is a 500, and a refusal is still a 409 carrying its sentence.)* **Spec C3 §5's metadata-editing routes are `metadata.rs`**: four admin-only PATCHes — `/api/library/works/{slug}`, `/api/library/series/{id}`, `/api/library/editions/{id}`, `/api/library/members/{id}` — each applying an edit through `taiyaku_library::librarydoc::apply` (or, for a series' own titles, `set_series_titles`), then `hand::save`, then re-projecting through `librarydoc::of`, so the answer is what the store actually holds rather than an echo of the request. All four resolve their slug or id through the caller's own `principal.access`, exactly as `library_api::work` already does for the identical `GET` (I8, whole-branch review): before this a restricted administrator's grant gated the read and not the write, so the same URL answered 404 on `GET` and 200 — with the full re-projected document — on `PATCH`. `patch_series` and `patch_work`'s own `series` field can each fan out past the one document named in the URL to every sibling work sharing a series, folding every `hand::Saved` through `worst_case`/`merge_worst_case` so `persisted`/`why` in the answer describe every document touched rather than only the last one a loop happened to reach. | **Spec C3 §6 adds `stamp.rs`**, two paths and the only route in this product that modifies a file the user owns: `GET /api/library/works/{slug}/stamp-comicinfo` answers which of a work's archives would change if its metadata were written into their `ComicInfo.xml`, and the `POST` does it — raw-copying every other zip entry so pages keep their stored bytes, staging beside the target and renaming, then moving the member digests and `content_key` in one transaction — with the hand document renamed **after** that commit, a rename not being a SQLite statement, which is exactly why a stamp can move the key and leave the document behind at the old address (O81) and why the browser's completion paragraph states the two separately. Gated on `can_stamp`, a capability an administrator does **not** inherit — the one documented exception to O86 — and it takes `library_job` so it cannot run beside a scan. **Spec C4 PR 1 adds `config_api.rs`**, six paths of administrative surface the browser had no equivalent for: `/api/library/rules` and `/api/library/roots` as ordinary CRUD, plus two that do something the CLI cannot. `POST /api/library/rules/preview` answers what a *proposed* rule list would do to the whole library — series merged, works re-homed, works orphaned, proposals reopened and raised, the files each rule matched and the ones it missed, and every pair of rules that would decide one field differently — by replacing the rule table inside a transaction, running the **real** derivation under a documents policy that touches no file, reading the result and rolling back. That it runs the same `derive` a save runs is what makes "the preview is true" a property of the code rather than a promise two implementations have to keep. **Measured at a median of 281 ms** on 130 series and 1858 works (`docs/notes.md`), which is why C4 §6's "safe to call on every keystroke, debounced" ships as *previewed on demand* — safe, but not affordable. **PR B adds the seventh path, `PUT /api/library/rules`, and it is the first route in this product that applies a rule.** Every other rules route wrote the `rule` table and returned — `patch_rule`'s own doc said so in as many words — so editing a rule changed the rule table and nothing else until somebody scanned, while C4 §1's button promised "Save — apply to 71 works". It is backed by `preview::apply`, which is `preview_with_rows` **without the rollback** and with `Documents::Reconcile` in place of `Skip`: one enum value is the whole difference in what reaches the user's tree, since a real derivation reconciles hand documents and a preview must not, a rollback being unable to undo a file it wrote. `rules::replace_all` stays `pub(crate)` — widening it for one caller would export exactly the half of the operation that is dangerous alone, a rule set replaced with nothing derived over it. The route is symmetric with the preview down to the body key, the `MAX_PROPOSED_RULES` bound and the `rule_from_json` reader, through one shared `read_proposed_rules(body, noun)` so the two cannot drift about what a proposed list is; `POST .../rules/import` stays what it is, a *document* import that mints fresh ids by design and so reopens every accepted proposal whose rule it replaces, which is correct for a foreign document and wrong for a save. **It is a method on a path that already existed**, so `guarded_routes` and the grant-posture table changed and `auth::tests`' `expected` array and its 19/32 counts did not — the list that can see a method arrive is `config_api::tests::every_config_api_route_refuses_a_reader`, which the spec's "a new route goes in four lists" never named. And `POST /api/library/roots/{id}/scan` is **the first time anything in the serving process has walked a filesystem tree**; it runs on a second connection so the shared one keeps serving, is single-flighted per root, and finally writes `root.last_scan_at` and `last_scan_status`, two columns that had existed since migration 1 with nothing in the tree reading or writing either. **PR 2 adds `users_api.rs` and `backup_api.rs`**, nine more admin-only paths and the end of C4's server half: user CRUD with C4 §4's three refusals, session and API-key revocation, and backup. The refusals live in the **store**, beside `account::promote`'s own, because the CLI is a second writer and a copy of an invariant in each is a copy that drifts — and they resolve into *two* rules rather than the spec's three: nothing may leave the library without an administrator who can log in (which covers both deleting the last one and demoting them, the same lockout by two routes), and an administrator cannot demote *themselves* even beside a second one, which needs to know who is asking and so takes an `acting_as` the CLI passes `None` for. `DELETE /api/library/users/{id}` states its counts first on the same `?dry_run=true` shape PR 1 chose for a root, and states five where a root states three — no work, edition or member is among them, which is the difference worth naming: deleting a person deletes none of the library. **PR C's screen found the one field this list never set**: `GET /api/library/users` passed `None` for the caller's session token, so every session it answered came back `current: false` — the flag C4 §6.2 wants precisely so an administrator can tell which row is the browser in front of them before revoking one. `session::list_sessions` had passed `token_of(&headers)` since spec A2 and this route had not; `assembled` takes it now, from `list_users` and `patch_user` alike, and only the caller's own account can hold a session whose sha256 matches, so passing it for every account is correct rather than merely harmless. Backup is `VACUUM INTO` through a new `taiyaku_library::backup`, a `taiyaku library backup` command and three routes, on a second connection so a file copy never holds the server's one mutex; **measured at a median of 6 ms** on the same 130-series library whose rule preview costs 281 ms (`docs/notes.md`), which is why the `POST` answers when the copy is done rather than streaming progress. `backup::prune` is the only file-deleting function in the store and deletes only names its own `list` returned. There is no restore route and no in-server schedule, both declined on purpose: restore is stop-move-start, and a cron entry runs the command. **All nine routes refuse a device credential as well as a non-admin**, through `session::refuse_a_device_credential` — the gate spec A2's second whole-branch review built for the self-service account routes, applied here because these are the same capability over *every* account and because the backup download is the password file. Measured before it was applied: `GET /api/library/users` under an administrator's API key answered 200 with every account's sessions, IP addresses and user-agents. **O93 closed the same gap on every other administrative route** — `config_api`'s twelve, `curation`'s six and `metadata`'s four — by collapsing three module-local copies of the role check into one `session::administering_or_forbidden` that asks both questions. A consequence worth knowing for anything that adds a route here: **the suite's default client is a browser**, and it was an API key until O93 — which is exactly why O93 survived four reviews, since every route test drove the credential these routes should have been refusing. `test_support::authenticating` opens a real session and sends its cookie with the `Host` and `Origin` a browser would; `key_authenticating` is what `opds/tests.rs` asks for by name, a key being the credential OPDS actually has.
| `taiyaku-wasm` | the binding a browser reader loads: parse a sidecar, resolve a slot between editions, and say which path the answer walked |
| `taiyaku-cli` | the `taiyaku` binary: image decoding, fingerprinting, the content-addressed cache, the HTML report, argument parsing, and duplicate resolution — a copy's signature, the all-pairs judgement over one set, and the inventory walk that finds the sets |
| `web/` | the browser reader (React, Vite): single page, two-page spread and continuous-scroll modes, an arrow that steps by what is drawn rather than by slot, a swap that never writes position, a slot the edition being read has no page for filled from another edition and marked as borrowed (the gap state that used to stand there, offering the nearest paired slots either side as moves rather than jumps, is one press of that mark away), and a chrome of two bars shown and hidden together — a top bar of the way back, the work's title, the layout control and settings, and a bottom bar of filmstrip, progress line and five independently-switchable readouts (position, reading time, chapter, alignment health, note count) — behind the `Resolver` interface `taiyaku-wasm` implements. Every reading behaviour not yet in this list is one of ten `ReaderModule`s — of thirteen the registry holds — against a single declarative contract (`module.ts`): `direction` (a per-edition reading-direction override), `motion` (animate on/off, transition duration), `regroup` (parity breaks a reader inserts by hand), `readouts` (which of the five lines show), `prefetch` (depth ahead/behind plus a double-buffered, decode-before-swap image so a fast flick presents no blank frame), `annotations` (notes anchored to a slot of one edition and travelling to every other through the alignment, several to a page, each in a colour of the reader's choosing — a name in the file, never pixels — drawn as margin tabs plus an on-demand region tint — the tab in the letterbox beside the page, moving to the *outer* margin in two-page mode since closing the seam leaves a page no margin on its seam side — with five keys — write, reveal, review, and step back and forth through what has been written — and revealing off by default, since the margin tabs already say where the notes are without drawing on the page; notes save themselves a moment after the last change rather than on a button, so `history.ts`'s snapshots are what a reader abandons a change with; a region is swept out with a drag and pushed around with another, and while the editor is open every region on the page is visible and grabbable with one Save writing all of them, on an overlay that measures the artwork it sits over rather than the element around it — the same box, until `fit` made the element the whole cell and the two routinely different (§6 below); a note anchors to the whole slot, a point or an area, and its words are readable in a numbered list beside or below the page as well as in the region itself), `fit` (§6 below: how large a page is drawn inside its own box, and how far the two pages of a spread sit apart — `upscaleToFit` **on** by default, reversing the M5 design, so a 685×1024 scan fills a 1263px box instead of sitting at 1:1 in the middle of it, with `upscaleFilter` exposed beside it because that is a 23% enlargement of line art; and `gap` at `0`, where it is the two pages' *artwork* that meets rather than their elements — a distinction the design got wrong and a browser corrected), `layout` (which of the three reading modes is drawn — a module purely so that it *persists*, since it was component state and so was forgotten on every reload), `zoom` (§5 below: free continuous zoom and pan in single-page and two-page mode — wheel or pinch anchored at the pointer, a drag to pan with the note editor closed or a modifier held, and `=`, `-` and `0`; the view is one rect in *group* space, since a viewport can straddle both halves of a pair, and it is drawn as a single CSS transform on a wrapper, so `PageView`, `paintedBox` and every existing measurement are untouched — by zoom, which is what this claim was about; `fit` later changed both, §6; a page turn carries the rect and re-clamps, and a swap carries the visible content onto the other edition's page through the derived transform, or throws it away, a preference either way; continuous scroll keeps fit-whole, deliberately — its zoom is a different mechanism, since the strip's layout height is a function of scale and pan competes with scroll rather than sitting beside it), and `borrow` (the "Later" entry below is the record: where the viewed edition has no page for the slot, another edition's is drawn there and marked, and the mark is a button revealing the `Gap` view it replaced — hub-first order minus the viewed edition and minus the authority *(no fallback to the authority itself since 2026-08-13, marked here rather than rewritten: it used to lend when nothing else resolved, which it always could, and that was 228 of 276 corpus borrows — the page the reader had just left — so it was removed; the "Later" entry below is the record)*; on by default, so a gap now reaches a reader only through that mark on a work with a third edition, and directly otherwise, which is most of the corpus). The other three registry entries claim no reading behaviour. `settings` claims only the panel's own key. The remaining two are panels rather than page treatments. `credits` (§4 below): `C` lists the viewed edition's credit pages, a thumbnail and a page number each and no labels the sidecar cannot support, and `skip` keeps them out of the reading flow by being handed to `groups` rather than filtering its result — so the page area, the arrows and the filmstrip skip together — and since 2026-08-14 so do the offers out of a gap, which is O40 *(this said "not always" and named the exception: an arrow taken from a gap or a borrowed page reaches its destination through `nearestPaired` rather than `groups`/`stepToGroup`, so it could land on a credit page the reader asked to skip. `nearestPaired` is now handed the same skipped set `groups` is, from both of `App.tsx`'s call sites, so the key and the `Gap` screen's own offer buttons name one destination. It was unreachable on the corpus when it was found and still is, since `skip` defaults false and no corpus edition names credit pages; the fixture that reaches it lives in `App.test.tsx`)*, and two-page mode re-pairs around a skipped slot instead of stranding its neighbour. `help` (§6 below): `?` or `h` opens a list of every key the reader answers, generated from `keys.ts`'s one ordered table and from the registry rather than written out — so a binding that changes what it does cannot keep an old description, because there is one description — and listing only the modules that are switched on. `Settings.tsx` is generated from each module's `defaults` — a value's runtime type decides its control, never the module's id, and a string a module declares `options` for becomes a select — with a reset per section. **Spec C1's browse UI shell landed on top of it**, five files of it new and five more testing them: `Routes.tsx`'s `AppRoutes` resolves the four routes `react-router` now carries — `/` (the library grid), `/series/:id` and `/search`, both shipped by this branch (Tasks 1–10), and `/read/:work` (the reader, `key={work}` so `App.tsx` stays non-re-entrant across a swap between works); `Library.tsx` is the grid screen — a continue strip over `home`'s in-progress works, three chips (`all`/`multi`/`unread`) and an `added` sort that filter and sort client-side over data already held (§7's measured 127 series), all reading `library.ts`'s `/api/library/*` client; `EmptyState.tsx` renders the three first-run states spec C1 §8 asks for — no roots, roots but unscanned, and no matches — told apart by `/api/library/status` rather than by a grid response identical in the first two; and `Chrome.tsx` gains a `Library` button, docked opposite the reader's three existing panels and hidden mid-correction, so a reader has a way back out of the reader that is not only the `L` key. **This branch moved the sidebar out of `Library.tsx` and into `Shell.tsx`**, which now wraps all three library screens (not the reader) and owns the status fetch every one of them needs, so three screens share one fetch rather than risking three that disagree; `Sidebar.tsx` draws the list it is handed and no navigation path may require a root. `Series.tsx` is one series and its volumes — a grid or a table, `Cover.tsx` behind each and remembering a failed cover by its `src` rather than a flag so a retry survives a re-sort — an edition selector that dims a volume rather than removing it, and an optional `i` panel naming a verdict and a paired count from `/api/library/works/{slug}`. **Spec C2's download control lands in both layouts, and differently in each**: the grid gets one per tile for the edition the tile is already showing (`shown ?? w.editions[0]`, the expression its cover uses), because a tile shows one cover and the edition selector has already answered "which edition am I getting"; the table has no such edition — it renders one column *per* edition key — so there each cell's page count *becomes* the link, which adds no element to a layout built for comparing editions side by side. §8 of the download design asked for "the same control as a column" and that did not survive the table's shape. A plain `<a download>` in both, never a `fetch`: an anchor navigation carries the session cookie like every other browser request and the browser's own download manager takes the file, where a `fetch`-to-blob would buffer a 1.49 GB volume in the tab; the attribute is bare so the server's `Content-Disposition` name wins. Both are **absent** rather than disabled when `Me.can_download` is false — a disabled control invites a reader to find out why and nothing on this screen can tell them — and absent too when no gate sits above the screen, since `gate.ts`'s rule is that an unknown account is never read permissively. `Search.tsx` is `?q=` in the URL, debounced through `debounce.ts` at the reader's own 250 ms, a sequence number for out-of-order arrival and `run.cancel()` plus a bump on an emptied box so clearing mid-flight cannot land a stale answer over it; its hits draw through `SeriesGrid.tsx`, split out of `Library.tsx` this branch so the two screens sharing §5.4's `{ series, unfiled }` shape share one cover fallback rather than risking the `⇄` mark landing in two different places. *Search has since moved onto the Library view, by the sidebar and readability redesign (the "Later" entry below): `SearchBar.tsx` is the box, `searchHits.ts` holds the debounce and the sequence number, `Library.tsx` draws the hits through `SeriesGrid.tsx`, and `/search` redirects to `/` with its query and its library.* Both switches and the panel are preferences, not props: `browseModule.ts` is a second `module.ts` registry — `layout`, `editionSelector`, `panel`, `density`, `continueStrip`, `showRoots` — and `BrowseSettings.tsx` is generated from it exactly as `Settings.tsx` is generated from the reader's, `fieldsOf`/`fieldChanged`/`sliceReset` reused rather than reimplemented. *`showRoots` has since gone from the registry, and since the sidebar and readability redesign so has `BrowseSettings.tsx`: a View button on the Library view and on a series page draws the preferences that act there, generated the same way (`ViewMenu.tsx`).* Migration 8 gives both FTS5 tables the `trigram` tokeniser — the schema is at version 9 — so a query of three characters or more finds any substring in either script, and `browse::search`'s `LIKE` path answers the two that trigram cannot, which is what `Search.tsx` types into without either being visible to the reader. Fifteen files new across the two (nine source, six tests), all under `web/src/`. **Spec C3's proposals inbox is the fourth library route**, `/proposals`, ten more files (four source, six tests): `Proposals.tsx` is the list — per-kind tabs each counting its own total regardless of which is selected, a score threshold defaulting to 1 on the `{0, 1, 2, 3}` scale with "Show weak proposals" dropping it to 0 rather than suppressing anything, and one row expanded at a time. `ProposalRow.tsx` draws the expanded row: two sides, subject and proposed target, each with a cover **and two interior pages** — the interiors being the point, since two editions' covers are two different illustrations and both in colour, and it is an interior that shows one edition to be the other colourised, confirmed against real archives at last (`notes.md`, 2026-08-26) after every screenshot until then had used placeholders — plus each side's directories, volume and page counts, format and root, the signed reasons behind the score, and a standing admission that nothing here compared the artwork. `AcceptForm.tsx` is the rule the accept would write: five controls (pattern, match-as, edition key, edition kind, language), a debounced dry run behind every edit, and an Accept disabled on any of the server's five refusal conditions *and* while the answer on screen measured a rule other than the one the button would send. `curation.ts` is the client for all of it. The screen asks only for `state: 'open'`, which is what an inbox is — but that leaves spec C3 §4's "a dismissal you disagree with later is a thing you have to be able to find" met at the store and API layers only, with no surface a person can reach, which is O72. *(O72 has since closed, on 2026-09-15: the inbox has an Open / Accepted / Dismissed control, and a dismissed row can be reopened — see its entry.)* The sidebar's `Proposals` entry **carries the open count**, which spec C3 §6 and the build design's file-structure table both asked for. *Since the sidebar and readability redesign the entry sits behind More, whose button carries the same count as a badge (`AccountMenu.tsx`).* *(It shipped without one for most of the branch, on the ground that `Sidebar.tsx` mounts around every screen and so a live count is a `listProposals` on every page load — a deviation recorded rather than silently taken, and reversed by the branch's whole-branch review. What settled it: that request is the flat one — `proposal`, joined to `series` for the two titles and grouped for the counts, with no rule compiled and no `scan_entry` walked, which is what the detail and dry-run routes both do — 0.10 ms at 8 rows and 0.23 ms at 25,008 against their 1.1–1.2 µs per entry, and it is admin-only, while the premise of §3's whole argument is that somebody learns the inbox is non-empty and nothing else in the app ever told them.)* The question the deviation left open was a real one and is answered rather than dropped: **what it counts** — the screen's header says *open*, but the list beneath it is narrowed to score 1 and above by default, so a badge counting every open proposal and a screen showing open-above-threshold disagree the moment a weak proposal exists, and the corpus raises those routinely, since a colour edition ingested to fewer volumes than its original scores exactly at the threshold. `curation.ts`'s `INBOX_MIN_SCORE` is one constant read by the badge's request and by the screen's own first request, so the badge is the number the header shows on arrival and pressing it can never land on an empty list. "Show weak proposals" moves the screen and not the badge, deliberately: that control is component state and starts false on every mount, so the list a person lands on is always the threshold's. `Shell.tsx` owns the fetch — one per shell mount, admin-only — and `shell.ts`'s `InboxRefreshContext` is how an accept or a dismiss moves it, since the two numbers are on screen together and a badge left stale beside a header that moved is a contradiction on one screen. **Spec C3 §6's ComicInfo confirmation is a section of `MetadataEditor.tsx`**, below Save and outside its `<form>` so pressing Save cannot trigger it, and absent rather than disabled for an account without `can_stamp`. **Everything in it renders from one derived phase** — `phaseOf` folds six state variables into eight named states (nothing asked, reading, read failed, empty plan, plan ready, rewrite in flight, rewrite refused, rewrite done) and every control, status line and paragraph reads that value. Three review passes had found defects there that were all one defect: each control stated its own condition over those six variables, each round's fix moved one condition, and the gates then disagreed — a plan read after a rewrite drew the archive list, the consequences and the irreversible warning with **no controls at all**, the two read controls could draw side by side under two alerts, and the one sentence saying a rewrite was running lived inside a button two reachable states did not render. Three separate comments arguing which combinations were reachable were each proven false by the pass after them, which is why the reachability claims that remain name a mechanism — a stamp supersedes any plan read it overtook the moment it is pressed, and a *successful* one also clears the plan it applied, so every plan on screen was read after the last rewrite that landed, while a refused one deliberately keeps the plan it refused, that being what a retry would send — rather than enumerating states. **Spec C4's settings area is the fifth library route**, `/config`, six more files (five source, six tests): `Config.tsx` is the area — one admin-only sidebar entry *(behind More since the sidebar and readability redesign)*, a sub-nav, and its own child routes declared from `config.ts`'s `TABS`, so a tab and the route behind it cannot drift apart and a bare `/config` redirects to whichever tab leads that list. No client-side admin guard, deliberately: `/proposals` has none either, and a non-admin who types the URL is shown the server's own 403 sentence rather than a bare status — a client that *decides* who may administer is the shape O93 was. Since the Configuration grouping's second PR its Libraries, Users and Backup screens are drawn on `settingsLayout.tsx`'s components: a page header with the page's title, description and actions and its way back where it has one, then sections, each a heading outside one box of rows with a label and its help beside the control, and last a removal box on a page that removes something. A removal's confirm draws its button and Cancel side by side, on two lines where they do not fit on one, and a library's or a directory's disclosure of what re-deriving would find after them. On a wide screen `SideBySide.tsx` draws an account's page or New account beside the Users table, and a directory's page beside its library's page; narrower, the page is drawn alone and the list is not mounted. Since its third PR the Rules tab is drawn on them too, at its own width rather than the pages' measure: the list and the rule being edited each in an untitled box, side by side where they fit, and the preview in a box titled Preview beneath them. Outside Configuration, `/account`, About and the licence page are drawn on them under a visible h1, each with h2 sections, and the reader's account panel, which draws `Account.tsx`'s controls, keeps its own look. A library's page and an account's are at their names (`configAddress.ts`), and New library and New account at `/config/new-library` and `/config/new-account`. `Libraries.tsx` is the tab where `Roots.tsx` was until F2a (`/config/roots` redirects to it), and since O219 it is four pages over one list: `LibraryTable.tsx`, a row per library that opens its page; `NewLibrary.tsx`, which makes a library from a name and one directory, typed or found with `DirPicker.tsx`'s browser of the directories the server may list; `LibraryPage.tsx`, a library's name, what it holds, a Scan library that walks each of its directories and derives once, Rename…, which edits the name in the title's place, a table of its directories with Add directory…, and Remove…; and `DirPage.tsx`, one directory's page, which is `RootRow.tsx`. A refused make or add is drawn as the server's sentence in the form that was submitted, and a refused rename in a callout laid over what is under the title, which moves nothing, and an answer whose page went before it came is said on the page the person is on; a directory added in this session shows its regime *before its first scan*, and one loaded from the list says its watch state is unknown rather than guessing; a scan anywhere in a library holds its Scan library and Remove… and each directory's Scan directory, Move… and Remove…, and while a library's removal is open its directories are drawn without links; and the screen keeps what the re-derive after a removal found, drawn one level up. `RemoveLibrary.tsx` states a library's removal before it acts, as `RemoveRoot.tsx` does a directory's. `RootRow.tsx` is one directory: the regime facts under its path, six settings in two sections, How it is scanned and How it is read, five boxes patched one at a time on blur (an emptied box clears to `null`, never to `''`) and a checkbox patched as it is ticked, C4 §3's scan trigger, Move…, which opens in a dialog over the page, and a 1 s poll that stops the moment the scan is done. `RemoveRoot.tsx` is C4 §2's most destructive action stated before it acts — `?dry_run=true`'s three counts, drawn even when they are zero, and **no confirm button at all until the counts arrive**, since a dry run that failed is exactly when somebody presses on regardless. `ScanReport.tsx` draws both server reports whole, thirty-seven fields, what-it-did-not-do first and every empty collection rendered as `0` — C4 §3's whole discipline, pinned by a test that counts the fields rather than naming them, because a test that enumerates has the hole it is checking for. **PR B adds the rule editor**, the second tab and C4 §1's screen — five more files (four source, one client): `RuleEditor.tsx` owns the rules, the selection and the preview; `RuleList.tsx` is the ordered list, where **everything addresses a rule by its position and never by its id**, because two rules the editor has added and not saved both carry `id: 0` — the same boundary `preview::Decided` and `preview::Override` hold in the store, and the reason the React `key` is the index here where it is usually wrong; `RuleForm.tsx` is one rule's fields, committing on blur or Enter and **never on keystroke**, since a preview costs a median 281 ms and the derivation is all of it, so a half-typed regex would buy a wrong answer at full price; `PreviewTabs.tsx` is four tabs, not C4 §1's three — overrides is a tab **with a count badge drawn from every tab**, which is what a warning-that-is-a-tab loses, an override hidden behind a tab nobody clicked. That tab read *Conflicts* until O95 closed on 2026-09-05: on the shipped seeded defaults its one row is rule 4 reading a series from the filename over rule 1 reading it from the directory, which is **what folds a colour edition into its main series** *(since O89, only where the directory does not qualify the filename's series — see O89)*, and the row's single action — *"Move rule 4 above rule 1"* — was the one thing that broke it. The detection was right and the noun was wrong, so the row now states the override, draws the two values it happened between, and offers reorder and *edit the winner* as options rather than as a fix. A preview runs on six gestures and no others: a blur or Enter, an enable toggle, a drag-reorder, an add, a delete, and selecting a different rule — the last because `rows` is scoped to the selection, so it is the only way to fill the Matched files and Provenance tabs, and it is one gesture rather than a stream. Save states what it will do from the live preview's `works_rehomed`, and **compares the impact it gets back against the preview it was made from**, drawing a difference as a warning rather than swallowing it: nothing should differ on a single-writer library, but the screen's whole promise is that the preview is true and the cheapest place to notice it is false is the save that just disproved it. "Proposals resolved" is **not** drawn though C4 §1 lists it — a derivation reaches one proposal writer and it only ever raises or reopens, so the figure could only ever read zero, and a structural zero drawn as a measurement is an invented number. `rules.ts` is the client and the one `root_path`↔`root_id` converter both directions of the export document use, so an export and an import cannot disagree about the mapping. Prepending Rules to `TABS` makes the flagship the landing screen, which is the one intended move of that list. *O219 has since put Libraries and Users ahead of it, so a bare `/config` lands on Libraries.* **Configuring a library is no longer a CLI-only activity**. **PR C adds the last two tabs** — five more files (three source, two clients): `Users.tsx` is the tab, which reads the list of accounts and the list of libraries an account's grant editor draws, and since O219 it is three pages over the accounts: `UserTable.tsx`, a row per account that opens its page; `NewAccount.tsx`, which makes an account from a name, a password typed twice and a role; and `AccountPage.tsx`, one account's page — role, the capability columns, the library grants (*Every library* or *Only these* with a tick per library, sent whole, since no route grants one, and the last tick never cleared; *O220 has since changed this: the last tick clears*), the sessions and API keys each revocable under the account that holds them, and a removal that states five counts and the name before it acts with no confirm button at all until they arrive. `configPages.tsx`'s `useLateAnswers` is what Libraries and Users do alike once a write answers: whether the person is sent to the page its line belongs on or the line is drawn on the page they are on, and what takes focus; and `Missing.tsx` is the line an unknown name or number draws on either screen, which takes focus. **The two invariants on the last administrator are one rule over two controls, its reason said beside each**, because the role dropdown and Remove reach the same lockout by different routes; the client's copy mirrors `account::another_admin_can_log_in` rather than counting administrators, which is wrong exactly when the second administrator is the locked one. Every disable is one-directional — this may refuse what the server would refuse and may never permit what it would not — and the self-password refusal names `/api/password` and the screen that calls it, which is O94: the account panel has no URL of its own and is reached by opening a work and pressing `A`. *(O94 has since closed, on 2026-09-14: the account screen is routed at `/account`, and the refusal links there.)* An account limited to no library is drawn as *None* in the table, and an administrator's as *Every library, as an administrator*; a failed libraries read says so rather than drawing an empty editor, because the empty state and the failed read look identical and mean opposite things. `Backup.tsx` is *Back up now*, the list with a plain `<a href download>` each — the route sends `Content-Disposition` and the cookie rides on the request — and C4 §5's four load-bearing sentences: restore is out of band, a backup is the whole database including password hashes, a sidecar export is not a backup, and the schedule is a cron entry running `taiyaku library backup --keep N`, the second as the page's description and the other three in its How backups work disclosure. **An account's page holds every control from a write until the list read that follows it lands**, because `user` is a prop and the window between a 200 and the reload is one where the page still draws its pre-write grants with its boxes live — and the grant editor sends a *whole list*, so a second tick inside that window silently revokes the first; `Users.reload` returns its promise for exactly that, and the revocations share the same claim so a double-click cannot draw a 404 over a revocation that worked. **Neither 409 it can meet is classified by status**: a same-second collision and an in-memory library share the code, so both are rendered as the server's own sentence |

`taiyaku-read` exists because the page sequence is normative — `sidecar.md`, "How
a member becomes pages" — so two consumers deriving it separately is a defect the
format cannot detect. The aligner and the server now derive it in one place, and
a gated corpus check compares all 617 pages of the primary volume across the two
paths.

The commands:

```
taiyaku align  <a-member>... -- <b-member>...   [--ltr] [--cache DIR] [--stats] [--write FILE [--title NAME] [--name-a KEY] [--name-b KEY] [--direct]] | --no-split-spreads [--stats]
taiyaku verify <a-member>... -- <b-member>...   [--out FILE] [--sidecar FILE [--cache DIR] [--ltr]]
taiyaku topology <hub>... -- <spoke>... -- <spoke>...
taiyaku lookup <taiyaku.json> <from> <to> <page>...
taiyaku serve  <taiyaku.json>...                [--addr HOST:PORT] [--db PATH] [--stump-url URL] [--stump-key KEY] [--log-requests FILE] [--refresh-stump] [--cache DIR] [--trusted-proxy CIDR]... [--secure-cookies] [--insecure-cookies] [--allow-web-setup] [--web DIR] [--dirs DIR]...
taiyaku inventory <root>                        [--min-pages N] [--pattern SPEC]
taiyaku duplicates <copy>... -- <copy>...       [--against <member>...] [--cache DIR] [--ltr]
taiyaku credits <member>...                     --sidecar FILE [--against EDITION] [--cache DIR] [--write]
taiyaku regions <member>...                     --sidecar FILE [--write]
taiyaku score  <member>...                      --sidecar FILE [--coverage F] | --sample <corpus-root>
taiyaku library import <taiyaku.json>           [--db PATH]
taiyaku library export <work>|--all             [--dir DIR|--in-place] [--user NAME] [--db PATH]   # --user needs --dir
taiyaku library scan  [--add PATH] [--library NAME | --root PATH] [--no-derive] [--db PATH]
taiyaku library libraries                     [--db PATH]
taiyaku library rules [--export | --import FILE] [--db PATH]
taiyaku library proposals [--min-score N] [--accept ID] [--dismiss ID] [--unaccept ID] [--reopen ID] [--db PATH]
taiyaku library backup [--out DIR] [--keep N]   [--db PATH]   # --out writes where the server will not list it
taiyaku library root documents <path> on|off    [--db PATH]
taiyaku library user promote|add <name>         [--role admin|user] [--db PATH]
taiyaku library user password <name> | user list [--db PATH]
taiyaku library user grant-library|revoke-library <name> <library> [--db PATH]
taiyaku library user every-library <name>       [--db PATH]
taiyaku library key create <user> --name DEVICE | key list <user> | key revoke <user> <id>   [--db PATH]
```

`align` takes several members on either side, which is how one volume aligns
against seven chapter files, or two tankōbon against one deluxe volume. `--write`
**merges** into an existing sidecar rather than overwriting it, which is how a work
gets a third edition; on a merge the a-side is the document's hub, so `--name-a` is
optional and an error if it names anything else. `--direct` overrides that for a
pair that has earned a direct edge. `verify` writes a contact sheet for looking at
what the aligner decided. `lookup` resolves a page through a written sidecar, which
is the operation a reader performs — through the hub where no direct edge exists,
saying on every call which path it walked. `serve` is handed sidecar paths the
same way `align` is handed members, because deciding which files constitute a
library is a different job *(reversed on purpose, marked here rather than
rewritten: as of the library store, `serve` imports every path it is handed
into the on-disk SQLite library — through `library::import_canonical`, the
same function `taiyaku library import` calls, so a work this command imports
and one a person imports by hand agree on what row it is — before serving from
the rows rather than the files. A path that cannot be imported is named on
stderr and skipped rather than taking the start down with it, and the start
refuses only when nothing at all could be imported. `taiyaku serve
<sidecar>...` still works unchanged, which is the migration path for an
existing install; deciding which files constitute a library is this binary's
job now too. See
`docs/superpowers/specs/2026-08-14-library-store-design.md`, §5, "What changes
in `taiyaku-serve`, and what does not")*; it serves `/api/works`, the sidecar
verbatim, a page by index, and a reading position, now accepts a hand
correction — `PATCH
/api/works/{work}/alignments`, guarded by `If-Match` against an `ETag` on the
sidecar — and 404s with a message naming what was missing rather than
returning a blank page. `--stump-url`/`--stump-key` (or their
`TAIYAKU_STUMP_URL`/`TAIYAKU_STUMP_KEY` env fallbacks) point it at a Stump
instance; pages come from whichever source — Stump or local disk — actually holds
the member, resolved once at startup and cached beside the sidecar. An OPDS 1.2 +
PSE feed at `/opds/v1.2/catalog` groups a work's editions under one entry, which
is what Stump and Komga cannot express on their own.
`--stats` prints what calibration measured and what shape came out — noise,
signal, separation, margin, run count, average run length and the verdict — one
line each, in a fixed field order so a survey can collect them into a table.
`scripts/margin-survey.sh` is that survey.

`taiyaku library import <taiyaku.json> [--db PATH]` is `import_canonical` by
another name — the same call `serve` makes on every path it is handed,
available without starting a server. `taiyaku library export
<work>|--all [--dir DIR|--in-place] [--db PATH]` writes a work's five
documents back out — `taiyaku.json` and its four siblings — and **refuses to
run with neither `--dir` nor `--in-place`**: unflagged, it would write back
over the sidecar a work was imported from, silently reverting any change made
to that file since, most plausibly a hand correction through `PATCH
/api/works/{work}/alignments`. *That correction now reaches the rows in the same
request as the file (O43, closed), so an unflagged export would no longer revert
it — but a change made to the file by anything else still would, which is what
the refusal is for.* `--in-place`, added by C2 (`8b78603`), is the
caller stating that overwrite is intended; `--dir DIR` writes elsewhere
instead, created if it does not exist yet, **one subdirectory per work named
for its `slug`** — `DIR/<slug>/taiyaku.json` and its siblings. That last part
is not cosmetic: every work in a real library keeps its sidecar at
`<dir>/taiyaku.json`, and export takes the filename stem from each work's own
`sidecar_path`, so a flat `DIR` gave every work the same five paths and each
overwrote the one before it — three works in, three files out, holding only
the third (whole-branch review 2's C1). Two smaller rules from the same
review's Minor findings: an export is a **snapshot**, so a
`<stem>.progress.json`, `<stem>.truth.json` or `<stem>.library.json` whose
state is now empty is
*removed* rather than left sitting beside a current sidecar asserting labels
the reader deleted (Minor 2, and the removal is named on stdout beside the
writes), and is not created in the first place when there was never anything to
say — `<stem>.library.json` used to be written unconditionally, so every
`--in-place` export left a `{"taiyaku_library": 1, "updated": "…"}` in every
work directory in the reader's library, whose only field changed on every run
(review 3's Minor 12) — **but only a document this library has had the content of**, read
into rows on an import or written out of them by an earlier export
(`migrated_document.absorbed`, review 3's C1). Overwriting a file and taking
one away are the same claim, that the database is the current copy, and it is
false for a sibling restored from a backup after the first import, for one
left unread because the rows already held state, and for a malformed one,
which is never read by design so that fixing it and importing again still
works. All three were being destroyed by a routine `--in-place` export; each
is now left exactly where it is and named on stdout as `kept`, with moving it
aside the way to get a fresh one written. `--dir` is exempt in both
directions: those paths are a destination a caller named, not the reader's own
documents. And under `--all` a work that cannot be exported no longer strands
the works after it — each failure is named on stderr, the rest are written,
and the command still exits non-zero naming the slugs that failed (Minor 1).
`--db PATH` on `serve`, `library import` or `library export` names the SQLite
file directly; without it the library lives at `$XDG_DATA_HOME/taiyaku/library.db`,
falling back to `~/.local/share/taiyaku/library.db`, and the resolved path is
printed to stderr on open so a person always knows which database a command
just touched.

`taiyaku library scan [--add PATH] [--root PATH] [--no-derive] [--db PATH]` is
spec B's scanner reaching a person: it walks every known root — or, under
`--root`, the one whose canonical path matches — records what it found and, by
default, goes on to derive series, works, editions and members from those
facts through `rules.rs`'s engine and raise whatever `proposals.rs` supports
against the new structure. `--no-derive` stops after the walk, which is what
makes "what would the facts be" answerable without changing any *structure* —
the walk itself still writes, `scan_entry` and `dir_state` both, since the
facts are what it exists to record. What it does not touch is `series`,
`work`, `edition`, `member` or `proposal`. `scan`
with neither `--add` nor an existing root refuses, naming `--add`, rather than
succeeding silently over nothing; `--root PATH` naming no known root refuses
the same way. `--add PATH` refuses, before adding a root, a path this
process cannot read, or one inside or containing a root the library already
holds, naming that root — the checks `POST /api/library/roots` makes, through
the same `scan::add_root_checked`. Each root's line is §8's report — `watching N directories —
scanning every 24h`, or a probe failure and the short interval, from
`watch.rs`'s two-second experiment (write a file, watch for the event) rather
than a guess from filesystem type — and §8.1's empty-root refusal is per
root: one that scanned empty while the library still holds entries for it is
named on stderr and skipped, and the run goes on to the rest exactly as it
does for an outright scan error (a permission refusal, say) — one root that
cannot be scanned does not take the others down with it. **It does take the
exit status down with it**, though, once every root has had its turn: a
scan whose only root refused used to exit 0 over having done nothing, the
no-roots refusal's own failure arriving by another door, and this file
already answered the identical question the other way for `library export`
— `export_targets` writes what it can, names every failure on stderr, and
still exits non-zero with a final line saying how many of how many failed,
"so a script sees it". `scan` now follows that precedent exactly: continue
and name each failure as it happens, and refuse at the end, naming how many
of how many, if any root did. What derivation produced prints as one line:
series, works, editions and members unconditionally, then the proposal count
— also unconditionally, so `0 proposal(s) raised` is a statement rather than
a silence — and `unattached chapter directories`, `unreadable`,
`suppressed`, `rekeyed`, `editions kept` and `merged` only when non-zero.
Those six are `DeriveReport`'s own counters: a candidate an ancestor
suppressed, a work whose stable address reconciled it onto a different row
than its content did, an edition this derive did not name and left standing,
and two entries that joined one edition. A counter that only ever prints
when there is nothing to report is exactly the silent failure they exist to
rule out — and the proposal count was itself an instance of it until the
whole-branch review's I1, printing an unconditional zero from a second
`raise` call whose `INSERT OR IGNORE` had nothing left to insert. Two things about it are not measured, per §13 of the
scanner design: what a scan costs on the deployment host rather than this
workstation's local disk — `docs/notes.md` has the walk, the sniff and the
first hash timed here, which is not that — and whether `watch.rs`'s probe
actually works on Unraid's `shfs`: the design's own §13 has the right
sentence for what is and is not known here, "it is designed to give a
correct answer there, not proven to."

`taiyaku library proposals [--min-score N] [--accept ID] [--dismiss ID]
[--unaccept ID] [--db PATH]` is the surface over §6's inbox
(`proposals::open`/`accept`/`dismiss`/`unaccept`), added because none of the
four had a caller outside their own tests before it — two rounds of Critical
fixes went into an `accept` a person could not invoke (whole-branch review's
I3). With no action flag it lists every open proposal at or above
`--min-score` (default the same `0.0` `proposals::open` itself defaults to),
one line each: id, kind, score and the human-readable `reason` a `raise` call
wrote. `--accept`/`--dismiss`/`--unaccept` each name a proposal and take the
one action; at most one of the three may be given at a time. *(A fourth since O72, 2026-09-15: `--reopen ID` takes a dismissal back, and the same at-most-one rule covers all four.)* Every refusal
`proposals::accept` returns reaches stdout unreworded — naming `taiyaku
align` for an `attach-chapters` proposal it cannot write a rule for, or
naming exactly what a swallowed directory would have taken — rather than
being summarised into something vaguer.

**The flat sibling layout raises a proposal that can never be accepted**, and
that is worth stating rather than leaving to be met. `accept` writes a
`prefix` rule on `path` derived from the one directory the subject's entries
share; where a series' own volume directories sit *directly* in the root — the
shape `Go-Toubun no Hanayome - v01/` beside
`Go-Toubun no Hanayome - Full Color Edition - v01/` — that shared directory is
the root itself, and no prefix narrows to only one of them. Measured
2026-08-18 against the shipped binary on exactly that tree: the scan raises a
maximum-score `edition-of` proposal (2.0, both signals) and accepting it
refuses with "this series' files sit directly in the root, so a `path` prefix
rule cannot narrow to only them". The refusal is correct — the alternative is
an accepted proposal whose rule decides nothing, or one that re-series the
whole root — but it means the layout that motivated the edition-key half of
the fix is one the inbox can propose and never apply. A `regex` rule written
by hand is the answer today, and a rule kind narrower than a path prefix is
what would change it.

`taiyaku library rules [--export | --import FILE] [--db PATH]` reads or
replaces §4's rule set — an ordered, user-editable list mapping a `rel_path`
onto a series, an edition key, a volume and a chapter, seeded with four
defaults on migration 3 and reachable no other way yet, since spec C4's
editor is not built. `--export`, or no flag at all, prints every rule as
JSON with the database's own row id left out — it names nothing once the
document moves to another host — and a rule scoped to one root is exported
by that root's own canonical path rather than its `root_id`, for the same
reason: a bare id would either miss on another host's `root` table or, worse,
silently land on whatever tree happens to hold that id there. `--import FILE`
**replaces** the whole set with what the file describes rather than layering
it over what is there, so importing the same four defaults twice cannot
double them, which matters the day someone has actually edited one — and a
rule whose path names no root this library has is refused outright, naming
the ordinal and the path, rather than silently scoping to every root or
handing back SQLite's own foreign-key error. Two more things §13 could
not measure: `proposals.rs`'s edition-of score rests on a roughly-20-to-17
hand classification of one library, made from knowing those series rather
than reproducible from data, and whether the four defaults this ships with
suit anyone else's library was never asked of a second one.

`inventory` and `duplicates` are the two halves of deciding which of several
copies of one volume to keep, and **neither of them moves, renames or deletes
anything**: they print what they found and leave the acting to a person.
`inventory` walks a release directory and takes as one candidate copy the
deepest directory that still holds a whole edition, so a volume whose chapters
sit in subdirectories counts once rather than seven times; it groups the
candidates by the volume number in the name — `v077`, `第077巻`, or a
`--pattern` where that fails — and prints every group with more than one copy,
together with what a content signature already settles about it. A group whose
name it could not parse is printed whether or not it is duplicated, because a
candidate that reached the page threshold and then vanished from the output is
the silence this whole project is written against. `duplicates` takes those
copies as `--`-separated groups, exactly as `align` takes members, and runs the
protocol over one set: signature and content hash first, then every unordered
pair solved once, then a clustering on identity, a flag on anything that refused
against every other copy, and a ranking of what is genuinely distinct. **Where
`--against` names a reference edition the reference does the flagging instead**,
because a copy that pairs with the reference is this volume however the set
treated it, and that is the only thing that can tell two copies refusing each
other apart. It prints
a table per set for a person and one fixed-field `set:` line for a survey, the
discipline `--stats` established; `scripts/duplicate-survey.sh` is that survey.

`credits` keeps that same discipline for a third question: which pages of an
edition are the scanlation group's own matter — donation cards, staff credits —
rather than the work. It proposes and prints; it writes the edition's `credits`
list only under `--write`. **Only one of its two signals classifies.** Signal 1
is `unpaired_runs`: the stretches of one member's pages the alignment against
the counterpart never reaches, split at member boundaries, and a run is a
candidate when it ends at its member's *last* page and is no longer than
`MAX_CREDIT_RUN`. Signal 2 is `recurrences`, how close each page comes to a page
in a different member by dhash; it is printed beside every row and consulted by
nothing, because on `corpus/gotoubun-v01` its two tightest matches in the whole
edition are the volume cover and it misses both staff-credit pages entirely.
The end-of-member rule is four for four on that volume and the first-index
version is zero for four — what accumulates at a member's start is front matter.
`MAX_CREDIT_RUN` is **1, measured** — surveyed over both fan editions the corpus
holds, 461 pages and four scanlation groups, where every credit page found is a
run of exactly 1 and the only run of length 2 or 3 is a volume's front matter.
`scripts/credit-survey.sh` is that survey. It cost the four-for-four claim above:
`kaguya-v01`'s one candidate is a **false positive**, a publisher's colour
announcement, so precision across the corpus is four of five. That group prints
no credit page at all. See `notes.md`, "A second group's habits, and what the
bound is worth".

**Re-measured 2026-08-13 over ten fan editions**, and the paragraph above is
kept rather than rewritten because three of the things it argues from turned out
to be properties of two groups rather than of scanlations. 1,938 pages, eight
scanlation groups, 25 known credit pages, every candidate extracted and looked
at: **signal 1 is precision 21 of 23, and recall 13 of 15 over the three
editions swept exhaustively** — the other seven had only flagged pages viewed,
so their denominators are floors. *Wave 2 has since taken this to thirteen
editions, and wave 3 to fourteen: 2,557 pages, twelve groups, 41 known cards.
Signal 1 is precision 23 of 25 and finds 23 of the 41 now known. **No rule is
correct on all of them, and the trade is a measured frontier rather than a gap
to close by trying harder:** holding the aspect test and sweeping the area
threshold gives zero false positives at 0.5x to 0.6x the volume's median page
area, missing 9 of 41, or zero misses at 0.8x to 1.0x for the price of four
false positives. **This is in progress and nothing in the binary depends on
it.**
Wave 2 added a group printing its card at a chapter's **start**, which signal
1's last-index rule cannot see and which contradicts `UnpairedRun`'s own doc.
Wave 3 added a webtoon whose alignment is **refused outright** — 0 whole
pairings of 321 slots, so signal 1 has no input at all — and whose six cards
the size rule misses, because tall scroll strips make the median page large
enough that a card clears the half-median cut. That blind spot was
pre-registered before the edition was viewed, and it is **relative** rather
than about resolution.* `MAX_CREDIT_RUN`'s justification no longer
holds either: `kobayashi-v01` page 152 is a credits card inside a run of 2,
which the doc comment says had never been measured. And a page's **size** turns
out to beat its position — area below half the volume's median *and* aspect more
than 15% off it is 25 of 25 on both counts, reads no alignment, and was
pre-registered against three held-out editions before they were viewed. See
`notes.md`, "Ten fan editions, and the three signals that did not survive them",
and `docs/superpowers/specs/2026-08-13-credit-size-rule-prediction.md`.

`regions` keeps that discipline for a fourth question and is **the first command
here whose answer is not yet good enough to use**. It proposes the text blocks
of one edition — page by page, in page fractions, with the axis the text runs
along — and writes them, under `--write`, to a sibling document
`<stem>.regions.json` rather than into the sidecar: regions are per page, and an
edition of 195 pages at a handful of blocks each is two orders of magnitude more
JSON than a file whose first two design goals are human-readable and diffable.
The document is the convention `<stem>.notes.json` and `taiyaku.stump.json`
already use, and an L0 reader does not merely ignore it — it never fetches it.
**Which block corresponds to which block on another edition is not stored**, and
that is deliberate: it is derived at read time from the alignment, so a third
edition costs one detection run rather than a re-pairing, a hand correction
re-pairs for free, and no stored derivation can outlive what it derived from —
which is O24's open failure mode. There is deliberately no `--cache`: the
fingerprint cache holds hashes and dimensions rather than pixels, so it could
not take effect, and a flag that cannot take effect is refused rather than
ignored. `scripts/region-survey.sh` is its survey. What that survey found is
below, under "What is known to be wrong or missing".

- **The web toolchain is pnpm, Node 26 and TypeScript 7** (2026-08-09). A
  toolchain branch that touched no Rust and no reader behaviour. Three things
  came out of it worth keeping: pnpm rejects the gitignored `file:` dependency
  outright and resolves it to a stale hard-linked copy when it does not, so the
  protocol is `link:`; the `wasm` job had been running the compiled module on
  Node 18 because that is what Debian bookworm ships, against 22 everywhere
  else; and `check-all.sh` now checks that `package.json` and the lockfile
  agree — measured across three runs at 249ms, 239ms and 237ms, a gap
  `docs/ci.md` had carried as permanent. TypeScript 7's `tsc` is a Go binary,
  which took the typecheck from 1.511s wall (3.19s CPU) to 0.164s and 0.166s
  across two runs (0.93s and 0.99s CPU), with no script change. bun was
  measured against, not merely dismissed: the whole JavaScript gate is about
  2.5 seconds, so there is no runtime budget to recover, and the one place a
  faster engine could apply is vitest, which bun cannot run in browser mode.
- **`clippy::pedantic` runs, advisory and non-blocking** (2026-08-10). This
  step first shipped carrying 100 per-site `#[expect(clippy::…, reason =
  "…")]` attributes; a same-day review then asked whether they could be fixed
  rather than documented; and a follow-up campaign fixed 60 of them outright
  — 3 `clippy::similar_names` renames, a `Pairing` enum replacing
  `Option<Option<String>>`, `SlotSpan` gaining its own serde impls, and 55
  casts routed through named `try_from(..).expect(..)` helpers that argue
  their bound once rather than at every site (`docs/notes.md` has the
  detail). The workspace now carries **40** per-site `#[expect]` attributes:
  33 on individual casts — 25 `clippy::cast_precision_loss`, for which no fix
  applies (all three routes measured; see `docs/notes.md`) — and 8
  `clippy::cast_possible_truncation`/`clippy::cast_sign_loss`, each kept for a
  reason specific to its site (four in `taiyaku-wasm`, where a panic traps the
  whole module rather than unwinding; two in `taiyaku-serve` tracing to
  request-path rather than in-process data; one each in `taiyaku-core` and
  `taiyaku-cli` with no viable conversion route) — plus 7 function-level
  (`clippy::too_many_lines` ×3, `clippy::needless_pass_by_value` ×3, and one
  `clippy::many_single_char_names` on `solve`, which judges a whole function
  rather than one binding). Plus two module-level
  `#[allow(clippy::pedantic, …)]`s, both on the one-off
  `*_experiment` modules and argued at the declaration, the sole exception to
  the per-site rule. `.forgejo/workflows/ci.yml`'s `lint` job and
  `scripts/check-all.sh` both run `cargo clippy --workspace --all-targets
  --all-features -- -W clippy::pedantic` after the blocking gate —
  `continue-on-error: true` in the first, `|| true` in the second — and both
  print, unconditionally, 15 `clippy::float_cmp` warnings from `assert_eq!`
  on floats in tests that `scripts/pedantic-sites.mjs` correctly excludes as
  not belonging to any file in this repository (see `docs/ci.md`, "The
  pedantic lints, and why they are advisory"). **The two halves of that
  mirror do not carry equal evidence.** `|| true` is a shell operator,
  verified on every local run of `scripts/check-all.sh`: the advisory step's
  own exit code is always swallowed, and there is nothing to check on a
  Forgejo runner that this repository's own commands can prove one way or
  the other. `continue-on-error: true` is Forgejo Actions' own key for the
  same behaviour, no other job in `ci.yml` uses it, and — since it can only
  fire when the step it guards actually fails — verifying it needs a step
  that fails on a pushed branch, not a local read. The branch has since been
  pushed and the pipeline came back green, but that does not verify the
  key: `cargo clippy -- -W clippy::pedantic` exits 0 regardless of how many
  warnings it prints — only `-D warnings` sets a non-zero exit — so the
  advisory step succeeded on its own and `continue-on-error` was never
  exercised. More fundamentally, the step cannot fail from a lint at all:
  its only possible failure is a genuine compile error, which the blocking
  clippy step above would already have caught, so both guards are
  belt-and-braces for a failure lints cannot produce today. The key is
  untriggered rather than untested; it has still never been observed doing
  its job.

## What is settled

Decided by measurement, not preference. Each of these was tried against an
alternative that lost.

- **dhash-16 over pHash.** 256 bits on luma, chosen on measured separation.
- **Margin over similarity as the confidence signal.** How far the best match beats
  the runner-up, not how good it looks in isolation.
- **A per-pair similarity floor**, calibrated from the pair itself rather than fixed
  globally. This is what refuses unrelated volumes.
- **Fragmentation as the quality signal**, read together with coverage — neither is
  sufficient alone.
- **Fragmentation is judged on average run length**, not runs per hundred pages.
  Measured across the corpus, every confirmed-good pairing runs 36.0 to 200.0
  slots a run and everything that must not pass 6.8 or below, with no overlap;
  the threshold sits at 27.0, three quarters of the weakest good figure, capped
  by the edition's own length because nothing shorter than 146 slots has been
  measured. Both measures separate on this corpus once refusals are gated on
  coverage; average run length was chosen because it states the property in
  slots rather than per hundred pages, so it does not shift with the edition's
  length.
- **Slots, not pages, as the addressed unit.** A slot is a whole page or one half of
  a joined spread. Spreads expand before solving, so the alignment stays strictly
  1:1 and the solver never learns they exist.
- **Reading direction is an input.** Monotonicity forbids reversing a pair, so it
  cannot be discovered.
- **Hub-and-spoke topology**, raw as hub, validated against all-pairs at ~0%
  disagreement, and **resolvable** — a lookup with no direct edge composes
  through the hub. Measured on the path a reader actually walks: 0.91% loss and
  0.00% disagreement over 2,427 comparisons on Go-Toubun v01.
- **One stored edge per spoke, reversed when needed.** The solver is symmetric on
  the pairing relation, measured across five pairs in two series, so N−1 edges
  carry every ordered pair rather than 2(N−1).
- **JSON for the sidecar**, with span notation (`7`, `4R`, `2-5`, `45R-46L`).
- **How a member becomes pages is part of the format, not an implementation
  detail.** The page sequence is derived, so a reader deriving it differently
  disagrees with every run while still matching every `sha256` and `pages` count
  in the file. Entries sort by whole path in natural order rather than as
  stored; non-images are not pages. Measured over 57 archives: getting the first
  wrong changes the sequence for 32 of them, the second shuffles chapters
  together on 2 of the 7 that nest, the third shifts everything after a
  `ComicInfo.xml` on 12.
- **A content-addressed fingerprint cache** keyed on member SHA-256. 3.4 s → 189 ms.
- **`pillarbox` as the transform a swap maps the visible rect through** — the
  shared artwork is as tall as the taller page says, and the wider-aspect page
  carries its extra as margin at left and right. Measured over the 284 slots
  whose search settled, of 288 sampled from twelve corpus volumes across four
  series: median residual **0.0195 of page width and 0.0041 of page height**,
  p90 0.0276 and 0.0116. It beat `identity` (0.0296 / 0.0041 — right on the
  vertical, uncorrected on the horizontal), `letterbox` (0.0296 / 0.0162, which
  is **worse than doing nothing**: it never rescales the axis that needed
  correcting and adds four times `identity`'s vertical error to the axis that
  did not) and reset-on-swap, which is a half-page bound rather than a residual.
  Neither of the two candidates the design named won, and `pillarbox` was not
  one of them; §5 below and `notes.md`, "Which derived transform maps a rect
  between editions", are the record.
- **Every `#[cfg(test)]` module lives in its own file**, declared `#[cfg(test)]
  mod tests;` beside the code it tests and written at `<stem>/tests.rs` — or
  `src/tests.rs` for a `lib.rs`, `main.rs` or `mod.rs`. Fifty modules moved on
  2026-08-14 and the 764 tests kept their names to the character, because a
  child module in a file has exactly the module path the inline block had and
  the same access to its parent's private items; `tests/` was not the
  alternative, since an integration test sees only the public API. The reason
  it is settled rather than cosmetic is that it changed what the coverage gate
  measures: cargo-llvm-cov drops a source file whose basename contains `tests`,
  so the test code stopped being counted as coverage of the code it tests and
  the workspace figure fell from 97.40% to **93.95%** with no test altered. Two
  `pub(crate) mod test_support` modules survived the tool's own rule for want of
  an `s` and are named in the `--ignore-filename-regex` instead, so the rule
  holds over the whole report: no `#[cfg(test)]` code is measured.
- **The web tests live in `web/tests/`, which mirrors `web/src/`**, and the
  helpers only tests use are in `tests/support/`. The user ruled it on
  2026-10-28, for the web alone: the Rust tests keep the layout the bullet
  above describes. The vitest projects look only in `tests/`, so a test file
  left under `src/` would never run, and `tests/srcHoldsNoTests.test.ts`
  fails while one is there, naming it. The source scans,
  `classNames.test.ts` and `colourLiterals.test.ts`, read the code they
  check from `src/` alone; `classNames.test.ts` also reads the tests' text,
  for the classes a test selects by. Spec
  `2026-10-28-web-tests-tree-design.md` is the record.
- **Coverage thresholds set at what was achieved, minus two points**, and
  proven by regression rather than asserted. Web: 96/92/97/97 under a measured
  97.72% statements, 94.05% branches, 99.16% functions, 99.31% lines —
  re-measured 2026-09-12, having been set against 98.03/94.38/99.29/99.53. **The
  branches margin got there by being worked back up, not by the bar moving**: it
  was 0.80 on 2026-09-11, and the cause was dilution rather than regression —
  the 66 files that existed on 2026-08-14 still measured 93.96%, while the 42
  added since measured 90.91%. See O103, which is also where the per-file
  branches floor turned out to be the tighter of the two. Rust:
  `--fail-under-lines 93` under a measured **93.64%** with twelve files ignored,
  plus a **per-file floor of 57% lines** under a lowest measured **78.75%**
  (`taiyaku-serve/src/lib.rs`) and a **minimum of 40 files** against **60** in
  the report — all four recomputed from `target/rust-coverage.json` on
  2026-08-18 under the gate's own ignore regex, having read 93.95%, 73.68% on
  `truth.rs`, 32 and 43 for long enough that none of them was true any more.
  The gate was green throughout, because every one of those thresholds is
  derived by `scripts/coverage-floor.mjs` from the data rather than typed in
  beside this prose — which is precisely how the prose went stale without
  anything failing, and precisely what this file warns about elsewhere. Those
  four Rust figures are implementation-only and were first re-measured on
  2026-08-14, when the test modules moved out of the files they
  test and stopped counting as their coverage; the same bullet read 95.75%,
  87.59%, 59.44% and 37 while they were inline, over a report of which roughly
  two thirds was test code measuring itself. **The aggregate's margin was 68
  lines on that measurement**, not the 857 it was, and 93 was kept rather than
  re-derived to 92 — a deliberate tightening, with the arithmetic and the escape
  hatch in `docs/ci.md`. (That margin moves every commit — 8 lines at one commit
  on 2026-08-17 and 64 a few commits later — so it is no longer restated here:
  `scripts/coverage-floor.mjs` derives it, and the files at or under it, from the
  report it already walks and prints both on every run, with `docs/ci.md`
  carrying the derivation and one dated measurement. It was four hand-maintained
  copies of one figure until then, and stale in all four.) Both gates were deliberately regressed and both proofs were
  re-run on the split tree: `#[ignore]` on all 100 tests in
  `taiyaku-cli/src/tests.rs` takes `main.rs` to 11.84% and the workspace to
  81.32%, exiting 1 with **zero bytes on stderr** while the per-file floor beside
  it names the file; skipping
  `viewport.test.ts` takes the web figures below all four thresholds and exits 1.
  It beat picking a round number up front, which is either unreachable or
  vacuous, and it beat gating Rust regions as well as lines, which needs a second
  number maintained for no extra signal. The Rust per-file floor also beat
  `cargo llvm-cov --fail-under-file-lines`, which exists in the pinned 0.8.7 and
  was measured exiting 1 with the ordinary coverage table on stdout and **zero
  bytes on stderr** — naming neither the file that tripped it nor the threshold,
  which above a 37-row table is not a gate anyone can act on. Its cost is a node
  install in a Rust container.
- **The regression proof found a hole in the gate it was proving, and the gate
  grew a second half because of it.** Skipping the whole of `pairing.test.ts`
  takes that file from 100% to 83.58% and the total only 98.03% → 97.49% —
  because `App.test.tsx` covers the same lines through a mounted reader — and
  **every aggregate threshold still passed**. A developer could delete a unit
  suite and be told nothing. `scripts/coverage-floor.mjs` is the answer: a floor
  per file, two points below each metric's lowest measured figure. It exits 1 on
  exactly that regression, naming the file and all three metrics that fell.
  vitest cannot express both halves itself — a glob entry in `thresholds` is
  aggregated over its matches, and `perFile` inside one is ignored; both were
  tried and measured before the script was written. **One script, two report
  formats, both gates**: it reads istanbul's `coverage-summary.json` for the web
  half and `cargo llvm-cov report --json` for the Rust half, detecting which by
  shape rather than by filename, and a report matching neither exits 2 rather
  than passing. One file beat two because the two checks are the same idea twice
  and only the adapter differs; the cost is that the differences between the
  formats — llvm-cov reports 0% for an empty metric where istanbul reports 100%,
  or the string `"Unknown"` on a collapsed report — all live in one table.
  `docs/ci.md`, "The coverage gates", is the record.
- **A minimum file count, because both halves used to pass on a report with no
  files in it.** Measured: with `coverage.include` pointed at a directory that
  does not exist, vitest prints `Unknown% ( 0/0 )` for all four metrics and
  **exits 0**, and the floor script printed "0 files, all above the floor" and
  exited 0 too — so a typo in one glob left the whole gate green while measuring
  nothing. **`minFiles` is 161 for web (168 measured) and 40 for Rust (82)**, and
  the web number is re-derived rather than scaled every time `web/src` grows by
  roughly ten files — it was 50 against 57 when this bullet was first written,
  59 against 66 after that, 101 against 108 after that, and 108 against 115
  after that, and it had stopped working by the time anyone looked on all four
  occasions. O98 is the second re-derivation; the whole-branch review of style
  pass ③, 2026-09-19, is the third; the web tests tree's branch, 2026-10-28, is
  the fourth — see `docs/ci.md` for the arithmetic, and for why seven files of
  margin is the rule rather than the number under it. **A margin rather than a
  round number**, and that is the
  measurement worth keeping: deleting one `*` from the include glob —
  `['src/*.ts']` — drops every file of `src/modules` and vitest reports every
  threshold passed, exit 0 (13 files and 97.79/93.90/99.20/99.48 when this was
  measured on 2026-08-10; 14 files and 97.25/92.43/99.11/99.18 when O98 re-ran it
  on 2026-09-11, against a tree grown from 57 files to 108; 14 files and
  97.64/93.89/99.14/99.29 when the whole-branch review re-ran it on 2026-09-19,
  against a tree grown from 108 files to 115). The
  per-file floor sees nothing either, because a file that is absent is a file no
  per-file check visits. Only a count can see it, and a count set with too little
  margin catches the bare collapse and lets that one through. On the Rust side the same check
  replaces a ritual `ci.yml` used to ask a reader to perform by hand — "check the
  file count when adding a crate". The cost is that a deliberate exclusion can
  trip it; the rule is to re-derive the number beside the exclusion rather than
  widen it on reflex.
- **The test count in this file is derived by a gate, because by hand it went
  stale twice in one day** — 639 stated against 656 actual, then 656 against
  658, each caught only because someone happened to look. `scripts/status-test-count.mjs`
  compares both stated figures against the JUnit report `cargo llvm-cov nextest`
  already writes — one line in `.config/nextest.toml` turns it on — so the check
  adds no build and no test run, and it runs in the `test` job and
  `check-all.sh` alike. **It cost 14.3 s a push until it did.** The first
  version shelled out to `cargo nextest list` for a *static* enumeration, which
  is genuinely the more robust source — it reads no environment and cannot be
  perturbed by a failing suite — and rebuilt the entire workspace to get there,
  since `llvm-cov` builds with instrumentation and a plain `list` cannot reuse
  those artifacts. That is 31% of the `test` job to check one number, and the
  three reasons written down for paying it did not survive being looked at:
  two output shapes is a local-fallback problem on the one path where the check
  is skipped anyway; the environment cannot reach a run both callers make with
  `TAIYAKU_CORPUS` removed; and a red suite is now *handled* — the script
  refuses to compare and names the failures — rather than avoided. The design
  was chosen first and defended afterwards, which is the reusable part. Proved by
  regression six ways: a stale figure and an added test each exit 1 naming the
  line, and a reworded claim, a run with failures in it, a zero-test report or a
  wrong shape each exit **2** rather than passing. It gates only the total,
  deliberately: the "thirteen standing checks" beside it is correct but
  separable only by a name prefix, and a gate resting on a naming convention
  would start lying silently. `docs/ci.md`, "The one documentation claim a gate
  can check", is the record.
- **`clippy::pedantic` is on as an advisory step, never in `[workspace.lints]`.**
  420 sites measured at `df8cbd9`; 375 fixed or given a per-site
  `#[expect(…, reason = …)]` stating the bound, and 45 in the two one-off
  experiment files covered by a module-level allow, argued at the declaration.
  It beat a blocking gate, which a floating toolchain turns red on an unrelated
  PR every six weeks or so; and it beat leaving the lint off, because `as` is
  invisible to every other instrument here — `docs/ci.md` records the debug
  profile as the only one with overflow checks, and overflow checks do not apply
  to `as`. The enforcement that remains is narrow and real: a stale `#[expect]`
  is `unfulfilled_lint_expectations`, an ordinary rustc lint the existing
  `-D warnings` gate already denies, so a cast deleted without its attribute
  fails CI.
  **The design's own mechanism for switching two lints off by name does not
  work, refuted rather than repaired** — a `[workspace.lints.clippy]` allow is
  overridden by the trailing `-W` of the pedantic group, measured directly:
  with `must_use_candidate = "allow"` in the table, all 47 of its sites still
  fired and the total stayed at 420. So both lints §10.1 would have switched
  off by name were *fixed* instead, and the sites supported that better than
  the design's own argument for switching them off did: all 47
  `must_use_candidate` sites were pure functions where discarding the result is
  a live bug, not the false positive the design assumed, and all 39
  `match_bool` sites were two-arm `match <bool> { true => …, false => … }`, not
  the exhaustive enum dispatch the design's argument described.
  **The branch found one live defect in the 131 casts it read** —
  `Vec::with_capacity(entry.size() as usize)` in `taiyaku-read`, twice, in
  `read_archive` and `member_page`. A `.cbz`'s central directory declares its
  own uncompressed size, unchecked against the real bytes until they are
  inflated; measured with a patched header, `entry.size()` reported 2 GiB
  against 104 actual bytes, attacker-controlled since `taiyaku serve` reads
  archives from requests. Bounded now by `MAX_ENTRY_CAPACITY_HINT` at 32 MiB —
  a whole-branch review found the first-cut 1 GiB ceiling reserved
  1,073,741,824 bytes for a 6-byte entry, and found the regression test that
  should have caught it self-referential (`capacity() <= MAX_ENTRY_CAPACITY_HINT`
  passes for any ceiling at or above the declared lie). 32 MiB is sized from
  the corpus itself — real pages run 100 KB–3 MB, a live client session
  transferred at 247 KB mean and 2.0 MB largest — not from
  `taiyaku_read::MAX_DECODE_ALLOC`, the separate ceiling that bounds a
  *decoded* pixel buffer rather than this *encoded*-bytes hint, and it matches
  `taiyaku-cli`'s independently-chosen `LARGE_PAGE_BYTES` on the same corpus.
  The regression test now asserts a literal reservation bound rather than the
  constant itself, and the identical clamp on `member_page` — the path
  `taiyaku-serve` actually calls per page request — has its own test too. The
  design read ten of the
  highest-risk sites before writing this plan and found zero live defects; the
  branch's full pass read 131, thirteen times that sample, and found this one.
  That is the result at 13× the sample the design's zero was drawn from, worth
  stating as a result rather than folding into the arithmetic as a footnote.

## What has been proven against real material

Everything here is a measurement against the library, not a synthetic test.

| Case | Result |
| ---- | ------ |
| Same volume, raw ↔ English | sound across eleven series |
| Colour ↔ monochrome | costs little; less than removing text does |
| Joined spread ↔ split pages | recovered symmetrically via slots |
| Chapters → volume | handled by multi-member alignment |
| **2 tankōbon → 1 deluxe volume** (Monster) | 427 of 444 slots, 13 runs |
| **4 volumes → 1 deluxe volume** (Nausicaä) | 553 of 600 slots, 20 runs |
| **Letterboxed spreads** (Tongari v09) | 176 of 180 slots once trimmed symmetrically |
| **All-spread edition ↔ ordinary** (One Piece v092) | 1 run, all 200 slots of the ordinary copy — which lies wholly *within* the all-spread copy's 202 |
| Unrelated volumes | refused |
| **A mislabelled file** (Bocchi *Gaiden* filed as v01) | refused, and its real counterpart found |
| Art book ↔ manga | pairs everything and should not — see below |
| **Twenty-two needles** (calendars, Jump issues, two art books) | 7 clear calibration and pair 100%; all 7 flagged `FRAGMENTED` |

## What is known to be wrong or missing

Stated plainly because each is a thing a reader would hit.

**A small needle in a large haystack always aligns — but `verdict()` already
catches it, and the tool was not saying so.** When side A is much shorter than
side B, the DP inserts gaps in B freely, so A comes back fully covered no
matter how unrelated the two are. Measured across twenty-two needles — One
Piece calendars, weekly Jump issues carrying chapters no volume in the pair
contains, and two art books — seven cleared calibration, and every one of the
seven paired 100% of its pages with nothing unmatched.

**Neither calibration statistic can refuse them, which is now measured rather
than suspected.** A One Piece art book against Go-Toubun no Hanayome — a
different series by a different artist — posts a margin of 0.020, beating four
confirmed-good pairings, and a separation of 0.145, beating seven. So margin is
not a weak signal but an inverted one, and `MIN_SEPARATION` cannot be raised to
catch needles without refusing genuine volumes. No margin guard ships.

What does separate is run structure: every needle that got through averages 1.1
to 1.7 slots a run against 36.0 to 200.0 for every confirmed-good pairing.
`verdict()` calls all seven `Fragmented`, and both `align` paths now print that
verdict rather than one of them discarding it. **The consequence for a reader is
that acceptance is a statement about calibration, not about the alignment —
only the verdict is the latter.** See "Twenty-two needles" in `notes.md`.

**Art books cannot be aligned and the solver does not say so.** Reading them works.
Linking their pages to the manga pages they came from is a different problem —
sparse, unordered, cross-composition — and wants nearest-neighbour search with a
margin threshold plus a crop-tolerant hash. Not the DP.

**`taiyaku regions` proposes text blocks and does not reliably find them.** The
command, the document format and the survey all work and are tested; the
detector behind them does not clear the L2 design's own bar. Measured over nine
editions of four works: `blocks_per_page` had to sit above 2 and **fails on all
nine**, reaching 1.86 at best and 0.59 at worst, which is most pages returning
nothing at all. The vertical/horizontal split does not reliably follow the
edition either — one volume comes back backwards on **both** sides at once, its
Japanese raw reading 71% horizontal while its English edition reads 66%
vertical.

**Now measured against hand-drawn ground truth rather than by proxy, and the
figure is 0.4%.** The dev set of `docs/superpowers/specs/…-l2-ground-truth-instrument-design.md`
was labelled: 45 pages across the same nine editions, **480 boxes**, one per
text run. `taiyaku score` over all of it:

| outcome | boxes |
| --- | --- |
| 1:1 | **2** |
| merged | 47 |
| split | 0 |
| fragment | 0 |
| missed | **429** |
| spurious | 3, or 0.07 per page |

Two of 478 scored boxes, both on `oshinoko-v01/en-mono`. Every figure above
this paragraph was a proxy for recall — `blocks_per_page`, the v/h split, block
composition — and each was right about the direction; this is the quantity
itself.

**The shape of the failure is the useful part, and it is what the five outcomes
were for.** `split` and `fragment` are both **zero**: the detector never
over-segments, so it is not finding text and cutting it wrongly. It is not
finding it. `spurious` is 3 across 45 pages, so it barely proposes anything at
all — and the 47 merged are what happens when the little it does propose lands
across several boxes at once. Counted directly on `gotoubun-v01/raw-mono`'s
five labelled pages: **45 truth boxes against 3 proposals**, with pages 9, 48
and 165 getting none whatever. Over that whole volume it emits 148 regions on
195 pages, **135 of which get nothing**, and the median region it does emit
covers **8.6% of the page** where a dialogue balloon covers about 1%.

**This is a floor, not a tuning problem.** With spurious at 0.07 per page there
is no precision traded against recall to recover, and `COVERAGE` — provisional
at 0.5, and still unmeasured — cannot be the explanation: no threshold takes 3
proposals to 45. Any candidate that proposes roughly one region per text run
improves both promotion terms at once, which makes this a cheap number to beat
and an honest one to be measured against.

**The L2b candidate has now been measured against that floor and is promoted:
48 of 478, at 6.73 spurious per page.** It is three changes to `detect_page`,
not one — a component-size band before the smear, a two-axis smear unioned and
deduped instead of one axis chosen per page, and `MIN_INK` counting ink pixels
instead of summed stroke bounding boxes — and its band was fixed by a sweep over
the same 45 pages rather than chosen. Both figures are sums over all nine
editions:

| outcome | incumbent | candidate |
| --- | ---: | ---: |
| 1:1 | 2 | **48** |
| merged | 47 | 73 |
| split | 0 | 152 |
| fragment | 0 | 81 |
| missed | **429** | 124 |
| spurious | 3 (0.07/page) | 303 (**6.73**/page) |

**The verdict, stated against the rule fixed on 2026-08-11 rather than against
the outcome:** a candidate is promoted only if it increases 1:1 *and* keeps
spurious-per-page at or under the dev set's own 10.7 truth boxes per page, ties
to the incumbent. Term 1 is 48 against 2. Term 2 is 6.73 against 10.7. **Both
pass, so it is promoted** — the first candidate on this problem that has been,
after two refusals under rules fixed the same way.

**The band was swept, and the losing rows are the deliverable as much as the
winner.** Eight values of `BAND`'s `lo` were scored over all 45 pages;
spurious-per-page falls monotonically from 294.40 to 0.42 across them while 1:1
peaks at `lo = 32` and falls away. `lo = 32` finds the most 1:1 matches of any
row — **57, nine more than the promoted band** — and is refused at 12.89
spurious per page. The whole table is in `notes.md`, "Where the band goes,
measured at eight bands over 478 hand-drawn boxes", and `band_sweep.rs`
reproduces it.

**What is promoted is still a detector that does not work, and the failure has
inverted rather than closed.** The largest column is no longer `missed` (124)
but `split` (152), with 81 fragments beside it: 233 of 478 boxes now carry a
prediction of the wrong shape, where the incumbent carried none at all. 48 of
478 is 10%, against phase 1's geometric ceiling of about 354 of 478 for any
smear-based detector. **Per kind, every dialogue hit is on an English edition** —
43 of 389 dialogue boxes overall, and 0 of 38, 0 of 40, 0 of 45 and 0 of 44 on
the four Japanese raw-mono editions — with 5 of 63 sound effects and nothing at
all among the 26 logo, credits, narration and other boxes. Vertical Japanese type
in a balloon is not being found. That per-kind split is the evidence the L2
design defers its C2 question (balloon-interior detection) to, which is why it is
recorded beside the total rather than under it.

**And `lo = 64` is not a boundary between two measured populations.** Phase 1 put
the outside population's p95 at 17‱ of page height and the inside population's
median at 8‱; the promoted cut is nearly four times the former and discards most
of what a person's own boxes contain. It wins anyway, because what it removes is
what welds — a result about the smear rather than about glyph size. 8 and 16 were
both swept and both lose.

**The cause is measured rather than guessed, and it is not the one that was
predicted.** The scale every gap is built from, `stroke_scale`, is the median
longer side over every ink component on the page — and 63% to 96% of those
components are **under 5 pixels**: screentone dots, JPEG ringing, antialiasing
fragments. So the scale collapses to 1–3px on pages 685–2147px wide and the
smear gap to 4–6px. The blob still forms, but not because the gap is large:
screentone packs ink densely enough that a few pixels of tolerance already lets
`merge_along`'s transitive passes weld thousands of specks into one component
spanning most of a page, which `MAX_BLOCK_AREA` then discards. On all eight
sampled non-blank pages it discarded the largest thing on the page, covering
33.6% to 96.6% of it.

**A passing `median_area` is not evidence of health**, which is worth stating
because it looks like it. `MAX_BLOCK_AREA` discards anything above 0.15 *before*
the median is taken, so that check is satisfied by the backstop rather than by
the detector finding text.

**Computing the scale over a population that excludes sub-glyph noise was
tried, twice, and neither attempt cleared the bar either.** L2A Task 9 fixed
an acceptance rule before measuring anything — beat `all` on both
`blocks_per_page` clearing 2 and the v/h split following the edition on both
sides of one work — then measured two candidates against it over the same
nine editions: `above-floor` (the median over components above a noise floor
derived from the page's own size distribution) and `modal` (the mode of the
size distribution rather than its median). `above-floor` is worse than `all`
on `blocks_per_page` for 7 of 9 editions, four of them dropping to zero
blocks across every sampled page; it overshoots past glyph size into art
size, because excluding the noise floor does not isolate a second, glyph-sized
population on most pages — there rarely is one — so the median of what is
left is a mix of type and everything else that is not screentone. `modal`
raises mean `blocks_per_page` from 0.84 to 1.02 but gets no edition newly
above 2, and neither candidate reaches even one work with a correct v/h split
on both sides, where `all` already reaches one (a fact this task also found:
the brief that set the bar believed nothing did). Per the rule fixed before
either candidate was measured, neither is promoted; the detector is
unchanged, and the constants' doc comments still say "a starting value, not a
measurement" because that is still true — this task measured the scale rule,
not the five thresholds built on it. See `notes.md`, "The text-block
detector, measured against four works, and why it does not work yet" and "The
scale rule, tried three ways, and neither candidate clears the bar".

**Nor is a passing `blocks_per_page` evidence of health, and that is the
sharper of the two.** The diagnosis above had a hole in it — the noisiest
edition sampled, `gotoubun-v01/en-mono` at 93%–99% of components under 5px, is
the *only* one of the nine to clear the design's bar, at 2.50, which the
screentone-welding account predicts should fail worst of all. Rather than start
a redesign on a mechanism that could not explain its own outlier, the same
harness was asked what that edition's surviving blocks are made of rather than
how many there are. **The median one holds 1,877 components whose median longer
side is 2 pixels**, on a 2147×3056 page — the same material as the page around
it, which is 97.6% sub-5px. 23 of the 25 are tagged with the wrong axis.
Meanwhile the edition whose blocks are made of markedly coarser components —
`oshinoko-v01/en-mono`, 12 of 14, correctly horizontal — scores 1.40 and
**fails** the bar. The two rank opposite ways under the count and under the
composition, so `blocks_per_page` cannot be the acceptance rule for whatever
replaces the detector.

**Ten of those blocks were then cropped and looked at, which corrected the
account of them.** Statistics alone had them down as fields of screentone;
they are **whole panels** — art, tone and the dialogue together — so the
detector on this edition is finding panels rather than text blocks. That
explains every figure at once: a panel sits comfortably under
`MAX_BLOCK_AREA`, so 2.50 `blocks_per_page` is roughly panels per page; a
panel has no reading direction, so the axis is wrong; a panel's components are
its screentone, so the median is 2px. One block of the six examined there is a
clean caption box. The same page also shows the opposite failure — seven
"blocks" that are vertical slices of one word — so one edition at one gap both
over-merges panels and under-merges lettering. `oshinoko-v01/en-mono`'s, by
contrast, were seen to be a title logo, a credits line and a caption box, plus
one panel. See `notes.md` for the table and for what ten blocks of 76 does and
does not establish.

**Why the noisiest edition scores best is the mechanism correction.** Not that
it escapes the welding — that it welds at a proportionally finer gap. The
noise-collapsed 2px scale gives the same ~4px gap everywhere, but on a page 3.1×
wider than `gotoubun-v01/raw-mono`'s that is **1.9‰ of page width against
5.8‰–14.6‰**. Where the coarser relative gap welds a page into one over-large
blob and `MAX_BLOCK_AREA` throws the whole page away, the finer one chops the
same screentone field into several *mid-sized* fragments, each under 0.15 of
the page — and `MAX_BLOCK_AREA` is the only filter with an opinion about size.
The edition scores best because its noise is cut into pieces the right size to
pass.

**Two further things about the mechanism were wrong and are now measured.**
First, the page-spanning blob is not always built by the smear: on **47 of the
89** sampled pages where `MAX_BLOCK_AREA` discarded something, the thing
discarded was already one connected component before `merge_along` ran at all —
7 of 10 on `gotoubun-v01/en-mono`, equal to the printed decimal. Denoising the
population that feeds `stroke_scale` cannot reach those pages; only changing
what `components` returns can, which bears directly on the route out of here.
Second, **`MIN_INK` has never rejected a block** — 0 blocks on 0 of 90 pages,
against `MAX_BLOCK_AREA`'s 166 on 89 — because it sums overlapping bounding
boxes, so 61 of the 76 surviving blocks score *above* 1.0 against a floor of
0.05. `MAX_BLOCK_AREA` is not the backstop the design calls it; it is doing
essentially all the filtering. That is O34. See `notes.md`, "The edition that
clears the bar is the one finding no text".

**Mutation testing has a harness since 2026-08-19, and it is not a gate.**
`scripts/mutants.sh` is the entry point — `--since main` for a branch,
`--crate`, `--shard`, `--file`, or the whole workspace — `.cargo/mutants.toml` holds
the config, and `.forgejo/workflows/mutants.yml` sweeps **one crate at a time,
sharded, dispatched by hand**, while **gating nothing** — `docs/mutants-baseline.md`
carries the dated log of when each sweep happened, which is what tells you one is
overdue. A full sweep is ~9.5
hours of runner time: 3,191 mutants at 3 h 12 m locally, and the runner is 3x
slower. Two reasons it cannot be a gate: **3 h 12 m** against
`check-all.sh`'s eleven seconds, and 92 untriaged survivors, which would make it
red on a clean tree for a correct reason — and a job that is red by default
teaches you to skip it. The shape is copied from `plaintextesports`'s
`live-smoke.yml`, the only other scheduled workflow across `~/projects`.

**`docs/mutants-baseline.md` is what makes the output readable, and it is the
part the roadmap entry did not ask for.** It records the survivors that have
been looked at and accepted, each with its reason, and `scripts/mutants.sh`
reports only what is *not* in it. Without that, every run prints the same
92 untriaged survivors and the one new entry is invisible among them. It
is also the only memory this project keeps of what has already been checked —
which is the thing four whole-branch reviews of spec A2 could not do. Coverage
records which lines ran; the baseline records which lines have an assertion
behind them, and it is committed, so the answer outlives the session that found
it.

**Spec A2's four modules had never been swept, and the first run found 30
survivors of 255 mutants in 23 minutes.** Ten were the session lifecycle:
`close_session`, `sweep_expired_sessions`, `sessions_of` and `revoke_session`
could each be replaced with `Ok(())`, `Ok(0)`, `Ok(vec![])`, `Ok(true)` *or*
`Ok(false)` with the whole suite green, because `account/tests.rs` named none of
them and `serve`'s route tests asserted through `session_owner` — which answers
`None` for a session never opened as readily as for one properly closed. **A
revoke that revoked nothing would have shipped**, on the route whose only job is
ending a session somebody believes is compromised. Three more were the
fail-closed answers argued for at length in comments and asserted nowhere,
including a whole `Ok(Attempt::Failed)` match arm that could be *deleted*,
falling through to the 401 beside it — the one answer that cannot be true when a
correct password's session write loses to a busy writer. `notes.md` has the
rest.

**Three of the thirty were code written that same morning**, and reading one of
them found a real defect rather than a missing test: `note_failure(pair).is_err()
|| note_failure(wide).is_err()` short-circuits, so a failure to record against
the pair key skipped the wide key entirely — the counter bounding the denial of
service, dropped on the path that most needs it. Both calls run unconditionally
now. That is the argument for the harness stated as an event: a defect
introduced at 09:00 was caught at 11:00 by a tool, rather than by a fifth review
round that was not going to be commissioned.

**Mutation testing found 346 survivors across the workspace. All five crates
are now closed, and every survivor that remains is either an equivalent mutant
carrying its argument or deliberately deferred.** Of the 346 — `taiyaku-core`
47, `taiyaku-cli` 217, `taiyaku-serve` 52, `taiyaku-read` 17, `taiyaku-wasm` 13
— read, wasm and serve are at zero, core is at three equivalent mutants and cli
at three more, with its remaining 44 deferred until L2's detector is redesigned
for the reason given below. `env -u TAIYAKU_CORPUS
cargo mutants --gitignore=true --test-tool=nextest` ran 1,660 mutants in 43
minutes on 2026-08-10: 1,149 caught, 346 missed, 136 unviable, 29 timed out —
75.4% of 1,524 viable mutants caught. `taiyaku-core`'s 47 were concentrated in
five files — `solve.rs` 26, `sidecar.rs` 11, `credits.rs` 7, `slot.rs` 2,
`lib.rs` 1 — and re-measuring the crate after the work gives **458 mutants,
411 caught, 3 missed, 38 unviable, 6 timed out: 97.9% of viable caught against
87.7% before.** All three remaining survivors are equivalent mutants carrying
their arguments, and both figures that moved outside the survivor count have
reasons rather than noise: two mutants disappeared because `percentile`'s
clamp turned out to be unreachable and was deleted, and one more became a
timeout because a new traceback test made a mutant hang rather than lie.
`notes.md`, "The mutation baseline, and the minus sign nothing was testing"
and "The 47, killed — and what the fixtures got wrong", has the full record,
including the gap penalty in `solve::solve`'s first column that could be turned
into a bonus
with all 526 CI-visible tests still passing, and the six planned fixtures that
passed while discriminating nothing. **`taiyaku-read` and `taiyaku-wasm` are now
closed too — 30 survivors to 0**, re-measured at 140 mutants, 110 caught, 22
unviable, 8 timed out. Their gaps were four, and each was a whole surface rather
than a scattering: the wasm shell's five value-forwarding methods, which
`binding`'s tests sit one layer below and cannot see; `tokenize`'s decimal-point
lookahead, in a file that carried one test; `MAX_DECODE_ALLOC`, a number nothing
asserted; and four odds in `pages.rs`. One survivor was not killed but
*deleted* — `unsupported_container`'s `len() > 262` was one past what the slice
it guards needs, and rewriting it as `bytes.get(257..262)` removed the
arithmetic along with three already-caught mutants at the same site. The eight
timeouts are all non-terminating `tokenize` loops and are O39's disposition
rather than untested behaviour. `notes.md`, "The other 30, and the four
surfaces they were hiding in", is the record. **`taiyaku-serve` is now closed too — 51
survivors to 0**, re-measured at 307 mutants, 272 caught, 31 unviable, 4 timed
out and none missed. Its gaps were concentrated rather than scattered: **27 of the 51 were in
`UreqTransport`**, the one component in the crate that talks to the outside
world — and the one every other test avoids by design, since `Transport` exists
precisely so the suite does not need the NAS. Nothing offline had ever driven
it, so `post_json` and `get_bytes` could each return a constant. They are now
tested against a hand-rolled `TcpListener` on a loopback port. Three more
survivors were decisions buried inside `eprintln!` arms — the warn-once latch in
`reqlog`, the "skipping N works" arithmetic in `resolve_all_works` — which no
test of either component's *output* could see; each is now a named function with
a return value. And `serve` itself could be replaced by `Ok(())`, starting no
server at all, with every other test green. `notes.md`, "The 51, and the client
that talked to nobody", is the record, including the comment in `get_page` that
justified a load-bearing filter with the condition backwards. A **fifty-second**
survivor appeared only while rebasing, on a mutant the earlier runs had caught
twice: the test guarding it is timing dependent and its own comment claimed it
was not. `notes.md`, "The mutant that was caught twice and missed once", is that
record. `taiyaku-cli` re-measures at **752 mutants,
217 missed**, 478 caught, 45 unviable, 12 timed out — the recorded figure
exactly. They fall in nine files, and the distribution is the useful part:
`main.rs` 70, `fingerprint.rs` 39, `detect.rs` 37, `report.rs` 30,
`inventory.rs` 19, `regions.rs` 7, `duplicates.rs` 7, `credits.rs` 5,
`cache.rs` 3. **`report.rs` is closed — 30 to 0**, verified at 40 mutants, 37
caught, 3 unviable; its gaps were `verdict`'s two margin thresholds, every
operator in the gauge arithmetic and the header's runs-per-hundred rate, and
`prepare`, which could return two empty vectors. Confirmed at crate scale as
well as in isolation: re-running the whole crate gives **752 mutants, 187
missed, 508 caught**, 45 unviable, 12 timed out — exactly thirty moved from
missed to caught, and nothing else shifted. `notes.md`, "The verdict bands, the
gauge, and two pages that hashed alike", is the record.
**`detect.rs`'s 37 and `regions.rs`'s 7 are deliberately not being closed**:
O31 to O34 record that the detector is measuring the wrong population and wants
a redesign, and pinning its present behaviour with 44 tests would cement what
those items exist to change.

**Five of the remaining six files are now closed — 73 survivors to 3**:
`fingerprint.rs` 39 to 1, `inventory.rs` 19 to 0, `duplicates.rs` 7 to 2,
`credits.rs` 5 to 0, `cache.rs` 3 to 0. All three survivors are equivalent
mutants and each carries its argument in `notes.md`: `flat_border`'s
crossed-scan guard, which the quarter-per-side caps make unreachable at every
page size from 8 pixels to 100,000, and two in `contradictions`, where a
singleton cluster has no pairs to examine and no producer of `Pair` ever builds
one judging a copy against itself.

Two shapes account for most of the 73, and neither is "untested code". **A
bound no fixture reached**: sixteen were the four `MAX_SIDE` caps in
`flat_border`, free because every letterbox fixture pads narrower than a
quarter, which is what a real letterbox does. **A decision inside a print
statement**: eleven were in `inventory`'s table loop, counted in a loop whose
only output is `eprintln!` — each now a named function returning a value, the
pattern `#41` established in `serve`. Three fixtures that could not fail turned
up on the way, including a ranking test whose tied clusters were listed in the
order its own assertion wanted, against a stable sort. `notes.md`, "The quarter
that never bound, and a constant no test could see" and "The loop that printed,
the test that copied it, and a fixture in the order of its own answer", are the
record.

**And `main.rs` is closed too — 70 to 0**, which finishes the 143. Its seventy
sat in five command handlers, almost none of them reachable from a unit test as
the code stood: `topology` runs against real archives, `align`, `verify` and
`lookup` print, `serve` starts a server. Three things carried it.
**Twenty-four were a single loop** — `topology`'s four per-page accumulators,
four running totals, verdict comparison and self-pair skip, all inside a
doubly-nested `for` whose only output is `println!`, now `ordered_pairs`,
`Divergence` and `PairReport`. **Fourteen died by deletion rather than by
test**: `Sidecar::edge_between` was already in `taiyaku-core`, tested and
`pub(crate)`, while `verify` and `lookup` each carried their own copy of the
two-way comparison — three copies of one predicate, and only the tested one
survived the change. **And `main` itself is only killable from outside the
process**, so `crates/taiyaku-cli/tests/cli.rs` spawns the binary and asserts
the two contracts `scripts/` depends on: a refusal exits 1 with `taiyaku: `
leading stderr and nothing on stdout, and no arguments prints the usage and
exits 0.

A fourth fixture that could not fail turned up here, written in this session
and caught only by re-measuring: `route_verdict`'s test compared `None` to
`None` throughout, because it named an edition the fixture does not define.
`notes.md`, "main.rs: 70 to 0, and fourteen that died by deletion", is the
record.

Confirmed at crate scale as well as file by file: re-running the whole crate
gives **785 mutants, 47 missed, 669 caught**, 57 unviable, 12 timed out,
against 752 / 187 / 508 / 45 / 12 before. The 47 are exactly `detect.rs`'s 37,
`regions.rs`'s 7 and the three equivalent mutants, and the 33 mutants the crate
gained are the extracted functions, every one of them caught. That leaves
**`detect.rs` and `regions.rs`** as the whole of what is measured and open, and
both are waiting on L2.

### Open items

Two hundred and thirty-one, each found by a whole-branch review, carried as a deferral,
turned up by reading a design against the code, turned up by running a
command against its own usage text, turned up by reviewing what the fix
before it left behind, turned up by measuring what a new feature or a fix
produced against the corpus, turned up by designing the fix for another one,
turned up by checking a diagnosis against the one case it did not explain,
turned up by simulating the reader from every position it can hold, turned
up by reading a file closely enough to write tests for it, turned up while
reading the code around a line a tool had pointed at, turned up by
hashing the pages a survey's own column described and finding it disagreed
with them, turned up by pointing a real OPDS client at a real library and
reading what it rendered, turned up by finding that the log which recorded
that session could not answer what was being asked of it, turned up by moving a
handler onto the async worker to see whether the test guarding it would notice,
turned up by writing the first code to read a column and finding nothing had
ever written it, turned up by pointing the scanner at a library somebody
actually keeps rather than at a corpus assembled to test it, turned up by a
CI run on `main` landing either side of a wall-clock second, turned up by a
review asking what a *new* feature's neighbours already fail to do, turned
up by holding one request open for ninety seconds against a real library and
reading a screen nobody had ever seen stand still, turned up by re-deriving
a gate's own margin that the branch before it had deliberately declined to
refresh, turned up by timing a stamp on the machine that will run it and
reading the code the stopwatch was for, turned up by timing a page turn
and reading which URLs the browser requested for it, turned up by
measuring what else stood where a fix had just moved a button, turned up by
five branches running their test suites on one machine at once, turned up by
what those branches found past the edges of their rows, turned up by
checking the premise a fix had been briefed on, turned up by the person
looking at a new screen on their own phone, turned up by an accessibility
audit of the restyled screens, turned up by typing a word into a search box
faster than the router could commit it, or turned up
by answering a question an earlier item's notes wrote down and left open, and
each judged not worth blocking on at the time. They
are tracked here rather than left in a scratch file so that closing one is a
visible act rather than a thing someone remembers. **One hundred and forty-three are now closed — O1, O2, O3, O4, O5, O7, O8, O10, O12, O13, O14, O15, O16, O17, O18, O19, O20, O21, O22, O25, O26, O27, O28, O29, O30, O33, O34, O35, O36, O37, O38, O39, O40, O43, O44, O47, O53, O54, O55, O57, O60, O62, O64, O65, O67, O70, O71, O72, O73, O75, O76, O77, O78, O79, O80, O83, O84, O85, O86, O89, O90, O92, O93, O94, O95, O96, O97, O98, O99, O100, O101, O103, O104, O105, O106, O107, O108, O110, O111, O112, O113, O114, O115, O116, O117, O119, O120, O121, O133, O134, O135, O137, O138, O140, O145, O146, O148, O149, O150, O151, O153, O154, O155, O156, O157, O158, O159, O160, O161, O163, O164, O165, O166, O173, O174, O175, O176, O177, O178, O179, O180, O182, O183, O192, O193, O195, O196, O197, O198, O199, O205, O206, O207, O208, O210, O216, O218, O219, O220, O221, O223, O224 and O228 — leaving eighty-eight.** *(This sentence carried **two** errors before ⑤c rebuilt it from the table on 2026-09-25: it omitted **O151**, closed on 2026-09-24, and "leaving fifty-seven" stood against sixty-two open rows. It is generated from the table now rather than edited, which is the only reason the list and the word cannot disagree again.* **A larger warning sits in how this note first read.** It claimed six errors — the word disagreeing with its own list, O34 and O39 listed twice each, and O31 named while its row read `[ ]`. None of those four existed. The check that found them anchored on the string `are now closed`, which first matches a sentence about mutation survivors a hundred and fifty lines earlier, and then read everything from there to the next `— leaving` — so it counted O-numbers out of hundreds of lines of unrelated prose and reported them as this list's. The rebuilt sentence was right and the story about why was invented, which is worse than the drift it was describing: a wrong count invites a recount, and a wrong history is believed. Caught by ⑤c's pre-merge claim check, which re-derived it from `git show` rather than from this paragraph.)* *(It read "leaving fifty" against
forty-nine open rows from the moment O97 closed: the count of closed items
moved and the count of open ones did not, which is the drift the sweep at the
head of this section exists to catch and did not run. Corrected here. **It
reads "leaving fifty" again as of 2026-09-08 — this time against fifty open
rows, because O102 opened.** The coincidence is worth the sentence: a reader
who remembers the drift would otherwise read the correct number as the old
wrong one. **And it stopped reading fifty on 2026-09-09, when O92 closed and
took it to forty-nine** — this sentence, whose whole job is to stop a reader
misreading the number, had itself gone stale by one close, and the
whole-branch review of O92's own branch is what caught it. A parenthetical
that explains a count is a claim about that count and drifts exactly like
one.)* *(This read "leaving thirty-one" both
before and after the edit that opened O67, which was not an oversight: O55
closed and O67 opened in the same session, off the same device run. O68 opened
from that same run afterwards, and moved it.)* *(O105 and O106 each opened and
closed on one branch, so each moved the closed count and not the open one.
O107, opened by O105's whole-branch review, and O108, opened by the review of
both branches' fix rounds, moved the open count to fifty. O108 closed on
2026-09-13 on a branch of its own, and took it to forty-nine. O109 opened the
same day with no code change, to give an open row to a decision #118 had
recorded only on O100's closed row, and took it back to fifty. O107 closed the
same day on a branch of its own, once a page turn had been timed, and took it
to forty-nine. O110 opened the same day with no code change, for a defect
O107's own timing found, and took it back to fifty. O111 opened the same day
with no code change, for the permit wait O107's timing measured and left, and
took it to fifty-one. O110 closed on 2026-09-14 on a branch of its own, once a
page turn had been timed before and after, and took it back to fifty. O65
closed the same day on a branch of its own, once the reader had been driven
before and after, and took it to forty-nine. O112 opened the same day with no
code change, for what O65's pre-merge check measured beside that fix's own
cost, and took it back to fifty. O75 closed the same day on a branch of its
own, once React's warnings had been counted with the output visible, and took
it to forty-nine. O47 closed the same day on a branch of its own, once
`library scan --add` had been run before and after, and took it to forty-eight.
O79 closed the same day on a branch of its own, once a database error had been
placed partway through a progress document, and took it to forty-seven.
O113 opened and closed the same day on one branch, as O105 and O106 did, so it
moved the closed count and not the open one. O71 closed the same day on a
branch of its own, once each handler it named had been moved onto the async
worker by hand, and took it to forty-six. O94 closed the same day on a branch
of its own, once the account screen had a route and had been driven in a
library holding no works, and took it to forty-five. O78 closed the same day on a
branch of its own, once a fold had dropped a note copy identical in every
column and still refused one that differed, and took it to forty-four. O70 closed the same day on a
branch of its own, once a store fault had been raised inside each of the three
writes, and took it to forty-three. O83 closed the same day on a
branch of its own, once a series' titles had been read back, corrected and sent
whole over real archives, and took it to forty-two. O114 through O118 opened the
same day with no code change, from what the O70, O71, O78 and O83 branches met
past their own rows, and took it to forty-seven. O114 closed on 2026-09-15 on a
branch of its own, once a series' titles had been corrected and the root
scanned again over real archives, and took it to forty-six. O115 closed the same
day on a branch of its own, once each handler that asks a grant had been given
its check back on the async worker by hand, and took it to forty-five. O117 closed
the same day on a branch of its own, once the collision had been reproduced on
purpose and every test's temp directory came from one helper, and took it to
forty-four. O119 opened and closed the same day on one branch, as O113 did, so
it moved the closed count and not the open one. O116 closed the same day on a
branch of its own, once each browse handler had been put back on the async
worker by hand, and took it to forty-three. O77 closed the same day on a branch
of its own, once a test naming its guard had failed against each half of it
mutated by hand, and took it to forty-two. O33 closed the same day on a branch
of its own, once each of its two filters had been removed by hand and a page it
refuses had come back, and took it to forty-one. O120 opened and closed the same day on
one branch, as O105 and O106 did, so it moved the closed count and not the open one.
O72 closed the same day on a branch of its own, once a dismissal had been found
under its own filter and reopened in the browser, and took it to forty.
O76 closed the same day on a branch of its own, once a fold had been driven over
real archives before and after, and took it to thirty-nine; O121 opened and closed
on that branch, as O113 did, so it moved the closed count and not the open one. O122 through O125 opened the same day with no code change, from what that six-branch queue left unfiled, and took it to forty-three — a sentence this parenthetical did not get until O89's branch counted the table against it, which is the drift the sweep at the head of this section exists to catch. O89 closed on 2026-09-16 on a branch of its own, once a real library had been re-derived against figures written down in advance and each of eighteen hand mutations had failed a test, and took it to forty-two. O126, O127 and O128 opened on that branch, from what that re-derive and the branch's whole-branch review met past O89's own row, and took it to forty-five. O111 closed on 2026-09-17 on a branch of its own, once a page that fits had been timed behind a cold filmstrip before and after, and took it to forty-four. O129 opened on that branch, from a hand mutation no test caught, and took it back to forty-five. O90 closed on 2026-09-17 on a branch of its own, once the scan said why each entry it skipped was skipped and a copy of the real library had named its one remaining file, and took it to forty-four. O130 and O131 opened on that branch, from what that one file turned out to be, and took it to forty-six. O132 and O133 opened on 2026-09-18 on the style pass's ② branch, from its design and its device pass, and took it to forty-eight. O133 closed on 2026-09-19 on a branch of its own, once the user's phone had fetched every cover at 768 px and a download-only account had been served every cover size, and took it to forty-seven. O112 closed on 2026-09-19 on the style pass's ③ branch, once the user had chosen among its options with the new bars drawn, and took it to forty-six. O134 and O135 opened on that branch from its design, and O136, O137 and O138 from its browser pass and its device pass, which took it to fifty-one. O137 and O138 both closed on 2026-09-20 on the style pass's ④ branch — the first with two exits built for it, the second by measuring that its subtraction is a platform setting and not this code's — taking it to forty-nine; and O139, O140 and O141 opened on that branch, from what the user found on their phone and from what a hand-accepted edition proposal put on screen for the first time, which took it to fifty-two. O142 opened on that branch too, on 2026-09-20, from a mouse/touch asymmetry the review of fix round 2 found in a release that slips from the page onto a bar, which took it to fifty-three. O140 closed on 2026-09-21 on a branch of its own, once the chrome had been opened before and after on the user's iPhone and the strip flicked after, and took it to fifty-two; O143 and O144 opened on that branch, the first from the candidate fix it declined and the second from what its device pass found six mis-marked double-page scans doing to a windowed rail, which took it to fifty-four. O145 and O146 opened on 2026-09-21 with no code change, from two questions #167 left open rather than answered — what subtracts the 15 px, and the fact that its own worst defect is held by a stubbed test and a hand-run browser check — which took it to fifty-six. O134 closed on 2026-09-22 on a branch of its own, once the user's iPhone had been served the strip and every tile it asked for had been counted off the cache at 320 px, and took it to fifty-five. O147 and O148 opened on 2026-09-22 with no code change, from what O134's whole-branch review found past the edges of its row — that `thumbnail_at` scales up as readily as down, and that the inbox's restated tile height has none of the guard the strip's has — which took it to fifty-seven. O149, O150 and O151 opened on 2026-09-23 on the style pass's ⑤a branch — the first two from its design, which declined both, and the third from its re-review, which found the gate built for ⑤a's own iOS-zoom Critical could not see the same defect one stylesheet over — which took it to sixty. O151 closed on 2026-09-24 across two branches — #173 scoped the floor to the reader, and the style pass's ⑤b branch widened the gate to the two shapes that had hidden it — and took it to fifty-nine. O152, O153 and O154 opened on that ⑤b branch and took it to sixty-two: the first from a mutation that *failed to fail*, which is how the gate's third limit was found rather than assumed; the second from a browser check, where making two paragraphs quiet left a third beside them loud; the third from its Task 1 review, which noticed one screen grouping checkboxes two different ways. **And the sentence at the foot of this file said fifty-seven while this one said sixty**, unmoved since O149 through O151 were filed on ⑤a's branch — three behind, corrected here, and the same drift this parenthetical already records catching twice before. **The count word at the head of this section was eight behind when O147 and O148 were filed**, unmoved since #165 while O139 through O146 opened; corrected here, and it is the drift the sweep at the head of this section exists to catch. O148 and O150 both closed on 2026-09-25 on the style pass's ⑤c branch — the first once the constant was derived from the stylesheet and four hand mutations had each gone red on the assertion meant to carry them, the second with all four candidates drawn on the real screen before the user chose — taking it to sixty. O155 through O158 opened on that branch and took it to sixty-four: two from its design and its browser survey, where the same screen calls thirteen of seventeen rows weak under a control offering to show weak ones, and marks a chosen one-of-N two different ways across four rows; the third from its Task 7 re-review, which found three controls under the 44 px floor that branch had just built and a comment deferring them to a pass that does not own them; the fourth from its own rename, where a class renamed in a component and its test but not its stylesheet would have left every gate green. **And the correction itself had to be corrected**: the closed sentence at the head of this section was first reported as carrying six errors, and it carried two — an omitted O151 and a remainder five behind. The other four were an artefact of a check that matched the wrong sentence and read across a hundred and fifty lines of unrelated prose. It is generated from the table now rather than edited; the invented history is recorded there beside it, because this parenthetical exists to record what drifted and a fabricated drift is the one kind it must not carry. O159 opened on that branch on 2026-09-25 and took it to sixty-five, from its device pass: on a phone, toggling one permission checkbox greys every control in that account's card for as long as the list takes to come back. **It is filed against a fix that is already refused in writing** — the flag is held across the whole row deliberately, and the comment saying why names the silent data loss that scoping it per control would bring back — which is the reason the row proposes a visual answer rather than the obvious behavioural one. O149, O153, O155, O157 and O159 closed on 2026-09-26 on the admin follow-ups branch, built as the options the user chose on 2026-09-25, and took it to sixty. O160 opened on that branch, noticed while drawing O153, and took it to sixty-one. O161 through O165 opened on 2026-09-30 on the accessibility audit's branch, with no code change, from its desktop half and the user's VoiceOver pass on their iPhone, and took it to sixty-six. O166 through O172 opened on the accessibility pass's reader branch — O166 from its design, reproduced before it was filed and closed the same day, O167 and O171 from its device pass, O168 through O170 from its reviews, O172 from its second claim check — and O135 and O161 closed there on 2026-10-01, with O162 narrowed and left open for a tester who knows VoiceOver; together they took it to seventy. O173 through O177 opened on the accessibility pass's markup branch — O173 and O174 from its design, folded into it by the user, O175 from its evidence pass, O176 and O177 from its reviews — and O154, O156, O163, O164, O165, O173 and O174 closed there on 2026-10-01, which took it to sixty-eight. O176 and O177 closed on 2026-10-02 on one branch between them — the first once a typed `/read/nope` had drawn its screen in Chromium on a copy of the real library, the second once a new case had failed against the rule's defect and passed against the rule — and took it to sixty-six; O178 and O179 opened on that branch, the first from what O177's fallback-font failures turned out to be and the second from the pixels of O176's screen, which took it back to sixty-eight. O178 closed there the same day, by the user's decision, once a scan report the server made had been driven before and after on a scratch library, and took it to sixty-seven; O180 opened and closed there too, from that drive, and so moved the closed count and not the open one. O158, O160 and O179 closed on 2026-10-03 on one branch, each built in a worktree of its own, O179 by the user's choice that day among four ways to fix it, and took it to sixty-four. O146 closed on 2026-10-04, and O181 opened the same day from the mutation pass that closed it, which left it at sixty-four. O145 closed the same day, on the same branch, and took it to sixty-three. O175 closed there too, by the user's choice that day, and O182 opened from its evidence pass, which left it at sixty-three. O182 closed the same day on a branch of its own, and O138 reopened there by the user's decision, which left it at sixty-three. O183 opened and closed on the live-works branch, as O105 and O106 did, so it moved the closed count and not the open one. O184, O185 and O186 opened on that branch, the first from the re-review of its own whole-branch fix round and the other two from the deployable branch's second re-review, and took it to sixty-six.)*

**What spec A2's fourth review reported and nobody has verified.** Four rounds
returned fifteen findings each. Round four's fifteen were: two fixed as security
defects (the claim route reading `admin_exists`, and the lockout key's
attacker-supplied name), two more fixed while in the same lines (`promote --role
user` bricking the library, `change_password`'s constant limiter key), one opened
as O57 and one as O58. **The remaining nine, plus roughly eighteen the review
ranked below its own cut, were reported and never checked** — and they are listed
here rather than as rows because an unverified claim in the table would be read
as a finding. What was named: `client_address`'s last-entry rule being wrong for
two proxy hops; `effective_host` never reading `request.uri()`;
`per_reader_by_default`'s `no-store` killing the ETag revalidation on the four
routes that implement it; import attribution using `MIN(id)`; `record_absorbed`'s
`NotFound` arm; `sessions_of` listing sessions that have expired; `label.html`
having no Login; and the account overlay having no focus trap.

Two of those are worth a sentence on why they are not simply defects. The
`no-store` one is a regression the third round's own fix introduced, so it is the
most likely of the nine to be real. The `sessions_of` one interacts with
`sweep_expired_sessions`, which now has a direct test, so it may already be
answered. **Neither has been reproduced**, which is the whole content of this
paragraph: they are a list of things to look at, not a list of things that are
true. The rate did not fall across four rounds and a fifth was not commissioned;
this is what that decision left on the floor, written down so it is a choice
rather than an accident.

*O41 had a prose bullet below and no row in the table until O40 closed, so this
sentence said "Forty-one … leaving ten" against a table holding forty rows and
nine open ones, while "What remains" further down agreed with the table rather
than with this sentence. The row is added above rather than the count reduced:
the item is real and measured, and it was the row that was missing. Caught the
same way the drift noted under "What remains" was — by counting the table's
rows against its own preamble, which is one grep — and worth recording because
every earlier drift here happened while **closing** an item, and this one
happened while opening one. Opening is the longer edit of the two.*

*O42 landed the same way and this sentence was not updated to match: it still
said "Forty-one … leaving nine" against a table holding forty-two rows and ten
open ones, from when the library store's own Task 13 review added it through
to this review adding O43 beside it. Both are folded into the one fix here
rather than caught and fixed separately, since a documentation review reading
this table closely enough to add a row to it is exactly the position this
drift needs to be caught from.*

Three of those closed the same day they opened, which the "judged not worth
blocking on" framing above does not fit: O35, O36 and O37 were found by the
coverage branch and fixed on it. They are recorded here anyway rather than only
in that branch's entry, because this table is where a reader looks for what was
wrong and whether it still is, and a defect that never appears here because it
was fixed quickly is a defect nobody can later find.
Closed rows stay in the table rather than being deleted, because a list that
only ever grows says nothing about whether anything gets fixed. The evidence for
each is in the detail below; the table is what a reader scans.

| ID | Open | What | Where | Opened by |
| --- | :---: | ---- | ----- | --------- |
| O1 | [x] | Page responses carry no cache headers — *closed by removing the round trip rather than by counting them: 0 page requests reached the server across a ten-slot re-read* | `routes.rs::get_page` | M2 review |
| O2 | [x] | Progress is written non-atomically | `progress.rs::store` | M2 review |
| O3 | [x] | `PUT /progress` does not range-check the slot | `routes.rs::put_progress` | M2 review |
| O4 | [x] | Two sidecars claiming one work id is untested | `works.rs::load_works` | M2 review |
| O5 | [x] | A non-numeric page index answers axum's default 400 | `routes.rs` | M2 review |
| O6 | [ ] | A cover from page 0 can be a 430 MB decode — *measured: 96–221 ms on real material* | `cover.rs` | M3 review |
| O7 | [x] | The request log grows in memory for nothing | `reqlog.rs::Log` | M3 review |
| O8 | [x] | Stump resolution blocks the listener | `lib.rs::serve` | M3 review |
| O9 | [ ] | No single-flight guard on the request path — *measured: nothing to coalesce* | `composite.rs::StumpSide` | M3 review |
| O10 | [x] | `MAX_DECODE_ALLOC` is duplicated across crates | `cover.rs`, `taiyaku-cli/src/fingerprint.rs` | M3 review |
| O11 | [ ] | Neither on-disk cache notices a change on the far side — *escape hatch added; covers measured at 165 KB, filmstrip tiles at 1.12 MiB per 195-page edition*. **Since O134 (2026-09-22) a page has a tile file per size a screen has asked for, not one**: measured on a 237-page edition, 1.57 MiB at 160 px, 5.18 at 320 and 10.52 at 480, so a library read on a 3x phone and a 2x desktop holds both of those sizes beside the 160 (`docs/notes.md`, "O134: what a sharper tile costs"). Each size is written only when some screen asks for it, and nothing evicts any of them — which is this row's own subject, now with a bigger number behind it | `stump/cache.rs`, `cover.rs` | M3 review |
| O12 | [x] | `spawn_blocking` covers three of five blocking handlers | `routes.rs::get_progress`, `::put_progress` | M1/M2 deferral |
| O13 | [x] | A hand-corrected run does not survive a re-solve | `sidecar.rs::merge_spoke` | M4 planning |
| O14 | [x] | The binding rebuilds the route on every resolve | `binding.rs::Binding::resolve` | M4 whole-branch review |
| O15 | [x] | A flag typed with no usable value was taken as absent, in silence | `main.rs::option_value` | duplicate-resolution review |
| O16 | [x] | A symlink cycle truncates the inventory walk — *silently, not by crashing* | `inventory.rs::totals` | duplicate-resolution review |
| O17 | [x] | The content-hash confirmation re-reads a copy once per candidate | `duplicates.rs::representatives` | duplicate-resolution review |
| O18 | [x] | The `--against` flagging rule had no corpus witness | `duplicates.rs::resolve_with` | duplicate-resolution review |
| O19 | [x] | `align` without `--split-spreads` ignored the whole sidecar-writing half of its flags | `main.rs::align` | O15 scoping |
| O20 | [x] | Six more `align` flags were ignored where they could not take effect | `main.rs::align` | O19 fix |
| O21 | [x] | A misspelled flag was accepted in silence by every command | `main.rs::reject_unknown_flags` | O20 review |
| O22 | [x] | `verify` ignored `--cache` and `--ltr` with no `--sidecar` to apply them to | `main.rs::verify` | O20 review |
| O23 | [ ] | Two hand corrections make a sound alignment report `fragmented` — *measured: avg run 64.33 → 32.00 → 21.22 on Go-Toubun v01, against a 27.0 threshold* | `sidecar.rs::rewrite`, `solve.rs::verdict` | M5d |
| O24 | [ ] | A solver-written direct edge to a third edition survives that edition being replaced | `sidecar.rs::merge_spoke` | M5d whole-branch review |
| O25 | [x] | The settings panel and the whole chrome are unreachable while showing a gap | `App.tsx::App`, the `at` narrowing | M5c browser pass |
| O26 | [x] | `align --write` truncates the sidecar before writing it | `main.rs::write_sidecar` | credit-pages whole-branch review |
| O27 | [x] | Two refs are held and read by nothing | `App.tsx::App`, the ref block above `viewportWidth` | O25 design |
| O28 | [x] | A forward arrow on the last group's second slot moves backward | `pairing.ts::stepToGroup` | credits-panel arrow simulation |
| O29 | [x] | A failed `--write` names a temp path the caller never typed | `atomic.rs::write` | O26's corpus drive |
| O30 | [x] | A relative `TAIYAKU_CORPUS` skipped every standing check and reported `ok` | `corpus.rs::corpus`, `corpus.rs::corpus_root` | O26's corpus drive |
| O31 | [ ] | The text-block axis is decided per page, where the design says per cluster — so a page cannot report vertical dialogue beside a horizontal SFX, and the survey's `v`/`h` figures are per-page decisions counted per block — *all three of those consequences are addressed by L2b Task 5 (`8531d05`): `axis_of` is `#[cfg(test)]`, `detect_page_with` unions both axes and dedupes with `uncontained`, and `region_experiment::summarize` counts per surviving block. **Open on what replaced them**: where both smears produce an identical box the axis reported is the dedupe tie-break's default rather than a measurement — 13 of 69 blocks changed label on an unchanged box over the 90-page sample — so `Region::axis` is still not "what the clustering knows", which is what this item tracks* | `detect.rs::detect_page_with`, `detect.rs::uncontained`'s tie-break | L2a whole-branch review |
| O32 | [ ] | The `median_area` disqualifier cannot fire — `MAX_BLOCK_AREA` clamps every region under it before the median is taken | `detect.rs::detect_page_with`, `regions.rs::median_area` | L2a whole-branch review |
| O33 | [x] | Two of the five detection constants reject nothing in any test, so a computation bug in either would ship unseen — *the gap is in the tests and stays open; against the corpus `MAX_BLOCK_AREA` fires on 89 of 90 pages and `MIN_BLOCK_STROKE_SPAN` on 1* — *closed 2026-09-15 by two tests, each a pair of pages one pixel either side of its constant, whose refused page comes back when that filter is removed* | `detect.rs`, the `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` filters; `detect/tests.rs`'s `a_block_shorter_than_two_stroke_scales_is_not_a_region` and `a_block_over_the_area_ceiling_is_not_a_region` | L2a whole-branch review |
| O34 | [x] | `MIN_INK` cannot reject anything on real material — it sums overlapping stroke bounding boxes, so 61 of 76 surviving blocks score above 1.0 against a floor of 0.05, and it rejected 0 blocks on 0 of 90 sampled pages — *closed by `aa0b479` (L2b Task 6) taking the first of the two routes this item named: it counts ink pixels at the page's own Otsu threshold, so the quantity is bounded by the block's own area by construction rather than by the material, and the ratio across the same sample fell from 0.38–1.89 to 0.09–0.56. It now rejects: 16 spurious blocks over the nine labelled editions, spurious-per-page 55.31 → 54.96. **Band-free it still rejects nothing on the 90 sampled pages** — the filter is live in the shipped, banded detector and not in the population `region_experiment.rs` samples* | `detect.rs::detect_page_with`, the `MIN_INK` filter and `ink_in` | the survivor-composition measurement |
| O35 | [x] | A `PUT` response with no `ETag` turns every later note write into a silent no-op — the same "write without a validator" hole the 412 path was fixed for, left open on the response path — *closed on the branch that found it (178f3e5): the guard became `writeGate` with three arms — `wait` (no document, the only silent one), `refuse` (document but no validator, carrying a notice), `write` — after the correct-behaviour test was watched failing against a faithful port* | `api.ts::putNotes`, `App.tsx::writeNotes`'s `notesEtag === undefined` guard | coverage-gate reading |
| O36 | [x] | The two write debounces are never cancelled on unmount, so an unmounted `App` can still `PUT` up to 700 ms later — *a two-line fix, not an impossibility: `useFlushOnHide`'s cleanup already exists and `debounce` already exposes `cancel()`* — *closed on the branch that found it (4303dee): `useFlushOnHide`'s cleanup now flushes both, decided per call site: cancelling `writeProgress` drops the last page turn, cancelling `saveNotes` destroys up to 700 ms of prose that exists nowhere else* | `App.tsx`'s `writeProgress`/`saveNotes` handles, `hooks.ts::useFlushOnHide` | coverage-gate reading |
| O37 | [x] | `reviewNotes`'s doc comment describes an arm of `toggleReview` no key can reach — it claims `a` closes the editor mid-edit, which `keyplan.ts` refuses; the code is right and the comment is wrong — *closed on the branch that found it (f4047b7): corrected, and the same wrong claim in `notes.ts` with it in `7cab6ec`* | `App.tsx::reviewNotes`'s doc comment | coverage-gate reading |
| O38 | [x] | `unpaired_runs` counts a gap run's own `a` span as "mentioned", so asking the `from` side of an edge which of its pages have no counterpart answers `[]` for pages that demonstrably have none — reachable today as `taiyaku credits <hub member>... --sidecar F --against <spoke>`, which prints zero candidates — *closed by `run.b.map(\|_\| run.a)`, with a corpus drive that found it answering `[]` on **all twelve** volumes — 26 gap runs over 36 raw pages — and hiding a real `credits` verdict on kobayashi-v01's last page* | `credits.rs::unpaired_runs`, the `true => Some(run.a)` arm | the Task 7 mutation review, by probe |
| O39 | [x] | Nothing bounds a test's runtime, so a regression that stops a loop terminating hangs the forge instead of failing it — there is no `.config/nextest.toml`, no `timeout-minutes` in `ci.yml`, and CI does not run nextest at all but `cargo llvm-cov` over `cargo test` — *closed by commits (`dadd6a2`, `bce5bf3`), PR #47: a 60 s per-test kill via `.config/nextest.toml` under `cargo llvm-cov nextest`, and `timeout-minutes: 10` on every job — the per-test bound was triggered on the forge itself (a deliberate hang failed the `test` job at 153 s, its exact revert passed at 117 s); the job-level bound was not, both runs finishing far under ten minutes* | `.config/nextest.toml`; `.forgejo/workflows/ci.yml`'s four `timeout-minutes` keys | the Tasks 2-6 mutation review |
| O40 | [x] | An arrow out of a gap or a borrowed page can land on a credit page the reader asked to skip — the new `step` branch reaches `nearestPaired` instead of `groups`/`stepToGroup`, and `nearestPaired` knows nothing about the skipped set — *closed by giving `nearestPaired` the same skipped set `groups` already receives, the route its own entry named: a candidate landing in it is passed over and the scan carries on, so a skipped page between the reader and a real one costs an offer rather than removing it. Both of `App.tsx`'s call sites — the arrow through `applyIntent`, and the `Gap` screen's own offer buttons — are handed it from one `skippedRef`, so the key and the buttons cannot name different destinations* | `session.ts::applyIntent`'s `step` case, `nearest.ts::nearestPaired` | borrow-lender-and-arrows whole-branch review |
| O41 | [ ] | A page's hash depends on which archive the page sits in, and the duplicate detector compares across archives — *measured: one byte-identical credits card in six chapter files is recorded at four different sizes, and `recurrences` answers 0, 0, 0, 0, 2 and 3 for it* | `fingerprint.rs::describe_pages`, the `common_border` crop | the fourteen-edition credit survey |
| O42 | [ ] | Every progress, notes and truth call in the library funnels through one connection behind one mutex, so WAL buys nothing and two different works no longer read in parallel — *unmeasured; the file store it replaced had independent I/O per work. **Spec A2 added the busiest caller of all**: the guard resolves a principal on every single request, so this mutex is now on the path of every page a reader turns rather than only of the writes — which raises what it costs without changing what it is* | `taiyaku-library/src/lib.rs::Library`, `conn()` | the library store, Task 13 review |
| O43 | [x] | A hand correction through the alignment editor writes the sidecar file and the in-memory `Work`, but never `state.library` — confirmed by reading the handler end to end, twice, and by grepping it and `Work::update` for any reference to `state.library`: none. Unlike notes, progress and truth, whose `store` functions call `taiyaku_library::state::put_notes`/`put_progress`/`put_truth`, there is no `put_alignment`-shaped function for it to have called. So a correction is invisible to the database — and to `library export`, and to any second `serve` process sharing the same database — from the moment it is made until the next import or `serve` restart re-reads the file it did land in — *closed by giving alignments the function the other three already had: `taiyaku_library::state::put_alignments`, reached through a `taiyaku-serve/src/alignments.rs` bridge that mirrors `progress.rs`/`notes.rs`/`truth.rs`. The handler now writes the file, then the rows, then memory, all inside the `Work::update` that `If-Match` is checked in. The one thing that had to be got right is lock order: `list_works` holds the library mutex across a `work.sidecar()` read, so the connection is taken **before** `update` rather than inside it, and `works::row_id_for_document` exists because `work.sidecar()` under `update`'s own write guard would deadlock on a non-reentrant `RwLock`* | `taiyaku-serve/src/routes.rs::patch_alignments`, `taiyaku-serve/src/alignments.rs::store`, `taiyaku_library::state::put_alignments` | the library store whole-branch review |
| O44 | [x] | `taiyaku.library.json` is the one document this project designs for hand-editing, and after a work's first import there is no route that applies a hand edit to it — `migrate` reads a sibling document once per work and records that it did (settled, and deliberate), and `librarydoc::apply` has no CLI command and no HTTP route. So the only way to make a hand edit take is a fresh `--db`. The same gap is why `librarydoc::forget_orphaned_series` was unreachable from any production path at the time: nothing applied a second document over a work. *No longer true once this closed: `patch_work`'s own `series` field, one of the four PATCH routes that close this item, gives `apply` — and so `forget_orphaned_series` — its first reachable production path, since moving or clearing a work's series is exactly the case that can hand it a real, now-empty series to delete* | `taiyaku-library/src/librarydoc.rs::apply`, with no caller outside `migrate::apply_library` | the review of review 3's Minor fixes, M8 |
| O45 | [ ] | `dir_state` is written on every scan for every directory and read by nothing — the migration's own comment says it is what an interval scan compares, and `scan.rs`'s doc says the saving is `(size, mtime)` on `scan_entry`. So the table is the design's §8 cheap walk, present and unused; the comment cannot be corrected because it sits inside a `CREATE TABLE` and therefore lands in `sqlite_master`, and migrations here are append-only | `taiyaku-library/src/scan.rs::record`, `schema/003_scanner.sql`'s `dir_state` | the scanner whole-branch review, Minor 2 |
| O46 | [ ] | `derive`'s `unreadable` counts every sidecar JSON under a root — *measured: 22 `taiyaku*.json` files in `corpus/` outside dot-directories, each sniffing to no known signature and so failing `is_readable`* — so a library following this project's own sidecar convention reports one "unreadable" per work directory as though something were wrong. The counter's doc is literally true ("for want of readable bytes") and the operator-facing reading is not. Whether the right answer is a fifth situation in the doc, an exclusion, or a separate counter is a decision, and changing it moves the corpus figures in `docs/notes.md` | `taiyaku-library/src/derive.rs::admit`, `DeriveReport::unreadable` | the scanner whole-branch review, Minor 3 |
| O47 | [x] | **Closed 2026-09-14, once `library scan --add` had been run before and after.** `add_root` accepts a root nested inside, or containing, an existing one, and the second scan then admits the same directory **twice as two members of one work** — *measured 2026-08-18 on a scratch tree: `--add lib`, then `--add lib/Series`, gives member ordinals 0 and 1 with the identical path and the identical sha256, and the work's page total goes from 3 to 6. Member order is page order, so the reader sees every page twice.* The report says `1 merged`, which is true and does not say that the two entries are one directory. Easy to reach by hand and nothing refuses it *(half stale by the time it closed: `POST /api/library/roots` has refused it through `scan::add_root_checked` since C4 PR 1, #105, on 2026-09-03 — `library scan --add` was the way left)*. **Still present on `main` (`b0ebc25`), by one path.** `add_root` validates nothing by design, and its one caller outside a test was `taiyaku library scan --add`. Reproduced with the release binary on a scratch tree (`docs/notes.md`, "O47: `scan --add` takes the checked add"): `--add lib`, then `--add lib/Series`, exited 0, printed `1 rekeyed, 1 merged`, and left the one edition with two members — ordinals 0 and 1, one path, one `sha256`, 3 pages each, one under each root. The other order, `--add lib/Series` then `--add lib`, did the same. **Fixed by having `--add` call `scan::add_root_checked`**, the path its doc names for a person: on fresh copies of the same tree the second `--add` now exits 1, with "`…/lib/Series` is inside existing root `…/lib` (id 1)" or "`…/lib` contains existing root `…/lib/Series` (id 1)", and the library holds one root and one member. Re-adding a root already held still exits 1, and its message changes from `UNIQUE constraint failed: root.path` to "`…/lib` is inside existing root `…/lib` (id 1)" — the exact-duplicate case `add_root_checked`'s doc names. That function's other checks come with it: a path this process cannot read is refused before a root is written, and the new root's `scan_interval_secs` is recorded from the watch probe (86400 there, NULL on `main`). **Pinned by two CLI tests**, a root inside a known one and a root containing one, each asserting the refusal's message and that one root and one member remain; watched red against `add_root` (the command returned `Ok`), against a generic error message (the message assertion), and against an insert made before the refusal (the count, `(2, 1)`). `add_root_checked`'s doc also named `import` as a caller of `add_root`, which it has never been — corrected. **Not changed**: `add_root` itself; and a root added by `--add` is probed twice when the same run walks it, once when it is added and again by `scan`'s loop, which walks every root unless `--root` names a different one | `taiyaku-library/src/scan.rs::add_root`, `taiyaku-cli/src/library.rs::scan` | the scanner whole-branch review, Minor 4 |
| O48 | [ ] | §8.1's empty-root refusal fires only at `seen.len() <= 1`, so a share that mounts one directory deep — or ninety-nine per cent unmounted — is applied silently and `forget_unseen` drops the rest. The reasoning that justifies the refusal at 100% applies just as well at 99%; what it needs is a threshold nobody has chosen and no measurement behind one | `taiyaku-library/src/scan.rs::scan_root`, the `held > 1` refusal | the scanner whole-branch review, Minor 6 |
| O49 | [ ] | `marked_number` strips exactly one separator between a marker and its digits, so `Ch.  7` (two spaces) is not a chapter. The loose fix (`trim_end_matches`) was settled against in Task 1 for a reason that no longer holds — the word-boundary guard added afterwards is what stops `Scan by ABC 2019` reading as chapter 2019 — but loosening it now would move the kanji-counter survey figures this branch measured, so it is a decision with a re-measurement attached | `taiyaku-read/src/naming.rs::marked_number` | the scanner whole-branch review, Minor 9 |
| O50 | [ ] | `library export --all --in-place` exits non-zero on any library holding scanned works: `export::work` refuses a work with no `sidecar_path` on record, `export_targets` counts it as a failure, and a work derivation minted has no such path by construction. So the mixed library — some works imported from a `taiyaku.json`, some found by a scan — is exactly the case the command cannot complete. Whether a scanned work should be skipped, or should get a sidecar written beside its hub member, is a decision | `taiyaku-cli/src/library.rs::export_targets`, `taiyaku-library/src/export.rs::work` | the scanner whole-branch review, Minor 11 |
| O51 | [ ] | Nothing acts on the watch probe's answer. `watch.rs` decides whether a root can be watched and reports the interval that answer implies, but `scan_root` never writes `root.last_scan_at` or `last_scan_status` — both columns exist in migration 1 — and no caller schedules a rescan, so the tiers §8 designs are computed, printed, and honoured by nothing. The scheduler belongs with project 2's job runner; what is missing here is the two columns the scanner could fill today. *Narrowed on 2026-09-14, and left open. The half this row calls missing "here" has landed: since #105 (`46bf9e1`), `scan_root` writes `last_scan_at` and `last_scan_status` on every path out — `ok`, `refused` or `failed` — and both `taiyaku library scan` and `POST /api/library/roots/{id}/scan` go through it, with `scan::mark_derive_failed` correcting an `ok` when the server's derive after it fails. `scan::tests::a_scan_records_when_it_ran_and_how_it_went`, `a_refused_empty_scan_records_the_refusal_rather_than_nothing` and `a_failed_scan_records_failed_rather_than_ok_or_nothing` hold it. The first sentence still stands, and a screen now draws it: `add_root_checked` writes the probe's interval to `root.scan_interval_secs`, C4's roots screen prints it as "scanning every …" (`config.ts::regimeLine`), and nothing schedules a scan on it. That is the half this row already placed with project 2's job runner. Measured in `notes.md`, "O77 and O51: two rows the code had moved past". The row stays open and its prose above stays as written, per the ritual.* | `scan.rs::scan_root`, `root.last_scan_at` | spec B's whole-branch review |
| O52 | [ ] | Taking filesystem I/O out of the write transaction fixed **cross-process** contention only. `Library::conn()` hands out a `MutexGuard`, so whichever caller holds it holds it across the whole walk and the whole resolve pass — *and that cannot be fixed by a function taking `&Connection`*, which is what `scan_root` is. Reachable the moment `serve` and a scan share one process. **Spec A2's guard is the first caller written against this item rather than into it**: `auth::from_basic` reads the stored hash, drops the guard, verifies with argon2 holding nothing, and re-takes the connection for the roots — three short holds rather than one spanning an 11.9 ms hash, which would have serialised the whole server behind every password login. That is a discipline at one call site, not a fix: `conn()` still hands out a `MutexGuard` and the next caller can still hold it across anything | `lib.rs::conn`, `scan.rs::scan_root` | spec B's whole-branch review |
| O53 | [x] | `librarydoc::apply`'s `Step::Apply` is unreachable for any work a scan has touched — `librarydoc::of` reads non-empty the moment a work has a derived volume or series, so the branch that applies a hand-edited `taiyaku.library.json` is never taken. O44 records that nothing *calls* `apply`; this is narrower and worse, because the one caller that exists cannot reach the step that does the work. The I7 fix works through the marker `export::work` sets, a real but different path. *(O80 has since removed the consequence this entry rests on: since migration 011 `librarydoc::of` projects only hand-set values for the four columns this entry rests on, so it no longer reads non-empty for a work whose volume and series are merely derived. *(Not for chapters: `of` projects `Filename`- and `Comicinfo`-sourced ones too, which is the overstatement marked in O80's entry on 2026-08-31. It does not touch this entry's reasoning.)* This prose is kept as written, in the present tense of when it closed — see O80's entry.)* | `librarydoc.rs::of`, `migrate::work`'s `Step::Apply` | spec B's Task 12, while fixing I7 |
| O54 | [x] | **Closed in two halves, and neither closed the way either spec expected.** `Principal.roots` closed first, under spec C1: every `/api/library/*` route hands it to `taiyaku_library::browse`, which applies it in SQL, and the `#[expect(dead_code)]` spec A2 had put on that one field came off in the same commit that first read it — which is what the attribute was for. **Closed the rest under spec C2, 2026-08-23:** `can_download` now gates the two download routes (`GET /api/works/{slug}/{edition}/download` and `.../members/{ordinal}/download`) and `can_stream` gates `/pages` and the OPDS PSE stream — `routes::refuse_if_cannot_download` and `refuse_if_cannot_stream` read them. A thumbnail, a cover and every browse route deliberately do not: a download-only account must still be able to see its shelf. Both columns default to 1 (`schema/004_auth.sql:18`), so nothing changed for any existing account the day this shipped, and nothing can set either to 0 outside a fixture until spec C4's user administration. **The column is `member.root_id`, not `work.root_id`** (migration 6): A2 declined the latter as O45's exact shape, and C1 found it is not merely undesirable but *unrepresentable* — `derive.rs`'s `Group` is "every entry across every root that resolved to the same (series, volume)", so one volume held in `Manga` and in `Manga (RAW)` is deliberately **one work with two editions** and a scalar root has no value to hold. A member resolves from exactly one `scan_entry` under exactly one root, and is also the only granularity that can express "grant `Manga`, not `Manga (RAW)`" as one edition hidden rather than the whole work. **An empty grant means unrestricted**, which is counter-intuitive and forced: `user_root` had no writer until now, so every account in every existing library holds an empty grant, and any other reading would have emptied every library the day migration 6 applied. `taiyaku library user grant-root` is that writer, shipped here rather than left to C4 because a permission filter nothing can exercise outside its fixtures is a filter nobody has tested. **The whole-branch review found the first version of this was a browse filter rather than a boundary**, and both halves are now closed: `editions_of` counted every member of a surviving edition, so a granted reader saw counts covering roots they could not see; and the page routes asked nothing at all, so a reader who knew a slug could read every page of every root. Reachable by default rather than in a corner — two roots added with `library scan --add` carry no `edition_key`, both fall back to `raw`, and derivation merges their members into one edition, which is precisely the case the original tests' fixture renamed its way around. `browse::edition_visible` is what `/pages` and `/thumb` now ask, and the sidecar route asks `browse::work`; progress, notes and truth deliberately do not, which is O60. Measured on the corpus, 2026-08-20: the filter costs about 0.2 ms over 23 series and 68 works (0.489 ms unrestricted, 0.701 ms with one root granted). *Its "nothing can set either to 0 outside a fixture until spec C4's user administration" stopped being true on 2026-09-04: C4 PR 2 shipped that administration — `account::update` in the store, `PATCH /api/library/users/{id}` over HTTP — so both capability columns and `user_root` now have a writer a person can reach. The item stays closed and its prose above stays as written, per the ritual; this sentence marks what overtook one clause of it.* *Since migration 15 (F1), a grant names a library: `user_library` replaced `user_root`, `grant-library` replaced `grant-root`, and `Principal.roots` is every folder of every granted library, for an account that is not an administrator.* *O220 has since replaced it with `Principal.access`, an `account::Access`.* | `auth.rs::Principal`, `browse.rs`, `member.root_id` | spec A2, on shipping; half closed by C1, the rest by C2 |
| O55 | [x] | The account screen tells a reader to give an API key to an OPDS client as the password, and **no OPDS client has been shown to accept one there.** §2's whole `tyk_` design rests on those clients treating the Basic password field as opaque, which the specification implies and no test has confirmed against Panels or Chunky. The server half is verified — `/opds/v1.2/catalog` answers 401 with `WWW-Authenticate`, then 200 with a `tyk_` key, and a page stream answers 200 `image/jpeg` — so what is unverified is the client, and one attempt against a real iPad settles it. Opened rather than left as a §14 unknown because a screen promising an unverified thing is a defect: if it fails, the fallback is a password-shaped per-device credential and §2's table is amended — *closed by that one attempt, 2026-08-25: **Panels accepts a `tyk_` key in the Basic password field.** `Panels/950 CFNetwork/3860.700.1 Darwin/25.6.0` in `serve --log-requests`, 401 then 200 on `/opds/v1.2/catalog`, then the works feed, five covers and page fetches. The design stands, the account screen's promise is now a verified one, and the fallback this entry names is not needed. It was driven as spec C2's second half's own prerequisite, because a feed no client will authenticate to cannot prove a download an OPDS client can reach.* **One thing the attempt needed is recorded nowhere else: iOS App Transport Security refuses plain HTTP**, so an OPDS source at `http://…` reports that the server cannot be found whatever the credentials, and the session reached the server behind TLS instead. A deployment fact rather than a defect, but the account screen's instructions do not mention it | `web/src/Account.tsx`'s API keys section, `credential::API_KEY_PREFIX` | spec A2 §14, unresolved on shipping; settled by spec C2's second half |
| O56 | [ ] | A member directory whose *contents* change without an entry being added or removed keeps its old digest, because `unchanged` compares `(size, mtime)` and a directory's mtime does not move when the bytes inside an existing child change — nor does its size, which is a block count whether it holds three pages or five. So replacing a page in place, or rebuilding a volume inside one second, is invisible to a rescan. **Found by CI rather than by a review**: two of spec B's own tests (`a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive`, `a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress`) depend on the rescan seeing exactly this and passed fifteen times running locally while failing on the runner. Both now stamp the directory forward rather than racing the clock, which fixes the tests and leaves the behaviour they were unintentionally relying on. Whether the walk should hash a directory's children rather than trust its stamp is spec B's §8 trade — the cheap walk exists to avoid opening containers — so this is a decision with a measurement attached rather than a defect to patch | `taiyaku-library/src/scan.rs::unchanged`, `derive.rs::member_digest` | CI on the `library-auth` branch, 2026-08-19 |
| O57 | [x] | **Closed 2026-10-12, by the user's ruling of 2026-10-11 that the cap be four and be set without a measurement on the NAS, and by what measuring the fix on this workstation then showed.** Each server has four verify permits (`session::VerifyLimit`, `MAX_CONCURRENT_VERIFIES`), and a password is verified only through `session::verify_holding`, by a caller holding one: twice in `session::login`, twice in `auth::from_basic`, once in `session::change_password`. On each of those paths the lockout is read before the request waits for a permit, so a caller already locked out is refused without waiting (`a_locked_out_login_is_refused_while_every_permit_is_held` and its two neighbours), and read again once it holds one, with the permit kept until the failure is recorded (`a_login_that_waited_for_a_permit_reads_the_lockout_again`, `a_login_with_a_wrong_password_records_its_failure_holding_its_permit`, and their neighbours on the other two paths). So the requests between that second read and the record are the ones holding a permit, and a burst against a counter one failure under its ceiling is verified at most four times (`a_burst_of_invented_names_spends_one_allowance_at_the_login_route` and the four beside it). The order is the permit and then the library's connection: nothing that holds the connection waits for a permit, which is why the routes that set a password take none (`only_a_password_verify_waits_for_a_permit`, `a_request_waiting_for_a_permit_does_not_hold_the_librarys_connection`). **The cap by itself did not bound what the server holds, which was found by measuring it.** With glibc's defaults, forty wrong passwords at once left a release build of the capped server holding about 600 MiB thirty seconds later, where `main` peaked at 627 MiB and at 709 MiB in the two runs kept, and in the one that read it again was at 33 MiB a second later: under the cap the verifies ran four at a time on forty threads, and glibc kept freed buffers in those threads' arenas. So a verify now runs in a working memory its permit owns (`credential::WorkingMemory`; `credential::verify_password_in` is `argon2`'s own verifier written against memory the caller supplies, and `the_two_verifiers_agree` holds it to that). A server makes at most four and uses them again, each is wiped after every run, a hash runs in a memory the process keeps, and `argon2`'s own allocating verifier is called only from `credential`'s tests, the public function over it being gone. With that, a release build answered forty wrong passwords at once at a peak of 92.1 MiB and two hundred at 96.5 MiB, verifying 52 of the two hundred and answering the rest 429, where `main` verified 155 of them at 1,212 MiB. **What it costs:** the server keeps the memories it has made, about 80 MiB over its starting size once four verifies have run at once, where `main` with the same defaults, in the one run that read it again, was at 33 MiB a second after its burst. **What is left** is O214 (a request waiting for a permit parks a blocking-pool thread, so a wide enough flood makes every request wait), O215 (what no test holds) and O217 (a stored hash that names a larger cost). The *Later* entry "Measure argon2's cost on the NAS" stays for the question it still answers, whether the parameters hold. Readings: `docs/notes.md`, "O57, O60 and O138". *The sentence below, "Left open rather than guessed at, because §14's argon2-cost measurement on the NAS is the evidence it needs", was half right: the number did not need that measurement, since what it bounds is memory, and something else did need measuring, which was what the server holds after a burst.* *As filed:* **The rate limiter bounds the sustained flood and not the burst.** Both credential paths check their counters and *then* do the argon2 work, so requests arriving together all read the same under-ceiling count and all proceed: `spawn_blocking`'s pool is 512 by default and argon2id at m=19456 KiB allocates 19 MiB per verify, so one concurrent burst is ~9.5 GiB of transient buffer on a host chosen for being small. Bounded per window by `ADDRESS_LOCKOUT_AFTER` once the first failures land, which is what makes the ten-thousand-request flood stop; a single burst is what remains. Increment-then-check does not fix it — an OPDS client presents Basic on every page request, so metering successes would lock a reader out of their own library — which leaves a semaphore capping concurrent verifies as the fix, and the cap is a number nobody has measured on the host that will run this. **Left open rather than guessed at**, because §14's argon2-cost measurement on the NAS is the evidence it needs and that measurement is still outstanding. *Part B of the day-to-day deployment spec puts taiyaku public behind Cloudflare, by the user's decision of 2026-10-04. The login limiter keys per address, and an IPv6 client arriving through Cloudflare can hold a whole /64.* | `session::VerifyLimit`, `session::verify_holding`, `credential::verify_password_in`, `auth.rs::from_basic`, `session.rs::login`, `session.rs::change_password` | the fourth whole-branch review, while fixing the address key |
| O58 | [ ] | **The two routes that write shared documents take no *credential-kind* check, so an API key can rewrite them.** `put_truth` is still declared `(State, AxPath, HeaderMap, Bytes)` — no `Principal`, no `Extensions` at all. `patch_alignments` is no longer: the split-across-roots fix round gave it an `Extensions` parameter and a `Principal` read, to filter its response and refuse a wholly-hidden work by root grant — but that read is `Principal`, not `By`, and `refuse_a_device_credential` gates on `By::may_administer()`, a different extension entirely. `patch_alignments` reads no `By` and calls no such gate, so it still cannot tell a browser session from a device credential, exactly as `put_truth` cannot. The guard requires *a* credential and `refuse_a_device_credential` narrows only the six account routes, which is where the third review put it. §2 argues a key is the thing an OPDS client presents for every page of a volume and hashes it fast on that basis; a key typed into a third-party reader can therefore silently rewrite the alignment and truth documents **every** reader sees, and those are shared rather than per-account. Verified by reading both signatures, 2026-08-24 — `patch_alignments`'s signature changed since the 2026-08-19 reading; the gap it was cited for did not. **Left open as a decision rather than fixed as a defect**: whether editing content counts as administering is a design call the spec does not make, an offline native reader is a legitimate future caller, and the fix wants C4's permission model rather than a sixth `refuse_a_device_credential` beside the others | `routes.rs::patch_alignments`, `routes.rs::put_truth`, `session.rs::refuse_a_device_credential` | spec A2's fourth whole-branch review |
| O59 | [ ] | **Two `derive` tests fail under mutation for environmental reasons, which credits mutants nothing deliberately catches.** `a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress` and `a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive` fail when cargo-mutants runs them in its copied tree and pass under the same mutation applied in place — verified by hand both ways on 2026-08-20. So `mtime_secs`'s three constant mutants were recorded **caught** by the local sweep and **missed** by the forge, and nothing anywhere was testing the property: `scan::tests::a_files_modification_time_reaches_its_scan_entry` now does, deterministically, and kills all three in both environments. This is the same pair O56 is written about and the same pair that raced a one-second clock in CI two days earlier, so the sensitivity was known; **what is new is that it inflates a measurement rather than just failing.** The residual is that any other mutant these two tests happen to fail under is credited the same way, and the local sweep's caught counts cannot distinguish those. Whether the fix is making the two tests independent of their tree or narrowing what they assert is a spec B decision | `taiyaku-library/src/derive/tests.rs`, `scan.rs::mtime_secs` | the first sharded run of `mutants.yml`, on the forge |
| O60 | [x] | **Closed 2026-10-12, by the user's ruling of 2026-10-11: a grant withholds the whole work.** The six handlers behind `/api/works/{work}/progress`, `/notes` and `/truth` ask `visible_editions_or_refuse`, the helper the sidecar route asks, and answer a work outside the caller's grant as they answer a slug no work holds, status and body (`progress_notes_and_truth_answer_a_hidden_work_as_a_slug_no_work_holds`; `every_route_naming_a_work_answers_a_hidden_one_as_a_slug_no_work_holds` walks the guarded paths that carry `{work}`, thirteen when it was written, with five methods each, with no body and with `{}`). A write is refused before any check the handler makes once it has found the work, so an edition's name, a slot count and the `ETag` of a notes or truth document are not told of a work the caller may not see (`a_write_to_a_hidden_work_is_refused_before_its_own_checks_are_made`); a body that does not parse is refused as that whatever work it names (`a_document_that_does_not_parse_is_refused_before_the_grant_is_asked`). Inside a work the caller sees part of, a position or a new note on an edition outside the grant is refused in the words an edition the work does not have is refused in, and a position read leaves such an edition out (`a_position_on_an_edition_outside_the_grant_is_refused_as_one_the_work_does_not_have`, `a_new_note_on_an_edition_outside_the_grant_is_refused_and_a_held_one_is_kept`, `a_position_read_leaves_out_an_edition_outside_the_grant`); for an account with grant limits that read also leaves out a position on an edition key the work no longer has, which an account with none is still sent (`a_position_on_an_edition_the_work_no_longer_has_is_sent_only_to_an_account_with_no_limits`). The truth document and a reader's own notes on such a work are sent whole, which O191 records. Nothing stored is deleted: a position and a note outlive a narrowed grant and are served again when it is widened (`a_position_and_a_note_outlive_the_grant_and_come_back_with_it`). `routes::grant_postures` declares progress `RefusesAndFilters` and notes and truth `Refuses`, `auth::tests::every_route_declares_its_grant_posture` drives all three, and `GrantPosture::KnownGap`, whose one entry was truth, is gone. Driven against a scratch store of four real volumes in two libraries, as an account granted one of them: on `main`'s code ten of eleven requests answered a work of the other library differently from a slug no work holds, among them a slot count, the `ETag`s of its notes and its truth document, and a position write that reached the store; on this branch none of the eleven did, and an administrator's answers were what they had been (`docs/notes.md`, "O57, O60 and O138"). O58 is not changed by this: which kind of credential may write a truth document is a different question from which works an account may see. *The sentence below, "Answering it is C4's permission model, not a sixth call to a helper", turned out the other way: the answer was the helper the sidecar route already asked.* *As filed:* **Progress, notes and truth take no account of the grant.** Spec C1's fix round guarded the material — `/pages`, `/thumb` and the sidecar route all ask `browse::edition_visible` or `browse::work` — and deliberately stopped there. A reader restricted to some libraries can still `PUT` a reading position, a note or a truth document against a work in a library they cannot see, because those routes are keyed on a work and resolve the reader from `Principal.user_id` alone. Nothing leaks: progress and notes are per-account, and a reader can only write their own. What is unresolved is what a grant *means* — whether it withholds material or withholds the work entirely — and truth is the awkward one, since it is **shared** rather than per-account and so sits beside O58 rather than beside progress. Answering it is C4's permission model, not a sixth call to a helper. *Since F1 a grant names a library and resolves to its folders; the gap is unchanged.* *Run on 2026-10-06 as an account granted one library, against a work of another: `GET` of its progress answered 204, of its notes 200 and of its truth 200, where a slug no work holds answered 404 on all three and the hidden work's page answered 404; `PUT` of a position on it answered 204. So these routes tell such an account which slugs another library holds, and a slug is spelled from a title, which "Nothing leaks" above does not say.* | `routes::visible_editions_or_refuse`, `routes::sidecar_as_seen`, `routes::grant_postures`, `routes.rs::put_progress`, `put_notes`, `put_truth` | spec C1's whole-branch review |
| O61 | [ ] | **With sidecar paths on argv, the browse API lists works the reader routes cannot serve.** `ServeState.works` is built from the paths named on the command line, while every `/api/library/*` handler queries the whole database — so `taiyaku serve foo/taiyaku.json` answers a grid covering every work the library has ever imported, and following any other tile to `/api/works/{slug}` gets a 404 from `ServeState::find` (`work_or_missing` until O183). Harmless for `serve --db`, which is the invocation spec C1 §5.3 built the browse API for and where the two sets coincide. The fix is a decision rather than a patch: either the browse queries are scoped to `state.works` in argv mode, or the browse routes are registered only when serving the whole library, and the second changes what `build_router` is a function of | `library_api.rs`, `lib.rs::ServeState::works`, `main.rs::serve_works` | spec C1's whole-branch review |
| O62 | [x] | **A work whose members are gone from disk is offered and then fails at its first page.** Spec C1 §8 asks for it to be shown as unreadable. The fact is derivable — the scanner deletes `scan_entry` rows for files it no longer meets while `member` rows survive — but not cheaply: `member.path` is absolute and `scan_entry.rel_path` is relative to its root, so the join is over a computed string rather than an index. Spec C1 §5.5 defers it to C3 with the rest of provenance. **Corrected 2026-08-26 — the sentence above is wrong and is kept as written, because that reasoning is why this item sat unbuilt rather than being one of C3's cheaper tasks.** *"Not cheaply"* does not hold: the computed string is on the **probing** side of the join, and the side being probed is `scan_entry`, whose `UNIQUE (root_id, rel_path)` is exactly the index the probe wants — `member.root_id` has existed since migration 6, which C1 itself shipped. *Measured 2026-08-26 on a database built by applying `schema/001`–`008` to an empty file, with a negative control (`notes.md`): `SEARCH se USING COVERING INDEX sqlite_autoindex_scan_entry_1 (root_id=? AND rel_path=?)`* — one pass over `member`, unavoidable because the question is asked about every member, and one covering-index probe per row. Shipped code already performs that exact join for another reason: `proposals::edition_formats` reads an edition's formats back out of `scan_entry` on `(root_id, rel_path)`. **So this is a defect with a known fix, not a decision**, no new column is needed, and C1 §5.5's framing of it as "what the store should record when a file vanishes" is answered *nothing further*. It stays open because the fix is not built | `scan.rs::forget_unseen`, `member.path`, `scan_entry.rel_path`, `scan_entry`'s `UNIQUE (root_id, rel_path)`, `proposals::edition_formats` | spec C1 §5.5, deferred to C3; cost claim corrected by C3's build design §8, 2026-08-26 |
| O63 | [ ] | **The table layout cannot reach alignment detail at all.** The verdict and paired count come from `GET /api/library/works/{slug}` and nowhere else, and only the grid's `i` button ever asks for it. Spec C1 §3's presets never pair the table with the panel — preset B is table with both attachments off — and §4's generated settings panel made the combination reachable anyway, by exposing the three switches independently rather than as a fixed set of presets. **Decided rather than fixed, 2026-08-21**: the inert controls this made reachable are hidden (`663a8bd`), and the table's own route to alignment detail is left unbuilt. What would reopen it: somebody who prefers the table saying they cannot get a verdict from it — switching to the grid is one click meanwhile | `Series.tsx::Table`, `browseModule.ts::BrowsePrefs::layout` | the library-browse-series whole-branch review |
| O64 | [x] | **`library import` clears `member.root_id`, so an imported work vanishes for every root-restricted account until the next scan.** Measured over HTTP during the browser pass on 2026-08-21: after one `library import`, an account holding a root grant saw `works=2` where it should have seen 3, and lost volume 8 entirely — the route answers 404 for that work. **This is migration 7's bug returning through a different door**: migration 7 backfills `member.root_id` for members a scan produced and migration 6 left NULL, and its own comment says the consequence is that "the first grant blanks the library" — `import.rs` writes fresh NULLs after that migration has run, and never mentions the column. `grant-root` warns about the condition, but only when granting, not when importing. `browse.rs`'s root filter is not the bug: `IN (…)` excludes NULL by SQL's own rules, which is spec C1 §5.1's deliberate design, so the fix belongs at the writer rather than the reader. **Closed by `a8863c5`: the "open decision" above was itself wrong.** The measured failure was never a purely-imported work with nothing to belong to — spec C1 §5.1 already settles that one correctly, NULL and unfiled. It was a work a *scan* had already stamped, whose association `replace_editions`'s wholesale delete-and-reinsert then destroyed with nothing to restore it, because `write_roots` only runs on the derive path and a plain `library import` never takes it. The fix extends the mechanism `replace_editions` already had for `chapter_declared`/`chapter_source`: `held_member_meta` now also reads `member.root_id` before the delete — widening its guard so a member holding only a `root_id` and no chapter data is still held — and restores it onto the fresh row after each reinsert, keyed on `sha256` like the rest of that map. A member genuinely new to the document still has no held row and still gets NULL, so a purely-imported work stays exactly as unfiled as before. Restoring it is redundant rather than interfering under `Scope::Scan`: `write_roots` runs unconditionally after every derive and overwrites whatever this restores, for every edition the derive wrote. **No migration**: the `path` migration 7's `LIKE r.path \|\| '/%'` backfill needs is also overwritten by the same reimport — with the incoming document's own value, relative to the sidecar rather than absolute from a root — so a migration finds nothing to match against an already-damaged row; the only recovery for a library damaged before this fix is the next scan, which rewrites `path` absolute again and lets `write_roots` restamp it | `import.rs::held_member_meta`, `import.rs::replace_editions`, `member.root_id` | the library-browse-series browser pass, 2026-08-21 |
| O65 | [x] | **Closed 2026-09-14, once the reader had been driven before and after. The reader chrome's `Library` button covered the layout buttons.** With the chrome open, the leave button (`z-index: 3`, at 16,16) sits over the centres of the `single` and `two` layout buttons: `document.elementFromPoint` returns `BUTTON:Library` for both. Shipped in #82 (`b9581ac`), so it is on `main` and not introduced here. A Minor — a layout collision with a known fix shape — rather than a decision. **No fix shape was recorded**, in this row, `docs/notes.md` or the browser pass's own session record, so three were put to the user and one chosen: the button now docks at the left end of the band just above the strip, on `Chrome.tsx`'s new `aboveStrip`, which is the `bottom` the hover preview already stood on. The other two were docking it under the toolbar, whose height depends on the browser's own button styles, and moving it into the toolbar row, which would take it out of `Chrome` along with its fade and its hide-while-correcting rule. It still renders only while not correcting, for a different reason than before: `CorrectionBanner` no longer shares its edge, but the hover preview's box now contains its corner. **Measured before and after** (`docs/notes.md`, "O65: the Library button moves above the strip"), against the browser pass's own scratch library, with two production bundles served side by side and driven by Playwright in a browser reporting `Chrome/153.0.0.0`, at 1440×900 and 768×1024. Before, at both sizes, `elementFromPoint` at `single`'s and `two`'s centres answered `Library`, whose box was 16,16–88.5,48.4. After, each layout button answered itself, and `Library` answered itself at 16,643.6–88.5,676 at 1440×900, overlapping no other button. **Pinned twice**: `Chrome.test.tsx` pins the declared geometry in jsdom, which CI runs, and `App.browser.test.tsx` repeats the measurement on the mounted reader in the browser project, which CI does not. Both went red with the button put back at `top: inset`, and both went red with it docked over the strip — the browser test only once its fixture was read `ltr`; read `rtl`, it passed that mutation. **One cost, measured before merging**: a note list placed below the page reaches up into the new band once it is long, and at ten notes the button covered the left end of the top one or two rows the strip leaves visible; every row that took a click at its centre on `main` still does. Beside it the same measurement shows something larger that predates this item: with the chrome open, the strip and the readout row cover the rest of that list, so at four notes no row takes a click at its centre, on `main` or here — filed as O112 *(Since style pass ③ the button is in the reader's top bar, which comes and goes with the strip — spec 2026-09-17 §8.3 — so `leaveButtonStyle` and `aboveStrip` no longer exist, and the band this docked it in holds nothing while reading — the hover preview still stands there while correcting.)* | `TopBar.tsx`'s `reader-back`, the layout control group | the library-browse-series browser pass, 2026-08-21 |
| O66 | [ ] | **A generated download's `Range` resume reads and discards the prefix rather than seeking to it.** `download.rs::generated` walks every member from page 0 regardless of `from`, decoding nothing but paying for every read up to the window, and hands each chunk to `cbz::clipped` to keep only `[from, upto)` — correct on every byte, since a passthrough single-member edition already seeks for real, but a resumed generated download re-reads a prefix it will throw away. `Plan::payload_span` already maps every entry's `[start, end)` in the output, which is the map a true seek would walk to open each member at the right page and `Seek::seek` past the rest instead of reading it. **Not a defect on evidence; a known cost with a known fix, deliberately not built here** (spec C2 §11): no client this project has measured has been shown to request a `Range` on an acquisition fetch at all, so building the seek first would be optimising a path nothing yet proves is taken. The trigger is named rather than left implicit — a fresh `taiyaku serve --log-requests` session against Panels, bound to the LAN, showing `Range` on an acquisition fetch. The existing `panels*.jsonl` logs predate request-header capture and report `(not recorded)` for headers, so their silence is an instrument's blind spot rather than evidence the trigger has not fired | `download.rs::generated`, `cbz::clipped`, `cbz::Plan::payload_span` | spec C2, on shipping |
| O67 | [x] | **A scanned work's title is the filename of whichever entry minted its row, so a work spanning several editions is named after one of them.** Measured against Panels on an iPad and in a browser, 2026-08-25: volume 1 of Go-Toubun no Hanayome holds five editions — `en-color`, `en-fan`, `en-mono`, `raw-color`, `raw-mono`, which is `derive.rs`'s `Group` working exactly as designed, since a `Group` is every entry across every root that resolved to the same `(series, volume)` — while `work.title` is `Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz`, the stem of the colour file that happened to create the row. `opds::work_feed` renders `{title} — {label}` faithfully, so a reader is shown "…Full Color Edition… — en-mono" for a monochrome edition, and the series page's table repeats it once per row: **the feed is right and the data is wrong.** Two faults stacked — edition-specific text asserted over four editions it does not describe, and a raw filename kept whole, `.cbz` and `(2021)` included. **This is larger than the cost already recorded** in the `taiyaku-library` entry above, which says only that a derived title is *stuck* ("a work this scanner minted keeps the title it first derived, so a later rule edit that would rename it does not reach the row"); what a real client shows is a title wrong about its own contents. `series_id` and `volume` are both on the row and would give "Go-Toubun no Hanayome, volume 1" — the exact string that same entry records a person having written before a scan overwrote it with "Series - v01" — but **`work.slug` derives from `work.title`, and the slug is the address progress, notes and truth all hang on**, so changing the derivation moves every scanned work's URL and strands the positions on it. A decision with a migration attached rather than a patch, and its home is C3's metadata editing, which O44 and O53 record as having no route and an unreachable apply step | `taiyaku-library/src/derive.rs`'s title derivation, `work.title`, `work.slug` | spec C2's second-half device session, 2026-08-25 |
| O68 | [ ] | **The request log records `bytes: 0` for every streamed response, so it cannot tell a completed download from an abandoned one.** `build_router`'s request-log middleware takes the count from `response.body().size_hint().exact()`, which is `None` for a body built with `Body::from_stream` — which is what both download routes use, deliberately, because the `mpsc` channel behind it is what keeps a 1.49 GB volume out of memory. Measured 2026-08-25, from the same Panels session that closed O55: the catalog logged `bytes: 815` and five downloads logged `bytes: 0` — one of those five separately measured at 334,752,036 real bytes over the wire, and the other four not measured at all, which is the point rather than an omission: the log is the thing that was supposed to say. The field's own middleware comment says why it exists — "bytes-per-page is one of the numbers this log exists to produce" — and it produces it for every buffered response and for none of the streamed ones. **This is a blind spot in the instrument this project settles open items with, which is why it is tabled rather than left in `notes.md`**: O66's trigger is a `--log-requests` session against Panels, and the qualification recorded against the negative result already taken is partly this — the log showed five downloads *started*, and that they finished is the client's report rather than the server's record. The same shape as the gap O66's own entry names, where the older `panels*.jsonl` logs report `(not recorded)` for headers. **A decision rather than a patch**, and that is the reason it is open: the line is written when the *response* is produced, and counting a streamed body means writing it when the body *completes* instead — which changes when every log line is emitted, and what `micros` measures, for every route rather than for downloads alone. Wrapping the body to count frames as they pass is the mechanism; whether one log line per request should describe the response or the transfer is the question. What would settle it: needing to distinguish a completed transfer from an abandoned one in a real measurement, which the direct-LAN repeat O66 now asks for is the first to require | `build_router`'s request-log middleware, `reqlog::Log::finish`, `reqlog::Entry::bytes` | spec C2's second-half device session, 2026-08-25 |
| O69 | [ ] | **`member.format` is a column nothing in the repository writes.** The only `INSERT INTO member` (in `import::replace_editions`) lists `(edition_id, ordinal, path, sha256, pages)` and no format; every `UPDATE` that reaches these rows sets only `chapter_declared`, `chapter_source` or `root_id`. Measured on a two-series fixture: `SELECT COUNT(*), COUNT(format) FROM member` answers `2, 0`, while `scan_entry.format` carries a real token for every entry behind those members. **Until spec C3 nothing read it either**, which is why no test ever noticed — the proposals panel is the first code in the project to want a format per edition, and reading the column gave `None` for every edition in every library: a permanently blank field a client cannot tell from an honest "not available yet". The panel now takes its formats from `scan_entry`, joined on `(root_id, rel_path)`, so this is a dead column rather than a live defect — and the fix is a decision rather than a patch, which is why it opens instead of closing. Either drop it by migration, or populate it from the `scan_entry.format` `import::scanned` already holds. Dropping is the smaller change and loses the ability to record a member walked as something its entry does not say; populating means a writer for a value derivation currently recomputes. Not introduced here, and it is the first branch to trip over it. **A second branch tripped over it on 2026-09-07** — C3 §6's ComicInfo write-back planned to filter members on this column, which would have made the feature fire on nothing in every library; `stamp::plan` sniffs the file instead, and deliberately does not take `edition_formats`' `scan_entry` join either, because that reports what a member *was walked as* where a rewrite needs what the bytes are now. Two callers now route around this column; the decision it is waiting on is unchanged | `schema/001_initial.sql`'s `member.format`, `import.rs`'s only `INSERT INTO member`, `proposals::edition_formats` | spec C3's Task 4 review, 2026-08-26 |
| O70 | [x] | **Closed 2026-09-14, once a store fault had been raised inside each of the three writes. `accept_with`, `unaccept` and `dismiss` answer `Result<Offer<_>, String>`: a refusal is `Ok(Offer::Refused(why))` and a store failure is `Err`, so a store fault during a write answers 500 and a refusal still answers 409 with its sentence.** **`evidence`, `resolves` and the three write paths report "no such row", "the library refuses this" and "the read failed" as one `Err(String)`.** *(Not `evidence`: in `791df0a`, the commit that filed this row and added `Offer`, a preview refusal was already `rule: None` through `Offer`, so its `Err` carried only a missing row or a failed read. `resolves` makes its refusals fields of `Resolution`, and its `Err` folds only a rule it cannot evaluate with a failed read, the half `evaluable` separates.)* `curation.rs` cannot tell them apart without string-matching an error message, which this codebase refuses to do, so the status a client gets is decided by which route it asked rather than by what went wrong. **Partly mitigated on this branch and deliberately not more**: `proposals::lookup` answers `Ok(None)` against `Err` for a missing row, which gives a real 404 on all five id-addressed routes, and `proposals::evaluable` takes no connection, so a rule that cannot be evaluated (400) is separate from a read that failed (500). What is left is the write half — **a store fault during an accept still answers 409**, because `accept_with` reaches both its refusals and its store failures as `Result<_, String>`, directly and through the helpers it calls, and the handler maps that one type onto one status. All three functions are public API with `taiyaku-cli` callers *(two of them: `taiyaku-cli` calls `dismiss` and `unaccept`, and reaches `accept_with` only through `accept`, in `791df0a` as on `main`. `rules::delete` calls `unaccept` as well, which this row did not name)*, so the change is a store change and a caller migration rather than a handler fix. `proposals.rs` already invented `Offer<T> { Made, Refused }` for exactly this distinction at `preview`, where a candidate rule that cannot be accepted is `Ok(Offer::Refused)` and a broken database is `Err`; generalising that type over the other five is the fix, and it is sized as one rather than deferred for lack of a design. **Reproduced on `main` (`6a2e72d`)** (`docs/notes.md`, "O70: a refusal is an offer, and a store fault is a 500"): a trigger raising `ABORT` on the accept's `INSERT INTO rule` answered 409, and so did a trigger on the un-accept's `DELETE FROM rule`, a trigger on the dismiss's `UPDATE`, and a dropped `scan_entry` table under the accept's dry run. The expanded row and the dry run already answered 500 for a store fault, and every id-addressed route answered 404 for an unknown id. **Fixed by making `Offer` public and returning it from the three writes.** `evidence` and `resolves` keep their types, since neither returns a refusal as `Err`. `acceptable` reads nothing, so it returns a bare `Offer`. `accept_with` now calls `engine_parts` before its dry run, so a rule it cannot evaluate stays a refusal instead of arriving as a failed read. `curation.rs` maps a write's `Err` to 500 and its `Refused` to 409 through one helper, `made`. `accept` keeps `Result<i64, String>` and folds the refusal back into its `Err`, as `preview` does, so no public signature changed beyond the three writes and `Offer` itself. The CLI's `--dismiss` and `--unaccept` fold it back the same way, and `rules::delete` returns it as its error, so each CLI refusal prints the same sentence and exits 1 as before. **Pinned by** three route tests that a store fault during an accept, an un-accept and a dismiss answers 500; a fourth for a failed read under the accept; a 409 for a rule the accept route cannot evaluate; a body assertion on the un-accept refusal; four store tests that call each write with a trigger fault and with a real refusal; and two CLI tests that assert a refusal's exact sentence and exit 1. **Hand-applied, 20 defects, 19 caught** (the table in `docs/notes.md`): each handler's `Err` mapped back to 409, each store refusal returned as `Err`, each store failure returned as `Refused`, a refusal's body dropped, a refusal answered 500, each CLI refusal reworded or swallowed, `accept`'s fold swallowed, and the `engine_parts` call removed. **The survivor** is `rules::delete` swallowing a refusal from `unaccept`. That refusal needs another connection to un-accept the proposal between `delete`'s lookup and its call, because on one connection `one` reads the `created_rule_id` the lookup has just matched | `proposals::{evidence, resolves, accept_with, unaccept, dismiss}`, `proposals::Offer`, `curation.rs`'s `NoAnswer` | spec C3's Task 5 review, 2026-08-26; closed 2026-09-14 |
| O71 | [x] | **Closed 2026-09-14, once each of the ten handlers had been moved onto the async worker by hand and a test that calls them directly had failed naming it. What remains is a reader with a grant, for whom four handlers here still take the library's mutex on the async worker.** **`routes/tests.rs`'s `a_progress_read_does_not_stall_the_runtime` is toothless, and ten handlers stand behind it.** *(Toothless for the handlers, not for everything: with `auth::guard`'s resolution moved onto the async worker it was the one test of 2129 to fail. It is `auth/tests.rs`'s `the_guard_does_not_stall_the_runtime` now, its body unchanged but for its failure message.)* Proved rather than suspected: `routes::get_progress` was moved wholly onto the async worker — the exact defect the test names — and the test still passed. *(Re-proved on `6a2e72d`: 2129 of 2129 passed, this test included.)* The cause is not in the test's own reasoning, which is careful, but in what a router-based test can observe: `auth::guard` resolves inside its own `spawn_blocking` and takes the mutex wait with it, so by the time the handler under test touches SQLite the reactor is already free, and the canary runs no matter what the handler does. **A defect in a guard is worse than no guard**, because the suite reports the property as measured. This is C1-era code that spec C3 did not touch; what C3 did was model a new test on it and, in checking that model, find the original blind. The ten `routes.rs` handlers that do their store work inside `spawn_blocking` — `list_works`, `get_page`, `get_thumb`, `get_notes`, `put_notes`, `get_truth`, `put_truth`, `get_progress`, `put_progress`, `patch_alignments` — *(complete and current: `routes.rs` holds exactly ten `spawn_blocking` calls on `6a2e72d`, one in each, and `routes/` holds only its tests; but `get_page` and `get_thumb` read a page in theirs, not the store, and a watchdog holding the library's mutex does not reach a page read)* have nothing else guarding that property. The shape of the fix is already on the branch and costs no new mechanism *(one small one: for the reason just given, `get_page` and `get_thumb` passed the direct test with both their closures on the async worker until the fixture's page source waited on the same mutex)*: `curation/tests.rs`'s `no_curation_route_stalls_the_runtime` calls handlers directly rather than through the router, and does have teeth (reverting any one of the six routes fails it and names that route; 0 failures in 25 runs under 24× CPU load). **Fixed by `routes::tests::no_works_route_stalls_the_runtime`**, which calls all ten handlers directly under `the_runtime_stalls_on` — moved unchanged from `curation/tests.rs` to `test_support` — against a fixture whose page source, `test_support::PagesWaitOnTheLibrary`, waits on the library's mutex before every read, and asserts that each call succeeded as well as that it did not stall. It carries the old test's other assertion too: the position written is the one read back. **Watched red for each of the ten** (`docs/notes.md`, "O71"): with that handler's closure run on the async worker, the whole workspace failed that one test, at that handler's assertion and naming it, 2129 of 2130 passing. Run 30 times alone at a one-minute load average of 11.28–12.42, it passed 30 times. **What remains**: for a principal carrying roots, `refuse_if_hidden` and `visible_editions_or_refuse` take `state.library.conn()` on the async worker before any closure, and under the watchdog `get_page`, `get_thumb`, `patch_alignments` and `get_sidecar` each stalled, where an empty grant did not. `curation/tests.rs`'s doc names only `get_sidecar` for this. Nothing tests it, and it is not filed as an item *(filed as O115 on 2026-09-14)* | `routes/tests.rs::a_progress_read_does_not_stall_the_runtime` *(now `auth/tests.rs::the_guard_does_not_stall_the_runtime`)*, the ten `spawn_blocking` handlers in `routes.rs`, `curation/tests.rs::no_curation_route_stalls_the_runtime` as the pattern, `routes/tests.rs::no_works_route_stalls_the_runtime`, `test_support::PagesWaitOnTheLibrary` | spec C3's Task 5 re-review, 2026-08-26; closed 2026-09-14 |
| O72 | [x] | **Closed 2026-09-15, once a dismissal had been dismissed, found under its own filter and reopened in the browser against a scanned scratch library. The inbox has an Open / Accepted / Dismissed control, and a dismissed row offers Reopen: `DELETE /api/library/proposals/{id}/dismiss`, `proposals::reopen`, and `library proposals --reopen ID` beside it.** **Spec C3 §4's "a dismissal you disagree with later is a thing you have to be able to find" is satisfied at the store and API layers only.** `proposals::list` takes a `state` filter and `GET /api/library/proposals` passes it through, both driven by their own tests — but `Proposals.tsx` sends `state: 'open'` and nothing in the app sends anything else, so the answer to "where do I find a dismissal" is nowhere a person can reach. **The hardcoding is right and is not what this item asks to change**: an inbox is the set of proposals still awaiting a decision, and omitting the filter would mix accepted and dismissed rows into the list with no control to sort them out again. What is missing is the control, and no task in this plan builds one — the gap is real, it is the only part of §4 not discharged, and it was left untracked until here. **Recorded as a later C3 PR's rather than C4's**, decided here rather than left hanging: the control belongs on the screen C3 built and needs nothing C4 introduces, where C4 is user administration and would be taking it on only because it happens to be next. A count of dismissed rows would give the sidebar entry a second thing to say; the *open* count it now carries needed no such control, and is not this item. **Built beyond the row's own ask, and why**: the row asks for a control to find a dismissal, and nothing could take one back — no route, no CLI flag, and `accept` refuses any proposal not `open` — so a dismissal found and disagreed with had nowhere to go. `proposals::reopen` sets a `dismissed` proposal back to `open` in one `UPDATE` whose `WHERE` carries the state, and refuses anything else with a sentence saying which: an open one has nothing to reopen, an accepted one is reopened by un-accepting it. A state change is the whole undo because a dismissal wrote no rule. **Accepted rows are listed and offer nothing**: no Undo, because un-accepting leaves what the rule derived standing (O74). **The header, the kind tabs' counts and the empty states read within the state selected**, since the server counts within the state a request carries: "2 dismissed" over a dismissed list, and three `decided-*` empty states that never say "nothing to answer". A reopened proposal is an ordinary open one again, so the next `raise` refreshes its score and reason, and `proposal_identity` keeps that pass from inserting a second row. **The dismiss control was guarded by `disabled` alone**, so two presses in one frame both reached the handler; Dismiss and Reopen now share one runner holding a ref. **Tests**: four in `proposals/tests.rs`, three in `curation/tests.rs` plus the reopen route added to the reader-refusal, unknown-id and stall enumerations, one CLI unit test and a `--reopen` case in the binary refusal test; in `web/`, one `curation.test.ts` case, one `EmptyState.test.tsx` case, four in `ProposalRow.test.tsx` and four in `Proposals.test.tsx`. Sixteen Rust and fifteen web hand mutations were each applied, watched fail and restored (`docs/notes.md`, "O72"). **A fix round after an independent review** made `dismiss` and `reopen` each one transaction holding a conditional `UPDATE` and the read that words any refusal, so a refusal describes the row the `UPDATE` missed; `dismiss` now also refuses a proposal still holding a rule, and `reopen` a dismissal still holding one — the row `dismiss`'s old read-then-write left when an accept committed between its two statements, a race that predates this branch (`cec59e2`). A store `Err` for a row gone by the time the store read it answers 404 (`curation::failed_unless_gone`), and the inbox's header and empty state read the state their list was fetched for rather than the pressed control, which had drawn "37 dismissed" over 37 open rows until the answer landed. It added three tests in `proposals/tests.rs` and tightened a fourth, one in `curation/tests.rs`, two in `ProposalRow.test.tsx` and one in `Proposals.test.tsx`; seven Rust and five web hand mutations went red, and one survived: the two handlers' wiring to `failed_unless_gone`, which no deterministic test reaches (`docs/notes.md`, "O72 fix round"). **What remains**: nothing on screen or in the CLI lists accepted or dismissed proposals outside this screen — `library proposals` still lists open ones only; the sidebar counts open ones only, as before; O74 is unchanged; and **the Accepted and Dismissed filters keep the inbox's default score threshold**, so a weak decided proposal is absent from the list whenever a strong one is in it, and the hint that Show weak proposals reveals more is drawn only over an empty list — a decision recorded by the fix round's review, not changed | `web/src/Proposals.tsx`'s state control, `ProposalRow.tsx`'s Reopen, `curation.ts`'s `reopenProposal`, `proposals::reopen`, `proposals::dismiss`, `curation::reopen`, `curation::failed_unless_gone`, `taiyaku-cli`'s `library proposals --reopen` | spec C3's Task 7 review, 2026-08-26; closed 2026-09-15 |
| O73 | [x] | **Accepting a proposal and rescanning left the subject's volumes present twice — once as the new edition of the target's work, which is what was asked for, and once still standing as their own works under the series the rule was supposed to fold away.** Recorded in `notes.md` with the reproduction (2026-08-26, ten archives hardlinked out of `corpus/` into a scratch tree, one accept and one `library scan` from a clean database): twelve member rows over ten files, and the scan's own summary printing counts — four series, eight works — that its database does not match, holding five and ten. Reproduced again at smaller scale while fixing the edition-kind defect beside it: four archives, six member rows, a summary saying one series and two works over a database holding two and four, unchanged by that fix. **The derivation was right and the store did not follow it.** The moved volumes were re-derived onto the target's work exactly as the accepted rule said, and nothing emptied the rows they were derived onto before: `DELETE FROM work` had appeared nowhere in this repository outside `schema/tests.rs`. Less an oversight than a case nothing had been designed for — `import::replace_editions` deliberately leaves an *edition* a scan no longer names standing rather than deleting it, with the reasoning recorded on it and the cost counted as `DeriveReport::kept_editions`, but a work being **emptied** had no equivalent, and accept was the first feature in the project that moved material between works. **The measurement the row asked for was taken on 2026-08-27** — a reader planted through the API on all four works of the reproduction, then the two works a fix would empty deleted — and it settled the question the row above left open: a bare delete took two `progress`, two `note`, two `truth_page`, two `truth_box` and two `truth_member` rows with it, so the third option (move what hangs off the emptied work, then delete it) was the only one that did not lose a reader's state. Building that ran into `progress`'s own key: `(user_id, work_id)` cannot hold two editions' positions on the same work, and folding two works onto one is exactly the case that produces that shape — widened to `(user_id, work_id, edition)` by migration 009 (`schema/009_progress_per_edition.sql`), which is also what stopped an ordinary edition switch from silently dropping a sibling edition's position, an older bug than this row. `derive::superseded` finds the emptied work (every member's `sha256` held by a work this derive wrote, every member's `root_id` non-NULL) and `fold::absorb` maps editions by member digest, moves `progress`, `note`, `truth_page`, `truth_box` and `truth_member` in that order inside one transaction, and only then runs the first `DELETE FROM work` in production code — refusing the whole fold instead, per work, when a reader row sits on an edition key neither side names or when one reader holds one note id on both works (see O76 through O79 below for what is still open around this fold). **Proved against real archives rather than only against fixtures, 2026-08-27**: the same four-archive reproduction, a reader planted through the API (not `INSERT`) on all four works, accept, rescan. The database went from 4 works/4 editions/4 members over 4 files to 2/4/4/4, and the scan printed `derived 1 series, 2 works, 4 editions, 4 members, 2 work(s) folded away (2 position(s) written, 0 position(s) lost, 2 note(s) and 2 truth page(s) moved, 0 truth page(s) already held, 0 row(s) discarded), 1 series left empty; 0 proposal(s) raised`. All twelve reader rows survived, remapped onto the two surviving works' new `full-color` editions; a restarted `serve` answered both positions newest first and served an actually-coloured page 33 under `full-color`. `docs/notes.md`, "O73's fold, proved against real archives, 2026-08-27" | `schema/009_progress_per_edition.sql`, `derive::superseded`, `fold::absorb`, `fold::edition_map`, `fold::DISCARDED` | spec C3's corpus session, 2026-08-26; cascade measured 2026-08-27; closed by the progress-per-edition design (`docs/superpowers/specs/2026-08-27-progress-per-edition-design.md`), proved against real archives 2026-08-27 |
| O74 | [ ] | **Un-accepting a proposal is unreachable from any screen, and would not undo the accept if it were.** `DELETE /api/library/proposals/{id}/accept` exists, is admin-gated and is driven by its own tests; `curation.ts`'s `unacceptProposal` exists and **has no caller** — nothing in the app sends it, and an accepted proposal is not reachable from any screen either, since `Proposals.tsx` asks only for `state: 'open'` (which is O72's half of this). **The reachability gap is the smaller half.** `unaccept` deletes the rule and reopens the proposals that rule answered, and that is all it does: a scan has already run by then, and `import::replace_editions` deliberately leaves an edition the incoming document no longer names *standing* rather than deleting it — counted as `DeriveReport::kept_editions`, with the reasoning recorded on it. So the `full-color` editions the accepted rule produced stay on the target's works after the un-accept, still carrying the `kind` that rule set, sourced from a rule that no longer exists. Measured on the same four-archive reproduction as O73 (`notes.md`, 2026-08-26), driven the whole way rather than reasoned about: accept, `library scan`, `library proposals --unaccept 1`, `library scan`. The un-accept deletes rule 5 and reopens the proposal, and **changes no `edition` row at all** — nothing has scanned yet. The rescan after it prints `derived 2 series, 4 works, 4 editions, 4 members, 2 editions kept`, and those two kept editions are the `full-color`/`official` rows on works 1 and 2, still standing, still holding the kind the deleted rule set. Six member rows over four files before the un-accept and six after it. **So a control labelled Undo would not undo**, which is why no task built one and why this is a row rather than a UI task: a button whose name is a false statement about what it does is worse than no button, and this is the same class of finding as the accept panel's own "move N works" sentence, corrected on this branch. **What would close it is a decision, not a screen**: either `unaccept` learns to withdraw what the rule derived — which runs into exactly O73's cascade question from the other side, since the editions to withdraw hang off works — or the surface states plainly that it reopens the question and leaves the material where the last scan put it. Neither is a line, and picking between them is a later C3 PR's. *Narrowed on 2026-09-15 by O72's close, and left open: the half this row gives to O72 is gone — an accepted proposal is listed under the inbox's Accepted filter, read-only, with no control on its line. `unacceptProposal` still has no caller and this row's decision is unchanged. The prose above stays as written, per the ritual.* | `proposals::unaccept`, `curation.ts`'s `unacceptProposal`, `import::replace_editions`'s `Scope::Scan` arm, `DeriveReport::kept_editions` | spec C3's whole-branch review, 2026-08-26 |
| O75 | [x] | **Closed 2026-09-14, once React's warnings had been counted with the output visible. Every component test in this repository runs `act()` outside an act environment.** `IS_REACT_ACT_ENVIRONMENT` is set in `web/tests/support/browser-setup.ts`, and `vite.config.ts` lists that file under `setupFiles` for the **`browser`** project only — the `jsdom` project, which is where `src/**/*.test.tsx` runs, has no `setupFiles` at all. That file's own doc states the consequence: without the flag "`act` does not actually flush effects synchronously — so a test can pass for the wrong reason, observing a state React had not committed yet". *(Wrong, and never measured: `act` flushes with the flag unset, and what the flag changes is which warning React prints — see below.)* It then says it is set "for the `browser` project only; the `node` project renders nothing", which accounts for two of the three projects and is why the gap reads as deliberate. **Scope: the whole `jsdom` project**, 603 tests across 34 files at the time of writing, including every component test spec C3 added. Not measured as *wrong*, only as unguaranteed — a mutation sweep over `AcceptForm` and `ProposalRow` killed 38 of 40, so those tests do observe real behaviour; what is missing is the guarantee that any of them observes committed state rather than passing on timing. Adding `setupFiles` to the `jsdom` project is one line; the reason it is an item rather than a patch is that turning the guarantee on may surface tests that were passing for the wrong reason, and that count is unknown until it is done. **What the flag does, measured** (`docs/notes.md`, "O75: what the act flag changes, counted with the output visible"): on React 19.2.8 it is read only by the code that chooses a warning. Unset, an update inside `act` logs "not configured to support act(...)" and an update outside `act` logs nothing; set, the first goes quiet and the second is reported as "not wrapped in act(...)". So `act`'s flush was never missing — the report of an update landing outside it was. **Counted over the whole `jsdom` project under vitest's `default` reporter**, since under Claude Code vitest picks its `agent` reporter, which prints no console output from a passing test, and a first run showed no warning of any kind: on `main`, 6,819 "not configured" (a control test's own among them) and 0 "not wrapped"; with the flag set, 0 and 20, in seven tests — three in `Routes.test.tsx`, four in `App.test.tsx`; 951 passed every time. **None of the seven was passing on timing**: in each the update landed after the last assertion — the route tests assert synchronously before their mocked fetches answer, and the reader tests change the zoom and return before `PageView` shows the page at its new `?w=`, which was logged either side of the settle — so each now ends in an `act`-scoped settle with nothing asserted after it, and the count is 0. Removed one at a time, each settle brought back warnings in its own test and no other. The flag is now set for `jsdom`, and `browser-setup.ts`'s doc says what it does. **Not done**: nothing fails on the warning — put to the user and declined — and `browser-setup.ts` keeps its name | `web/vite.config.ts`'s `jsdom` project, `web/tests/support/browser-setup.ts` | spec C3's Task 9 review, 2026-08-26 |
| O76 | [x] | **Closed 2026-09-15, once a fold had been driven over real archives before and after. A fold carries an alignment with its editions wherever every span of it covers the same pages and halves on the surviving work, and a twin holding no spread flags takes the folded edition's.** **A hand-corrected `alignment` is discarded by the fold and cannot be recovered.** `fold::DISCARDED` counts an emptied work's `alignment` rows by one predicate, `work_id = ?1`, with no distinction between a solver's own output and a person's correction of it — unlike `fold::DISCARDED`'s `member` entry, which already draws exactly this line for `chapter_declared` by checking `chapter_source IS NOT 'filename'` before counting a loss. `alignment.by` carries the same distinction for alignments — `'solver' \| 'hand'` — and nothing reads it here. *That sentence is wrong, marked here rather than rewritten: a person's correction is recorded per run, as `run.by = 'hand'`, which is `Sidecar::merge_spoke`'s own test, and `alignment.by` reads `hand` only on an edge `Sidecar::correct` created — a correction on a solver's edge leaves it `solver`. Found closing this row, 2026-09-15.* A `taiyaku align --write` result a person corrected by hand is, in `member.chapter_declared`'s own words about the case DISCARDED already handles correctly, "the one thing here no re-derive could recover", and it is gone with the emptied work the moment a fold succeeds, counted only as one more number in `Moved::discarded` beside a scanlation credit or a stale edition label. **Not found by this branch's own feature** — alignments predate spec C3 entirely — but reached by it: `absorb` is the first code in this repository that deletes a `work` row, so it is the first place this loss was ever possible **The decision was the user's** (2026-09-15): carry rather than refuse, because nothing undoes a correction short of making another (`web/src/correction.ts`), so a refused fold would have stood duplicated for good, and a fold only goes ahead between editions holding the same archives in the same order. **The premise it was briefed on was false, and that is O121**: identical archives do not make an alignment's slots name the same pages on both works. A slot is an index into the sequence an edition's spread flags and its work's reading direction build, and a scan writes no spread flag, so a probe that moved an alignment onto a twin holding none read it back as unreadable and `route` answered nothing; emulated over the corpus's 22 alignments by `scratchpad/o76/corpus-emulation.py`, whose log is kept beside it, 14 are unreadable on twins holding no flags. **Fixed by** `fold::plan_slots` and `fold::plan_alignments`, decided before any write: the twin takes the folded edition's spread flags where it holds none and no two folded editions meeting on it hold different sets — one holding none is unmeasured, since a scan never writes a flag, and does not disagree; every `progress` and `note` slot on either work is placed by page and half, and the fold is refused over one that named a page and has no slot naming it, while a slot past the end of its edition names no page and moves as it is; an alignment is carried where its spans cover the same slots, and a hand-corrected one replaces a pair's answer that holds none in either stored direction; one left behind is counted in a new `uncarried_alignments`, except a solver's alignment whose pair the target already answers and a solver's edge between two editions the fold makes one, which are left uncounted — through `fold::Moved`, `DeriveReport`, the scan summary, the scan JSON and the scan report. `alignment` and `run` left `fold::DISCARDED`, and its `page_flag` entry counts credit flags only. **Driven over real archives both ways** (`docs/notes.md`, "O76 and O121"): an aligned `taiyaku.json` imported over two hardlinked Go-Toubun v01 archives, a correction pairing raw page 100 with page 95 made through the route, and a rule change that folds the work. On `main` the scan printed `16 row(s) discarded` and `lookup` answered `no counterpart`; on this branch it printed `0 row(s) discarded, 0 alignment(s) not carried`, the surviving work holds the seven spread flags and both hand runs, `lookup raw-mono en-mono 100` answers 95, and a second correction on the carried edge, which does not start at the hub, answered 200. **What remains**: the fold writes rows and no file — a surviving work with a `sidecar_path` holds a carried alignment in rows until `library export --in-place`, and the folded work's own `taiyaku.json` still names it, as has always been true of notes and truth; and a fold is refused, not carried, where a place or note on either work, or an alignment the surviving work holds, sits on a page the new flags split, or where two measurements of one archive's spreads disagree under a place or note that depends on them — an alignment in that case is left and counted instead. **Unmeasured, read and not driven**: `library import` of a document over a surviving work (`import::reimport`, `Scope::Document`) replaces its page flags from the document, so a document holding no spreads would drop the flags a fold wrote, and every place and note the fold had placed under them would then name another page. **The review of the first commit** found eight things, each fixed or recorded on the branch's second commit: `docs/notes.md`, "O76 and O121", "The review's fix round". | `fold::DISCARDED`, `fold::absorb`, `fold::plan_slots`, `fold::plan_alignments` | spec C3's Task 10 corpus session, 2026-08-27; closed 2026-09-15 |
| O77 | [x] | **Closed 2026-09-15, once a test named the guard and failed against each half of it mutated by hand.** **`derive`'s `unattached_chapter_dirs` guard survives a mutation, and it predates this branch.** `admitted.resolved.volume.is_none() && admitted.resolved.chapter.is_some()` still passes the whole `taiyaku-library` suite — 521 tests, 0 failed — with `&&` replaced by `\|\|`, verified by making the substitution and running the crate directly rather than by reading the claim off a stale record. Under `\|\|`, an ordinary `v01 - c05` directory (volume *and* chapter both resolved) satisfies the guard and would be reported as an unattached chapter directory. `docs/mutants-baseline.md` has carried this exact mutant since the first workspace-wide sweep, 2026-08-19, marked **UNTRIAGED** throughout and renumbered once already (`:1262` to `:1451`) as code was added above it in the file — that file's own words for an UNTRIAGED entry are "not because anybody has judged it acceptable", and triaging it here finds a live missing assertion rather than an equivalent mutant or a decision inside output nobody reads, so it does not belong on that file's accepted list. **Deliberately left off it rather than added with a reason**, because a baseline entry is a claim that someone looked and found it fine. Found by this branch's mutation sweep, from a function the scanner branch wrote. *Out of date as of 2026-09-14, and left open. The substitution no longer passes: applied by hand in place on `6a2e72d`, `taiyaku-library`'s suite ran 785 tests and failed one, `preview::tests::a_preview_and_a_real_save_produce_the_same_structure`, whose literal `Impact` came back with `proposals_raised: 1` against `0` — `proposals::raise_attach_chapters` raises one proposal per series holding a directory `unattached_chapter_dirs` names. That catch is incidental: the test pins a preview against a real save and does not name this guard, so the deliberate assertion this row asks for is what remains. Measured in `notes.md`, "O77 and O51: two rows the code had moved past". The row stays open and its prose above stays as written, per the ritual.* **Closed by `derive::tests::only_a_directory_resolving_a_chapter_and_no_volume_is_asked_about`**, which calls `unattached_chapter_dirs` itself, because `report.unattached_chapter_dirs` is `classify`'s count and cannot see this guard. Its fixture is five directories: two resolving a chapter and no volume, in different series, one a volume and a chapter, one a volume alone and one neither. It asserts the map of the two. `\|\|`, `volume.is_some()`, the chapter half dropped and the volume half dropped each fail it with a different map, and the last of the four failed nothing else in `taiyaku-library`. The mutant's UNTRIAGED entry is gone from `docs/mutants-baseline.md`. Measured in `notes.md`, "O77: a test that names the guard". | `derive::unattached_chapter_dirs` | spec C3's Task 10 mutation sweep, 2026-08-27 |
| O78 | [x] | **Closed 2026-09-14: two copies of one note id that agree in every column now fold, the superseded work's copy dropped; copies that differ in anything still refuse the whole fold, and that refusal is what remains.** **A fold pair is permanently unfoldable when one reader holds a note id on both works.** `fold::absorb` refuses the whole fold rather than re-key a `note.id` that collides between `from_work` and `into_work` for one reader (`fold::shared_note_ids`), because `note`'s key is `(user_id, work_id, id)` and moving the source's row onto the target's key would collide with an id the reader's own sidecar already named there. Migration `005_note_id_per_user.sql` widened that key specifically to *permit* this state — its own rationale names the routes: "copying a `taiyaku.notes.json` between volumes, or importing one sidecar against two works" — so the refusal is declining to touch a shape the schema was deliberately opened to allow. **The identical-note case is losslessly resolvable and nothing does it today**: same id, same body, and the same anchor once the mapped edition's slot rewrite is accounted for *(there is no slot rewrite: `move_notes` rewrites the edition key alone, and `edition_map` pairs editions holding the same digests in the same order, so slot and region carry across unchanged)*, describes one fact twice, and dropping the source's copy loses nothing — but `absorb` refuses on any shared id regardless of whether the two rows agree, not only the ones that conflict. `taiyaku-serve::notes::validate`'s own grandfathering of a note whose edition key nothing currently maps means a reader who reaches this state has no ordinary path back out of it through the app, so it is self-perpetuating rather than something a reader will clear up by using the product normally *(not supported: a note the grandfathering keeps is on a key the edition map does not hold, and `absorb`'s `stranded` check refuses that before it ever compares note ids; deleting one copy through the app was not tried)*. **Confirmed on `main` (`6a2e72d`)**: all 25 `fold::tests` passed, and one of them asserted a refusal over five ids identical in every column on both works. **"Same body, same anchor" left `color`, `created` and `updated` unsettled**, so work stopped there and the user decided: every column. Two copies are one note when `body`, `slot`, `rx`, `ry`, `rw`, `rh`, `color`, `created` and `updated` are equal and the target's `edition` is exactly the key the source's maps to — compared in Rust, because SQL's `NULL = NULL` is not true and the ordinary whole-slot note carries five NULLs. **The drop comes after all five refusal checks and before the move loop**, so every refusal is still a `SELECT`. It is counted in `Moved::dropped_notes` and `DeriveReport::dropped_notes` and reported as "identical note(s) dropped" in the CLI summary, `dropped_notes` in the scan JSON and "Identical notes dropped" in `ScanReport.tsx`; `moved_notes` still counts rows moved. **Ten new `fold::tests`** — an identical whole-slot pair folding with the target's row checked by `rowid`, a pair with a region and a colour on keys that map, one refusal each for body, colour, `updated`, `created`, region and a key that does not map, an identical id beside a differing one refused whole, and a second reader left untouched — with the old refusal test renamed as the slot case and `the_shared_note_refusal_counts_the_ids_and_shows_a_few` given a different body per work, since it asserted a refusal over identical copies. The refusal tests make every write to `note` raise first, because the transaction's rollback hides a drop made before a refusal. **Watched red**: four tests before the fix, and each of fifteen hand-applied defects on the whole workspace — body only, raw edition keys, the edition ignored, differing copies dropped, the target's copy dropped, the drop before the refusal, `color`, `updated` or `created` ignored, NULL regions unequal, drops counted as moves, the delete or the match without `user_id`, the `derive` fan-out removed and the CLI summary's arguments swapped — failed at least one test, and both defects applied to `ScanReport.tsx` failed its test (`docs/notes.md`, "O78"). The drop before the refusal passed every test once the triggers were taken out of the test. **Driven on real material**: two hardlinked copies of `kaguya-v01` folded through `library scan` with two identical copies dropped, and were refused, naming only that id, when one copy's body differed | `fold::absorb`, `fold::shared_note_ids`, `schema/005_note_id_per_user.sql`, `taiyaku-serve::notes::validate` | spec C3's Task 10 design review, 2026-08-27 |
| O79 | [x] | **Closed 2026-09-14, once a database error had been placed partway through a progress document.** **`migrate::apply_progress`'s loop is not atomic.** It calls `state::put_progress` *(`state::put_progress_at` — the loop has called that since `91b08d8`, the commit that filed this row)* once per position in a legacy `taiyaku.progress.json` document with no transaction around the loop, so a failure partway — one malformed position later in the document, or a database error — *(half wrong when written: `progressdoc::from_json` parses the whole document before the loop writes anything, in `91b08d8` as now, so a malformed position fails with nothing written; only a statement SQLite refuses partway reaches the loop)* leaves the positions before it written while `migrate_progress` returns `ProgressOutcome::Malformed` rather than `Applied`. `record_settled` runs only on the `Ok` arm, so the document is never marked settled either. The next run's `holds_state` check (`!state::progress(…)?.is_empty()`) then finds the rows the partial run already wrote and reports `Step::Kept` — read as "already present" — rather than `Step::Apply`, so the remaining positions in the document are never retried: the migration gets stuck half-done and nothing anywhere records that it is. **Still present on `main` (`19a6988`), by that one path.** Nothing in `progress`'s schema can refuse a later position once an earlier one was accepted, so a test trigger stands in for the error (`docs/notes.md`, "O79: a progress document commits once"): refusing the second of three positions on `main`'s code left the first written, returned `Malformed` and recorded no marker, and with the trigger dropped the next import returned `AlreadyPresent`. **Fixed by one `unchecked_transaction` around the loop, committed after the last position** — the idiom `state::put_notes` and `state::put_truth` use — so a refused position rolls every position back and the next import applies the whole document. No caller of `migrate::work` holds a transaction open when it runs. **Pinned by one test** asserting `Malformed` naming the refusal, no rows, no marker, and — trigger dropped — `Applied` with all three positions; watched red against no transaction, a commit before the loop, a transaction per position and a commit on the failing position (each left the first position written), no final commit (no positions after the retry), a discarded error (`Applied` on the first import), a marker recorded on failure, and `main`'s code with the no-rows assertion removed (`AlreadyPresent` on the retry). **Not changed**: `record_settled` still runs after an apply has committed, on all four documents. Refusing that insert in a throwaway test left the position written, no marker, and `migrate::work` returning `Err`; the next import answered `AlreadyPresent` and recorded `absorbed` false, and an in-place export then left `taiyaku.progress.json` as it was, in `Exported::kept`, rather than rewriting it. The rows were complete, so it is not this item | `migrate::apply_progress` | spec C3's Task 10 design review, 2026-08-27 |
| O80 | [x] | **Closed 2026-08-31 by giving all six fields the provenance migration 010 gave `title` and `label`, which is I7's own answer.** The defect was one asymmetry: `librarydoc::of` projected `title_hand` and `label_hand` — the *hand* values — but plain `work.volume`, `work.series_id`, `edition.language`, `edition.kind`, and **every** `series_title` row including the one derivation mints. So the document every save wrote claimed derivation's own answers as a person's; `hand::save` armed the `library` marker on any non-empty document; and `write_work` read that marker as "never touch these four columns again". `MetadataEditor.tsx` sends `{ title }` unconditionally and title is the only field the shipped UI edits, so pressing Save — even unchanged — withdrew three columns nobody had touched, irreversibly, since no route can clear a field and `derive` never reads the document again. **Migration 011** adds `work.volume_hand`, `work.series_id_hand`, `edition.language_hand`, `edition.kind_hand`, `series_title.hand` and `series.reading_hand`. `derive` writes `COALESCE(<hand>, <what this pass derived>)` and **the `absorbed` guard comes off the overwrite entirely**, keeping only its real job — *this document has been read back, do not read it again*. Conflating those two jobs in one flag is what let a title edit freeze a volume. One column each where 010 needed two: these are recomputed every pass, so there is no derived copy to keep. **`SeriesInfo::chosen` is what made the series half possible.** That block carries two different facts — the *series'* titles, which are series-level data replicated into every member work's document, and *this work's* attachment. Without an explicit flag a document correcting only the titles re-asserted a derived attachment as hand-chosen on the next apply, which is this defect moved one table across rather than fixed. `apply_series` still resolves by title text, deliberately: a document reaching a database that has never seen this library has nothing else to resolve against. What changed is only whether that resolution is recorded as a person's choice. **It also closes a consequence O53 recorded and this item did not name**: `of` read every `series_title` row, so *every scanned work* already projected a non-empty document — O53's *"librarydoc::of reads non-empty the moment a work has a derived volume or series"*. That is the same defect one level up, and it is gone. **Two regressions caught on the way, both by existing tests rather than by review.** An empty series block used to mean "take this work off the shelf"; it now means "nobody corrected this series", true of nearly every scanned work, so clearing `series_id` unconditionally detached all of them — withdrawal now removes the effective attachment only where it was hand-chosen. And the clear-versus-keep answer differs per field: `language`/`kind` clear immediately because derivation restores them, `label` falls back to `label_derived` because nothing recomputes it, `volume` keeps its value until the next scan. **What remains, and it is a design round rather than a defect**: the series' own metadata is series-level data stored per work, which is the common cause of O85's N+1, O86's straddling grant and the `chosen` flag this needed. A series-level document would dissolve all three; nothing here attempts it. **Correction, 2026-08-31: this entry's own title, "the document projects only what a person set," does not hold for chapters.** `librarydoc::of` also projects `ChapterSource::Comicinfo` and `ChapterSource::Filename` chapters alongside `Hand` ones, and `derive::write_chapters` writes a fresh `Filename` chapter from the member's name on every scan, whether or not a person has ever touched the editor — chapters were never brought under migration 011's provenance columns the way the six fields above were. Found while building `hand::reconcile`'s staleness comparison, which had to filter `Filename` chapters out of its own check to stop reporting derivation's own churn as a person's disagreement. That passage above is left as written because the six-field fix it describes is real and unaffected; chapters are a field this entry never reached | `schema/011_hand_provenance.sql`, `librarydoc::of`, `librarydoc::apply_series`, `derive::write_work` | spec C3 PR 2, 2026-08-28; closed 2026-08-31 |
| O81 | [ ] | **A hand document is stranded when the hub edition's own files change.** `hand::path_for` addresses `<root>/.taiyaku/<content_key>.library.json`, and `identity::content_key` hashes the hub edition's members' `sha256`s — so re-scanlating, repacking or adding a page to a hub member moves the key and `hand::load` no longer finds the document that recorded the correction. The file is not deleted and an exact key match still finds it if the content returns, and the address deliberately survives the three things that actually happen routinely — renames, remounts, and *added editions* (the hub is fixed at creation for exactly that reason) — so this is the narrow remainder rather than the common case. What makes it worth an item is that it is silent: derivation simply re-derives over the work and nothing reports that a document at a now-unreachable address describes it. **Measured on 2026-08-31 against five works copied out of `corpus/` (`docs/notes.md`), and the claim in the sentence above — that derivation re-derives over the work — is wrong.** It has not been true since O80 closed on 2026-08-30: migration 011's `_hand` columns are re-applied by `COALESCE` on every pass, and `title_hand` came through all three key-moving events intact. That passage is left as written because it is why this was filed as the narrow remainder. What is actually lost is the durability half alone, and that half is not narrow — repacking an archive and adding one page each moved the key, and after rebuilding the database from the same root **2 of 5 works came back at their derived filename**, their corrections sitting unread in `.taiyaku`. A rename moved nothing, exactly as the entry claims. O81's orphan and O82's gap are produced by the same event and found by the same listing: one reconciliation pass, not two items. **Partly answered on 2026-08-31: an orphan is now reported with the title it holds, so the loss is no longer silent. It is not prevented — that is `rename_for_rekey`'s job, on its own branch.** *(That branch has landed, the same day: `hand::rename_for_rekey` moves the document from its old address to its new one the moment `derive` detects the rekey, called from `derive::write_work` after `write_roots` — not from `import_with`, where an earlier version of this fix placed it and found `hand::path_for` answering `None`, because the member whose bytes just changed has its `root_id` wiped by `replace_editions`' delete-and-reinsert and `write_roots` is what restores it, by ordinal, one step later. Never overwrites: a file already at the target address is left standing under both keys, and the old one still reports as an orphan for a person to read, on the reasoning that two works cannot legitimately share an address. This closes the *files-changing* half of this item. The fold half below is untouched by it.)* A hand document is stranded by a second cause besides its files changing: `migrated_document.work_id` is `ON DELETE CASCADE`, and `fold::absorb`'s own `DELETE FROM work` is the only place in this project that deletes a `work` row, so a work folded away takes its `migrated_document` row with it and leaves the document itself unaddressed — nothing carries it forward to the fold target the way a rekey's rename will. `reconcile` runs after the fold for exactly this reason and reports the document as an orphan on the same pass that strands it; nothing moves it. Whether the fold should carry the document to its target instead is a decision beat this does not take. The files-changing cause is now **prevented** as of 2026-08-31 — `hand::rename_for_rekey`, called from `derive::write_work` after `write_roots`, carries the document to the new address at the one moment both keys are known — verified against real archives with a repack and an added page, where all three corrections survived a database rebuild that the same shape had cost two of five before. What remains open is the fold cause alone | `hand::path_for`, `identity::content_key`, `hand::rename_for_rekey`, `derive::write_work`, `fold::absorb` | spec C3 PR 2, 2026-08-28 |
| O82 | [ ] | **`persisted: false` is reported and nothing ever retries it.** `hand::save` writes the row and records the `LIBRARY` marker whether or not `<stem>.library.json` reaches disk, so an edit made against a read-only root is safe from the next scan and is honestly labelled as not durable — the PATCH routes answer `persisted: false` with a `why`. But nothing re-attempts the write when the root becomes writable, and no command exists to sweep works whose marker is set and whose document is absent. A library on read-only media therefore accumulates corrections that live only in the database, which is the exact durability claim (store spec §4.4: *losing the database costs a rescan, not the organisational work*) that the document exists to keep. **Measured on 2026-08-31 (`docs/notes.md`): confirmed, and there is a third state this entry does not name.** Write permission was restored and a full scan and derive run over the root; the document still held the superseded title afterwards, so *nothing retries* is now driven rather than read off the code. But an edit made against a read-only root whose document already exists leaves that document **stale rather than absent** — `persisted: false` came back naming `staging a temporary beside it: Permission denied`, the file stayed where it was holding the previous correction, and a rebuild from the same root read that older title back over the newer one. A sweep counting only presence calls such a work reachable and walks past it, and the revert looks like a success rather than a loss, which makes it the worse of the two shapes. The same listing answers all three: `migrated_document` where `document = 'library' AND absorbed = 1`, resolved against each root's `.taiyaku`. **Partly answered on 2026-08-31: `hand::reconcile`, run per root at the end of `derive`, writes a document a failed save left missing and reports the two states it must not resolve. What stays open is the stale case — a document that exists and disagrees — which is reported and nothing else.** **Narrowed on 2026-09-02: `root.keeps_documents`, off by default (migration 012), lets a root decline this document outright, and a root that declines never attempts the write this entry's failure depends on — so the read-only failure described above cannot arise there at all; the setting removes the precondition rather than repairing it (`docs/superpowers/specs/2026-09-02-hand-documents-off-by-default-design.md`).** | `hand::save`, `hand::path_for`, `hand::reconcile` | spec C3 PR 2, 2026-08-28 |
| O83 | [x] | **Closed 2026-09-14, once a series' titles had been read back, corrected and sent whole over real archives. `GET /api/library/series/{id}` now carries `series.titles`: every title the series holds, derivation's included, each `{title, kind, primary}` in the shape `PATCH /api/library/series/{id}` takes.** **Spec C3 §5's series-level editing is designed, routed, and not delivered — no read path carries the titles back.** `PATCH /api/library/series/{id}` ships and works, but the series page cannot use it: editing a series' titles means sending `LibraryDoc.series.titles` whole *(its type, not its rows: a document holds only `hand = 1` titles since O80, and `set_series_titles` deletes derivation's row as well, so a document's list sent whole would still drop one — which is why the read added here answers every row)*, because `librarydoc::set_series_titles` — what `patch_series` calls directly onto the id the URL names, since C1's fix retired the old resolve-by-title path through `apply_series` — replaces a series' whole title set rather than merging into it (an explicit `DELETE FROM series_title WHERE series_id = ?1` before `titles` is written back), and nothing the browse API returns for a series carries that array back for a caller to send the rest of. `SeriesCard.title` is already the merged display string *(not merged: `browse::series` chooses the primary title, falling back to `sort_title`)*, never the underlying `{title, kind, primary}` entries — so a screen wired to that field would send a one-element list and **silently drop a series' native and alt titles** the first time anyone corrected the romaji one. The editor is therefore mounted only on a volume's detail panel, where a work's slug is all the correction needs. Closing this is a read path (`series.titles` on the series response), not a UI change. **Still present on `main` (`6a2e72d`).** No route that answers a series carried the entries, and the two tests below failed there in reading them, with "carries no `titles` array". **Fixed by `librarydoc::series_titles`**, which reads every `series_title` row for the series in `rowid` order, the order `set_series_titles` inserts. `library_api::one_series` adds them only once `browse::one_series` has answered the series, so a series the grant hides is still a 404, and sends a `false` `primary` rather than leaving it out. The grid, search and the PATCH answer are unchanged. `web/src/library.ts` gains `SeriesDetail`, which `oneSeries` answers. **Two tests**, over three scanned series whose ids differ from their grid positions and, for two of them, from their works' ids, each holding two or three titles with derivation's row kept: one checks every field of every entry of all three, and one reads Kaguya's, corrects the romaji entry, sends the whole list and reads all three series again. **Watched red**, each applied by hand to the final code with the whole workspace's tests run and the file put back byte for byte: `kind` left out of each entry, only the primary entry, another series' titles, the display string as the one entry, a work's id read as the series', `hand = 1` rows only, and a `false` `primary` left out each failed both tests and nothing else, 2129 passing, against 2131 for the unchanged control. The work-id mutant passed both until the fixture's first series gained a second volume. **Over real archives** (`docs/notes.md`, "O83: a series answers its own titles"): four `en-mono` archives hardlinked from `corpus/` and scanned; Kaguya's derived title was re-kinded romaji beside a native and an alt title, then its text corrected and the whole list sent back, and the native and alt entries came back as sent, `primary` included, with the other two series unchanged. **What remains: no screen edits a series' titles.** `MetadataEditor` edits one string and hands `onPatch` a `WorkEdit`, so spec C3 §5's series page needs a list editor of its own, and reading direction, tags and external ids have no read path on the series response. **Seen and not acted on**: after that correction a rescan minted a second Kaguya series under the scanned title and moved both volumes onto it. `set_series_titles` had moved `sort_title` to the corrected primary text, and `derive::reconcile_series` finds a series by `sort_title` *(filed as O114 on 2026-09-14)* | `browse::SeriesCard`, `librarydoc::set_series_titles`, `librarydoc::series_titles`, `library_api::one_series`, `web/src/library.ts`'s `SeriesDetail`, `web/src/MetadataEditor.tsx` | spec C3 PR 2's Task 9, 2026-08-28; closed 2026-09-14 |
| O84 | [x] | **Closed 2026-08-29: the browse response carries the edition the reader will open, and the guard asks about that one rather than about all of them.** `unreadable` was `editions.every(missing > 0)` — right about the case it was written for, a volume readable through an intact sibling, and wrong about the volume whose *own* opening edition is the broken one, which it offered and which failed at the first page. **The fix is not the hub, and the entry proposing the hub was the cheaper wrong answer.** `startup.ts` calls `reading::restore(progress, hub, resolver)`, which opens the *stored position's* edition when the resolver knows it and the hub only otherwise, and `api.ts::getProgress` hands it the head of a newest-first list because a position became per-edition in migration 9. So `WorkCard.opens` is the caller's newest position **if the grant still shows that edition**, and `work.hub` in every other case — a progress row naming an edition the caller cannot see is one `resolver.slotCount` answers `undefined` for, which is exactly `restore`'s own fallback. Withholding on the hub alone would have dropped a work out of `Library.tsx`'s Continue strip while its reader was part-way through the intact sibling: O62's over-withholding in a narrower form, and the reason the cheaper answer was declined rather than shipped. **This makes `WorkCard` per-caller**, which it had never been: `works_of` took no `user_id` and now does, threaded through `work`, `unfiled` and `search` and their four `taiyaku-serve` call sites. `opens_by_work` answers the whole set in one statement, O85's rule beside it. **Both wrong implementations are ruled out by test rather than by argument** — hand-applying hub-only fails `opens_names_the_stored_position_and_falls_back_to_the_hub` on its position assertion, and dropping the visibility filter fails it on its fallback assertion. **One defect was reported on the way and did not exist; the real one was in this fix.** It was written up here that a grant hiding the *hub* breaks the reader outright, reasoning from `session.ts::orderedEditions` prepending the hub regardless. That is wrong, and it is left stated rather than deleted because the wrong reasoning is the point: `routes::filter_sidecar_editions` already substitutes a hidden hub with the first surviving edition in key order, and says so in its own doc — the claim came from reading the client without reading the route that feeds it. **What the substitution does mean is that `opens` had to make it too**, and the first version did not: with the hub hidden, `opens` named it anyway, `unreadable`'s `find` answered `undefined`, the guard answered "readable", and whatever the substitute had actually lost went unasked — this fix's own version of the defect it was closing. `opens` now falls through to the first visible edition in key order, the same order that function's `BTreeMap::keys().next()` takes, and `opens_substitutes_the_first_visible_edition_when_the_grant_hides_the_hub` fails without it | `web/src/library.ts`'s `unreadable`, `browse::WorkCard::opens`, `browse::opens_by_work` | spec C3 PR 2's Task 9, 2026-08-28; closed 2026-08-29 |
| O85 | [x] | **Closed 2026-08-29: a listing costs two statements at any size, where it cost one per work and one more per edition of each.** `works_of` called `editions_of` once per work in whatever set it resolved, and `editions_of` called `missing_members` once per edition of that work — an N+1 nested inside an N+1, in the very module whose own comment on `WorkDetail` cites the whole-branch review's 111-query fix in `home` as the precedent not to repeat. `editions_by_work` and `missing_by_edition` answer the whole set in one statement each, keyed by work id and by edition id; `missing_by_edition` returns a map in which **an edition with nothing missing has no row at all**, so a caller reads absence as zero rather than expecting a key per edition. **Measured against the corpus, which is what this entry said had not been done** — 13 roots, 11 series, 2026-08-29: listing the largest series fell from **8 statements to 4**, and `home`, whose three lists are already bound at `LIMIT 20`, from **43 to 5**. `unfiled` — reached by both `/api/library/series` and `/api/library/search`, and the one path carrying no bound at all — is the same 4. **The check is scale invariance rather than a literal**: statements are counted through a `#[cfg(test)]` thread-local that this module's own `prepare` and `query_row` wrappers increment, and a 2-work series is compared against a 6-work one — 8 against 20 before, 4 against 4 after. A test asserting a constant would have to be edited by the very change that breaks it. **What remains: nothing enforces that a later query goes through those two wrappers.** One written straight onto `conn` is uncounted rather than caught, which weakens the check silently; the wrappers' own doc comment is the only thing that says so | `taiyaku-library/src/browse.rs::works_of`, `::editions_by_work`, `::missing_by_edition`, `::unfiled` | the C3 metadata-editing whole-branch review, 2026-08-28; closed 2026-08-29 |
| O86 | [x] | **Closed 2026-08-30 by deciding the permission model rather than by handling the case: an administrator is unrestricted, so there is no series one sees only half of.** `patch_series` makes two writes with different scopes — `librarydoc::set_series_titles` writes `series_title` for the **whole** series, because that table is keyed `(series_id, title)` and has no per-work column a grant could filter on, while the per-work loop re-saves `.library.json` only for the works `works_in_series` returns, which is `principal.roots`-filtered (I8). A restricted administrator therefore renamed a series for the entire library and left every work outside their grant holding a stale cached copy, permanently: not corrupt, the row data being untouched, but a later rebuild re-applies it through `derive::write_work`'s read-back into `apply_series`' resolve-by-title. **The entry offered two ways out and one of them was not available.** "Leave `series_title` partially updated" cannot be done — there is no per-work dimension in that table to leave partial. That left refusing a straddling edit, which buys the coherence of the permission model with a rule every future shared-row edit would have to remember. **So the model was fixed instead.** `auth::Principal::from_account` now drops an administrator's `roots` — and forces `can_download`/`can_stream` true on the same rule: **role dominates the other three permission columns.** Spec A2 lists all four side by side and never says whether they compose; they do not, and this is the one place that decides it. Done where the principal is built rather than by refusing the CLI grant, because all three resolvers pass through it and a CLI refusal would leave an existing row live — including on an account promoted to admin after being granted. The row is kept, not deleted, so a demotion restores it, and `taiyaku library user grant-root` now says the grant is inert while the account is an admin. **Measured before changing anything: this library holds two accounts, both with zero grants**, so no live behaviour changed; the shape was reachable only by a CLI configuration nobody had made. **The suite had eleven fixtures granting a root or withholding a capability from the seeded *admin*** — each asserting a restriction that, under this rule, cannot fail. They now route through `test_support::grant_root`, which demotes first, so the rule lives in one place. Three driven proofs were reclassified rather than repaired: the four admin-only metadata routes moved from `Refuses` back to `GrantIndependent` and their blocks now assert the role gate, and I8's positive control was re-pointed at an admin caller so the pair still reads non-admin → 403, admin → 200 against identical material instead of collapsing into one assertion made twice. **The rule also cost a guard its coverage, which a sweep found and a test restored.** `metadata`'s four resolvers still take `roots` and filter on it — correct for any caller holding a grant, and the difference between a leak and none if the role rule is ever revisited — but no route can now hand them a non-empty one, so `root_grant_clause -> String::new()` **survived**: deleting the whole grant filter passed every test. `the_grant_clause_still_filters_even_though_no_route_can_supply_a_grant` asserts the contract at the function, with a slice no route supplies, and fails against that mutation. The functions were never *unreachable* — they run on every edit; it is the clause's restrictive effect that became untestable through a route, and saying "unreachable" was this entry's own first wording | `auth::Principal::from_account`, `taiyaku-cli`'s `user::admin_note`, `test_support::grant_root` | the delta review of the fix waves, 2026-08-29; closed 2026-08-30 |
| O87 | [ ] | **The mutation harness under-reports cross-crate coverage, so a survivor list cannot be read at face value.** `cargo-mutants`' `--test-workspace` defaults to `false`, which runs only the tests in the *package the mutant is in*. `taiyaku-library` is the store and a great deal of its behaviour is asserted through `taiyaku-serve`'s routes, so a mutation whose only assertions live across the crate boundary is reported MISSED while being thoroughly caught. Measured 2026-08-29: `librarydoc.rs`'s `apply_with_series_id -> Ok(())` was reported MISSED by `scripts/mutants.sh --since main`, and hand-applying it in place fails **five** `taiyaku-serve` tests; the timings say it plainly, 26 s of tests at baseline against 7 s for the mutant. **The error runs one way only** — a mutation caught solely by another package reads as MISSED, and nothing uncaught ever reads as caught — so a zero-survivor run stays trustworthy and a survivor list does not, which is the half that matters because the survivor list is the output the harness exists to produce. Some of the 92 untriaged entries in `docs/mutants-baseline.md` may be false for this reason and nobody has separated them. **The fix is one line** — `test_workspace = true` in `.cargo/mutants.toml` — **and its cost is the open question**: every mutant would then run the whole workspace's suite rather than one package's, against a workspace sweep already measured at 3 h 12 m, on a forge whose runner has a hard ceiling near 180 minutes. Sharding, or `--test-package` naming just the store-and-serve pair, are the middle options nobody has costed | `.cargo/mutants.toml`, `scripts/mutants.sh`, `docs/mutants-baseline.md`'s run log | spec C3 PR 2's post-review sweep, 2026-08-29 |
| O88 | [ ] | **A function whose whole-body mutant cannot be compiled has no mutation coverage, and the run reports that as `unviable` rather than as a gap.** `cargo-mutants` replaces a function body with a literal it generates; if that literal does not compile, the mutant is never run — and **an unviable mutant is not a survivor**, so it is invisible in the one output the harness exists to produce. **Cause 1, the fully-qualified path — closed 2026-08-29, and its scope was overstated here by more than an order of magnitude.** The generated literal uses the *short* type name (`Ok(HashMap::new())`), so a function **returning** `std::collections::HashMap` loses every whole-body mutant. This entry first said **"37 files"**, which counted files that *mention* `std::collections::` — the wrong measurement, since only a **return type** can produce the uncompilable mutant. Measured across the workspace: **two functions**, `opds::visible_editions_by_work` and `routes::visible_editions_or_refuse`. Two `use` lines fixed both; their twelve whole-body mutants are now viable and **all twelve are caught**, and no function anywhere still returns a fully-qualified collection. The same measurement on `taiyaku-library/src/proposals.rs` — this entry's own example, at 42 unviable — found **zero** from this cause despite it writing `std::collections::BTreeSet` sixteen times, every one of them a local or a parameter. **What remains is cause 2, which is the common one.** Of `proposals.rs`' 42 unviable, **18** are `Ok(Default::default())` and its variants against types deriving no `Default`, and **24** are `cargo-mutants` guessing a constructor for a custom type (`Ok(Offer::new(…))`). `browse::home` and `browse::search` are hidden the same way. The baseline's 2026-08-21 run log already recorded this shape — *"refused at compile time — `Match` and `SearchResults` derive no `Default`"* — and read it as benign; it is the same blindness. **The fix is a `derive(Default)` on a public type made for a test tool's benefit, which is a trade this file cannot make on its own**, and the `Offer::new` third of it is not fixable here at all. Nothing has separated, across the workspace, how many functions are hidden by cause 2 — that count is the next measurement, and it is the one that says whether this is worth a derive | `crates/taiyaku-library/src/browse.rs`'s `Home` and `SearchResults`; `docs/mutants-baseline.md`'s run log | O85's own mutation sweep, 2026-08-29; cause 1 closed and the scope corrected by O88's own spike the same day |
| O89 | [x] | **Two releases of one volume merge into one edition when only the directory, or a suffix after the volume marker, tells them apart.** Derivation resolves a series from the text *before* the volume marker, so `[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz` and `[Oshi no Ko]/[Oshi no Ko] - v01 (2023).cbz` resolve to one series, group on one `(series, volume)`, and are written as two members of one edition — a reader opening volume 1 gets the fan translation followed by the official release as a single page sequence. `Monster - The Perfect Edition - v01.cbz` survives as its own series because its distinction sits before the marker, which is also why `proposals::raise` can then offer it as an `edition-of`: the curation machinery works on the cases that reach it, and these never do. **Measured over a real library on 2026-09-01** (`docs/notes.md`) — 128 series, 1915 archives — at **25 works across three series**: `[Oshi no Ko]` 12, `Death Note` 9, `Yu Yu Hakusho` 4. The outcome is counted and not attributed: `DeriveReport::merged` is the field built for this exact case and it reported 25, but nothing records *which* works, or that their members came from different directories — which is the half a person needs to act on it. Whether the fix is a rule that reads the directory, an edition key derived from it, or only attributing the count so the proposals inbox can offer the split, is a spec B decision this entry does not take — *closed 2026-09-16, and by none of the three: rule 1 already reads the directory, and rule 4, the next rule to set a series, overwrote it, so `rules::Engine::resolve` now restores a directory's value when a **captured** series discarded a qualifier the directory carried (a literal `set_series` is never reconciled). Re-derived over the real library against figures written down in advance: editions holding more than one member 25 → 0, works 1870 → 1895, and exactly the seven predicted directories changed series (61 members). Each of 18 hand mutations fails a test. It reverses O95's rationale for such a directory, by decision — see O95. `Death Note (Colored Council)` is now correct and unproposed, which is O126; the renames left proposals on series holding no work, which is O127; and a user series rule placed between a directory rule and a later capture, over a qualifying directory, makes the Overrides tab credit a value to a rule that never produced it, which is O128. `docs/notes.md`, "O89", and the design `docs/superpowers/specs/2026-09-15-o89-directory-qualified-series-design.md`* | `derive::classify`, `derive::Group`, `rules::Engine::resolve`, `DeriveReport::merged`; `rules::qualifies`, `rules::captured_series` and the reconciliation closing `Engine::resolve`; `rules/tests.rs`' `a_directory_that_qualifies_the_captured_series_keeps_its_qualifier` and ten beside it, `derive/tests.rs`' `two_releases_told_apart_only_by_their_directories_are_two_works`, `preview/tests.rs`' `a_directory_that_qualifies_the_filenames_series_is_not_an_override` | a real library, 2026-09-01 |
| O90 | [x] | **Closed 2026-09-17: taiyaku still reads no RAR, because it is GPL-3.0-or-later and RARLAB's UnRAR source carries a restriction the GPL forbids adding, so the twelve volumes were repacked to zip and the scan now says why each entry it skipped was skipped.** **A RAR archive named `.cbz` is counted unreadable and skipped.** `taiyaku_read::sniff` answers what a container is from its first 512 bytes rather than from its extension, and gets this right — `scan_entry.format` records `rar`, which is the half of *a zip named `.cbr` and a RAR named `.cbz` are both routine* that already works. `derive::is_readable` then admits `Dir` and `Zip` and nothing else, so the entry is counted in `DeriveReport::unreadable` and never becomes a member. **Measured over a real library on 2026-09-01** (`docs/notes.md`): 12 volumes of one series, invisible. The count is honest — the scan says so rather than dropping them in silence — but honest about a whole series a reader cannot open *(not a whole series: its other 25 volumes read throughout, found while closing it)*. `Format` already names `rar`, `7z`, `tar`, `pdf` and both `epub` shapes, so the sniffer describes more containers than the reader admits, and nothing anywhere records that gap as deliberate. Note the field has a second site: the thirteenth `unreadable` in that run was a member whose bytes could not be hashed, not a format that cannot be read — one counter, two causes *(wrong twice, found while closing it and left as written: `DeriveReport`'s own doc already listed four causes, and the thirteenth entry was `Bisque Doll de Chu ♡.cbz`, a zip that opens cleanly and holds 185 JPEG XL pages, so the walk could not count its pages and `classify` counted it before hashing anything)*. **The twelve were repacked to zip on 2026-09-15** with `bsdtar`, each verified entry by entry before it replaced its original: `unreadable` went from 13 to 1, works from 1858 to 1870, and Vagabond from 25 works to 37 (`docs/notes.md`, "O90"). **No decoder, and the record of why is the half of this that is not a counter.** `unrar` builds UnRAR with Roshal's restriction, `unrar-rs` declares GPL-3.0 while incorporating the same source, and the clean-room `rar` crate is too young for the crate every page read passes through; `Format::is_readable`'s doc now says the seven variants it refuses, six containers and `Unknown`, are a decision, and the design's §1 names what would reopen RAR: a library where repacking is not available. **`DeriveReport::unreadable` is now a `derive::Unreadable`**, one field per cause: `by_format` counts refused formats per `scan_entry.format` token (`unknown` for a NULL format); `uncounted` names each container whose pages the walk could not count, by its full path; `vanished` names each member gone before it could be hashed; and `no_pages` counts containers holding no page images of their own, a volume directory of archives being the usual one. `total()` and `refused_format()` are methods, so the sums cannot disagree with the breakdown. **Paths for two causes and counts for two, by the person's decision**: a file someone has to find is named, and a whole format or a layout is counted. The CLI's scan line, `config_api::derive_report_json` (a scan, a rule apply and a root removal alike) and the Settings scan report each carry every cause. **Tested** by four fixtures asserting the whole `Unreadable` by equality, a RAR named `.cbz`, two formats and a file no signature matched, an archive of `.jxl` pages, the sums, the CLI line clause by clause, the JSON key by key and the screen element by element; twenty hand mutations, each watched red (`docs/notes.md`, "O90"). **What remains**: the walk keeps no reason for a NULL page count, so an archive that would not open and one of pages taiyaku does not read share `uncounted` *(filed as O131)*; and taiyaku reads no JPEG XL page, which is why that archive was unreadable until it was converted to PNG on 2026-09-17 *(filed as O130)* | `derive::Unreadable`, `derive::admit`, `derive::classify`, `taiyaku_read::Format::is_readable`, `config_api::derive_report_json`, `ScanReport.tsx`'s `UnreadableCauses` | a real library, 2026-09-01; closed 2026-09-17 |
| O91 | [ ] | **Reachable from a text box since 2026-09-09, not only by deleting files.** O92's scanner half means an `exclude_globs` pattern puts works into exactly the state below — the real-library run left 242 of them standing that way, deliberately, and clearing the pattern brought them all back. The condition and its absent remedy are unchanged; what changed is how easily a person enters it and at what scale. O92's design §10 proposed "its own item" for pruning them without noticing this row already is it. **A root that shrinks *partially* leaves its vanished works standing forever, and nothing can clean them up.** Spec B §8.1 refuses a scan that finds a root *wholly* empty — an unmounted share, a container started before its bind mount was ready — and applies nothing. A root that merely loses *some* files is not refused, and nothing removes what went: `derive` deletes a work only through `fold::absorb`, which fires when material **moves** to another work, never when its facts simply vanish (`derive.rs`'s own comment called itself "the only `DELETE FROM work` in the project" until spec C4 added a second one). `scan::remove_root` cannot help either, because no root was removed. So deleting files from disk and rescanning leaves the work, its editions, its members and every reading position and note hanging off it in the database permanently, with no command, route or pass that reaches them. **Found while reviewing C4's root removal, which is the code that had to solve the same problem for its own case** and did so by computing the doomed works from `member.root_id` before the root row goes. Generalising that — teaching `derive` to drop any work with no surviving member — is the obvious fix and is deliberately **not** taken here: it would fire on every derive call site rather than on one counted, deliberate act, which is the blast radius B §8.1 exists to guard against, so it wants the same scrutiny B §8.1 got rather than arriving as a side effect | `derive::derive_with`, `scan::apply_removal`, spec B §8.1 | spec C4 PR 1's Task 9 review, 2026-09-03 |
| O92 | [x] | **`root.exclude_globs` is settable, patchable and never read.** The column ships in migration 1, `scan::Root` carries it, `scan::update_root` patches it and C4's roots screen edits it — and `scan::scan_root` never consults it, so nothing is ever excluded from a scan. C4 §3 lists "directories skipped by `exclude_globs`" among the four things a scan report must confess, and asserts that every item of that discipline "is already a field"; that one is not, and cannot be, because nothing skips anything. The discipline is covered three ways of four. **This is the same shape as O80's `_hand` columns and as `last_scan_at`/`last_scan_status`** — a column written into migration 1 against a feature nobody built, discovered by the first code that went looking for it. The other two are now closed; this is the third and last of them. **The screen this row anticipated now exists**, and the anticipation was half right: `RootRow.tsx` draws the field, labels it *"recorded, not yet applied — O92"* and patches it — but `GET /api/library/roots` does not send the column, so the box is **write-only** and starts empty whatever the row holds. **`scan::Root` does not carry `exclude_globs` at all** — only `scan::RootPatch` does — so sending it is a change to the store's own type and its query, not merely a key in `root_json`, which is what this entry assumed before anyone looked. Until that lands the editor treats the column as *unknown* rather than as empty, which is what lets a clear reach the server at all: three passes of C4 PR A's whole-branch review were needed to get that right, the first two each shipping a fix that worked only within a single mount. Two gaps now rather than one. **The second of those two closed on 2026-09-08, and this row is back to the one gap it opened with.** `scan::Root` carries `exclude_globs`, `scan::roots` selects it, `config_api::root_json` sends it and `RootRow.tsx` paints the box from it — so the field is no longer write-only and an admin can see the pattern they set. Three claims above are superseded by that and are left as written: the headline's *"never read"*, *"`scan::Root` does not carry `exclude_globs` at all"*, and *"the box is **write-only**"*. **What that fix took out of `RootRow.tsx` is worth the sentence**: `TextField.from` had an `undefined` arm meaning "this row cannot describe this column", and it existed for this column alone — with the list route answering, the blur guard compares `exclude_globs` against the row exactly as it does the other three, and the arm is gone. The rule the three review passes converged on is untouched: **a write happens because somebody edited a box, never because its value differs from something.** Two of that file's tests were restaged rather than deleted — each had to type a value in before it could clear one, because the box could not show what was stored, and each now stages a root that *holds* a pattern. **What remains is the gap this row opened with and the only one that was ever a decision**: `scan::scan_root` still never consults the column, so nothing is excluded from a walk, and C4 §3's fourth confession is still uncoverable. Making it work is a scanner change with its own blast radius, exactly as the C4 design says. **Verified against a copy of the real library on 2026-09-08** — 1,858 works, 2,044 scan entries — where the box loaded holding `**/extras/**` for the first time, emptying it really cleared the column, and neither a retype back to the stored value nor a bare focus-and-leave sent anything; `docs/notes.md` has the run and the isolation proof. **Closed 2026-09-09.** `scan_root_inner` compiles the column once per scan into an `Excluder` — gitignore's language taken whole from the `ignore` crate rather than a matcher of this module's own, so negation, anchoring, directory-only patterns, comments and blank lines all arrive with it — and `measure_dir` consults it in its entry loop, **before `entry_count` and before the page-image/file split**, so a matching directory is never `read_dir`'d at all and an excluded file arriving is not a change `dir_state` reports. Four fields carry the result out through `scan_report_json`, the CLI summary and `ScanReport.tsx`: `excluded_dirs` — **C4 §3's fourth confession, finally a field** — plus `excluded_files`, `bad_exclude_patterns`, and `forgotten_by_exclusion`, the last split out of `gone` because *you excluded it* and *it disappeared* are different facts. `PATCH` refuses a line that will not compile; a stored line that will not compile disables itself and is named in the report, `rules.rs`'s rule. The box is a `<textarea>` now and its label no longer apologises. **A fourth claim above is superseded, and it is the one that kept this half deferred**: the blast radius. This row and the C4 design both reasoned that an exclusion would make `forget_unseen` drop rows, derive produce fewer works, and progress and notes go with them. Three links of that chain do not exist — nothing references `scan_entry` (`schema/004_auth.sql` says so in as many words), `derive`'s only `DELETE FROM work` is a fold that needs the material to have a home elsewhere, and `import::scanned` deletes only the editions a scan named. An exclusion leaves the works it covers **standing and stale**, and destroys nothing. **Measured against 1,858 real works on 2026-09-09**: `One Piece*` matched five directories, 266 entries were forgotten, `gone` stayed 0, and works, editions and members were unmoved at 1,858/1,858/1,883 — all 266 restored by clearing the column. **The saving is proportional, not multiplicative**: 12.3% of wall time for 13.0% of entries skipped, once the page cache was controlled for. The 23× a first uncached walk suggested was cache fill and must not be quoted. Skipping at descent avoids a whole subtree, but this root is flat and has none to avoid, so the feature's value is a property of the tree's shape rather than of the feature. **What the run could not test**: that copy holds no `progress` or `note` rows, so their survival is argued from `ON DELETE CASCADE` hanging them off a `work` row that was never deleted, rather than observed | `scan::Excluder`, `scan::measure_dir`, `scan::forget_unseen`, `root.exclude_globs`, C4 §3 | spec C4 PR 1's Task 12 review, 2026-09-03; narrowed to its scanner half 2026-09-08; closed 2026-09-09 |
| O93 | [x] | **Closed 2026-09-04, and the fix is one function rather than a check added in twenty-two places.** `config_api`, `curation` and `metadata` each defined their own `admin_or_forbidden` reading `principal.role` and nothing else, and `users_api`/`backup_api` copied one — **three copies of a half-check, which is how the missing half went unnoticed through four whole-branch reviews**. All five are gone; `session::administering_or_forbidden` is the only function in the crate that answers "may this caller administer", and it asks both questions. **Measured before it closed: `GET /api/library/users` under an administrator's `Authorization: Basic name:tyk_…` answered 200 with every account's sessions, IP addresses and user-agents**, and the same key reached `DELETE /api/library/roots/{id}`, the action C4 §2 calls the most destructive in the product. **The suite could not have caught it**: `test_support::authenticating` sent a key, so every route test in `taiyaku-serve` drove the credential these routes should have been refusing — adding the gate turned 164 tests red. That default is inverted now: `authenticating` opens a real browser session and `key_authenticating` is what OPDS's tests ask for by name, since a key is the credential OPDS actually has. Two standing checks keep it closed, and the second is derived rather than listed: `no_module_answers_the_role_question_alone` greps for a fourth copy of the half-answer, and `exactly_the_declared_routes_refuse_an_api_key` drives **every** route in `guarded_routes` with a key and asserts set-equality between the ones that refuse it and the ones declared to — so a new route is classified by what it does, not by whether anyone remembered a list. A hand-typed list was the first version and repeated O93's own shape. **O58's two routes are not an exception**: `put_truth` and `patch_alignments` take no `Principal` at all and are not administrative — what credential may rewrite a *shared document* is a different, still open question | `session.rs::administering_or_forbidden`, `auth.rs::By::may_administer`, `test_support.rs::authenticating` | spec C4 PR 2's review of its own fix round |

| O94 | [x] | **Closed 2026-09-14, once the account screen had a route of its own and had been driven to from `/` in a library holding no works.** **The one screen that can change your own password is reachable only from inside the reader.** `Account.tsx` — password change, API keys, sessions — is mounted by `App.tsx`, which `Routes.tsx` mounts on `/read/{work}` alone, so there is no URL for it and no way to it from any library screen. That was survivable while it was the *only* account surface. It stops being survivable with spec C4's user administration: `PATCH /api/library/users/{id}` deliberately **refuses** an administrator resetting their own password and names `/api/password` as the route that gets it right (#106's own whole-branch review), so C4 §4's screen has to send a person somewhere — and the somewhere is a panel they can only reach by opening a work and pressing a key. An administrator of a library with no readable work cannot reach it at all. **Found while designing the users screen, before it was built** — PR C is what makes the refusal visible to a person, and the fix is a route rather than anything in C4: `Account` is a screen that has been living inside the reader since spec A2, and moving it under `Shell` alongside `/config` is the same shape C1 already applied to the library screens. *The screen this row anticipated shipped on 2026-09-06: C4 PR C's `UserRow` is where an administrator meets the refusal, and it sends them to the panel in the only words available — "open any work and press A". The row stays open and its prose above stays as written, per the ritual; this sentence marks that the item now has a caller rather than only a design.* **Still present on `main` (`6a2e72d`)**: `Routes.tsx` routed `/`, `/series/:id`, `/search`, `/proposals`, `/config/*`, `/read/:work` and `*`; `<Account>` was rendered by `App.tsx` alone and opened by `A`; `UserRow`'s refusal read "open any work and press A"; and `users_api::patch_user`'s refusal names `/api/password`. **Fixed by a route**: `/account` mounts `Account` inside `Shell`, and the sidebar draws an Account link after Configuration with no `admin` guard, since `Account`'s own doc comment says no route it calls names another account. `Account` takes a `placement`: `'panel'`, the default, is the reader's and is unchanged; `'page'` draws in `<main>` with no overlay, no Close and none of the panel's fixed dark palette, which on a light canvas would draw the "Password changed" notice pale green on white, and `index.css`'s `.account` block draws it in system colours. `UserRow`'s refusal links to `/account`. **The reader's panel is kept**: no spec asks for its removal, and `A` still opens it. No spec names the URL or the placement, so `/account`, linked beside `/config`, was chosen. **Seven tests, each watched red before the fix** (`docs/notes.md`, "O94"): in `Routes.test.tsx`, the route in a library with no works and signing out from it; in `Sidebar.test.tsx`, the link for a reader and for an admin; in `Shell.test.tsx`, the link through the shell for a signed-in reader; in `UserRow.test.tsx`, the refusal's link, asserted as an element; and in `Account.test.tsx`, the page placement. Applied by hand to the final code, deleting the route turned both route tests red, deleting the link turned the three link tests red, gating it on `admin` turned the reader and shell tests red, putting `UserRow`'s sentence back turned its test red, and giving the page the panel's palette turned the page test red; none survived. **Driven in a browser** reporting Chrome 153, against `vite` in development and a scratch library holding no works: signed in at `/`, which read "No library yet", followed the sidebar's Account link to `/account`, changed the password there and signed out from it; the old password was then refused with a 401 and the new one signed in. With one volume hardlinked in and scanned, `A` in the reader still opened the panel and `Escape` closed it. **Not tried**: a non-admin account in the browser, where only jsdom has seen the link; the page in a dark colour scheme; a touch device | `Account.tsx`, `App.tsx`'s account panel, `Routes.tsx`, `users_api::patch_user`, `Sidebar.tsx`, `UserRow.tsx` | spec C4's screens, designing PR C, 2026-09-04; closed 2026-09-14 |
| O95 | [x] | **Closed 2026-09-05 by reframing the row rather than by changing the detection, and the tab is now called Overrides.** The defect: on a library holding nothing but the four rules migration 3 seeds, the Conflicts tab opened reading **1 conflict, 178 examples**, and the conflict was real — rule 1 reads the series from the directory (`match_on: path`, `^(?<series>[^/]+)/` → `Death Note (Colored Council)`) while rule 4 reads it from the filename (`match_on: name`, `^(?<series>.+?)[ _-]+v` → `Death Note`), and rule 4 wins on ordinal. **That override is not a mistake — it is what folds a colour edition into its main series**, and rule 4's own seeded note says so: *"series from the filename where it repeats it."* **Measured on the real library: 27 of 128 directories, 21%, carry a filename stem differing from their directory** — `Chainsaw Man - Digital Colored Comics`, `Bocchi the Rock! - Anthology Comic`, `GTO - Great Teacher Onizuka - 14 Days in Shonan` — so this is structural rather than one library's quirk, and any library naming directories with a group or edition suffix will show it. **The defect was not the badge; it was the remedy.** The row's sole action read *"Move rule 4 above rule 1"*, and pressing it made rule 4 come first, so rule 1 won as the later rule, the series became `Death Note (Colored Council)`, and **the colour edition stopped folding** — the screen's one offered fix was the one thing that broke it. Spec C4 §5.4 anticipated the badge and narrowed the definition to value-*differences* precisely "because taken literally it flags the shipped seeded defaults"; the narrowing did not reach this, because producing a *different*, normalised value is exactly what the pairing is for. **What shipped is framing, not detection.** `Conflict` is `Override` in the store, `conflicts` is `overrides` on the wire and the tab reads **Overrides** — one word end to end, rather than a screen and the route it calls using two. The row states *"series — rule 4 overrides rule 1 on 178 files"*, names the file it is about, and draws **the two values it happened between**, which the wire did not carry before: naming the two rules says *that* they disagreed and never what about, and what about is the whole of the judgement the tab now asks for. Under them sit **two options rather than one fix** — reorder, and *Edit rule 4*, which puts the winning rule in the form, because narrowing means editing a regex and no button can guess how. **Deliberately not fixed by suppression**: no exemption can be written honestly, because "these two rules are a deliberate layering" is intent and intent is not a column, and special-casing the seeded ids goes stale the moment somebody edits one. Detection is unchanged. **The executed fixture is `library_with_colour_edition_directories`** — two directories carrying an edition suffix their filenames do not, which is both the real shape and the only arrangement that can tell a row carrying its own example's values apart from one carrying whichever entry was walked last. **A second defect was found closing this one, and closed with it**: `field_value`'s `Volume` and `Chapter` arms could be exchanged with **every test in the workspace still passing** — a chain compared on the wrong field silently drops real disagreements, and since this change the row *draws* the value too. `the_numeric_fields_report_their_own_values_and_not_each_others` pins both. The mutation sweep could not see it — `FieldValue` derives no `Default`, so the function's only mutant is the unviable whole-body one, which is O88's second cause reported as `unviable` rather than as a gap. **Verified against the real library on 2026-09-05**, which is how the quoted row above is known to be the row: it reads *"series — rule 4 overrides rule 1 on 178 files"* over `Death Note (Colored Council)/Death Note - v01 (Colored Council).cbz`, with `^(?<series>[^/]+)/ → Death Note (Colored Council)` above `^(?<series>.+?)[ _-]+v → Death Note (wins)`, and *Edit rule 4* opens rule 4 in the form. **An earlier draft of this entry said `edition_key`**, carried over from C4 §1's older worked example — migration 3 seeds no rule touching `edition_key` at all, so the row it described could not exist. Caught by this branch's whole-branch review. **Reopens on**: anyone reporting they have learned to ignore the badge. The framing *is* the fix, so that is the one report that would say it did not work *(Reversed for one shape by O89 on 2026-09-16, by decision. A directory that qualifies its filenames' series, `Death Note (Colored Council)` over `Death Note - v01 …`, is its own series now, so the fold this entry defends no longer happens there, and the Overrides tab no longer reports it (O128 is the one known exception). 117 of the 178 files still override, and O95's two tests moved to the GTO directories, which are among them. See O89's entry. The prose above stays as written, per the ritual.)* | `preview::overrides`, `preview::Override::first_value`, `PreviewTabs.tsx`'s override row and its two options, migration 3's seeded rules 1 and 4 | driving C4 PR B's rule editor against the real library, 2026-09-05; closed the same day |
| O96 | [x] | **Closed 2026-09-06 by freezing the list and holding the form's commit, which are one decision and not two.** The defect: `RuleEditor::save` set `saving` and never `busy`, and its `.then` ended with `setRules(answer.rules)`, so every gesture committed during the **280.7 ms** a save costs on the real library (`docs/notes.md`) was applied, previewed, and overwritten by the list the server sent back, with nothing on screen saying so. **What shipped:** `RuleList` takes a `saving` prop and disables its five controls — the enable toggle, the select, the delete, the confirm dialog's *Delete and reopen*, and the add — and stops the drag with **two** guards, because `draggable={!saving}` cannot cancel a drag already in progress when the save began, and flipping the attribute under a live one does nothing to the drop that ends it. **On `saving` and never on `busy`:** a preview in flight is superseded by `runPreview`'s sequence guard and is never lost, so a gesture during one is correct and cheap; gating on `busy` would grey the list for ~281 ms after every gesture, which is the rhythm the build design's §5.3 costed out and kept. **`RuleForm` is deliberately not frozen, and the mechanical guard is worse than it looks**: `disabled` on a *focused* input makes the browser fire `blur`, and this form commits on blur — so disabling it would itself commit the half-typed edit into the very window this item was about. Instead the fields stay live and the *commit* is held, then applied on top of `answer.rules` taking that row's `id` and `ordinal` from the server. **The freeze is what makes that merge exact**: `answer.rules` is positionally aligned to the list that was sent, `rules::replace_all` minting an id only for a zero one, and a list that moved under the request has no correspondence to it at all — which is also why the third option, freezing nothing and reconciling afterwards, was refused: it needs a client-side row key `RuleList`'s own identity model does not have. Carrying the edit's `id: 0` forward instead would mint a **second** row on the next save and orphan `created_rule_id` on any accepted proposal that wrote the first, reopening it for nothing; the executed fixture returns a minted `id: 7` against a sent `id: 0` precisely so a test cannot pass against that defect, and hand-applying it fails that test and no other. **A second defect in the same window closed with it, structurally rather than by a fix**: `save`'s `.then` called `runPreview(answer.rules, selected)` with the `selected` its own closure had captured, so selecting a rule mid-save left the preview's `rows` scoped to the rule that *was* open while the form showed the new one — a sharper symptom than the discarded gesture, because nothing about it looks wrong. With `RuleList` frozen and `PreviewTabs`' two options `disabled={busy \|\| saving}` since O95, no surface can change `selected` during a save; a `selectedRef` was refused because it would hide that invariant rather than state it. **A failed save keeps the edit too** — it wrote nothing, so the list is still the one that was sent and there is no minted id to adopt — **and its reason goes to the notice slot rather than to `failure`**, because applying the edit runs a preview whose success path calls `setFailure(undefined)`, and the reason a person most needs to read here is the 409 `save_rules` answers while a scan holds `library_job`. That is the third instance in one file of a notice needing a slot `runPreview` does not touch, after `importNotice` and `saveMismatch`. A failed save with nothing held is unchanged, because nothing follows it to wipe it. **Six tests, each hand-applied against its own defect and each failing alone** — the asymmetry between the `draggable` guard and the `onDrop` guard is what proves the two are not one test twice. **One gap the tests could not see and `tsc` could**: adding a required prop left two mount sites in `RuleList.test.tsx` uncompilable, and all 21 tests in the file passed against a call that would not build — vitest does not typecheck. **Verified against a copy of the real library on 2026-09-06** — 1858 works, 2044 scan entries, the four seeded rules, the root repointed to an empty scratch directory first because a save re-derives and writes `<root>/.taiyaku/`. With the PUT held open at the transport so the 280.7 ms window could be inspected: every list gesture disabled and the row `draggable="false"` while *Saving…* stood, a drag begun before the save and dropped during it reordered nothing, and the pattern field stayed live and writable. A blur inside the window on rule 4 survived a save that re-homed **178 works and merged 3 series**, the notice read *"Your edit to rule 4 arrived while the save was running… it is applied to the list on screen and is unsaved"*, and the `Saved:` line was still standing beside it after the follow-up preview had landed — which is the `failure`-slot trap, disproved on real data rather than only in a fixture. On an injected 409 carrying `save_rules`' own sentence, the edit survived and the notice carried both facts with nothing in `failure`. Isolation proven afterwards rather than assumed: the real `library.db`'s mtime unchanged, nothing under `temp/manga-test` newer than the start stamp, no `.taiyaku` written into the real tree. **The drive predates the whole-branch review below, so the Import guard and the notice-clearing it added are covered by tests and not by a run against real material.** *A note on the drive, not on the change*: the failure notice embeds the raw body — `409 {"error":"…"}` — which is `api.ts`'s existing `message(err)` formatting, identical to what `failure` rendered before this change and not introduced by it. **A whole-branch review of this branch found five more, and the worst of them was the same defect on the one surface nobody had enumerated — Import.** `RuleEditor`'s Import button and its hidden file input carried no `disabled` at all (`disabled` appeared exactly once in the whole JSX, on Save), and `doImport` commits like every other gesture. Import is worse than the rest because `POST .../rules/import` **writes the rule table** and reopens accepted proposals: an import landing inside the save window left the *store* holding the imported rules, the *screen* showing the pre-import list `setRules` put back over them, and `importNotice` telling the person the import had worked — and the next Save would write the screen's list back, reverting the import and reopening those proposals a second time. Every other discarded gesture costs a click; this one outlives the session. **It is refused rather than held, and that is forced rather than chosen**: by the time a held import could be applied the write has already happened, so the guard has to sit before the request. Two guards again, for the drag's reason — the button is disabled, *and* `doImport` checks, because a file picker opened before the save started is still open and still delivers. **The second finding was a comment asserting a guard that did not exist**: `held`'s own doc block said the freeze left "the form the one surface that can still commit", which was false while Import was live — the exact shape O95's third pass kept finding, reproduced here on the branch that cites it. Three minor: this entry said "five gestures" and listed six; the confirm dialog's *Delete and reopen* was disabled but untested, the one frozen control the test helper cannot reach and the one that reopens a proposal; and `heldNotice` named a rule by position and claimed it was the list on screen, with nothing clearing it on a later mutation — `commit` now clears it, which is `RuleList`'s own M3 for its confirm dialog, for the same reason, and the failed-save path sets it *after* calling `commit` so its own message survives the clear. **A sixth was found in the review's own fix round**, which is why the fixes were reviewed too: the first version of the Import-disabled test chained `.then()` off `act()`, whose thenable swallows the rejection — the test printed its own `AssertionError` and vitest still reported it passing. **What remains: nothing enforces that a future surface setting `selected` carries a guard.** The invariant is stated in `RuleEditor.tsx`'s doc comment and asserted nowhere. *Its `saving` prop was renamed `frozen` by O97 on 2026-09-08, when Import began taking the same gate; the prose above is left in the present tense of when this closed, per the ritual, and only the site column is updated.* | `RuleEditor.tsx::save`'s held commit, `RuleList.tsx`'s freeze prop (`saving` when this closed, renamed `frozen` by O97 when an import began taking the same gate), `RuleForm.tsx` (unchanged, deliberately) | O95's third whole-branch review, 2026-09-06; closed the same day |
| O97 | [x] | **Closed 2026-09-08 by making the two writers one gate, and it closed two losses in the same window that never had rows of their own.** The defect: `doImport` set no flag, so Save's `saving \|\| busy \|\| preview === undefined` did not see an import in flight — the save sent the *pre-import* list, the import landed and `commit`ed the imported one, then the save landed and `setRules` put the pre-import list back over it, with `importNotice` still saying the import worked and the next Save reverting it and reopening every proposal a second time. **What shipped: one three-valued `writer`** (`'save' \| 'import' \| undefined`) in a ref for the synchronous gate and mirrored into `writing` for the render, replacing the `savingNow`/`saving` pair. Two booleans would admit `saving && importing` — a state that must never exist and that four conditions would then have to agree about — and O96 is what agreeing by hand cost: it guarded an import during a save and left the save during an import, which is this item. Both writers open with the same line, so the refusal is one condition rather than four. **The two unnumbered losses close with it rather than opening rows**, being the same defect and the same fix: every *other* gesture was discarded too, because `doImport`'s success path calls `commit(answer.rules, undefined)` and replaces the list wholesale — so the toggle, select, delete, drag, add and the form's commit-on-blur were each applied during the import and then wiped; and **Import was enabled during an import** (`disabled={saving}`), so a second pick raced two writes to the rule table. `RuleList`'s and `PreviewTabs`' `saving` prop is renamed `frozen` and passed `writing !== undefined` — renamed rather than passed a lie, since a prop named `saving` that is true during an import is exactly the staleness this table exists to avoid. **Two more defects were found after the design, both the same shape — a gate taken and never released — and neither is in it.** The first, found while implementing: `doImport`'s `.catch` did not clear the gate, so a *failed* import disabled Save and Import for the rest of the session. The second, found by the whole-branch review and **not caught by fixing the first**: `importRules` is not `async` and evaluates `JSON.parse(doc)` as an *argument*, so a document that is not JSON throws **synchronously**, before any promise exists for that `.catch` to attach to — the gate is taken, the throw escapes to the file input's own handler, and both writers are dead for the session with only a parse error on screen. Picking a file that is not JSON is the most ordinary way to reach it, and it needs a `try` around the call rather than another `.catch`. That the second survived the first is the argument for reviewing the branch whole and for grepping a finding's *shape* rather than its function: the two are one sentence apart in the same function. Each has its own test. **A refused save, and a form commit discarded by an import, go to a fourth notice slot** (`refused`) rather than to `heldNotice` or `importNotice`: `commit` clears the first and the import's own answer overwrites the second, so a refusal in either disappears at the moment the person needs it. Proved by moving the notice to `heldNotice` and watching *only* the survival test fail. **The form stays live and its commit splits by writer** — held for a save, as O96 shipped it; discarded for an import, which is forced rather than chosen, because an import mints fresh ids and replaces every rule so no index carries the edit across. **Nine tests, each hand-applied against its own defect and watched:** removing the ref guard takes down O96's own second-save test as well as this item's, proving it load-bearing; swapping the two dispositions fails six; freezing `PreviewTabs` on `saving` fails the freeze test, which is the only coverage that component's frozen state has, since `PreviewTabs.test.tsx` mounts only with `frozen={false}`. **Two defects in the plan itself, both found by executing it**: it had Task 1 declare `frozen` a task before its first reader, which `noUnusedLocals` rejects and which an implementer met with a `@ts-expect-error` in shipped code; and its discard test opened a rule *during* the import, which the freeze this branch adds makes impossible. **The window is measured, and it is narrow**: a rule import costs a median **2.3 ms** whole-process against a save's 280.7 ms (`docs/notes.md`, 2026-09-08, ten runs against an isolated copy of the real library) — that figure is the CLI store path, so it excludes the HTTP route's auth, parse and network, and the same figure over HTTP and on the NAS is still owed. Two orders of magnitude is also why this survived from #109 with nobody tripping over it. It changes nothing about the fix: a window that must not admit a second writer is not a question of width. **What remains: nothing enforces that a future writer takes the gate.** A third route that writes the rule table would have to set `writer` itself, and nothing makes it — O96's own remaining gap restated one level up, stated in `RuleEditor.tsx`'s doc comment and asserted nowhere. *Postscript, 2026-09-08 — the two owed figures and the real-library drive were done after this closed, and are in `docs/notes.md` under “O97: the same figure over HTTP and on the NAS, and the drive”. The prose above is left as written, per the ritual, but two of its numbers should not be quoted without it. **The 2.3 ms is a tmpfs figure**: the same import costs **15.2 ms** with the database on ext4 and **6.0 ms** over HTTP, and the difference is one `fsync` — 4.885 ms on this NVMe, 0.000 on tmpfs. **And the NAS is not slower**: an import with the database on the NAS costs **1.7 ms** against the local disk's 6.0 ms, because NFSv4 answers the commit from the server's cache at 0.144 ms — a weaker durability promise rather than a faster disk, and it is `docs/notes.md`'s own sentence about what keeps the window real — “a LAN round trip is 1–5 ms on its own, and the NAS is slower” — that was wrong, in both halves. Neither figure changes the gate, which is §0.2's own argument. **The drive confirmed every one of the build design's Step 4 assertions** against 1858 real works with the POST held open for ninety seconds at a proxy that forwards rather than at a patched `fetch`, the seeded-defaults conflict on 178 files supplying the real override row the freeze was checked against — and found three things, none of them in the gate: `doImport`'s refusal for a **second import** is written to `importNotice`, which that import's own success path overwrites, so the person is told an import worked while their second pick was discarded — the exact failure §5 created the `refused` slot to avoid, in the function that reasoning was written for, and untested because no test delivers a second file during a held import and the case titled “so a second pick cannot race the first” asserts only the disabled button, which is the *first* guard; both `refused` sentences claim “the imported rules are on screen now” while the import is still in flight, one of them pointing at a disabled button, with the tests matching on substrings that exclude the wrong clause; and the gate is taken **after** `file.text()` resolves, so the disabled button does not cover the file-read window, which is how the first of the three is reachable without a synthetic event. The first two are **O100** and the third is **O101**, both opened and closed the same day; O101's fix closed a window that was not its own, and O100's row says which.* | `RuleEditor.tsx`'s `writer` gate, `RuleList`/`PreviewTabs`' `frozen` prop, `RuleEditor.tsx`'s `refused` slot | O96's own whole-branch review, 2026-09-06; closed 2026-09-08 |
| O98 | [x] | **The web coverage gate's file-count floor has gone stale again, which is the exact failure its own comment predicts.** `scripts/coverage-floor.mjs` holds `minFiles: 59`, derived on 2026-08-14 when `web/src` produced **66** files in the report: seven of margin, chosen so that the measured near-miss of the day — an `include` glob that silently dropped fourteen files, leaving 52 — still tripped it. `web/src` now produces **106** (109 source files, three of which the report does not visit), so the floor tolerates **47** files leaving the gate rather than seven, and no exclusion anybody is likely to make would trip it. The comment above the constant says exactly this — *"a fixed `minFiles` against a growing tree loosens on its own, silently, and nothing here derives it — so re-run the three rows above when `web/src` grows by roughly ten files"* — and the tree has grown by forty across C4's three UI PRs without anyone re-running them. **Deliberately not fixed here, and not by scaling the number**: the constant's own instruction is to re-run the experiment (drop the `.tsx` glob, drop a directory, and measure what each costs) and to check *"the near-miss still trips it"* rather than *"the count is still above the floor"* — and a number scaled by 106/66 would be a figure nobody measured, which is the thing this file refuses everywhere else. The per-file floors are untouched by this: they caught two of this branch's own files at 79% and were what made it write the tests. **Closed 2026-09-11 by re-running the experiment, which is what the constant's own comment asks for rather than by scaling the number.** All three rows behave as they did on 2026-08-14, at the counts a larger tree gives: `['nope/**/*.ts']` reports 0 files and vitest still **exits 0**; `['src/*.ts']` reports **94** — all 14 files of `src/modules` gone — with vitest green on all four thresholds (97.25 / 92.43 / 99.11 / 99.18) and `coverage-floor` printing *"94 files (floor requires 59), all above the per-file floor"*, which is the second time this constant has passed its own near-miss; `['src/**/*.ts']` reports 108, confirming for a third measurement that the `.tsx` glob is inert. The clean tree measured **108** rather than the 106 recorded five days earlier, which is the growth rate the staleness comes from. **`minFiles` is now 101, derived rather than scaled**: 108 clean, 94 near-miss, and the fourteen between them split seven and seven — the same split as 2026-08-14, and structural rather than lucky, because the near-miss *is* `src/modules` leaving the gate and that directory held 14 files then and holds 14 now, so the gap has not widened with the tree. Scaling 59 by 108/66 gives 96, which catches 94 with two files of room instead of seven. Proven by regression both ways: at 101 the near-miss exits 1 naming the config to open, and the clean tree exits 0. `docs/ci.md`'s own section is re-derived with it — and its summary table, which read "≥32 files" for the Rust half against the script's own `minFiles: 40`, was stale in the same way and is corrected | `scripts/coverage-floor.mjs`'s `minFiles`, `web/vite.config.ts`'s coverage `include` | C4 PR C's own gate run, 2026-09-06; closed 2026-09-11 |
| O99 | [x] | **Closed 2026-09-06, the day it was opened: a test compared a field the clock fills and no column holds.** `preview::tests::a_save_writes_the_library_tree_a_preview_would_have_left_alone` — #109's test, and the only thing in this crate that proves a save derives under `Documents::Reconcile` rather than `Skip` — snapshotted the library tree, deleted the work's document, applied, and asserted the tree came back byte-identical. `.library.json` is in that tree, and `librarydoc::of` fills its `updated` from `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')` at projection time with nothing in the schema holding it, so the two snapshots agreed only when the fixture's write and the save's fell inside the same wall-clock second. **Found on `main` by CI run 437's push job rather than on any branch** — `21:28:50Z` against `21:28:49Z`, on a tree otherwise identical to the byte — and reported rather than quietly fixed, which is why it has a row at all. **Reproduced deterministically before anything was changed**, by sleeping 1.1 s between the two writes: the failure names one file and one field, every other byte under the root equal. **The fix is a second helper, `tests::tree_snapshot_stamps_normalised`, and deliberately not a change to `tree_snapshot`.** The crate's three other snapshot comparisons — a preview, a `Documents::Skip` derivation, `scan::removal_cost` — each assert that an operation wrote *nothing*, and for those a rewrite that moved only the stamp is precisely the defect they exist to catch; normalising inside the shared helper would have blinded all three to close one flake. `hand::reconcile` already normalises the same field for the same reason before deciding whether a document on disk is stale, and its comment says what comparing it would cost there. **Three checks, because a normalisation is a way of not looking**: the fixed test passes with the 1.1 s sleep still in it; with that sleep in place *and* `Reconcile` hand-swapped for `Skip` inside `preview::apply` it still fails, so the normalisation did not cost it its claim; and `normalising_the_stamp_hides_a_second_and_hides_nothing_else` — which pins the two stamps that run actually produced, refuses a changed title, and passes a non-document file through untouched — fails when the normalisation is removed. That third test exists because **the flake cannot be proved from the test that carried it**: a unit test's two writes land inside the same second almost every time, which is how this survived from #109 to now | `tests::tree_snapshot_stamps_normalised`, `librarydoc::of`'s `updated`, `preview/tests.rs`'s save test | CI run 437's push job on `main`, 2026-09-06; closed the same day |
| O100 | [x] | **Closed 2026-09-08, the day it was opened: a refusal was written to the slot the gesture it refused owns.** `beginImport`'s guard — the second of the two, for a file picker opened before a write started and still delivering — put both of its refusals in `importNotice`. For a save in flight that survived by luck rather than by design, since nothing in `save` writes that state. For an **import** in flight it did not: the running import's success path calls `setImportNotice('The imported rules are stored…')`, so the sentence saying the second pick was refused was replaced, milliseconds later, by one saying an import had worked — and the person was left believing the file they picked second was the one in the store, when the first document was. **This is the exact failure the build design's §5 names as its whole reason for creating `refused`** ("**Not `importNotice`.** … its success path overwrites it … leaving the person told the import worked and never told their Save did nothing"), reached in the function that reasoning was written for. **No test could see it**: none delivered a second file during a held import, and the case titled *"so a second pick cannot race the first"* asserts only that the button is `disabled` — which is the *first* guard. **Found by driving it**, not by reading: with `POST …/rules/import` held open for ninety seconds at a proxy the refusal stood on screen at 21:02:44 and was gone at 21:03:21, replaced by the import's own sentence, against a copy of the real library (`docs/notes.md`, 2026-09-08). At the measured 1.7–6.0 ms an import actually costs, the refusal is on screen for less time than a frame. **What shipped:** both refusals move to `refused`, which `commit` does not clear — moved together rather than only the broken one, because a message about a gesture that did *not* happen cannot live in a slot owned by the gesture that did, and leaving the save case behind would have made that rule unreadable from the code. **Four sentences also described a state that had not happened yet**: two `refused` messages ended *"The imported rules are on screen now"* and two more said *"now the save/first has finished"*, all four set while the write they name is still running — so for the width of the window each was false, and one of them pointed at a Save button `disabled` for exactly as long as it was on screen. Invisible at two milliseconds; the only thing on screen for ninety seconds, which is how they were found. **Five defects hand-applied and watched, each failing one test alone**: making the running import's answer clear `refused` fails the survival test; restoring *"on screen now"* fails the discard test; and restoring each of the two *"now … has finished"* clauses fails one test each — the assertions name the clause that was wrong rather than a substring that skips it, which is what the tests this item replaces did. The fourth sentence, `save`'s own refusal, is not pinned because it is not reachable; O101's row says why. **The fifth defect was in this item's own fix round**, which is the argument for reviewing those too: the first rewording of the discard notice ended *"Its rules replace the list when it lands"* — true when it does, and a promise this screen breaks when the document is refused, which is the commonest ending this route has. It never shipped; it was caught by re-reading the fix and is now pinned by driving the discard and then *failing* the import. **What remains, and it is a decision rather than a defect**: a form commit discarded by an import is discarded even when that import *fails* and the list is therefore unchanged — where O96's failed **save** keeps the held edit for exactly that reason. The design's "discard is forced rather than chosen" holds only for an import that succeeded; nobody considered the other ending. Left as it is because holding across an import is a behaviour change, not a wording one *(filed as O109 on 2026-09-13 — see its entry)* **What remains: `save`'s own refusal message is now unreachable** — see O101, which is why | `RuleEditor.tsx::beginImport`'s guard, the `refused` slot | the real-library drive owed by O97, 2026-09-08; closed the same day |
| O101 | [x] | **Closed 2026-09-08, the day it was opened: the writer gate was taken after the file was read, so the disabled button did not cover the gesture.** The hidden input's `onChange` did `file.text().then(doImport)`, and `writer.current` was set inside `doImport` — so between picking a file and its bytes arriving, Import was still enabled, Save was still enabled, and the gate was still free. On a small document that window is one microtask; on a large one it is however long the read takes, with nothing on screen saying the gesture had started. It is also the route by which O100 is reachable without a synthetic event: pick a large file, pick a small one while the first is still being read, and the small one's import is running when the large one's finally fires. **What shipped:** `doImport(text)` becomes `beginImport(file)` — the whole gesture from the picked file, taking the gate before the read and releasing it in a `.catch` the read's own failure needs, which is a *third* failure shape for a gate this screen has twice leaked. `sendImport(text, release)` is the request half, and the `try` that used to wrap `importRules` is gone rather than kept: the call is now made inside a `.then`, where a synchronous `JSON.parse` throw is a rejected promise, and O97's non-JSON test is what keeps it caught if anyone moves it back out. **The fix closes a window that was not this item's**: `beginImport` takes the gate inside the discrete change event, which React flushes synchronously, so `frozen` and `writer.current` change together — where before there was a microtask in which the ref was taken and the Save button was still drawn enabled. That window is what made `save`'s second guard *reachable*, and the O97 test that drove it can no longer stage its own scenario: React skips a click handler whose props say `disabled`, so no click reaches `save` during an import at all. That test is restaged as the assertion that now protects the case — the button is disabled in the same tick the gate is taken, with no `await` to hide behind — which makes it this item's regression guard rather than a restatement of its neighbour. **Four tests, each hand-applied and watched**: moving the gate back after the read fails three, including the restaged O97 one; removing the release from the read's `.catch` fails two, one of them the non-JSON case, which is how the two failure shapes were confirmed to be one chain rather than two. **What remains, and it is a loss rather than a gap**: `save`'s `writer.current !== undefined` check is now unreachable through the UI, and its refusal message with it. The check is kept — the invariant is what it states, and a future writer taking the gate outside a discrete event would reopen the window there rather than in `save` — but nothing exercises it, and an unreachable sentence is the mirror of the comment asserting a guard that does not exist that O96's review found | `RuleEditor.tsx::beginImport`, the file input's `onChange`, `RuleEditor.tsx::save`'s ref guard | the real-library drive owed by O97, 2026-09-08; closed the same day |
| O102 | [ ] | **The blur guard compares against a `root` prop a failed reload can leave stale.** *Re-read 2026-10-18 against O219's directory page, and kept: `RootRow` is that page's body, its boxes' `onBlur` still compares through `textPatch` against `root`, the save sent for a box still typed in as the page goes compares the same way against the row as last drawn (`leftPatch`; `RootRow.test.tsx`'s "sends nothing for a box typed back to what the row holds"), and `Libraries.tsx`'s `reload` still leaves the list it held standing when a read fails.* *As filed:* `RootRow.tsx`'s four text boxes decide whether to write by comparing the box against `field.from(root)`, and `root` is whatever `Roots.tsx` last *successfully* loaded. `save` calls `onChanged` — that reload — only after the `PATCH` resolves, and a reload which itself rejects leaves the previous `roots` array standing. So: the row holds `**/extras/**`, the admin sets `**/tmp/**` and the server takes it, the reload blips, and `root.exclude_globs` still reads `**/extras/**`; the admin changes their mind, types `**/extras/**` back and blurs — `next === field.from(root)`, nothing is sent, and the server keeps `**/tmp/**` while the screen shows the value the admin believes they restored. **The `edited` ref does not help**: the admin genuinely typed, so the first guard passes and the second is the one that returns. **Pre-existing for `edition_key`, `default_kind` and `default_language`** — O92's readable half extended it to `exclude_globs`, which is why it is filed now rather than in C4 PR A: the one column that could not be compared at all was the one column this could not reach. **Not fixed here, and the fix is a decision rather than a patch.** Comparing against the last value the *server acknowledged* rather than against the prop means this card holds state that has to outlive a reload — which is the shape C4 PR A's review already rejected once, when "the last value this card had sent" fixed pass 1 only within a single mount. What would make it worth taking is a reload failing often enough to be seen, and nothing measures that today. Found by reading the guard this branch rewrote, not by a failure | `RootRow.tsx`'s `textPatch` and `leftPatch`, `Libraries.tsx`'s `reread` | the review of O92's own fix round, 2026-09-08 |
| O103 | [x] | **The web `branches` aggregate margin had narrowed to 0.80 points and nothing regressed — the gate was being diluted by new code, not eroded by old.** `web/vite.config.ts` sets `branches: 92` under a measured 94.38% on 2026-08-09; it read 93.99% on 2026-08-14 and **92.80%** on 2026-09-11, which O98 measured and deliberately left because whether 92 still holds is a judgement rather than a transcription. **Measured first, and the cause is not what the number suggests**: splitting the report against `git log` (no web source file deleted or renamed in the window), the 66 files that existed on 2026-08-14 measure **93.96%** — the 93.99% recorded for them then, so the old tree did not move — while the 42 added since measure **90.91%** and carry 97 of the 203 uncovered branches on 38% of the branch mass. Had the new work landed at the tree's own rate the total would still read 93.96% and the margin 1.96. The whole 1.19-point slide is dilution, and the aggregate was green at every commit on the way down because it cannot see a marginal rate. **Closed 2026-09-12 by moving the coverage rather than the bar**, which is the sentence `web/vite.config.ts` has carried since the thresholds were set. **Most of the fix removes branches rather than testing them**: `err instanceof Error ? err.message : String(err)` stood at **40 sites across 21 files** — four of which had already grown a private one-line helper for it, three named `messageOf` and one `message`, which is `bytes.ts`'s "a second formatter is one that drifts" arriving — and 30 of the 203 uncovered branches were that ternary's `String(err)` arm, reachable only by a throw that is not an `Error`. One `messageOf` in `web/src/message.ts` took 78 branch locations out of the report and moved the aggregate 92.80 → **93.69** with no assertion about an unreachable state. **The remaining nine were bought honestly and the choice of which nine is the point**: the 35 `??` fallbacks and 38 `if (!current) return` guards are states the code refuses, so a test for one asserts that an impossible thing is handled; the race guards are reachable the moment two reads are in flight. Nine were pinned — `Users.tsx` ×2, `Library.tsx` ×2, `Series.tsx` ×2, `RuleEditor.tsx` ×2, `Search.tsx` — each by holding the *first* call open with a deferred promise and settling it last, since two `mockResolvedValueOnce`s resolve in call order and the guard is then never true. Every one hand-mutated and watched: guard deleted, suite run, exactly one test red, file restored. `RuleEditor`'s success guard had been evaluated **116 times and taken none**. **The `cancelled`-on-unmount guards were left uncovered deliberately** — React 19 warns on nothing when state is set after unmount, so a test for one passes with the guard removed, which is a test that passes against its own defect. **Two things found while re-deriving, one of them worse than this item**: the *per-file* branches floor had overtaken the aggregate — `Users.tsx` at **78.78%** against `coverage-floor.mjs`'s floor of 78, margin **0.78**, and at 26/33 branches a single uncovered one fails CI — having passed the statements margin that script's own comment names as the one to watch, and found by re-deriving the table in `docs/ci.md` rather than by any run; and this document's jsdom test count read 934 against an actual **936** before this branch, the ungated half of the figure, corrected to the measured 947 rather than by adding a delta to a wrong number. **One test beyond the nine**: `Labeller.tsx` lost a covered statement with its private helper, taking the per-file statements margin from 0.85 to 0.78, and the fix is the one `coverage-floor.mjs` prescribed — *"a test for `Labeller.tsx` rather than an 89 here"*. It turned out to be the one documented key nothing had ever pressed, **`Tab`**, handled ahead of every other binding with three statements behind it that had never run. **All eight margins ended better or level**: aggregate 1.42→1.72 statements, **0.80→2.05** branches, 2.16 functions, 2.22→2.31 lines; per-file 0.85→1.85 statements, **0.78→3.25** branches, 2.33 functions, 3.00 lines. 94.00 was the target because it is the figure at which the two-point rule re-derives the 92 that is already set. **What remains: the tightest web gate is now aggregate statements at 1.72**, and the dilution that caused this will recur — nothing gates the branch coverage of new code on its own, which an aggregate structurally cannot do | `web/src/message.ts`'s `messageOf`, `web/vite.config.ts`'s `thresholds`, `scripts/coverage-floor.mjs`'s `floors` | re-deriving `docs/ci.md` after O98 declined to, 2026-09-12; closed the same day |
| O104 | [x] | **Closed 2026-09-12, the day it was opened: nothing in the tree synced a file before renaming it into place, so the two writers that promise crash atomicity did not have it.** `stamp::rewrite_one` stages a whole archive beside the original and renames it over the top; `atomic::write` does the same for sidecars, hand documents, exports and covers. `rg -n 'sync_all\|sync_data' crates/` returned nothing outside tests, while their docs said "a crash leaves either the old archive or the new one, never a half-written file" and "a crash cannot publish half of one". A rename is atomic only for bytes already on disk: it can reach the journal first, and a crash in between leaves the path naming data never written with the old file already unlinked — ext4's `auto_da_alloc` guards this pattern and **XFS, which the NAS's array disks run, does not**. **Measured on the host that will run it** (`docs/notes.md`, 2026-09-12 and 2026-09-13): on its XFS parity array a synced same-disk copy of the library's largest archive, 2.39 GB, takes **55.3 s**, so a stamp written there left about a minute in which a crash destroyed a reader's only copy of a volume. The library share as configured stages new files on the NAS's NVMe pool instead, and through `shfs` a rename onto an original on the array puts the new file on NVMe and deletes the array copy — two operations on two filesystems, whose crash behaviour is unmeasured. The fix does not depend on which path a write takes: a share's cache setting is not taiyaku's to rely on, and the mover returns every archive to the array. **Fixed by syncing the staged file before the rename and its directory after**, in both, through one `atomic::sync_directory_of` — the directory sync checked first against Unraid's `shfs` and the array disk directly, since one that errored on FUSE would have failed every stamp. **Tested on the syscalls, because nothing else a test can read tells a synced file from an unsynced one**: each test re-runs itself as a child under `strace -f -qq -y` through `taiyaku_read::test_support`, which `taiyaku-library` reaches by a dev-dependency feature, and **fails rather than skips without strace** — the `NOT MEASURED:` shape is for a fixture's validity, not the only guard on a lost file — so CI's `test` job and `mutants.yml`'s sweep now install it. **The whole-branch review found the first version of these tests passing against the defect itself**: any sync of the staged file before the rename satisfied them, so syncing `atomic`'s empty temporary and then writing it, or syncing a stamp's archive before `finish` wrote its central directory, stayed green — reproduced before anything changed. The harness now traces every write syscall, and `assert_replaced_durably` requires a write, a sync after the last one, the rename, no write to the destination after it, and the directory sync. Every named defect hand-applied in both crates and watched red at its own assertion: no file sync, the file synced before its bytes or after the rename, no directory sync, the directory synced before the rename, the same bytes rewritten after the rename; plus a child `--exact` name that no longer matches, which libtest answers with exit 0, and strace absent from `PATH`. **The coverage report then found a line no test reached, and it was a regression the fix would have shipped**: `Path::new("out.json").parent()` is `Some("")`, not `None`, so without `sync_directory_of`'s fallback to the current directory `taiyaku align --write out.json` writes its sidecar and then reports failure. Pinned through strace as well — the child changes into the test directory and writes a bare name — because an `is_ok()` passes for a fallback that syncs the wrong directory; both the missing fallback and a wrong one hand-applied and watched red. **What the fix costs**: a stamp's request now holds for the flush — up to about a minute for the largest archive on the array, less where the share stages on NVMe — and every `atomic::write` pays two syncs, the cover and page cache and `hand::save` under the connection mutex included, measured through `shfs` onto the NVMe pool at **about 1.1 ms a write** under a load average of 17. taiyaku sets no timeout of its own, so that is a failure only behind a proxy that cuts sooner — and then a false one, since `spawn_blocking` finishes the stamp regardless — and no Dockerfile or proxy configuration here says whether one exists. **Same shape, deliberately unchanged**: `stump::cache::store` writes and renames with no sync, but holds derived answers `load` treats as nothing when unreadable. `atomic.rs`'s call-site tally was wrong a fourth time, eight naming three stores long since moved into the database, and is seven from the grep | `taiyaku_read::atomic::write`, `taiyaku_read::atomic::sync_directory_of`, `stamp::rewrite_one`, `taiyaku_read::test_support::traced` | timing a stamp on the NAS for the write-back design's §11 and reading the code the figure was for, 2026-09-12; closed the same day |
| O105 | [x] | **Closed 2026-09-13, the day it was opened. A cache hit was any file `fs::read` could open, so an empty cover, tile or reading page was served as `200 image/jpeg` with no body, and nothing ever regenerated it.** The key is content-addressed and never changes, so a broken file stayed broken. The whole-branch review of O104 found it: its reviewer's throwaway test got back a zero-length page. #124's "What remains" named `cached_or_generated_page`, and **that named one site of two.** `at_path` serves every cover and tile, and had the same line. The series cover route redirects to a tile, so it reached `at_path` too. A broken file gets there when a rename reaches the disk before its data, which `atomic::write` allowed until O104, or when a copy is cut short. **Fixed by one hit check, `cover::cached`, that both sites call.** A file counts as a hit only if it starts with `FF D8 FF` and ends with `FF D9`. Anything else is a miss: the caller regenerates, and `atomic::write` renames the new file over the broken one. It checks markers instead of decoding because the hit path runs outside `GENERATION_LIMIT` and has the bytes in memory already. **Checked against every cache the server had written on the development workstation before this work** (`docs/notes.md`, 2026-09-13): 1,156 files in three directories, covers, tiles and reading pages of both page format versions. All 1,156 pass and none is empty. **Tested with six broken shapes through all three entry points.** An empty file and a zero-filled one fail both halves of the check. The other four each fail exactly one half, and the fixture asserts that construction: truncated (keeps its start marker), cut short on a `D9` byte (a one-byte end check accepts it), headless (keeps its end marker), and damaged at the third byte (a two-byte start check accepts it). Each test requires one regeneration, the file replaced on disk, and the replacement served as a hit. Reading pages also get the hit test that covers and tiles already had. Two more tests hold every generation permit and require a warm cover and a warm reading page to be served within five seconds anyway, because the reason for checking markers instead of decoding depends on the hit running outside the permit. **Those two tests shipped a deadlock in their first version, and the review of the fix round found it.** Under plain `cargo test`, which every mutation sweep uses, they share one process, each could take one of the two permits and wait forever for the other, and 5 of 6 runs of `cover::tests` hung. A lock held for the whole helper fixed it: 0 of 30 runs of `cover::tests` and 0 of 10 of the whole crate hung afterwards, and both permit mutations still fail in about five seconds under either runner. **That review also found the permit's other half untested**: deleting the line that takes it, at either site, left every test green. Two more tests hold every permit and require a cold cover and a cold reading page not to fetch their page for 300 ms, then to fetch once released. A correct generation cannot fetch while the permits are held, so neither test can fail on a slow machine. *(Since O111 closed on 2026-09-17, a reading page that needs a resize fetches while the permits are held, by design; its test watches the resize finish instead, and still goes red with either break below.)* Deleting either permit, or building the semaphore with one permit more than `MAX_CONCURRENT_GENERATIONS`, now goes red. The value of `MAX_CONCURRENT_GENERATIONS` is deliberately not pinned: it is a tuning choice with its measurement in its own doc comment. Fourteen hand-applied defects all went red on the case built for them: no check, either half alone, `&&` as `\|\|`, rejecting every file, each site bypassing the helper, a one-byte end check, a two-byte start check, the hit moved inside the permit at either site, the permit deleted at either site, and a semaphore with one permit too many. **The whole-branch review found four of those surviving the first version of the tests**: the two short marker checks and both permit moves. Its other findings were wording, and one defect outside this item, opened as O107. *(O107 has since closed — see its entry.)* **Driven against a copy of the real library** with a scratch cache. The pre-fix binary, with three cache files emptied, answered `200 image/jpeg 0B` for an OPDS cover, a tile and a `?w=300` page. The fixed binary regenerated each one to its original byte count. It did the same for a file cut in half, a zero-filled one and one a byte short, and a later request left the file's mtime unchanged. **Left unchanged on purpose.** Damage between the two markers is still served, because catching it would put a decode on the hit path. A reading page's `ETag` names the cache key, not the bytes, so a browser already holding an empty page is answered `304` and keeps it (checked with curl). Covers and tiles send no `ETag` and recover on the next request. No browser can hold one today: no taiyaku container runs on the NAS, none of the three caches holds a broken file, and before O105 nothing ever replaced one. *O106 has since changed every resized tag once, which replaces such a page. See its entry below.* | `cover::cached`, `cover::at_path`, `cover::cached_or_generated_page` | the whole-branch review of O104, named in #124's "What remains"; closed 2026-09-13 |
| O106 | [x] | **Closed 2026-09-13, the day it was opened. A resized page's `ETag` did not carry `PAGE_FORMAT_VERSION`, so a browser holding an old rendering of a page would be answered `304` for it after the rendering changed.** The cache file name and the tag each built the page's key by hand, and #35 (`f81913d`) put the version into the file name only: `"<sha256>-p<offset>-r<edge>"` against `…-r<edge>v2.jpg`. `page_etag`'s own doc already names this trap for the long edge ("a page that looks soft for a day with nothing to point at"). A version bump is the same failure under an unchanged size: the server regenerates every cached page, and every browser revalidates its old copy, gets `304` and keeps it. **Reached once, on the branch that introduced it.** This entry first said it had never been reached, because `git log -S` on `main` sees only #35's squash commit. The whole-branch review read the PR's own commits in `refs/pull/35/head`. `69eb898` served resized pages as `…v1.jpg` under the version-less tag, and `08daa05` bumped to version 2 for a new resize filter about an hour later and left the tag alone. `web/.taiyaku-cache` still holds 312 version-1 pages from that night. Pages were then sent with `private, max-age=86400`, so a browser that read one would have kept it for a day without asking, and would then have been answered `304` and kept it again. **Found by reviewing what O105's fix left behind.** The same unchanging tag is what would keep a browser that was served an empty page showing nothing after O105. **Fixed by one key, `cover::page_key`, which is both the cache file's name and, in quotes, the tag**, so a parameter added to one reaches the other. File names do not change, so no cached page is regenerated. Every resized tag changes once, so each browser re-fetches each resized page one time, and that re-fetch also replaces any empty page served before O105. For a page that already fits, the re-fetch is the original bytes. The full-size tag is unchanged, because the member's own bytes do not depend on resizing. **Tested**: the tag must equal the cache file's name without `.jpg`; the key must change when the hash, the offset, the long edge or the format version changes on its own, and must keep the shape `<sha256>-p<offset>-r<edge>v` every cached page already has; `page_key` must use the real constant; a request carrying the old tag must get `200` and the page; and a width above `MAX_FIT_LONG_EDGE` must carry the same tag as the ceiling. The tag-equals-key and old-tag tests failed on O105's branch. **The whole-branch review found three holes in the first version of these tests.** The key test changed only the version, and an older test changed offset and edge together, so a key without the offset passed every test. Such a key would give every page of a member at one width the picture of whichever page was cached first. Swapping offset and edge also passed, which renames every cached page, and so did building the tag from `?w=` as sent rather than the clamped width. The fix round's first attempt at the swap asserted that swapped arguments give a different key, which a swapped key satisfies too; the name's shape is what catches it. **The review of the fix round found the same hole one function over**: a tile's cache path could lose its hash, `THUMB_LONG_EDGE` or `FORMAT_VERSION` with every test green, because only its offset was ever varied alone. Without the hash, page N of every member shares one tile. `thumb_cache_path` now builds its name through `thumb_keyed_path`, which takes the parameters as arguments, and a test changes each one alone and pins the name every cached tile already has. That review also found four route sites that could use a page's index where its offset belongs with every test green, opened as O108. *(O108 has since closed — see its entry below.)* Fourteen hand-applied defects now all go red: the tag built by hand without the version, the key without the version, the key at the wrong version, the file name built by hand without the version, the tag without quotes, the key without the offset, the hash or the edge, offset and edge swapped, the tag from the unclamped width, and the tile path without its hash, its long edge or its format version. **Driven against a copy of the real library** (`docs/notes.md`, 2026-09-13). On O105's binary the `?w=300` tag was `"<sha>-p5-r300"` and a request carrying it got `304`. On this branch's binary the tag is `"<sha>-p5-r300v2"`, the old tag gets `200` with all 22091 bytes, and the new one gets `304`. The full-size tag `"<sha>-p5"` is unchanged and still gets `304`, and the page cached by O105's binary was served without being rewritten | `cover::page_key`, `cover::thumb_keyed_path`, `routes::page_etag` | reviewing what O105's fix left behind; closed 2026-09-13 |
| O107 | [x] | **Closed 2026-09-13, once a page turn had been timed. A `?w=` request for a page that already fits read the page out of its archive twice, and the first read was inside a generation permit.** `cover::cached_or_generated_page` fetched the page to learn whether it fit and dropped the bytes when `fit_at` answered `None`, and `routes::get_page` read the page again to serve it. Its doc comment, and two comments in `get_page`, said the caller already had the bytes. **Found by the whole-branch review of O105**, which counted archive opens under `strace`. **Measured before fixing** (`docs/notes.md`, "O107"), on the workstation's NVMe with a warm page cache and not on the NAS. Every page header was read: all 195 `raw-mono` pages fit a request of 1024 or more, and 234 of 235 `en-fan` pages fit 2048. So at `devicePixelRatio` 1, any window taller than 768 CSS pixels reaches that arm on `raw-mono`. The browser confirmed it: 1440×900 in `two` mode asked for `?w=1024`, and each of 60 cold page turns on `raw-mono` and `en-fan` requested its `?w=` URL after the keypress. So the double read landed on the turn and not on prefetch. `strace` counted, on a JPEG and a PNG member: 1 open at full size, 2 for a page that fits on every request (nothing is cached for one), 1 for a resize and 0 for a cache hit. The second read cost the server about 1.2 ms on `raw-mono` and 1.4 ms on `en-fan`'s JPEG pages (0.27 ms on PNG), against a keypress-to-paint median of 52.5 to 54.0 ms per turn. **The permit wait does show.** While a cold `en-mono` filmstrip generated, a request for a page that fits waited a median 164 to 166 ms, against 3 ms without one, and the full-size and cache-hit requests beside it did not wait. **Fixed by handing the fetched page back.** `cached_or_generated_page` returns `Fitted::Resized`, or `Fitted::Original` holding the page as fetched, and `get_page` serves the `ServedPage` it fetched, content type included. After the fix `strace` counts 1 open for a page that fits. The server's difference against full size fell to about 0.04 ms, while the pre-fix build rerun straight after still showed 1.1 to 1.3 ms. Responses are byte for byte unchanged, and a PNG page is still `image/png`. The page turn did not measurably change: before and after, turns land on 16.7 ms frame boundaries. **Tested by counting reads.** `test_support::CountingSource` records each read's member and offset. A route test requires one read each for full size, a page that fits and a resize, and none for a cache hit, and it failed before the fix with two reads. A cover test requires a page that fits to come back as fetched, the field beside its bytes included, and not cached. Six hand-applied breaks each went red against 571 tests: the second read put back, which only the new route test sees; that arm serving `image/jpeg`; the second read by offset; the one fetch by offset; a page that fits written to the cache; and a page that fits answered as a resize. Plain `cargo test` passed 571 of 571 three times. `mutants.sh --since main` found eight mutants: four caught, none missed, and four unviable, with no survivor outside the baseline. **All four unviable mutants are `cached_or_generated_page`'s.** `Fitted` has no constructor or `Default`, so cargo-mutants built no mutant of the function this item changed. The two whole-function replacements it could not build, `Resized` of empty and of one-byte bytes, were applied by hand, and each turned 11 tests red. **What remains.** The fetch and the header parse still run inside the permit, so the wait behind a cold filmstrip is unchanged (170 ms median after the fix). Moving them out is not done here, and the trade is narrower than bounding how many pages are held: a full-size request already fetches with no permit at all, and the permit's own doc comment justifies it by decode memory. What would change is that a request needing a resize would hold its fetched page while it queues for the decode. *(Filed as O111 on 2026-09-13, and closed on 2026-09-17: the fetch and the header parse now run outside the permit, and a page that fits no longer waits — see its entry.)* **Found while timing, and not fixed: prefetch warms the wrong URLs.** *(Filed as O110 on 2026-09-13 — see its entry.)* `App.tsx`'s prefetch builds `pageUrl` without the long edge, so it never warms the `?w=` URL a turn requests, and on a first visit each page is sent twice. The NAS, where a read is a seek behind parity, was not measured | `cover::cached_or_generated_page`, `cover::Fitted`, `routes::get_page`, `test_support::CountingSource` | the whole-branch review of O105, 2026-09-13; closed 2026-09-13 |
| O108 | [x] | **Closed 2026-09-13, the day it was opened, with tests and a fixture and no production change: the shipped code was right at all five sites, and no test could have said so.** Four route sites could key a page by its index in the edition instead of its offset in its member with every test green: `get_page`'s `ETag` and its reading-page cache key, `get_thumb`'s tile key, and `opds::stream_page`'s `ETag`. The offset is what lets one archive shared by two editions or two works share one cache entry and one tag per page. **The entry as opened said every route fixture requests pages whose index equals their offset, and that was wrong**: `test_support::work`'s edition reaches such pages, and route tests request them. What no fixture did was put one member at two positions. A name built from the index is still unique within one edition, the one fixture with two editions gives both the same members in the same order, and `work`'s pages cannot be decoded, so none of its requests reached a resize or a tile. `get_page` could also hand the resize and its cache key the unclamped `?w=`, because the fixture's 16×16 page fits inside every width where the two differ, and no route test read the cache. **The fixture is `test_support::state_with_a_shared_member`.** `b.cbz` is the second member of edition `ab` and the whole of edition `b`, so page 2 of `ab` is page 0 of `b`, while page 2 of `b` is a different page at the same index. Its pages are solid red, green and blue, so a test can say which one it was served after a resize. One page of `a.cbz` is 4 pixels wider than `MAX_FIT_LONG_EDGE`, so a request for `MAX_FIT_LONG_EDGE + 8` resizes it only when the width is clamped. **Five tests, and each site broken by hand against all 567 `taiyaku-serve` tests:** an index-keyed tag at either route answers `304` for a different page, an index-keyed cache at either route serves `b.cbz`'s third page for its first, and the unclamped width serves the page whole, as PNG. No earlier test went red under any of the five. **The cache tests read the files, and that caught one more:** `get_page` caching its reading pages in the covers directory instead of `thumbs` left every earlier test green. **The whole-branch review found three more mutants that left all 567 green**, each rerun here: `get_page` reading a page that already fits by its offset, `get_thumb` taking a tile's hash from the edition's last member, and the OPDS cover route doing the same, which predates this branch. A test of a page that already fits, a test of each edition's cover, and a tile request for a page of `a.cbz` now each fail under one of them, against 569. The runs are in `docs/notes.md` under "O108". | `routes::get_page`, `routes::get_thumb`, `opds::stream_page`, `test_support::state_with_a_shared_member` | the review of O105's and O106's fix rounds, 2026-09-13 |
| O109 | [ ] | **A form commit discarded by an import stays discarded when that import fails, where O96's failed save keeps the held edit — and nobody decided that.** A commit from `RuleEditor.tsx`'s form that arrives while `writer.current === 'import'` is dropped, and `refused` says why: *"an import replaces every rule, so there is no row left to apply it to. Make the edit again once the import has finished."* That reason is true of an import that succeeds — `sendImport`'s success path calls `commit(answer.rules, undefined)`, and an import mints fresh ids — and not of one that fails. `sendImport`'s `.catch` calls `release()` and `setFailure` and never touches the list, so the rules on screen are still the ones the edit was made against, `RuleList` was frozen for the whole window, and the index the form holds still names the rule it was editing. `rules::import_json` runs in one transaction, so a refused import wrote nothing on the server either. That is O96's own reason for keeping a failed **save**'s held edit: it wrote nothing, so the list is still the one that was sent. A refused document is this route's ordinary failure, because `rules/import` rejects a malformed one. **Until now it was recorded only where nothing open points**: on O100's closed row, in `docs/notes.md` (search "decision nobody had made"), in #118's squash message (`c72a258`), and in the comment at the discard itself. **A decision, not a defect.** Holding the edit across an import and applying it only when the import fails is a behaviour change: a second disposition for one commit, and a notice that has to say which ending happened. Discarding on both endings is one rule rather than two. **What would decide it: a commit landing inside an import that then fails, often enough to be seen.** Since O101 the window runs from the file read to the answer. The request half measured 1.7–6.0 ms (`docs/notes.md`, O97's postscript); the read half has never been measured. So today a blur has almost nothing to land in, and a large document, a slow route, or a report of a lost edit is what would change that. Surfaced in #118's own fix round, when its discard test was made to fail the import | `RuleEditor.tsx`'s form commit (its `writer.current === 'import'` branch), `RuleEditor.tsx::sendImport`'s `.catch` | the fix round of O100 (#118), 2026-09-08; filed 2026-09-13 |
| O110 | [x] | **Closed 2026-09-14, once a page turn had been timed before and after. Prefetch warmed a page at full size, and a page drawn fit-whole asks for it resized, so on a first visit prefetch never warmed the page a turn drew.** `App.tsx`'s prefetch effect built `pageUrl(current.work, queuedFor.edition, target.page)` with no long edge and handed it to `decodeTo`, which sets a throwaway `Image`'s `src` and awaits `decode()`. A cell builds `pageUrl(…, pageLongEdge)`, which at fit-whole is a `?w=` rung from `pageSize.ts::requestedLongEdge`. Those are different URLs and so different browser cache entries. **Dates from #35** (`f81913d`), which put `pageLongEdge` into the cell's URL and edited the prefetch effect in the same diff without giving its URL a long edge. **Measured before fixing** (`docs/notes.md`, "O110"), on this workstation and not on the NAS: `en-mono` in `two` mode at 1440×900, twenty turns a run, a new server cache and a cleared browser cache each time, four runs alternating. Before the fix, all 36 drawn pages were requested only after their keypress, in both runs, and the turn took a median of 229.5 and 210.6 ms. The server's log showed all 38 pages the visit drew sent twice, once whole to prefetch and once resized for the cell that drew it, and a resize took a median of 122–124 ms. A visit transferred 91.78 MB of pages. **Fixed by building prefetch's URLs from the cells' binding.** `pageLongEdge` moved ahead of the prefetch effect, the effect passes it to `pageUrl`, and it is in the effect's dependencies, so a resize or a zoom that moves the rung warms the window again. A turn carries the zoom, so the page it lands on is drawn at the rung the reader is already at. After the fix no drawn page was requested after its keypress, the turn took a median of 3.8 and 3.1 ms, no page was sent whole, and a visit transferred 6.86 MB. **Tested by recording what was warmed.** Three `App.test.tsx` tests stub `Image`, turn a page, and require that the drawn URL was recorded before the keypress: a plain turn, a turn after a resize, and a turn after a zoom, the last two also requiring that the rung moved. All three failed before the fix. Applied by hand, the URL built without the long edge turned all three red, and `pageLongEdge` left out of the dependencies turned the resize and zoom tests red. **What remains.** A new rung does not drop jobs queued at the old one, since `superseded` compares only the position. Prefetch's `?w=` requests now take a `GENERATION_LIMIT` permit on a cache miss, where its full-size requests took none, which bears on O111. *(O111 has since closed, on 2026-09-17: a prefetch for a page that already fits takes no permit now, and one that needs a resize still does.)* The prefetch window's defaults were tuned in #35 against full-size decodes and have not been measured again. The NAS was not measured | `App.tsx`'s prefetch effect (the `pageUrl` it queues on `prefetchQueue`) and `pageLongEdge`, `pageSize.ts::requestedLongEdge`, `App.test.tsx`'s `prefetch` tests | timing a page turn for O107, 2026-09-13; closed 2026-09-14 |
| O111 | [x] | **Closed 2026-09-17, once a page that fits had been timed behind a cold filmstrip before and after. A `?w=` page request waited for a generation permit behind a cold filmstrip, even when the page already fit and nothing was decoded — measured at a median of about 165 ms a request.** `cover::cached_or_generated_page` took `GENERATION_LIMIT` before it called `page()`, and `fit_at`'s header parse ran inside the permit too, so a request that missed the cache queued for one of `MAX_CONCURRENT_GENERATIONS`' two permits whether or not a decode followed. `cover::at_path` takes the same permit for every cover and filmstrip tile it generates, so a cold filmstrip, or a cold cover grid, can hold both. A full-size request fetches with no permit, and a cache hit is served before the permit is taken. *(Since O110 closed on 2026-09-14, prefetch asks for pages at `?w=` too, so on a cache miss its requests took a permit as well, and prefetch queued for the same two permits as a cold filmstrip. The wait below was measured before that.)* `Chrome.tsx`'s doc comment cites a measurement that the tiles' `loading="lazy"` did not defer their requests in the environment tested, so an open filmstrip asks for its tiles in a burst. **Measured once, while timing O107** (`docs/notes.md`, "O107", "The permit wait"; this workstation, not the NAS): with 187 `en-mono` tiles requested cold over six connections, 7.0–7.1 s of generation, a fitting `raw-mono` page at `?w=1024` answered in a median of 163.8–166.5 ms across three runs, against 2.9–3.1 ms with no filmstrip, while the full-size request beside it stayed at 1.6–1.8 ms and a cache hit at 0.25–0.32 ms. After O107's fix the same wait measured 170.1 and 169.7 ms: that fix removed a read, not the queue. **Filed as a decision, not a defect**, with two shapes and neither free: taking the fetch and the header parse out of the permit, or giving reading pages a budget of their own, and it named a page turn timed in the browser on the production host as what would decide it. **Its build design narrowed that** (`docs/superpowers/specs/2026-09-15-o111-permit-scope-design.md`): the permit's own doc comment justifies it by decode memory (`MAX_DECODE_ALLOC`) and by nothing else, and a page that fits decodes nothing, so holding a permit to learn that it fits rations something it never asks for. A second budget lost because two budgets bound two things and their sum bounds nothing. **Fixed by splitting `fit_at` in two.** `cover::fits_within` reads the header and answers whether the page already fits; `cover::resize_to_long_edge` decodes, resizes and encodes. `cached_or_generated_page` now serves a cache hit, fetches, asks `fits_within`, hands back a page that fits with no permit, and takes the permit around `resize_to_long_edge` alone. `fit_at` had no production caller left and is gone. `at_path` is unchanged: a cover or a tile always decodes. **The cost, stated rather than hidden**: a page that needs a resize now holds its fetched bytes while it queues, which is not measured. **Measured before and after** (`docs/notes.md`, "O111"; this workstation, release builds, the O107 method rebuilt and kept as `scripts/permit-wait.sh`): under a cold filmstrip a fitting page answered in a median of 167.0–179.4 ms on `main`, **1.78–2.05 ms** on the fix and 161.8–170.1 ms on `main` again straight after. With no filmstrip it was 1.67–2.05 ms on every build. A page that needs a resize, timed in the same runs, was not expected to move and did not move by anything like that: 176.6–194.4 ms on the fix against 187.0–212.6 ms across both runs of `main`, and a filmstrip took 6.9–7.6 s on every build. **Tested**: `a_page_that_already_fits_is_served_while_every_generation_permit_is_held` failed before the fix. `a_cold_reading_page_waits_for_a_generation_permit` watched the fetch, which a page needing a resize now does while the permits are held, so it was rewritten as `…_that_needs_resizing_waits_…` to watch the resize finish. `a_page_is_resized_only_when_its_long_edge_exceeds_the_request` pins the boundary. Applied by hand, the permit put back around the fetch, the permit moved to the header parse, and `<=` turned to `<` or `>` each turned a named test red; the permit deleted around the resize turned the rewritten test red. **What remains**: the guard bound to `_` rather than `_permit` drops the permit as soon as it is taken, and every test passes against it; `at_path` takes its permit the same way and was not mutated *(filed as O129)*. Peak memory with fetched pages queued is unmeasured, and so are the NAS and a page turn drawn in the browser | `cover::cached_or_generated_page`, `cover::fits_within`, `cover::resize_to_long_edge`, `cover::at_path`, `GENERATION_LIMIT` | timing a page turn for O107, 2026-09-13; closed 2026-09-17 |
| O112 | [x] | **Closed 2026-09-19, on the style pass's ③ branch, once the user had chosen among its options with the new bars drawn (spec 2026-09-17 §8.1): while the chrome is shown, the note list under the page is `visibility: hidden` — still mounted and still taking its space, so the page area does not change on `Tab` — and it comes back when the chrome goes. Measured in the browser project at 1440×900, 768×1024 and 390×844 with nine notes on the slot on screen: with the chrome hidden every row takes a click at its centre, and with it shown the list's computed visibility is `hidden` and the page area's height is unchanged (`App.browser.test.tsx`, "O112"). Measured again in the running app (`docs/notes.md`, "Style pass ③"): two notes, page area 835 px at 1440×900 and 779 at 390×844, the same with the chrome shown, and the readout still saying "2 notes here of 2". The band above the strip where O65 docked `Library` holds nothing while reading now that the button is in the top bar (§8.3); the hover preview still stands there while correcting. The cost the user accepted: no note can be read or opened while the strip is up.** **With the chrome open, the strip and the readout row cover a note list placed below the page; at four notes, no row took a click at its centre.** `NoteList` placed `below` sits in flow at the foot of `App.tsx`'s column, under `belowStyle`'s `maxHeight: '28%'`, while `Chrome`'s strip stands from 40 to 216 px above the reader's bottom edge with its readout row below it, both absolutely positioned over whatever is there. **Measured in the browser project** (`docs/notes.md`, "O65", "The note list below the page, measured before merging"), with notes on the slot on screen and the chrome shown: at four notes the whole list lay within the bottom 216 px that the strip and the readout row stand over, and a click at the centre of each row reached none of the four, at 1440×900 and at 768×1024; at ten notes, three of ten and two of ten. The counts were the same with `Chrome.tsx` as it stood before O65 (`0ae0eaf`) and with O65's, so this predates O65; what O65 added is that, at ten notes, its button covers the left end of the top one or two rows the strip leaves visible. `below` is not only a setting a reader picks: `notes.ts::listPlacement` chooses it under the default `auto` for any window narrower than 1100 px. **A decision, not a patch**, because every shape costs something: making room for the list above the strip while the chrome is shown resizes the page area on `Tab`, which `Chrome` exists not to do; floating the list over the strip hides the strip's tiles; floating it above the strip covers the page, and the band where O65 docks the `Library` button; hiding the list while the chrome is shown hides notes the reader asked to see. **What would decide it**: which of those a reader with notes revealed minds, on the window sizes `auto` sends to `below`. **Not measured**: a real library's notes, since the probe used notes with no region on a six-page fixture; and how often a reader has the chrome open while notes are revealed | `NoteList.tsx`'s `belowStyle`, `App.tsx`'s `notesPlacement === 'below'` mount, `Chrome.tsx`'s bottom bar, `NoteList.tsx`'s `covered` | the pre-merge check of O65 (#133), 2026-09-14 |
| O113 | [x] | **Closed 2026-09-14, the day it was opened. When a cell's `src` came back to the page it was already drawing, a decode issued before the return was still drawn once it settled, and stayed on screen until `src` next changed.** `PageView`'s effect returned without taking a number from its `ordering.ts` instance in two cases: when `src` equalled `shown`, and when unbuffered. `decode.ts`'s `presentWhenDecoded` presents a superseded decode and leaves ordering to `PageView`, whose callback, it says, "can never paint over a newer one"; `inOrder` drops only an attempt older than one already drawn, and a return had created nothing newer. **Dates from #35** (`f81913d`): the early return is already in #10 (`5172922`), but until #35 `presentWhenDecoded` presented only `if (!stale())`, which dropped such a decode. **Found by answering what O75's notes left open**: "= and - and 0" ends showing the `?w=1536` page, and whether that is the width it asks for there "was not looked into". **It is not.** Logged in jsdom (1024×768, `devicePixelRatio` 1, `single` mode), fit-whole asks for `?w=1024` and `=` for `?w=1536`; `-` and `0` brought `src` back to the drawn page without a number, and the two `?w=1536` decodes were drawn after `0`. **Three ways a reader meets it, each driven in a browser reporting Chrome 153** (`docs/notes.md`, "O113"), on `gotoubun-v01` `en-mono` at 1000×700 in `single` mode, through a proxy that holds chosen requests before forwarding them: `=` then `0` before the zoomed page had decoded left the 1079×1536 page drawn at fit-whole, pressed 5 and 57 ms apart with nothing held and 307 ms apart inside a 3 s hold on `?w=1536`; a turn and a turn back inside a hold on the neighbouring pages left page 73 drawn while the image's `alt`, which names the reader's position, said page 72; and at 1.749×, which asks for `?w=2048`, `-` then `=` inside a hold on `?w=1536` replaced the 2048 page with the 1536 one. When the zoomed page decoded before `0`, with and without a hold, the page went back to `?w=1024`. **`two` mode is reached as well**: in jsdom, a turn onto a group of one page and back left both cells drawing that page, and `secondCellAlone`, which reads what each cell reports through `onShown`, hid the second. **So is the unbuffered path, for a moment**: a decode that settled while the Prefetch module was off was what `shown` held when it was switched back on, and in jsdom the mounted reader drew that page until the new decode settled. **Fixed by giving every `src` a number**, including the two cases that start no decode, and drawing it under that number at once, so a decode issued earlier is older and `inOrder` drops it. It still joins no cohort. **Five tests, each watched red**: in `PageView.test.tsx`, a decode settling after a return to the drawn page and one settling while unbuffered; in `App.test.tsx`, "= and - and 0" now requires `?w=1024` after its settle, and a turn and a turn back in `single` mode and in `two` mode. Written before the fix, all five failed. Applied by hand to the final code, `main`'s `PageView.tsx` failed all five, and so did the number taken without drawing under it; the unbuffered branch put back alone failed only the unbuffered test, and the return put back alone failed the other four. cargo-mutants does not cover `web/`, so those are the mutation evidence. **Driven again on production bundles after the fix**: `=` then `0` with and without a hold, the turn and turn back, and `-` then `=` each ended on the page and rung the reader was at, and a flick of twelve presses 25 ms apart drew seven intermediate pages before the fix and eight after, on different pages of the edition. **Not tried**: the NAS, a touch device and the wheel; `two` mode and the unbuffered path were checked in jsdom only | `PageView`'s effect, `ordering.ts::inOrder` | the observation O75's notes left unexamined, 2026-09-14; closed 2026-09-14 |
| O114 | [x] | **Closed 2026-09-15, once a series' titles had been corrected through the API and the root scanned again over real archives. A correction no longer moves `series.sort_title`, which stays the key the scan and the proposals find a series by.** **A rescan after a series' titles are corrected mints a second series under the scanned name and moves every volume onto it.** `PATCH /api/library/series/{id}` goes through `librarydoc::set_series_titles`, which moves `series.sort_title` to the new primary title, and `derive::reconcile_series` finds a series by `sort_title` — so the next `library scan` finds nothing under the name it reads from disk, inserts a new series, and the works follow it. Measured on O83's drive (`docs/notes.md`, "O83", "A rescan after the correction, seen and not acted on"): after Kaguya's romaji title was corrected, one `library scan` derived `3 series, 4 works … 1 series holding no work`; the database then held a series 4 under the scanned title with both Kaguya volumes on it, and series 2 keeping the three corrected titles and no works. A correction that keeps the primary text was not rescanned, so whether one survives is not measured. **Reachable today only through the API** — no screen edits a series' titles (O83's what remains) — and it is the first thing a series-title editor would meet, so it wants settling before one is built. **A defect whose fix is a choice of key**: keep `sort_title` as the scan's own key and let a correction change only the titles, or have `reconcile_series` match a series by any title it holds. **Fixed by** `librarydoc::set_series_titles` and `apply_series`'s matched branch no longer writing `sort_title` — the insert branch still does, since a series a document mints has no scanned name — and by `derive::reconcile_series` writing its title row only onto a series holding none: with the key frozen, the scanned text a correction had removed would have come back on every pass as a second primary title. `browse::series` shows the first title written, in `rowid` order, before `sort_title` where no title is primary, which is what the grid showed while `set_series_titles` wrote that title into `sort_title`. **The key was the user's choice** (2026-09-15): `sort_title` stays the scan's own key rather than a series being matched by any title it holds, which would not have found the measured case, where the correction replaced the scanned text. **The row's reach was narrower than the defect's** *(it named the rescan alone)*: `proposals` also finds a series by `sort_title` — `raise_attach_chapters`, a candidate rule's reach, `raise_edition_of`, `raise_same_series` and the evidence sweep — so on `main` a corrected subject's evidence already named no directory before any rescan. **Seven tests**: a correction replacing the scanned text, one making another title primary, one keeping the scanned primary, a document naming another primary, an `attach-chapters` proposal across a correction and a rescan, a list with no primary on the grid, and O83's drive through `PATCH` in `taiyaku-serve`. On `main` five failed; the kept-primary case passed, which answers this row's unmeasured question — that correction survived a rescan before the fix too — and so did the grid's, whose fallback `main` did not need. **Watched red** (`docs/notes.md`, "O114"): each of the four changes put back by hand, with the whole workspace run, failed between one and four of the new tests and nothing else. **Over real archives**: O83's drive repeated on the fixed binary; the rescan derived `3 series, 4 works, 4 editions, 4 members` with no series holding no work, and Kaguya kept id 2, both volumes and the three corrected titles, its `sort_title` still the scanned text. **What remains**: a proposal row, its evidence and `GET /api/works`'s `series` now show the scanned name for a corrected series where they showed the corrected text, because each reads `sort_title`, and the rule an accept writes (`offered_rule`'s `set_series`) has to name what the scan resolves. Whether an inbox row should show a corrected title beside it is a question for the series-title editor O83's what remains names | `librarydoc::set_series_titles`, `librarydoc::apply_series`, `derive::reconcile_series`, `browse::series`, `series.sort_title` | O83's drive over real archives, 2026-09-14; closed 2026-09-15 |
| O115 | [x] | **Closed 2026-09-15, once each handler that asks a grant had been run with its check back on the async worker, by hand, and a test calling it directly had failed naming that handler. Both checks now ask the library on the blocking pool for a grant that names roots, and an empty grant still never reaches the library.** **For a reader whose grant names roots, four `routes.rs` handlers take the library's mutex on the async worker before any `spawn_blocking`.** *(Eight handlers, not four: `refuse_if_hidden` is `pub(crate)`, and `opds::stream_page`, `opds::cover`, `download::edition` and `download::member` call it first in the handler the same way. Against `aaacc2b`'s handlers the test below stalled on all eight.)* `refuse_if_hidden` (in `get_page` and `get_thumb`, and twice in `patch_alignments`) and `visible_editions_or_refuse` (in `get_sidecar`, and first in `patch_alignments`) call `state.library.conn()` in the handler itself. Measured under O71's watchdog (`docs/notes.md`, "O71", "A reader with a grant"): with `roots: vec![1]`, `get_page`, `get_thumb`, `patch_alignments` and `get_sidecar` each stalled the runtime, and `get_page` and `get_sidecar` with an empty grant did not. `routes::tests::no_works_route_stalls_the_runtime` drives an empty grant, so it cannot see this, and `curation/tests.rs`'s doc names only `get_sidecar`. The failure O12 closed for the handlers' own store work, one call earlier. **A defect with a known fix**: take the grant check inside each handler's closure *(not inside: `get_page`, `get_sidecar` and `opds::stream_page` answer a matching `If-None-Match` with 304 before their closures run, and a hidden work has to be refused first, so each handler still asks at the point it did and the check takes its own trip to the blocking pool)*, and give the stall test a principal with roots. **Fixed by `routes::on_the_grant`**, which both checks now go through. For a principal whose grant names roots it runs the check on the blocking pool, and for anyone else it answers on the async worker without the library, as before. `patch_alignments` parses the body first, asks about the work, `from` and `to` in one trip through `routes::visible_editions_for_a_correction`, and refuses a body that did not parse only after that trip, so its refusals keep their order. `curation/tests.rs`'s doc no longer says `get_sidecar` asks on the async worker. **Three tests** in `routes/tests.rs`. `no_grant_check_stalls_the_runtime` calls all eight handlers directly under `the_runtime_stalls_on`, as a reader granted root 1, over `test_support::state_whose_grant_waits_on_the_library` (O71's fixture with every member under root 1 and its archives written), asserts every call answered 200, and names every handler that stalled. `a_hidden_work_is_refused_before_its_validator_is_answered` asserts that a matching `If-None-Match` to `get_page`, `get_sidecar` and `opds::stream_page` answers 404 for a reader granted root 2, which holds nothing, and 304 for root 1. `a_hidden_work_is_refused_before_its_correction_is_read` asserts the same of a malformed correction, 404 against 400. **Watched red** (`docs/notes.md`, "O115"), each applied by a script to the final code with the whole workspace's tests run and the file restored and compared: each of the eight handlers with its check back on the async worker failed the stall test naming that handler alone, 2155 passing against 2156 for the control; `on_the_grant` without `spawn_blocking` failed it naming all eight; each of the three handlers asking after its 304 failed the validator test with 304 for 404; a malformed correction refused before the grant failed the correction test with 400; dropping the `from` check, the `to` check, or both failed `auth::tests`' existing correction tests; and a fixture leaving every member under no root failed all three new tests. Run alone 30 times at one-minute load averages from 17.29 to 14.12, the stall test passed 30 times. **What remains**: the watchdog sees a call's first wait on the library's mutex, so a second wait added on the async worker after a grant check would pass the test. And OPDS's `works` and `work_feed` ask a grant through their own code, `opds::visible_editions_by_work` and an inline `browse::work`, on the async worker rather than through these two checks: under the same watchdog each stalled with `roots: vec![1]` and not with an empty grant. That is not filed as an item *(filed and closed as O119 on 2026-09-15)* | `routes::refuse_if_hidden`, `routes::visible_editions_or_refuse`, `routes::on_the_grant`, `routes::visible_editions_for_a_correction`, `opds::stream_page`, `opds::cover`, `download::edition`, `download::member`, `routes/tests.rs::no_works_route_stalls_the_runtime`, `routes/tests.rs::no_grant_check_stalls_the_runtime`, `test_support::state_whose_grant_waits_on_the_library` | O71's watchdog run with a grant, 2026-09-14; closed 2026-09-15 |
| O116 | [x] | **Closed 2026-09-15, once each browse handler had been run back on the async worker, by hand, and a test calling it directly had failed naming it. Every handler in `library_api.rs` reads the library through `library_api::on_the_library`, one trip to the blocking pool per request, whatever the grant.** **`library_api.rs`'s seven browse handlers take the library's mutex on the async worker, with no `spawn_blocking` at all** — *unmeasured* *(measured 2026-09-15: all seven stalled, for a reader granted root 1 and for an empty grant)*. `home`, `series`, `one_series`, `work`, `search`, `cover` and `status` each call `state.library.conn()` in the handler body, so a statement that is slow, or a mutex another caller holds (O42's one connection), holds a runtime worker for as long as it waits. Read, not measured: no stall test covers these routes, and none has been timed against a real library under concurrent requests. O12's failure on the routes spec C1 added. **Waits on a measurement**: O71's watchdog pointed at one of these handlers would say whether it stalls, and O42's would say for how long it matters *(the first was taken for all seven and the second was not: a wait on the async worker holds that thread however long it lasts, and O115 and O119 moved the same wait in ten handlers without timing it)*. **Fixed by `library_api::on_the_library`**, which runs a handler's read on the blocking pool over the library's connection and answers a panic in it with a 500 naming what was being read. Each handler's body moved into it whole: `series` and `one_series` ask both their questions in one trip, and `cover` builds its redirect inside the trip rather than after dropping the connection. **Nothing else in `taiyaku-serve` takes that mutex on the async worker**, placed again on `a57fde4` rather than taken from O119's table (`docs/notes.md`, "O116"): of the other 21 calls outside a `spawn_blocking`, 16 are in synchronous functions reached only from inside one, 4 in `serve` before the listener binds, and 1 in `works::from_library` at startup. **One test**, `library_api::tests::no_browse_route_stalls_the_runtime`, calls all seven directly under `the_runtime_stalls_on`, as a reader granted root 1 and with an empty grant, over `test_support::router_over_a_scanned_work_in_a_series` with every member moved under root 1, asserts each answered 200, or 307 from `cover`, and names every call that stalled. On `a57fde4` it failed naming all fourteen calls, each having answered as asserted. **Watched red** (`docs/notes.md`, "O116"), each applied to the final code with the whole workspace's tests run and the file restored and compared: each handler back on the async worker failed that test alone, naming that handler for both grants, 2169 passing against 2170 for the control; `on_the_library` without `spawn_blocking` failed it naming all fourteen calls; and the test's members left under no root failed it at `one_series`, 404 for 200. Run alone 20 times at a one-minute load average of 9.27–9.81, the new test passed 20 times. **What remains**: the watchdog sees a call's first wait on the library's mutex, as for O115, so a second wait added on the async worker after a trip would pass; no test reaches `on_the_library`'s panic arm; and O42's one connection is unchanged, so a browse request still waits for the mutex, on a blocking thread | `library_api::{home, series, one_series, work, search, cover, status}`, `library_api::on_the_library`, `library_api/tests.rs::no_browse_route_stalls_the_runtime` | reading O71's handlers beside their neighbours, 2026-09-14; closed 2026-09-15 |
| O117 | [x] | **Closed 2026-09-15, once the collision had been reproduced on purpose and every test's temp directory came from one helper. `taiyaku_read::test_support::TempDir` names a directory for the process and the call and removes it on `Drop`, and a standing check fails on any line under `crates/`, outside the helper's own files and comments, that calls `temp_dir()`.** **Tests write fixed names under `std::env::temp_dir()`, so two checkouts running the suites at once can delete each other's files.** Test files across `crates/` call `std::env::temp_dir().join(…)` with a literal name on 83 lines and with `format!` on 25 *(and the three standing corpus modules on five more, test code too and moved with the rest. The count is not the size of the work: over 160 calls to `test_support::work` and its siblings, 24 local helpers handed a name, and five covers directories placed beside a fixture with `with_file_name` under fixed names that no grep for `temp_dir()` finds)*, and `taiyaku-serve`'s `test_support::work(dir)` removes and recreates the fixed directory its caller names (`taiyaku-test-composite`, `taiyaku-test-source`, …). Seen twice on 2026-09-14 while five branches ran their suites on one machine (`docs/notes.md`, "O70" and "O78"): six `taiyaku-cli` `tests::serve_works_*` tests failed with `disk I/O error` over names such as `taiyaku-test-serve-works-single.db`, and `auth::tests::every_route_refuses_an_anonymous_request` failed at `test_support.rs:23` with `NotFound`. Each passed alone; neither was reproduced on purpose, so the collision is the one cause the two share rather than a measured one *(reproduced on purpose on 2026-09-15: two copies of one binary sharing a `TMPDIR` failed in 10 attempts of 10 for `serve_works_`, 6 of 10 for that test and 10 of 10 for all of `taiyaku-read`, with those messages)*. CI runs one checkout and cannot see it, and a red run in a parallel session reads as the change's fault. `taiyaku-read`'s `format/tests.rs` already has a `TempDir` that cleans up on `Drop` *(one of four: `taiyaku-library`'s `tests.rs` and `export/tests.rs` and `taiyaku-serve`'s `Fixture` had the same, and all four are the shared type now)*. **A defect with a known fix**: one helper handing out a directory unique to the process and test. **Fixed by `TempDir::new(label)`**: `taiyaku-test-{label}-{pid}-{n}` under the temp dir, dereferencing to `Path`, in `taiyaku-read`'s `test_support` behind the `test-support` feature, which `taiyaku-serve` and `taiyaku-cli` now enable as `taiyaku-library` did. The 138 lines naming `temp_dir()` in 32 files are five, all in the helper's own two files; `test_support::cleanup` and its 156 calls are gone, since the guard removes the directory and a panicking test never reached the call; the scanned fixtures whose own directory was the scan root moved the tree to `tree/` and covers to `covers/`. **Five tests**: two directories under one label are distinct and exist, one is removed with its contents when dropped, its name carries the process id, a label holding `/` is refused, and `nothing_outside_this_helper_calls_temp_dir`. **Watched red** (`docs/notes.md`, "O117"): a fixed name failed 284 tests, no process id 253 and no counter 16, each including the helper test that names the loss; an empty `Drop` failed 3; a `temp_dir()` call added back and a walk that reads nothing each failed the standing check, which named the line and the count. The same thirty paired runs then passed, and left nothing behind. **What remains**: removing a stale directory before creating one survived its mutation and is unpinned; the standing check sees the call and not a fixed name built another way; cargo-mutants excludes the helper's file | `taiyaku-cli/src/tests.rs`'s `serve_works_*`, `taiyaku-serve::test_support::work`, the `temp_dir().join` sites in tests, `taiyaku_read::test_support::TempDir`, `taiyaku_read::test_support::tests::nothing_outside_this_helper_calls_temp_dir` | two parallel suite runs, O70's and O78's, 2026-09-14; closed 2026-09-15 |
| O118 | [ ] | **`library rules --import` replaces the whole rule set, seeded rules included, and does not say which rules it removed.** `rules::import_json` hands the file's rules to `rules::replace_all`, which clears `rule` and writes them back — on purpose, and the function's doc says so. The rule editor's import section says "Import replaces every rule with the document’s"; the CLI prints `imported rules from {path}` and that nothing has been re-derived, and names no count. Met on O78's drive (`docs/notes.md`, "O78"): a file holding only the one fold rule went in through `library rules --import`, the four seeded rules went with the table, and the rescan derived one series and one work and folded nothing; the drive then exported the rules first and imported all five. **A decision, not a defect**: whether an import that removes rules it does not name should say so, refuse without a flag, or stay as documented. What would decide it is whether anyone imports a partial file on purpose | `rules::import_json`, `rules::replace_all`, `taiyaku-cli`'s `library rules --import` | O78's drive over real archives, 2026-09-14 |
| O119 | [x] | **Closed 2026-09-15, the day it was opened, once each OPDS feed had been run with its grant check back on the async worker, by hand, and a test calling it directly had failed naming it. `opds::works` asks through `routes::on_the_grant` and `opds::work_feed` through `routes::visible_editions_or_refuse`, so a grant that names roots is asked on the blocking pool and an empty one still never reaches the library.** **For a reader whose grant names roots, OPDS's `works` and `work_feed` take the library's mutex on the async worker.** `opds::works` reads that reader's visible editions through `opds::visible_editions_by_work`, and `opds::work_feed` through an inline `browse::work`, each calling `state.library.conn()` in the handler before anything runs on the blocking pool: O115's failure, in two handlers that ask a grant through code of their own rather than through `refuse_if_hidden` or `visible_editions_or_refuse`. Measured on O115's branch under O71's watchdog (`docs/notes.md`, "O115", "Found beside it, not changed"): each stalled with `roots: vec![1]`, and neither with an empty grant. **A defect with a known fix**: ask both through `routes::on_the_grant`. **Fixed by** `opds::visible_editions_by_work` asking through `routes::on_the_grant`, and `opds::work_feed` asking `routes::visible_editions_or_refuse`, which answers the same `browse::work` question with the same 404 and 500; both helpers are `pub(crate)` now. Each feed asks at the point it did, and neither takes the library after its check. **Every other place `taiyaku-serve` takes the library's mutex or reads a grant's roots was read** (`docs/notes.md`, "O119"): each runs inside `spawn_blocking`, before the server binds, or without that mutex, apart from `library_api.rs`'s seven handlers, which are O116, so nothing beyond the two feeds changed. **One test**, `routes::tests::neither_opds_feed_asks_a_grant_on_the_async_worker`, calls both feeds directly under `the_runtime_stalls_on`, as a reader granted root 1 and as one with an empty grant, over `test_support::state_whose_grant_waits_on_the_library`, asserts every call answered 200, and names every call that stalled. On O115's branch (`67f2f5b`) it failed naming both feeds for root 1, all four calls answering 200. **Watched red** (`docs/notes.md`, "O119"), each applied to the final code with the whole workspace's tests run and the file restored and compared: `works`' check back on the async worker failed that test alone, naming `opds::works, granted root 1`, and `work_feed`'s named `opds::work_feed, granted root 1`, 2156 passing against 2157 for the control; `works` asking no grant at all failed `auth::tests::every_route_declares_its_grant_posture` and the two `opds::tests` that hold its filter, and `work_feed` asking none failed that test and the two that hold its. Run alone 20 times at a one-minute load average of 21–23, the new test passed 20 times. **What remains**: the watchdog sees a call's first wait on the library's mutex, O115's limit, so a second wait added on the async worker after either feed's check would pass; and `library_api.rs`'s handlers take that mutex on the async worker whatever the grant, which is O116 *(closed 2026-09-15)* | `opds::works`, `opds::work_feed`, `opds::visible_editions_by_work`, `routes::on_the_grant`, `routes::visible_editions_or_refuse`, `routes/tests.rs::neither_opds_feed_asks_a_grant_on_the_async_worker` | the O115 branch's watchdog run beside its own row, 2026-09-15; closed 2026-09-15 |
| O120 | [x] | **Closed 2026-09-15, the day it was opened, once the feed had been run with its `stat` back on the async worker, by hand, and a test asking the kernel which thread made the call had failed naming that thread. `opds::work_feed` asks each edition's acquisition type through `opds::advertised_types`, one trip to the blocking pool, and asks nothing for a reader who may not download.** **`opds::work_feed` asks the filesystem on the async worker.** Its acquisition link declares what `download::advertised_type` answers, and for an edition of one member that answer is `download::passthrough_extension`'s `is_file()`: a `stat` in the handler, once for each such edition the feed names, after the grant check O119 moved to the blocking pool. O119 recorded it as not the library's mutex and not measured, O116 recorded it again, and neither changed it (`docs/notes.md`, "O119" and "O116", "What remains"). This server's host bind-mounts its library from a FUSE share, where nothing promises a `stat` is quick; that is the reason, not a measurement, and it has not been timed there. **A defect with a known fix**: ask on the blocking pool. **Fixed by** `opds::advertised_types`, which answers the type of every edition the feed names in one `spawn_blocking`, over the sidecar snapshot the feed is built from, and answers a panic with a 500 naming what was being asked; the feed's loop reads its answers. **The rest of the handler was read** (`docs/notes.md`, "O120"): its `work.sidecar()` waits on `Work`'s `RwLock` while `patch_alignments` holds it for writing across the sidecar's file write and its rows, the same wait `opds::works`, `get_sidecar`, `get_page` and `opds::stream_page` have, and it is not changed here. *(Four, not five: `get_page` takes that read inside its own `spawn_blocking`, so it is not among them. Counted again from the code while filing O122, 2026-09-15.)* **Every filesystem call in `taiyaku-serve` outside its tests was placed** the same way: apart from this one, each runs inside a `spawn_blocking`, on a download's own thread, in `serve` before the listener binds, or only from tests, except `reqlog::Log::finish`, which writes each request's line on the async worker while `--log-requests` is on. **One test**, `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool`, runs itself again under `strace` through `taiyaku_read::test_support::strace`, split out of O104's `traced` for it, calls the feed on a current-thread runtime for a work of two one-member editions, requires each member to have been statted and only by a thread that runtime started, and requires each entry to declare its own edition's type. **Watched red** (`docs/notes.md`, "O120"), each applied to the final code with `taiyaku-serve`'s tests run and the file restored and compared: the handler as it was on `main` failed that test alone, naming the thread that statted `volume.pdf` as the async worker with an empty blocking pool, 585 passing against 586 for the control; `advertised_types` calling its closure in place of `spawn_blocking` failed it the same way; the loop declaring `CBZ_TYPE` for every edition, and every edition typed as the first, each failed it at `raw`'s link. Run alone 20 times at a one-minute load average of 15.37–15.58, it passed 20 times. **What remains**: `work.sidecar()`'s wait above, in five handlers; `reqlog::Log::finish`'s write; no test reaches `advertised_types`' panic arm or its 404, sees that an edition outside the grant is not statted, or sees that a reader who may not download makes no trip; and nothing here has been timed, on this workstation or on the NAS | `opds::work_feed`, `opds::advertised_types`, `download::advertised_type`, `opds/tests.rs::the_work_feed_stats_its_members_on_the_blocking_pool`, `taiyaku_read::test_support::strace` | what O119 and O116 recorded as remaining, 2026-09-15; closed 2026-09-15 |
| O121 | [x] | **Closed 2026-09-15, on the branch that opened it and together with O76, because one mechanism closes both. A fold carried a reader's place and note as slot numbers, and a slot only names a page inside one edition's spread flags and reading direction.** `fold::move_positions` and `fold::move_notes` copied `progress.slot` and `note.slot` unchanged onto the twin, and a twin a scan has just built holds no spread flags — `derive` builds every edition with none — so past a spread on the folded edition the number named a later page, and the fold's own counters said so nowhere: the drive below printed `1 position(s) written, 0 position(s) lost, 1 note(s) and 0 truth page(s) moved, 0 identical note(s) dropped, 0 truth page(s) already held` — the place and the note counted as moved, and `lost_positions`, `dropped_notes` and `collisions`, the three that report a loss, all zero. **Found by checking the premise O76's fix was briefed on**, not by a review. **Measured on `main`**: `fold::tests::a_carried_position_and_note_name_the_page_they_named_before_the_fold` failed there, the place on page 4 landing on page 5 and the note on the left half of page 2 on page 3; and over real archives (`docs/notes.md`, "O76 and O121") a place on page 50 and a note on the left half of page 4, on a Go-Toubun v01 edition with seven spreads, came back after the fold as page 54 and page 5. **Fixed by** `fold::plan_slots`: the twin takes the folded edition's spread flags where it holds none, a slot moves as the page and half it named — a half by its side of the scan, so across reading directions too — the twin's own rows are placed under the flags it takes, and the fold is refused over a place or a note on either work, and over an alignment the surviving work already holds, that would name another page. Refused rather than counted, because a place on the wrong page with a number beside it cannot be undone the way a standing duplicate can. The same test passes on the branch, and the same drive brought both back on page 50 and the left half of page 4. **What remains**: none of O121's own; the refusals it adds, and the unmeasured `library import` over a surviving work that could drop the flags it placed rows under, are O76's. | `fold::plan_slots`, `fold::move_positions`, `fold::move_notes`, `Edition::slots` | checking the premise O76's fix was briefed on, 2026-09-15 |
| O122 | [ ] | **`work.sidecar()` waits on the work's lock on the async worker, in four handlers.** `works::Work::sidecar` takes the work's `RwLock` for reading and `works::Work::update` holds it for writing across a whole correction — `routes::patch_alignments` runs `taiyaku_read::atomic::write` of the sidecar file *inside* that closure — so a reader that waits on it waits on a file write, not on a lock held for a memory swap. Four handlers take the read outside a `spawn_blocking`: `opds::works`, `opds::work_feed`, `routes::get_sidecar`, and `opds::stream_page` through `routes::locate_member`. *O120's row says five and names `get_page` among them. `get_page` calls `locate_member` inside its own `spawn_blocking`, so it is not one — marked here rather than rewritten, and counted again from the code on 2026-09-15.* *(That correction was itself wrong: `get_page` runs `work` and `locate_member` on the async worker, before its `spawn_blocking`, so that its `ETag` can answer a `304` without one — the page route's O1 comment, now in `routes::serve_page`, says so. Since O133 `get_thumb` does the same for a library cover. So it is six: these four, `get_page` and `get_thumb`'s cover sizes. Found by the review of O133's fix, 2026-09-19.)* Same family as O115, O116, O119 and O120, and the same fix shape: ask on the blocking pool. **Unmeasured**: how long a correction holds the write lock on the host this serves from, and whether a reader ever meets one — `patch_alignments` is one person dragging a filmstrip tile, so the window is rare by construction. *Since O183, a refresh also holds each work's write lock while it reads that work back from the library, so a reader on the async worker can wait on it too. Unmeasured, like the correction's.* | `works::Work::sidecar`, `works::Work::update`, `opds::works`, `opds::work_feed`, `routes::get_sidecar`, `opds::stream_page`, `routes::serve_page` (`get_page`, and `get_thumb`'s cover sizes) | the O120 branch's own filesystem sweep, 2026-09-15 |
| O123 | [ ] | **The request log writes each line to an unbuffered file on the async worker.** `reqlog::Log` holds a `Mutex<File>` and `reqlog::Log::finish` calls `write_all` on it with no buffering, from `build_router`'s logging middleware — a plain `axum::middleware::from_fn` that awaits `next.run(request)` and then writes — so every logged request pays a `write` syscall on a runtime worker thread. It happens only under `--log-requests`, which this project turns on for a measurement session rather than for ordinary serving, and the line is one JSON object per request. The fix shape is O120's, either the write on the blocking pool or a buffer flushed off it. **Unmeasured**: what the write costs on the NAS this deploys to, where the log would sit on the same share as the library — and the sessions this log exists for are exactly when the cost would be paid | `reqlog::Log::finish`, `reqlog::Log`'s `file`, `routes::build_router`'s request-log middleware | the O120 branch's own filesystem sweep, 2026-09-15 |
| O124 | [ ] | **`library import` drops the spread flags a fold wrote, and the rows placed under them then name other pages.** Under `Scope::Document`, `import::import_with` deletes every edition of the work — `ON DELETE CASCADE` takes `page_flag` with it — and re-inserts flags only from the document's own `spreads` and `credits`; `held_flags` is deliberately empty for that scope, where `Scope::Scan` carries what the library already held. So importing a `taiyaku.json` that lists no spreads over a work a fold has touched removes the flags O76's fold copied onto the twin, and every position, note and alignment O121's placement put under them names a different page afterwards — the defect O121 closed, reached from the other side. Recorded as unmeasured in O76's own row and given a number here, because a closed row is not a tracker. **What would decide it**: importing a document over a work a fold has touched, and reading a position back | `import::import_with`'s `Scope::Document` arm, `import::reimport`, `fold::plan_slots` | the whole-branch review of O76, 2026-09-15 |
| O125 | [ ] | **`taiyaku library proposals` can reopen a dismissal it cannot list.** The command's listing goes through `proposals::open` alone, so it shows open proposals at or above `--min-score` and nothing else, while `--reopen ID` — added by O72 — acts only on a dismissed one. A person at a terminal has no way to learn those ids from the CLI: they read them off the web inbox's Dismissed filter, or out of the database. `proposals::list` already takes a `Filter::state`, which is what the web screen sends, so the fix is a flag over a filter that exists rather than new machinery. Recorded in O72's What remains and given a number here for the same reason as O124 | `taiyaku-cli`'s `library proposals`, `proposals::open`, `proposals::list`'s `Filter::state` | O72's own report, 2026-09-15 |
| O126 | [ ] | **`raise_edition_of` proposes only a `" - "` suffix, so a parenthesised edition qualifier is never offered as an edition.** Since O89, a directory that qualifies its filenames' series is a series of its own, and the curation inbox's `edition-of` proposal is how it gets folded back with an edition key. But `raise_edition_of` requires a `sort_title` that is another series' title plus `" - "`, so `Death Note (Colored Council)`, 9 volumes in the real library, is correct and unlinked: two series, neither proposed as an edition of the other. **Predicted absent in O89's spec §1.2, and measured absent on 2026-09-16.** `Parasyte - Full Color (Colored Council)` and `One Piece - Definitive Edition (Colored)` are proposed only because their qualifier also carries a `" - "`. **A decision, not a patch**: reading a parenthesised qualifier as a suffix widens what the function matches, and how wide it can go before it proposes a series that merely ends in parentheses is the question. **What would decide it**: the real library's parenthesised directories, listed and judged, before the match changes | `proposals::raise_edition_of` | O89's real-library re-derive, 2026-09-16 |
| O127 | [ ] | **An open proposal whose subject series holds no work stays open.** Derivation deliberately keeps a series row whose works have all gone (`re_deriving_keeps_a_series_row_so_a_dismissal_still_points_at_something`), and nothing closes or marks the proposals that name one, so the inbox offers to fold a series with nothing in it. **Measured on the real library, 2026-09-16:** two such proposals before O89 (ids 19 and 29, both `One Piece - Cover Stories - Digital Colored Comics`), and five after it. O89's three renames, `One Piece - Definitive Edition`, `Parasyte - Full Color` and `Shingeki no Kyojin - Kui Naki Sentaku`, left their old-name proposals open beside new ones for the new names. **Pre-existing**: O89 raised the count, and the shape was already there. **Read, not executed** (O89's whole-branch review): accepting one from the inbox is refused while its rule is derived, since no scanned path is left for the pattern to span, and a rule written by hand through `accept_with` is refused because it reaches no live subject — so the harm is clutter and a refusal whose message does not name the cause, never a wrong rule | `proposals::raise`, the series rows `derive` keeps | O89's real-library re-derive, 2026-09-16 |
| O128 | [ ] | **The rule editor's Overrides tab can credit a rule with a value it never produced.** This happens when a user series rule sits between a directory rule and a later capture, over a directory that qualifies the captured series. `preview::overrides` builds engines over growing prefixes of the rule list and credits each step to the rule just added. Since O89, adding a capturing rule can also trigger `rules::Engine::resolve`'s reconciliation, which restores the directory rule's series, so the step is credited to the capture with a value the directory rule produced. **Reproduced on 2026-09-16 with one scratch program built against O89's branch and against `main`.** The setup is the four seeded rules plus a user `name` rule at ordinal 3, over `Death Note (Colored Council)/Death Note - v01 (Colored Council).cbz`. The user rule was either a prefix literal `Shinigami Notebook` or a capture `^Death Note - (?<series>v\d+)`. On `main` the row is truthful: rule 4 wins with `Death Note`, and `others` is 1. On the branch the row names rule 4 as the winner with `Death Note (Colored Council)`, and `others` is 0, while `resolve` and the Provenance tab credit rule 1. **A regression O89 introduced, and unreachable with only the seeded rules and the rules `accept` derives**, though `accept_with` takes a hand-written rule of any kind, so a widened accept could reach it. **A decision, not a patch.** Crediting each step to the rule provenance names makes the row truthful, but its reorder option would then offer to move rule 1 above a rule it already precedes, so what the row should offer is the open question. `preview.rs`'s chain doc is marked in place, and `docs/notes.md`, "O89", has the reproduction | `preview::overrides`' chain attribution, `rules::Engine::resolve`'s reconciliation, `PreviewTabs.tsx`'s override row | O89's whole-branch review, 2026-09-16 |
| O129 | [ ] | **No test sees a generation permit released as soon as it is taken.** Bound to `_` rather than to a name, as in `let _ = GENERATION_LIMIT.acquire();`, the guard drops at the end of its own statement: the call still waits for a free permit, and the decode after it then runs holding none, so the bound `MAX_CONCURRENT_GENERATIONS` puts on decode memory is gone. Every test waits on the acquire, and none can see the release. **Measured on 2026-09-17 while closing O111** (`docs/notes.md`, "O111", "Every break, watched"): with `cached_or_generated_page`'s guard turned so, all 593 `taiyaku-serve` tests passed, and cargo-mutants makes no mutant of the permit at all. `cover::at_path` takes its permit the same way and was not mutated. **What would catch it**: a generation held part-way through inside its permit while a test counts the permits left. `at_path` fetches inside its permit, so a fetch closure could be held there; `cached_or_generated_page`'s resize takes no closure, so holding it mid-decode needs a seam that does not exist | `cover::cached_or_generated_page`, `cover::at_path`, `GENERATION_LIMIT` | a hand mutation while closing O111, 2026-09-17 |
| O130 | [ ] | **taiyaku reads no JPEG XL page, so an archive of `.jxl` pages is never a member.** `taiyaku-read`'s `IMAGE_EXTENSIONS` does not list `jxl`, so `member_pages` finds no page in such an archive and refuses it, the walk stores its `pages` as NULL, and derivation reports it under `Unreadable::uncounted`. The `image` crate taiyaku decodes with, 0.25.9, has no JPEG XL codec; its `hooks` module is the only place it names the format. **Found while closing O90** (`docs/notes.md`, "O90"): the one entry a real library still counted unreadable after its RAR volumes were repacked was `Bisque Doll de Chu ♡.cbz`, a zip that lists cleanly and holds 185 lossy JPEG XL pages. **That file was converted to PNG on 2026-09-17**, outside taiyaku, with the original kept as a backup outside the scan root. Every page was pixel-identical to its `djxl` decode, the archive went from 168 MB to 433 MB, and a rescan of a copy of the library read it as a member of 185 pages, taking works from 1895 to 1896 with nothing left unreadable. **Deferred by the person's decision** on 2026-09-17: reading JPEG XL is a future feature. **What would decide it**: a decoder checked against GPL-3.0-or-later for licence and against `taiyaku-read`'s place for maturity, as O90's design §1 did for RAR, and a library where converting is not available | `taiyaku_read`'s `IMAGE_EXTENSIONS`, `taiyaku_read::member_pages` | closing O90, 2026-09-17 |
| O131 | [ ] | **The walk keeps no reason for a NULL page count, so an archive that would not open and one whose pages taiyaku does not read are reported alike.** `scan::pages_of` calls `taiyaku_read::member_pages(…).ok()` and drops the refusal, and the refusal is what tells the two apart: `no page images` for an archive that opened, an open or parse error for one that did not. So `Unreadable::uncounted` names the file and not the cause, under the one label "whose pages could not be counted", and a person has to open the archive to learn whether it is broken or in a format taiyaku does not read. **Found while closing O90**, whose one real case was the second kind (O130). **What would decide it**: whether the reason earns a `scan_entry` column. `pages_of` swallows the refusal so that one bad file cannot fail a library's scan, and storing the message would not undo that | `scan::pages_of`, `derive::Unreadable::uncounted` | closing O90, 2026-09-17 |
| O132 | [ ] | **The continue strip shows no reading progress.** Each card draws a volume's cover, title and number (style pass ②), not where the reader is in it: the mockup the user chose on 2026-09-17 drew "Vol. 1 · page 8" and a bar, but `progress` holds a `slot` per edition, and nothing turns a slot into a page for `/api/library/home`. **The user wants both, chosen by a setting** (2026-09-18): the browse module's `continueStrip` checkbox becoming *Off / Covers / Progress*. What it needs: the home route sending each continuing work's page and page count in the edition it opens — resolved from the slot through that edition's spread flags and its work's reading direction, as the reader resolves it — and the card drawing them | `browse::home`, `library_api.rs`'s `/api/library/home`, `Library.tsx`'s `ContinueCard`, `browseModule.ts`'s `continueStrip` | the style pass's ② design, 2026-09-18 |
| O133 | [x] | **Closed 2026-09-19, the day after it was opened, once the user's iPhone had fetched every cover at 768 px on its long edge and the user had found them sharp, and an account that may download but not stream had been served every cover size. A library cover is offered at 256, 512 and 768 px — `library.ts`'s `coverSources`, one size per screen density, drawn by `Cover.tsx` — through the tile route, which takes `?w=` for page 0 at those sizes and no others, and the series cover route passes a cover size through to it.** **Every cover in the library is the reader's filmstrip thumbnail, so covers are blurry on a phone.** `/api/library/series/{id}/cover` redirects to `/api/works/{slug}/{edition}/pages/0/thumb`, and the web draws that same route for a series page's volume tiles and the continue strip's cards (`library.ts`'s `workCover`). Its long edge is `cover.rs`'s `THUMB_LONG_EDGE`, 160 — "sized for the strip rather than for a grid" — so a cover arrives 160 px on its long edge, 113×160 for a typical one. A phone's two-column grid draws it at about 171×256 CSS px, 513×768 device pixels at 3×: about 4.8× upscaled, and visibly blurry on the user's iPhone (2026-09-18); a 1440 px desktop at 1× draws it at about 152×228, 1.4×. `cover.rs` already makes a 512 px cover (`LONG_EDGE`), for OPDS feeds only. Not caused by the style pass; its phone tiles made it obvious. **The user decided** (2026-09-18) to fix it in its own PR after ②. What would settle it: a cover size for the web — 512 px or more, for a 3× phone — through the cover path and its cache, and the web's covers drawn from it. **Fixed by** `coverSources` and `Cover.tsx`'s `srcSet` (`256 1x, 512 2x, 768 3x`) over `workCover`'s tile URL, and the tile route (`routes.rs::get_thumb`) answering `?w=` for page 0 at one of `cover::COVER_LONG_EDGES` through `serve_page` — the page route's own body, moved out of `get_page` so both call it — with the resize chosen for line art, a cache file per size and the page route's `ETag`, but without `refuse_if_cannot_stream`: any other page, size or `w` gets the 160 px tile (`cover_edge`), so a cover is no way to read a page an account may not stream. `library_api.rs::cover` redirects a cover size to `/pages/0/thumb?w=`, and anything else to the tile; both routes read the first `w` and ignore one that is not a number, rather than answering `400` (`routes::first_query_value`). **The first version sent covers through the page route**, which refuses an account without `can_stream`, so every cover on such an account's shelf would have been a placeholder; the review of that version found it by measuring such an account, and the gate was green, because the one test of that policy (`download/tests.rs`) asks only for the unsized tile. Measured (`docs/notes.md`, "O133"): Berserk v01's page 0 comes back 181×256, 362×512 and 543×768; every one of the user's iPhone's 42 cover requests asked for `w=768`; a download-only account on a copy of the scratch library gets every cover size and a `403` for a page. **What remains**: a cold cover costs a full resize the first time each size is asked for, timed in `docs/notes.md`; the cache keeps every size of every cover it has made, about 100 KB at 768 px against about 7 KB for a tile, and nothing evicts it; and the filmstrip, and the proposals inbox's page tiles, keep the 160 px tile. *That last clause stopped being true on 2026-09-22: O134 gave both a tile sized per screen, through the same route and the same `?w=` this row built — gated on `can_stream`, since a tile rung reaches any page where a cover size reaches only page 0. The credits panel still takes the 160 px tile, deliberately. The item stays closed and its prose above stays as written, per the ritual; this sentence marks what overtook one clause of it.* | `library.ts`'s `coverSources` and `workCover`, `Cover.tsx`, `routes.rs`'s `get_thumb`, `cover_edge`, `serve_page` and `first_query_value`, `cover.rs`'s `COVER_LONG_EDGES`, `library_api.rs`'s `cover` | the style pass's ② device pass, 2026-09-18; closed 2026-09-19 |
| O134 | [x] | **Closed 2026-09-22, once the user's own iPhone had been served the strip and the tiles it asked for had been counted off the cache.** A filmstrip tile is offered at 160, 320 and 480 px on its long edge (`cover.rs`'s `TILE_LONG_EDGES`), one per screen density, and the client asks for the rung its own box needs: `tileSize.ts`'s `requestedTileEdge` rounds `drawn height x devicePixelRatio` up the ladder, `Filmstrip.tsx` reads it once per mount through `stripTileEdge` before the first render, and `ProposalRow.tsx` passes its fixed 108 px box. `thumbUrl` carries the rung as `?w=`. **The line the route draws**: `routes.rs`'s `tile_edge` honours a rung on **any** page but only for a caller `can_stream` answers `true` for, and everything else — an unknown `w`, a non-numeric one, an account that may browse and download but not read — falls back to the 160 px tile rather than a refusal, the same fallback `cover_edge` already made. O133's ungated surface is untouched: page 0 at a cover size, for an account without `can_stream`, exactly as before. The two ladders are disjoint and a `const _` block in `cover.rs` fails the build if they ever stop being. **What it costs** (`docs/notes.md`, "O134: what a sharper tile costs"): on `akame-ga-kill----v01--2015--cbz`, 237 tiles are 1.57 MiB at 160 px, **5.18 MiB at 320 (3.29x)** and **10.52 MiB at 480 (6.69x)**, each size cached only if some screen asks for it. That corrects the design's pre-measurement guess, as a ratio, of 3.21x and 6.43x; the shape it assumed (bytes as area^0.85) held, and the ratios came in 2 and 4% higher. Its absolute predictions were for a 195-page edition and were never re-measured — this is a 237-page one. **The device pass** (`docs/notes.md`, "O134: the device pass"): the production build over the tailnet on the user's iPhone 17 Pro Max wrote **40 tiles, every one of them 320 px, and none at 160** — 26 on the first open, which is the same first-open count O140 measured for a windowed rail, arriving here on real hardware. **What the user could actually see is the honest part of this row**: asked whether the strip looked sharp they said *"…it's hard to tell on a phone because the thumbnails are so tiny. I can't really differentiate so I would say they are sharp enough as is."* The upscaling is gone by measurement — 288 device pixels drawn from a 320 px image where a 160 px image used to be stretched 1.8x — and it is **not a difference this reader could pick out**. This row closes on the measurement and on "sharp enough", not on a visible improvement, and the next proposal of the same shape should be asked to show the difference is visible before it is asked to show it is cheap. **What remains**: nothing evicts the tile cache, and it now holds a file per size per page (O11); `cover.rs`'s `thumbnail_at` **upscales**, so a `w` above a short page's own long edge caches a file larger and blurrier than the source; and `ProposalRow.tsx`'s `TILE_HEIGHT_PX` is pinned by no test — at `devicePixelRatio` 1 every height from 1 to 160 rounds to the same rung — where the strip's own height cannot drift from `reader.css`, because `reader.css.test.ts` parses the stylesheet and pins it. The credits panel keeps the 160 px tile deliberately: its thumbnails are 56 px tall, so even a 3x screen wants only 168, and the next rung would fetch 320 to close an 8 px gap | `cover.rs`'s `TILE_LONG_EDGES` and `thumbnail_at`, `routes.rs`'s `tile_edge` and `can_stream`, `tileSize.ts`, `Filmstrip.tsx`'s `tileEdge`, `ProposalRow.tsx`'s `TILE_HEIGHT_PX`, `api.ts`'s `thumbUrl` | the style pass's ③ design, 2026-09-19; closed on its own branch, 2026-09-22 |
| O135 | [x] | **Closed 2026-10-01 on the accessibility pass's reader branch, once the keyboard had been walked with real keys in Chromium and by the user in Firefox on their desktop.** `Tab` now steps round a ring the reader owns — the page, Library, the three layouts, ⚙, the strip's current tile, whatever the page itself offers, and the page again — with the bars up while focus is on one of theirs (`ring.ts`, `focus.ts`, `App.tsx`'s `ringMove`); `Escape` hides them; `←` and `→` turn the page from any bar button; `Escape` closes Settings, and so does `s` once ⚙ has opened it (spec 2026-09-26 §3.1–§3.3). Held by `App.browser.test.tsx`'s real-key walks at three sizes and its every-stop cases, each watched failing against its defect; walked on the real library (`docs/notes.md`, "The reader's keyboard and screen-reader access") and by the user in Firefox 157 on 2026-10-01 — *"yes everything behaves like that"*, with two progress writes from that browser in the request log. **Not done here**: VoiceOver on the device, which the user set aside for a tester who knows it (O162). **For the accessibility pass: the keyboard reaches no control in the reader's top bar, and Settings closes on neither `Escape` nor `s` once ⚙ has opened it.** `keyPlan` answers `Tab` with `toggle-chrome` and leaves its focus move to the browser, so one `Tab` shows the bars and the next hides them. **Measured in Chromium at 1440×900 with real keys, on this branch and on `main` alike**: the first `Tab` focuses the strip tile after the current one, not any top-bar control; `Space` then activates that tile and turns a page rather than swapping editions; a second `Tab` returns focus to `<body>`. Every top-bar control has another keyboard route — `L`, `s`, and Layout inside Settings — so nothing is out of reach. Settings, a dialog with a Close button since style pass ③, has no `Escape` arm in `keyPlan`'s `Escape` branch, where Help and the notes panel do; and after ⚙ opens it the click leaves focus on ⚙, so `keyPlan`'s interactive-target guard stands `s` down too — Close is the way out; a click outside only takes focus off ⚙, after which `s` closes it again. Left to the accessibility pass the user put after ⑤ (spec 2026-09-17 §5). **Narrowed by ④ on 2026-09-20**: Settings now dismisses on a press that goes down and comes up on its own scrim, so a press outside it dismisses it as it does every other panel — its Close button was already its pointer exit, since style pass ③, as this row's own text says above. The keyboard half is untouched. **And sharpened by the user the same day**, reading on a desktop and quoted in `docs/notes.md`: *"I can press it to close the menu, but then after that tab highlights stuff in my browser and starts moving around there outside of the in-page content"*. The mechanism is `keyPlan` returning `preventDefault: false` for `Tab` deliberately, so a press toggles the bars *and* lets the browser move focus, which walks out of the document. Offered a dedicated key instead, they chose to leave it to this pass. Preventing the default would stop that and lock the keyboard out of every control inside the page, which is the redesign this row is waiting for rather than a patch it is missing. **Measured again by the accessibility audit on 2026-09-26, and the head of this row is not quite right**: at 1440 and 390 alike, `Shift+Tab` from a fresh load shows the chrome and focuses ⚙, so a key reaches the top bar's last control and none of the others (`docs/notes.md`, "The accessibility audit"). **On the user's iPhone on 2026-09-30, a VoiceOver double-tap on the page brought the bars up**, after which VoiceOver read a layout button as *"single, toggle button, pressed"* and ⚙ was double-tapped, so VoiceOver reaches the top bar by a route the keyboard does not have. Credits' missing `Escape`, which this row did not name, is O161 | `keyplan.ts`'s `keyPlan` (`Tab`'s `ring`, the `Escape` branch, `controlKeeps`), `ring.ts`, `focus.ts`, `App.tsx`'s `ringMove`, `TopBar.tsx`, `Settings.tsx` | the style pass's ③ design, 2026-09-19 |
| O136 | [ ] | **In scroll mode, taking a page from the strip moves the reader's position but not the view.** Measured in the running app at 1440×900 (`docs/notes.md`, "Style pass ③"): with the chrome shown in scroll mode, clicking the tile for slot 10 left the readout reading `slot 10 / 207 · p. 10` while `scrollTop` stayed at 9,803 with pages 4 and 5 on screen. **Measured the same way on `main`**, served beside the branch against the same server: the readout reads slot 10 and `scrollTop` stays 0, so this predates style pass ③ rather than coming from it. `useScrollToCurrent` (`hooks.ts`) calls `scrollIntoView({ block: 'start' })` on the current group whenever `[mode, authority, slot, viewing]` changes, and in scroll mode every group is mounted at once, so the likely cause is the target's own height not being known when the effect runs — the same shape `Filmstrip`'s own centring comment records for lazily-sized tiles — but that was not proven. **What would settle it**: re-running the click with the pages already decoded, and with a `load`-driven re-scroll like `Filmstrip`'s | `hooks.ts`'s `useScrollToCurrent`, `App.tsx`'s scroll-mode branch and `currentGroupRef` | the style pass's ③ browser pass, 2026-09-19 |
| O137 | [x] | **Closed 2026-09-20 on the style pass's ④ branch, with both exits the user chose — and on a phone in portrait the only pointer route to them runs through Settings' *reveal* checkbox and the note list, driven end to end, since the margin tab that opens an existing note is off screen at 390 px and a new note still needs `n` (O139).** The editor now carries a Close button, 44 px under `(pointer: coarse)` and measured at 44 px tall by 61 wide, beside Delete and disabled by the same `busy` rule; and a press in the margin around the draft's own painted page finishes the note, which is `gestureFor`'s rule 1 firing for the first time — **in two-page mode this is one-sided**: a press in the *facing* cell's letterbox is on an image that is not the draft's own, so rule 2 answers `ignore` for the whole of it regardless of where it falls, and the margin around the facing half looks identical to the margin around the draft's own half without closing the note. Deliberate, and matches `main`. **That second exit had to be built, not measured**: the page `<img>`'s box equals the page area at every size tested and `object-fit: contain` letterboxes the picture *inside* that element, so "on an image" was true of every press in the reader and rule 1 could never fire — `NotePanel`'s own doc had promised that exit since the editor shipped. Measured before, three points at each of two sizes, all six left the editor open; after, the margin closes and the painted page still draws (`docs/notes.md`, "Style pass ④"). The original defect: **with the note editor open, a finger has no way to close it but Delete.** `NotePanel`'s own doc offers two exits — `Escape`, or a click on the background around the page — and a touch screen has neither: a tap on the page area while `panelMode === 'edit'` is region drawing's gesture (`App.tsx`'s `beginRegion`), so it starts a rectangle instead of closing the panel. Reported by the user on their iPhone during style pass ③'s device pass (iOS 18.7, Safari 26.6.1, user-agent read from the request log), where opening a note left Delete as the only way out. **Not style pass ③'s doing**: the panel's exits are as they were. ④ is where a tap becomes a gesture the reader understands, so its design is where this belongs — a Close button on the editor, or a tap outside the panel that closes rather than draws | `NotePanel.tsx`'s exits, `App.tsx`'s `beginRegion` and `panelMode`, spec 2026-09-17 §5 ④ | the style pass's ③ device pass, 2026-09-19 |
| O138 | [x] | **Closed 2026-10-12, by the user's ruling of 2026-10-11 for the first of the three options with the wheel added to it: the rail draws no scrollbar, and the wheel scrolls it.** `Filmstrip.tsx`'s rail wears the class `reader-strip-rail`, and `styles/reader.css` gives that class `scrollbar-width: none` and a `::-webkit-scrollbar { display: none }` rule. Read on the running app in a browser that draws a classic scrollbar, where a plain `overflow: scroll` box beside the rail still lost 15 px: at 1440×900 the rail reads 176 against a client height of 176 where it had read 161, and a tile is 164 tall with its image at 160 where they had been 149 and 145; at 390×844 under desktop emulation the rail reads 112 and 112 where it had read 112 and 97, and a tile is 100 tall with its image at 96 where they had been 85 and 81. Those are the headless shell with `--hide-scrollbars` removed, before and after; Google Chrome 155 with a window gave the same after figures at 1440×900 (`docs/notes.md`, "O57, O60 and O138"). `--strip-unit` now starts at 107 px and 64 px, to the nearest pixel the widths a one-page tile's image drew with nothing taken, and the rail writes no correction over either. A vertical wheel over the rail now scrolls it, toward later pages when it turns down, in either reading direction (`tiles.ts`'s `stripWheel`, and a listener on the rail in `Filmstrip.tsx`). A wheel with Ctrl or Shift held, and a delta that is sideways as much as it is vertical, are left to the browser, which still scrolls the rail with a sideways wheel. **What the ruling costs:** the bar's thumb was a mouse's fast way across a long volume, and a notch of the wheel moves the strip 100 px, about one tile. Where scrollbars overlay, the iPhone among them, the rail gains no height, since it had lost none, and by the same declarations no longer draws the bar that showed while it moved; that was not looked at on a device. **Not measured:** Firefox, which may hand the page pixels where `stripWheel` expects lines and so move the strip less far for a notch; and a wheel under a hand, since every reading used Playwright's wheel or a made event. Both are O216's *(which has since closed, on 2026-10-12, with both taken by the user's hand in Firefox: see its row)*. *As reopened:* **Reopened 2026-10-04 by the user's decision, once O145 had found the subtraction in Google Chrome on this machine.** Google Chrome 154, opened here with a window and without `--hide-scrollbars`, draws a classic scrollbar on the rail and takes 15 px of its height: at 1440×900 the rail reads 176 against a client height of 161, and a tile is 149 tall with its image at 145, where the headless shell with `--hide-scrollbars` draws no scrollbar and they are 164 and 160 (`docs/notes.md`, "O145: what takes 15 px of the rail"). The closing text below says neither this machine's Chromium nor the user's own desktop shows a classic scrollbar; the first was a headless launch carrying `--hide-scrollbars`, and what Firefox draws was not measured. The three options below are unchanged, and none is chosen. *As closed:* **Closed 2026-09-20 on the style pass's ④ branch, by measuring it twice and finding the subtraction is the platform's, not the code's.** In headless Chromium at 1440×900 the rail reads `clientHeight` 176 against `offsetHeight` 176 — the scrollbar takes **0 px** — with a tile button at 164 and its image at 160, which is exactly what spec §8.4 claims. The user then looked on their own desktop: a scrollbar track is visible and the thumbnails are the height the strip is. Both are the overlay case. The 15 px belongs to a **classic** scrollbar, which is a platform setting no code here chooses, and neither this machine's Chromium nor the user's own desktop shows one, while iOS overlays its scrollbars (③'s row) — so none of the three options was worth taking, and the 1:1 claim holds wherever it can be seen. **What this closure does not do is reconcile ③'s own reading.** ③ measured 149 and 145 inside the same 176 px strip, through the same Playwright harness on the same machine, and ④ gets 164 and 160 there — with the rail genuinely overflowing both times, so the condition was present. ④ did not re-run ③'s measurement or establish what differed between them; it closed on the two readings it has, this machine's and the user's own desktop, both of which show the overlay case. If the 15 px returns, that unexplained difference is where to start. **Reopen it** if a reader turns classic scrollbars on and minds. The original defect: **the strip's rail spends 15 px of every tile on its own scrollbar.** The rail is `overflow-x: auto` and a real volume's tiles always overflow it, so a classic scrollbar takes 15 px off its client height: measured in Chromium on the running app, a tile button is 149 px and its image 145 inside the 176 px strip at 1440×900, and 85 and 81 inside the 112 px strip at 390×844 — where spec 2026-09-17 §8.4 claims 160 and 96, "the thumbnail's own height, shown 1:1". The subtraction predates style pass ③ (the rail's `overflow-x` is untouched) and does not happen where scrollbars overlay rather than take space, which includes iOS. **A decision, not a patch**: hiding the scrollbar (`scrollbar-width: none`) buys the 15 px and takes away the one affordance saying the strip scrolls; making the strip taller by the scrollbar's width moves the page area on every platform to pay for one platform's chrome; leaving it costs the 1:1 claim on a desktop. **What the browser test cannot see**: its six-page fixture never overflows the rail, so its tile-height case passes at the full 160/96. *O145 has since closed, on 2026-10-04, having taken both readings again: ③'s came from a browser that draws a classic scrollbar on this machine and ④'s from a launch that hides scrollbars — see its row.* | `Filmstrip.tsx`'s `railStyle`, `styles/reader.css`'s `.reader-strip-rail` and `--strip-unit`, `tiles.ts`'s `stripWheel`, `Chrome.tsx`'s `stripStyle` | the style pass's ③ browser pass, 2026-09-19 |
| O139 | [ ] | **A new note can only be started by `n`, and on a phone held upright the obvious pointer route to an existing one is off screen.** `modules/annotations.ts` registers `n` and nothing else starts a note, so there is no touch route to a *new* one. An *existing* note has two pointer routes — `Annotations.tsx`'s margin tab and `NoteList`'s rows, both of which call `App.tsx`'s `openNote`. The **tab** is the direct one and it works where there is room for it: measured at 1440×900 and at 844×390 a click on it opens the editor, but at 390×844 the painted page fills the width (`[0, 130, 390, 585]`), so the tab is laid out at **x = 394 in a 390 px viewport** — and in `two` mode at x = −22, off the other edge. The **list** needs `annotations.reveal`, which `r` toggles and which the Settings panel writes by the other door: `reveal` is a key of `annotationsModule.defaults`, and `Settings.tsx` generates a control for every key — a checkbox for a boolean one. ⚙ opens Settings by pointer and ④'s middle tap now opens the chrome, **so a portrait route does exist, and it was driven end to end at 390×844**: middle tap, ⚙, scroll the panel to Annotations — *reveal* is the sixth section's third row, laid out at y ≈ 1134 in an 844 px viewport — tap it, dismiss the panel, drop the bars, tap a row. The bars must come down first: with them up the list is `visibility: hidden` and a tap where a row sits hits a filmstrip tile and turns the page. **The way in is smaller than what this row already calls too small**: that checkbox is 13 × 13 px, tappable through its 301 × 33 px label. Reported by the user on their iPhone during style pass ④'s device pass — *"I'm not sure how to make a note on mobile, but it's ok. this is a small feature and we can worry about it later"* (`docs/notes.md`) — and deferred by them. **Also measured on that pass**: inside the editor the five colour swatches are 22 px under `(pointer: coarse)`, half the 44 px taken by the two buttons in the row below them, so the panel needs a touch pass and not only a way in | `modules/annotations.ts`'s `n`, `Annotations.tsx`'s tab, `NoteList.tsx`'s rows, `modules/settingsPanel.ts`'s `fieldKind`, `NotePanel.tsx`'s swatches | the style pass's ④ device pass, 2026-09-20 |
| O140 | [x] | **Closed 2026-09-21, once the chrome had been opened before and after, and the strip flicked after, on the user's own iPhone over the tailnet against the production build.** (The flick has no "before": ④'s device pass never drove one, and the flick defect was found on this branch's own changed code.) `Chrome` returned `null` while hidden, so every show built the bars from nothing and the rail mounted an unwindowed tile for every page in the volume — 774 elements, 238 images and 237 tiles on a 237-page volume, 290–500 ms at 4× CPU throttling. **The rail now renders a window of about 33 tiles between two spacers** sized in nominal tile widths (`tiles.ts`'s `tileUnits`, `stripWindow`, `spacerWidth`, `unitsBetween`), re-anchored as the reader scrolls or tabs. **Measured at 390×844 with 4× CPU throttling on `akame-ga-kill----v01--2015--cbz`, warm thumbnails** (`docs/notes.md`, "O140: the chrome's build cost"): first open **274 → 87 ms**, a later open **274 → 62 ms**, a page turn with the chrome open **178 → 78 ms** (10 of 10 turns moved), a page turn with it closed **37 → 33 ms** (unchanged), thumbnail requests on a first open **101 → 26**; shown, the chrome is **155 elements, 34 images and 33 tiles** against 774, 238 and 237. **The unmount's protection is kept rather than traded away**: nothing mounts until the chrome is first shown — by `Tab`, by ④'s middle tap, or by a correction — so a reader who never opens it requests zero thumbnails — measured over six page turns — and the fade still plays on showing (`transition: opacity 120ms ease` in `modules/motion.ts`; opacity sampled per frame runs 0 → 1 across eight values). **The 178 ms page turn with the chrome open was a live cost on `main` that no row recorded**, found by profiling rather than reading; windowing more than halved it. **Mount-once-and-freeze, the other candidate this row offered, was not built**: the user declined it on 2026-09-21, on the figure they were shown that day — about 46 ms, taken before the window grew from 25 tiles to 33; against this row's final numbers it would be nearer 60 — after the phone said the chrome was fast enough. It is O143, with its figures. **What the device pass cost, and what the gate was not doing at the time** (the gate was **not** green: `reader.css.test.ts` broke at `ba160157`, the first commit to touch `reader.css`, and was not fixed until `3b784e93` — 1 failed / 1274 passed in the node project for most of this branch's life, the whole device pass included. Every dispatch named specific test files rather than a whole project, so reports of "all green" were true of what they had been asked to run; when an agent did notice the failure it checked it against another commit *on this branch* and concluded it pre-existed. When the user reported the strip could not be flicked): the rail's `load` listener pulled the rail back to the current tile on every thumbnail that arrived, which windowing turned from settle-once into continuous; the window re-anchored on nearly every frame, 65 times in a 1.2 s flick; and `--strip-unit` was a constant that cannot be right everywhere — measured at 390×844 on one machine in one browser, the rail subtracts 15 px of its own height under desktop emulation and nothing under mobile, so a loaded tile is 58.16 px wide in the first condition and 68.19 in the second against a nominal of 58 either way, and the content drifted 31 frames, worst 217 px. **What performs that subtraction is still unexplained**: O138 measured this rail at 1440×900 in the same browser at 0 px and found no classic scrollbar on either this machine or the user's desktop, and said that if the 15 px returned that difference was where to start — it returned here and nobody went there, so this branch takes the measurement and leaves O138's question open. *O145 has since answered it, on 2026-10-04 — see its row.* Each was found by measurement and none by reading | `Filmstrip.tsx`'s window, spacers and re-anchor; `tiles.ts`'s `tileUnits`, `stripWindow`, `spacerWidth`, `unitsBetween`; `styles/reader.css`'s `--strip-unit` | the style pass's ④ device pass, 2026-09-20; closed on its own branch, 2026-09-21 |
| O141 | [ ] | **On a phone the readout line overflows its row once a second edition is viewed.** `Chrome.tsx`'s inline `readoutStyle` is `white-space: nowrap` with `overflow: hidden`, `text-overflow: ellipsis` and 12 px of side padding — **not** `styles/reader.css`'s `.reader-readout`, which carries only `color` and `font-size`. With one edition it fits; with the viewed edition named and a verdict shown — `raw slot 1 / 187 · p. 1 · 1% · verdict unknown · <1m read · no chapter edition` — it measures `scrollWidth` **402** against `clientWidth` **390** at 390×844, and the text sits flush to both edges instead of inside its padding. No overflow at 1440×900. The bar is style pass ③'s; ④ found this only because accepting an `edition-of` proposal by CLI gave the scratch library its first multi-edition work. **What would settle it**: which fields a phone should drop, whether the row should wrap to two lines at that width, or — the whole-branch review's own addition — that `readoutStyle` is also `display: flex`, and `text-overflow: ellipsis` has no effect on a flex container, which is why the row measured flush to both edges instead of ellipsised rather than merely tight; fixing that would mean moving the truncation onto a non-flex child | `Chrome.tsx`'s `readoutStyle`, `modules/readouts.ts`'s `readoutRow`, `styles/reader.css`'s `.reader-readout` | the style pass's ④ browser pass, 2026-09-20 |
| O142 | [ ] | **A release that slips from the page onto a bar turns or toggles on a touch screen and does nothing with a mouse.** `App.tsx`'s `endPress` classifies a release against where its press went *down*, not where it comes up (§9.2's rule), and a release within `TAP_SLOP` (10 px) and `TAP_HOLD` (500 ms) of its own down is a tap regardless of what it is over. A touch pointer gets implicit capture on `pointerdown`, so `pointerup` is delivered to the page area, and `endPress` runs, no matter where the finger actually lifts; a mouse's `pointerup` lands wherever the cursor really is, over the bar, so `onPointerLeave` sees it instead and nothing is classified at all. **Measured by the review of fix round 2, in Chromium at 390×844 with `hasTouch`, driving CDP touch events over a replica of this layout**: a press on the page area released 6, 8 and 10 px lower, over the bar, all delivered `pointerup`'s target to the page area (implicit capture), classified — as every release is — against the down position, turning or toggling depending on which third it was in. Chromium's own touch-scroll slop is what ends the window: re-measured for this row over a replica carrying **this app's own viewport meta** — `width=device-width, initial-scale=1.0, viewport-fit=cover`, `web/index.html`'s exactly, `viewport-fit` included, since that is what makes the reader draw into a notched phone's safe areas — no cancel at 15 px and a cancel from 16 — the same under `isMobile: true` and `isMobile: false`, and at either device scale factor. *An earlier 36/38 px figure came from a replica with no viewport meta at all, where mobile emulation falls back to Chromium's 980 px default viewport and scales the page to 0.398, so the same ~15 **physical** px slop reads as ~37.7 CSS px — a page the reader never loads.* A real phone's own slop is its own figure, and `TAP_SLOP` is 10 px either way, so every slip this rule calls a tap is inside the live window. The same slip driven as a mouse left `pointerup` targeted at the bar every time, so nothing fired. **Follows from "where the press went down decides", §9.2's own rule, rather than being a defect in it**: a reader who aimed at the outer third and lifted a few pixels onto the bar asked for a turn and gets one; the mouse doing nothing is the accident, not the touch behaviour. **What would settle it**: whether a reader on a phone minds a bar tap sometimes turning the page under it — a question for a *later* device pass, not for code, since no fix is implied unless the answer is yes. **④'s own device pass cannot be read as the answer**: it ran the same day and `docs/notes.md` records the user reporting that nothing turns by accident, including a finger that lands, drifts and lifts — but it does not record whether the bars were up for that, and a slip onto a bar needs a bar to slip onto, so the condition this row is about was never controlled for | `App.tsx`'s `endPress`, `onPointerUp`, `onPointerLeave` and `leaveRegion`; `gestures.ts`'s `TAP_SLOP` and `TAP_HOLD` | the review of fix round 2, style pass ④'s fix round 3, 2026-09-20 |
| O143 | [ ] | **The reader's chrome is still rebuilt from nothing on every open, about 33 tiles of it.** O140 windowed the rail, which took a later open from 274 ms to **62 ms** at 390×844 under 4× CPU throttling; what it did not do is keep the chrome mounted. `Chrome`'s `if (!visible) return null` is unconditional, so it unmounts on every hide and the bars are rebuilt on every show. **What would fix it**: mount once and hide with `display: none`, which measured **15 ms** against a rebuild's 274 ms on the pre-windowing chrome (`visibility: hidden` measured 4 ms but leaves the rail in layout, so it would reflow on every page turn — the cost O140 exists to remove). **It needs a second mechanism, which is why it is its own row**: `display: none` stops the painting and not the re-rendering, so a mounted chrome reconciled on every page turn, measured at **165 ms against 37 ms** — a 4.4× regression on the most common interaction in the app. The chrome must also stop re-rendering while hidden, by holding the element tree it last built so React skips the subtree. **What would decide it**: whether an open still reads as slow on a phone. It does not today — the user opened the chrome on their own iPhone on 2026-09-21 and called it faster, and declined this work the same day on the saving they were shown, about 46 ms. Against O140's final numbers it is **47 ms** — 62 ms down to the 15 ms a `display: none` re-show measured on the pre-windowing chrome, which is the nearest figure there is and not a like-for-like one. **A freeze that silently thaws looks identical on screen and costs 165 ms a turn**, so building this needs a test that fails when the freeze is removed; the design records one (§7: count `Filmstrip` renders across a page turn while the chrome is hidden, and assert zero) | `Chrome.tsx`'s `return null`, a frozen element tree | O140's design §4 and its device pass, 2026-09-21 |
| O144 | [ ] | **A double-page scan the sidecar never marked `spread: true` makes the filmstrip's nominal tile width wrong, and the rail jumps as it loads.** `tiles.ts`'s `tileUnits` answers from the data — two nominal page-widths for a `spread: true` group or a `two`-mode pair, one otherwise — so a page that is *drawn* as a double-page but is not *recorded* as one is called one unit while it draws at two. O140's window sizes its two spacers in nominal widths, so such a tile is under-counted by a full unit while it is covered and over-counted the moment it renders: the content before the viewport changes size and everything shifts under the reader. **Measured on `akame-ga-kill----v01--2015--cbz`, 390×844 at 4× CPU throttling** (`docs/notes.md`, "O140: the chrome's build cost"): **six** of 237 pages have 160×120 thumbnails, and a flick across the rail drifts on **4 frames, worst 64 px** — one unit — down from 31 frames and 217 px before `--strip-unit` was measured at runtime, which fixed every *ordinary* tile and left exactly these. **Two candidate fixes**: cache each tile's real width once it has been seen, which removes the drift on every pass after the first and leaves it on the first; or carry a page's dimensions in the API so `tileUnits` can answer from them, which removes it outright and fixes every surface rather than only the strip. **What would decide it**: whether a reader minds four jumps across a whole volume's strip — the user saw it on 2026-09-21, called the result "much smoother", and chose to stop there | `tiles.ts`'s `tileUnits`, `Filmstrip.tsx`'s spacers, the `edition`/`member` page rows | O140's device pass, 2026-09-21 |
| O145 | [x] | **Closed 2026-10-04, once the rail had been measured in each context this row lists and the thing taking the height had been seen in a capture: a classic scrollbar, drawn on the rail itself.** **Nobody knows what subtracts 15 px of the filmstrip rail's height in one context and nothing in another, and O138 left an unreconciled reading that may be the same puzzle.** **Measured on one machine in one browser at 390×844** (`docs/notes.md`, "O140: the chrome's build cost"): under desktop emulation the rail reads `offsetHeight` 112 against `clientHeight` 97 — 15 px gone — and a loaded tile draws **58.16 px** wide; under mobile emulation it subtracts **nothing** and the same tile draws **68.19**. **O138 closed on 2026-09-20 having measured this same rail at 1440×900 at 0 px**, in headless Chromium, and recorded that neither this machine's Chromium nor the user's own desktop shows a classic scrollbar — so a classic scrollbar, the obvious candidate at 15 px, is the one explanation that row rules out on the evidence it had. That row also records a discrepancy it did **not** reconcile — ③ measured 149 and 145 inside the same 176 px strip, through the same Playwright harness on the same machine, where ④ got 164 and 160 — and ends: *"If the 15 px returns, that unexplained difference is where to start."* **The difference it points at is ③-against-④, not desktop-against-mobile**, and this row is about the second; whether they are one puzzle or two is itself unestablished. The 15 px returned on O140's branch and nobody went to either; a story was written around it instead, in every place the code was explained, and it took four passes to clear — a whole-branch review, a review of the fix round, and two pre-merge claim checks — with each correction round introducing new false claims of the same shape (`docs/notes.md` has the account; this row exists so the *question* is not cleared along with the false answers). **Why it is not urgent**: O140's fix does not depend on the cause. `Filmstrip` takes the median of the tiles that have drawn, each normalised to one unit, and writes that into `--strip-unit`, so the floor, the spacers and the tiles agree whatever is taking the height. **Why it is still worth answering**: a layout constant whose value depends on an unidentified context is a constant that will be wrong again somewhere nobody has measured, and `--strip-unit`'s CSS values are still that constant for the first paint. **What would settle it**: the same rail measured at one size across headless, driven-desktop-emulated and driven-mobile-emulated Chromium, and on a real iPhone, reporting `offsetHeight - clientHeight` and the drawn tile width for each — plus **a re-run of ③'s own 1440×900 measurement, which is the reading O138 actually named** and which nothing since has repeated. Five readings that between them say whether the variable is the harness, the emulation mode, the viewport width, the platform, or something that changed between ③ and ④. **What closed it** (`docs/notes.md`, "O145: what takes 15 px of the rail"): on the machine these were taken on, Chromium draws a classic horizontal scrollbar on the rail unless it was launched with `--hide-scrollbars` or the page is under mobile emulation, and a plain `overflow: scroll` box loses the same 15 px in exactly the readings where the rail does — sixteen readings at 390×844 and 1440×900, in a headless shell with the flag and without it and in Google Chrome as two MCP servers launch it. **It is one puzzle.** ③ against ④ was the launch: the two sessions' transcripts show ③'s 15 px was read in the Playwright MCP server's own browser and ④'s 0 px by a Node script whose headless launch carries Playwright's default `--hide-scrollbars`, where O138's row says both came through the same harness; both figures were taken again, from one build, and match to the pixel. Desktop against mobile is the same scrollbar, which mobile emulation removes. So of the five: the harness, through one launch flag, and the emulation mode; not the viewport width; and the user's iPhone, in Safari, takes nothing, the rail reading 112 and 112 with a tile 67.89 wide. **What it leaves behind**: both `--strip-unit` constants are the width with the scrollbar drawn, and O138's sentence that this machine's Chromium shows no classic scrollbar was true of the flagged launch only — Google Chrome opened with a window here draws one and takes the 15 px. **Not measured**: Firefox, Safari on macOS, and Chrome on any other machine | `styles/reader.css`'s `--strip-unit`, `tiles.ts`'s `refinedStripUnit`, O138's row | O140's device pass, 2026-09-21 |
| O146 | [x] | **Closed 2026-10-04, once the case this row asks for had been run with the retry taken out of `Filmstrip.tsx` by hand and had failed in both reading directions.** **The blank strip O140's review found is defended by no test that runs real layout.** O140's whole-branch review found that turning a page with the chrome open, *after* the reader had flicked the strip, left the strip **blank**: the centring effect reset the window and then re-centred synchronously against the old one, where the new current tile no longer existed, and its dependencies never changed again so nothing ran a second time. **Reproduced in a real browser at 390×844**: after a flick the window held pages 115–147 at `scrollLeft` −8165; after the turn it correctly rebuilt to 62–94 while `scrollLeft` stayed −8165, with **0 tiles visible** (`docs/notes.md`, "O140: the chrome's build cost"). **It is fixed** (`Filmstrip.tsx`'s `pendingRecentre` and its retry effect) **and it is defended by a jsdom test plus a hand-run browser check — neither of which is the real path.** The jsdom test stubs the rail's own box and lets jsdom zero the rest, because jsdom has no layout; the browser check was the controller driving Chromium once and reading the numbers, which leaves nothing behind. **Why that matters here specifically**: this is a defect no per-task review caught, that no single commit introduced — before windowing, a tile that existed was always rendered, so the centring chain was null only in states with no tile to centre on, never because the tile lay outside a window — and whose test *named* the behaviour while its 4-group fixture sat wholly inside the window and could not reach it. The next change to the window, the anchor or the centring chain has only a fully stubbed test standing in its way. **What would fix it**: a case in `Filmstrip.browser.test.tsx` — real layout, real `scrollIntoView` — that scrolls the rail until the current tile leaves the window, changes `currentIndex`, and asserts both that the rail moved and that the current tile is on screen. The file already measures spacer geometry in both reading directions, so the fixture exists. **What closed it**: `Filmstrip.browser.test.tsx`'s "a page turn after the reader has flicked the rail", one case for each reading direction, at 1440×900 on the file's 120-tile fixture with nothing stubbed. Each mounts on tile 60, writes the rail's own `scrollLeft` back to a tenth of where the mount centred it, waits until the current tile has left the DOM, checks that the window holds tile 0 and not tile 61, turns to tile 61 on the same instance, and asserts that `scrollLeft` moved and that the new current tile's left and right edges lie inside the rail's. With the retry effect's body removed by hand both cases fail — `scrollLeft` still 576 and the tile's right edge at 6060 left-to-right, −576 and its left edge at −4620 right-to-left, against a rail from 0 to 1440 — and they fail the same way with `pendingRecentre` cleared after the first attempt whatever it found. **What it does not hold** is the `!resetPending` in the first attempt's condition (O181). It runs at 1440×900 only, not at the 390×844 this row's reproduction used (`docs/notes.md`, "O146 and O181") | `Filmstrip.browser.test.tsx`, `Filmstrip.tsx`'s centring effect and `pendingRecentre` | O140's whole-branch review, 2026-09-21 |
| O147 | [ ] | **A tile asked for at a size larger than its own page is cached bigger and blurrier than the source.** `cover.rs`'s `thumbnail_at` calls `DynamicImage::thumbnail(long_edge, long_edge)`, which fits a page into that box in *both* directions — it scales up as readily as down. Before O134 the only tile size was 160 px and no real page was smaller, so the direction never came up. Since O134 a caller that may stream can ask any page for 320 or 480, and a page whose own long edge falls under the rung is re-encoded **larger than it was**, then cached under that rung's name and served to every later request at that size. **Why it is not urgent**: a manga page runs 1500–2100 px on its long edge, so this needs an unusually small scan — a colophon, a low-resolution insert — and costs one oversized cache file per (page, rung) when it happens. No instance has been observed; this row exists because the behaviour is nobody's decision rather than because it has bitten. **The machinery to fix it already exists**: `cover::fits_within` has held exactly this rule for the page route since O111 — a page already within the requested edge is served as it is rather than re-encoded. `thumbnail_at` does not ask it. **What would settle it**: a fixture page shorter than 480 px on its long edge, asked for at 480, asserting the bytes back are the source's own and that no cache file was written above the source's size — the assertion this needs is a *byte* comparison, since a re-encode at the same nominal size decodes to the same dimensions and would pass a dimension check. **What would reopen it as urgent**: a library whose pages are routinely small, or a rung added above 480 | `cover.rs`'s `thumbnail_at` and `fits_within` | O134's whole-branch review, 2026-09-22 |
| O148 | [x] | **Closed 2026-09-25 on the style pass's ⑤c branch, once the constant had been derived from the stylesheet and four hand mutations had each gone red on the assertion meant to carry them.** `TILE_HEIGHT_PX` moved from `ProposalRow.tsx` to `tileSize.ts`, beside the reader strip's own two restatements, because `styles/admin-proposals.css.test.ts` runs in the `node` project and no test there imports a `.tsx` — `styles/reader.css.test.ts` already reads that module the same way, which is the precedent this row named. The test parses `width` and `aspect-ratio` off `.proposal-detail .pages img` and asserts the constant follows. **The subtlety it turns on**: that rule carries a 1px border with no `box-sizing`, so the drawn box is 74×110 while the content box is 72×108, and 108 is the right one — it is the box the artwork is scaled into. A test derived from the drawn box would assert 110 and fail against correct code, which is why `TILE_HEIGHT_PX = 110` is one of the four watched mutations, beside `= 1` (the one that used to pass the entire suite), `width: 5rem` and a dropped `aspect-ratio`. **The original row follows.** **The proposals inbox's tile height is restated in TypeScript and pinned by nothing, where the reader's strip is pinned against its own stylesheet.** `ProposalRow.tsx`'s `TILE_HEIGHT_PX = 108` is the box `styles/admin.css`'s `.proposal-detail .pages img` draws — `width: 4.5rem` at `aspect-ratio: 2 / 3` — and it feeds `requestedTileEdge` to pick the tile size the inbox asks for. Change the CSS and the constant silently disagrees: the inbox keeps asking for the rung the old box wanted, and every test stays green. **Verified by hand on O134's branch**: setting `TILE_HEIGHT_PX` to `1` passes the entire suite, because `devicePixelRatio` is 1 in jsdom and every height from 1 to 160 rounds to the same bottom rung there. **The asymmetry is the point.** The strip cannot drift this way: `styles/reader.css.test.ts` parses `reader.css` and pins `--strip-tile`'s two values *and* the 900 px breakpoint against `tileSize.ts`'s constants, which is what made restating them safe. The inbox got the restatement without the guard. **What would settle it**: the same parse against `admin.css` — read `width` and `aspect-ratio` off `.proposal-detail .pages img` and assert `TILE_HEIGHT_PX` follows from them. `web/tests/support/css.ts`'s `rules` and `declared` helpers already do the reading *(corrected 2026-10-29: `css.ts` has no `declared` helper; the tests that use one each define their own)*. **Pick it up in ⑤c rather than alone**: ⑤ was split three ways on 2026-09-22 and ⑤c is the one that rebuilds the proposals screens, so it will have this file open anyway, and a PR whose whole content is one parsing test is worse than a row ⑤c closes in passing | `ProposalRow.tsx`'s `TILE_HEIGHT_PX`, `styles/admin.css`'s `.proposal-detail .pages img`, `styles/reader.css.test.ts` as the pattern | O134's whole-branch review, 2026-09-22 |
| O149 | [x] | **Closed 2026-09-26 on the admin follow-ups branch, as the option the user chose against drawings on the real library on 2026-09-25.** The Provenance tab drops a field no shown row decided and names it in one quiet line above the table, through `unsetFields` and `unsetSentence` in `rules.ts`, and the Provenance tab now draws the 500-row cap notice Matched files always did. **The cap notice is drawn above the table on both tabs**, by the user's decision of 2026-09-26, and on Provenance above the line as well. Measured on the built branch at 390px, the table is 368px wide: in a 358px box in a touch context, and in a 343px box where a classic 15px scrollbar takes its share (`docs/notes.md`, "Admin follow-ups"). The drawings the user chose between, with a classic scrollbar drawn, measured 614px in 343 before and 366px after, as the admin follow-ups design's §0 records. **The line sits above the table, not beneath it as §12.8 of the style pass spec first recorded**, because beneath is 224 rows down. **Four of the Provenance table's eight columns are empty in every row, and it is eight columns wide because a constant says so.** `PreviewTabs.tsx`'s `FIELDS` is a seven-element constant, so the table draws `file` plus seven whatever the data holds. **Measured against the real library on 2026-09-22**, over the 224 rows a preview returns: `edition_key`, `kind`, `language` and `title` carry **0** values apiece; `file` carries 223, `series` 95, `volume` 61, `chapter` 20. So more than half the table is em-dashes, and it is the width of those dashes that makes the eight-column case the one that cannot fit a phone. **Why it was not done in ⑤a**: it changes what the screen *shows* rather than how it looks, which spec 2026-09-17 §6 reserves; the table's shape would then shift as the data does; and "nothing here sets `language`" is arguably information a person wants rather than noise. **What would settle it**: whether a reader would rather see seven columns of mostly nothing or three columns of something, asked with both drawn. **What it would buy**: measured at HEAD on 2026-09-23, at a 390 px viewport, the table is **614 px inside its 343 px scroll box** and the four empty columns are **248 px of that** — so dropping them takes most of the scrolling away. (The 524 px this row first quoted was the *pre-branch* figure, when the table overflowed the document rather than its own box; ⑤a's scroll box is why the two differ, and the present-tense claim was wrong for one commit) | `rules.ts`'s `FIELDS`, `unsetFields`, and `PreviewTabs.tsx`'s `provenance` table | ⑤a's design, 2026-09-22 |
| O150 | [x] | **Closed 2026-09-25 on the style pass's ⑤c branch, with all four candidates drawn on the real screen before the user chose among them.** The threshold left the row and became a checkbox on a line of its own, so the kind row is one-of-N throughout and takes the underlined tab look; the state row keeps its pills, as the coarser of the two filters — two identical underlined strips stacked were drawn and rejected for giving no hierarchy at all. **Measured at 390 px**: the three kind tabs fit one line where four pills wrapped to two. The fourth candidate, a fourth *state*, was rejected on a closed decision rather than on taste: `Proposals.tsx`'s own doc comment records the state, the kind and the threshold as three independent filters, and `Proposals.test.tsx` asserts the whole filter object on every click precisely so that independence is testable. **And the pills came home**: `library.css`'s **bare** `.chips` had exactly two consumers, both in `Proposals.tsx`, and its own comment said it was holding them until this PR. The library's own row is the third element that matched the selector rather than a third consumer of the bare one — `Library.tsx` wrote `chips library-chips` and overrode almost all of it — which is why `.library-chips` is self-sufficient now, with the four declarations it had been inheriting written out. One current-item rule carries three preludes so the three rows cannot drift apart visually; which ARIA attribute marks a chosen one-of-N is a separate question and has a row of its own. **The original row follows.** **The proposals inbox's filter row mixes a one-of-N filter with an independent toggle, and ⑤a's tab look is held off that row because of it.** Style pass ② found this the hard way: the underlined tab look reached the inbox and drew "All" and "Hide weak proposals" with the same accent underline side by side, so it was pulled back to the library's own row (spec §7.2, decided 2026-09-18). ⑤a then gave `config-nav` and the reader's `preview-tabs` that look on the rule that **the line is one-of-N against independent toggle, not navigation against panel** — which leaves the inbox the only tab-ish row in the app still wearing pills, for a reason that is about one control in it rather than about the row. **What would settle it**: move "Hide weak proposals" out of the row — a checkbox beside it, or into the state filter as a fourth state — and the row becomes one-of-N like the other three, so it can take the same look. **Deliberately ⑤c's and not ⑤a's**: it changes what the inbox *is*, and ⑤a could not look at it against the real screen | `Proposals.tsx`'s `chips` row, `library.css`'s `.chips` vs `.chips.library-chips` | ⑤a's design, 2026-09-22 |
| O151 | [x] | **Closed 2026-09-24, once the floor reached the reader and the gate that should have found it could see both shapes it was blind to.** #173 scoped the 16 px floor to `.reader-dialog-panel`; style pass ⑤b widened `styles/base.css.test.ts` to the `font` shorthand and to a subject wrapped in `:where()`/`:is()`, which is the half that stops it recurring. The widened gate's first catch on the way in was `admin.css`'s `.account input { font: inherit }` — not a live defect, since `.account` sets no size, but unprovable from the text and outranking the zero-specificity floor at (0,1,1); it has a companion now. **The original row follows.** The reader's Settings dialog draws `type="number"` boxes at 14 px, which makes iOS Safari zoom the document on focus — the defect ④ already fixed once, in a place its fix does not reach. `base.css`'s floor is `@media (pointer: coarse) { :where(.library, .login) :where(input, select, textarea) { font-size: max(16px, var(--text-ui)) } }`, written because signing in left the whole document zoomed and a pinch was the only way back (`docs/notes.md`, ④'s device pass). **It is scoped to `.library` and `.login`, and the reader is neither**, so it never reached `.reader-dialog-panel`. That panel sets `font-size: var(--text-ui)` — 14 px — and `reader.css`'s `.reader-dialog-panel :where(input, select) { font: inherit }` passes it to every control inside, which `settingsFields.tsx` fills with `type="number"` boxes for prefetch's ahead/behind/concurrency and motion's milliseconds. **Older than ⑤a and not caused by it**, but found by it: ⑤a's C2 was this same defeat inside `.library`, and building the gate for that one surfaced this one. **Invisible to that gate twice over** — `styles/base.css.test.ts` matches `font-size:` and so cannot see the `font` shorthand, and it reads a selector's subject and so cannot see one wrapped in `:where()`. Both limits are now written into its own doc comment. **What would settle it**: the same floor, scoped to the reader, plus widening the gate to the `font` shorthand and to wrapped subjects. **Confirmed on a device on 2026-09-23** — the user's iPhone over the tailnet, against the production build, in the same pass that verified ⑤a's own C2: tapping a Settings number box zooms the document, and tapping a Rules-screen field no longer does. Nothing on the development machine can show either, because driven Chromium is `pointer: fine` and the media query never fires; `docs/notes.md` has the pass | `styles/reader.css`'s `.reader-dialog-panel :where(input, select)`, `styles/base.css`'s floor, `settingsFields.tsx` | ⑤a's re-review, 2026-09-23 |
| O152 | [ ] | **The 16 px floor gate pairs a rule with its coarse-pointer companion by selector *text* across the whole concatenated cascade, so a second rule with the same selector is covered by the first one's companion — including a rule written after that companion, which beats it.** `styles/base.css.test.ts` collects every control given an unprovable `font-size` outside a `@media (pointer: coarse)` block and requires an identically-selectored rule inside one. Matching by identical text is deliberate and is what makes the check sound without computing specificity: two rules sharing a selector always tie. But the collection is a `Set` over the whole cascade rather than a relation between one rule and one companion, so it cannot tell two rules with the same selector apart. **Demonstrated 2026-09-24**, while running ⑤b Task 8's own mutations: appending a second `.account input { font: inherit }` to the end of `admin.css` — after its companion, therefore beating it at equal specificity — leaves the file green. Found because the mutation that task specified did not fail, and the implementer reported that rather than adjusting it until something went red. **A second half, found by Task 8's own review**: a wrapped list mixing controls with anything else is *dropped* rather than checked — `expandControlSubjects` returns `[]` for it — so it never enters `exposed` and nothing can go red for it. An earlier draft of the gate's comment called that "fails closed", which is backwards; corrected, and stated as the file's fourth limit. `base.css`'s `:where(.library, .login) :where(button, input, select, textarea) { font: inherit }` is exactly that shape and carries the very shorthand Task 8 taught the gate to read. **It is safe today only by accident of a neighbour**: the separate coarse floor beneath it, `:where(input, select, textarea)`, is a different selector text and sets `font-size` as a longhand, so it wins over the shorthand — a fact a text-matching gate cannot know. A new mixed rule written without such a neighbour would pass unseen. Dropping it is the right engineering call (keeping it whole would go red against safe code); only the claim about it was wrong. **Not a live defect** in either half: no selector in the tree is written twice, and the one mixed list that exists is covered by its neighbour. **What would settle the first half**: each rule's position, so an exposed rule can require a companion *after* it. `tests/support/css.ts`'s `rules()` returns `{prelude, body}` and no index, and the tests that parse CSS share it, which is why ⑤b documented the limit as the file's third stated one rather than widening the helper. The narrow ordering guard for `.field input` — an `indexOf` pair — stays as the one case checked by hand | `styles/base.css.test.ts`'s `fontSizedControls` and its cascade case, `tests/support/css.ts`'s `rules()` | ⑤b's Task 8, 2026-09-24 |
| O153 | [x] | **Closed 2026-09-26 on the admin follow-ups branch, with option B chosen against drawings on 2026-09-25:** the quiet-text rule's `.user-row .why` became `.users .why`, so the add-account hint is 12px in `--muted` like the other two (`docs/notes.md`, "Admin follow-ups"). **Three paragraphs share the class `.why` on one screen and render two different ways.** On `/config/users` the role hint inside `.user-row` and the password hint inside `.reset` both compute `--muted` at `--text-meta` (12 px); the one inside `.add-user` computes `--text` at 16 px, because `admin.css`'s quiet-text rule is `.user-row .why, .user-row .locked` and the add form is not inside a `.user-row`. Measured in Chromium 2026-09-24: `rgb(107, 97, 81)` at 12 px against `rgb(35, 30, 23)` at 16 px. The class name asserts the three are the same kind of thing, and the largest of them also reads louder than the form's own labels, which are 11.2 px uppercase — the same `--text` as the paragraph, at 16 px against their 11.2 px, so it is size and not colour that carries it. **Older than ⑤b and not caused by it** — that paragraph was never styled at all — but newly visible, because ⑤b is what made its two siblings quiet. **A decision, not a defect**: whether to mute the long one or un-mute the short ones is a judgement about what an explanation under a form is for, and ⑤b's §11 never asked the question. **What would settle it**: asking it with both drawn, the way ⑤b settled its own five | `styles/admin.css`'s `.users .why, .user-row .locked`, `Users.tsx`'s add-account form | ⑤b's Task 3 browser check, 2026-09-24 |
| O154 | [x] | **Closed 2026-10-01 on the accessibility pass's markup branch.** `UserRow.tsx`'s `.checks` is a `role="group"` named "Permissions": a name with no visible text and no `<fieldset>`, so nothing on screen moves — the user's choice on 2026-10-01, offered against a fieldset with a visible legend and against closing the row unchanged. Chromium's tree holds a `group` "Permissions" on each user card beside "Roots this account may see", where `main` has none (`docs/notes.md`, "The markup (§4, 2026-10-01)"). `RootRow.tsx`'s `.checks` and the inbox's `.weak-check` hold one box each and take no group. No screen reader was run: VoiceOver, which read the two blocks alike, was not re-measured. *As filed:* **The permission checkboxes are grouped by a bare `<div>` while the grant checkboxes six lines below them use `<fieldset><legend>`.** ⑤b moved the three permission boxes out of the field grid into `<div className="checks">`, because a checkbox's label is a sentence rather than a field name; the root row's single box followed. `UserRow.tsx`'s own `.grants` block already demonstrates the more accessible pattern on the same screen, so the inconsistency is visible in one file. **Two reasons ⑤b left it**, both recorded rather than assumed: spec 2026-09-17 §11.8 enumerates exactly three markup changes for that PR, deliberately, so they arrive as decisions rather than as styling, and a `<legend>` would be a fourth *and* put new visible text on the screen; and `<fieldset>` carries `min-width: min-content` from the UA stylesheet, which is a shape that has already cost this pass time on these screens. **What would settle it**: the accessibility pass that follows ⑤, where a grouping label is the question rather than a side effect. **The audit's evidence** (`docs/notes.md`, "The accessibility audit"): Chromium exposes the `fieldset` as `group "Roots this account may see"` and the three bare boxes as no group at all; on the user's iPhone on 2026-09-30, VoiceOver read both sets alike, each box's text and then its state (*"checkbox, checked, double tap to toggle setting"* for a ticked one), and the user reported no difference between them | `UserRow.tsx`'s `.checks` and `.grants`, `RootRow.tsx`'s `.checks`, `styles/admin.css`'s `.checks` rules | ⑤b's Task 1 review, 2026-09-24 |
| O155 | [x] | **Closed 2026-09-26 on the admin follow-ups branch, with option C chosen against drawings on 2026-09-25:** `strength` answers `maybe` for a score of 1, so the default view reads 4 `strong` and 13 `maybe`, and "Show weak proposals" reveals exactly the 18 rows that say `weak` (`docs/notes.md`, "Admin follow-ups"). It keeps both closed decisions of the curation build spec, strong at 2 and the threshold at 1. **`strength()` and `INBOX_MIN_SCORE` draw the line in two different places, so most of the inbox reads "weak" under a control offering to show weak proposals.** `curation.ts`'s `strength` returns `weak` for any score under 2; `INBOX_MIN_SCORE` is 1, so the default view hides only score 0. **Measured against the real library on 2026-09-24**: of 35 open proposals, 18 score 0, 13 score 1, 3 score 2 and 1 scores 3 — so the default list is 17 rows of which **13 carry a chip reading `weak`**, directly beneath a checkbox labelled *Show weak proposals* which is unchecked. Both words are defensible alone and they cannot both be right on one screen. **Not ⑤c's to settle**: spec 2026-09-17 §6 reserves anything that changes what a screen *says*, and which of the two lines is wrong is a curation judgement about what the score means, not a styling one — ⑤c tokenised the chip and left the word. **What would settle it**: either `strength` draws its line at 1 so the chip and the filter agree, or the chip gains a third word so 0, 1 and 2+ read differently. The counts above are the evidence either way | `curation.ts`'s `strength` and `INBOX_MIN_SCORE`, `Proposals.tsx`'s checkbox label | ⑤c's browser survey, 2026-09-24 |
| O156 | [x] | **Closed 2026-10-01 on the accessibility pass's markup branch.** Each row takes the role that fits its job — the user's choice on 2026-10-01, offered against `aria-pressed` everywhere and against radio groups. `PreviewTabs.tsx` is a `tablist` of `tab`s with `aria-selected` and one `tabpanel`, no longer a `<nav>`; `←`, `→`, `Home` and `End` move the selection with the focus, and a key pressed with Alt, Ctrl or ⌘ is left to the browser. The library's filters are a `group` "Show" with "Added" outside it, and the inbox's rows are `group`s "State" and "Kind", all still `aria-pressed`. `RuleList.tsx`'s `aria-current` is on the row's `.select` button, which is what takes focus, and `admin.css` draws the row through `:has()`. `TopBar.tsx`'s layout buttons, `NotePanel.tsx`'s swatches and `Filmstrip.tsx`'s tile are unchanged. Chromium's tree shows the tabs and the three groups; the selected rule's mark is in neither capture and rests on the tests (`docs/notes.md`, "The markup (§4, 2026-10-01)"). No screen reader was run. *As filed:* **Four one-of-N button rows mark the chosen one two different ways.** `PreviewTabs.tsx` writes `aria-current="true"` on a `<button>`; the library's filter row (`Library.tsx`) and the inbox's state and kind rows (`Proposals.tsx`) write `aria-pressed`. `Config.tsx`'s `config-nav` is a fifth row and is **not** part of the disagreement — it is a `<nav>` of links and `aria-current="page"` is the right answer there, which is why the question is only about the four buttons. **⑤c made the look one look and deliberately did not touch the markup**: `admin.css`'s current-item rule carries all three preludes in a single rule so the rows cannot drift apart visually, and `styles/admin-proposals.css.test.ts` asserts that rule's selector list is exactly those three. Changing the attribute would move what four rows announce to a screen reader, which is a decision rather than a side effect of a stylesheet. **What would settle it**: the accessibility pass that follows ⑤, where what a filter that is not navigation should announce is the question rather than a consequence. **The count above is short: corrected by the accessibility audit on 2026-09-26.** It missed the reader's layout buttons (`TopBar.tsx`, `aria-pressed` inside `group "Layout"`) and the note editor's colour swatches (`NotePanel.tsx`, `aria-pressed` inside `group "Note colour"`), both one-of-N; `RuleList.tsx` marks the selected rule with `aria-current="true"` on its `<li>`; and `Filmstrip.tsx` marks the current strip tile with `aria-current="true"` on its `<button>`, which is a position rather than a choice. **What VoiceOver says**, on the user's iPhone on 2026-09-30 (`docs/notes.md`, "The accessibility audit"): an `aria-pressed` one-of-N button is *"toggle button, pressed"* (*"open, …"*, *"all 17, …"*, *"single, …"*), which does not say that choosing another releases it; `aria-current` on a preview tab is *"current item"*, and the `<nav>` round those four buttons is announced as *"nagivation, landmark"* (the user's spelling). In Chromium's tree the inbox's state row and kind row are six buttons with no group or name between them, so two `[pressed]` sit in one run of six | `PreviewTabs.tsx`, `Library.tsx`, `Proposals.tsx`, `TopBar.tsx`, `NotePanel.tsx`'s swatches, `RuleList.tsx`, `Filmstrip.tsx`, `admin.css`'s three-prelude rule | ⑤c's design, 2026-09-24 |
| O157 | [x] | **Closed 2026-09-26 on the admin follow-ups branch.** Under `(pointer: coarse)`, the three underlined rows take a 44px target from an `::after` with no paint, drawn exactly as before, and the wordless `<label className="enable">` around the rule-enable box is 44×44. Measured in a touch context at 390px, `docs/notes.md` "O157". **The label is not free under a coarse pointer**: it takes 44px of the row's layout, so at 390px every rule's name has 13px less width (`.select` 197 → 184) and two of the library's four names wrap one line more (their rows 109 → 133). The box is drawn 24×24 either way, 6.5px further right and 7px lower. The user accepted the wrap on 2026-09-26. At a fine pointer `.rule-row .enable { display: flex }` keeps the box where it was, 12px below its row's top with the label and without, where the unstyled label had put it at 15 (1440px). **Overlap was checked two ways**: a centre-point `elementFromPoint` check of the controls in `main` against the three tab rows, and the rectangle check the admin follow-ups design's §2.4 specifies, each grown 44px rectangle against every visible control's box in the document, which found 0 intersections on /config/rules, /proposals and /config/users (`docs/notes.md`, "O157" and "Admin follow-ups"). **`RuleEditor`'s file input leaves the row rather than closing with it**: it has the `hidden` attribute, so it is never drawn and cannot be touched, and the Import button that opens it already clears 44. **Five rows and controls sit under the 44 px floor `admin.css` gained in ⑤c, and §7.1 asks for them.** **Two of the five were added by ⑤c's whole-branch review**, which measured the three underlined one-of-N rows together and found the floor reaching two of them: `.proposal-tabs` and `.preview-tabs` are `<button>`s and boxed 44 with a 49 px strip, while `.config-nav`'s `<a>`s stayed at 20 and 24 — so two thirds of one design took the treatment the spec calls a redesign, on the strength of the element name alone. ⑤c excluded both to match `.config-nav`, which leaves all three rows short of 44 and all three in this row rather than one. The original three follow. That clause is *"links **and** controls get 44 px rows under `(pointer: coarse)`"*, and ⑤c's floor reaches every button, field, checkbox row, link-shaped action and `<summary>` on the six screens `admin.css` draws — except three, each excluded for a measured reason. **`.config-nav`'s four `NavLink`s render at 20 px**: the floor would put the underlined current-tab marker 12.5 px below the word and take the strip from 29 px to 49, which is a redesign of that row rather than a floor applied to it. **`RuleList`'s rule-enable checkbox has no `<label>` at all**, only an `aria-label`, so at 24 px the box is the entire tap target — and 44 px on the box draws the button-shaped checkbox the floor's own reasoning refuses. **`RuleEditor`'s file input is `hidden`** and is reached by its own button, which does clear 44. **Filed rather than left to a comment**: an earlier version deferred these to "the accessibility pass, which already owns tap targets", and it does not — O135 is the keyboard reaching no control in the *reader's* top bar, O154 is checkbox *grouping markup*, and spec §5 defines that pass as VoiceOver on the phone and a screen reader on the desktop. The only place the spec assigns 44 px tap targets is a device pass (§9.4). So nothing held these three until this row | `admin.css`'s `(pointer: coarse)` floor, `Config.tsx`'s `config-nav`, `RuleList.tsx`'s enable checkbox | ⑤c's Task 7 re-review, 2026-09-25 |
| O158 | [x] | **Closed 2026-10-03 on a branch it shares with O160 and O179, once the rename this row describes — `reader-success` in `Account.tsx` and its test, `reader.css` left alone — had been made by hand and the new test failed naming the class and the file.** **Nothing ties a class name a component writes to the rule that draws it, so a half-finished rename stays green.** `Account.tsx` names `reader-success`; `reader.css` defines `.reader-success`; `Account.test.tsx` asserts the string `'reader-success'`. Rename the class in the component and the test and leave the stylesheet alone and every gate passes, with the element drawing nothing. **Found while doing exactly that rename** — ⑤c renamed `.reader-note-ok` to `.reader-success` and the fix agent noticed the gate could not have caught it half-done. **General, not this branch's**: the same hole exists for every component/stylesheet pair in the app, which is why ⑤c filed it rather than building a guard for one screen. **What would settle it**: a test that collects the class names a component writes and asserts each has a rule in some stylesheet — `tests/support/css.ts`'s `rules()` already does the reading, and `colourLiterals.test.ts` already shows the shape of scanning source files as text. The awkward half is a class written as a template or composed at runtime, which such a scan cannot see and which would need naming as a stated limit the way this project's other gates name theirs. **What closed it**: `classNames.test.ts`, in the `node` project, reads the non-test `.tsx` files as text for the classes they write — `className="…"`, the strings and whole template words inside `className={…}`, and the two tables a site names instead, `Account.tsx`'s `LOOKS` and `Shell.tsx`'s `classes` — and fails when no selector in `index.css` or `styles/*.css` names one. Its header names what it cannot read today — a class composed at runtime (`NoteList.tsx`'s `note-list-${placement}`) and one a function returns (`ProposalRow.tsx`'s `strength(…)`) — and the shapes it would miss that nothing writes yet. **Its first run found 80 classes no selector names**, and none is a rename's leftover: 50 that a test selects by, 8 that something else draws, and 22 that nothing reads. They are exempted by class and held both ways — an exempt class that gains a rule or stops being written fails. The 22 stay listed, by the user's decision on 2026-10-03, rather than filed or removed. Measured in `docs/notes.md`, "O158, O160 and O179 (2026-10-03)" | `Account.tsx`'s `LOOKS`, `reader.css`, `tests/support/css.ts`'s `rules()`, `classNames.test.ts` | ⑤c's Task 5 fix round, 2026-09-24 |
| O159 | [x] | **Closed 2026-09-26 on the admin follow-ups branch.** The card's checkboxes carry `aria-disabled` rather than `disabled` while saving, so a checked box keeps its fill. In Chromium, `accent-color`, `opacity` and `filter` on `:disabled` did not bring the fill back to a `disabled` one (`docs/notes.md`, "Admin follow-ups"). The hold is unchanged, because `begin`'s `inFlight` ref was always what refused a second write, and `UserRow.test.tsx` now proves the refusal by clicking a live box mid-save; its double-press DELETE cases are what prove the guard is the ref. On an account's page the role select, the password field and the buttons are held by `aria-disabled` as well, so they keep focus, and a write draws them held only once it has lasted 200 ms; they keep `disabled` only for the actor and last-administrator rules and for Set password with nothing to set. **A tick refused mid-save shows nothing**: the box keeps its real value and carries no cue. Held, each of these is drawn as a `disabled` one of its kind is (`base.css`): Set password and Revoke with O201's dashed edge on the page's ground, the select at the opacity of 0.7 a browser gives a `disabled` one, except under forced colours, and the password field with the arrow and not the text cursor; under forced colours their words and edges are `GrayText`. In Chromium 151.0.7922.34, `aria-disabled` also keeps keyboard focus on the box, where `disabled` sends it to BODY the moment it is set (`docs/notes.md`, "Admin follow-ups"). **Toggling one permission checkbox on `/config/users` greys every control in that account's card and back.** `UserRow` holds a single `saving` flag **per card** — `Users.tsx` renders one `UserRow` per account, and `Users.tsx`'s own comment says "Every card holds its own controls" — and gates **eleven** `disabled` props on it, **three of which are per-item** — their `name` interpolates an id (`root-${root.id}`, `revoke-session-${session.id}`, `revoke-key-${key.id}`) — so the number of controls that go grey together grows with the account's roots, sessions and keys. A *checked* checkbox that goes `disabled` loses its `accent-color` fill to the UA's own greyed rendering, which is why what flashes is colour. **It lasts long enough to see because the lock is held past the write**: `settled()` releases it when `onChanged()` — the list *reload* — settles, not when the write answers. **The obvious fix is already refused, in writing**: `settled`'s doc comment records that holding the *whole* row is deliberate, because the grant checkbox sends a whole list, and a second tick inside the window between the 200 and the reload computes the new grants from the pre-patch `user.roots` and so silently revokes the first. Scoping `saving` per control would reintroduce that data loss on exactly the control this row is about. **Reported from the user's iPhone during ⑤c's device pass** — *"it's all the checkboxes on that page"* — and that is exactly right for the library it was seen in, which holds **one** account, so one card is the whole screen. With a second account only the touched card greys, which is the scope an earlier draft of this row overstated as the screen. Not reproducible on the development machine, where the reload returns too fast to see. **Not ⑤c's to fix**: what the screen does while saving is behaviour, which spec 2026-09-17 §6 reserves, and ⑤c changed neither the flag nor its reach. **What would settle it is most likely visual rather than behavioural**: keep a checked box reading as checked while it is disabled, since the greying is the UA's and the fill is already a token. A behavioural answer would have to free the revoke buttons while keeping the grant list held as one, which is a narrower change than per-control scoping and does not contradict `settled` | `UserRow.tsx`'s `saving`, `begin`/`settled`, `base.css`'s `accent-color` | ⑤c's device pass, 2026-09-25 |
| O160 | [x] | **Closed 2026-10-03 on a branch it shares with O158 and O179, with wording the user kept that day.** **The add-account form's explanation speaks the repository's language to an administrator.** `Users.tsx`'s add-account `.why` reads "C4 §4: there is no self-service registration — an administrator makes accounts. A new account is a reader unless this says otherwise, which is the column default `004_auth.sql` chose because any insert that omitted the role minted an administrator." "C4 §4" is a spec section and `004_auth.sql` a migration file, and the person reading the screen can open neither. **Noticed while drawing O153 on 2026-09-25.** O153 made the paragraph quiet and left its words alone. **What would settle it**: a rewrite in the reader's terms (who makes accounts, and that a new one is a reader unless the role says otherwise), with the history kept in the source comment, where it is true. **What closed it**: the paragraph reads "There is no sign-up: an administrator makes accounts. A new account gets the user role unless Role is set to admin." It says "the user role" and not "a reader", as this row's settle-line did, because `user` and `admin` are the Role select's own options and nothing on the form says "reader". The history is in the JSX comment above it. **The old paragraph's reason was not what makes its claim true on this form**: the form starts its role at `'user'` and always sends one, so the column default `004_auth.sql` chose never applies to an account made here. A `Users.test.tsx` case reads that paragraph, and failed with "C4 §4: " put back in front of it by hand; it also asks that "Role", `user` and `admin` are the select's label and options | `Users.tsx`'s add-account form | the admin follow-ups' design, 2026-09-25 |
| O161 | [x] | **Closed 2026-10-01 on the accessibility pass's reader branch.** Every panel now takes focus on opening — onto the panel itself, and the note editor on into its textarea — hands it back on closing to what held it (⚙, a bar stop, or the page area, which is also where it goes when that element has gone), keeps `Tab` inside it, and closes on `Escape`, Credits and Settings included (`focus.ts`'s `usePanelFocus`, `keys.ts`'s `inPanel`, `keyPlan`'s `Escape` branch); Settings and Account carry `aria-modal="true"`. Held by `focus.test.tsx`, `App.test.tsx`'s "the panels and focus" and `App.browser.test.tsx`'s real-key traps for every panel, the traps each watched failing against their defect. **Not checked on the device**: what VoiceOver now says when Settings opens, which the user set aside for a tester who knows it (O162). **Of the reader's panels only the note editor takes focus when it opens, none hands it back when it closes, and Credits stays open on `Escape`.** Measured in Chromium at 1440 and 390 by the accessibility audit on 2026-09-26 (`docs/notes.md`, "The accessibility audit"): opened by its key, Settings (`s`), Help (`?`), Credits (`C`), All notes (`a`) and Account (`A`) each leave focus on `BODY`, and Settings opened by ⚙ leaves it on ⚙. Only the note editor moves focus in, to its textarea. Closed by its own Close button, every one of them drops focus to `BODY`. **With Settings, Help, Credits or All notes opened by its key, the first `Tab` shows the chrome behind the panel** and lands on a strip tile, because `keyPlan` answers `Tab` with `toggle-chrome` while they are open; the account panel, which stands every key but `Escape` down, is the one whose first `Tab` reaches its Close. **On the user's iPhone on 2026-09-30**, a double-tap on ⚙ opened Settings and VoiceOver read ⚙'s own name and state, *"settings, expanded"*, not anything inside the panel. **Credits stays open on `Escape`**, where Help, All notes and Account close: `keyPlan`'s `Escape` branch has no arm for it, as it has none for Settings (O135, whose row names Settings alone). For the reader PR (spec 2026-09-26 §3) | `focus.ts`'s `usePanelFocus`, `keys.ts`'s `inPanel`, `keyplan.ts`'s `keyPlan` (the `Escape` branch), `Settings.tsx`, `Help.tsx`, `CreditsPanel.tsx`, `NotePanel.tsx`, `Account.tsx`'s overlay | the accessibility audit, 2026-09-26 |
| O162 | [ ] | **With VoiceOver on, the user could not turn a page from the page.** **Narrowed 2026-10-01 on the accessibility pass's reader branch, and left open for a tester who knows VoiceOver.** Built: two buttons, "Previous page" and "Next page", over the page area's outer thirds, drawing nothing and taking no pointer, out of the `Tab` ring, doing what the arrows do (`TurnButtons.tsx`); and one always-mounted `aria-live` region that says where a turn landed, with the readout row no longer a live region (spec 2026-09-26 §3.4, §3.5). **Not established on the device.** On 2026-10-01 the user tried VoiceOver and then set it aside: *"I think it's quite hard for me to test the voice over because I'm not used to it. I would need someone better at it to help test."* What was tried, an observation rather than a result: *"swiping on the reader just iterates through the next and previous page zones, and then double tapping opens the menu and doesn't change pages"* — no progress write in the log for it. It is not the failure §3.4 named as its assumption to check — the point under either turn button is an outer third, where a tap turns the page — and why it happened is not known (`docs/notes.md`, "The device pass, 2026-10-01"). What would decide it: a tester who knows VoiceOver, with a build that logs every press, release and click, as the notes describe. *Before this branch:* On the user's iPhone on 2026-09-30 (`docs/notes.md`, "The accessibility audit"): *"I can't swipe. but if I do click on the page it reads the page"*, and with the bars hidden, *"it basically reads the metadata of the page, and it's hard to change pages"*. A one-finger swipe is VoiceOver's own gesture for moving between items, and a double-tap on the page brought the bars up, which is what ④ does with a tap in the middle third. The user also reported *"yes the strip tiles change pages"*, of the same session, without saying whether VoiceOver was on; whether a strip tile turns a page under VoiceOver is not established. Whether the readout, `role="status"` and mounted only while the chrome is, announces the page a turn lands on was not answered. For the reader PR (spec 2026-09-26 §3). **§4's checks for the same tester** — the Proposals link's count, the named groups, Exclude globs' hint, the preview's tabs, the rotor's headings and the skip link — are listed in `docs/notes.md`, "The markup (§4, 2026-10-01)" | `TurnButtons.tsx`, `App.tsx`'s live region, `modules/readouts.ts`'s `spokenPosition`, `gestures.ts`, `Filmstrip.tsx` | the accessibility audit, 2026-09-30 |
| O163 | [x] | **Closed 2026-10-01 on the accessibility pass's markup branch.** One `h1` and one `main` on every screen once it has drawn — the reader draws nothing until its work has loaded (O176) — with nothing on screen changed, the user's choice on 2026-10-01. Login's wordmark and the account page's title are promoted and held at the size they had; home, search and the configuration screens take a visually hidden `h1` ("Library", "Search", "Configuration"); the inbox's says "Proposals, " ahead of its count; the series screen has a hidden "Series" until its series has loaded; login and the reader gain a `main`, and the reader a hidden `h1` naming the work and edition, there with the bars down. Chromium, at 1440 and 390 on the real library: exactly one `h1` and one `main` on every capture (`docs/notes.md`, "The markup (§4, 2026-10-01)"). The sidebar's two `h2`s still come before `main` at 1440; `main`'s `h1`, the landmark and the skip link (O174) all reach past them. The rotor was not tried: no screen reader was run. *As filed:* **Two of ten screens have an `h1`, and the reader has no heading and no landmark.** Chromium's tree, from the accessibility audit on 2026-09-26 (`docs/notes.md`, "The accessibility audit"): an `h1` on series and proposals only; home and search have no heading inside `main`; the four configuration screens and the account screen start at `h2`; login's one heading is an `h2`; and at 1440, on every screen with the sidebar, two `h2`s come before `main`, so the first two headings a screen reader meets there are the sidebar's; at 390 they are inside the closed drawer. The reader's tree is the page's `img` and, with the chrome up, its controls, with the title as plain text. VoiceOver's rotor, which moves by heading, was not tried on the phone | each screen's own `h1`, `App.tsx`'s root, `ReaderHeading.tsx` | the accessibility audit, 2026-09-26 |
| O164 | [x] | **Closed 2026-10-01 on the accessibility pass's markup branch.** The link takes `aria-label="Proposals, 17 open"` while it has a count, in the inbox heading's own words; Chromium's own tree names it "Proposals17" on `main` and "Proposals, 17 open" on the branch (`docs/notes.md`, "The markup (§4, 2026-10-01)"). **What VoiceOver says is not known to have changed**: it said no count at all, for a reason nobody established, and no screen reader was run on this branch. *As filed:* **The sidebar's Proposals link runs its count into its name.** `Sidebar.tsx` draws the open count as `<span className="count">` straight after the word, so Chromium names the link "Proposals17", in the sidebar and in the drawer; on the user's iPhone, VoiceOver said *"proposals, visited, link"*, with no count at all (`docs/notes.md`, "The accessibility audit"). Neither says what the number counts | `Sidebar.tsx`'s proposals `NavLink` | the accessibility audit, 2026-09-26 |
| O165 | [x] | **Closed 2026-10-01 on the accessibility pass's markup branch.** Every `TEXT_FIELDS` box is named through `aria-labelledby` by its label's own `<span>` and, where it has a hint, described through `aria-describedby` by the `<small>`. In Chromium's own tree the name is "EXCLUDE GLOBS (ONE GITIGNORE PATTERN PER LINE)" with the hint as its description, where on `main` the name ran on through the hint (`docs/notes.md`, "The markup (§4, 2026-10-01)") — in capitals because that tree applies the label's `text-transform` (O175); the sentence-case name quoted below was Playwright's snapshot rather than Chromium's tree. No screen reader was run. *As filed:* **The Exclude globs field's accessible name is its label and its whole hint.** `RootRow.tsx` renders a field's `hint` as `<small>` inside the field's `<label>`, so Chromium names the textarea "Exclude globs (one gitignore pattern per line) A trailing slash or bare name excludes the directory and everything under it; a '/**' suffix excludes only what's inside it." (`docs/notes.md`, "The accessibility audit"). It is the one field in `TEXT_FIELDS` with a hint today, so it is the one name affected. *O175 has since changed how these boxes are named, on 2026-10-04: an `aria-label` holding the label's words, where the `aria-labelledby` named a `<span>` drawn in capitals. The name is still the label alone and the hint is still the description — see its row.* | `RootRow.tsx`'s field rendering and `TEXT_FIELDS` | the accessibility audit, 2026-09-26 |
| O166 | [x] | **Closed 2026-09-30, the day it was opened, on the accessibility pass's reader branch. A key pressed on one of Settings' selects reached the reader.** `isInteractiveTarget` knew a button, a link, an input, a `textarea`, `contentEditable` and `tabindex`, and not a `select`, and four modules put one in Settings (`layout`, `fit`, `zoom`, `annotations`). Reproduced before it was filed, in the browser project with real key presses (`docs/notes.md`, "The reader's keyboard and screen-reader access"): with focus on `layout`'s `mode` select, `s` closed Settings under it and `→` turned the page. `keys.ts`'s `controlKind` replaced the rule and counts a `select` as text entry, which keeps every key; the two browser cases, which failed against the old rule, now pass, and on the real library's build the select changed and Settings stayed open | `keys.ts`'s `controlKind` | the accessibility pass's §3 design, 2026-09-30 |
| O167 | [ ] | **In two-page layout the reader shows only the notes on the slot it stands on, not the other page's.** Found by the accessibility pass's device pass on 2026-10-01 (`docs/notes.md`, "The device pass, 2026-10-01"): the test note showed under the page in single-page layout and not in two-page — *"I see the note only in 1 page view and not 2 page view"*. `App.tsx` collects the page's notes with `notesHere(notesForHere, at.edition, at.slot, resolver)`, for the reader's own slot only, and the same line stands on `main`, so it is older than the branch. Not measured: whether the other page's note shows any marker at all | `App.tsx`'s `here` (`notesHere(…, at.slot, …)`) | the accessibility pass's device pass, 2026-10-01 |
| O168 | [ ] | **The keyboard ring misses or misplaces a stop in three states.** Found by the accessibility pass's whole-branch review on 2026-09-30. **`Shift+Tab` from the page skips the note list below it while a tap has the bars up**: the list is `visibility: hidden` under the bars (O112) and its rows leave the ring while covered, and `ringMove` asks the ring a second time only when the bars must change, so the step lands on the strip; the review's scratch probe on the note-list fixture landed on the strip with the bars up and on the note row with them down. **A strip flicked away from the current tile has no stop**: only the current or anchor tile carries `tabIndex={0}`, so once the window has moved off it `Tab` from ⚙ steps past the strip. **In scroll mode the page's own controls are every mounted page's note tabs**, so `Tab` past the strip would focus the first one and, since `focusStop` focuses a control without `preventScroll`, scroll the column to it and move the position — found by reading the code, not measured. *And since O216 closed, on 2026-10-12, a state of another kind: a mouse drag of the strip hands focus that the press moved onto a tile back to whatever had it before the press, and the browser does not draw the ring there. A review of that branch's focus fix read `:focus-visible` true with a 2 px outline on `Library` after `Tab`, and false with none after a drag that had handed focus back to it, where Space still leaves the reader.* | `App.tsx`'s `ringMove`, `focus.ts`'s `controlStops`, `Filmstrip.tsx`'s `isStop` | the accessibility pass's whole-branch review, 2026-09-30 |
| O169 | [ ] | **Focus can fall out of a panel, or out of a correction, to `<body>`.** Found by the accessibility pass's whole-branch review on 2026-09-30. **A modal panel's `Tab` trap holds only while focus is inside it**: Account's Create and Revoke drop focus to `<body>` (`disabled={busy}` and the refresh re-render, as `keyPlan`'s own comment records), `keyPlan` answers nothing while Account is open, and the browser's own `Tab` then walks from `<body>` to whatever is tabbable behind the overlay; in Settings, a field that unmounted while focused would do the same, and the next `Tab` would step round the ring behind the scrim, because `inPanel` is false for `<body>`. **`Escape` that cancels a correction from one of the banner's buttons leaves focus on `<body>`**: `cancelCorrection` moves none, and the next `Tab` restarts the ring. Not measured on a device | `focus.ts`'s `usePanelFocus`, `keyplan.ts`'s `keyPlan` (`accountOpen`, `inPanel`), `App.tsx`'s `cancelCorrection` | the accessibility pass's whole-branch review, 2026-09-30 |
| O170 | [ ] | **The browser project prints React's "not wrapped in act(...)" warning many times over, from the reader's real-key tests.** At the start of the accessibility pass's reader branch the browser run printed it twice — one warning, which Vite forwards and Vitest prints again. At the branch's head it prints about 350 lines, about 175 distinct warnings, and the count moves by a few lines between runs: from the tests that press real keys with `userEvent` (the O166, ring, every-stop, gap, note-tab and correction cases) and from what a turn sets off (`PageView`'s `setShown` once a failed decode settles). Their assertions are deterministic. **Two fixes were tried and refused**: wrapping `userEvent` in `act` made a walk's first `Tab` leave focus on `<body>` — tried before the ring tests started with focus on the page area, which a test run on its own turned out to need for the same symptom, and not retried — and setting `IS_REACT_ACT_ENVIRONMENT = false` for a block traded the warning for "not configured to support act(...)" from the block's own `act`-wrapped helpers. Turning it off around each `userEvent` call only was suggested and not tried. Recorded so that a new warning is not lost among these | `App.browser.test.tsx`'s real-key blocks, `browser-setup.ts` | the accessibility pass's reviews, 2026-09-30 |
| O171 | [ ] | **On a phone, iOS zooms the reader when the note editor's text box takes focus.** Found in the event log of the accessibility pass's device pass on 2026-10-01 (`docs/notes.md`, "The device pass, 2026-10-01"); the user did not mention it. The reported viewport went from 440×736 to 385×644 — exactly 14/16, the scale that brings 14 px text to 16 px, which is what puts it on the editor's textarea; the log records no focus events — while the editor was open, and back to 440×736 after each of two two-finger gestures. The textarea is `.reader-field` inside `.docked-panel`, which sets `--text-ui` (0.875rem), and the reader's 16 px floor under `(pointer: coarse)` in `styles/reader.css` covers only `.reader-dialog-panel`, so Settings' and Account's fields are spared and the docked panels' are not. A wider floor alone would not reach it: the textarea carries an inline `font: 'inherit'` (`NotePanel.tsx`), which beats any stylesheet rule that is not `!important`. Older than this branch | `styles/reader.css`'s coarse-pointer floor (`.reader-dialog-panel`), `.docked-panel`, `NotePanel.tsx`'s textarea | the accessibility pass's device pass, 2026-10-01 |
| O172 | [ ] | **The note editor names the slot counting from 0, where the readout counts from 1.** Noticed by the accessibility pass's second claim check on 2026-10-01 in the device pass's event log, where the editor's heading read "raw slot 1" for the test note, stored at slot 1. `NotePanel.tsx` prints `{draft.slot}` as it is stored, while `modules/readouts.ts`'s `positionLine` prints `slot + 1`, so the readout calls that same slot "slot 2" — found by reading the code; the two were not seen side by side. The same line stands on `main`, so it is older than this branch | `NotePanel.tsx`'s editor heading (`{draft.slot}`) | the accessibility pass's second claim check, 2026-10-01 |
| O173 | [x] | **Closed 2026-10-01, the day it was opened, on the accessibility pass's markup branch. Every screen had the same tab title.** `index.html`'s "taiyaku reader" stood on every screen, because nothing set `document.title` — so browser tabs, history and a screen reader's page announcement could not tell one screen from another. Found while designing §4, not by the audit, and folded into §4 by the user on 2026-10-01. Every routed screen and login now names itself through `title.ts`'s `useDocumentTitle` — "Library — taiyaku", the series' own title, "Rules — taiyaku", the work and edition in the reader — and the reader says "taiyaku" alone until its work has loaded. Chromium, on the real library: "taiyaku reader" on every capture on `main`, and a title of its own on each on the branch (`docs/notes.md`, "The markup (§4, 2026-10-01)") | `title.ts`'s `useDocumentTitle`, `ReaderHeading.tsx` | the accessibility pass's §4 design, 2026-10-01 |
| O174 | [x] | **Closed 2026-10-01, the day it was opened, on the accessibility pass's markup branch. A sighted keyboard user had no way past the sidebar.** At 1440 the sidebar's links, the library settings and the theme control all came before anything in `main`; headings and landmarks let a screen reader skip them, and nothing let a keyboard alone. Found while designing §4 and folded into it by the user on 2026-10-01. `Shell.tsx` draws "Skip to content" as its first stop, shown only while it has focus; it focuses `main` without changing the URL, and `main` is focusable only while it holds that focus — the design first gave `main` a standing `tabIndex`, which the whole-branch review found taking the focus of every click on plain text inside it. Chromium, on the real library: the first `Tab` stop is the link on all nine library screens at 1440 and at 390 (`docs/notes.md`, "The markup (§4, 2026-10-01)"). **What is left**: while `main` holds the link's focus, a click on plain text inside it still lands on `main`, until focus moves elsewhere | `Shell.tsx`'s `.skip-link`, `shell.css`'s `.skip-link` | the accessibility pass's §4 design, 2026-10-01 |
| O175 | [x] | **Closed 2026-10-04 on a branch it shares with O145 and O146, by the user's choice that day to fix it without first hearing a screen reader.** **Chromium's own accessibility tree names `.field` labels in capitals.** A `.field`'s label is drawn as an upper-case micro-label through `text-transform`, and Chromium carries the transform into the accessible name: in the tree read over the DevTools protocol the boxes on the roots screen include "EDITION KEY", "DEFAULT KIND", "DEFAULT LANGUAGE" and "ADD A ROOT", those on the users screen "NAME", "PASSWORD" and "ROLE", and those on the account screen "CURRENT" and "NEW", where Playwright's ARIA snapshot — which is what O165 quoted as Chromium's — gives them as written (`docs/notes.md`, "The markup (§4, 2026-10-01)"). The same on `main` and on the markup branch, so older than it. **Not measured**: what any screen reader does with a name in capitals, and the rule form and the inbox's accept form, whose fields are in no capture. **What closed it**: each control under a `.field` label carries its label's words in an `aria-label`, in `UserRow`, `Users`, `Roots`, `RootRow`, `AcceptForm`, `RuleForm` and `Account`, and no stylesheet declaration changes. `RootRow`'s four text boxes were named through `aria-labelledby` (O165), which names the same capitalised `<span>` and outranks `aria-label`, so there `aria-label` replaces it; `aria-describedby` still points at the hint. On the running app, in Chrome's own tree, the users, roots, account, rules and proposals screens gave each such control in capitals before and as written after, the rule form and the accept form among them (`docs/notes.md`, "O175 and O182"). `fieldNames.browser.test.tsx` reads Chromium's tree over the DevTools protocol in seven cases, one for each form or card, each first asserting that its labels compute `text-transform: uppercase`, and a jsdom case beside each component holds each `aria-label` to its label's own words. Each `aria-label` taken out by hand failed its component's jsdom case and its browser case, the browser failure showing the capitalised name. **What it leaves**: a label's own text is still in capitals in that tree, as static text beside a control named as written, and two headings and a group are still named in capitals (O182). **No screen reader was run**, before or after; what to listen for is in the notes | `styles/admin.css`'s shared `.field` block (`text-transform: uppercase`), `fieldNames.browser.test.tsx`, `tests/support/fields.ts` | the accessibility pass's §4 evidence pass, 2026-10-01 |
| O176 | [x] | **Closed 2026-10-02 on a branch it shares with O177. A work that failed to load left the reader blank.** The load effect's `catch` keeps what failed, and `ReaderFailed.tsx` draws it from beside `if (!session) return null`: one `main`, a visible `h1` saying "This work could not be opened", the message in a `role="alert"` the way `Library.tsx` and `Series.tsx` show a failed read of theirs, and a "Back to the library" button that calls `onLeave`. A 401 still draws nothing and logs nothing here; a failure from a load its effect has cancelled reaches neither the console nor the screen; until the load answers the reader still draws nothing; and the tab still says "taiyaku" (O173). There is no retry, and no key: the reader's key listener waits for a session, so `L` is not bound on this screen and `Tab` is the browser's own. Driven in Chromium on a copy of the real library (`docs/notes.md`, "O176 and O177 (2026-10-02)"): a typed `/read/nope` drew the screen at 1440 and at 390 with the document the size of the window, `Tab` reached the button, the button went to `/`, and Back drew the screen again. **The sidecar read's 404 is said in the screen's own words** — "The server found no work named" and then the slug, in a `<code>` — by the user's decision the same day: shown as thrown it was `api.ts`'s sentence around the server's body, ``loading nope: 404 no work `nope` ``, and the pixels showed its backticks drawn over the letters beside them (O179). `api.ts` throws a `WorkNotFound` for that 404 and the load's `catch` asks for the type; every other failure is still shown as thrown. It says what the server found and not what the library holds: driven on a scratch library, a volume scanned since the server started answered 404 here while the library's own route listed it (`docs/notes.md`, "O178, O179 and O180, the same day"). *As filed:* **A work that fails to load leaves the reader blank.** `App.tsx`'s load effect catches a failed `openWork` and logs it, and `App` returns `null` until it has a session — so a sidecar read that fails leaves `Routes.tsx`'s black `.reader-route` with nothing on it: no message, no way back but the browser's own, no heading and no landmark. Driven in jsdom on 2026-10-01 with the sidecar read rejecting: no markup, and one `console.error` (`docs/notes.md`, "The markup (§4, 2026-10-01)"). Noticed by the markup branch's whole-branch review while it checked the reader's tab title, which reads "taiyaku" in this state since that branch; the blank page is older than it. A 401 is a different case, routed to the sign-in screen by the gate, as the load effect's own comment says | `App.tsx`'s load effect and the guard beside its `if (!session) return null`, `ReaderFailed.tsx`, `api.ts`'s `WorkNotFound` | the accessibility pass's §4 whole-branch review, 2026-10-01 |
| O177 | [x] | **Closed 2026-10-02 on a branch it shares with O176. `Config.browser.test.tsx`'s scan-report case could not tell its defect from its fix.** A new case beside it, "keeps the scan report's list of causes inside its row", holds `.scan-report > section > dl` on the list itself and not on the document: with the rule put back to `.scan-report dl` it fails on `expected 376 to be less than or equal to 357.5`, and with the rule as it stands it passes. **What was left unestablished is measured now** (`docs/notes.md`, "O176 and O177 (2026-10-02)"). The defect does not reach the document at 390 — the list ends 2px past its card's border, and the card sits 16px inside `main` — and it did not at `5e09e474` either, where the old case failed alone and passed inside its file with the defect put back and with the rule as committed alike. The 309px row in `admin.css`'s comment on the rule belongs to a 375px client width, where the lane's probe read the document 1px wider than its client with the defect put back and the fonts in, under a report that names no path; the comment's 2px was not reproduced. That comment and the old case's are rewritten to what was measured, no rule in `admin.css` changed, and `admin-config.css.test.ts` now holds the selector in the `node` project, which CI runs. **The 12px in the fallback font was not the font's doing alone**: with the fonts in, the paths in that fixture already put its list 14.2px past its card, and the fallback carries that past the viewport's edge — filed as O178. *As filed:* **`Config.browser.test.tsx`'s scan-report case cannot tell its defect from its fix; what it has failed on is the fallback font.** "does not scroll sideways with a populated scan report" was written on ⑤b to hold `.scan-report > section > dl`, which as a descendant selector gave `UnreadableCauses`'s nested `<dl>` a 256px floor (`admin.css`'s comment on that rule). **Measured on 2026-10-01** with a probe in the case, run alone (`docs/notes.md`, "The markup (§4, 2026-10-01)"): with the rule as it stands, and with it put back to `.scan-report dl` so that the nested `<dl>` was one 256px column, the document and `main` were each 402 px wide in a 390 px viewport before the web fonts had loaded and 390 px after `document.fonts.ready` — the same either way. So the case passes against the defect it names once the fonts are in, and fails against the fix before they are. That is what its failures were: run alone with `-t` on `main` at `16504d61` and on the markup branch, and inside whole runs once on the reader branch on 2026-09-30 and twice on the markup branch on 2026-10-01, the second turning a run of `scripts/check-all.sh` red. **Narrowed 2026-10-01 on the markup branch, by the user's decision**: the case waits for `document.fonts.ready` before it measures, so the fallback font no longer turns it red — run alone, it passed three times of three. **What is left**: the case holds nothing about the nested `<dl>`, and whether that defect still reaches the document at 390 on the current tree is not established; and in the fallback font this fixture is 12 px wider than the screen, which may or may not deserve a fix of its own. CI does not run the browser project; `scripts/check-all.sh` does | `Config.browser.test.tsx`'s scan-report case, `admin.css`'s `.scan-report > section > dl` | the accessibility pass's §4 fix-round review, 2026-10-01 |
| O178 | [x] | **Closed 2026-10-02, the day it was opened, on the branch that closed O176 and O177, by the user's decision that day. A scan report that named a path stuck out of its card at 390, and a longer path scrolled the document.** A `.paths` row that holds a list lets the list drop under its label when the two do not fit on one line, and a path longer than its line breaks where it must — that in the removal reports too. A row with no list does not wrap. This is the notes' wrapping candidate, changed as the notes say. Driven in Chromium on a scratch library, under a report the server made (`docs/notes.md`, "O178, O179 and O180, the same day"): in a 390px window, whose scrollbar leaves a 375px client, the document read 618 of 375 before; 525 with these rules, the rest being O180's; and 375 with that fixed too, with nothing past the card at 390, 768 and 1280. At 1280 the label "Pages could not be counted" went from a box 85px wide beside its list to one line above it, and the formats' row above it, whose list fits, kept its left edges and its sizes. A path still breaks inside a word where it has to. **The second whole-branch review found the fix wrong at 1280**: where a row is stretched to the height of the row beside it, a dropped list started 307px below its label's top. `align-content: flex-start` on the same rule, and a case that fails without it. `Config.browser.test.tsx` asks the card and not only the document, in the faces and in their fallbacks, and `admin-config.css.test.ts` holds the declarations in the `node` project. *As filed:* **A scan report that names a path sticks out of its card at 390, and a longer path scrolls the document.** A path does not break at a slash, and neither of the `<dd>`s that hold it — `.paths > dd` and `.unreadable-causes > dd`, both flex items — goes under its content's narrowest, so the list of causes is as wide as its longest unbreakable run. Measured in Chromium on 2026-10-02 with the fonts in (`docs/notes.md`, "O176 and O177 (2026-10-02)"): `Config.browser.test.tsx`'s populated fixture ends 14.2px past its card's border and 1.8px short of the viewport's edge at 390, which is the whole of that case's margin; and a report naming the one file the real library's scan left uncounted, in a root's card, reads 446 of 390, document and `main` both, and fits at 1280. In the fallback stack the same two read 402 and 459 of 390, the first being what O177's case failed on. The lane that found it read the same widths with the whole `<Roots />` screen mounted over a mocked API. **The report was a fixture both times**: no scan was run and no server was asked. **Five candidates were tried in a probe and none taken**, because each either leaves the overflow or redraws a row that fits today — `min-width: 0` on the outer `<dd>` alone leaves that report at 446 of 390; `overflow-wrap: anywhere` on the paths' list items holds every fixture inside its card at 390, 375 and 1280 in both fonts; what the wrapping variants do to a row that fits today is in the notes, from the lane's captures and not measured twice. Same shape elsewhere: under an invented fixture the list in a top-level `.paths` row ended 8.2px past its card at 1280 and 11.2px at 375; the removal report's list, which is outside `.scan-report`, is inside its card at 390 | `admin.css`'s two rules on `.scan-report .paths`, `ScanReport.tsx`'s `Paths` | O177's close, 2026-10-02 |
| O179 | [x] | **Closed 2026-10-03 on a branch it shares with O158 and O160, by the user's choice that day of the third option below, made from the face's own glyphs.** **A backtick has no width in the interface font, so a server sentence that quotes a name in backticks draws them over the letters beside it.** **Narrowed 2026-10-02 on the branch that found it**: the failed-load screen, where it was seen, says the sidecar read's 404 in its own words now and draws no backtick for it (O176). **What is left** is the font, and every other server sentence that quotes a name this way, wherever one is drawn. As seen on that screen before it was reworded, in Chromium on a copy of the real library: the alert's text was ``loading nope: 404 no work `nope` `` and what was drawn read as "no work ǹopè". Measured there (`docs/notes.md`, "O176 and O177 (2026-10-02)"): at 16px in "Zen Kaku Gothic New" a backtick alone is 0px wide and the slug between two of them is 34.55px, the same as the slug alone; in `system-ui` the backtick is 4.5px. `Library.tsx` and `Series.tsx` show a failed read's sentence as thrown too; **neither was looked at**, and nor was any other screen. Older than the branch that found it. Not decided: draw a quoted name as `<code>`, strip the marks, or take that one character from another face. **What closed it — and it was seven characters, not one.** Zen Kaku Gothic New's Latin files give the backtick (U+0060) and ¨ ´ ¸ ˆ ˚ ˜ a width of their own and class all seven as marks in GDEF, and HarfBuzz gives a mark no advance; read with fontTools in `@fontsource/zen-kaku-gothic-new` 5.3.0 at 400, 500 and 700. A `"taiyaku marks"` face per weight — those seven glyphs cut from the same files with GDEF, GPOS and GSUB dropped, each file under 1 KB, under the face's own OFL — goes ahead of Zen Kaku in `--font-ui`, so text in that stack draws them at the width the font gives them. `fonts.browser.test.tsx` measures it at each weight, `˚` included, whose width differs by weight; `tokens.css.test.ts` holds the declarations in the `node` project, which CI runs; CI does not run the browser project. With Zen Kaku put ahead of the face in `--font-ui` by hand, nine of the browser file's twelve cases and one of the node file's two failed. Server sentences still quote names in backticks, and are drawn with them. **Not measured: iOS Safari** | `tokens.css`'s `--font-ui`, and wherever `messageOf`'s string is drawn | O176's browser pass, 2026-10-02 |
| O180 | [x] | **Closed 2026-10-02, the day it was opened, on the branch that closed O176 and O177. A root whose path has a long run with nowhere to break took the roots screen sideways in each place this branch found the screen drawing it.** Found by driving O178's fix on a scratch library under `target/`, whose own path is that long: with the scan report inside its card the document still read 525 of 375, and the one thing past the card was the card's own heading, which is the root's path. The removal's confirm button, which says the path too, read 398 of 375 with the heading fixed. The sentence the screen says about a root just added read 603 of 375 under a second root made for it, with a 63-character name, and no element's box was past the window — the words ran out of their paragraph. Two reviews then named four more by reading the code: a refused scan's sentence and a turned-down "Scan now"'s, inside a card (509 of 390 each in a browser case before the rule reached them); a scan that ran and failed (519 with the rule narrowed by hand to leave it out); and the card's `watch` fact after a failed probe (439 before its rule). The heading, the button, every paragraph under `.roots` and that fact break a long path now. On the scratch library the document read 375 of 375 with the heading, the open removal, the just-added sentence and an add's refusal each on screen (`docs/notes.md`, "O178, O179 and O180, the same day"); the other four are held by their cases and were not driven. **These were found by one drive and two readings, and nothing here says the list is complete.** The real library's root, `/home/davey/temp/manga-test/Volumes`, does not scroll the document at 390 — `Config.browser.test.tsx`'s roots case passes on it — which is why that case had not seen this | `admin.css`'s `.root-row > h3`, `.root-row .regime dd`, `.remove-root .confirm` and `.roots p` rules | O178's browser pass, 2026-10-02 |
| O181 | [ ] | **The `!resetPending` guard in the centring effect is held by no test.** `Filmstrip.tsx`'s centring effect clears `pendingRecentre` after its first attempt only when no flick was pending — `if (recenter() && !resetPending)` — so that a turn to a tile the flicked window already held is centred a second time, once the window has been rebuilt around it (O140's fix-round review, finding D, in the effect's own comment). With `&& !resetPending` removed by hand every web test passes: 2,655 in the `node` and `jsdom` projects and 175 in the browser project, at `1b300032`. O146's browser cases cannot reach it, because their turn goes to a tile outside the flicked window, where the first attempt finds no tile and the condition is false with or without the guard. O146's lane also tried a flick that kept the target inside the window, and reported the rail landing in the same place with the guard removed as with it. **Why is not established.** That file loads no thumbnail, so every tile in it is at its nominal width, and that is the first thing to vary. **What would settle it**: a real-layout case in which tiles draw wider than nominal, a turn to a tile inside the flicked window, and an assertion on where that tile lands; or a measurement in the running app that the second centring moves nothing, in which case the guard can go | `Filmstrip.tsx`'s centring effect and `resetPending` | O146's mutation pass, 2026-10-04 |
| O182 | [x] | **Closed 2026-10-04, the day it was opened, by the user's choice that day to give these the same fix as O175.** **Two headings and a group are still named in capitals in Chromium's accessibility tree.** The `text-transform` O175 was about is also on `.proposal-detail .lab` and on `.rule-form .rule-sets legend`, and those names are in capitals too: on the running app, in Chrome's own tree, a proposal's two `h2.lab` headings are "WHY THIS WAS PROPOSED" and "THE RULE THIS WRITES", and the rule form's `sets` group is "SETS" (`docs/notes.md`, "O175 and O182"). `.proposal-detail .side-role` is drawn the same way and names nothing: "SUBJECT" and "PROPOSED TARGET" are static text in that tree, as each `.field` label's own text still is after O175. Left out of O175, whose row is about `.field` labels. **Not measured**: what any screen reader does with a heading or a group named in capitals. **What would settle it**: the same `aria-label` on the two headings and on the `fieldset`, or a screen reader heard first. **What closed it**: the first. Each of the two headings and the `fieldset` carries its words in an `aria-label`, and on the running app, in Chrome's own tree, the headings are "Why this was proposed" and "The rule this writes" and the group is "sets" (`docs/notes.md`, "O182 and O138"). `fieldNames.browser.test.tsx` reads Chromium's tree for them in two cases, each first asserting that the words are drawn in capitals, and a jsdom case beside each component holds the attribute to the words. Each `aria-label` taken out by hand failed both. The two headings' own text, the `sets` legend's, each `.field` label's and `.side-role`'s two labels are still static text in capitals, and no screen reader was run | `ProposalRow.tsx`'s and `AcceptForm.tsx`'s `h2.lab`, `RuleForm.tsx`'s `fieldset.rule-sets`, `styles/admin.css`'s `.proposal-detail .lab` and `.rule-form .rule-sets legend` rules | O175's evidence pass, 2026-10-04 |
| O183 | [x] | **Closed 2026-10-05, on the live-works branch (Part A of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`).** **The reader served the works the server started with.** `ServeState.works` was a `Vec<Work>` built once at startup, while every `/api/library/*` route read the database. So a volume a scan had just derived was listed by the library, left out of `/api/works`, and answered "no work" by the reader and OPDS until the process restarted. A rule save, an accepted proposal, a root removal or a ComicInfo write-back left the reader on the old structure the same way, and `finished_scan_json` said so in a `note` on every scan report. Now migration 14 keeps a `structure_version` counter that triggers bump on every write to the six tables a `Work` is built from. A live registry (`registry::Works`) reads every work again when the counter has moved: every two seconds, and before answering a slug it does not hold. It keeps each surviving `Work` and swaps its document under that work's own lock. On a copy of the real library, a volume added by another process answered 200 on the first request for it, and `/api/works` went from 1896 to 1897 without a restart (`docs/notes.md`, "Live works (Part A, 2026-10-04)"). | `registry::Works`, `ServeState::find`, `ServeState::find_blocking`, `taiyaku_library::structure`, `schema/014_structure_version.sql` | the day-to-day deployment spec's §2, 2026-10-04 |
| O184 | [ ] | **A write within one refresh of a work's deletion brings the work back.** A progress, notes or truth `PUT` that reaches a work the registry still holds, after that work's row was deleted, reaches `works::row_id_for_write`. It mints a `bridge:` work row with `sidecar_path = ''`, and the next refresh serves it as a work with no editions. It predates Part A: the snapshot server minted the same row, and served it from the next restart. Part A narrows the window to one refresh or less. It contradicts the comment above `lib.rs`'s `pub use works::{…}`, which says a work read from its own row never needs the bridge row, and `alignments::store`'s doc, which says that in production the row is never minted. Found with a scratch test. **What would close it**: Part D of the same spec (missing works), which should never mint from a live registry and answer "no work" instead | `works::row_id_for_write`, `works::row_id_for_document`, the comment above `lib.rs`'s `pub use works::{…}`, `alignments::store` | the live-works branch's re-review of its own whole-branch fix round, 2026-10-04 |
| O185 | [ ] | **A ComicInfo write-back cut while staging leaves `X.cbz.taiyaku-stamp` beside the archive, and the next scan sniffs it.** `stamp::rewrite_one` stages each archive beside itself (`stamp::staging_path`) and removes the staged file only when it returns an error, so a process stopped while staging leaves the file there. The scan's walk (`scan::measure_dir`) skips names that begin with `.` and, beyond a root's own exclusions, nothing else, so the next scan takes the staged file for an entry of the library. It predates both parts. Part B's bounded shutdown reaches it sooner: a stamp still running at the shutdown's bound is cut instead of finishing | `stamp::rewrite_one`, `stamp::staging_path`, `scan::measure_dir` | the deployable branch's second re-review, 2026-10-04 |
| O186 | [ ] | **A scan cut during derive leaves the root at `last_scan_status = 'ok'` with its derive unfinished.** `scan::scan_root` writes `ok` when the walk commits, before derive runs, and `scan::mark_derive_failed` corrects it only after a derive that returned an error. A process stopped during derive returns nothing. A rescan heals it. It predates both parts | `scan::scan_root`, `scan::mark_derive_failed` | the deployable branch's second re-review, 2026-10-04 |
| O187 | [ ] | **A test named for a title collision never reaches the lookup it names.** `metadata::tests::patch_series_does_not_delete_it_over_a_pre_existing_title_collision` plants one title text on two series and says `librarydoc::find_series_by_title` must not resolve the edit onto the other. Against the defect it names (`edit_series_member_raw` calling `librarydoc::apply` again) it fails only at its seed `PATCH`, before the collision is planted, and the same way with the unrelated directory in the series' library or in one of its own. `librarydoc::of` carries only `hand = 1` titles, and the planted row and the scanned title are both `hand = 0`, so the document the loop applies has no titles and the lookup is never asked (read). It predates F1 | `metadata::tests::patch_series_does_not_delete_it_over_a_pre_existing_title_collision`, `librarydoc::of` | F1's review of its derivation fixes, 2026-10-06 |
| O188 | [ ] | **A derive can give two series of one library the same title text, and a hand document then resolves onto the wrong one.** `derive::reconcile_series` finds a series by `sort_title` and does not look for it in `series_title`. Run in one library on the F1 branch: `Other` is given the alt title `Series`, and files then name `Series`. The derive mints a second series holding that text. `librarydoc::set_series_titles` on the scanned `Series` is then refused ("already belongs to series 1"), and a document naming `Series` on the scanned work resolves onto `Other` and the scanned series is deleted. `set_series_titles` refuses to make such a pair itself; the derive is the writer that still can. `main` looks the series up the same way (read, not run there) | `derive::reconcile_series`, `librarydoc::find_series_by_title`, `librarydoc::set_series_titles` | F1's review of its derivation fixes, 2026-10-06 |
| O189 | [ ] | **The proposals dry run answers for a proposal that accept refuses.** `proposals::resolves` does not ask `proposals::across_libraries` of the proposal it is asked about. For a proposal whose two series are in different libraries it answered `collateral: []`, `subject_reached: true`, `works_moved: 1`, while `accept_with` refused the same proposal (run). `resolves` can refuse only with `Err`, and `curation::resolution` answers that as a 500, so a refusal there needs a new field or a new status (read); `acceptable`'s three refusals have the same gap (read). It writes nothing, and the web mounts the accept form only where `evidence` offers a rule, so only a direct caller of the route sees it | `proposals::resolves`, `curation::resolution` | F1's review of proposals within a library, 2026-10-06 |
| O190 | [ ] | **A proposal from before libraries whose two series are now in two of them is left as it was.** One still `open` is listed like any other; its row shows "There is no rule this proposal could be accepted as, as it stands." and not why, and the sentence that says why (`proposals::across_libraries`) is printed only by the CLI's `--accept` or returned to a direct `POST`. It blocked nothing in the run that looked: another proposal's accept went through beside it. One `accepted` before migration 15 stays `accepted`, and its rule now sends the subject's files to a series of that title in the subject's own library (read, not run). Both need a store that held series across two roots before F1. A third case needs only a proposal accepted before F1, in a store of one directory too: its subject series holds no work, so migration 15 leaves it in no library, and un-accepting the proposal and accepting it again is refused, as having a series in no library, until a scan has filed the series (run with the CLI) **Run on F2b, after a move** (`a_proposal_across_a_moved_dir_still_exists`): with the two series put in two libraries by Move, the open proposal row was unchanged and still `open`, and `proposals::preview` refused it: "its two series are in different libraries, and a rule moves material only within one. Dismiss it; the next raise proposes only within a library." | `proposals::across_libraries`, `ProposalRow.tsx` | F1's review of proposals within a library, 2026-10-06 |
| O191 | [ ] | **A reader granted one library is shown another library's work row, through an edition that work kept from the reader's directory.** After migration 15 a work that spanned two directories is split, and where each directory gave its own edition key the old work keeps the other directory's edition (`DeriveReport::kept_editions`). Run as an account granted only the other directory's library: it is listed the old work and its series (id, slug, title, volume, series titles), and the series cover redirects to the kept edition. The old work's own edition answers 404 on every page, thumbnail, download and OPDS route, and no body names its directory. The guard filters editions by directory, as it did before F1 for a work across two roots. *Filed expecting F2's directory move to make the same shape:* **F2b's Move does not**, since it divides a work whose editions are on both sides of the directory. A store migration 15 split still holds the shape, and Move does not mend it: there, a move that would fold the old work away is refused (`a_divided_work_the_derive_would_fold_away_refuses_the_move`), and so is one that would replace an edition of the other library's work that carries a hand-set label or a page flag (`an_edition_a_person_has_edited_is_not_replaced_by_a_split`, `a_spread_flag_on_the_edition_a_split_would_replace_refuses_the_move`) *Since O60 closed, on 2026-10-12, for a work of this shape: a position write or a new note on the edition outside the grant is refused as one on an edition the work does not have, and a position read leaves that edition out. The truth route still sends that edition's entries and takes a write that replaces or removes them, since a truth document is read and written whole; and the notes route still sends a reader's own notes on it.* Since F3 a listing narrowed to one library reads that library's directories, so a library's page and its OPDS feed list such a work under each library whose directory holds one of its editions, with that edition alone, as an account granted only that library sees it; the by-work feed lists it under the library the store files it in, with every edition the grant allows (`opds::tests::the_by_work_feed_lists_a_work_under_the_library_it_is_filed_in_with_every_edition`). For an account with no grant limits, a library's by-directory feed counts such a work's editions under that library's directories alone, while the work's own feed, which the entry links to, lists every edition (run by F3's whole-branch review). | `browse::edition_visible`, `DeriveReport::kept_editions` | F1's grants task and its review, 2026-10-06 |
| O192 | [x] | **Closed 2026-10-08 by F2b: a folder's row has Move…, which moves the folder to another library or a new one, files its works itself before the derive, and is refused whole where a reader's place cannot be kept** (`relocate::move_root`, `POST /api/library/roots/{id}/move`). For the cases below: a folder whose works the target does not hold keeps their ids and slugs, whether its library goes with it (`a_folder_alone_in_its_library_takes_its_works_and_the_library_goes`) or keeps another folder (`a_folder_leaving_a_library_that_keeps_another_keeps_its_works_as_they_were`, which holds their positions, notes and hand titles too). A volume the target holds under the same edition key refuses the move, naming it (`the_same_volume_under_the_same_edition_key_in_the_target_refuses_the_move`). One the target holds under another key joins the target's work as an edition, with its position (`a_volume_the_target_holds_under_another_key_joins_it_as_an_edition`). A work with an edition on each side is split: the row goes with its hub edition, and each edition's positions and notes with that edition (`a_divided_work_stays_with_its_hub_and_its_other_edition_takes_its_history`, `a_divided_work_goes_with_its_hub_and_the_edition_that_stays_keeps_its_history`). The removal case at the end of the row is a removal and not a move, and F2b does not change it. A `root.library_id` set by hand and derived still does what the row describes; once a folder is added, nothing but Move writes that column. *As filed:* **Nothing moves a folder between libraries yet, and what the store does after one is not yet what a Move action can ship.** F1 has no writer of `root.library_id` once a folder is added; each case below was run by setting it by hand and deriving. A folder alone in its library, moved into a library that holds nothing at its works' addresses, keeps their ids, slugs and positions. Moved into a library that holds the same edition of a volume, the moved work is left in no library with its position, and both derives that were run reported `unfolded_works=1`. Moved into one that holds the volume under another edition key, it is folded into that work, and its row and slug go. Where the old library keeps another folder, the moved folder's works are minted again under the new library and the old ones folded into them, so each gets a new id and slug; with `keeps_documents` off, a hand title, hand volume and tag did not follow. A work spanning the moved folder and one that stays is not folded at all. One of these needs no move: where migration 15 split a work across two libraries, removing the folder that holds the old work's hub edition leaves that work in no library with its positions and notes, and the derives run after it each reported `unfolded_works=1`. `docs/notes.md`, "Libraries, F1", has each. F2a adds no writer of `root.library_id` either; F2b is where Move is built | `relocate::move_root`, `relocate::refused_before`, `fold::rehome_editions` | F1's reviews of derivation within a library, 2026-10-06 |
| O193 | [x] | **Closed 2026-10-06 by F2a: the Users card grants libraries, and a tick is a library whatever number of folders it has.** The card sends the whole list of library ids (`account::Patch::libraries`) and the server replaces the account's grants with it; a body that carries the old `roots` key is refused with a sentence naming `libraries`; the last tick cannot be cleared, and choosing "Every library" is what sends the empty list. The account-removal dialog counts library grants. Run on a copy of the real library at `904bec95`: an account set to "Only these" with one library of three folders ticked was shown that library's 13 works and none of the other library's 1896; its last tick would not clear and sent nothing; and "Every library" left it no grant. *O220 reversed the last tick on 2026-10-17: it now clears, leaving the account seeing no library, and "Every library" sends `every_library: true` rather than an empty list, because an empty list now means no library (O220's spec, §7).* *As filed:* **Unticking one folder of a two-folder library on the Users screen does nothing, and nothing says so.** The screen sends the ticked folders and the server grants libraries (`account::update` diffs by library). Run with an account holding a library of two folders: a list naming either folder alone answered 200 with both, so the card redraws the tick, and no request the screen can send drops that library; `revoke-library` does. An unrestricted account sent one folder of it came back with both. The account-removal dry run answered `grants: 1` for the first account, which the dialog prints as "1 root grant" (read). Only the CLI makes a library of two folders in F1; F2's screens are where it is fixed | `account::update`, `UserRow.tsx` | F1's whole-branch review, 2026-10-06 |
| O194 | [ ] | **A `taiyaku.json` imported for the first time lands on a work of its content chosen without regard to the library its directory is in.** A document import looks its work up across the whole store (`import::find_by_key` with no library): a work in a library before one in none, then the oldest. Run with the same bytes in two libraries: a document beside the second library's copy, naming its path, landed on the first library's older work, whose member row then named the first library's root and a path in the second's directory until the next derive; removing the first library's directory then deleted that work and the position on it, counted, and the document moved to the second library's work. A document a work already holds goes back to that work. It needs byte-identical hub content in two libraries and a document for it | `import::find_by_key`, `import::sidecar` | F1's whole-branch review, 2026-10-06 |
| O195 | [x] | **Closed 2026-10-07 by the F2a follow-ups: choosing "Every library" on an account that holds grants, by click or by an arrow key, asks before it lifts the limit, and nothing is saved until it is answered.** The question is a group labelled "Lift the limit" under the radios, with "See every library", which saves an empty `libraries` list and puts focus on the "Every library" radio, and "Keep the limit", which closes the question and puts focus on "Only these"; both are held while a write is out. The question is retired when the account holds no grant. *O220 has since changed this, on 2026-10-17: "See every library" sends `every_library: true`, and the question is retired when the account sees every library, since an account limited to no library holds no grant and is still asked.* *As filed:* **On the Users card an arrow key on the "Every library / Only these" radios saves "Every library" at once.** They are native radios and each choice is saved as it is made (spec §5), so with focus on "Only these" one arrow key selects "Every library" and sends an empty grant list: the account sees every library and its ticks are gone, with nothing asked. Run three times: with real keys in the browser test project, where ArrowUp, ArrowLeft, ArrowDown and ArrowRight each sent an empty `libraries` list; and in both drives over the real server on a copy of the real library, the second reaching the radio by Tab alone and pressing ArrowUp once (`PATCH /api/library/users/2` 200, and the account's one grant was gone). The choices were: leave it, ask before a limit is lifted, or save explicitly. Ruled 2026-10-07: ask | `UserRow.tsx` | F2a's review of the Users card, 2026-10-06 |
| O196 | [x] | **Closed 2026-10-07 by the F2a follow-ups: a library or a folder is not removed, for the dry run and the removal alike, while a scan of it is reserved (409), and the per-folder scan job reads its folder again once it holds the job lock.** The re-read is the library job's: a folder removed while the scan waited, or whose id another folder has taken, is walked for nothing, and a folder that is the same is walked with the settings it holds now. The job's report is written back only over its own reservation. A walk already past that re-read when the removal commits is held by `scan::scan_root` itself: it records a walk only where, under its write lock, the folder's id still names the path it walked, and its status write asks the same, so a walk whose folder went, or whose id another folder took, records nothing. *As filed:* **A library or a folder can be removed while a scan of it waits to start, and the per-folder scan job then scans the path it captured under whichever folder holds the id now.** Neither `DELETE` refuses while a scan is reserved. A library's own scan job reads its folders again once it holds the job lock, walks nothing if they changed, and writes its report back only over its own reservation (F2a); the per-folder job, older than F2a, does neither. Run: a folder's scan reserved while the job lock was held, its library removed (200), and a new library's folder taking the freed id; three `scan_entry` rows from the removed folder's path were then filed under the new folder, whose `last_scan_status` read `ok` and whose report named the removed path. No work was made. The Libraries screen no longer offers a removal while a scan runs in that library; the server still accepts one. A card left open from before a removal acts the same way on whichever folder holds its id now (read) | `config_api::run_scan_inner`, `config_api::delete_root`, `libraries_api::delete_library`, `scan::scan_root` | F2a's whole-branch review, 2026-10-06 |
| O197 | [x] | **Closed 2026-10-07 by the user's ruling: accepted as `folders_api`'s module doc states it, with no code and no new dependency.** *As filed:* **The folder browser follows a link if a directory inside one of its folders is swapped for one while it walks.** The walk looks each name up and then hands the same path to the kernel again, and lists by path. Run with a second thread swapping a directory inside the folder for a link out while `list` was called in a loop: some of its answers were listings of the directory outside. It needs an administrator's session and something renaming inside the share at that moment, and an administrator can already point a library at any path. Closing it means walking by descriptor, which needs a dependency `taiyaku-serve` does not have | `folders_api::Prefixes::walk` | F2a's whole-branch review, 2026-10-06 |
| O198 | [x] | **Closed 2026-10-07 by the F2a follow-ups: the derive block of a folder's scan report is headed "What derivation made of every library".** The figures under it are unchanged, and are still drawn in every folder row of the library; per-library figures were not part of the ruling. *As filed:* **A folder's scan report shows the whole store's derive figures, in every folder row of a library.** The derive is one for the store, and each folder's report carries it. Run on a copy of the real library: after a scan of a library of one work, each of its two folder rows read "Works 1897", and "pdf 1" stood under the folder that holds no PDF. The choices were: say the figures are the whole store's, or draw them once per library. Ruled 2026-10-07: say they are the store's | `ScanReport.tsx`, `libraries_api::run_library_scan_inner` | F2a's first drive, 2026-10-06 |
| O199 | [x] | **Closed 2026-10-18 by O219's first PR: the add-folder form is drawn once, on a library's page, behind Add folder…, and the make page has one Name and one folder box.** A library's page draws one Add folder…, which opens `FolderPicker.tsx`'s "Folder path" and Browse… in its place (`LibraryPage.test.tsx`'s "draws one Add folder…, and the picker only once it is pressed"), and a library is made on a page of its own from a name and one folder (`NewLibrary.tsx`), more being added on its page. Each library and each folder has its own page, so there is no card to repeat them in. *Narrowed 2026-10-07 by the F2a follow-ups:* the library's button reads "Scan library" and each folder's "Scan folder", and the line drawn while a library scans reads "A scan is running in this library…". *As filed:* **A library's Scan now and each of its folders' Scan now are the same button under the same name.** "Folder path", "Add folder", "Browse…" and "Name" repeat per card the same way, and the card says "Scanning this library's folders…" for a scan of one folder (read; seen in both drives' pictures). The words for the two scan buttons were ruled on 2026-10-07 | `LibraryPage.tsx`, `NewLibrary.tsx`, `RootRow.tsx` | F2a's review of the screen's styles, 2026-10-06; closed 2026-10-18 |
| O200 | [ ] | **Narrowed again 2026-10-17 by F3: the sidebar's list is headed "Libraries" and lists libraries, and it is read again when Configuration → Libraries reads its list, so a removed library leaves it without a reload (`Libraries.shell.browser.test.tsx`); `EmptyState`'s `no-libraries` and `not-scanned` states say library and folder ("directory" since the rename) and name Configuration → Libraries.** **Still open:** `RuleForm`, `ProposalRow`, `PreviewTabs` and `AcceptForm`, the rule routes' own "there is no root <id>", which the rule editor draws, and the store faults' own sentences in `scan.rs`. *Narrowed before:* **Narrowed 2026-10-07 by the F2a follow-ups: refusals from the server say "folder" ("directory" since the rename).** `config_api::start_scan`'s 409, the sentence after a scan whose derive failed, the folder routes' 404 and patch sentences, `scan::vetted`'s overlap sentences, `scan::check_exclude_globs`'s two sentences, the empty-scan advice and the `root_panicked` texts say folder ("directory" since the rename), and the only-grant refusal ends with a full stop. **Still open:** the sidebar's "Roots" (F3, by the spec), `EmptyState`, `RuleForm`, `ProposalRow`, `PreviewTabs` and `AcceptForm`, the rule routes' own "there is no root <id>", which the rule editor draws, and the store faults' own sentences in `scan.rs`, which still name a root. *As filed:* **"Root" is still drawn where a person reads it.** The sidebar's list is headed "Roots" and went stale in both drives: after a library was removed it still listed that library's folders until the page was reloaded (`Roots.tsx` on `main` did not refresh it either: read). Refusals drawn on the Libraries screen read "a scan of root 2 (…) is already running" and "… is inside existing root … (id 2)", and the only-grant refusal ends with no full stop (run, the final drive). `EmptyState.tsx`, `RuleForm.tsx`, `ProposalRow.tsx`, `PreviewTabs.tsx` and `AcceptForm.tsx` say root too (read). The spec leaves the sidebar and `/api/library/status` to F3 | `RuleForm.tsx`, `ProposalRow.tsx`, `PreviewTabs.tsx`, `AcceptForm.tsx`, the rule routes, `scan.rs` | F2a's first drive, 2026-10-06 |
| O201 | [ ] | **Narrowed 2026-10-07 by the F2a follow-ups: `base.css` gives a disabled button a look of its own on every screen its rules reach, which is all but the reader** (a dashed border on the page's ground, held by `buttons.browser.test.tsx`). Its rules have no specificity, so a button whose own class rule or inline style sets its background or border keeps whatever held look those give it, or none: the proposals panel's Dismiss and Reopen a muted word, the sign-in button its accent fill under the dashed border, a frozen rule row nothing. **Still open:** no alert has a look of its own. Some of the app's `role="alert"` paragraphs are notices and not refusals (`RootRow`'s `.scan-notice`, `RuleEditor`'s `.held-edit`), so one colour for all of them is a design question. *As filed:* **A disabled button is drawn like an enabled one.** `base.css` gives a disabled button no look of its own, and the Libraries screen holds most of a library's buttons while it scans or a removal is open. In the final drive a held button's computed opacity, background and colour were an enabled one's, and only the cursor differed. A refusal is drawn as body text the same way: no alert on these screens has a look of its own. Older than F2a | `base.css` | F2a's first drive, 2026-10-06 |
| O202 | [ ] | **Narrowed 2026-10-18 by O219's first PR: the refusal of a directory's `DELETE` whose dialog a status read closed is said on the page the person is on.** `RemoveRoot.tsx`'s `confirm` hands a refusal to `onLateRefusal` once `useDrawn` says the dialog is no longer drawn, whatever closed it, and a directory's page passes it the screen's `say` (`Libraries.tsx`), which draws it there. Read, not driven: `Libraries.test.tsx`'s "says $what refused once its page has gone on the page the person went to, without taking focus" holds it for a page left, not for a dialog closed under a page still drawn. **Still open, each read in the code:** a refused removal under a hold keeps its refusal, its old counts and a live confirm, since `RemoveRoot`'s `held` effect runs only when `held` changes; a failed re-read after a press is not brought into view when a scan's quiet re-read overtakes it, since the read store keeps only the newest read's answer and `Libraries.tsx`'s `reread` does not reveal a quiet one's failure; and no test closes the dialog by a status read while its removal is out (`RootRow.test.tsx`'s "closes an open removal dialog when its own mount read finds a scan running" sends none). *As filed:* **Corners of the Libraries screen that need a status read to answer late.** Each was driven by a reviewer against a mocked client. After a directory's removal is refused while something else in its library is busy, its dialog keeps the refusal and the counts from before over a live confirm, which sends the removal again. A row's own late read closes its dialog under its own `DELETE`, and a refusal of that `DELETE` is then drawn nowhere. A failed re-read after a press is not brought into view when a scan's own re-read overtakes it. And no test holds that the dialog's closing itself does not depend on its removal being out | `RemoveRoot.tsx`, `RootRow.tsx`'s `absorb`, `Libraries.tsx`'s `reread` | F2a's last review of the screen, 2026-10-06 |
| O203 | [ ] | **Narrowed 2026-10-18 by O219's first PR: an action that changes page puts focus on what says what happened or on the new page's heading, Add directory… moves it into the picker and back, and the make form's Take off is gone.** A make puts focus on the made library's heading and the make page's Cancel on the table's (`Libraries.test.tsx`'s "puts focus on the made library’s heading, and only once" and "puts focus on the table’s heading when the make form is cancelled"); a removal's report takes it on the page the removal goes up to ("moves focus to the removal report, which is named, when one is drawn"); an unknown id's "No such library." takes it; Add directory… puts it in the path box and the picker's Cancel back on Add directory… (`LibraryPage.test.tsx`'s "draws one Add directory…, and the picker only once it is pressed"); and the make page has no list of directories to take one off. **Still open, read and not run:** `LibraryPage.tsx` moves no focus for Rename…, Save, the rename form's Cancel, Remove library… or `RemoveLibrary.tsx`'s Cancel; and on a directory's page Move… and Remove… are disabled while their dialog is open, and nothing in `RootRow.tsx`, `MoveRoot.tsx` or `RemoveRoot.tsx` moves focus when it opens or when its Cancel closes it. *As filed:* **Focus is lost when the control that had it goes.** After Rename…, Save, Cancel, Remove library…, the removal dialog's Cancel and the make form's Take off, `document.activeElement` is `body` (run in Chromium by a reviewer). F2a moves focus on purpose in two places: to a removal's report, and to the directory browser's line that says where it is | `LibraryPage.tsx`, `RemoveLibrary.tsx`, `RootRow.tsx` | F2a's whole-branch review, 2026-10-06 |
| O204 | [ ] | **`scan::create_library` vets its directories against the stored roots before its transaction opens.** A second writer adding an overlapping directory in between is not compared: the same path then fails on `root.path`'s UNIQUE as a store fault, and a nested path goes in. Read, not run; `add_root_checked_in` has had the shape since before F1 | `scan::create_library`, `scan::add_root_checked_in` | F2a's review of the store, 2026-10-06 |
| O205 | [x] | **Closed 2026-10-07 by the F2a follow-ups: `account::grant_library` and `account::update` refuse a library that has no folder** (`ungrantable`), and the users route answers 409 with ``the library `<name>` has no folder, so there is nothing in it to grant``. A list naming such a library grants nothing. *O220 removed this guard and its 409 on 2026-10-17: an account limited to libraries now sees only what they hold, so granting a library with no folder grants nothing, which is no longer a widening (O220's spec, §4).* *As filed:* **An account granted a library that has no folder is unrestricted.** `account::update` grants any library that exists, and `roots_for` then answers an empty list, which the access guard reads as seeing everything. Run with a `library` row inserted by hand; nothing public makes a library without a folder, and a library goes with its last one | `account::update`, `account::access_for` | F2a's review of grants, 2026-10-06 |
| O206 | [x] | **Closed 2026-10-07 by the F2a follow-ups: a library's name is stripped of all surrounding white space, and two names that differ only in ASCII case are one name.** `libraries::trim_name` is `str::trim`; schema 16 (`016_library_names.sql`) strips the stored names the same way, makes a `COLLATE NOCASE` unique index over them, and makes the folder-naming trigger ask both rules. A stored name that clashes with an older library's once stripped and folded is renamed `Name (id)`, and the older library keeps its name; a name of white space alone is left. Name lookups (`libraries::id_of`, the CLI's `--library`) are case-insensitive. A create or a rename refused for a clash names the holder as stored. *As filed:* **A library's name keeps a tab or a newline, and names differ by case.** Only spaces are stripped, which is F1's rule. Run through the rename route by a reviewer: a name with a leading tab and a trailing newline was stored beside the same name without them, and `ROOT-B` beside `root-b` | `libraries::trim_name` | F2a's review of the libraries routes, 2026-10-06 |
| O207 | [x] | **Closed 2026-10-07 by the F2a follow-ups: Up from a configured folder that sits inside another goes to the outer folder.** `folders_api::list` answers the canonical path's parent where that is within a prefix, and no parent where it is not. *As filed:* **In the folder browser a configured folder that sits inside another has no way up.** Up from the inner one lands on the top listing and not on the outer folder; the plan's text gives a configured folder no parent. Ruled 2026-10-07: Up goes to the outer folder | `folders_api::list` | F2a's review of the folder browser, 2026-10-06 |
| O208 | [x] | **Closed 2026-10-08 by F2b, all three merged** (ruled 2026-10-07; the user chose to merge the third as well). One query asks whose only grant a library is, `scan::only_grant_holders`, under `removal_refusal`, `library_removal_refusal` and Move's own refusal. *O220 has since removed the three refusals; `only_grant_holders` now names who a removal or a move leaves with no library.* The folder and library routes answer through one `config_api::AdminAnswer`, `admin_panicked` and `DryRunQuery`. Both scans reserve, run and write back through `scan_jobs` (`reserve`, `spawn`, `Reread`). *As filed:* **Three things in the libraries routes are written twice.** The only-grant query (`scan::removal_refusal`'s and `scan::library_removal_refusal`'s); `LibraryAnswer`, `library_panicked` and `DeleteLibraryQuery` beside `config_api`'s `RootAnswer`, `root_panicked` and `DeleteRootQuery`; and the per-folder scan's reservation and job beside the library's, of which only the library's read its folders again until O196. The plan wrote the first two as pairs. The choice put to the user was whether to merge them | `scan::only_grant_holders`, `config_api::AdminAnswer`, `scan_jobs::spawn` | F2a's reviews, 2026-10-06 |
| O209 | [ ] | **Narrowed 2026-10-18 by O219's first PR: a library made is drawn on its own page, and each change of page reads the list again.** A make goes to the made library's page, drawn from the create's own answer, so a read that fails after it leaves the library shown as made (`Libraries.shell.browser.test.tsx`'s "draws the list’s failure in sight on the new library’s page after a library is made and the reload fails"), and a read that finds the library drawn gone says "No such library." (`Libraries.test.tsx`'s "puts focus on the line that says there is no such library when a read finds the one drawn gone"). The make page sends the path in its one box, so no path waits to be added: "Make the library" is held while the name or the box is empty, and says nothing of why, as `main` did for an empty box ("holds Make the library, and says nothing of why, while the name or the directory box is empty"). **Still open:** a library removed in another tab draws the server's refusal on each press on its page, and nothing reads the list again until the page changes (`LibraryPage.tsx`'s `rename`, `add` and `scan` read nothing on a refusal); after a rename whose re-read fails, the sentence says the list could not be read; and a library just made is drawn last on the table until a read orders it ("draws a made library after those already drawn until the next read orders it"). *Narrowed 2026-10-07 by the F2a follow-ups:* the make form's, Add directory's and rename's refusals are drawn through `Refusal`, which scrolls them into view (`reveal.ts`) and carries a `scroll-margin-bottom`, which still holds; and the held button was described while a path was typed and not yet added, which went with the list of directories. *As filed:* **What the Libraries screen leaves unsaid or out of sight.** A refusal is drawn directly under the button that was pressed, and where that button sits at the foot of the window the sentence is below the fold and the page does not move (run, the final drive: a button at 866 px in a 900 px window, its refusal at 916 px). "Make the library" is held, with nothing to say why, while a path is typed and not yet added to the list. A library removed in another tab draws "there is no library N" on each press, and nothing reads the list again. After a rename or a make whose re-read of the list fails, the sentence says the list could not be read and not that the thing was done, and a library just made is at the foot of the list. The last three were read or driven by a reviewer against a mocked client | `Libraries.tsx`'s `made` and `reread`, `LibraryPage.tsx` | F2a's whole-branch review, 2026-10-06 |
| O210 | [x] | **Closed 2026-10-08: `derive::write_work` holds a work's writes in one transaction, where the import's own transaction and three writes after it each committed on its own, and each commit waited for the disk.** Those were `write_roots`, the `UPDATE work` and `write_edition_metadata`, and `Library::configure` sets no `synchronous` pragma, so SQLite synced every commit. Over a copy of the real library's store a scan went from 11.6 s to 3.7 s, a move from 12.7 s to 5.0 s, and the syncs from 7,774 to 2,023. A rekeyed work commits twice, so its document is renamed only after the key it follows is committed, unless a caller holds the derive in a transaction of its own, as a rule save does and as on `main`. *As filed:* **The derive a scan runs after its walk takes about twelve seconds over a copy of the real library's store, on `main` as on F2b.** The store was the drive's copy, the real library's 1,896 works and its two scratch volumes. `taiyaku library scan` with its derive took about twelve seconds with `main`'s release binary and with the branch's, and 0.02 s with `--no-derive`. A real move derives again after its commit: the drive's two moves answered in 12.5 s and 12.3 s, where the first rehearsal, which derives twice under `Documents::Skip`, answered in 1.4 s. Where the time goes, and how a move's own steps divide it, is not measured | `derive::write_work` | F2b's drive, 2026-10-08 |
| O211 | [ ] | **Six mutants in lines of `fold.rs` that F2b did not change survive every test.** A mutation sweep of `relocate.rs` and `fold.rs` whole, at 7b31b366, missed six, none in F2b's diff: `fold::pair`'s body replaced four ways, its `<=` as `>`, and `absorb`'s `rest > 0` as `rest >= 0`, in the sentence that lists the first few conflicting rows | `fold::pair`, `fold::absorb` | F2b's mutation sweep, 2026-10-08 |
| O212 | [ ] | **A series a move empties is left in no library, with what was put on it.** Where the moved directory was its library's last, and a series of the directory's joined the target's series of the same title, the old series row stays with no work and no library; the derive counts it under `series_holding_no_work`. Its alternative titles, tags, external ids and collection places stay on that row. Seen in a review's scratch states, not driven | `relocate::refile`, `derive::reconcile_series` | F2b's task and whole-branch reviews, 2026-10-08 |
| O213 | [ ] | **A work of a third library placed by hand in the moved directory's series follows that series into the target, where the directory was its library's last.** The library goes with its last directory, the series is left in no library, and the target's derive takes it; a work elsewhere whose `series_id_hand` names the series then points into the target library. It pointed into another library before the move as well. Where the old library keeps another directory, the series stays there. Read from the code, not driven | `derive::reconcile_series`, `relocate::refile` | F2b's task review, 2026-10-08 |
| O214 | [ ] | **A request waiting for a verify permit waits on a blocking-pool thread, and nothing bounds how many may.** Since O57 a password verify needs one of four permits, and a request without one parks the `spawn_blocking` thread it is on. Guard resolution for every kind of credential, every grant check and every handler's own work need a thread of the same pool, which is tokio's default 512, so once that many are parked every request waits behind them: a reader with a cookie or a key, and a static file, as much as a login. A caller already locked out is refused before it waits, so what can fill the pool is a burst that arrives before its own failures land, a flood from many addresses, or one account presenting its right password over Basic faster than four verifies at a time can answer, since a verify that passes records nothing. The rate at which that starts is four over one verify's time, whatever the core count. Measured by the whole-branch review with a flood of wrong Basic passwords, each from its own address, and a reader with a key probing beside it, on release builds of `main` and of the branch: pinned to eight cores the branch was no worse than `main` at any rate tried and far better past about 350 a second, where `main` held 13 GiB and its reader waited seconds; pinned to sixteen, at 420 a second, the branch's reader waited seconds and `main`'s mostly did not (`docs/notes.md`, "O57, O60 and O138"). **It needs no bound to close**: `login`, `guard` and `change_password` are `async`, so the permit can be awaited on a `tokio::sync::Semaphore`, as `download.rs` awaits its own, with the first lockout read in a short pool task and only the four holders on pool threads; a waiter then costs a future. Not done on the branch that closed O57, which had been reviewed around a blocking wait | `session::VerifyLimit::permit`, `limit::Semaphore::acquire`, `auth::guard` | the design that closed O57 and its whole-branch review, 2026-10-12 |
| O215 | [ ] | **What no test holds about the verify permits, and two older tests that do not reach the branch they name.** Found by O57's reviews. Each of the first seven was put back by hand on the branch that closed O57, and the server's tests whose names carry the permits', the limiter's or the passwords' words stayed green. On the login route, a permit given back after the lockout is read under it and taken again before the verify. The pair's half of the login route's lockout read before the wait, and the address's half of Basic's: each test in which a locked-out caller is refused while the permits are held raises one key. One key's read failing open while the other still fails closed, run for Basic's address read: the tests that break the read drop the whole `login_attempt` table. A failure that cannot be recorded being answered as a lockout, on the login route, Basic and the password change. `a_failure_the_limiter_cannot_record_is_answered_as_a_lockout` and `a_basic_failure_the_limiter_cannot_record_is_answered_as_a_lockout` are named for that and are older than O57; both drop `login_attempt`, so the lockout read made before any verify fails closed, and that read is what answers their 429. A commit hook that rolls the record back reached the branch when a reviewer tried it, and a trigger that refuses a write to the table while a read succeeds would. The permit given back on the success path of the login route and of Basic; the password change's is held. The claim route taking a permit while it holds the library's connection; `account::update` doing so was not run. The order in which a permit gives back its memory and then its place, on which "no more memories than permits" rests. **Read and not run:** a permit waited for on a runtime worker; a place a password is verified in a memory made for the call, which `credential::WorkingMemory::new` and `verify_password_in` being public for `taiyaku-serve` leaves open to write; and that the server's hashing shares one kept memory, which rests on each of its call sites holding the library's connection. The wipe of a working memory in the shipped binary rests on a disassembly of one release build (`docs/notes.md`, "O57, O60 and O138") | `session::login`, `auth::from_basic`, `session::change_password`, `session::VerifyPermit`, `credential::verify_password_in` | O57's task reviews, whole-branch review and the review of its fixes, 2026-10-12 |
| O216 | [x] | **Closed 2026-10-12 by the user's own hand check that day, and by the two things the user then asked for.** In Firefox, on `main`'s build served on this machine, a notch of the wheel moved the strip "just over 1 page" and "it feels fine": those are the Firefox reading and the wheel under a hand this row asked for, and the step is left as it is. For the decision this row left open the user asked for a sign of where the strip is, and for a drag. **The mark.** The progress line under the strip carries a mark for how far along its travel the rail has been scrolled (`Chrome.tsx`'s `reader-progress-shown`, from `tiles.ts`'s `shownShare`): flush with the start of the line at the start of the volume and with its end at the end, an eighth as wide as the share of the volume the strip shows and never under 32 px, half transparent (`--reader-mark`) so the fill shows through it, and hidden where the strip shows the whole volume. The line runs the way the strip runs: for a work read right to left it is mirrored, its fill with it, so the read-so-far fill grows from the right there, where it grew from the left. The mark is moved without a render (`moves the mark for a scroll with no render of the bars or of the strip`) and reaches the line's end on a rail that barely overflows (`follows a rail that barely overflows to its end a pixel at a time`). **The drag.** A mouse's primary button pressed on the rail and carried 5 px along it drags the rail one to one, in either reading direction (`Filmstrip.tsx`, `tiles.ts`'s `STRIP_DRAG_SLOP`; `follows the pointer one to one, both ways, and turns no page`). The click after a drag is swallowed; focus the press moved onto a tile goes back to where it was before the press, so that Space and Enter do not turn to that tile (`hands focus back when a press becomes a drag, and Space and Enter then turn no page`); and a press ends where its button comes up, on the strip or elsewhere in the page. **The glide.** The user tried the built drag and asked for momentum ("strip dragging works, but there's no momentum"), and of a prototype of it said "yeah it feels good". A drag let go while the pointer is moving carries the rail on: the speed is read over the 100 ms before the drag's last move, and the rail travels that speed times 325 ms in all, slowing exponentially (`tiles.ts`'s `releaseSpeed`, `glideTravel` and `glideSpeed`). There is none where the pointer had rested 60 ms, under 0.1 px a millisecond, where the reader has animation off or the system asks for reduced motion. A glide stops at an end of the rail, at a press on the rail, at a wheel, at a page turn's recentre, at a change of reading direction, and where a tile of the rail takes focus. A mouse's press that stops a glide still carrying the rail at 0.1 px a millisecond or more is no click (`stops at a press while it is still carrying the rail, which turns no page and leaves focus where it was`); slower, it is the click it looks like (`takes a press as the click it is once the glide has slowed under 0.1 px a millisecond`). A finger's or a pen's press begins neither a drag nor a glide, and stops a glide that is under way; they scroll the rail as they did. **Not measured:** Firefox and Safari by any test, the suite's browser being Chromium (the user tried the mark's prototype, the built drag and the glide's prototype by hand); a physical finger or pen (the glide's review sent a touch through the browser's own input protocol: a tap stopped a glide and turned the page, and a pan was the browser's alone); a release outside the browser's window; and what the two style writes a scroll frame now makes cost on a phone. **Not held:** the rail's standing is read through `Math.abs`, so a rail pulled past its start would read as scrolled into the volume and the mark would move in from the line's start. It was not seen in a browser. Readings: `docs/notes.md`, "O216". *As filed:* **How far a notch of the wheel moves the strip is not measured in Firefox or under a hand, and a mouse has lost its fast way across a long volume.** Since O138 the rail draws no scrollbar and a vertical wheel scrolls it, 100 px a notch. `tiles.ts`'s `stripWheel` reads a wheel that reports lines and one that reports pixels as the same notch, but by the whole-branch review's account of Firefox's source, which it had from a summary, Firefox hands a page pixels and `deltaMode` 0 when the page reads a wheel's deltas before its `deltaMode`, as `stripWheel` and the zoom's `wheelBy` both do; a notch in Firefox may then move the strip by Firefox's own pixel figure for three lines. A script-made line-mode event is exempt and Playwright's wheel is sent in pixels, so neither showed it. Every reading behind O138 used Playwright's wheel or a made event: none was a wheel under a hand. And the bar's thumb was a mouse's fast way across a long volume; a notch is about one tile. Whether that wants a larger step, a drag, or nothing is a decision, and the Firefox reading is what a larger step would be sized against | `Chrome.tsx`'s mark and `placeMark`, `tiles.ts`'s `shownShare`, `STRIP_DRAG_SLOP`, `releaseSpeed` and `glideTravel`, `Filmstrip.tsx`'s `onShown`, `glides` and drag effect, `styles/tokens.css`'s `--reader-mark` | O138's whole-branch review, 2026-10-11 |
| O217 | [ ] | **A stored password hash that names a larger memory cost grows a working memory for good.** Since O57 a verify runs in a memory its permit owns, sized on first use to what the stored hash's own parameters ask for and kept. A hash naming a larger `m` than this server writes makes the memory that verified it that much larger for the life of the process, and one naming a very large `t` holds a permit for as long as argon2 runs, as it would have held a thread before. `argon2` itself has no ceiling below `u32::MAX` KiB, and refusing such a hash would change who can sign in. Only someone who can write `user.password_hash` can plant one, and that someone can already write an administrator's hash. Recorded so that a ceiling, or giving the excess back after an outsized run, is a choice somebody made | `credential::WorkingMemory`, `credential::verify_password_in` | Task B4's review on the branch that closed O57, 2026-10-12 |
| O218 | [x] | **Closed 2026-10-12: the build's CSS step is told which browsers the UI supports, and `light-dark()` ships as written.** The cause was a default. Vite hands the stylesheet to Lightning CSS with `build.cssTarget` as the browsers to write for; unset, it takes `build.target`'s value, which resolved to Chrome 111, Edge 111, Firefox 114, Safari 16.4 and iOS 16.4, and for those Lightning CSS compiled each of the thirteen paired tokens to a pair of `var()` fallbacks, substituted on `:root`, where the tokens are declared. `vite.config.ts` now sets `build.cssTarget` to Chrome 123, Edge 123, Firefox 120, Safari 17.5 and iOS 17.5: for each browser the first version at which Lightning CSS left the function as written, read one version either side, and for Safari the version spec 2026-09-17 §3 already requires for `light-dark()`. `build.target` is not set. The other way this row named, declaring the token block on `.reader` as well, was not taken: the tokens are declared once, on `:root`, as before. **The built stylesheet** declares the thirteen paired tokens as `light-dark()` pairs and names neither switch property, and `main`'s built stylesheet with the compile undone by a script is the branch's byte for byte. **Read in the app:** the branch's build served by `taiyaku serve` from a copy of the test library, in headless Chromium 151 and Firefox 153, under a light and a dark colour scheme with the theme left to the system, pinned to Paper and pinned to Ink. In those twelve readings the progress line is `rgb(53, 49, 42)`, the readout `rgb(166, 155, 135)`, and the bottom bar's text and the top bar's `rgb(236, 228, 212)`, the Ink halves; the mark O216 put on the line is `rgba(236, 228, 212, 0.5)`. On `main`'s build a page of the bottom bar's markup read the Paper halves in the three of those six combinations where the page's own scheme is light, in the two browsers (`docs/notes.md`, "O218"). **Held by** `builtStylesheet.test.ts`, which builds `index.css` with the project's config in the `node` project: with any one of the five versions lowered by one, or the line deleted, fourteen of its fifteen cases fail. **Not measured:** Safari; a browser older than the five versions, which the build now gives `light-dark()` as written where it gave `var()` fallbacks before; the reader's panels | `web/vite.config.ts`'s `build.cssTarget`, `web/tests/builtStylesheet.test.ts` | found while prototyping O216's mark, 2026-10-12 |
| O219 | [x] | **Closed 2026-10-18 by O219's second PR: Configuration → Users is a table, a new-account page and an account's page.** `/config/users` is a table of Name, Role, Libraries and Last seen whose rows each open an account's page (`UserTable.tsx`; `Users.test.tsx`'s "draws a table of Name, Role, Libraries and Last seen, each row a link to its account’s page through its name"); `/config/users/new` takes a name, a password and a role, and goes to the page of the account it made (`NewAccount.tsx`); and an account's page carries what its card did, under the headings Role & permissions, Libraries, Password and Sessions & keys, with Remove…, whose removal goes back to the table and says "Removed `<name>`." there (`AccountPage.tsx`; spec `2026-10-17-o219-config-tables-design.md` §2 and §4). Focus follows §5: a make puts it on the made account's heading, a removal on its line, an unknown id on "No such account.", and the make page's Cancel on the table's heading. A removal's answer moves the person only while they are on the page that asked, and a make's only while the form that sent it is drawn; one that comes after they went elsewhere leaves them there, and its line, or a refusal whose page has gone, is drawn on the page they are on without taking focus. The alert for a libraries list that could not be read is drawn on an account's page only, the one page with a grant editor. Under 36rem each row of the table is a stacked block reading "Role: …", "Libraries: …" and "Last seen: —", and no Users page scrolls sideways at 390 px (`Config.browser.test.tsx`'s "stacks each row of the table into a block of two lines, each value after its label"). Libraries and Users place a late answer through one copy, `configPages.tsx`'s `useLateAnswers`, and draw "No such …" through `Missing.tsx`, as the user ruled on 2026-10-18. *Narrowed 2026-10-18 by O219's first PR:* Configuration → Libraries is a table, a library's page and a folder's page, and a library is made from a name and one folder (`LibraryTable.tsx`, `LibraryPage.tsx`, `FolderPage.tsx`, `NewLibrary.tsx`); Libraries and Users lead the Configuration tabs, so a bare `/config` lands on Libraries (`config.ts`'s `TABS`). *As filed:* **Configuration → Libraries is hard to follow, and cluttered and long.** Trying F3's build on a scratch copy on 2026-10-17, the user found making a library confusing (a name, then Browse… or a typed path, then "Add to the list", then "Make the library") and every library card repeating the same add-folder form (O199's repeated labels), and the screen "cluttered and long". Ruled the same day: redesigned after F3, as a piece of its own. Widened the same day, after trying O220, to Configuration → Users, with the user's direction for both: a table with a row per library or account, where a row opens that one's own edit page, not a list of cards. Designed with the user on 2026-10-17 (to be tried on a prototype): a folder has its own page under its library's, and a library is made from a name and one folder, with more added on its page | `Users.tsx`, `UserTable.tsx`, `NewAccount.tsx`, `AccountPage.tsx`, `configPages.tsx`'s `useLateAnswers`, `Missing.tsx`; `Libraries.tsx`, `LibraryPage.tsx`, `FolderPage.tsx` | the user's try of F3, 2026-10-17; closed 2026-10-18 |
| O220 | [x] | **Closed 2026-10-17: a library that is some account's only grant can be removed, and the account is then left seeing no library, never every library.** The user ruled that day that such an account sees nothing until an administrator grants it a library, over choosing per account in the dialog or widening it to every library after a warning (spec `2026-10-17-o220-no-library-design.md`). **The store:** each account has `user.every_library` (migration 17), which that migration set to 0 for each account holding a grant and left at 1 for the rest; 0 with no grant sees nothing, so an empty grant no longer means every library. **The server** carries an account's reach as `account::Access`, `Every` or `Only` its folders, and `Only` with no folder is a false clause in SQL (`Access::grant_clause`), never a clause left out. **What went:** `scan::removal_refusal`, `scan::library_removal_refusal` and `only_grant_of`, and Move's own only-grant refusal in `relocate::refused_before`; migration 15's `only_grant_refuses_delete` and `only_grant_refuses_move` triggers, which migration 17 drops; and O205's guard. **Remove and Move** go through. Their dry runs and answers name the restricted accounts left with no library (`left_with_no_library`, from `scan::only_grant_holders`, which names no administrator and no account set to every library), and `RemoveLibrary.tsx`, `RemoveRoot.tsx` and `MoveRoot.tsx` say so under the counts (`libraries.ts`'s `noLibrarySentence`). **The account:** `/api/library/status` says `restricted`, and Home and a library's page draw "No library shared" (`EmptyState.tsx`'s `no-grant`). **The Users card** clears the last tick, saves "Only these" at once as no library, says "This account sees no library." on an account that is not an administrator, and lifts a limit with `every_library: true` (`UserRow.tsx`). **The CLI:** `library user every-library <user>` lifts a limit, and `revoke-library` of the last grant says the account now sees no library. **Not changed:** what an administrator sees; and Search, which draws its ordinary empty result for such an account (O221). *O221 has since closed: such an account is offered no search box — see its entry below.* *As filed:* **A library that is some account's only grant cannot be removed, and the user wants it removable.** The user said on 2026-10-17 that it should be possible to remove a library an account can see even when it is that account's last. F1 refuses it, naming the accounts (`scan::only_grant_holders`; migration 15's `only_grant_refuses_delete` and `only_grant_refuses_move` triggers), because an empty grant means unrestricted: the account would see every library afterwards. What such an account should see after the removal is not ruled | `account::Access`, `account::access_for`, `017_every_library.sql`, `scan::only_grant_holders`, `scan::Removal::left_with_no_library`, `relocate::Moved::left_with_no_library`, `libraries.ts`'s `noLibrarySentence`, `EmptyState.tsx`'s `no-grant`, `UserRow.tsx`'s `toggleGrant`, `taiyaku-cli`'s `user::every_library` | the user's try of F3, 2026-10-17; closed 2026-10-17 |
| O221 | [x] | **Closed 2026-10-19 by the sidebar and readability PR: an account that sees no library is offered no search box, and the Library view's "No library shared" says why.** Search is a bar on the Library view now (spec `2026-10-18-sidebar-readability-about-design.md` §2), and the box follows what the grid shows: where the grid's empty state is "No library shared", "No such library" (a `?library=` naming no library, an unknown id or a value that is no id) or "No library yet" on a server with no library, there is no box and no search is run, and a `?q=` the page arrives with is not sent (`Library.tsx`'s `unsearchable` and `offered`; `Library.test.tsx`'s "offers an account that sees no library no box, and says none is shared — O221", "offers no box on a server with no library, where the grid says none is made yet" and "offers no box at …, and says there is no such library"). The server with no library is the PR's own ruling, by the same reason: there is nothing to search. *As filed:* **Search gives an account that sees no library no hint that nothing is shared.** `Search.tsx` draws `EmptyState`'s `no-matches` for such an account ("Nothing here", "Nothing matched …"), as it does on a server with no library. Home and a library's page say "No library shared", the sidebar lists no library, and O220's spec (§6) asks only that search finds nothing, so this is a choice and not a defect. If it is wanted: draw `no-grant` where `status.libraries.length === 0 && status.restricted`, as `Library.tsx`'s `emptyKind` does | `Library.tsx`'s `unsearchable` and `offered`; `Search.tsx`, a redirect now | O220's whole-branch review, 2026-10-17; closed 2026-10-19 |
| O222 | [ ] | **Narrowed 2026-10-27 by the loading work's first PR: a return to the Libraries screen before the answer to a write sent from it draws the list as the write left it once the answer lands.** The answer marks the list stale in the read store, not in the screen that sent the write, so the screen drawn on the return, which draws the list, reads it again then (`Libraries.tsx`'s doc; `Libraries.test.tsx`'s "draws the list as a write sent from the screen left it, with no change of page, after another tab replaced the screen and $order — O222", whose "the return, then the answer" case fails on `main`). **Still open:** a removal's or a move's report, or the refusal of any write sent from the screen, that comes once Users, Rules or Backup has replaced it is dropped with the screen, on Users as on Libraries. *Re-read 2026-10-18 against O219's second PR, and kept:* the Users screen now places its answers through the same `useLateAnswers` (`configPages.tsx`), whose `place` sends nobody once another tab has replaced the screen and whose lines go with the screen, so a removal's line or a refusal that comes then is drawn nowhere on Users too, as on `main` (`Users.test.tsx`'s "leaves the person on another Configuration tab when a removal answers there, and says it nowhere"). *As filed:* **An answer that lands after another Configuration tab has replaced the Libraries screen is drawn nowhere.** A removal's or a move's report, or the refusal of any write sent from the screen, that comes once Users, Rules or Backup has replaced it is dropped with the screen; and a return to the screen before the answer shows the list as it was until the next change of page: the screen drawn on the return read it before the write answered, and the read the answer sets off belongs to the screen that went. As on `main`, where the report lived in the same screen. Inside the screen such answers are drawn on the page the person is on (spec `2026-10-17-o219-config-tables-design.md` §5). | `configPages.tsx`'s `useLateAnswers`: its `place`, which `Libraries.tsx`'s `answered` and `Users.tsx`'s `removed` call, and its `say`, which the pages take as `onLateRefusal` | O219's whole-branch review, 2026-10-18 |
| O223 | [x] | **Closed 2026-10-19 by the sidebar and readability PR, which opened it: the search box keeps every character typed into it, however fast.** The box draws its own text, and `?q=` sets it only on a change the box did not make — Back or Forward, a link, the redirect from `/search`. Each change the box makes to the address carries a tag in the router's navigation state, so the late commit of an earlier key's URL is told apart from any other change, where by value the box's own late request and a link to the same query could not be (`SearchBar.tsx`; `SearchBar.browser.test.tsx`'s "keeps every character, in the box and in ?q=", typed with real keys, and `SearchBar.test.tsx`'s "keeps its own text when the URL of an earlier key commits after a later one — O223"). *As filed:* **`main`'s Search box drops keystrokes typed fast.** `Search.tsx` drew its box from `?q=`, which the router commits inside a transition, so the URL lagged the keystroke and React's controlled-input reset put the box back to the older value for the next key to land on. On `main`'s running server it held "osi", "ohi" and "oshi" over three fast typings of "oshi" | `SearchBar.tsx`; `Search.tsx` on `main` | by hand on `main`'s running server while planning the sidebar and readability PR, 2026-10-18; closed 2026-10-19 |
| O224 | [x] | **Closed 2026-10-29 by the loading work's second PR, once the user's iPhone had gone Back to the grid and the server's log held no cover request for any of those returns. Each library cover's address carries a version of its bytes, which the thumbnail route answers with `COVER_CACHE_CONTROL` (`private, max-age=31536000, immutable`), so the browser keeps it; and the Library view, a series page and search read through the store, so a return draws the rows left behind, with the same cover addresses, in its first render.** The list answers carry each cover's address (`library_api.rs`'s `cover_address`, with `cover::cover_version`), the web draws it through `coverSources` and no longer asks for the series cover `307`, and `pageReads.ts`'s `gridRead`, `homeRead`, `seriesRead` and `searchRead` keep the answers for the tab (spec `2026-10-28-loading-library-design.md`). **On the iPhone** (Safari, `Version/27.0.1`, 2026-10-29), the returns to the grid logged `00:19:02.045 GET 200 /api/library/series` and `00:19:02.045 GET 200 /api/library/home`, `00:19:28.902 GET 200 /api/library/series` and `00:19:28.902 GET 200 /api/library/home`, `00:19:35.519 GET 200 /api/library/series`, `00:19:49.393 GET 200 /api/library/series?library=1`, and, for the reader's Library button, `00:19:59.339 GET 200 /api/library/series?library=1` and `00:20:04.190 GET 200 /api/library/series?library=1`. No image request followed any of them but the first, whose next, 8.6 s later, was a cover asked for the first time, from scrolling on. The phone's 269 requests held no `307`, no `304` and no `/cover` path, and asked for no cover or thumbnail address twice; the user found every return where they had left it (`docs/notes.md`, "The cover version", has the lines in full). **What remains** is O230: in Chromium, after heavy image use, a return drew the grid before its covers, read again from the browser's HTTP cache with no request. *As filed:* **Going Back to the Library view flashes every cover while the grid is drawn again.** The user saw it on 2026-10-19 on an iPhone (Safari) in the sidebar and readability PR's device check, on that branch's build and on `main`'s (`e4f2e4e5`), so it predates the PR. The server's request log shows the same on both builds for each Back to the grid: the view reads `/api/library/series` and `/api/library/home` again, and the covers are asked for again, `/api/library/series/<id>/cover?w=768` answering 307 and the thumb it redirects to 200. **What the code does:** leaving the Library view unmounts it, since each route in `Routes.tsx`'s `AppRoutes` draws its own screen, so `Library.tsx`'s `loaded` is empty again on the return, and its effect on `library` reads both lists afresh through `library.ts`'s `grid` and `home`, plain `getJson` calls that keep nothing between them; each tile's `Cover` is a new image. `library_api.rs::cover` sets no `Cache-Control` on its 307, so the guarded router's `per_reader_by_default` gives it `private, no-store`, as it gives both lists. The thumb at a cover size is `routes.rs::get_thumb` handing page 0 to `serve_page`, which sends an `ETag` and `PAGE_CACHE_CONTROL` (`private, no-cache`), and answers a matching `If-None-Match` with a 304. **Not established:** the cause, and so whether the fix is the client keeping the grid's rows across a Back, the cover redirect or the thumb being cacheable, or both; which to do is a decision for later, filed with the loading work's second PR in Later | `pageReads.ts`'s `gridRead`, `homeRead`, `seriesRead` and `searchRead`; `library_api.rs`'s `cover_address`; `cover.rs`'s `cover_version`; `routes.rs`'s `serve_page` and `COVER_CACHE_CONTROL`; `library.ts`'s `coverSources` | the sidebar and readability PR's device check, 2026-10-19; closed 2026-10-29 |
| O225 | [ ] | **A Filmstrip browser test fails now and then, and passes on the rerun.** `Filmstrip.browser.test.tsx`'s "stops when Tab brings focus to the rail, and leaves the rail where the browser scrolled it" failed in its `rtl` case on 2026-10-20, at the head of a branch that changed only docs (expected 1546 to be <= 1440), and in its `ltr` case on 2026-10-21, in `check-all.sh`'s first run on the folder-to-directory rename's branch. Neither run left a coredump, so it is not the renderer crash the browser gate has had before, and each time the whole browser project passed on the rerun. **Not established:** the cause, or whether the test or the strip is at fault | `Filmstrip.browser.test.tsx`'s "stops when Tab brings focus to the rail" | two gate runs, 2026-10-20 and 2026-10-21 |
| O226 | [ ] | **Mutation-baseline keys in files the rename did not touch no longer name the lines they were written for.** The folder-to-directory rename re-keyed the keys its own edits had moved, and its review found about fifteen more still stale in files it left alone: in `taiyaku-library`, `librarydoc.rs`, `lib.rs`, `migrate.rs`, `preview.rs`, `schema.rs` and `state.rs`; in `taiyaku-read`, `format.rs`; in `taiyaku-serve`, `session.rs`. A stale key matches no mutant, so the next sweep reports the survivor it records as new, and the entry itself as an accepted survivor a test now sees (`scripts/mutants-baseline.mjs`). **The fix is the rename's own re-key:** list the mutants with `cargo mutants --list` in a temporary worktree with a `CARGO_TARGET_DIR` of its own, pair each stale key with its mutant by description, check each against the source line at the commit that wrote the key, and drop the entries a test now kills | `docs/mutants-baseline.md`'s entries for the files named | the rename PR's review, 2026-10-20 |
| O227 | [ ] | **A server with no library yet opens on a Library view whose top is mostly empty.** The user saw it on 2026-10-22 on a new server: above "No library yet" the first row holds only the View button, and the row under it holds the All, Multi-edition and Unread filters and the Added sort, with nothing for them to act on. **What the code does:** `Library.tsx`'s `page-head` withdraws the search bar where `unsearchable` holds (O221) and keeps the row for `ViewMenu`; the `library-chips` row is drawn whatever the grid holds, above `EmptyState`'s `no-libraries`. By the code, the same two rows stand over "No library shared" and "No such library". **Not decided:** what the view keeps when there is nothing in it | `Library.tsx`'s `page-head`, `unsearchable` and `library-chips`; `EmptyState.tsx`'s `no-libraries` | the user's look at a new server, 2026-10-22 |
| O228 | [x] | **Closed 2026-10-25 by the Configuration grouping's third PR: a key that is made is announced beside its box without its secret, and Change password and Create keep focus while their request is out.** The user ruled that day to announce the key, and, once they had tried the build, where. When Create makes one, a visually hidden polite region in API keys, inside the form that made it and so beside the secret's box, says `Key made: <name>. Copy it now — it is not stored and cannot be shown again.`, or `Key made. …` for a key with no name, so the line takes no room and moves nothing. The line never says the secret. It goes when the next key is asked for, so a second key of the same name is said again, when the panel closes, and when the shown key is revoked. "Password changed" is said the same way, from a visually hidden polite region in Password, beside its form, in the render that draws the Signed in list after the change; nothing above the sections says it, and a Create leaves it. The secret's box is drawn with the made key's row, by the listing that follows the create, and its caption names the key: `Key “<name>”: copy this now — …`. A listing that lands after a later one has been drawn is not drawn, and a later one that lands first does what the earlier one waited to do, so the box still comes with its row (`landWaiting`). The box goes with that row, by the listing that follows a revoke of the key it shows, where the listing after its create held that one key new to it; a revoke of another key leaves it, and so does one that answers once a later key's box is drawn. Where the clipboard can be written, the box is a button drawn as the box and named by its caption: a click, Enter or Space copies the secret, and "Copied" is said as Copy address says it (`useCopy`, shared with `CatalogAddress`) and drawn on the box in its caption's place for a moment, moving nothing (`CopyWords`, Copy address's own). Elsewhere, and after a refused write, the secret is text to be selected by hand; after a refusal it is selected, with focus on its box. Both submits are `HeldButton`s, drawn held once a request has been out 200 ms (`useDrawnWait`), so the one pressed keeps focus; the handlers' `busy` guard turns a second press down from the send, and one request out holds both, a create's until the key list after it lands and a change's until the Signed in list does. While the new password and its confirm differ, Change password is held too, and a press of it says why (spec §4). When a Revoke's row goes with focus on it, focus goes to the nearest row's Revoke in that list, else to the New key box or Signed in's heading, so it is not dropped to the page (`useFocusAfterRevoke`). All of this holds on `/account` and in the reader's account panel, whose `.reader-button` draws a button held by `aria-disabled` as a disabled one, under forced colours too. (`Account.test.tsx`'s cases on the line, its lifetime, the box's revocation and its copy; `Account.browser.test.tsx`'s "says a key was made in API keys and moves nothing", "copies the secret on a click, Enter and Space on its box", "draws Copied on the secret’s box for a moment after a press that copied", "says Password changed with the Signed in list that follows it", "keeps focus on Change password", "keeps focus on Create" and "gives focus, as Enter on each Revoke takes its row, to the next key’s Revoke", in both placements.) *As filed:* **A new API key's box is drawn after Create, and nothing announces it.** On `/account`, and in the reader's account panel, which draws the same `Account` component, a key that is made is shown in a box under the form, with "Copy this now — it is not stored and cannot be shown again." The box is not in a live region and focus is not moved to it, so a screen reader is not told that the key exists. A live region would read the secret aloud. **Options:** announce that a key was made without saying the key, or move focus to the key's box. **Not decided:** which; it is decided with PR 3's `/account` work. On the same page, its two submit buttons, Change password and Create (`Account.tsx` ~:313 and ~:337), are held with `disabled={busy}`, so focus on either falls to the page while a request is out; the 2026-10-23 ruling on held controls (keep focus, held by `aria-disabled`) applies to them when PR 3 moves `/account` | `Account.tsx`'s `shown` (`ShownKey`: the secret, its id and its line), `keyRegions`, `passwordRegion`, the secret's copy button in `secretBox` (`useCopy`, `copy.ts`), and `busyDrawn`; `landWaiting`; `CatalogAddress.tsx`'s `CopyWords`; `revokeFocus.ts`'s `useFocusAfterRevoke`; `admin.css`'s `.account .key-form` and `.account .settings-box:has(> .visually-hidden)`; `reader.css`'s `.reader-button` held rule | PR 2's sweep of the success lines its screens draw, 2026-10-23 |
| O229 | [ ] | **The reader's account panel opens only from the keyboard.** Inside a book, Shift+A is the one way to open it: `A` in `keys.ts` resolves to the `account` intent, `keyplan.ts` plans `open-account`, and `App.tsx` sets `accountOpen`. No stop on the reader's bar, no button and no entry in the ⚙ settings panel opens it, so on a phone or a tablet the panel cannot be opened from a book; `/account`, reached from the library's sidebar, draws the same `Account` component. The user looked for it on 2026-10-25 and could not find it. **Options:** a stop on the reader's bar, an entry in the settings panel, or a link to `/account`. **Not decided:** which | `keys.ts`'s `A` binding, `keyplan.ts`'s `open-account`, `App.tsx`'s `accountOpen` | the user's look at PR 3's checkpoint server, 2026-10-25 |
| O230 | [ ] | **After heavy image use, Back to the Library view can draw the grid before its covers.** In the loading work's second PR's live check, in Chromium on 2026-10-29, after One Piece's 111 thumbnails and three visits to the reader, Back to the grid kept its scroll and had 1 of 18 visible covers complete at React's commit, and 18 of 18 at +590 ms. No cover was asked of the server: they came back from the browser's HTTP cache under `COVER_CACHE_CONTROL`, so the browser's in-memory image cache had dropped them. A return by the reader's Library button had 1 of 18 at its commit too, and the user's iPhone asked for no cover or thumbnail address twice (`docs/notes.md`, "The cover version"). **Not established:** why the browser drops them, and whether the app can keep them drawn; the user filed it on 2026-10-29 as a known limit, to be investigated later (spec `2026-10-28-loading-library-design.md` §10) | `Cover.tsx`'s `Cover`; `library.ts`'s `coverSources` | the loading work's second PR's live check, 2026-10-29 |
| O231 | [ ] | **`open_archive` reads its 512-byte header with one `read`, so a short read can make a tar look like a cbz that will not open.** `open_archive` fills a `[0u8; 512]` buffer with a single `file.read` and passes the first `read` bytes to `unsupported_container`. `Read::read` may return fewer bytes than were asked for, and the tar magic sits at offset 257, so a read that stops short of it gives `from_header` no tar to recognise. `unsupported_container` then answers `None`, `ZipArchive::new` fails on the tar, and the error says "not a readable cbz" instead of "this is a tar archive". `format.rs`'s `read_header` is the loop that avoids this and its doc warns of exactly this failure; `open_archive` does not use it. **Found by reading, during the comments pass on 2026-10-29; not reproduced.** **Not established:** whether `open_archive`'s read is ever short in practice; `read_header`'s doc says a read comes back short over a network filesystem. The effect would be a wrong message on a file taiyaku cannot read either way. The fix comes later, in its own PR | `open_archive` in `taiyaku-read`'s `pages.rs`; `read_header` in `format.rs` | reading `open_archive` during the comments pass, 2026-10-29 |

Sites are named by symbol rather than line, because a line number in a document
is wrong the first time someone edits the file above it.

**Two of these came from one device session, and it is the first time a real
OPDS client has been driven against this server.** Spec C2's second half needed
that run to prove its own purpose — a download an OPDS client can reach — and
it settled O55 in the same breath, since a feed no client will authenticate to
cannot be proven end to end. What the run then showed on screen is O67: the
titles Panels renders are the titles this scanner derived, and a work holding a
colour edition beside three monochrome ones is named after the colour file. No
review had found it in five specs, and no test can — every assertion in the
suite is over data a fixture wrote, and a fixture names its work `T`. What
found it was one shelf rendered by something this project did not write. **O68
came from the same run and from the opposite direction** — not from what the
client displayed but from what the server recorded about it, which turned out
to be `bytes: 0` for every download. Worth separating from O67 rather than
folding into it: one is a defect in what this project stores about a work, and
the other is a defect in what it can observe about its own behaviour. The
second is the more awkward, because the instrument is what the open-items
process runs on: O66's negative result, taken the same evening, needs a
qualification it would not have needed if the log could say how many bytes
actually moved. *One
thing the run needed is worth recording where a deployment would look for it:
**iOS App Transport Security refuses plain HTTP**, so an OPDS source at
`http://…` reports that the server cannot be found, whatever the credentials.
The session reached the server behind TLS instead.*

**Four came from spec C3's own task reviews, and each is ground the branch was the first to stand on rather than the first to break.** The proposals inbox is the first code here to want a format per edition, a status on a write that distinguishes a refusal from a fault, a stall test it could model on an existing one, and a proposal state a person could choose — and asking each of those four questions found what was underneath it thinner than it looked. O69 is a column five specs wrote and none read. O70 is an error type that was adequate while only a CLI read it and stops being adequate the moment a status code is derived from it; this branch narrowed it rather than closing it, and its row says which half is left. *O70 has since closed, on 2026-09-14 — see its entry above.* **O71 is the uncomfortable one, because the defect is in a test** — `a_progress_read_does_not_stall_the_runtime` reports a property it is not positioned to observe, and how that was settled is worth more than the item: move the handler onto the async worker, which is exactly the defect the test names, and see whether it goes red. It did not. *(O71 has since closed, on 2026-09-14 — see its entry above. The same check run on the guard in front of that handler did turn the test red, so it was kept, renamed for what it sees.)* That check costs one edit and is worth running against any test whose subject is something *not* happening, because such a test passes both when the property holds and when the test cannot see it. O72 is not a defect in code at all but a spec clause with no surface a person can reach, recorded here so that it is not rediscovered by whoever first goes looking for a dismissal they regret. *(O72 has since closed, on 2026-09-15 — see its entry above.)* **None was introduced on this branch and none blocked it**, which is why they are rows rather than commits.

**One came from running the branch's own feature against real archives, and it
closed the same way.** Accepting a proposal is a rule written now and applied
on the next scan, so nothing about it is observable until a scan runs — and
every accept before this one had run against a fixture whose two series were
three files apiece. Ten hardlinked corpus archives, one accept and one rescan
produced O73: a library holding each moved volume twice, with the command's
own summary line disagreeing with its database on every count it prints.
**Two defects came out of that one run and only one of them was a row.** The
other was `edition.kind` coming back NULL beside a rule holding
`set_kind = 'official'` — the accept form's Edition Kind and Language controls
reaching nothing — and it was fixed on that same branch rather than tabled,
because it needed a decision about one column's writers rather than about what
a work *is*. That the two were separate faults was not established when they
were found, and separating them was the first thing the fix had to do.
**O73 itself closed on this branch, on the measurement it was waiting for.** A
design (`docs/superpowers/specs/2026-08-27-progress-per-edition-design.md`)
widened `progress`'s key to `(user_id, work_id, edition)` so a reader can hold
a place in two editions of a folded volume at once, then `derive::superseded`
and `fold::absorb` move a reader's `progress`, `note` and `truth_*` rows onto
the work the material went to, in one transaction, before running the first
`DELETE FROM work` in production code — refusing the whole fold, per work,
wherever a row cannot be moved safely rather than guessing. Proved against the
same four archives rather than only against fixtures, 2026-08-27: planted
through the API, accepted, rescanned. The database went from 4 works over 4
files to 2, every one of the twelve reader rows planted survived the fold, and
a restarted `serve` handed the reader back their own colour page.

**One came from the branch's own whole-branch review, and it is a gap rather
than a fault.** O74: the un-accept route, its store function and its client
call all exist and are tested, and no screen sends it — so the review's first
reading was "build the surface". Checking what the surface would *do* is what
turned it into a row. `unaccept` deletes the rule and reopens the proposals
that rule answered; by then a scan has run, and `import::replace_editions`
deliberately keeps an edition the incoming document no longer names, so the
editions the rule produced stay standing with the `kind` it gave them. A
control labelled Undo would reopen the question and leave the library exactly
as the accept left it. **Which is the same class of finding as the accept
panel's own sentence** — a screen making a claim the store does not honour —
and it is worth noticing that the review found both on one branch, one before
the button and one after it. The panel's sentence is corrected here; the
un-accept surface is not built here, because building it correctly means
answering O73's cascade question from the other side.

**And one is older than any of this, was found the same way, and is about the
tests themselves.** O75: `IS_REACT_ACT_ENVIRONMENT` is set in
`browser-setup.ts`, and `vite.config.ts` names that file under `setupFiles` for
the `browser` project alone — so the `jsdom` project, where every
`src/**/*.test.tsx` in the repository runs, has none. `browser-setup.ts`'s own
doc already states what that costs: without the flag "`act` does not actually
flush effects synchronously — so a test can pass for the wrong reason,
observing a state React had not committed yet." **What made it invisible is the
next sentence**, which says the flag is set "for the `browser` project only;
the `node` project renders nothing" — two projects accounted for, three
projects present, and the omission reads as a decision. It is the same shape as
the fifteen comment-overreach instances this branch's own reviews turned up,
sitting in the file whose job is to prevent the failure it describes. Nothing
here is measured as *wrong*: a sweep over `AcceptForm` and `ProposalRow` killed
38 of 40 mutants, so those tests do observe real behaviour. What is absent is
the guarantee. The fix is one line and the item exists because turning the
guarantee on may surface tests that were passing on timing, and how many is
not knowable until it is. *O75 has since closed — see its entry above, which
also says why the doc sentence quoted here was wrong.*

**Four more came from closing O73 itself — three from designing and building
the fold, one from sweeping it for mutants once real archives had proved it
fires.** `fold::DISCARDED` already draws the line between a scanner-derived
chapter number and a hand-entered one before counting either as a loss; it
draws no such line for `alignment`, which carries the identical `by` column
recording `'solver'` or `'hand'`, so a `taiyaku align --write` correction is
discarded exactly like a solver's own output the moment the work it lives on
is folded away. **O76.** *O76 has since closed — see its entry above.* Sweeping `fold.rs` and `derive.rs` for survivors
this branch's own `.cargo/mutants.toml` doc calls the point of running —
found one that is neither: `derive::unattached_chapter_dirs`'s
`volume.is_none() && chapter.is_some()` has stood UNTRIAGED in
`docs/mutants-baseline.md` since the very first sweep, and replacing `&&` with
`||` was verified here to still pass the whole `taiyaku-library` suite. An
UNTRIAGED entry is explicitly not an accepted one, and this triage finds a
live gap — an ordinary `v01 - c05` directory would misreport under `||` — so
it is a row rather than a baseline entry, from a function the scanner branch
wrote rather than this one. **O77.** *The substitution no longer passes as of 2026-09-14, caught incidentally — see O77's entry above.* *O77 has since closed, on 2026-09-15 — see its entry above.* Designing the move itself found the
edge the widened `progress` key does not reach: `fold::absorb` already
refuses a fold outright rather than re-key a `note.id` one reader holds on
both works, because migration 005 widened that very key to *permit* two works
sharing one reader's note id. The identical-copy case — the ordinary result of
copying a `taiyaku.notes.json` between volumes — is losslessly resolvable by
keeping one side, and nothing does; `taiyaku-serve`'s own note grandfathering
then means a reader who reaches this state has no ordinary way out of it.
**O78.** *O78 has since closed — see its entry above, which also records that
the grandfathering sentence was never supported: a note the grandfathering
keeps is on a key nothing maps, and `absorb` refuses that earlier, as a
stranded row, before it ever compares note ids.* And the loop that applies a legacy `taiyaku.progress.json` onto the
widened table turned out to have no transaction around it: a failure partway
leaves the positions before it written and the document never marked settled,
so the next run's own "already holds a position" check reads that partial
write as complete and never retries the rest. **O79.** *O79 has since closed —
see its entry above.*

**One pointer is knowingly dangling, and is left that way.**
`schema/001_initial.sql` ends its note about the `truth_` tables with "`export.rs`'s
`LOCAL_USER` comment is the other half of this note: it explains why the
per-person documents get away with one hardcoded id today." Spec A2 deleted that
constant, so the sentence points at nothing. Migration 1 is **not** edited to fix
it: migrations here are append-only, which O45 gives as a reason in its own right
and separately from the `sqlite_master` one. The paragraph's actual *claim* is
still true — `truth` stayed library-wide and only `notes`/`progress` became
per-reader, which is exactly what that comment predicted A2 would find. It is the
citation that went stale, and this is where it is corrected.

**`serve` was a first cut, and five things about it were known thin** — O1 to
O5, each found by the whole-branch review. All five are now closed. O1 was the
last and the longest-lived: it closed once, on a browser-reader `304` count
that looked like the second measurement the first one (against Panels) had
been waiting for — and reopened on review, when the count turned out to be
measuring this task's own traffic as much as a reader's. It closed for good on
the *other* of the two exits its own entry named, which needed no session log
at all. O5 was nearly deferred alongside it on the strength of a wrong
estimate — "a custom rejection handler touches routing for every route" —
which was checked rather than repeated and turned out to be two handlers and a
local parse:

- **O1 — Page responses carry no cache headers. Closed.** *Everything from
  here to "What would close it" is the record as it stood while the item was
  open, kept rather than rewritten, because the reopening is the part worth
  reading. The close is at the end.* The bytes for a
  given work, edition and index never change, and the sidecar already
  records a `sha256` per member, so an `ETag` (shipped the task before this
  one — `routes.rs::locate_member`/`page_etag`) was a line of code once the
  evidence question was fixed (see `notes.md`, "An ETag would have saved
  nothing, and the obvious way to check that is wrong"). The validator
  genuinely shipped and genuinely produces `304`s — each hit turns 150–850 KB
  into roughly 300 bytes, and that saving is real and stays in the record.
  What did not survive review was closing the item on the strength of it: the
  measurement behind the close overstated what it had shown, and the owner
  reopened O1, annotated with the number rather than hidden behind it.
  **Two different real behaviours showed up in the literal scenario, and
  only one of them is about the `ETag`.** Twenty-four `Space` presses fired
  100ms apart against one fixed slot, from a fresh page load — the literal
  scenario asked for — produced **zero** further `/pages/` network activity
  for either edition's URL after each was first displayed: not a `304`,
  nothing at all. A `PerformanceObserver` watching `resource` entries live
  (not a buffered query taken after the fact, which undercounts) recorded
  four requests across the whole run, all prefetch neighbours, none for the
  two centre URLs. A source the browser decoded a moment ago and is asked to
  redisplay does not go back to the network at all — its own short-lived
  in-memory image cache repaints directly. That is cheaper than a `304`, not
  a failure of one, but it does mean the exact swap-on-one-slot scenario
  produces no `304`s to count, for a reason that has nothing to do with the
  `ETag` — see `notes.md` for the full trace.
  **The 17-of-39 figure is real; what it was evidence of was not.** Ten
  slots read forward, swapping to the second edition and back at each one,
  then *the same ten slots read backward*, with the same swap at each,
  produced 39 `/pages/` requests: 22 full `200`s, 17 `304`-sized responses
  (classified by `transferSize`: a full page runs 150–850 KB, a
  headers-only revalidation about 300 bytes — a different thing from the
  literal scenario's zero-byte, zero-request repaint above; `transferSize`
  is what tells the two apart). That session is a revisit generator, not
  reading — nobody re-reads the ten pages they just read, backwards,
  immediately — and twelve of the seventeen `304`s are exactly that second
  pass: all twelve backward-pass requests, every one of them a genuine
  revisit of a slot the reader had left and come back to.
  **The other five are not reader traffic at all.** The forward pass alone
  produced 22 full `200`s and 5 of its own `304`s — self-inflicted, not
  genuine. The prefetch window warms a neighbouring slot into a throwaway
  `Image()` (the same default builder `decode.ts` documents,
  `() => new Image()`, garbage once its `decode()` promise settles) and
  requests it before the reader arrives; when the reader's own swap reaches
  that slot moments later, the decode gate builds a *second* `Image()` for
  the same URL and requests it again. The browser answers the second one
  with a `304` because the first put the `ETag` in its cache — but neither
  request would exist without this task's own prefetch running, so against
  a no-prefetch baseline the validator saved nothing on those five: it
  neutralised traffic the reader introduced, not traffic a reader brought to
  it. **The honest split is twelve genuine reader revisits and five
  self-inflicted duplicates**, out of one scripted session — one browser,
  one corpus, one machine, no repetition — against which O1 had already been
  deferred twice, most recently on 177 requests from a real recorded client
  session (Panels). The close named its own reopening bar as "a real session
  log … over genuine reading, the same standard Panels' own measurement was
  held to" and then closed on a synthetic script that met neither half of
  that standard — which is the finding this review turned up.
  **The instrumentation the count rests on has not itself been checked,
  beyond one indirect spot-check.** All seventeen were classified by
  `transferSize` alone. The one comparison against the wire that exists
  anywhere in this work was run as an isolated repeat — a different request,
  in a different run, from the 39-request session above — where the
  CDP-level network log confirmed the literal wire text `304 Not Modified`
  for a response Resource Timing's own `responseStatus` field called `200`
  (which normalises a validated hit to the status the *page* effectively
  got, not the status the *wire* carried, and is why `responseStatus` was
  abandoned as a classifier here — a genuinely useful trap to have on
  record). Sixteen of the seventeen — everything but that one indirect
  check — rest on `transferSize` and nothing else.
  **Also verified directly, not assumed:** a `200` carries neither
  `cache-control` nor `last-modified` — `fetch(url, {cache: 'no-store'})`
  against a never-before-requested page returned exactly `{connection,
  content-length, content-type, date, etag, vary}`. Both headers are still
  absent, unchanged by this task, so heuristic freshness is still zero and
  every reuse, genuine or self-inflicted, still asks the network before
  anything is served — never silently from disk, but never for free either.
  **That is the part that keeps O1 open on its own terms, not only on
  sample size.** O1's rationale was that the design's "one fetch, one cache
  entry" argument for serving whole spreads assumes the client caches, and
  nothing yet helps it — a validator that only shrinks the response doesn't
  change that. Prefetch exists to hide a round trip before the reader
  notices it, and a round trip that still has to happen, just for fewer
  bytes, is a round trip the reader still waits on: on a slow connection
  this saves body bytes and no latency.
  **What would close it:** a real session log, from genuine reading rather
  than a script built to force revisits, showing conditional-request reuse
  net of prefetch-induced duplicates — the same real-recorded-session
  standard, not a synthetic one, that already answered the equivalent
  question for Panels at zero. `scripts/request-summary.sh` already reports
  the conditional-request count; what is missing is a log from an actual
  reading session to feed it. Landing `Cache-Control`/`Last-Modified` so a
  genuine repeat costs no round trip at all, rather than a cheaper one,
  would close the item outright, independent of any session log.
  **It closed on that second exit, and taking it made the session log
  unnecessary rather than merely optional.** The item's own argument for
  reopening was that a validator shrinks a response and does not remove the
  round trip; `PAGE_CACHE_CONTROL` — `private, max-age=86400`, on
  `get_page`, `get_thumb`, the OPDS stream and the OPDS cover — removes it.
  **That freshness lifetime was later withdrawn and the round trip is back**,
  as `private, no-cache`: a work id is the sidecar's filename stem and every
  sidecar in the corpus is `taiyaku.json`, so one URL addresses a different
  volume's page depending only on which sidecar `serve` was started with.
  Measured on two of them — the same URL, SHA-256s `ab5da993…` and
  `17655ca0…`, under ETags the server had already derived from different
  member digests. The reasoning recorded here was sound and rested on a
  premise that was not: that the URL names the bytes. See the constant's own
  doc in `routes.rs`.
  With freshness in hand there is no reuse left to count, so the question
  "how many revisits does real reading produce?" stops gating the item: the
  answer costs nothing either way.
  **Measured from the server's side of the wire, which is the half every
  earlier attempt at this was missing.** Ten slots read forward in the
  browser reader against `corpus/gotoubun-v01`, all three passes driven
  through the real reader rather than by script: the pass that populates
  the cache, then the same ten slots read again after a full reload. The
  second read touched 14 distinct pages once prefetch is counted and sent
  **zero page requests to the server** — not zero bytes, zero requests,
  confirmed against `--log-requests`'s own JSONL rather than against a
  browser-side classifier. The only traffic in that window was ten progress
  writes, two work-list reads, and a `304` apiece for the sidecar and the
  notes. Before this change every one of those 14 would have been a
  conditional request the reader waited on.
  **`Last-Modified` was named in this item and is deliberately not shipped.**
  It would need the member file's mtime, which needs a `stat`, on the async
  worker, on every request — putting disk I/O back on the one path
  `locate_member` is written to keep clear of so a `304` can be answered
  before any work happens. It would also be the weaker of the two validators
  on material addressed by content: re-downloading a byte-identical archive
  moves the mtime and would throw away a cache entry that is still correct.
  The `ETag` answers revalidation and `Cache-Control` answers freshness;
  there is no third question left for a date.
  **`immutable` is not shipped either, and the reason is the URL rather than
  the bytes.** A page's bytes are fixed by `(member sha256, offset)` — that is
  what `page_etag` is derived from — but the URL names a work, an edition and
  an index, none of which move when an archive is replaced. `immutable` would
  tell a client never to revalidate for the entry's whole lifetime, which is
  O11's failure mode imported into the browser. The one-day bound is chosen
  from two ends rather than measured: long enough that a work reopened the
  next morning costs no round trip, short enough that a replaced archive
  corrects itself with nobody clearing anything.
  **One defect was introduced closing this and caught by driving it, not by
  the tests written for it.** `not_modified` is shared by four handlers, and
  the first version of the change put the page constant inside it — so
  `get_sidecar` and `get_notes`, which share it, began telling clients they
  could reuse a *sidecar* for a day. In the browser both documents stopped
  reaching the server at all, which is a lost correction waiting to happen:
  `PATCH /alignments` rewrites the sidecar, and a reader who made a
  correction and reloaded would have been served their own stale copy. The
  test written alongside the change did not catch it, because it asserted on
  the `200` path and the leak was on the `304` — and a client stores what a
  `304` tells it (RFC 9111 §4.3.4), so the two are worth the same. `freshness`
  is now a parameter every caller must supply, and the test checks both
  statuses on both mutable routes; reintroducing the leak fails it by name.
  See `notes.md`, "O1 — closes here, and the fix broke the sidecar on the way".
- **O2 — Progress is written non-atomically. Closed.** `fs::write` truncated
  first, so a crash mid-write left a short file and the next read answered 500
  rather than "nothing recorded" — the worse of the two wrong answers, since it
  tells the reader the server is broken rather than that it has no position to
  restore. `store` now writes a temp file and renames, the pattern the Stump
  resolution cache (`taiyaku.stump.json`) already used beside the same sidecar.
  The temp name is **unique per attempt** rather than fixed, which the cache's
  is not: progress is the one file here a person writes from two devices at
  once, and two writers sharing one temp name can interleave into publishing
  the empty file the change exists to prevent. Same reasoning and same shape as
  `taiyaku_read::atomic::write`, which is where `cover.rs` gets it since its own
  copy went; the two are not shared yet. A crash cannot be
  staged in a test, so the test pins the mechanism instead — the destination is
  replaced by a rename, never opened for truncation, made observable by taking
  write permission off the existing file, and skipping with a printed line
  where that precondition does not hold (root, or a filesystem ignoring the
  mode bits) rather than reporting a green that measured nothing.
- **O3 — `PUT /progress` does not check the slot is in range. Closed.** It
  validated the edition, so a position could name a real edition and a page
  past its end. It now range-checks too, and **in slots rather than pages**:
  a position is a slot — that is why `progress.rs` records one — and an edition
  that joins spreads has more slots than pages, so checking against
  `total_pages()` would have refused the last slot of every such edition, One
  Piece v092 being spreads throughout. A test holds that distinction: 4 pages
  with page 0 a spread is 5 slots, and slot 4 must be accepted. The refusal is
  404 with the same wording shape as the edition check above it, naming the
  edition and how many slots it has.
- **O4 — Three error paths were named here; all three are now covered.**
  `PageSource` narrowed to one member during M3, so the edition
  walk that used to live in `LocalLibrary` now lives in `ServeState`. An
  unknown edition is tested there, both directly and end to end over HTTP (a
  404 for an edition a work does not have). An out-of-range page index is
  tested at the `ServeState` level and dispatched through the router on both
  the `/api/…` page route and the OPDS stream route. **Two sidecars claiming
  one work id is now tested too** — the code refused it from M2 and nothing
  exercised it, so it was correct by inspection only. The test asserts the
  message names *both* paths, since the point of refusing is that the person
  running the server cannot otherwise tell which two files collided; a second
  test loads two differently-named sidecars side by side, so the refusal
  cannot be spelled "refuse everything".
- **O5 — A non-numeric page index answered axum's default 400. Closed.** It was
  the only message this server sent that this server had not written:
  `Invalid URL: Cannot parse value at index 2 with value `abc` to a `u32``,
  which names a Rust type and a position in a path tuple, neither of which
  means anything to a client. Both routes addressed by page index — the
  `/api/…` one and the OPDS stream — now take the segment as a `String` and
  run it through one shared `page_index`, which answers ``\`abc\` is not a page
  number; pages of `raw` are numbered from 0``.
  **The status stays 400 rather than becoming 404**, deliberately: `pages/4`
  names a page an edition does not have and `pages/abc` does not name a page at
  all, and a client is entitled to tell those apart. What they now share is
  that both come from here.
  Shared rather than written twice, and pinned from both ends — Panels reaches
  pages through the OPDS route, not the `/api/…` one, so fixing only one would
  have fixed the one nothing uses. The refusal is carried by a named
  `NotAPageIndex` rather than a bare `Response`, the pattern `NoSuchWork`
  already established in the same file for the same reason: a `Response` is 128
  bytes and `Result<u32, Response>` trips `clippy::result_large_err`.

**M3 found six more, the same way** — O6 to O11, its own review, not blocking.
Three are closed. The four that remained each wanted a decision before they
wanted code, which is why they were never simply "next", and all four decisions
have now been taken by measuring rather than by arguing. **O6, O9 and O11 each
sit on a measurement and name what would reopen them** — the pattern O1
established — without any of the three closing; the artefacts were already on
disk, so nothing new had to be run. See `notes.md`, "The only concurrent
requests Panels ever issued were covers". **O8 went the other way**: the same
discipline applied to it produced a number that argued for shipping, so it is
closed. See `notes.md`, "O8, measured three ways: the third one is why it was
worth fixing".

- **O6 — A cover is generated from page 0, which on an art-book edition is
  a 430 MB decode. Open, and now bounded at both ends.** The corpus's Color
  Walk material is TIFF at that size, so the first request for such a cover is
  slow and nothing warns before or during it. The decode limit is raised to
  3 GB, mirroring `taiyaku-cli/src/fingerprint.rs`, so the request succeeds —
  but succeeding is not the same as being fast.
  **The ordinary case is now measured rather than assumed**, from the one cold
  cover grid a real client has ever produced: 95.9, 109.0 and 221.4 ms, for 54
  to 60 KB of JPEG apiece. The art-book case is about twenty times that — four
  Color Walk pages fingerprint in 8.6 s — paid once per edition and then cached
  forever.
  **What keeps it hypothetical is upstream of this crate.** A cover is
  generated for an edition named in a sidecar, and art books cannot be aligned
  at all: four pages, four pairings, every margin at or below zero and two of
  them negative. So the material that would make this decode expensive is
  material nothing should be writing a sidecar for. That is reachability, not
  impossibility — `align --write` will pair an art book against a volume and
  refuse nothing — so the premise stays one bad invocation away rather than
  excluded.
  **The same measurement settles the half of O6 that was a guess.**
  `MAX_CONCURRENT_GENERATIONS = 2` was chosen against the assumption that "a
  cold OPDS grid view issues one cover request per edition at once". It does —
  three at once, `in_flight` reaching 3, on the only cold grid ever logged. The
  guess was right and is now a number.
  **What would reopen it:** art-book linking shipping, which is the "Later"
  item that would give an art book a legitimate reason to be in a sidecar; or
  any edition whose first page is slow enough to be felt on a grid, where the
  figure to beat is 221 ms.
- **O7 — The request log accumulated in memory as well as on disk, unbounded.
  Closed.** Every entry logged to disk was also kept in a `Vec` behind a mutex
  for the life of the process, and nothing in production ever read it back:
  `Log::entries()` was called only from tests, while
  `scripts/request-summary.sh` — the actual consumer — reads the JSONL on disk.
  So the growth was real and bought nothing, on a log that exists precisely to
  be left running through a reading session. The `Vec` and `entries()` are
  gone, and `Log::new` no longer takes an `Option<PathBuf>`: a log with nowhere
  to write would now record nothing anywhere, and `serve` already expresses
  "not logging" as no `Log` at all.
  The four tests that read `entries()` read the file instead, **converted
  first and run green before the in-memory copy was removed**, so the disk
  artefact was shown to carry every fact the `Vec` did rather than assumed to.
  That also closes a weakness the old suite had and named in a doc comment:
  every other test used `Log::new(None)`, so deleting the whole file-writing
  branch would have left the suite green on the strength of one test. Every
  test here now goes through the file a person actually measures from.
- **O8 — Stump resolution happened at startup, for every member of every work,
  before the listener bound. Closed.** A summary line printed once a work
  *finished* resolving, so a work with many unresolved members was silent for
  its whole duration and a slow resolution was indistinguishable from a hung
  one. `resolve_work` now reports each member as it asks about it — its path,
  its position and how many there are. A member answered from the cache reports
  nothing, so a warm start is exactly as quiet as before: a line per member on
  every start would be noise that trains a reader to ignore it, and a test pins
  both halves.
  **The blocking itself is now gone too, and it was measured before it was
  fixed** — this item had been deferred twice on "a corpus small enough that
  nobody has felt the wait", which turned out to be right about two cases out
  of three:

  | Start | Before | After |
  | ----- | -----: | ----: |
  | Warm cache, Stump reachable | 0.10 s | 0.10 s |
  | Cold cache, Stump reachable | 3.15 s | 0.10 s |
  | Cold cache, **Stump unreachable** | **30.09 s** | **0.10 s** |

  The first two rows are why deferring was defensible: a warm start made no
  queries at all, and one nine-member work resolves in about 350 ms a member.
  **The third row is the one that changed the case.** A downed instance cost a
  full 30 s POST timeout before the listener existed — on *every* start, not
  once — to reach pages that were sitting on local disk the whole time. The
  breaker bounded it to one timeout for the whole library rather than one per
  work, so it was 30 s and not ten minutes; it was still 30 s of a server that
  could have answered everything and refused to start.
  **The fix is that resolution now runs behind the bound listener**, on a
  blocking task, with `StumpSide` holding its resolutions behind an `RwLock`
  instead of owning a finished `HashMap`. The map starts empty and fills as
  each work finishes. A member not yet resolved is indistinguishable from one
  Stump does not have — both say "read it off disk" — so the window is safe by
  construction rather than merely short.
  **Reporting is per work, not one aggregate at the end**, which is the half
  that would have been easy to miss: handing back every work's answers together
  would have moved the wait behind the listener rather than removing it, and a
  twenty-work library would still have served all twenty from disk until the
  twentieth was asked about.
  Measured after: against a deliberately unroutable address, the listener binds
  in 0.10 s and three real pages — 1.18 MB, 1.34 MB and 339 KB — are served in
  about 4 ms each **while the resolution is still hanging on its 30 s timeout**.
  Two tests, each checked by sabotage rather than by having once failed to
  compile: neutering `learn` makes the first serve disk bytes where Stump bytes
  were required, and aggregating instead of reporting per work collapses the
  second's two batches into one. What is **not** claimed is the ordering inside
  `serve` itself — that binds a socket, which no test here drives.
- **O9 — There is no single-flight guard on the request path. Open, and
  measured against.** Several requests arriving during one Stump timeout all
  wait it out independently rather than sharing the one outcome.
  **The concurrency exists; it is the wrong kind.** `notes.md` records
  "concurrent page requests: none observed, in any session", and the qualifier
  turns out to be doing real work — that claim was read here as "Panels issues
  no concurrent requests" while this item was being scoped, and it does not say
  that. Across 242 page requests in two sessions no page request was ever in
  flight beside another, but `in_flight` reaches 3, and every overlapping
  request is a **cover**. Panels fetches a work's covers together, so a
  three-edition work produces a three-deep burst on a cold cache — down the
  same `ServeState::page` → `CompositeSource::page` path pages take, and so
  squarely inside what O9 describes.
  **It is still not a burst single-flight can merge.** A guard deduplicates
  concurrent requests *for the same key*; this burst is one request per
  edition, three different members resolving to three different Stump books.
  There is nothing to coalesce, so the guard would add a map, a lock and a wait
  and merge none of the only requests that have ever overlapped here. The
  repeat case is already covered by the cooldown latch, which sends every
  request after the first failure straight to disk; what stays unguarded is
  requests already in flight when that first failure returns, and this log
  bounds that at three, all distinct.
  **What would reopen it:** a client issuing concurrent requests for one key.
  The browser reader is the candidate — prefetch colliding with a reader
  jumping onto the page being prefetched, say. Not the filmstrip: this
  paragraph used to name it as the trigger for O14 too, and both guesses were
  wrong the same way. The filmstrip resolves nothing (`nearestPaired` was
  O14's real trigger, not the filmstrip — see its own entry) and its tile
  requests were checked directly once it shipped and do not reopen this
  either — see "The filmstrip's tiles do not reopen O9", below.
- **O10 — `MAX_DECODE_ALLOC` was duplicated. Closed.** Between
  `taiyaku-cli/src/fingerprint.rs` and `taiyaku-serve/src/cover.rs`, across a
  crate boundary with no dependency edge between them. Each copy said so in a
  comment, and a comment asking whoever changes one to keep the other in step
  is the arrangement that drifts. It now lives in `taiyaku-read`, which both
  already depend on — it is the crate they share — and whose pages are the
  bytes both of them decode, so the ceiling on that decode sits alongside them.
  Not `taiyaku-core`, which compiles to WebAssembly and describes the format; a
  native decoder's memory ceiling is neither. No new test: the duplication is
  removed rather than checked, so the compiler holds what a test would have
  had to assert, and the existing decode tests on both sides cover the move.
- **O11 — Two on-disk caches never notice a change on the far side, and this
  was documented nowhere until now.** `taiyaku.stump.json`, written beside each
  work's sidecar (same directory, same stem), records the Stump book each
  member resolved to — and, deliberately, records the members Stump does not
  have too, so a miss costs one query once rather than one per page turn. That
  is also its weakness, named as Risk 5 in the design and left unaddressed:
  nothing watches for the library changing on Stump's side, so a book added to
  Stump after a work's first resolution stays invisible to taiyaku until
  someone knows to delete `taiyaku.stump.json`, which forces every member of
  that work to be re-resolved on the next start. **That last sentence is now a
  flag rather than a piece of folklore:** `taiyaku serve --refresh-stump`
  forgets every handed work's resolutions before resolving them again, which is
  precisely what deleting the file by hand did, and it says so rather than
  doing nothing when no `--stump-url` was given. The tradeoff itself stands —
  a remembered miss still costs one query once instead of one per page turn,
  and nothing watches Stump for changes — but acting on it no longer requires
  having read this paragraph. The generated-cover cache has the same
  shape without the excuse of a design tradeoff behind it: covers are
  unbounded and never pruned — an archive that is replaced gets a new
  cover under its new hash, but the old file is never reclaimed.
  **That second half now has a size, and it is 165 KB.** Three files, one per
  edition of the one work ever served, at 54 to 60 KB each. Growth needs an
  archive to be *replaced*, not a page to be read — the key is the member's
  `sha256`, so reading a work forever adds no files at all — which puts a
  hundred works at three editions apiece, every one replaced once, at about
  33 MB. Unbounded in form, negligible in size, and stated here so that the
  next person to read "never pruned" knows what it has actually cost.
  **The two caches no longer share a tree, and that is new.** They did:
  `.taiyaku-cache/covers/` was the server's, and the 55 `<sha256>.json` files
  beside it — about 1.2 MB — are the CLI's fingerprint cache, the one
  `align --cache` names and the one measured at 3.4 s → 189 ms. The command
  has to be said with the flag now: `serve` took a `--cache` of its own on
  2026-08-11 and it names the server's root, so a bare `--cache` no longer
  picks out one of the two caches this sentence exists to tell apart.
  Deleting the parent threw away both, and only one is cheap to rebuild.
  Since 2026-08-11 the server's cache lives under `$XDG_CACHE_HOME/taiyaku`
  (or `$HOME/.cache/taiyaku`), so wherever either variable is set — every
  ordinary environment — `.taiyaku-cache/` holds the fingerprint cache alone
  and clearing it can no longer discard the expensive half by accident. The
  hazard is recorded rather than deleted because a warning that vanishes
  without explanation reads as an oversight — and it has not vanished
  entirely: where neither variable is set, as under a `systemd` unit without
  `Environment=HOME` or a bare `env -i`, the server falls back to the old
  relative path and the two share a tree again. That branch is the one that
  prints, naming the directory it settled on and the `--cache` flag that pins
  it, so the exception announces itself rather than being found later as a
  pile of regenerated pages.
  **What would reopen it:** archives replaced routinely enough for 55 KB
  apiece to accumulate, or a second consumer of the server's cache directory —
  the filmstrip's thumbnail-per-page route, deferred to M5 in `cover.rs`.
  **That second consumer now exists (M5a Task 2) and has been measured, in a
  `thumbs` directory that is a sibling of `covers` rather than the same
  directory** — both under `.taiyaku-cache` when the measurement was taken,
  and both under whichever root resolves since 2026-08-11; the sibling
  relationship is what the figures below depend on, and it holds under every
  one of the four rules.
  Fetching every tile of one 195-page edition (`corpus/gotoubun-v01`'s
  `raw-mono`) produced 195 files totalling 1,177,982 bytes — about 1.12 MiB —
  individual tiles from 1,117 to 8,808 bytes. Roughly seven times the covers'
  bytes for sixty-five times the file count, which is proportionate to 195
  tiles against one cover rather than the kind of blowup "changes the
  arithmetic entirely" was written to warn about. See `notes.md`, "M5a Task 2:
  the filmstrip's tile cache, measured against the real corpus".
  **Still not closed:** the reopening condition stands, now at the tile
  cache's larger constant factor, and a library with many editions' filmstrips
  warmed at once is a case this one-edition run does not speak to.
  **A third consumer looked likely and turned out not to be one.** `taiyaku
  regions` decodes every page of an edition, which is exactly the shape of work
  the fingerprint cache exists for, and the L2a design said it would cache under
  `.taiyaku-cache/` keyed on the member's `sha256`. It does not, and takes no
  `--cache` at all: such a cache would hold the same boxes the regions document
  already holds, under the same key, in a second place that can disagree with
  the first — which is the duplication O10 was closed to remove — and `--write`
  is what makes a second run cheap. So `regions` adds no cache of its own, the
  tree it would have shared ordinarily holds the fingerprint cache alone now,
  and the flag is refused rather than accepted and ignored.

**One deferral was carried rather than found** — O12, named while planning M1
and M2, and now closed:

- **O12 — `spawn_blocking` covered three of the five handlers that do blocking
  I/O. Closed.** `get_page`, and the OPDS `stream_page` and `cover`, handed
  their work to a blocking pool; `get_progress` and `put_progress` read and
  wrote files directly on the async worker. All five do now, so the only
  handler left on the worker is `get_sidecar`, which serves the parsed
  document from memory and touches no disk at all.
  `put_progress`'s validation moved inside the boundary with its write rather
  than staying outside it: it blocks on nothing, but splitting the handler
  across the boundary would mean looking the work up twice, since a borrow
  cannot cross into the closure. A small type, `NoProgress`, carries the
  distinction `spawn_blocking` would otherwise flatten — a work, edition or
  slot that does not exist is a 404 and the client's mistake; a position file
  that will not read or write is a 500 and this server's. `get_page` needs no
  such type because both of its failures are 404s.
  **Measured, not asserted.** A FIFO at the progress path blocks a real read on
  the real code path, and on a deliberately `current_thread` runtime a read
  that runs on the worker stops the runtime dead — so a canary task never gets
  scheduled and a watchdog thread has to rescue the request, which it records.
  Before the change the test fails in 0.56 s; after it, it passes in about
  60 ms. The write path gets the same treatment and is **not** measured that
  way: `store` writes through a temp file whose name is unique per attempt, so
  there is no path a test can pre-create as a FIFO.
  The other deferral recorded alongside it was already closed: the index →
  (member, offset) walk now lives in `taiyaku_core::Edition` rather than in
  `LocalLibrary`.

**One came from reading a design against the code** — O13, turned up on
2026-08-01 while planning M4, and not blocking because nothing writes a
hand-corrected run until the correction UI exists:

- **O13 — `sidecar.md` promises a hand correction survives a re-solve, and
  `merge_spoke` did not keep one. Closed.** The format doc is explicit: `by` is
  `"solver"` or `"hand"`, and "a hand-corrected run must survive a re-solve;
  this is the field that makes the correction UI more than a viewer." What
  `merge_spoke` did was drop the stored edge in both directions and push the
  newly solved alignment in its place, so every run on that edge was replaced
  wholesale — a hand correction included. Nothing had noticed because
  **`Source::Hand` was constructed nowhere in the workspace**: the variant was
  defined, serde round-tripped it, and no code path wrote one. The promise had
  never been exercised, which is how a documented guarantee rots without any
  test going red.
  **Half of it was guarded, and the half that was not decided the shape of the
  fix.** `merge_spoke` refused when a stored member's `sha256` differed from
  the one just aligned — "re-solve the work rather than merging into it" — but
  that comparison was against the *`from`* side, the hub the alignment was
  solved against. The spoke was inserted wholesale, and
  `re_aligning_a_spoke_with_no_hand_runs_replaces_it_rather_than_duplicating` pinned that as
  deliberate: re-solving a spoke after fixing its archive is ordinary, so the
  test swaps the spoke's `sha256` and expects the merge to succeed. A merge
  could therefore legitimately replace the very edition a hand correction
  addressed, and carrying hand runs across *that* would resurrect runs
  pointing into slots of an edition that no longer exists — the failure the
  from-side guard exists to prevent, let back in through the other door.
  **The fix divides exactly that way.** Where the spoke's members are
  unchanged, hand runs now carry across and the merge reports how many, as
  `Merged::CarriedHandRuns(n)`. Where the merge would replace a spoke that
  holds hand runs, it is refused instead, naming the spoke and how many
  corrections are at risk. Where the spoke is replaced and holds none, the
  merge proceeds exactly as before.
  **Three things turned up while building it, each worth a sentence.** The
  first `carry_hand_runs` dropped a solved run that merely *overlapped* a hand
  run rather than splitting around it. On a real test it discarded 28 of a
  30-slot run's slots — every slot but the two that were corrected — leaving
  `resolve` to answer `None` for pages no correction had touched. It now
  splits the solved run around each hand run's span instead. The spoke-changed
  test compared whole `Member` structs rather than hashes, so a
  renamed-but-identical archive read as "replaced"; it now compares the
  ordered `sha256` sequence, the idiom the from-side guard's own comment
  already states: "Content, not paths… only the hashes can be compared." And
  the orientation assumption was reachable and silent — `--direct` plus
  `Sidecar::correct`'s own direct-edge branch let a pair be re-merged with the
  names swapped relative to the stored edge, and without a guard the merge
  *succeeded*, reporting `CarriedHandRuns(2)` while misfiling the correction
  against the wrong edition's slot table. Now refused by name.
  **What remains open, deliberately:** what should become of a correction
  addressing bytes that no longer exist. Refusing the merge is what stops that
  being answered silently, not an answer to it. See `notes.md`, "O13 — closes
  here".
- **O14 — `taiyaku-wasm` rebuilt the path between two editions on every
  resolve. Closed.** `Binding::resolve` called `route` per invocation, which
  rebuilt both slot tables and, where the answer composed through the hub,
  recomposed the run lists. `taiyaku_core::Route` exists precisely so a caller
  can pay that once — `route.rs` says so, and puts a number on it: a reader
  paging a 195-page volume would otherwise pay it 195 times. One swap per
  keypress did not notice.
  **The trigger named here was wrong, and M5a found the real one before this
  branch fixed anything.** This item named "a filmstrip resolving per tile" as
  what would notice — but the filmstrip resolves nothing; it consumes a slot
  table built once. What actually rebuilt the route repeatedly is
  `web/src/nearest.ts`'s `nearestPaired`, called while showing the gap state:
  it calls `display` once per candidate slot on either side of a gap, and
  each cross-edition `display` is one `Binding::resolve`. That was up to
  `slotCount` route rebuilds, and because nothing in `web/src` memoised it,
  the same rebuilds repeated on every render of the gap state rather than
  once per swap.
  **`Binding` now memoises the last `(from, to)` pair's `Route`.** A
  `RefCell<Option<CachedRoute>>` on `Binding` — interior mutability because
  `resolve` takes `&self`, safe because a `Binding` is single-threaded by
  construction on the JS side, both stated in `binding.rs`'s own doc comment
  rather than left implied. A different pair replaces it outright rather than
  growing into a map, because the caller this was measured against holds
  `from`/`to` fixed for a whole scan and varies only the slot.
  **`App.tsx` also stopped resolving each gap offer twice.** `nearestPaired`
  called `display` to find the nearest paired slot either side of a gap, and
  `App.tsx` then called `resolveOffered` on that same slot a second time to
  get the page to show — a second `Binding::resolve` for an answer the scan
  already had. `nearestPaired` now returns the resolved landing alongside the
  offered slot (`nearest.ts`'s `NearestOffer`), so displaying an offer costs
  no resolve at all. `resolveOffered` still runs, once, when a reader actually
  takes an offer — `takeOffered` calls it to find where to commit authority —
  which is a different call, made once per click rather than once per render.
  **Measured against the real corpus, not asserted.** Go-Toubun v01's
  `raw-mono` is 195 slots; resolving every one against `en-mono` built the
  route 195 times before the memo and once after — the exact figure
  `route.rs` predicted, reproduced rather than repeated. `raw-mono` slot 1 has
  no counterpart in `en-mono` at all — a genuine gap, not a synthetic one:
  the corpus's own stored run is `"a": "1"` with no `b` side. A scan finding
  its paired neighbours (slots 0 and 2) plus the now-removed second pass
  resolving them again cost four route rebuilds before this fix and one
  after:

  | Case                                                          | Before | After |
  | -------------------------------------------------------------- | -----: | ----: |
  | 195 resolves of one pair (paging all of `raw-mono`)             |    195 |     1 |
  | The real gap at `raw-mono` slot 1: scan + former second pass    |      4 |     1 |

  Two tests in `binding.rs` pin these against a `#[cfg(test)]` counter kept
  only to make a cache hit observable from outside without timing the call —
  `resolving_every_slot_of_one_pair_builds_the_route_once` and
  `a_gap_scan_and_its_former_second_pass_build_the_route_once`. Both failed
  with the pre-memo counts above before `resolve` was changed to route
  through the cache, and pass now.
  **What the memo does not cover: a caller alternating between two pairs.**
  It holds exactly one — `a→b`, `b→a`, `a→b`, ... on successive calls rebuilds
  every time, the same as no cache at all. Nothing in this reader does that
  today: a swap changes what is *shown*, not what `resolve` is asked, and
  `nearestPaired`'s own scan holds `from`/`to` fixed throughout — but a
  caller that interleaved pairs (two gap scans on different edition pairs
  racing, say) would thrash it exactly that way, which is what would reopen
  this.

**The `duplicate-resolution` whole-branch review found four more** — O15 to
O18 — and unlike every group above it, none of them outlived the branch that
found them by long: three were closed on it, and the fourth, whose fix was
larger than the branch, on the commit it asked for:

- **O15 — A flag typed with no usable value was taken as absent, in silence.
  Closed.** `option_value` found the flag and returned whatever argument
  happened to follow it, without asking whether anything did. Both ways that
  can go wrong were quiet. A flag last on the line has nothing after it, so
  `taiyaku inventory releases --min-pages` read as no flag at all and walked
  with the default threshold of 60. A flag followed by another flag took that
  flag as its value, so `taiyaku align a -- b --write --ltr` wrote the sidecar
  to a file named `--ltr` — the direction still applied, since `--ltr` is
  matched separately, so nothing anywhere reported a problem.
  The `duplicate-resolution` branch fixed the instance and left the class:
  `against_members` was changed during the review to *refuse* a valueless
  `--against` rather than fall through to no reference at all, on the argument
  that a flag typed and then ignored is worse than one refused — dropping the
  reference edition changes `decided=` and the rank order, so the silence cost
  a wrong answer rather than a default. None of that argument was about
  `--against`. Thirteen other valued flags, across nineteen call sites in
  `align`, `verify`, `serve`, `inventory` and `duplicates`, still did the
  ignoring, and the pass over all of them is this commit.
  **The two modes did not split evenly, which the deferral had not noticed.**
  Mode A — a flag last on the line — was silent for all thirteen, measured on
  `inventory`: `--min-pages` alone walked with the default and reported two
  candidates as zero, exit 0. Mode B — a flag eating the next flag — was
  already caught by accident wherever the value was parsed for its own sake,
  which is only the two `inventory` flags: `--min-pages --pattern` answered
  "`--pattern` is not a number of pages" and `--pattern --min-pages` answered
  "needs one `{}` where the volume number goes". Everywhere else it either
  surfaced late and elsewhere as a failed read, bind or request, or did not
  surface at all — `--out` would have written the contact sheet to a file named
  after the next flag exactly as `--write` did. So a fix that only validated
  values would have left mode A untouched everywhere and mode B untouched on
  every flag whose value nothing inspects.
  **`--name-a` is the case that decided the rule.** `validate_key` looks like
  it should catch mode B and cannot: `-` is in its safe alphabet, so `--name-b`
  is a well-formed edition key. Measured before the fix — `align … --name-a
  --name-b` wrote a sidecar whose editions were `["--name-b", "ed-b"]`, a key
  that then appears as a path segment in two URLs. So the refusal is **a value
  beginning with `--` is a flag**, applied in `option_value` without asking
  which flags exist, rather than per flag. That is the rule `split_editions`,
  `serve`'s path list and `against_members` already apply to positional
  arguments, so it is this command's existing convention rather than a new one.
  The cost is that a file genuinely named `--something` cannot be named on the
  command line.
  Each flag names what it wanted, the shape `--against` set: `--write needs a
  path, and `--ltr` is another flag`; `--min-pages needs a number of pages, and
  nothing followed it`. **`align` and `verify` now resolve every valued flag
  before either edition is read**, which the key checks already did on the
  argument that a typo should not cost the seconds it takes to fingerprint two
  editions — a refusal arriving after that is most of the cost of the mistake
  anyway. That also removed the second lookup four flags each used to do —
  `--name-a`, `--name-b` and `--title` in `align`, `--sidecar` in `verify`.
  Six tests: four on `option_value` — including the two paths a refusal must
  not break, an absent flag and a present value — and two end to end through
  `align` with member paths that do not exist, so a version that resolved
  options late would fail on a file-not-found instead of ever refusing.
- **O16 — A symlink cycle truncates the inventory walk, and this item was
  opened under the wrong framing. Closed.** It was opened as "a symlink cycle is
  a stack overflow", which is worth saying here rather than correcting quietly,
  because the wrong framing would have aimed the fix at the wrong failure.
  `Path::is_dir()` follows symlinks, so a link back to an ancestor does recurse
  — but Linux caps symlink resolution at 40 hops and returns `ELOOP`, and
  `is_dir()` swallows that into `false`. Measured on this platform rather than
  assumed: past roughly forty levels an unguarded walk does not crash, it
  returns `Ok` with the cyclic subtree reported as childless. That is **silent
  truncation**, the worse of the two failures and the one this project's whole
  idea of a gate that says what it measured exists against. Closed with a depth
  ceiling of 32, deliberately **below** the kernel's 40 so the named error fires
  before the OS masks the condition — a ceiling above it would never have been
  reached. A limit rather than a real cycle check because a cycle check needs a
  `canonicalize` per directory, a syscall apiece on a walk that is 579
  directories on the One Piece collection, to guard against something this rare.
  Two tests hold it: a genuine self-referential symlink under `#[cfg(unix)]`,
  and plain nesting past the ceiling on every platform.
- **O17 — The content-hash confirmation re-read a copy once per candidate it
  was compared against. Closed.** The closure `representatives` uses to confirm
  that two copies with matching signatures really do hold the same page bytes
  hashed both sides from scratch on every call, so a copy compared as the
  earlier half of several pairs paid for its own full read once per comparison:
  O(n²) full reads where O(n), one hash per copy, answers the same question.
  Harmless at the two or three copies a duplicate set usually holds, which is
  every set in this corpus; the bad shape is many copies whose page-size
  multisets collide while their bytes differ, so nothing ever confirms and every
  earlier copy is tried against every later one. Closed by memoising per copy,
  filled **lazily** rather than up front, so a copy whose signature never
  matches anything is still never hashed — the property
  `copies_with_different_signatures_are_never_hashed` already pinned, and the
  one a prefilled table would have broken while every existing assertion stayed
  green. `a_copy_is_hashed_once_however_many_candidates_compare_it` pins the
  other half: five colliding signatures, ten comparisons, one hash apiece.
- **O18 — The rule that a reference edition decides which copy is the impostor
  had no corpus witness, and the witness was already in the corpus. Closed.**
  `--against` moves the decision outside the set: a copy that pairs with the
  reference is this volume however the set treated it, which is the only thing
  that can tell two copies refusing each other apart. That rule was held by unit
  tests while these documents described it as though it had been measured — no
  set in the corpus had both a two-copy refusal and a reference edition to hand,
  so nothing had ever run it over real material. **The witness was there and had
  not been recognised as one.** `incoming/bocchi/` holds three directories named
  as volume 1 of *Bocchi the Rock*, and the third is the *Gaiden* spinoff, a
  different work by a different artist, filed under the main series' name — and
  the same directory holds `en-gaiden-v01.cbz`, the English edition of what that
  file actually is. Resolved twice over the same three copies: with no reference
  the *Gaiden* file is the copy flagged as not this volume, and `hamaji` ranks
  best at 125 slots; against `en-gaiden-v01.cbz` the flagging **inverts** —
  `hamaji` and `krc` pair nothing with the reference and are the flagged ones,
  and the *Gaiden* file is the only copy ranked, at 185 slots. The same three
  files and the opposite answer, decided entirely by what they were judged
  against. That is the argument for `--against` existing, and it is now a gated
  check rather than a claim. See `notes.md`, "What the six gated checks
  measured".

**Two came from running a command against its own usage text** — O19, turned up
on 2026-08-01 while scoping O15 and closed the same day once the question it was
blocked on turned out to be answerable by reading the format, and O20, which came
out of O19's own list of what it had left undone. Both were closed by refusing
rather than by wiring something up, and the pair is why that reads as a rule
here now: a flag that cannot change what a run does is refused, not ignored.

- **O19 — `align` without `--split-spreads` ignored the whole sidecar-writing
  half of its flags, silently. Closed.** `taiyaku align a -- b --write
  out.json` printed the summary, wrote no file, said nothing about not writing
  one, and exited 0. `--write` was read inside `align`'s split-spreads branch;
  the plain branch returns after `report_summary` and never reached it.
  `--title`, `--direct` and `--cache` are unreachable the same way, and
  `--name-a` and `--name-b` are worse than unreachable — they are validated
  *before* the branch and then discarded, so a run that could not have used a
  good key still refuses a bad one.
  **Both this document and the tool's own usage text said otherwise.** The
  synopsis above listed `[--write FILE]` and `[--split-spreads]` as independent
  options, and `--help` listed them on one flat line. Nothing anywhere stated
  the dependency; `corpus/README.md` only says to pass `--split-spreads`
  essentially always, which is why nobody had hit it.
  **This was not O15, and the distinction decided the fix.** In O15 a flag has
  no usable value; here the value is present and usable and the flag was
  ignored anyway, so no amount of validating values catches it.
  **It was opened as a decision — plain path grows a write path, or the
  combination becomes an error — and the format settled it rather than a
  preference.** The plain path has pages where a sidecar wants slots.
  `describe_edition` derives an edition's `spreads` list from its slot table, a
  page being a spread exactly when it appears there twice, and the plain path
  never builds one. So the only sidecar it could write is one claiming
  `"spreads": []`, having never looked — and that claim is not cosmetic:
  `sidecar.md` makes the slot sequence *follow from* `spreads`, so an edition
  that does join spreads would have every run after the first silently
  re-indexed. That is the shape of two failures already recorded here —
  `verify` reporting "192 of 187 pages paired, at 102.7%", and the −8 that was
  "entirely an artifact of joined spreads having no representation". Growing a
  write path would have written an unverified claim into the normative format,
  so the alternative was not cheaper, it was the only sound one.
  Closed by refusing the combination, before either edition is read, with a
  message naming the flag it needs and why: ``--write needs --split-spreads: a
  sidecar records which pages are joined spreads, and only that path detects
  them``. Two tests hold it from both sides — the refusal, and a run with both
  flags getting past argument handling entirely, so the rule cannot be spelled
  "refuse `--write`". `--help` and the synopsis above now state the dependency
  they used to deny.
- **O20 — Six more `align` flags were ignored where they could not take
  effect. Closed.** Opened out of O19's own "deliberately not done" paragraph,
  which said five flags were dead *on the plain path*. Both numbers were wrong,
  and checking rather than repeating them changed the fix. It is six, and four
  of them are dead on **both** paths: `--title`, `--name-a`, `--name-b` and
  `--direct` are read only inside the `--write` block, so `--split-spreads`
  alone skips them exactly as the plain path does. Their rule is therefore
  **needs `--write`**, not needs `--split-spreads` — a run with spreads split
  and no sidecar written ignored a `--title` just as silently.
  The remaining two divide the other way, which a measurement settled rather
  than a reading: `--cache` is **live** on the split path even with nothing
  written — the fingerprint cache directory is created and filled — and inert
  on the plain path, which calls `fingerprint_edition` directly and caches
  nothing. So `--cache` needs `--split-spreads`. `--ltr` is the same shape for
  a different reason: `solve` takes no direction and only slot expansion reads
  one, so on the plain path there is no direction to apply.
  **`--ltr` is the one case where the flag could not have changed the answer**
  rather than merely not having, since a page pairing is direction-free. It is
  refused anyway, because the defect O19 named is "typed and ignored", and a
  reader who passes `--ltr` and is told nothing has been told the tool agreed
  with them. No documented invocation breaks: `corpus/README.md`,
  `docs/sidecar.md` and `scripts/margin-survey.sh` all pass `--split-spreads`.
  One message covers the four sidecar flags, because the reason is identical
  and four wordings would imply four rules; the other two get their own, each
  saying what the plain path does instead, since "needs `--split-spreads`" on
  its own reads as arbitrary. `validate_key` still runs first, so a malformed
  `--name-a` reports its own shape rather than this one — the value is wrong
  whatever the run then does with it. Five tests, four refusals and one guard
  that a full write invocation gets past argument handling untouched, so the
  rule cannot be spelled "refuse these flags".
- **O21 — A misspelled flag was accepted in silence by every command. Closed.**
  O15, O19 and O20 each refuse a *known* flag used wrongly, and all three key
  off the flag's name, so none of them could see a name typed wrong. That left
  one door open, and it led back to the room O19 had just closed: `taiyaku
  align a -- b --writ out.json` reported a plausible alignment, wrote nothing,
  and exited 0 — the exact failure O19 fixed, reached by dropping a letter.
  Worse is the near-miss that still runs: `--split-spread` for
  `--split-spreads` does not fail, it returns a **different** answer, page-level
  where the caller asked for slot-level, and nothing marks it as the wrong one.
  **O15 is what made this checkable at all.** Once a flag's value can never
  itself begin with `--`, every `--`-prefixed token in an argv is unambiguously
  meant to be a flag, so an unrecognised one is a mistake rather than possibly
  somebody's filename. Before that rule the scan would have had to know which
  flags take values and step over them, which is most of an argument parser.
  Each of the twelve commands declares its own set rather than sharing a global
  one — `--pattern` is real for `inventory` and a typo for `align`, and one
  list would accept it in both. `library import` and `library export` declare
  separately for the same reason, being two commands that happen to share a
  first word: `--dir` is real for one and a typo for the other. The message names the whole alphabet rather
  than guessing a nearest match: a typo is usually one character off, but the
  case this exists to catch is a flag the caller believed in, and a single
  guess would be confidently wrong often enough to mislead. `topology` and
  `lookup` take no flags at all and say that instead of trailing off into an
  empty list.
  **One thing fixed alongside it, because it is the same scan:** a flag typed
  before the `--` separator lands in the a-side member list, and used to
  surface as ``--stats: No such file or directory`` — a message naming a file
  the caller never mentioned. It now says the flag is on the wrong side of the
  separator. That one was never silent, only misleading, which is why it was
  not worth an item of its own.
- **O22 — `verify` ignored `--cache` and `--ltr` with no `--sidecar` to apply
  them to. Closed.** O20's rule, in the command O20 did not reach. Both are
  read only from the `--sidecar` arm: without one, `verify` solves the pair
  directly, never calls `cached_edition`, and so consults neither the cache nor
  a reading direction. Found by measurement rather than reading — `verify a --
  b --cache DIR` did not create DIR. It is two flags rather than the one this
  was opened as, which is the second time in this group that checking a count
  rather than repeating it changed the fix.
  `--ltr` is doubly inert here: even *with* a sidecar it is the document's own
  `reading` order that indexes the runs, as the comment at that site already
  said; the flag reaches only `cached_edition`'s spread expansion.

**And then the thing four of them deferred was done: `--split-spreads` is
retired and splitting is the default.** O19 to O22 each ended by recording that
the flag was still opt-in rather than the default `corpus/README.md` describes,
and each declined it as a surface change rather than a defect fix. What changed
the case was those four items themselves. `corpus/README.md` had said to pass it
"essentially always" since before there was a second caller — unhandled spreads
make a correct alignment look broken, which is how several hours went once — and
O19 and O20 then made the default path the one that can do almost nothing: it
cannot write a sidecar, and it refuses six of `align`'s nine flags. **A default
nobody should choose is the same silence this group had been closing, one level
up.**

The flag is **retired rather than accepted as a no-op**, which is the one part
of this that was a choice. Keeping it would have broken the rule the four items
before it established — a flag that cannot change what a run does is refused,
not ignored — and letting `reject_unknown_flags` handle it would have called
yesterday's documented flag a misspelling. So it has its own message, naming
what replaced it. `--no-split-spreads` gives the page-level diagnostic, which
still takes `--stats` and still cannot write a sidecar, for O19's reason
unchanged. The three refusals O19 and O20 introduced now key off
`--no-split-spreads` rather than off the absence of `--split-spreads`; the
reasoning behind each is the same sentence it always was.
`corpus/README.md`, `docs/sidecar.md` and `scripts/margin-survey.sh` no longer
pass it, and the survey was re-run against the corpus to confirm it still
measures — 195 slots on Go-Toubun v01, the split-spread count.

**Everything above about O19 to O22 is left as it was written**, naming
`--split-spreads` in the present tense, because each describes what was true
when it was closed. Only the synopsis at the top of this document and the tool's
own usage text describe the command as it is now.

**One came from measuring what the correction UI produces** — O23, turned up
on 2026-08-04 while measuring M5d's correction UI against the corpus, and not
blocking because nothing yet corrects a work more than once in real reading:

- **O23 — Two hand corrections make a sound alignment report `fragmented`.
  Open.** Measured on the primary corpus volume, correcting `raw-mono`
  against `en-mono`: zero corrections leaves Go-Toubun v01 at 193 paired
  slots, 3 paired runs, average run 64.33, `sound`; one correction takes it to
  192 slots, 6 runs, average run 32.00, still `sound`; two corrections take it
  to 191 slots, 9 runs, average run 21.22, `fragmented`. Each correction adds
  three paired runs and strands one slot, because the 1:1 rule means the
  asserted slot takes its target from whoever held it, and that slot becomes
  a gap: the run it sat in becomes the part before it, the corrected slot
  alone, the part between it and the slot it displaced, and the part after —
  four paired runs where there was one, plus the gap.
  The threshold is 27.0, and it exists to separate needles (1.1–1.7 slots a
  run, measured across twenty-two of them) from confirmed-good pairings
  (36.0–200.0). A twice-corrected volume at 21.2 is neither, so the verdict
  misleads in a way the threshold was never calibrated for.
  **Nothing is changed here.** Altering the fragmentation policy on a guess is
  what this project does not do — the same reasoning that kept O1 open on
  evidence rather than shipping an `ETag` on plausibility.
  **What would decide it:** a work corrected enough times in real reading to
  say whether the verdict misleads a reader in practice, or a measurement
  showing that hand runs and solver runs should be counted differently. See
  `notes.md`, "O23 — opens here".
- **O24 — A solver-written direct edge to a third edition survives that
  edition being replaced. Open.** `merge_spoke` now refuses a merge that would
  replace a spoke holding *hand-corrected* runs on any edge — that is O13's
  fix, extended during this branch's whole-branch review when a direct edge to
  a third edition turned out to escape the original check. What it still does
  not cover is the same edge written by the solver rather than by hand:
  `align --direct` records one, nothing marks it `by: "hand"`, and replacing
  either endpoint leaves it addressing slots of an edition that is no longer
  there. `route` then prefers that stale edge to the hub and reports the
  answer as `Direct`, which is the strongest confidence this format has.
  **Not introduced by the correction UI, and not widened by it.** A direct
  edge has always required a deliberate `align --direct`; what M5d changed is
  that a *hand* one can now be made by clicking a filmstrip tile, and that
  case is the one now guarded. This item is the older half, found by looking
  at the new guard and asking what it does not cover.
  **What would close it:** deciding what a merge owes an edge it did not
  solve. Refusing is the conservative answer and matches O13's stance;
  dropping every direct edge that mentions the replaced edition is the other,
  and is defensible precisely because a solver edge carries no human decision
  to lose. Neither is obviously right, and no corpus work has yet had a
  `--direct` edge and a re-solve in the same document — which is also why this
  has never been felt.

**One came from driving a new surface against the corpus** — O25, turned up on
2026-08-05 during M5c's browser pass, and not blocking because the two panels
it hides are both reachable one keypress earlier:

- **O25 — The settings panel and the whole chrome are unreachable while
  showing a gap. Closed.** `App.tsx`'s `shown.kind === 'gap'` branch returned
  the `Gap` view *instead of* the reader's own tree, so everything mounted
  below it — `Chrome` (filmstrip and readouts), `Settings`, the mode toolbar,
  and until M5c the note panel — was simply not rendered. The keys fired and
  the state flipped; nothing appeared. Found by pressing `a` on a gap and
  getting silence.
  **The early return is gone rather than added to.** `Display` is narrowed once
  (`at`), five values gain a gap form, and the page area draws `Gap` in place of
  a page — so there is now exactly one place in the component a panel can be
  mounted, and a panel added later is reachable in a gap because there is
  nowhere else to put it. Adding two mounts to the gap branch would have closed
  the item as tabled and left the shape that let a third panel be forgotten.
  **The design question this waited on settled smaller than it looked.**
  `goToSlot` is `navigate(state, slot)` against the viewed edition, which is the
  commit `takeOffered` already makes for `Gap`'s offered moves, so a tile click
  in a gap needed no new semantics — only the marking and the scroll position
  were open. The strip shows the viewed edition with **nothing marked**
  (`currentIndex: -1`, correction mode's own convention) and centres on the
  nearest slot that resolves, which is the page `Gap`'s own button offers. The
  two neighbours are deliberately not given the dashed border: dashed means
  "where the alignment says this lands", and in a gap it lands nowhere.
  **Two things were already written and unreachable.** `healthLine`'s
  `no counterpart here` string had no caller that could produce it —
  `healthReadout` passes `hasCounterpart: true` unconditionally, and its own doc
  says why: `Chrome` never mounted in a gap. `gapHealth` reaches it now, and
  reports the authority↔viewed pair rather than hub↔viewed, so the row agrees
  with the verdict sentence `Gap` prints directly above it instead of falling
  silent under it. The readouts otherwise report from the authority — a real
  position in the work, which only the viewed edition has no page for — with the
  edition named, so the change of scale is never silent.
  **Measured in a real browser against the corpus**, on two gaps chosen out of
  the sidecar because the obvious one cannot fail: `raw-mono` slot 1 in
  `en-mono` is a *sound* pairing, so `sound pair · no counterpart here` on
  screen can only have come from `gapHealth`; and `raw-mono` slot 193 in
  `en-fan` anchors at `en-fan` slot 239 — its *last* slot of 240, drawn as the
  last of 235 tiles, since five joined spreads make the two counts differ —
  where `scrollLeft` read 24260 of 25824 rather than 0. See `notes.md`, "O25: the chrome in a gap,
  driven against the corpus", which records all ten checks — including the review
  list's "not reachable from here" marking, run under a backup of the corpus's
  JSON and restored to byte-identical afterwards.
  **The panel that waited on this is unblocked, not shipped.** The credits
  module (§4 below) is registered, its preferences load and save, and mounting
  it is now an ordinary addition to the one mount site rather than a question.

**One came from reviewing a branch that fixed the same hazard next door** —
O26, turned up on 2026-08-07 by the credit-pages whole-branch review, and not
blocking because it is older than that branch and nothing on the branch made
the exposure worse. It closed the same day, on a branch of its own — which is
the point of having deferred it rather than taking it as a drive-by:

- **O26 — `align --write` truncates the sidecar before writing it. Closed.**
  `fs::write` opens for truncation and then writes, so a crash or a full disk
  inside that window leaves a short file where a whole document was. The
  document is the worst one in this project to lose half of: it holds every
  edition of the work, every alignment between them, and every run a reader
  hand-corrected through the reading UI. This is the same hazard O2 closed for
  the reading position, and that `atomic.rs` now owns for every writer inside
  `taiyaku-serve`.
  **`credits --write` had the identical exposure and was fixed on the branch
  that opened this**, which is what made the gap visible. The fix was cheap
  because `taiyaku-cli` already depends on `taiyaku-serve`; the only barrier
  was visibility, `atomic` being a private module and `write` being
  `pub(crate)`. Both are public now and `credits::write_sidecar` goes through
  them, pinned by a test that watches the destination's inode change.
  **`align` now goes through the same call.** `main::align` builds a `Sidecar`
  — merged into whatever the file already held, that part having been fixed
  long ago — and ended on `fs::write(out, sidecar.to_json())`; it ends on
  `write_sidecar`, which is `taiyaku_serve::atomic::write`. **That
  `write_sidecar` is `credits`' own, moved up to the crate root and shared**,
  so `atomic.rs`'s caller tally stays at five: a sixth command-line writer, one
  call site for the two of them. The module doc records that, since a tally of
  *commands* would now be wrong. Both commands' tests still stand, over the two
  documents each puts at risk. The neighbouring
  `fs::write` in `verify`'s contact-sheet path was **not** in scope and is
  unchanged: an HTML report is regenerated from the archives on demand, so half
  of one costs a re-run rather than a document.
  **The estimate said "one line and a test" and was one line short**, in a way
  worth recording because it is a shape that recurs: the line to change sat in
  the middle of `align`, and a test can call a function but cannot reach a
  statement — `align` solves before it writes, so driving that write through
  the command needs real archives with real pages in them. The write moved into
  a named `write_sidecar` first, unchanged, and only then changed. The
  extraction is what made the failure observable.
  **Pinned by `main::tests::writing_an_alignment_replaces_the_sidecar_rather_than_truncating_it`**,
  the shape `credits::tests::writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it`
  established: inode identity, no temp file left beside the sidecar, and the
  document readable whole afterwards. It was watched failing against the
  `fs::write` version first — one inode, unchanged, which is the truncation
  itself — and the fixture is a three-edition star carrying a hand-corrected
  run, because that is what `align --write` rewrites in order to add one spoke
  and therefore what a short file would take with it.
  **The command itself was driven against the corpus**, since a test that calls
  `write_sidecar` does not prove `align` still calls it: two runs on
  `gotoubun-v01` to a scratch path outside the corpus, the second merging a
  second spoke into what the first wrote, where the destination's inode moved
  575378 → 575396 and `lookup` read three editions back off it. See
  `notes.md`, "O26: align --write, driven against the corpus".

**One came from designing another item's fix** — O27, turned up on 2026-08-07
while tracing which render-assigned refs O25's restructure would newly reach,
and not blocking because a write nobody reads changes nothing a reader can see.
It closed the same day, on O25's own branch:

- **O27 — Two refs are held and read by nothing. Closed.**
  `App.tsx` declares `pageUrlRef` beside `pageAreaRef` under one comment
  claiming both are "read by the layout effect below", and assigns
  `pageUrlRef.current` further down. **Neither is read, and there is no layout
  effect in the file at all** — every hook there is a `useEffect`. `pageUrlRef`
  has no reader anywhere in `web/src`; `pageAreaRef` is attached to the page
  area's own `ref=` and its `.current` is never read either. *This bullet
  claimed on 2026-08-07 that `pageAreaRef` was read and that the region-drag
  geometry came off it. It does not: `beginRegion` takes its rectangle off the
  `<img>` via `draftImage`/`pointOn`, and geometry coming off the page area is
  the exact bug M5c fixed — see `notes.md`, "The annotation overlay was placed
  on the page area, not on the page". Corrected by this branch's own
  whole-branch review, before anyone closed the item from a false premise.*
  **It was never a leftover from a removed reader.** `git log -S` finds exactly
  one commit touching either line, `bcafee5` (M5c), and it *adds* both. The ref
  was born dead, most likely because M5c's overlay ended up measuring the image
  it sits over — the fix recorded under "The annotation overlay was placed on
  the page area, not on the page" — rather than working from a URL an effect
  would have had to compare.
  **What it costs, and what it costs to leave.** Nothing at runtime: one string
  assignment per render. The cost is the comment, which tells the next reader
  that a reader exists and sends them looking for it — the same staleness the
  open-items table's symbol-not-line rule exists to avoid, one level down.
  **It was tabled rather than fixed in passing, and then folded in anyway on
  the owner's call.** The argument for a separate branch was the standing one:
  O25 is a restructure of the same function, the remote squashes, and two
  claims in one commit cannot be reverted separately. The owner chose to close
  it on O25's branch instead, which is recorded here rather than deleted,
  because the reasoning is still the reasoning — it was overruled, not wrong.
  **How it closed.** Both refs deleted, along with `pageAreaRef`'s `ref=` on
  the page-area div and the comment that promised a layout effect. They were
  dead in different ways and the difference mattered to the edit: `pageUrlRef`
  was declaration-plus-assignment and nothing else, while `pageAreaRef` was
  attached to a live element, so removing it meant removing the attribute too.
  **The one behaviour that could have depended on the page area's node was
  driven, not reasoned about.** With the refs gone, a real pointer drag across
  the page in a browser still swept out a region — the note's anchor went from
  `whole page` to `raw-mono slot 193 · region`, and the rectangle drew during
  the drag. That is the check worth running, because geometry taken off the
  page area rather than the image is precisely the M5c bug named above. Run
  under a copy of the corpus's JSON, restored to byte-identical afterwards. No
  test covers this; nothing in the suite touches `App.tsx`, which stayed at
  397 passing.

**One came from simulating the reader rather than reading it** — O28, turned up
on 2026-08-07 while checking that the credits panel's skipping could not strand
an arrow, and not blocking because it costs one page of movement in a state a
reader reaches only sideways:

- **O28 — A forward arrow on the last group's second slot moved backward.
  Closed.** `stepToGroup` clamped with `Math.min(Math.max(here + by, 0),
  all.length - 1)` and then landed on the chosen group's *first* slot. At the
  end of the volume those two combined: the clamp kept the reader in the group
  they were already in, and its first slot was behind them.
  **The decision the item was waiting on was taken the way it was framed** —
  the clamp now returns the slot unchanged wherever there is no next group, at
  *both* ends rather than only the one that reversed direction. `stepToGroup`
  holds two rules, and they meet only at a boundary: landing on a group's first
  slot says where an arrow goes when it *moves*, and a press with nowhere to go
  does not move. One rule now covers both ends instead of two rules meeting in
  the middle, which is how this arose.
  **Two things this entry had wrong, both found by running it rather than
  reading it, and both changing what the fix had to be.** It recorded that
  backward at the start "already returns the slot unchanged" — true only from a
  group's *first* slot, which is the only case the existing test covered. From
  the second half of a leading joined spread, backward answered 0: the same
  invisible move, in the direction the reader had pressed, which is why it read
  as correct. And it describes a defect of volumes ending in a joined spread; in
  `two` mode a pair is a group too, so an ordinary six-page volume has it at
  slot 5, which the entry's own first measured line shows and its prose did not
  say.
  **None of the affected presses changed the picture**, which is why a manual
  pass never found it. A group is one image and both slots sit inside it, so the
  arrow appeared to do nothing — correctly, at the end of a volume — while
  silently re-authoring which slot a swap, a note or a written position answers
  from.
  **It predated everything around it**, including group stepping's own arrival
  in `c11c1e9` — that commit's clamp meeting that commit's
  land-on-the-first-slot rule — and reproduced with no `skipped` set at all, so
  the credits work neither caused it nor widened it.
  Three tests pin it, across all three modes and both ends, and were checked by
  restoring the old clamp rather than by having once failed to compile: the two
  boundary tests fail with the predicted values and the third — a second slot
  *away* from a boundary still moving in both directions — passes either way,
  which is what stops the fix being spelled "a second slot never moves". See
  `notes.md`, "O28 — closes here".
  **What is unchanged:** an arrow still cannot *put* a reader on a group's
  second slot. That is the cost `c11c1e9` accepted and this does not revisit;
  a swap, a confirmed correction or a restored progress record are still the
  three ways onto one, and an arrow now leaves a reader there rather than
  moving them off it in whichever direction the nearest boundary faces.

**One came from driving a fix against the corpus after making it** — O29,
turned up on 2026-08-07 while checking that `align --write` still wrote at all
once O26 moved it, and not blocking because it makes a message noisier without
making it wrong:

- **O29 — A failed `--write` named a temp path the caller never typed. Closed.**
  `atomic::write` named the *temporary* when staging failed and the destination
  when the rename failed. Inside the module that is the right distinction — the
  two failures are not the same event. At a command line it was not, because the
  temporary is a filename the caller never chose and cannot look for:

  ```
  $ taiyaku align … --write /nonexistent-dir-xyz/out.json
  taiyaku: /nonexistent-dir-xyz/out.json.tmp.4041762.0: No such file or directory
  ```

  The destination was still legible inside that string and the diagnosis
  unchanged, so nothing was *misleading* — noise, not a wrong answer, which is
  why it was tabled rather than fixed on the branch that found it.
  **It was not new with O26 and not confined to `align`.** `credits --write` had
  answered this way since the branch that fixed it, and the four callers inside
  `taiyaku-serve` since `atomic.rs` existed — where it matters less, because a
  500 does not print a path to anyone. *That last clause is false, and finding
  out why is what decided where this was fixed; see below.* What O26 did was put
  it on a second command-line surface, which is what made it worth writing down.
  Before the change `align` carried `.map_err(|e| format!("{out}: {e}"))` and
  named the destination for both failures.
  **Measured, not reasoned about:** the line above is a real run, recorded in
  `notes.md`, "O26: align --write, driven against the corpus".
  **Closed, and in `atomic::write` rather than at the command line.** The
  decision was framed here as a choice between the two, with the command-line
  wrap preferred as the cheaper: `write_sidecar` is a single place since O26
  merged the two writers. **Reading the callers before building it settled the
  choice the other way, because the paragraph above is wrong about them.** It
  says the four `taiyaku-serve` callers matter less "because a 500 does not
  print a path to anyone". `routes.rs`'s `Failed(message)` arm is
  `(StatusCode::INTERNAL_SERVER_ERROR, message).into_response()` — the string
  *is* the body — so `notes::store`, `progress::store` and the sidecar rewrite
  were all handing the temporary's name to HTTP clients. Only `cover.rs`
  discards it. A command-line wrap would have fixed one surface of four.
  **The fix needed no caller changes at all**, which the framing had also
  missed: `atomic::write` is already given the destination, and merely named
  the temporary instead in one of its two branches. Both now name the
  destination, and the distinction between them — a staging write failing is
  not a rename failing — is carried in words rather than by substituting a
  path the caller cannot look for, since the temporary is deleted before the
  error returns. Driven against the corpus, the scenario at the top of this
  entry now answers ``/nonexistent-dir-xyz/out.json: staging a temporary
  beside it: No such file or directory``. Two tests, one per branch, each
  asserting the absence of `.tmp.` rather than an exact sentence, and both
  checked by sabotage: restoring either branch's old wording fails its own
  test and only its own.

**One came from the same corpus drive, one level up** — O30, turned up on
2026-08-07 when O26's fix was checked against the corpus and the documented way
of doing that turned out to run nothing. It closed the same day, beside O29:

- **O30 — A relative `TAIYAKU_CORPUS` skipped every standing check and
  reported `ok`. Closed.** `cargo test` gives each package's test binary that
  package's own directory as its working directory, not the workspace root, so
  the `TAIYAKU_CORPUS=corpus` that `crates/taiyaku-cli/src/corpus.rs` gave in
  its own header resolved under `crates/taiyaku-cli/` and found nothing. Every
  check then skipped, printed `TAIYAKU_CORPUS unset; skipping` — naming a cause
  that was not the cause, since the variable was set — and libtest reported
  `ok`. **A green run that measured nothing**, which is the one outcome a
  standing check must not have. It is the same failure `atomic.rs`'s test
  comment describes one level down, arriving through the gate rather than
  through a filesystem.
  **`scripts/check-all.sh` already knew.** It canonicalises the path and refuses
  a non-directory, in a comment naming this trap exactly — "the checks then skip
  while this gate says it ran them". So the gate was never wrong; the two module
  headers carrying the trapping command were, and nothing else was checking.
  **The two crates failed differently, and one hid better.** `taiyaku-cli`'s
  root resolver tested `is_dir` and folded the result into the unset branch.
  `taiyaku-serve`'s never looked at the path at all, so each of its checks
  reported `<root>/gotoubun-v01/taiyaku.json absent — skipping` — a real path,
  reading like a corpus missing one volume rather than a root that does not
  exist.
  **How it closed: O15's rule, applied to an environment variable.** A value
  typed with nothing usable behind it is refused rather than read as absent.
  Unset still skips, so CI stays green with no corpus; set-but-not-a-directory
  now panics with the working directory named and the cause spelled out. Both
  headers give a working invocation and point at `check-all.sh` — and **both
  were run as written**, which is the whole lesson of this item: `taiyaku-cli`'s
  measures 2,427 comparisons in 38.71 s, `taiyaku-serve`'s 617 pages and 629
  anchors in 15.13 s. A documented command nobody executes is how this started.
  `taiyaku-serve`'s header had no invocation at all until this item; the bullet
  above claimed it did before that was checked.
  **Pinned by three tests** in `corpus.rs`'s own `tests` module — the first
  ungated tests that file has had, which is the point: they run in CI, where
  every standing check beside them skips. They test the message rather than the
  environment, because setting a variable inside a test is process-global and
  races a parallel runner. What they assert is that the message names the
  variable, the value, and the working-directory cause, and that it never says
  `unset` about a value that is set.
  **Driven, not only tested.** `TAIYAKU_CORPUS=corpus` now fails in both
  crates, naming `/home/davey/projects/taiyaku/crates/taiyaku-cli` and
  `…/crates/taiyaku-serve` respectively; unset still skips green; an absolute
  path still runs all twelve. See `notes.md`, "O26: align --write, driven
  against the corpus", which is where the trap was found.
  **`taiyaku-serve` carries a second copy of the guard deliberately.** Both are
  `#[cfg(test)]` modules in different crates, and exporting test scaffolding
  across a crate boundary to save six lines costs more than it saves. The
  message is pinned where nine of the twelve checks live.

**Series-level identity is manual.** Which two files are the same volume is decided
by the person running the tool.

**Blank pages are still anchored on.** A blank page's dhash is exactly zero, so
every blank in every edition is identical to every other and the solver places
them arbitrarily. The corpus check declines to measure them rather than pretending
they agree; refusing to anchor on low-variance pages is unbuilt. See
`notes.md`, "Blank pages match each other perfectly and lie about it".

**Hub choice is declared, not measured.** `notes.md` argues the best hub is the one
minimising total loss across spokes, and that every input to that decision is
something an alignment already reports. A merge keeps whatever hub the file names.

**Three came from L2a's whole-branch review** — O31 to O33, none blocking because
the detector they describe does not work yet for reasons upstream of all three,
and fixing any of them alone would sharpen an instrument that is measuring the
wrong population *(O33 has since closed — see its entry below)*:

- **O31 — The axis is decided once per page, where the design says once per
  cluster. Open.** `detect_page` calls `axis_of` over the whole page's
  components, uses that single answer for its one `merge_along`, and stamps it
  on every region. The design's §4 says the opposite, and §1 justifies storing
  `axis` on a region on the grounds that "the clustering already knows it" —
  which is not yet true of the code. **Three consequences, and the third is the
  one that changes how existing evidence should be read.** A page carrying
  vertical dialogue beside a horizontal SFX or narration line cannot report
  both, which is the case the per-cluster rule exists for. The smear runs on
  that one axis, so a page whose global axis resolves wrong is smeared wrong end
  to end — a second plausible contributor to the measured failure that the
  survey's own diagnosis does not name. And **the survey's `v=`/`h=` figures are
  block counts of a per-page decision**, so the effective sample behind "5 of 9
  nominally correct" is pages, not the 148–402 blocks the table shows: the
  evidence is coarser than it reads. Shipped without being recorded as a
  deviation, which the design now corrects in place.
  **All three consequences are addressed, and the item stays open on what
  replaced them.** L2b Task 5 (`8531d05`) smears both axes, unions the results
  and drops what `detect::uncontained` drops. `axis_of` is now `#[cfg(test)]`
  and no longer reached by the detector at all; `detect_page_with` carries the
  axis on the block rather than the page, so a page can report a vertical
  balloon beside a horizontal sound effect, and no page is smeared end to end
  on one resolved direction; and `region_experiment::summarize` counts `v`/`h`
  over surviving blocks, so those figures are block counts of block decisions.
  The paragraph above describes code the branch deleted, and is kept as written
  in the present tense of when it was true.
  **What keeps it open is a residue the branch measured, not a leftover of the
  old design.** Where both smears produce an *identical* box, both joined the
  same strokes identically and there is no axis to discover: the label reported
  is `uncontained`'s tie-break default — the earlier index, which is
  `Axis::Vertical` given the order `detect_page_with` pushes them in — rather
  than a measurement. That is not a corner case. Over the 90 band-free corpus
  pages `region_experiment.rs` samples, 69 blocks survive both before and after
  the change and **13 of them changed axis**, all horizontal to vertical, every
  one on a block whose box, stroke count, median stroke and ink are identical
  in the two runs; the sample's v/h split moved 55/21 to 65/8.
  `uncontained`'s own doc comment states and bounds this. So the design's §1
  justification for storing `axis` on a region — "the clustering already knows
  it" — is still not true of every region, which is the claim this item exists
  to track. **What would close it:** an axis decided by something the
  clustering measures on a duplicate box, or a `Region::axis` able to say it
  does not know.
- **O32 — The `median_area` disqualifier cannot fire. Open.** The L2 design
  names two absolute disqualifiers for a detector, one of them "median region
  area > 0.15 of the page". `MAX_BLOCK_AREA` discards any block above 0.15
  *before* a `Region` is constructed, and `median_area` is computed over the
  regions that survive — so every region satisfies the bound by construction and
  the median is clamped under it whatever the detector does. The survey reported
  it passing on all nine editions while `blocks_per_page` failed on all nine,
  which is how a check that measures nothing reads when everything is broken.
  **What would close it:** taking the block-size check *before* the filter, over
  the merged blocks as `merge_along` returns them. Until then L2b's promotion
  rule has one live disqualifier rather than the two its wording implies, and
  that is the reason this is tracked rather than left as a curiosity — it
  governs a decision nobody has made yet.
  **Narrowed, not closed, by L2b.** The decision it governed has since been
  made: Task 7 promoted the first candidate under a rule stated entirely in 1:1
  matches and spurious-per-page, which never consults `median_area` — so the
  disqualifier being vacuous cost that decision nothing, and `MAX_BLOCK_AREA`'s
  own doc comment now records why the rule is written that way. Two further
  measurements bear on it and neither closes it: exactly **1 of 478** hand-drawn
  truth boxes exceeds 0.15 of the page (a full-page sound effect on
  `kobayashi-v01/raw-mono` p65, at 0.35), so the ceiling excludes 0.2% of the
  target and was never what cost recall; and against the corpus the filter fires
  on 89 of 90 sampled pages, so it is doing real work rather than sitting inert.
  **The code defect this item names is untouched** — every surviving region is
  still clamped under 0.15 before the median is taken, and the fix named above
  has not been written — so it stays open on its own terms rather than being
  ticked because the decision it was blocking went ahead without it.
- **O33 — Two of the five detection constants reject nothing in any test.
  Closed.** `MIN_STROKES` and `MIN_INK` are each pinned by a fixture that reaches
  them and is refused by them. `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` are
  not: the panel-border and lone-speck fixtures are both rejected by
  `MIN_STROKES` first, and the blocks that do survive sit at 0.015 against a
  0.15 cap and 180 against a span of 40 — so a factor-of-several error in either
  computation would ship green. Both were hand-checked and are correct today.
  *Half of that sentence is no longer true, corrected here on 2026-08-12 rather
  than rewritten: the **panel border** is not rejected by `MIN_STROKES` any
  more.* Since Task 4 put the band before the smear, a 400×600 border is one
  component spanning the whole page at 10,000‱ and `BAND`'s `hi` of 2,000 drops
  it at `glyph_band`, before `stroke_scale` is reached and before any block
  exists for `MIN_STROKES` to count. `MIN_INK`'s own doc comment states this
  correctly and this entry did not. The item is unaffected — the border is
  still rejected before `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` see it,
  which is what the entry is about — only the named rejecter moves, and the
  lone-speck fixture is still `MIN_STROKES`'s (by 2‱ of band margin, which
  `a_lone_speck_is_not_a_region` now asserts rather than assumes).
  **One further trap worth recording rather than discovering later:**
  `MIN_INK`'s own fixture merges at *exactly* the smear threshold, so a future
  `SMEAR_K` below 2 leaves its strokes unmerged, rejected by `MIN_STROKES`
  instead, and the test still passing while no longer touching the constant it
  is named for.
  **The corpus has since been asked the same question, and it answers a
  different one.** Over 90 sampled pages `MAX_BLOCK_AREA` rejects 166 blocks on
  89 of them and `MIN_BLOCK_STROKE_SPAN` rejects 2 blocks on 1 — so the two
  constants this item names are not inert in practice, `MAX_BLOCK_AREA` least
  of all, and the gap it describes is genuinely in the tests rather than in the
  behaviour. It stays open on those terms. What the same measurement did find
  inert is the constant this item lists as *pinned*: `MIN_INK` rejected nothing,
  on any page. That is O34.
  **Closed on 2026-09-15, by two tests that each draw a pair of pages one pixel
  either side of a constant.** `a_block_shorter_than_two_stroke_scales_is_not_a_region`
  lays six 40px bars across a block, so `stroke_scale` is 40 and the span asked
  for is 80: a 79px block comes back empty and an 80px one as one region, drawn
  stacked and turned, so a filter reading `width()` or `height()` in place of
  `longer()` fails one orientation each.
  `a_block_over_the_area_ceiling_is_not_a_region` stacks five 100×39 bars into
  a block 0.15075 of the page, which is refused, and a twin at 0.14975, which is
  kept at its own size. Every other filter has room on both pages, and removing
  the filter under test brings its refused page back, so nothing else is doing
  the refusing. Each constant at ×4, ×2, ÷2 and ÷4, each comparison inverted,
  the span read off the wrong side, and the area read off the wrong side or in
  pixels all fail one of the two tests. Two mutations survive, and no test here could tell either apart:
  `<=` as `<`, since only a page of at least 2²⁵ pixels can put a block exactly
  on `f64::from(0.15f32)`; and the span's `scale * MIN_BLOCK_STROKE_SPAN` as the
  smear `gap`, which is the same number while `SMEAR_K` is 2. The table is in
  `docs/notes.md`, "O33: two fixtures that reach the two block constants".
  Production code is unchanged.

**One came from asking a diagnosis about the case it did not explain** — O34,
found by the measurement that answered why the noisiest edition was the only
one clearing the detector's bar, and not blocking for the same reason O31 to
O33 are not. It has since closed, on the redesign it was waiting for:

- **O34 — `MIN_INK` cannot reject anything on real material. Closed by
  `aa0b479`.** It was
  written as a floor of 0.05 on a block's ink over its own area, and its
  "ink" is the summed **bounding-box** area of the strokes inside — which its
  own doc comment already flags as looser than true coverage. Measured, that
  looseness is total: across the 76 blocks that survived all four filters on
  90 sampled pages, the ratio runs 0.38 to 1.89, **61 of the 76 exceed 1.0**,
  and the filter rejected 0 blocks on 0 pages. Strokes overlap, so summing
  their boxes double-counts paper as ink — and the dense screentone fields
  this filter would be wanted for score 1.03 to 1.29, twenty times the floor,
  rather than anywhere near it. The consequence is not that a wrong block gets
  through today (the filtering is being done by `MAX_BLOCK_AREA`, which fires
  on 89 of 90 pages, and `MIN_STROKES`, which fires on 21), it is that a
  redesign counting on four backstops has two that do anything.
  **What closed it:** the first of the two routes this item named — a true ink
  count. `detect_page_with` keeps the page's own Otsu threshold rather than
  inlining it into `components`, and `ink_in` counts dark pixels inside the
  block against the block's own pixel area, which is the same quantity
  `truth_population.rs::ink_fraction` measured inside 478 hand-drawn boxes at the
  same threshold. The bound is now structural instead of a property of the
  material: a count incremented at most once per pixel of the rectangle it walks
  cannot exceed that rectangle's area, so the `#[expect]` on that filter carries
  a bound that holds where the old one did not. Across the same 90-page sample
  the ratio fell from 0.38–1.89 to **0.09–0.56**, and 61 blocks above 1.0 became
  none.
  **What it rejects now, measured rather than assumed, including the half that
  did not move.** Over the nine labelled editions it rejects 16 spurious blocks,
  taking spurious-per-page from 55.31 to 54.96 — live, but not where the
  precision problem is. **Band-free it still rejects nothing on any of the 90
  sampled pages**, which is the population `region_experiment.rs` measures: at
  0.09–0.56 nothing there comes near 0.05 from above. So the filter is doing work
  in the shipped detector and none in the band-free one, and the item is closed
  on the first of those rather than on both. What it catches is a block that is
  mostly paper — strokes strung far enough apart that the box the smear drew
  around them is nearly empty — and not screentone, which `glyph_band` removes
  upstream; `MIN_INK`'s own doc comment states that distinction, because the
  wording this item was opened against described a different filter.

**O35 to O37 came from the coverage branch, and none of them was introduced by
it.** They are what reading a file closely enough to write tests for it turns
up — which is an argument for the exercise independent of the number it moved.
**All three were fixed on that branch, and the argument against doing so is
worth keeping because it was a good one.** The case for leaving them open: a
coverage branch is a gate and its tests, this repository squash-merges, and a
behaviour change buried in forty-odd commits cannot be reverted on its own.

What decided it the other way was that the branch already carried exactly such
a change by explicit decision — the 412 retry, whose fix is described above and
which is the direct sibling of O35. The revertability cost had been accepted
once already; three more of the same class did not change the shape of the
branch, and leaving a known write-loss filed rather than fixed had a cost of its
own. Recorded so that the next branch faces the question fresh rather than
inheriting this as a precedent: the default for a gate branch is still to file
and not fix.

- **O35 — A `PUT` response with no `ETag` silently killed every later note
  write. Closed by `178f3e5`.** `api.ts::putNotes` returns `etag: res.headers.get('etag') ??
  undefined`, so a response without the header yields `undefined` rather than
  an error. `notesWrite.ts` passes that straight through — `{ kind: 'written',
  doc, etag: fresh.etag }` — `App.tsx` stores it with
  `setNotesEtag(outcome.etag)`, and `writeNotes` opens with `if (!notesDoc ||
  notesEtag === undefined) return`. From then on every note a reader writes
  returns at that line: no request, no notice, no console entry, and the editor
  closes as though it had saved.
  **The same hole was already recognised and fixed one path over.** On the 412
  re-read, `notesWrite.ts` has `if (fresh.etag === undefined) return { kind:
  'refused', notice: message }` — refuse rather than write without a validator.
  That is exactly the right instinct; it was applied to the re-read and not to
  the ordinary response.
  **Not a hypothesis about the server:** it is a statement about what this
  client does if a response omits the header, and the client is the half that
  fails quietly. `taiyaku serve` sets the header today, so nothing misbehaves
  now — which is why this was a latent hole rather than an outage, and why
  there was an argument for filing it at all.
  **What closed it:** the 412 path's own instinct, applied to the ordinary
  response. The single `if (!notesDoc || notesEtag === undefined) return` became
  `writeGate`, which answers with one of three arms rather than with one
  silence — `wait` (no document yet, a load still in flight, and the only arm
  that stays silent because there is nothing yet to tell the reader about),
  `refuse` (a document but no validator: exactly the O35 case, and it now
  carries a notice), and `write`. The correct-behaviour test was watched failing
  against a faithful port of the old guard before the fix went in, and
  `App.test.tsx` observes the refuse notice a reader actually sees (`4105a1c`).
  **It cost a fifth behaviour change, on a branch that recorded four.** On the
  `refused`-after-failed-retry path, `notesWrite.ts` no longer refreshes
  `notesDoc` and `notesEtag` from the 412 re-read; the old code set both before
  recursing. So a write that 412s, and whose retry then also fails, pays one
  extra 412 and one extra re-read before the next write lands. That is a real
  difference in what a reader's machine does, and it was accepted rather than
  overlooked: the cost is one round trip on an already-rare path, and *not*
  adopting the other device's document is what keeps the reader's own notes on
  screen instead of silently replacing them with a copy they never saw. It is
  recorded here because the branch's other four behaviour changes are recorded,
  and this one should not be the exception for having been a consequence rather
  than an intention.

- **O36 — The write debounces outlived the component. Closed by `4303dee`.** `writeProgress`
  and `saveNotes` are `debounce` handles with timers up to 700 ms, and no
  unmount path cancels either. An unmounted `App` can therefore still issue a
  `putProgress` or `putNotes` against a work the reader has left.
  **A reviewer established this is a two-line fix rather than an
  impossibility**, and that finding is the reason it is written down instead of
  waved off: `hooks.ts::useFlushOnHide` already runs a cleanup on unmount, and
  that cleanup today does nothing but remove its two listeners; `debounce`
  already exposes both `flush()` and `cancel()`, and no unmount path calls
  either. The pieces are all present and unconnected.
  **Nothing observed it, which is the whole reason filing it was arguable**:
  `App` is the SPA root and never unmounts, so no reader could reach the
  behaviour. It becomes real the moment a route, a work switch, or a test
  harness mounts and unmounts it — and a test harness is now something this
  repository has, which is what turned the argument.
  **What closed it:** the decision the diagnosis said was the work — and it came
  out **flush at both**, made per call site rather than once for the pair.
  `useFlushOnHide`'s cleanup now flushes `writeProgress` and `saveNotes` on
  unmount, because cancelling `writeProgress` drops the reader's last page turn
  and cancelling `saveNotes` destroys up to 700 ms of prose that exists nowhere
  else: a debounce window that has not elapsed is not evidence that the reader
  changed their mind. `cancel()` keeps its place where the pending write is
  genuinely *stale* rather than merely pending — at `stepTo`, where a newer
  position is about to be written over it anyway, and at the panel's delete,
  where the note the pending write describes is gone. The decision was indeed
  the work; the code was the two lines a reviewer had promised.

- **O37 — `reviewNotes`'s doc comment described an arm nothing can reach.
  Closed by `f4047b7`.** It said `a` "closes the editor if that is what is open, so `a` always
  means 'show me the list'". `keyplan.ts` disagrees and is right: `if
  (guards.panel === 'edit') return undefined` refuses every intent but
  `Escape` while the editor is open, so `a` never reaches `reviewNotes` at all
  and `toggleReview`'s close arm is unreachable from the keyboard.
  **The behaviour the code has is the one a reader wants** — `a` is a letter,
  and someone typing a note should get the letter rather than lose the panel —
  so this is a documentation defect, not a behaviour one. The file already
  states the correct rule a dozen lines above, in `startNote`'s comment: "no
  module key reaches this at all while the editor is open (`keyPlan`'s `panel
  === 'edit'` refusal)". Two comments in one file contradict each other, and the
  wrong one sits on the function a reader would check.
  **What closed it:** the comment was rewritten to describe the toggle the
  function actually performs, which was the cheapest fix of the three — and
  chasing the *claim* rather than the file turned up a second copy of it.
  `notes.ts`'s own `toggleReview` doc repeated `App.tsx`'s wrong sentence
  unchanged, and was corrected with it in `7cab6ec`. The one judgement in the
  item went the conservative way: `toggleReview`'s close arm stays. It is
  unreachable from the keyboard, not unreachable from the function, and
  deleting a correct branch to make a comment true is the wrong direction of
  fix. This was the only one of the three where fixing it found more of itself
  somewhere else, which is the argument for grepping the sentence rather than
  editing the line.

**O38 and O39 came from the mutation branch, and neither is a mutant.** Both
were found *beside* the survivors rather than among them: one by a reviewer
declining to take a claim on trust and writing a probe instead, the other by a
reviewer asking what a survivor's disposition would mean if it ever happened in
CI. That is worth recording because it is not what mutation testing is
advertised to do. The tool finds unverified lines; what actually surfaced these
two was a person reading the code around a line the tool had pointed at.

- **O38 — `unpaired_runs` treats a gap run's own pages as covered. Closed.** The
  function answers "which pages of `edition` does the alignment against
  `against` never reach", and it builds that answer by collecting every page
  any run mentions and complementing it. The collection is unconditional:

      let range = match mine_is_a {
          true => Some(run.a),
          false => run.b,
      };

  The `false` arm is right by construction — a gap run has no `b`, so `run.b`
  is `None` and the gap's pages stay uncollected. The `true` arm is wrong for
  exactly the same case: a gap run's `a` span is precisely the pages that have
  *no* counterpart, and marking them "mentioned" is the opposite of what the
  function is for. Demonstrated rather than argued, on a two-member fixture
  whose middle run is `{"a":"3"}` with no `b`: the `to` side reports
  `[UnpairedRun { pages: 3..4, … }]` and the `from` side reports `[]`.
  **Reachable today.** `main.rs:1268` stores edges as `from: a_name`, defaulting
  to `sidecar.hub`, so stored edges run hub→spoke; `credits.rs:246` lets
  `--against` bypass the hub refusal, and `credits.rs:299` then calls
  `unpaired_runs(doc, edition, counterpart)`. So `taiyaku credits --edition
  <hub> --against <spoke>` takes the defective arm and prints zero candidates
  for a volume that has them.
  *The two `--edition <hub>` invocations above are wrong and are kept as
  written: `credits` has no such flag. Its usage is `credits <member>...
  --sidecar FILE [--against EDITION]`, and the edition is identified by
  fingerprinting the archives passed, not named on the command line. The
  mechanism the passage describes — hub as the `from` side, `--against`
  bypassing the hub refusal, `unpaired_runs(doc, edition, counterpart)` — is
  exactly right, and the drive below ran the corrected form.*
  **Why it was open rather than fixed:** the branch that found it was a test-only
  branch, and this is a behaviour change. The fix is one expression —
  `run.b.map(|_| run.a)` — but it wanted its own commit and its own corpus drive,
  because "prints nothing" is the failure mode that hides how long it has been
  wrong. It fails silently in the found-nothing direction, corrupts no stored
  document, and no current caller in the corpus checks hits it, which is why it
  was judged not worth blocking the branch on.
  **Closed, and the corpus drive is the reason it was worth doing separately.**
  The fix is the one expression the entry predicted. The drive was not: twelve
  volumes across four series, each aligned raw-mono → en-mono and asked from the
  hub side, **every one of which answered `runs=0` before and the stored gap
  count after** — 26 gap runs spanning 36 raw pages, none of them reportable by
  the side a reader would naturally ask. The estimate that "no current caller in
  the corpus checks hits it" was true only because no corpus check asks the hub;
  every volume in the corpus has the material. And it was hiding a real result
  rather than only runs: `kobayashi-v01` raw page 145 of 146 comes back
  `at-end yes … credits`, where the same command previously printed a bare
  header. The `before` column was built from a clean worktree at `ddb6149`
  rather than by reverting the fix in place, so no number depends on an edit
  having landed. **The corpus now watches this arm rather than only having been
asked once**: `gotoubun_hub_reports_the_pages_its_english_edition_never_reaches`
is a thirteenth standing check, and it was confirmed failing against `ddb6149`
with the new test applied to it — `left: []`, `right: [1, 6]`. Both existing
credits checks ask the `to` side, so neither could ever have seen this.
`notes.md`, "O38: the arm that answered nothing, on every
  volume in the corpus", is the record — including why
  `each_end_of_a_directed_edge_reports_its_own_gaps`, which executes the
  defective arm and was hardened in `ddb6149` for being too weak, still cannot
  fail on this: its only run is paired, so both arms agree.

- **O39 — nothing bounded a test's runtime, so a hang was not a failure. Closed.**
  Mutation testing produced this one by accident. `cargo-mutants` reports a
  mutant that stops a loop terminating as `TIMEOUT` rather than `MISSED`, in its
  own bucket, and exits 3 — it is *not* counted as caught, though it is real
  discrimination in the sense that the mutant cannot produce a silently wrong
  alignment. Asking whether that disposition is good enough turned up the
  gap: **there is no `.config/nextest.toml` anywhere in the repository, no
  `timeout-minutes` in `.forgejo/workflows/ci.yml`, and CI does not run nextest
  at all** — the `test` job runs `cargo llvm-cov`, which wraps `cargo test`. So
  the auto-timeout that makes a hang legible locally exists only inside
  `cargo-mutants`, and a regression of that shape reaching `main` would hang the
  forge until the runner's own limit rather than fail it.
  This branch takes `solve.rs` from one tracked timeout to two: a test added for
  the traceback converted the `-=` → `/=` mutant in `solve`'s `SkipB` arm from
  `MISSED` to `TIMEOUT`, which is an improvement in verification and a standing
  cost of about 21 s on every future run of that file — measured at 20.5 s for
  that mutant and 21.5 s for the one already there, against the tool's
  auto-selected 20 s bound.
  **Why it was open rather than fixed:** it was a CI change, and the branch that
  found it changed no CI. It also wanted a measured number rather than a guessed
  one — the whole workspace suite was 552 tests in 0.76 s at that branch's head,
  so a per-test bound was cheap to set, but the right value had to come from the
  slowest real test rather than from a round figure.
  **Closed, by two bounds sized from a measurement rather than a guess.**
  Bisecting the whole workspace under a temporary short `slow-timeout` found
  nothing slower than 0.75 s in 639 tests, so `.config/nextest.toml` carries a
  30 s warn and a 60 s `terminate-after` — 40x and 80x the slowest test that
  exists — and CI now runs the suite under `cargo llvm-cov nextest` rather than
  plain `cargo llvm-cov`. Every job also carries `timeout-minutes: 10`, against
  a worst observed job duration of 134 s. **The per-test half was triggered on
  the forge itself, not only reasoned about**: PR #47 carried a deliberate hang
  for one run, and the `test` job failed at 153 s on that commit and passed at
  117 s on its exact revert — both far under the ten-minute job bound, so only
  the per-test kill fired. **The `timeout-minutes` half is untriggered rather
  than untested, and deliberately so**: proving it would fire means hanging a
  job for the full ten minutes on the shared runner, where the per-test bound
  above could be — and was — triggered in about a minute. **What was and was
  not directly observed is worth separating honestly, and that has changed
  since this was first written.** Against Forgejo `15.0.5~gitea-1.22.0`, this
  build registered no job-log endpoint, so the CI log text itself could not be
  retrieved; the log excerpt on record came from running the same `ci.yml`
  command locally against the hanging test, not from reading the forge's own
  output, and what the forge itself showed directly was only the
  `test`-job-only failure at 153 s on the hang commit and the success at 117 s
  on its revert. The instance has since been upgraded to
  `16.0.2+gitea-1.22.0`, which registers `/actions/runs/{id}/jobs` and
  `/actions/jobs/{id}/logs`; the log for job 3121 (run 213, the hang commit)
  has now been retrieved directly from the forge, and it shows
  `SLOW [> 30.000s]`, `TERMINATING [> 60.000s]`, `TIMEOUT [ 60.003s]` and
  `Summary [ 61.243s] 640 tests run: 639 passed, 1 timed out` — the values
  `.config/nextest.toml` configures, read on the runner rather than stood in
  for. What was inferred is now observed. `notes.md`, "O39: what the slowest
  test actually is, and the two bounds sized
  from it", is the record, including why nextest's process-per-test isolation
  makes coverage measurement nondeterministic by a few lines without moving
  either coverage gate.

**One came from the independent review of `borrow-lender-and-arrows` itself**
— O40, turned up while checking the new arrow-on-a-gap path against what
`credits` promises, and recorded as not blocking because nothing in the corpus
could reach it. It has since been closed anyway, which is the entry below; the
paragraph that argued for leaving it is kept rather than rewritten, because
what it decided was to *not fix it speculatively*, and that was right at the
time and is what the fix had to answer. **O41 follows it as a second bullet and
came from somewhere else entirely** — the fourteen-edition credit survey,
hashing the pages its own recurrence column described — and is filed here
rather than under its own heading only because it opened after this section was
written:

- **O40 — An arrow out of a gap or a borrowed page can land on a credit page
  the reader asked to skip. Closed.** `modules/credits.ts`'s doc states the
  mechanism — `skip` is carried out by handing the skipped slots to
  `pairing.ts`'s `groups` rather than by filtering its result — and
  `docs/status.md`'s own module description states the promise it buys: "the
  page area, the arrows and the filmstrip skip together." That promise holds
  for every arrow that goes through `groups`/`stepToGroup`, which is the only
  path that ever consults the skipped set. It does not hold for the branch
  this same review's Fix 1 added a paragraph about: out of a gap or a borrowed
  page, `session.ts`'s `applyIntent` `step` case reaches `nearest.ts`'s
  `nearestPaired` instead, which resolves a candidate slot straight off the
  `Resolver` — `display`'s `at`/not-`at` split, nothing else — with no
  knowledge of `skippedApartFrom`'s set. A reader standing in a gap or on a
  borrowed page whose nearest resolving neighbour is a credit page they asked
  to skip lands on it anyway.
  **Unreachable today, on purpose left that way rather than fixed
  speculatively**: `skip` defaults `false`, and no corpus edition names any
  credit pages, so `skippedApartFrom` never has anything in it for
  `nearestPaired` to fail to consult.
  **What would make it reachable:** a corpus edition (or a future one) with
  credit pages named by `crates/taiyaku-cli/src/credits.rs`'s heuristic and
  `skip` switched on, positioned so a gap or a borrowed page's nearest
  resolving neighbour is one of them.
  **What would close it:** giving `nearestPaired`'s scan the same skipped set
  `groups` already receives, so a candidate slot in it is passed over the same
  way `stepToGroup` already passes over one — rather than teaching a second,
  independent module the invariant `skippedApartFrom`'s doc already states is
  meant to live in one place.
  **Closed on 2026-08-14 by exactly that**, and the route mattered more than
  the fix's size. `nearestPaired` takes the skipped set as a fourth argument,
  defaulting to an empty one, and `landingAt` returns `undefined` for a
  candidate in it — so the *scan carries on* rather than the *result being
  rejected*. That distinction is the whole behaviour: rejecting at the end
  would leave a reader with no offer at all wherever a credit page sits between
  them and a real one, which is precisely the case the item is about.
  **The defect was never in either function**, which is why the test that
  proves it is a mounted one. `App.tsx` calls `nearestPaired` twice — once
  through `applyIntent` for the arrow, once directly for the `Gap` screen's own
  offer buttons — and handed the set to neither. A unit test can only pin what
  each function does with a set it is given; whether both call sites give it
  one, and whether they give the *same* one, is a question only the component
  can be asked. Both now read one `skippedRef`, assigned during render beside
  `groupedRef` from the same `skippedSlots` the grouping was built from, and the
  jsdom test asserts the arrow's destination and the button's label together —
  an arrow that skipped while the buttons beside it still advertised the skipped
  page would be the same defect wearing the opposite face.
  **`skippedRef` is a second ref rather than a lookup through `groupedRef`.**
  A skipped slot is absent from the grouping — but so is a slot past the end of
  the edition, and the gap branch has to tell those apart.
  **What made it reachable is what the entry above asked for, built in the
  test rather than found in the corpus:** `App.test.tsx`'s fixture names `en`
  page 4 — the page the offer before the gap lands on — as a credit page and
  switches `skip` on. The corpus still cannot reach it and `skip` still defaults
  off, so this is the first assertion in the project about `skip` interacting
  with a gap, and it exists because the fix could not otherwise be watched
  failing. Four unit tests sit either side of it — two in `nearest.test.ts` and
  two in `session.test.ts`, each covering both a set that leaves an offer and
  one that exhausts the side — and the *default* is pinned by every other test
  in both files, all of which pass no set at all and none of which moved.
- **O41 — a page's hash depends on which archive the page sits in, and the
  duplicate detector compares across archives. Open.**
  `crates/taiyaku-cli/src/fingerprint.rs`'s `describe_pages` computes
  `common_border(pages)` — the median flat border over about twelve sampled
  pages, symmetrised — and crops **every** page by it before hashing. The
  fingerprint cache is keyed on a member's `sha256`, so `describe_pages` runs
  per member and each chapter file derives its own border from its own pages.
  A page is therefore hashed as a function of itself *and* of the archive it
  was distributed in.
  That is right for the job it was written for. A per-edition crop is what
  removes scan letterboxing consistently so two editions can be aligned, and
  `flat_border`'s own `MAX_SIDE` bound exists to keep it conservative. But
  `credits.rs`'s `recurrences` — signal 2, "the nearest page in a *different*
  member" — is the one consumer that compares hashes **across** members, which
  is exactly the axis the border is allowed to vary along.
  **Measured on 2026-08-13, on material where re-encoding is ruled out by the
  bytes.** `corpus/kobayashi-v01/en-fan` carries one credits card at the end of
  six chapters, byte-identical in all six: entry `sha256` `2da7969ae253…`,
  398,900 bytes, 1028x663, in six archives with six distinct `sha256`. The
  cache records it at 1028x663, 1028x661, 1028x659 and 1016x661 depending on
  the chapter, and `recurrences` reports its distance to the nearest copy as
  0, 0, 0, 0, **2 and 3**. Identical input, six different answers, and the two
  wrong ones cannot be blamed on encoding because there is only one encoding.
  **The error is signed.** A crop that varies pushes two copies apart and never
  together, so signal 2 under-reports recurrence and never over-reports it. Its
  measured precision of three in five is therefore a floor rather than an
  estimate — a page it missed may be a duplicate it failed to see rather than a
  page that does not recur.
  **What this does not overturn.** `notes.md`'s "the pleas sit at 14-24 rather
  than near zero" is not explained away by this: those three Go-Toubun cards
  have three different `sha256` and three different byte lengths, so the
  re-encoding that section names is real. Cropping is a second contributor it
  did not consider, and plausibly the larger one there — a 600x600 card cropped
  by a border derived from 1465x2162 story pages — but that ordering is
  unmeasured and this entry does not claim it. The section is corrected in
  place rather than rewritten.
  **What would close it:** hashing uncropped for the recurrence pass.
  `describe_pages` already decodes each page exactly once and already computes
  more than one hash per page for landscape pages, so a second, uncropped hash
  costs a resize rather than a decode, and `CachedPage` is versioned. The
  alternative — making the border edition-wide rather than member-wide — was
  considered and is worse: it would change what every alignment in the corpus
  hashes, to fix the one consumer that reads across members.
  **What it blocks:** nothing that has shipped. Signal 2 is a diagnostic
  `credits.rs` is explicit about never letting decide, and signal 1 reads the
  alignment rather than any hash. It does bear on the credit-detection tier
  design, whose fallback corroborator is this signal.

**O231 was found by the comments pass on 2026-10-29**, by reading
`open_archive` for a comment's sake, and its bullet follows.

- **O231 — `open_archive` reads its header with one `read`, and a short read
  can turn a tar into "not a readable cbz". Open.** `taiyaku-read`'s
  `pages.rs` `open_archive` fills a 512-byte buffer with a single
  `file.read(&mut header)` and hands `&header[..read]` to
  `unsupported_container`, which asks `format::from_header`. Every signature
  but the tar's lives in the first six bytes; the tar magic sits at offset
  257. A read that stops before that magic therefore leaves a tar
  unrecognised, `unsupported_container` returns `None`, and
  `zip::ZipArchive::new` fails on it with "not a readable cbz" where a whole
  read says "this is a tar archive". That is the failure `format.rs`'s
  `read_header` doc describes, and `read_header` is the loop that fills the
  buffer; `open_archive` does not call it.
  **Found by reading during the comments pass, 2026-10-29, and not
  reproduced.** The fix is to read the header through the same loop, in its
  own PR.

## What comes next

Roughly in order. All four are now settled — one closed by a measurement that
came out against shipping it, one shipped as two commands, the third the large
piece, shipped across four milestones, and the fourth shipped as an eighth
command plus a field in the format — and they stay here rather than being
deleted, for the reason the open-items table gives: a list that only ever grows
says nothing about whether anything lands. **All four are now shipped whole**:
the fourth's reader half — the panel and a `skip` that governs something —
landed on 2026-08-07 with O25, which had blocked it, closed the same day.

**A fifth arrived from below rather than from this list.** L1 geometry sat under
"Later" from 2026-07-30, the day this file was written, until it shipped on
2026-08-08; it is §5. The entry that advertised it is kept and corrected at the
foot of "Later" rather than deleted, because both halves of what it promised
turned out to be wrong and that is worth reading. Everything still genuinely
unbuilt is under "Later".

**A sixth was never on any list, here or under "Later".** It is three reader
complaints raised after L1 shipped — pages not filling the page, a gap down the
middle of a spread, and no way to find out what the keys do — grouped onto one
branch because each is small and none touches the alignment. It is §6. That it
came from reading the corpus rather than from a review is the pattern `notes.md`
keeps recording: the browser is where the reader's own defects surface, and no
amount of unit testing substitutes.

### 1. Refuse on margins, not just counts — measured, and margin does not fit

The candidate fix was measured against the corpus rather than shipped. Margins
are quantised to whole 256ths, and the corpus's one confirmed needle (Color
Walk, margin 0.004) sits exactly one bit below its weakest confirmed-good
pairing (Monster's 2:1 repagination, margin 0.008). No safety factor fits in a
one-bit gap, so no margin threshold ships. Average run length carries the case
instead — every confirmed-good pairing at 36.0 to 200.0 slots, everything else
at 6.8 or below — and on this corpus runs per hundred pages separates as well,
once refusals are gated on coverage; the swap was made because run length is
stated in slots rather than per hundred pages, not because the old measure
could not tell these rows apart. `verdict()` already called the needle case
`Fragmented`; both `align` paths now print that verdict, so the case that
opened this item is no longer silent.
**Closed.** Twenty-two needles have since been measured, and they settle it
harder than the survey could: a needle outscores four confirmed-good pairings
on margin and seven on separation, so neither calibration statistic can be made
to refuse this case. Only run structure separates, by a factor of twenty. What
remains open is narrower and belongs to the reader rather than the solver —
`Fragmented` is reported and nothing refuses on it, so a reader consuming a
sidecar has to look at the verdict rather than assume acceptance means
correspondence.

### 2. Duplicate resolution as a command — shipped, and one word in the protocol was wrong

The protocol was worked out by hand across 109 volumes and is now two commands.
`inventory` finds the sets in a release directory; `duplicates` resolves one set
into a decision. On the One Piece collection the hand run took 19 duplicate sets
down to 13 real choices, with 2 mislabelled files flagged rather than silently
ranked, and the commands run the same three steps:

1. **Signature** — file count, total bytes, sorted size list, confirmed by a
   content hash over page bytes wherever it matches. Settles exact duplicates
   without decoding a page, and a copy it settles is never fingerprinted at all.
2. **All-pairs alignment within the set** — not everything against the first
   copy, which hides clusters. Spreads always split, with no flag to turn that
   off, because the headline case in this collection is 200 images against 101.
3. **Cluster on identity, flag what is not this volume, and rank the rest** by
   content first — slots, then pairings against an `--against` reference — and
   only then by how cleanly it aligns. What is not this volume is whatever
   refused against every other copy; with a reference edition it is whatever
   pairs nothing with the reference, which is the only thing that can answer a
   two-copy set where each refuses the other.

**Say what shipped, not what the design sketched.** `notes.md` used *identity*
two ways, and the command uses only the stricter one. `align` reports unmatched
slots for the **A side alone**, so a pair that covers all of A prints as
"nothing unmatched" whether or not B carries slots of its own that nothing
reaches; read that way, containment looks like identity. The command asks both
sides and reports five relations — identity, `within`, `contains`, distinct,
refused. Of the five pairs in the corpus that come within two slots of covering
each other, **three are containment and two are identity**: One Piece v092 is
200 slots wholly within 202, Bocchi's two real scans are 124 within 125, and
Chainsaw Man's `src-a` is 198 within `src-b`'s 199, while Chainsaw Man's two
198-slot scans and One Piece v077's c1 and c2 do cover each other on both sides.
`notes.md` called four of the five identity. Clustering merges on identity
alone, because a container holds content the other copy lacks and collapsing the
two would discard the very difference the choice is about — the ranking already
prefers the container, on slot count.

**Six gated corpus checks measure this, and all six pass.** v077 resolves to
clusters `{0}` and `{1, 2}`, the identity pair at 1 run and 234 of 234 against
104 and 100 fragmented runs for either copy against c0. v092 comes out
containment, two clusters, c1 first on 202 slots. Bocchi's *Gaiden* file refuses
against both real scans, is flagged, and is excluded from the ranking. A sixth
runs those same three Bocchi copies a second time, against the *Gaiden* file's
own English edition, and measures the flagging inverting — the two real scans
flagged and the *Gaiden* file the only one ranked, at 185 slots against the 125
it ranked behind with no reference. Four Chainsaw Man raws against the English
release rank `src-b` first on 199 slots, with `src-a` and `src-c` collapsed into
one identity cluster. `inventory` over `op-duplicates` finds 34 candidates
across 16 volumes, every one of them duplicated. The full figures are in
`notes.md`, "Duplicate resolution as a command, and half the identity claims
above are containment".

**What it does not do, stated with the example rather than as an abstraction.**
The ranking prefers whichever copy holds more material, including material that
may not belong: on v077 the command names copy 0 the best, on 236 slots against
234 — and copy 0 is the different-crop outlier that fragments against both of
the others. The pairs table above the ranking shows that plainly and `--against`
a reference edition settles it, but a ranking read on its own picks the outlier
here. `inventory` has the matching caveat: it groups on the deepest directory
that holds a whole edition, which is not always the copy directory a person
would name, and `--pattern` matches against that directory's name rather than
the copy's.

### 3. The reader — shipped

A PWA consuming the sidecar, with the correction UI as part of it rather than
after it. This was the multi-week piece and the point of everything above, and
every stage it was split into — M5a, M5b, M5c, M5d — has now landed.

**Its blocker is cleared.** Until 2026-07-30 there was no way to produce a
sidecar with three editions and no way to resolve through one, so on any work
with more than two editions — which the primary development corpus is, at five —
the reader could not have answered "show me this page in edition C". `align
--write` now merges and `resolve` now composes.

**The swap UX is settled, not open — this line previously said `notes.md` left
it undecided, which had stopped being true.** `Space` toggles between the
current edition and the last one and `1`..`9` jump directly, settled on
2026-07-30 and reopened and re-settled on 2026-08-01: the case against a
hold-to-open picker was "it costs the one-key promise," which only holds on a
device with a key, so a later brainstorm's picker did not lose the argument, it
answered a different device. Desktop-first was confirmed the same day, which is
what keeps the toggle for this reader; the picker moves rather than dying,
reassigned to a future native iOS reader where a sheet is the right gesture for
a device with no key to promise. See
`docs/superpowers/specs/2026-07-30-reader-design.md`, "Swap is a toggle key
plus number keys."

**M1, M2 and M3 have landed.** M1 moved page reading into `taiyaku-read`, so the
aligner and anything serving pages derive the normative sequence from one place,
and added addressing a single page by index — serving page 47 of a 195-page
volume costs one page rather than the volume. M2 added `taiyaku-serve` and the
`serve` command: the work list, the sidecar verbatim, a page by index with its
content type, and a reading position stored beside the sidecar, all behind a
`PageSource` trait so running beside Stump and replacing it are the same code
above that line. A gated corpus check compares every page the server serves
against the page the aligner sequenced — 617 of them on Go-Toubun v01, no
disagreement.

M3 added Stump support and an OPDS 1.2 + PSE feed. **Say what shipped, not what
the design sketched:** the design named a third `PageSource` implementation,
`StumpLibrary`, alongside `LocalLibrary` and a composite. It was never built,
because it could not have answered anything on its own — resolving a member to
a Stump book needs the resolution map, which only the composite holds, so a
standalone Stump-only source would have been unusable by construction. What
shipped is `CompositeSource`, which resolves each member to whichever source
holds it, with the Stump-backed branch (GraphQL to resolve, REST to fetch
bytes) living inline inside it. `PageSource` has **two** implementations, not
three.

**What would make the third one worth building**, recorded so the decision can
be revisited on evidence rather than re-argued: a work whose members live only
in Stump and never locally. That is not reachable today for a reason deeper
than the trait — `resolve_member` needs the member's *local* file, for its size
and for the page-hash confirmation that makes a match trustworthy, so a member
taiyaku does not hold can never resolve at all. M3 therefore offloads page
reads for files you already have; it does not let taiyaku serve a work it does
not have. Serving one would need a different way to match a member to a book,
which is its own design question and wants its own pass — at which point a
standalone Stump source stops being nomenclature and starts being load-bearing. **Say what the feed actually earns, not what the milestone originally
claimed:** M3's stated end state was "Panels reads the library", but that was
already true without taiyaku — this Stump instance serves OPDS-PSE itself, and
a third-party reader could point at it today. What taiyaku's feed adds is
grouping several editions under one work, which neither Stump nor Komga can
express: on this instance `Go-Toubun no Hanayome` and its Full Color Edition
are two unrelated 14-book series, with no link between them and no way to make
one inside either server. The gated `PageSource` equivalence check measured
this for real against the live instance: 187 pages of `en-mono` agree
byte-for-byte between `LocalLibrary` and the composite's Stump-backed path,
and 8 members — the raw and all seven fan chapters — are absent from Stump and
correctly recorded absent rather than retried per page. See `notes.md` for the
coverage table and what the instance exposes.

**M4 has landed, and it is the smallest of them.** `Route::resolve_slot` makes
the slot rather than the page the thing a reader resolves, which matters exactly
where a run ends mid-spread: in the primary sidecar, `en-mono`'s page 4 is a
joined spread whose right half pairs with raw 5 and whose left half pairs with
nothing, so a reader resolving the page instead of the half would show one and
call it the other. `taiyaku-wasm` puts that in the browser's reach without
`taiyaku-core` taking a `wasm-bindgen` dependency, and the gate now builds both
for `wasm32-unknown-unknown` — the binding for a second reason, since a
signature that cannot cross the JS boundary compiles natively and fails only
there. What was **not** claimed at the time: nothing had executed the module
under a wasm runtime. The agreement check compares the binding against the
core on every slot of every ordered pair of a two-edition fixture, which
catches conversion drift and not codegen.
**M5a closed that gap by running it.** `scripts/smoke-wasm.mjs` loads the
compiled `.wasm`, parses the fixture and checks six properties against it —
`editions`, both slot counts, a half-aware `slotAt`, `indexOf` and one
`resolveSlot` — against the same values `binding.rs` already pins natively.
All six passed, and the generated `.d.ts` matched what the plan had assumed
about the binding's shape. See `notes.md`, "M5a, measured directly: the
reader's own evidence, not the scratch ledger's". **What is still not
claimed:** CI does not run this script — `docs/ci.md` records why — so a
codegen break would still go green there; the discipline stays a local one,
run after any change to `lib.rs`.

**The Panels measurement has started, and found a defect before it found a
number.** Panels read from this server over the LAN on 2026-07-31 — see
`notes.md`, "Panels, measured: the feed was one-based and should not have been".

The first session's finding was not a performance figure but a correctness bug:
**OPDS-PSE counts pages from 0 and taiyaku's feed counted from 1**, so Panels
met a 404 on opening every book, and every page it then displayed was the one
before the page it believed it was on. Thirty per cent of the session's requests
were that 404. Fixed, and the test that used to assert the wrong convention now
asserts the right one at both ends of the range.

**The targets are now measured**, over a deliberate fast-flick session of 177
page requests. The server is not the constraint by a wide margin: median
processing 1.43 ms against a tightest client request rate of 40 ms. Panels
issues no concurrent page requests — zero overlaps, and the arrival floor holds
at 40 ms even flicking as fast as the UI allows — and never asks for a reduced
size, so it takes whatever resolution the server holds, 247 KB a page here.

One thing is deliberately **not** claimed: whether Panels *blocks* on each
response. At 1.43 ms no request was ever still open when the next arrived, so a
blocking client and a UI-paced one are indistinguishable in this log. Measuring
it would need a deliberately slowed response, which is not worth building.

**M5a has landed.** The swap works: `Space` toggles between the current
edition and the last one, `1`..`9` jump directly, and a swap never writes
position — round-tripping through any number of editions is lossless by
construction, not by care. All three reading modes ship — single page,
two-page spread (right-page-first under `rtl`), continuous scroll — with a
joined spread drawn once, not as two halves, in every one of them. Landing on
a slot with no counterpart in the edition just swapped to reports a gap rather
than silently moving to the nearest slot, and offers the nearest paired slot
either side as a move the reader takes deliberately rather than one that
happens to it. A floating chrome shell — a filmstrip and a slot readout,
brought in and taken away together by `Tab` — never reflows the page
underneath it, and its tiles are served by a per-page thumbnail route measured
against the real corpus (O11, below). Reading position is restored on load
from the server and written back on `navigate` — debounced, so arrowing
through a run of pages is one `PUT`, not one per keypress — and never on
`swap`, which has nothing to record.

**What M5a leaves open.** There is no prefetch and no double buffer: a fast
flick between adjacent pages can still show a blank or half-decoded frame
before the next one paints, closed in M5b behind two `<img>` elements and
`img.decode()`. Continuous-scroll mode does not write reading position back
either: entering the mode scrolls to where you were, but hand-scrolling never
updates the stored slot, so a reader who reads by scrolling and closes the tab
loses their place — an `IntersectionObserver` is the fix, and it belongs to a
later milestone, same as prefetch.

**What M5b's decode gate leaves open, in two-page mode.** The gate is per
`PageView` instance, not per pair: an ordinary spread's two halves each run
`presentWhenDecoded` independently, so a turn can present one freshly-decoded
half beside one still-showing old half for as long as the slower of the two
takes to decode. That is not the defect the gate exists to close — neither
half is ever blank or half-painted on its own, which is the guarantee
`decode.ts` actually makes — but simultaneity across a pair is not something
this build guarantees, and no comment here should be read as claiming it is.
Left independent deliberately rather than gated together: gating would make
the faster half wait on the slower one on every ordinary turn, a real latency
cost paid on every pair for a mismatch nothing has measured a reader
noticing, particularly now that `App.tsx`'s separate swap and prefetch decode
queues (`swapQueue`/`prefetchQueue`) keep either half's decode from being
starved by prefetch traffic in the first place. A measurement showing the
mismatch is large enough to matter is the reason to revisit this, not a
guess made ahead of one — see `App.tsx`'s `renderGroup` for the same
reasoning kept beside the code it describes.

**The filmstrip's tiles do not reopen O9 — this paragraph previously said
they could, and that was wrong.** Each tile resolves its own thumbnail, which
on a cold strip is a burst of up to N requests for N *distinct* pages —
structurally the same shape as the cover burst O9 already measured and
declined to guard: one request per member, nothing to coalesce. O9's own
reopening condition is concurrent requests **for one key**, which this is not.
Nothing on this branch reopens O9; it stays open on the reasoning already
recorded there. The mitigation this paragraph also claimed for the burst
turned out weaker than stated, though **not as weak as this paragraph said
until O140 re-measured it**. It read "does not defer thumbnail requests at
all in the environment tested", which is true of the rail M5a measured and
false of a long one: at 390×844 on a **237-tile** rail, 101 of 237 thumbnails
were requested and the tiles past index 101 stayed 4 px wide and issued
nothing until scrolled to (`notes.md`, "O140: the chrome's build cost").
So `loading="lazy"` does bound a cold-strip burst, at about a hundred
requests rather than at the rail's length — **neither figure should be cited
again without saying which rail length and viewport it was taken at**. See
also `notes.md`, "M5a, measured directly: the reader's own evidence, not the
scratch ledger's".

**O14, not O9, is the item M5a actually gave a trigger to, and from a caller
nobody had named.** `web/src/nearest.ts`'s `nearestPaired` calls `display`
once per candidate slot either side of a gap, and each cross-edition `display`
is one `Binding::resolve` — up to `slotCount` route rebuilds, unmemoised, on
every render of the gap state. See O14's entry above.

**M5b has landed.** Every reading behaviour M5a shipped as fixed code is now
a `ReaderModule` against one declarative contract (`module.ts`): a `defaults`
object, an optional set of claimed keys, checked for a keybinding collision
or a duplicate id at import time rather than at runtime. Five modules exist
against it — `direction` (a per-edition reading-direction override, `d`,
layered over `work.reading` and never written back to it), `motion`
(animation on/off and transition duration, an empty style object rather than
an inline `transition: none` so a reader's own `prefers-reduced-motion` still
governs when animation is off), `regroup` (parity breaks a reader inserts by
hand, `b`, for the one case a joined spread cannot derive), `readouts` (the
four lines above, each independently switchable, with an unknown alignment
verdict reported as `verdict unknown` rather than silence that could be
mistaken for `sound`), and `prefetch` (a tunable window ahead and behind the
current slot, decoded off-screen through `decode.ts`'s double-buffered
`<img>` pair and swapped in only once `img.decode()` resolves — the fast
flick M5a left flickering). `Sidecar::chapter_at` in `taiyaku-core`, carried
across the `wasm-bindgen` boundary and into `Resolver`, feeds the chapter
readout; it names a member, not a chapter number, and the one chapter-
organised edition in the primary corpus (`en-fan`, reached from the hub by a
`fragmented` alignment) is marked `~` in the readout rather than shown as
settled fact, for the reason the twenty-two needles already established.
`routes.rs` now sends an `ETag` on every page response, keyed on a member's
`sha256` and its offset within that member, which is what let O1 be measured
rather than argued (below). `Binding` memoises its last-resolved route,
closing O14 (above).

**Task 11 closes the milestone: the settings panel, generated rather than
hand-written per module, and every module verified switching off on real
material.** `Settings.tsx` renders one section per registered module with a
control inferred from each `defaults` key's runtime type — boolean → check-
box, number → numeric input, string → text (a select is what the design
calls for once a module declares one with options; none does yet, and the
contract carries no field to declare options against, so that path is not
built ahead of a case that exists only in the design) — never branching on
which module or key it is looking at. A sixth registry entry, `settings`,
claims one key (`s`) for the panel itself the same way any module claims a
key, its own `defaults` empty, so the generated panel renders no section for
it. Two of the five modules hold a preference keyed per edition
(`direction`'s `override`, `regroup`'s `breaks`) rather than a flat value;
the panel has no way to know "edition" is what a given `Record` is keyed by
without being told per module, so these render a read-only entry count and
are cleared by the section's own Reset rather than edited field by field —
the module's own key (`d`, `b`) still edits one entry at a time, as before
this task. All five modules were then driven in a real browser against
`corpus/gotoubun-v01`: each one's behaviour stopped when switched off, took
effect when tuned, and survived a reload, with no remnant left on screen
when off. `prefetch`'s `maxConcurrent` needed the decode queues themselves
to change from a value fixed at mount to one rebuilt when the setting
changes — without that, tuning it in the panel would have been a control
that did nothing until the next reload, the exact kind of generated-but-
broken input this milestone was built to avoid. See `notes.md`, "M5b Task
11: the settings panel, generated, and all five modules measured switching
off", for the module-by-module table.

**A whole-branch review before this branch merged found "generated rather
than hand-written per module" true only of `Settings.tsx`, and fixed the gap
rather than merely narrowing the sentence.** `App.tsx`'s own preference
state — a `useState`/`loadPrefs` pair, a `savePrefs` effect, and a case in
each of `handleSettingChange`'s and `handleSettingReset`'s switches, per
module — was still hand-written as of the paragraph above, so a seventh
module (`fit`, `filters`, both still deferred) would have gotten a
normal-looking control in the generated panel that changed nothing until
someone also hand-wired those four places: the same "generated but broken"
failure already fixed once, above, for `prefetch.maxConcurrent`, moved one
layer up into code only a manual browser pass could catch. Fixed by making
that state one map built from `registry.all()`, keyed by module id
(`module.ts`'s `sliceFor`), and by moving the registry itself out of
`App.tsx` into `modules/registry.ts`, so registering a module no longer
touches `App.tsx` at all. Proved with a throwaway seventh module registered
only in `modules/registry.ts`: it appeared in the generated panel and its
preference persisted across a reload with zero lines changed in `App.tsx`,
then was removed. `modules/prefetch.ts`'s doc comment, which still described
toggling `enabled` by hand-editing storage — true before Task 11's panel
shipped, false since — was corrected too. All five real modules were
re-driven in a real browser after the refactor and still switch off, tune
live, and persist, matching the table above.

**What M5b did not reach.** The filmstrip's tile burst is still exactly
where M5a left it: `prefetch`'s decode cap governs `PageView`'s page
decodes through `swapQueue`/`prefetchQueue`, and `Filmstrip.tsx` never
receives either — it renders thumbnails as plain `<img loading="lazy">`, so
a cold strip still fires one request per distinct tile, and the
`loading="lazy"` finding above still stands, unaddressed, confirmed again
rather than assumed carried over. Continuous-scroll mode **still** does not
write reading position back — entering the mode restores where you were,
but hand-scrolling never updates the stored slot, unchanged from M5a and
needing the same `IntersectionObserver` that milestone already named. The
`fit` and `filters` modules the design specifies are not built; the contract
they would slot into is what M5b spent five modules proving out instead.

**M5d has landed.** A reader can now point at the right counterpart in the
filmstrip and have the fix written into `taiyaku.json` as `by: "hand"`,
surviving a re-solve and a second device writing at the same time. The
arithmetic is one pure function in `taiyaku-core` (`correct_runs`) and one
document operation built on it (`Sidecar::correct`), so no second
implementation of run-splitting exists to disagree with `compose`. The
browser sends the *assertion* — slot `a` of `from` pairs with slot `b` of
`to` — never a computed run list, so the split is applied once, server-side,
against the authoritative document: a deliberate departure from the design's
"the client computes a run split, `PATCH` to serve", made because a
client-computed list would put the 1:1 eviction rule in a second
implementation a future reader could get wrong, and would leave `If-Match` as
the only thing standing between a stale client and a wholesale replacement of
the edge. `taiyaku-serve` gained its first mutating route, `PATCH
/api/works/{work}/alignments`, guarded by `If-Match` against an `ETag` on the
sidecar, which forced the in-memory document to become replaceable
(`RwLock<Arc<Sidecar>>`) rather than loaded once at startup; `GET
/api/works/{work}` now carries an `ETag` too. O13 closes in the same branch
(above), because the correction UI is what first constructs a `Source::Hand`.
**Driving the UI against the real corpus found one defect the whole review
chain had missed.** A correction silently stripped every `margin` on the edge
it touched — `entries_from_runs` writes `margin: None` for every entry it
builds, having no confidence figure to hand them, and the correction sent the
whole edge through it. One correction took `raw-mono -> en-fan` from 8
margins to 0 while the untouched `raw-mono -> en-mono` kept all 3. Fixed: a
margin now carries forward only where both spans are identical, and the two
runs a correction actually reshapes lose theirs honestly rather than keeping
a stale figure. See `notes.md` for the full measurement. **The same pass
found that two hand corrections are enough to take a sound alignment to
`fragmented`**, opened as O23 above rather than answered on a guess.

**A defect that manual pass reported was not M5d's, and shipped on its own
commit after it: an arrow moved by slot where a group is what is drawn.** A
joined spread is two slots and one wide image, so stepping by slot redrew what
was already on screen and the press appeared to do nothing — *"I have to right
arrow twice to go by it"* — and in `two` mode a press advanced the pair by one,
redrawing half of it. `stepToGroup` (`web/src/pairing.ts`, pure, ten tests)
steps by group instead, measured on `en-mono`, which has spreads at pages 45,
46 and 49: pressing right from displayed slot 45 walks `45 → 46 → 47 → 49 →
51`, one image per press, symmetric backwards. **Landing on the group's first
slot fixes a second thing the report did not name** — both halves of a joined
spread draw the same image but `resolve_slot` answers them *differently*, so a
reader who could not see which half they stood on got one of two different
answers on a swap from a screen that looked identical either way. **What it
costs:** the second half of a joined spread is no longer reachable by arrow,
nor is its own swap answer; it stays addressable where addressing it is the
point — the correction filmstrip puts one tile per slot, and `two` mode draws
the halves in separate cells. Confirmed acceptable during the same manual
pass. *That cost is unchanged by O28, which closed the clamp this same commit
introduced — see its entry above; what changed is only what an arrow does where
there is no next group to land in.* The grouping reaches `applyIntent` as an optional parameter with the
slot arithmetic kept as the fallback, because how the page is grouped right
now — mode, direction override, hand-inserted parity breaks — is something
nothing below the component tree knows.

**M5c has landed, and with it the thing the sidecar was for.** A note written
on `raw-mono` is readable while reading `en-mono` or `en-fan` — resolved
through the same `Resolver.resolve` the swap makes, so no second
implementation of travel exists to disagree with the first. Notes live in
`<stem>.notes.json` beside the sidecar, the placement `progress` established,
written whole under `If-Match` through the atomic writer O2 introduced; the
`GET` synthesises an empty document where no file exists, so a client's *first*
write is an ordinary conditional write rather than a second precondition
spelling. A note is anchored to a **slot**, never a page, while its region —
when it has one — is fractions of the **page that slot is drawn on**, so a
re-scan or a different served resolution cannot move a marker. *That last figure
read "fractions of that slot" until 2026-08-08; the code has measured against
the drawn page since M5c, and the two differ only on a joined spread, whose two
slots share one wide image. `notes.ts`'s `Region` carries the correction and
what it costs L1.*

**What it deliberately does not do, stated rather than discovered.** A
region does **not** travel, so a travelled note draws a margin tab and never a
tint, because drawing the untransformed box would put it over the wrong panel.
*Until 2026-08-08 the reason given here was that transforming one between
editions is L1 geometry. L1 shipped; the transform exists and is measured, and
what is missing is drawing a region through it — deliberately out of that
milestone's scope, and cheaper now rather than free, since the transform is
shared and the drawing is not. See §5.* Two devices annotating
one work do not *merge* — the second is refused with the current validator and
re-applies, the same bargain `patch_alignments` makes, and merging would need
this server to decide what two conflicting edits to one note mean. Export is the stored
document rather than a rendering, so it can be put back.
**Markers work in all three reading modes**, including continuous scroll,
where each mounted group carries its own overlay and every slot's notes are
resolved once for the whole edition rather than once per page.

**Measured against the corpus rather than asserted.** Before any code, a
throwaway test resolved every slot of one edition into another and back — 193
of 193 on Go-Toubun v01 and 28 of 28 composed through a hub, with **no
disagreement in either direction** — which is what licensed resolving once per
edition rather than once per note, and so kept `Binding`'s route memo (O14)
useful. In the browser, a note on `raw-mono` slot 88 drew its travelled tab on
the page `taiyaku lookup` independently names, in both other editions; a
dragged region round-tripped to within 1e-5; a note on `raw-mono` slot 1 — a
real gap — was listed as unreachable rather than dropped; and a second writer
racing the reader was refused, re-read, and lost nothing. See `notes.md`,
"M5c: a note written on the raw, read on the English". **One defect turned up
in that pass and it was older than the branch**, now O25: no panel below the
gap branch was ever rendered, so the review list could not be opened from the
one state it was written for. The list was fixed there; the settings panel and
the chrome were not, and the item said why. *O25 has since closed — see its
entry above. This paragraph describes the state at the end of M5c, which is
what it is here to record.*

**Two more came from the owner opening the reader, and both are worth stating
because a nine-item browser pass had already passed over them.** Typing a note
fired every reader key — `isInteractiveTarget` knew about a button, a link and
an input, and nothing about a `textarea`, because until M5c the reader had
none — so `s` opened the settings panel mid-word, Space swapped edition and a
digit jumped to one. And drawing a region worked while showing nothing at all:
the gesture recorded correctly, but no rectangle was drawn during the drag, and
an unsaved draft is in no note list for `reveal` to tint. Both are fixed, and
both were missed for the same reason — the pass produced state the shortest
way (`fill()`, dispatched pointer events) rather than the way a person does, so
it confirmed the arithmetic and never the gesture. A third followed: the
overlay was positioned on the **page area** rather than on the page, so every
marker was offset by the letterbox around a portrait page — the drag
arithmetic had been right all along and the drawing wrong, which is why every
check that read the stored fractions back agreed with itself. Markers are now
placed by an overlay that measures the page it sits over, which also gave
continuous-scroll mode the markers it had never had. *That fix stopped one
layer short, and only `fit` could reveal it: the overlay measured the `<img>`
**element**, and until `upscaleToFit` shipped on 2026-08-08 the element and the
artwork inside it were the same box. §6 has the 376px that opened up between
them, and where the measurement moved to.* **All four share one
cause:** each was invisible to a check comparing this code's output to its own
other output, and obvious to one comparing it to what a person meets — the
pointer's coordinates, the image's rect, a real keystroke. See `notes.md`,
"M5c, reported from a real browser" and "The annotation overlay was placed on
the page area, not on the page".

What remains: the eighty-eight open items tabled above — **a large minority of
them opened by a review** rather than by building something, which is the
fact worth carrying. **The count that used to stand here is deleted rather
than corrected**, after three passes over one afternoon produced twenty-five,
thirty-one and twenty-three: the rule was never written down, every pass
chose a slightly different one, and each new number was as unsupported as the
last. Anyone who wants it can `grep` the last column of the open rows and say
which kinds of review they counted. Those rows were found by a review (O42 and O44 among
them — **O44 has since closed, on 2026-08-29, and is no longer one of them** — O96 from the *third* such pass over one branch, which is the most any
change here has had — **it has since closed, on 2026-09-06, and is no longer one
of the items this sentence is counting** — O45 through O53 all from the scanner branch's own whole-branch
review or from a fix round inside it — **O53 and O47 have since closed, on 2026-08-29 and 2026-09-14, and are no longer among them** — O57 and O58 from spec A2's fourth — **O57 has since closed, on 2026-10-12, and is no longer one of them** — O59 from the mutation workflow's first sharded run, O74 from spec C3's whole-branch one, O109 from the fix round of O100, filed five days after it surfaced, O124 from the whole-branch review of O76, O128 from O89's, O142 from the style pass ④ branch's own fix round 3, reviewing fix round 2, O184 from the live-works branch's re-review of its own whole-branch fix round, and O185 and O186 from the deployable branch's second re-review), and
O69 through O72 from spec C3's own
per-task reviews rather than from a whole-branch one — **O70 and O71 have since closed, both on 2026-09-14, and O72 on 2026-09-15, and are no longer among them** — and O67 opened
by driving a real OPDS client against a real library rather than found by a
review at all — a way of finding things this list had not had before — **and O95 was the second, from
driving C4 PR B's own rule editor against the real library after four review passes
and every gate had passed it; it has since closed, on 2026-09-05, and it is no longer
one of the items this sentence is counting** — and the
same run that closed O55, which spec A2 had opened on shipping for the
mirror-image reason: a screen promising something no client had confirmed. O66
opened on shipping too, by spec C2: a deliberately-taken shape named in its own §11 rather
than drift, for the reason `download.rs::generated` and `cbz::clipped` give in
its own entry above — *and its trigger has now been checked once, against Panels on 2026-08-25, with a log that does capture request headers: five acquisition downloads, no `Range` on any, so it did not fire. Narrowed rather than closed, and `docs/notes.md` records the three qualifications, chief among them that the session ran over a TLS proxy rather than the direct LAN connection the item names.* O54 opened alongside O55 the same way and both have now
closed; the count of review-found items still moves by one, and it
moves for the reason the rounds keep finding things: O57 is what a fix for one
finding left standing, named rather than left for a fifth round to rediscover
*(O57 has since closed — see its entry above)*.
O73 was found the same way this run finds things — the branch's own accept
path, driven over real archives rather than turned up by any review — and it
has since closed the same way it was opened: the same four archives, run all
the way through rather than only against a fixture.
Now that O1 (found by one,
closed by removing the round
trip its own entry was arguing about), O12 (carried rather than found), O14
(found by one, closed by M5b), O26 (found by one, closed on its own branch),
O28 (found by walking the reader through every position it can hold, closed on
its own branch), O29 (found by driving O26's own fix against the corpus, closed
the same way), O35, O36 and O37 (found by reading `web/src` closely enough to
write tests for it, and closed on that same branch), O38 (found by a probe
beside the survivors, closed on a branch of its own with the corpus drive its
entry asked for) and O39 (found beside the same survivors, closed by a
per-test `slow-timeout` and a job-level `timeout-minutes`, both sized from a
measurement), O34 (turned up by asking a diagnosis about the one case it
could not explain, closed by L2b Task 6 counting ink pixels) and O40 (found by
one, closed on a branch of its own by giving the gap's own scan the skipped set
every other path already had) and O43 (found by one, closed on the same branch
by giving alignments the `state::put_*` function the other three per-work
documents already had) are all out of the
count.
Two are the exceptions: O23, turned up by measuring what a new surface produced
against the corpus rather than by reviewing a branch, and O41, turned up by
hashing the pages a survey's own column described and finding the answer
depended on which archive the page had been distributed in.
**Every milestone M5 named has now shipped.**

*This paragraph said "the five open items" until 2026-08-08, having missed
O31, O32 and O33 when L2a landed them and O34 when the detector measurement
opened it — the exact drift the open-items ritual warns the count word is
prone to. It went to "twelve" when the coverage gate opened O35, O36 and O37,
and stayed at twelve after that same branch closed all three — the third time
the word has drifted, and this time it was caught by a reviewer counting the
table's rows against its own preamble rather than by the ritual.*

**Four of the eleven wait on evidence rather than on code**, which is worth
stating plainly now that the one item with a code-shaped exit has taken it:
O6 needs an art book with a legitimate reason to be in a sidecar,
O9 a client issuing concurrent requests for one key, O11 archives replaced
often enough for the cache to matter, and O23 a work corrected enough times
in real reading to say whether the verdict misleads. O24 is the odd one —
not evidence but a decision, about what a merge owes an edge it did not
solve, and no corpus document has yet had a `--direct` edge and a re-solve
in it.

**Two came from the mutation branch and are unrelated to each other**: O38, a
real defect in `unpaired_runs` that no test covered because the branch that
found it was a test-only branch, and O39, the absence of any bound on a test's
runtime. Both are recorded above.
*O38 has since closed — see its entry above. O39 has since closed too — see its entry above.*

**The remaining four are the detector's**, and they are held together rather
than picked off: O31 (the axis is per page where the design says per cluster),
O32 (the `median_area` disqualifier cannot fire), O33 (two constants reject
nothing in any test) and O34 (`MIN_INK` rejects nothing on the corpus either).
Each would sharpen an instrument that is measuring the wrong population, so
none is worth fixing before the detector is redesigned — and what that
redesign has to account for is now measured rather than assumed: see the two
mechanism corrections under "What is known to be wrong or missing".
*The redesign has since happened, and it closed one of the four: O34, by
counting ink pixels. The other three are still open and still held together —
O32's own fix is unwritten (the decision it was blocking was made under a rule
that never consults `median_area`, which narrows it without closing it), the gap
O33 names is in the tests, and O31's per-page axis was replaced by a two-axis
union on the L2b branch. **O31's entry has since been rewritten against that
change rather than left describing deleted code** (2026-08-12): all three of
its consequences are addressed, and it stays open on the residue the union
introduced — a block both smears produce identically has no axis to discover
and takes the dedupe tie-break's default, which is 13 of 69 blocks over the
90-page sample. See each entry above.* *O33 has since closed too, on
2026-09-15, by fixtures that reach both of its constants — see its entry above —
which leaves O31 and O32 the two still held together.*

**Three more were the reader's, and they are closed rather than open** — O35,
O36 and O37 were each a decision plus a few lines, and the branch that found
them made the decision and wrote the lines rather than filing them: `178f3e5`,
`4303dee` and `f4047b7`, plus `7cab6ec` for the second copy of O37's wrong
claim that fixing the first one turned up. They were the first items in a while
that could be closed in an afternoon, and they were. The argument for filing
them instead was a good one and is kept under "O35 to O37 came from the coverage
branch" above, together with what decided it the other way.

**The ninth is none of those categories** — O41, which is neither waiting on
evidence nor on a decision nor the detector's, and is the one open item with a
measurement already in hand and a named fix nobody has written: `describe_pages`
crops every page by a border derived from the archive it sits in, so the one
consumer that compares hashes *across* archives asks the same bytes six times
and gets four answers. It blocks nothing that has shipped, because the signal it
degrades is a diagnostic `credits.rs` is explicit about never letting decide —
but it bears on the credit-detection tier design, whose fallback corroborator is
that signal, which is why it is worth reading before that design is picked up
rather than after.
*O40 was a tenth until 2026-08-14 and is closed — see its entry above. It was
the last of the reader's, and it went the way O35 to O37 did: a decision plus a
few lines, on a branch of its own rather than in a file.*

### 4. Skipping scanlation credit pages — shipped

A reader setting that keeps donation pleas, credits cards and recruitment ads out
of the reading flow, and puts them in a section of their own that can be opened
deliberately. Not deleted and not hidden — a scanlation group's credit is the
thing it asked for in exchange for the work, and a reader that silently discarded
it would be taking a side. It is simply not part of reading the chapter.

**The evidence says taiyaku can already find them**, which is why this was on the
roadmap rather than in the "if it is worth it" pile. Measured on the primary
volume — see `notes.md`, "Scanlation credit pages are already visible in the
alignment" — **four** of the fan edition's seven chapter files each contribute
exactly one page that pairs with nothing and sits at the member's final index:
`c001`, `c003`, `c004` and `c005`, at pages 95, 168, 198 and 225. Three are the
same `lhtranslation` donation card; the fourth, page 225, is a **staff-credits
page** naming three people, not the donation card. *This paragraph said "three"
and "all three are the same donation card" until 2026-08-07, which is the exact
mislabelling `notes.md` corrected on 2026-08-05 — the branch fixed the working
log and left the living document standing wrong. Both now say four.*

**Long unpaired runs are usually genuine content the other edition lacks — but
not always, and a bad alignment is the exception.** On Go-Toubun the 42-page run
really is the prototype one-shot the raw volume has no chapter for. On
`kaguya-v01`, whose sidecar the solver itself rates `FRAGMENTED` (average run
13.3 slots, 23 runs), the runs at pages 105–111 and 113–118 are **ordinary
manga** — `c5.5` has fifteen pages and the solver paired exactly two of them, so
the runs either side are an artefact of the alignment rather than a fact about
the release. Length therefore says what it says only as far as the alignment
under it is sound. **Position is the part that survives**, and it is what the
rule actually tests: an isolated unpaired page at a member's *final* index. Every
sidecar already carries the alignment that shows it.

A second signal needs no alignment at all: the cards recur across sibling members
of one edition, so a near-duplicate by dhash finds them even in an edition with no
counterpart, which is the common case for a fan release. They are byte-different
per chapter, so exact hashing does not. **It shipped as `credits::recurrences`,
a diagnostic that classifies nothing** — measured on Go-Toubun, its two tightest
matches in the whole edition are the volume cover, and it misses both
staff-credit pages entirely, because each appears exactly once. It is printed
beside every row so a person can judge a candidate by it, and consulted by no
code; that module's own doc says why gating signal 1 on it would veto the two
true positives signal 1 uniquely finds.

**All four of the questions this section used to leave open are settled, and
three of them are settled in code.**

- **Where the classification lives.** In `taiyaku.json`, as a per-edition
  `credits` list of page indices — the way `spreads` is, and for the reason both
  had in common. Specified in `sidecar.md`; carried by `Edition::credits` in
  `taiyaku-core::sidecar`, and carried across a merge or dropped with a replaced
  spoke by `merge_spoke`.
- **Who records it.** Not `align`. A command of its own, `taiyaku credits`, which
  **proposes and prints and writes nothing without `--write`** — the discipline
  `inventory` and `duplicates` already set. The rule itself is
  `credits::credit_pages` over `taiyaku_core::unpaired_runs`.
- **What the toggle does to page numbering.** Nothing. **Skipping never
  renumbers**: a slot keeps its number whether or not it is skipped, so the
  readout jumps rather than counting differently. The slot is the addressed unit
  and `sidecar.md` is normative on that, so a per-reader toggle that changed what
  an address meant would make one device's progress and another's notes wrong.
  Same precedent as the swap: what is shown changes, what is written does not.
  Recorded on `web/src/modules/credits.ts`'s `creditsModule`, and carried out by
  `pairing.ts`'s `groups`, which is handed the skipped slots and leaves them out
  of the grouping — so the page area, the arrow keys and the filmstrip all skip
  together, and `Group.slots` still indexes the unfiltered table.
  *This said "carried out by `nextReadable`, which walks to the next unskipped
  slot" until 2026-08-07. That function was written three commits after an arrow
  stopped stepping by slot and started stepping by group (#12), so it could not
  have been wired without undoing that; and it governed arrows alone, leaving a
  skipped page in the filmstrip and in scroll mode. It is deleted. A first
  attempt filtered finished groups instead, which turned out to do nothing at
  all in two-page mode — see `notes.md`, "The credit-pages panel, and where
  skipping had to be decided", for the 122-tiles-either-way measurement that
  moved the decision inside `groups`.*
- **The false-positive rate against colour inserts and author afterwords.**
  Measured, and the answer is not the comfortable one: a colour insert *is* swept
  up. `kaguya-v01`'s one candidate is the publisher's own colour announcement
  page. See `notes.md`, "A second group's habits, and what the bound is worth".

**Precision is measured, and it is not perfect: four true positives and one false
positive across the corpus.** The four are Go-Toubun's pages 95, 168, 198 and 225;
the one is Kaguya's page 212. Both are pinned by gated corpus checks
(`taiyaku-cli/src/corpus.rs`), which assert the classified *set* rather than its
size — four of six is reachable by classifying the wrong four — and the Kaguya
check pins the false positive deliberately, so that a later improvement which
stops classifying 212 goes red and the expectation is updated rather than the
rule reverted. **Recall is worse than precision and is bounded above rather than
known:** `c000`'s page 43 is a credits page for a second group that the solver
*paired*, so no unpaired-based signal can reach it at all — four of five at best,
out of twelve pages anybody has looked at. `MAX_CREDIT_RUN` is **1, measured**
over 461 pages and four scanlation groups, and one of those four groups prints no
credit page at all. Two volumes are not the population.

*Ten volumes are a better one, and the re-measurement on 2026-08-13 changed both
halves of this paragraph. Recall is 13 of 15 over the swept subset and precision
21 of 23 overall; the paired-page ceiling is no longer a single case, since
`kobayashi-v03` pages 91 and 152 are the second and third; and `MAX_CREDIT_RUN`
now has the counterexample its doc says does not exist. Kept rather than
rewritten — the reasoning was sound on the evidence it had.*

**The reader's half has now shipped too, and both toggles govern something.**
`C` opens a panel listing the viewed edition's credit pages — a thumbnail and a
page number each, and no labels, because the sidecar records page indices and
which one is a donation card is something a person establishes by looking. An
edition with none says so in words. Taking an entry goes there whether or not
`skip` is on: nothing here is meant to become unreachable, and the arrow that
follows steps off again.

**`skip` is honoured inside `groups` rather than by filtering its result**, and
that was not the first design. Filtering finished groups can only drop a group
whose every slot is skipped, and measured on this volume that is none of them —
each of the four credit pages sits at a chapter's end and so pairs with an
ordinary page, giving 122 tiles in two-page mode with the setting on and 122
with it off. Deciding while pairing lets a skipped slot's neighbour pair
forward instead. See `notes.md`, "The credit-pages panel, and where skipping had
to be decided".

`creditSlots` is gone with it, replaced by `creditSlotsIn`, which filters the
slot table `allSlots` already returns rather than re-deriving slot expansion in
TypeScript — one derivation, read through the `Resolver` from
`taiyaku_core::slot::expand`, which is what its own doc comment asked for. The
`Pagination` it could not be given no longer exists.

### 5. L1 geometry — shipped, and what shipped was in neither candidate list

Promoted out of "Later" on 2026-08-08 rather than reached in order, which is why
a list that opens "all four" has a fifth entry.

**What it is.** Free continuous zoom and pan in single-page and two-page mode —
wheel or pinch anchored at the pointer, a drag to pan — a page turn that carries
the view onto the next group and re-clamps it, and a swap that carries the
*visible content*: the rect the reader is looking at, named against the other
edition's page and then fitted around. Zoom is a `ReaderModule` like the other
eight, so its four preferences generate their own settings section with no
hand-wiring and its three keys are checked against the live registry at startup.

**A drag pans from the grey around the page as well as from the page.** That
took two goes. The first rule was "the press is on the drawn `<img>`", which
was the narrowest thing true of the page and false of a control — and it made
the letterbox dead to a drag, which a reader trying it reported straight away.
The comment defending it had *stated* that cost and judged it acceptable on the
grounds that the letterbox shrinks as the zoom grows; what that missed is that
a reader reaches for the nearest grey to drag from, and being ignored there
reads as the pan being broken rather than as a boundary. It now excludes
controls instead of requiring the page, via `keys.ts`'s `isInteractivePress` —
the ancestor-aware sibling of the `isInteractiveTarget` the keydown handler
uses, and the ancestry is the point: a press on a note-list entry lands on the
*text*, so the keydown rule answers `false` for exactly the case the image test
existed to catch. Confirmed in the browser both ways: a drag from the grey pans,
a drag begun on a note's margin tab does not.

**Verified in a browser against the corpus**, on sidecar copies with absolute
member paths so nothing written landed in `corpus/`: a wheel zoom holds the
point under the pointer to zero drift and the scale is uniform at every
magnification sampled; a swap onto a joined spread lands on the correct half
(`raw-mono` 5 → `en-mono` 4 at page fraction x 0.526–0.974, which `taiyaku
lookup` independently calls "4 right half"); two-page mode carries a viewport
straddling both cells; an annotation region stays on its art through a zoom and
a pan; and scroll mode draws no transform at all. What no test here touches is
trackpad *feel* — `PER_NOTCH.pinch` and the notch caps are guesses, and a
synthetic wheel event cannot say whether `{ passive: false }` defeats a real
trackpad's chrome-zoom.

**Fit beat fill, and it is the swap's whole promise.** Filling the viewport
crops a sliver that was visible a moment ago, and a swap that hides something
the reader was looking at is the failure that makes the feature untrustworthy.
Fitting shows a little extra instead — the same conservative direction the
composition loss takes, where a reader sees "no counterpart" rather than a wrong
page. Magnification is deliberately *not* the invariant: two editions are
different physical printings at different scan resolutions, so "the same zoom
factor" is a statement about pixels rather than about art.

**Carrying the view across a page turn beat two alternatives.** Panning to the
reading-order start is the conventional reader behaviour and is wrong here for
the same reason the swap preserves content — someone zoomed into a corner is
working through something, and moving them is a decision made on their behalf.
Resetting on every turn has no hidden state and makes a reader with small text
re-zoom on every page, at a cost borne entirely by the person the feature exists
for.

**Nothing in the sidecar, the aligner or the server changed**, and that was the
scope decision rather than a happy accident. The transform is derived at read
time from the two drawn images' natural dimensions; `sidecar.md`'s
forward-compatibility rule already permits adding a stored one later without a
version bump, so the door stays open at no cost — which is the reason not to
have walked through it on no evidence, the same ground the gatefold question is
deferred on.

**The measurement went against the design, and that is the headline.** The
design named two candidate transforms and a baseline, and fixed the promotion
rule before any number existed so that the result decided the cost rather than
the cost deciding how the result reads. `crates/taiyaku-cli/src/geometry_experiment.rs`
then sampled 288 paired slots — 24 each from twelve corpus volumes across four
series, `raw-mono` against `en-mono`, the one edition pair all twelve carry —
and for each ran a coarse-to-fine grid search for the best rect-to-rect map,
scored with the solver's own `similarity` over `fingerprint` rather than a
metric invented for the question, so the numbers are in the same currency as
every other figure in `notes.md`. Reported over the 284 whose search settled;
the four dropped ended the coarse pass against the wall of its own range, which
is the search saying it ran out of room rather than that it found a minimum.
The outcome:

- **`pillarbox` won, and neither the design nor the plan had named it** — the
  experiment is where it came from, because nothing had looked yet. It takes
  the **min** of the two aspects as the shared content's where `letterbox`
  takes the max — one token in `mapRect`. Median residual 0.0195 of page width
  and 0.0041 of page height, p90 0.0276 and 0.0116; it beats `identity` on the
  horizontal in 10 volumes of 12, and the two it does not are the two whose
  aspects nearly match already, where there is almost nothing for either rule
  to correct.
- **`identity` finished second**, 0.0296 / 0.0041 — exactly right on the
  vertical, uncorrected on the horizontal.
- **`letterbox`, which the design named and which `mapRect` carried from the
  task that built it until the measurement replaced it, finished third and lost
  to doing nothing.** 0.0296 / 0.0162: it never rescales
  the horizontal, so it inherits `identity`'s horizontal residual whole, and
  then adds a vertical error four times `identity`'s at the median on an axis
  the pixels wanted left alone. A candidate that can only add error to one axis
  and correct nothing on the other is not a candidate.
- **`reset` loses to all three.** It is a bound rather than a residual — the
  reader sees the whole page and re-finds the balloon, so up to half of one —
  and it does not need measuring while every candidate above is inside 5% of a
  page.

**The horizontal finding could have been an artefact, and two runs exclude it
between them.** `fingerprint` resizes every crop to one fixed grid, so a search
can buy similarity by squaring two renderings up rather than by locating
artwork, and the finding — the best map's horizontal scale tracking `aFrom/aTo`
in twelve volumes of twelve — is exactly what that would look like. A control
ran the same search against a resample of the *source* page, where the aspect
difference is fully present and the true answer is known to be identity: it came
back `sx/sy = 1.0000` in all twelve, at a residual of 0.0011. And the same
search run backwards over the same slots inverts, in twelve of twelve — the two
directions' horizontal scales multiply to a median 0.9941, against the 1.0842
they would give if the scale belonged to the metric rather than to the pages.
Neither run alone would do it: the control excludes the direction-antisymmetric
artefact and the reverse run excludes the direction-blind one.

**The threshold this was judged against is still a guess, and says so.** What
fraction of a page a panel occupies has never been measured and this experiment
did not measure it either; the working figure was that 5% of a 1200px page is
about 60px, a fraction of a balloon rather than a panel. `pillarbox`'s p90 of
2.8% of page width sits at half that and its vertical p90 of 1.2% at a quarter.

**The spread half was a scope gap, and it was closed on 2026-08-08.** Where one
edition joins a spread and the other splits it, a swap now lands on the half the
reader's slot names. Both halves of a joined spread are drawn as one wide image
and share one coordinate space (`notes.ts`'s `Region`), so getting a reader to
the correct half is the transform's job rather than something the slot model
absorbs — which is what the design's §1 said and what the implementation did not
do. The fix is two arguments and no new arithmetic: `viewport.ts`'s `Drawn`
carries a `half`, `mapView` takes the rect out of that half's box on the way in
and puts it back on the way out, and `mapRect` is handed the half's own frame
rather than the whole spread's, since it reconciles two *aspects* and a joined
spread's is twice its half's. Where the half sits in the composition is forced
rather than chosen — a half is half of the *image*, and `paintedIn`'s letterbox
is where image fractions begin, so a half applied before it would be half of the
cell instead. A slot with no half passes through both new steps as a
floating-point identity rather than a near one, which is why they are
unconditional and why every `mapView` test written before them holds unchanged.

*Until that date this paragraph read "what it does not do, stated plainly
because a reader would hit it". `mapRect` was handed the two drawn images and
nothing about halves, so it read the spread's extra width as margin at both
edges and mapped its central band onto the single page — the fold, rather than
either of the pages the other edition prints. The finding was **read off
`mapView`'s composition rather than driven**: the by-hand pass crossed a joined
spread with an *arrow*, which is a turn within one edition and unaffected, and
did not swap between an edition that joins one and an edition that splits it.
That is the part worth keeping now the defect is gone. It was reachable on the
primary corpus — `en-mono` joins Go-Toubun v01's page 4 where the raw prints two
pages — and a pass with hands on the keys still did not meet it, because meeting
it needed one particular key pressed in one particular place.*

**Two smaller exposures remain, and both are measured rather than guessed.**
`pillarbox` leaves 2.0% of page width at the median, part of which is a per-page
horizontal offset that moves by up to 0.083 within a single volume and which
nothing derived from two frames can correct; and the vertical residual is 0.4%
at the median against a control floor of 0.1%, uncorrected by every candidate
here.

**Travelled note regions are still not drawn**, and the reason has changed
rather than gone: the transform they needed now exists, so what remains is
drawing through it. L1's design put that out of scope by name to keep the
milestone one shippable thing.

### 6. Fit, the seam, and the keybind help — shipped, and two of its designs were wrong

Three unrelated reader complaints raised after L1 shipped, grouped onto one
branch because each is small and none touches the alignment. They are not one
feature and this entry does not pretend otherwise.

**What it is.** A `fit` `ReaderModule` — `upscaleToFit` (on), `upscaleFilter`
and `gap` — so a page fills its box instead of sitting at natural size in a
large letterbox; a two-page seam the artwork actually meets across, with the
note tabs moved out of the way of it; and a `help` module whose overlay lists
every key the reader answers, generated from `keys.ts`'s one ordered
`READER_KEYS` table and from the registry. Nothing in the sidecar, the aligner
or the server changed, and no dependency was added.

**The default reverses the M5 design, deliberately, and that document is
corrected in place rather than rewritten.** It set `upscaleToFit` off and
argued it well: enlarging a low-resolution scan costs something real, so the
reader should ask for it. Measured in a browser at a 1400×1263 page area on
`corpus/gotoubun-v01`, `en-mono` (2147×3056) drew at 887×1263 and filled the
height while `raw-mono` (685×1024) drew at 685×1024, leaving 239px unused — and
the first reader to meet that reported it as a defect in the images or the
aligner, not as a setting they had not found. A default that makes a working
reader look broken is the wrong default however sound the reasoning behind it,
so the default reversed and the reasoning stayed. The enlargement is 23% of
inked line work, which is why `upscaleFilter` shipped in the same commit rather
than later. Neither the aspect difference (raw 0.6689 against 0.7026, about 5%)
nor raw-versus-English explains the complaint — `en-fan` at 1114×1600 does the
same thing on a tall enough display — and both were checked before anything was
built.

**The seam's design closed the gap on the wrong element, and only a measurement
said so.** The design had each cell justify its page toward the seam. Built
exactly as specified it moved nothing: the two `<img>` *elements* already
touched at 0px while their artwork sat **359px apart**, because `upscaleToFit`
had just made each element fill its whole cell — leaving `justify-content`
nothing to distribute — and `object-fit: contain` then centres the art inside
that element. The fix is `object-position` on the image, which took the 359px to
**0px**, measured the same way; the cell keeps only the padding that splits
`gap`. **No test could have caught the dead version.** `justify-content` on a
box with a single full-width child is silently inert, so an assertion can only
see that the declaration is present, which is exactly what the assertion in
`fit.test.ts` did.

**This branch also introduced a regression and then fixed it, which is worth
the record.** Making the element larger than the artwork inside it broke every
annotation marker, because `Annotations.tsx` measured the element: in single
mode a 1385×475 element holding 334×475 of artwork, so a region tint that
belonged at x 842 was drawn at 466 — out by 376px, on the default settings, from
the moment `fit` shipped. `notes.ts`'s `paintedBox` was wrong the subtler way:
it had been an identity for the whole of its life (with `auto` sizing, box and
content coincide), and when the distinction finally arrived it split the
letterbox evenly — right for a centred page, wrong for a page against the seam.
In two-page mode it answered 179 and 872 across the page area where the artwork
was at 359 and 693. Both now measure the artwork, through one answer in
`App.tsx` that the drawn pages, the marker overlay and the drag arithmetic all
ask.

***And a fourth consumer did not ask.*** This paragraph closed "so what is
drawn and what measures it cannot disagree" until 2026-08-08, when a review of
the merged branch found `viewport.ts`'s `paintedIn` — which its own doc calls
the fractional twin of `paintedBox` — had never been given the hug. `mapView`
calls it for both sides of a swap, so a zoomed swap in two-page mode measured
both pages as centred while both were hugged to the seam. Fixed on the branch
after; `Drawn` carries the hug now. Single and scroll mode were never affected,
because their hug is `centre` and the two answers coincide there — which is
precisely why three rounds of review and a browser pass all missed it.

**The tab move is the M5 design's own guarantee going out from under it.** That
document wrote "fit-whole guarantees the side margins the tabs live in on every
portrait page", which was true of a centred page and stopped being true of a
seam-aligned one. In `two` mode a tab now sits in the margin *away* from the
seam; single and scroll mode are unchanged, and the default is the old
behaviour. Letting the tabs overlap was cheapest and is the one thing this
codebase must not do — a tab drawn over the facing page's art asserts a note is
on a page it is not on.

**The help is generated, which is the whole of its design.** `Space`, the
arrows, `Tab`, `c` and `1`–`9` used to live in `intentOf` as bare `if`
statements that nothing could enumerate, so a written help screen would have
been a second copy of a rule — and this project has repeatedly spent review rounds on
comments that were true when written and false later. One ordered table is now
dispatched from and listed from. `KeyBinding` gained an optional `label` for it,
which is a widening of the module contract and was reported rather than done
quietly, per `module.ts`'s own rule.

**Verified in a browser against the corpus**, on a scratch sidecar with absolute
member paths so nothing landed in the gitignored `corpus/`: the artwork seam at
0px in two-page mode; a region tint at x 842 where 842 was expected in single
mode, against 466 before the fix, and 663 where 663 was expected in two-page; a
note tab at x 552–570 with the facing page's artwork starting at 574; `?` and
`h` both opening the overlay and `Escape` closing it; and the ZOOM section
disappearing from the list with `zoom.enabled` off. See `notes.md`, "Fit, the
seam, and the keybind help".

**What is not built**, and why it is a deferral rather than an omission: `trim`
and `fillWidth`/`fillHeight`, which the M5 design lists in the same module.
Nobody asked for them, and `trim`'s case — Tongari v09's letterboxed scans — has
no reader waiting on it. The reasoning is kept in the M5 design for whoever hits
it.

**One known hole, stated because a reader could meet it.** A module is listed in
the overlay unless it declares `enabled: false`, and `credits` gates its key on
a preference called `panel` instead. Turning that off leaves `C` listed while
the key does nothing. It is the only module in today's registry with an off
switch under another name; the fix if it grows is a field on the contract, not a
list of per-module exceptions inside the help module, which is the copy of a
rule that file exists to avoid.

### 7. Reading direction, and what it did not reach — shipped

Reported as "the reading direction doesn't seem to work". It worked, and did
less than its name claims: it decided which slot of a pair was drawn on which
side, and nothing else.

**Two things it did not reach.** The filmstrip ran left-to-right under a
right-to-left work — measured on `gotoubun-v01`, whose raw is `rtl`: tiles ran
page 1 at x 26 through page 10 at x 939, *unchanged* when the direction was
flipped, so a reader hunting a page had to read the strip backwards from the
pages beside it. And the right arrow always advanced, so in a right-to-left
work it moved away from the page about to be read.

**One rule, three consumers.** `stepFor` in `modules/direction.ts` answers
which way a physical arrow turns the page. `App.tsx` maps the arrow intent
through it, `Filmstrip` lays its rail out with `row-reverse` from the same
answer, and `helpGroups` asks it which arrow advances so the overlay stops
telling a manga reader that `→` is the next page. Deriving the help rather
than restating it is the point: three places each deciding for themselves is
how they came to disagree.

`row-reverse` rather than reversing the tile array, deliberately. `stepToGroup`,
the current-tile border and the correction picker all address tiles **by
index**, and reversing the data would have made every one of them wrong in a
way no type could catch.

**Verified in a browser** on `gotoubun-v01` with the raw set to `rtl`: `→` went
page 2 → 1 and two `←` went 1 → 3; the strip came back `row-reverse` with tiles
descending left to right and the current tile still scrolled into view; the
overlay read `→ previous page` and `← next page`.

**The mode is remembered too**, which is unrelated except in being a thing the
reader forgot. `mode` was a `useState`, reset to single page on every load.
`layoutModule` holds it now, and registration is the whole fix — `App.tsx`
builds its preference state from `registry.all()` and writes every slice
through `prefs.ts`, so loading, saving and a settings control all follow.

It is a module rather than a direct `loadPrefs('mode', …)` call, which was
shorter and would have been the first value this reader stored without being a
module — and so the first stored key nobody thought to namespace. Everything
in `localStorage` was already under `taiyaku.prefs.*`; what was missing was
anything stopping the next value from not being, and `prefs.ts` now states that
rule where the keys are built.

### 8. Two-page mode: halves that arrive together, and a pairing that agrees across editions — shipped

Two complaints from real reading, both about `two` mode, and both the same kind
of defect: a rule that was right about one page and had never been asked about
two. `notes.md`, "Two pages that arrive together, and a pairing that agrees
across editions", is the record.

**The halves presented one at a time, and the code said so.** `decode.ts`'s gate
is per image, so each cell showed its page the moment its own decode settled.
`App.tsx` carried a comment stating exactly that, arguing for leaving it — "a
mismatch nothing has measured a reader noticing" — and naming a measurement as
the thing that should overturn it. **A reader noticed, and the measurement
agreed with them**: with a warm cache on the production bundle, a median 89.7 ms
between the two cells at a slow reading pace, 158.4 ms at worst, and **every one
of eight turns tearing by more than a frame**; flicking took the median to
213.8 ms and the worst case to 1.28 s. `cohort.ts` is a barrier that holds the
first decode until the other settles, with the decode queue nested *inside* it
so the concurrency cap still governs the decodes and the barrier governs only
when they are shown. **Membership closes on a tick rather than a count**, since
nothing knows in advance how many images a turn will change; a turn that changes
one cell forms a cohort of one. Re-measured the same way: **0.0 ms, on every
turn**.

**The "off by 1" between editions was not the aligner.** That was checked first
and comes back clean: a shift test over every paired slot of 12 alignments
across 9 volumes and 4 series — score the pairing the solver chose against the
same slot's score against `to ± 1` — finds **zero improvable pairings**. Its
first version reported four, all at volume tails with large margins, and every
one dissolved on adding the one-to-one constraint the scan itself had been
missing: several near-identical end pages match one `to` page, the solver gives
it to the highest scorer, and the loser then looks improvable against a slot
already taken.

**It was `pairing.ts`.** `groups` anchors `two` mode's pairing at slot 0 of
whichever edition it is handed, and nothing carried that across a swap — so the
same reading position drew `(n-1, n)` in one edition and `(n, n+1)` in the next,
one page right and the other its neighbour. Over every ordered pair of
`gotoubun-v01`'s three editions, **206 of 580 swaps kept the pair intact**, and
between the two English editions — the pair the complaint names — roughly seven
groups in ten were torn. Intermittently, which is why it read as "some pages
pretty early on" rather than a constant offset. `alignedPairs.ts` derives a
grouping from the alignment and feeds it in as more of `groups`' existing
`breaks`, so the grouping itself needed no new mechanism. **The hub and not the
authority** as the space it is derived in, because `authority` follows wherever
the reader last navigated and would re-group every other edition when they
turned a page.

**The first version of that rule shipped, and the reader broke it in one press
— which is the part worth keeping.** It derived the *spokes* from the hub and
left the hub on its own parity: 572 of 598, 96%. Then "there's still some
mis-alignments, like slot 48, p46". A joined spread is one image, so `groups`
gives it its own group and no break can halve it — and `en-mono` joins pages 45
and 46 back to back where the raw carries four ordinary pages. Their
counterparts are raw slots 47–48 and 49–50, but raw's untouched parity pairs
`(46,47)` and `(48,49)`, so **no raw group was the counterpart of that spread**
and the swap drew one page out either side of it. A rule with a fixed point that
cannot move is what left the 4%.

**Pair boundaries are a property of the work, not of an edition.** Wherever
*any* edition joins a spread, that is a boundary the work has; those are mapped
into the hub's slot space, unioned, and every edition — the hub included — takes
its pairing from the one grouping that results. Measured across eleven volumes
in four series it is **never worse than the spokes-only rule and strictly better
on five**: `gotoubun-v01` 96% → **99%**, `gotoubun-v08` 93% → **100%**,
`kobayashi-v08` 99% → **100%**, and 581 of 590 on the three-edition document the
complaint came from.

**A third complaint came out of the same corner, and two of its obvious causes
were wrong.** "Switching pages sometimes seems slow": median 110 ms from
keypress to both halves. `modules/prefetch.ts` reached `ahead` *slots* where an
arrow steps by *groups*, so in `two` mode the next group's second half was never
prefetched — measured on the wire as exactly one page requested after the
keypress on every forward turn. That is the same missing fetch as the tearing
above; gating the pair had turned it from a tear into a wait. `groupWindow`
reaches whole groups: **110 ms → 14 ms**. Widening the window then reintroduced
a tail, and the reason is the interesting part — a decoded page is ~26 MB, a
joined spread twice that, so a wide window evicts the pages the reader is
arriving at. **Prefetch was competing with itself**, confirmed from the other
side by raising `maxConcurrent`, which made it worse. The defaults are now one
group each way: **median 4 ms**. What survives is a tail of a few turns in
twenty at 100–300 ms, when the browser has evicted a bitmap.

**Two dead ends are recorded rather than dropped.** Retaining the prefetched
images does not retain their bitmaps — 72.5 ms with no references held against
160.4 ms holding twenty-six, since Chrome discards the raster for a detached
image either way. And progressive JPEG is the genuine decode cost — `en-mono`
ships progressive at 76.7 ms against 26.4 ms for the identical image as
baseline, where downscaling to a quarter of the pixels saves only 6 ms — but a
lossless transform was built against `turbojpeg` and **backed out**: it costs a
median 86 ms in process, more than the 50 ms it saves, so it needs a cache and a
cold-path policy or the first read of every page gets slower; it needs
`libturbojpeg` in CI; and Komga, the comparison that prompted it, passes JPEG
through untouched and pays the same decode.

**What it costs is a page drawn alone, and it is not hidden** — but it costs
less than the rule it replaced, because the hub can now move rather than every
spoke bending around it. Each position where the parity genuinely has to change
costs one solo group: 4 added to `en-mono`'s 104 on `gotoubun-v01` where the
first rule needed 14, 2 to `en-fan`'s where it needed 12, and `gotoubun-v08`'s
`en-mono` 16 → 2. A reader who never swaps pays that too. Taken deliberately: a
solo page says there is nothing here to pair this with, where a silently
mismatched pair said nothing at all.

### Later

- **The loading work, in two PRs. Both have landed**: the first on
  `loading-reads` (#210), whose spec `2026-10-27-loading-reads-design.md`
  is the record, its §9 the calls made while building it and its known
  limits; the second on `loading-library`, whose spec
  `2026-10-28-loading-library-design.md` is the record, its §10 the same.
  This entry now says what each shipped. The user saw on 2026-10-26 that the
  sections of a Configuration page and of `/account` land at different
  times, and asked whether anything is kept between pages. They ruled the
  same day that the work is two PRs of its own after PR 3, and that the
  build label's PR comes before them. **The first** puts an in-memory read
  store (`reads.ts`) and one page hook (`usePage.ts`) under Libraries, a
  library's page, Users, an account's page, Backup, Rules, `/account` and
  the reader's account panel, About, and the sidebar's build label and
  library status. A first visit draws the header at once and the body in
  one step, once every read has answered or failed, with "Loading…" in the
  body's place once the wait has held 200 ms (`PageLoading`), and none in
  the reader's account panel. A return draws the kept answers at once and
  reads them again, though an answer a write has made stale is not drawn on
  arrival. Answers are kept for this tab only, and emptied by signing in or
  out (`Gate.tsx`). Each write marks stale every read in which it changes
  something a screen draws, on its own screen or another's, but for the
  last seen of the caller's session, which every request moves; a
  directory's own scan marks its library's scan status without drawing it
  again (the spec's §9, call 9). That replaces `StatusRefreshContext`
  and `useStatusRefresh`, which are gone. The build label, About and
  `/account` share one read of `/api/about`, so `about.ts` holds no answer
  and `forgetAbout` is gone. A library's page
  waits for its scan status as well as the list, so a running scan is drawn
  in its first body, and the licence page, which reads nothing, keeps its
  texts once loaded, so a return draws them at once. It fixes two
  first-load defects: Rules no longer draws its empty states, "No preview yet…" among
  them, before its reads land, and an account's page no longer says "The
  libraries list has not been read" while that list is being read; nor does
  `/account` draw a list empty while it is read. It narrows O222. **The
  second** puts the Library view, a series page and search on the store
  (`pageReads.ts`'s `gridRead`, `homeRead`, `seriesRead` and
  `searchRead`), so Back to any of them draws the rows left behind in its
  first render, then reads them again quietly and updates in one step. The
  store keeps at most 50 answers that nothing draws (`IDLE_KEPT`), and
  marks stale by path prefix (`invalidatePrefix`). The grid's, home's,
  search's and a series' answers carry each cover's address with a version
  of its bytes in `?v=`, which the thumbnail route answers, while the
  version matches, with `COVER_CACHE_CONTROL`, so the browser keeps the
  cover for good and a return asks for none; the web no longer uses the
  series cover `307` (`docs/notes.md`, "The cover version"). The chip and the sort are
  `?show=` and `?sort=` in the address, and a series page's edition
  `?edition=`. A scroll keeper (`scrollKeeper.ts`) restores the scroll on
  Back and Forward, and the reader's Library button opens the last Library
  view seen in this tab at its scroll. A scan's end, a Rules save, a
  library's removal, a directory's removal or move and a work's title edit
  mark the views stale. **Reading progress is the second exception**, after
  the last seen above, to each write marking stale what it changes: a
  progress save marks nothing, a return after reading draws the kept rows,
  and their re-read waits for the reader's last save to answer, so the
  Continue strip moves once.
  It closes O224, and its live check opened O230. On 2026-10-28 the user
  ruled that the web tests' move out of `web/src`, into a `web/tests/`
  tree of their own (#211), comes before the second loading PR.

- **The comments clean-up. The pilot, `taiyaku-core`, `taiyaku-read` and
  `taiyaku-wasm`, is done**; see
  `docs/superpowers/plans/2026-10-29-comment-pass-pilot.md`. The user ruled on
  2026-10-29 how the rest of the repository goes: a full pass in which Sonnet
  cuts, Opus reviews every block blind, then one fix round and a pointer pass.

- **Measure argon2's cost on the NAS, and set the semaphore cap O57 wants.**
  §14 recorded that OWASP's parameters — `M_COST_KIB` 19456, `T_COST` 2,
  `P_COST` 1 — are a starting point that has never been measured on the host
  that will run this, and that unknown has now blocked something. **O57's fix is
  a semaphore capping concurrent argon2 verifies, and its cap is a number this
  measurement produces.** Left as a dependency inside O57 until now, which is
  how an item stays open forever: the thing it waits on was named in prose and
  tracked nowhere.

  *O57 closed on 2026-10-12 without this measurement. Its cap is four by the
  user's ruling of the day before: what the cap bounds is memory, which does
  not depend on how fast the NAS is. Of the two answers below this entry
  still owes the first, whether the parameters hold. The second was settled
  another way, by measuring what the server holds after a burst (see O57's
  row). The reporting test also prints a verify in kept memory, which is what
  the server runs since then.*

  The measurement itself is one command —
  `cargo test --release -p taiyaku-library credential::tests::report -- --ignored --nocapture`,
  which exists for this and prints the mean of ten hashes and ten verifies. The
  workstation figure is 11.9 ms hashing, 10.9 ms verifying (2026-08-18). Two
  answers follow from the NAS figure rather than one: whether the parameters
  hold, and how many verifies may run at once before 19 MiB each becomes a
  problem on a machine chosen for being small. If the parameters go **up**, note
  that `credential::tests::the_stored_hash_carries_this_modules_parameters` is
  what then stops `hasher() -> Argon2::default()` being an equivalent mutant —
  `docs/mutants-baseline.md` says so in its entry, and that entry becomes wrong
  on the same day.

- **`taiyaku-cli` has not been swept since its exclusions landed, and deciding
  when to sweep anything is now a judgement rather than a cron.**
  `mutants.yml` ran six times on 2026-08-20 and every one of the first five
  failed — glibc, the runner's own ceiling, zero-based shards, a config file at
  the wrong path, an unbounded process count — each hidden behind the one before
  it, none findable by reading the file. The sixth was green. That is the record
  of what dispatching it by hand bought, and it is the argument for doing so
  before trusting any workflow that has never executed.

  What is left is two things. **`taiyaku-cli` was stopped partway through the
  re-sweep**, so its 84 survivors are the 2026-08-19 figures taken before
  `detect.rs` and `regions.rs` were excluded; `docs/mutants-baseline.md`'s run
  log says so rather than letting them read as current. Sweeping it is one
  dispatch of `taiyaku-cli`, five shards, and the log wants a row afterwards.

  **And there is no schedule**, deliberately — the forge's host is an 8-core
  machine at a load average around 16 carrying 68 containers, and a sweep is
  hours of build-and-test. So nothing will tell you a sweep is due except that
  run log. Read it when a crate has had real work done to it, or when something
  makes the suite suspect. A long gap there is the signal the cron would have
  been standing in for, and the failure mode this entry exists to name is that
  nobody ever looks.

- **The page cache used to follow the working directory. Shipped 2026-08-11**
  on `serve-cache-root`; `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md`
  is the record, and this entry is kept as the defect that opened it.
  `ServeState`'s `cache_dir` defaulted to the *relative*
  `.taiyaku-cache/covers`, so where a cache landed depended on where `taiyaku
  serve` was launched from. That was cheap while the cache held covers and
  tiles; since `?w=` it holds every reading-size page, so starting the server
  from a different directory silently produced a second cache and regenerated a
  whole volume at ~126 ms a page. Found by noticing 120 MB of pages under
  `web/` after a session that had launched `serve` from there. Gitignored, so
  nothing could be committed, and harmless beyond the wasted work.
  It now resolves to `$XDG_CACHE_HOME/taiyaku`, falling back to
  `$HOME/.cache/taiyaku` and then — printing a line that says so — to the old
  relative path, with `--cache DIR` overriding all three. **Two alternatives
  the entry itself had proposed both lost.** Resolving against the sidecar,
  the way `stump::cache::path_for` does, cannot work as stated: that function
  takes a `&Work` and runs per work, while `cache_dir` is one directory for a
  server that may be handed sidecars in several directories, so there is no
  single sidecar to resolve against. Canonicalising against the first sidecar
  would have made the location depend on argument order instead, trading one
  surprise for another. Both would also have written into the corpus tree.

- **Move `web/` from npm to pnpm. Shipped 2026-08-09** on
  `toolchain-pnpm-node26-ts7`; `docs/ci.md` is the record now, this entry is
  kept as the decision that opened it. Decided while adding the browser test
  project, and deliberately not done in the same branch: it touched the
  lockfile, `.forgejo/workflows/ci.yml`'s `npm ci`, every `npm --prefix web`
  call in `scripts/check-all.sh`, and the docs that named them — none of which
  had anything to do with the reader. It also had to be stacked rather than
  parallel, since the lockfile it produces had to already contain the browser
  dependencies.
  **Two things were flagged to check rather than assume, and both were.**
  `taiyaku-wasm` was a `file:` dependency on the gitignored, generated
  `crates/taiyaku-wasm/pkg`, and `vite.config.ts` widens `server.fs.allow` to
  exactly that directory — pnpm's symlinked layout does resolve such a
  dependency differently from npm's flat one: `file:` does not survive a
  fresh clone under pnpm at all, which is why the protocol shipped as `link:`
  instead (`docs/ci.md`, "Why `link:` and not `file:`, which is what npm
  used"). And the image running CI's `web` job had no pnpm; it ships as
  `npm i -g pnpm@11.3.0` in a step of its own rather than through `corepack`,
  which shipped with Node 22 but not enabled by default. Whether
  `node:26-bookworm` still carries it was recorded as unmeasurable — no local
  docker to check the image, per the design document's §10, "What could not
  be measured." It is not merely unmeasured: this machine's own Node 26.7.0
  install has no `corepack` on `PATH` (`command -v corepack` returns
  nothing), so the parenthetical this sentence used to carry — "and is in
  Node 26" — was wrong, not just unverified. A later branch reaching for
  `corepack enable` in place of `npm i -g pnpm@11.3.0` on the strength of
  that sentence would die on the `web` job the same way.

- **Fall back to another edition where this one has no page, marked as such.
  Shipped 2026-08-13** on `gap-fallback`;
  `docs/superpowers/specs/2026-08-12-gap-fallback-design.md` is the record, and
  this entry is kept as the opportunity that opened it.
  The reader shows `Gap` when the edition being read has no counterpart for the
  slot — an honest answer, and a missed opportunity. The alignment already
  knows which *other* editions do hold that page: `route` will answer for any
  pair, and `nearest.ts` already scans for the closest paired slot. So instead
  of a page that is simply absent, the reader could draw the counterpart from
  whichever edition has one — the raw, the fan release, a colour edition — with
  the substitution **marked** rather than silent, in the same spirit as the
  `via` field distinguishing a direct answer from a composed one.
  **This is arguably the whole point of the alignment**: a reader following a
  story should not lose a page because the edition they happen to be in dropped
  it, and no single-edition reader can offer that. It wants a setting rather
  than a default — a reader comparing translations may want to *see* that a
  page is missing — and it wants the marking designed properly, since a page
  silently drawn from another edition would be worse than a gap. Raised while
  reviewing the two-page work, where the four solo groups the aligned pairing
  produces are the same fact seen from the other side.
  **What shipped** is `web/src/modules/borrow.ts`'s `lenderFor`: candidates are
  `orderedEditions`' hub-first order minus the viewed edition and minus the
  authority, the first that resolves lends, and the authority lends when none
  of them does. *That last clause is false since `borrow-lender-and-arrows`,
  kept rather than deleted per this file's own convention — see "The
  authority stops lending", below.* It is layered onto `display` by
  `withBorrow` rather than built into it, because `nearest.ts` reads
  `display`'s own answer to decide whether an offer lands on a *real* page,
  and a `display` that returned `borrowed` would have turned every gap offer
  into a jump to a third edition. The mark is
  a tinted leading edge and a button naming the lender and its page, with the
  page's `alt` text saying so **unconditionally**, so the substitution is never
  a purely visual signal; and that button **is** the way back, since pressing
  it reveals the `Gap` view for that slot, offers intact, rather than spending
  a keybinding on a screen most readers meet rarely. **The revealed side
  carries a control back to the borrowed page**, which the first cut did not
  and which a whole-branch review caught: pressing the mark is looking, not
  choosing, and with the arrows inert (below) and `Gap` taking no dismiss
  handler, a reader who looked could only get the page back by swapping to
  another edition and back again. The setting the entry
  asked for exists and defaults **on** — the case it made, that a reader
  comparing translations may want to *see* a page is missing, is served by the
  switch existing rather than by its default, and a reader who never opens
  settings is better served by the page than by the gap.
  **Two findings from reading the code shaped all of it, and this entry knew
  neither.** The authority edition can always lend: `display`'s `from` is
  `resolver.slotAt(authority, slot)`, so wherever it is defined the authority
  holds that exact position by construction — which means **the only gap that
  survives the feature is the one where the authority's own slot is invalid**,
  a state `restore`'s validation and `navigate`'s resolver-derived slots
  already prevent. *Both claims are false since `borrow-lender-and-arrows`,
  kept rather than deleted per this file's own convention — see "The
  authority stops lending", below: removing the fallback surfaced 228 further
  gaps this finding said could not exist.* And a gap is a single-position
  state rather than a per-page one: `App.tsx` consults `shown.kind` *before*
  it consults the mode, so a gap
  replaces the whole page area in every mode and a borrowed page is therefore
  drawn **solo**. **The entry was written expecting the opposite**, and that is
  worth the same keeping as L1 geometry's wrong line below: "draw the
  counterpart from whichever edition has one" pictures a borrowed page woven
  into the sequence beside real ones, and there is no scroll column with a hole
  in it and no half-borrowed pair for one to be woven into. An early
  brainstorming sketch drew exactly that state; it is not reachable.
  **What the rule gets wrong, recorded rather than left to be discovered.**
  Preferring a third edition over the authority is right when the authority is
  the raw and the third is another English release — the case the feature
  exists for — and wrong in the mirror case, authority `en-fan`, viewed
  `en-official`, third `raw-mono`, where it draws the raw over the other
  English edition. **Nothing available distinguishes them**: the sidecar's
  `editions` block carries a free-text `label` and no language, script or kind
  field (`docs/sidecar.md`), so "prefer a same-language edition" is not
  expressible at all. A reader-set order per work is the escape hatch, deferred
  until reading produces the case rather than built against a guess.
  **Verified in a browser against `gotoubun-v01`**, the only corpus material
  with three editions and so the only place the rule does anything the rejected
  "authority always" rule would not: viewing `en-mono` at `raw-mono` slot 1
  draws `en-fan` page 44, and viewing `en-fan` at `raw-mono` slot 193 draws
  `en-mono` page 186. **The authority lent in neither direction** — which is
  exactly what preferring a third edition exists to produce, measured rather
  than assumed. The session is `notes.md`, "The lender rule in a browser: two
  directions, and the authority lending in neither", including what it does not
  establish — one volume, one session, and the mirror case the rule gets wrong
  is not reachable on any corpus material.
  **The two-edition degeneration, stated rather than hidden.** *This
  paragraph describes behaviour removed on 2026-08-13, kept rather than
  deleted per this file's own convention — see the paragraph after this one
  for what replaced it and why.* With only two editions there is no third to
  prefer, so the rule draws the page the reader just swapped away from,
  marked — and that is most of the corpus. The mark is what distinguishes it
  from the swap having done nothing; whether a reader finds it useful or
  irritating is a question only reading answers, and neither the design nor
  this entry claims to know. It carries a consequence larger than "adds a
  branch", which the design did not state: with the module on, **`Gap` is now
  reachable only through the mark**, and on a two-edition work it is
  otherwise seen only with the module switched off.
  **The authority stops lending. Shipped 2026-08-13** on
  `borrow-lender-and-arrows`;
  `docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`, §2,
  is the record. The paragraph above was wrong about something the design
  had not yet measured: `crates/taiyaku-cli/src/borrow_frequency.rs`'s
  `how_often_a_reader_meets_a_gap`, driven against the twelve-volume corpus's
  6,340 reader positions, found **228 of 276 corpus borrows were the
  authority lending its own page back** — the page the reader had just
  swapped away from, since `applyIntent`'s toggle already goes to
  `state.authority` whenever `viewing !== authority`, true of every gap — and
  only **48 were a genuine third edition**, the case the feature was built
  for. `lenderFor`'s final arm is `undefined` now rather than the authority,
  so those 228 positions show `Gap` again: on a two-edition work — nine of
  the corpus's twelve volumes — `borrow` changes nothing at all, and the
  switch above governs only the 48. The setting stays rather than being
  retired, since a reader who already turned it off should not have that
  choice taken back. `notes.md`, "How often a reader meets a gap, and who
  actually lends", is the session; the figures above are unchanged by this
  fix, which only removed a branch that never should have counted as
  lending.
  **A borrowed page is still not a reading position.** `at` stays `undefined`
  exactly as in a gap, so annotations do not draw, zoom does not engage, and
  the page is not a prefetch target — every rule that guards on `at` is
  unchanged, which is what keeps this off the correction UI, note anchoring,
  `pairing.ts`'s grouping and the filmstrip.
  **And the arrow keys do nothing**, which is the one item on that list a
  reader meets without going looking for it. *False since
  `borrow-lender-and-arrows`'s "An arrow on a gap takes the offered move, on
  both screens", kept rather than deleted per this file's own convention —
  see the paragraph after the next one for what replaced it.* `session.ts`'s
  `step` calls the
  *unwrapped* `display` and refuses anything that is not `at`, so from a
  borrowed page ← and → are silently inert while an apparently ordinary page
  fills the screen. Measured in the browser rather than reasoned about:
  `ArrowLeft` left the page exactly where it was, and the design had not
  anticipated it (`notes.md`, "The lender rule in a browser: two directions,
  and the authority lending in neither").
  **That is the feature's real cost, and it is a discoverability cost rather
  than a mechanical one.** *Also false as of the same commit — see the
  paragraph after this one.* `Gap` announced its dead end in a sentence and put
  both escapes on screen beside it, so an arrow that did nothing was explained
  by the screen it did nothing on. A borrowed page announces nothing. Its only
  in-page escape is `Borrowed`'s mark, whose label reads as a caption rather
  than as a control, and a reader who does not press it has nothing on screen
  to tell them why the arrows are silent — they have been handed a page that
  looks like every other page and behaves like none of them. The trade was
  made knowingly, and the entry's own premise is that the page is worth more
  than the gap; it is still a trade rather than a free win, and any complaint
  that arrives about this feature will most likely be this one.
  **Both fixed on the same branch.** `session.ts`'s `step` no longer refuses a
  state that is not `at`: on a gap or a borrowed page it now takes
  `nearestPaired`'s before/after offer on the pressed side via `takeOffered` —
  the same destination `Gap`'s own buttons use — so ← and → move rather than
  sitting inert (`borrow-lender-and-arrows`, "An arrow on a gap takes the
  offered move, on both screens"). *"The same destination" was true of both and
  right about neither where a credit page was involved: both call sites ignored
  the `credits` module's skipped set until O40 closed on 2026-08-14, so they
  agreed on a page the reader had asked not to be taken to. They are now given
  that set from one place, and still agree.* And `Borrowed` no longer waits for the mark
  to be pressed to say anything: a line reading "No {viewing} page here." sits
  along the bottom of the page, with the same two escape buttons beside it —
  `GapOffers` (`web/src/GapOffers.tsx`), extracted so `Gap` and `Borrowed`
  share one component rather than risk the two drifting apart, now that the
  arrows take the identical destinations on both. A reader who never presses
  the mark now sees why the page behaves differently and has both ways out on
  screen, the same as `Gap` always offered (`borrow-lender-and-arrows`, "The
  borrowed page says what it is, and shares Gap's two escapes").
  That the page is not a reading position is a limitation rather than an ideal,
  and it is **deliberately a note rather than an open item or a new entry in
  this list**: no reading has produced the complaint, and how often a
  gap arises at all is itself unmeasured (the design's §10). *That last clause
  is false since `borrow-lender-and-arrows`, kept rather than deleted per this
  file's own convention — the count is `notes.md`'s "How often a reader meets
  a gap, and who actually lends".* `notes.md`,
  "A borrowed page is not a reading position, and might one day want to be",
  carries the three questions making it one would have to answer first.

- **Pre-warm a work's page sizes on load.** Since `?w=` shipped, the first read
  of a page at a given size costs the server a decode and a resize —
  **126 ms median generation against 10 ms warm**, measured over 235 real
  generations. Prefetch reaching one group ahead covers ordinary reading, since
  generation is faster than a reader turns pages; a fast flick through cold
  pages is not covered, and neither is the first page of a session.
  **Deliberately not built**, and the reason is a hazard rather than effort:
  resizing runs behind `cover.rs`'s two-permit `GENERATION_LIMIT`, shared with
  covers and tiles, so a warm pass would occupy both permits and the reader's
  *own* cold page would queue behind it — making the case it was meant to fix
  worse. *(Since O111 closed on 2026-09-17, only a cold page that needs a
  resize queues there; one that already fits takes no permit.)* Any attempt needs to answer that first: a separate low-priority permit,
  or warming only while the reader is idle. It must also fetch without decoding,
  or it re-creates the bitmap-eviction problem `pageSize.ts` exists to remove.
  Client-driven, too — the server does not know a viewport and would have to
  guess a rung or generate all six.

- **L2 text regions** — a reader points at a balloon on the raw and sees the
  matching English one. **Stays here, and the line that used to describe it was
  wrong twice.** It read:

  > **L2 text regions** — bubble pairing by transformed centroid, which L1's
  > `mapRect` now makes arithmetic rather than a matching problem.

  It is right that the pairing is arithmetic now. It is wrong that a region is a
  **bubble** — what serves both the pairing and a later text-selection pass is
  the **text block** inside the balloon, since a balloon detector is blind to
  the borderless narration manga uses constantly. And it is silent about
  **detection**, which is the larger half of the work by a wide margin: nothing
  in this repository detected anything on a page, and "pairing by transformed
  centroid" presumes a centroid on each side to pair.
  **L2a has since landed the half that is not detection** — the
  `<stem>.regions.json` format in `taiyaku-core`, the `regions` command, and
  `scripts/region-survey.sh` — and measured the half that is. The detector does
  not yet clear the design's own bar, for a reason that is now measured; see
  "What is known to be wrong or missing" above, and `notes.md`, "The text-block
  detector, measured against four works, and why it does not work yet". What
  remains before a reader sees anything is a detector whose scale is computed
  over glyphs rather than over screentone, and then L2b's pairing.
  **The instrument that can judge such a detector shipped on 2026-08-11**, as
  sub-project A of two;
  `docs/superpowers/specs/2026-08-11-l2-ground-truth-instrument-design.md` is
  the record. A `taiyaku.truth.json` format in `taiyaku-core`, `GET`/`PUT
  /truth` on the server, a browser labeller at `/label.html`, and
  `taiyaku score`, which reports **five outcomes per hand-drawn box** —
  one-to-one, merged, split, fragment, missed — plus recall per kind and the
  count of boxes flagged `ignore`. `taiyaku score --sample corpus` prints the
  worklist: 45 dev pages over nine editions, and 30 sealed held-out pages over
  six the survey has never touched.
  **Why an instrument came before another candidate**: every acceptance rule
  this project has stated for the detector has since failed *as a rule* —
  `median_area` could not fire, `blocks_per_page` ranks the editions
  backwards, and L2A Task 9's rule inherited the second one's blindness. A
  fourth would have failed the same way, and nobody would have found out until
  someone cropped boxes by hand again.
  **The first cut of the scorer reproduced that failure exactly**, which is
  worth recording rather than quietly fixing: it decided "merged" by *counting*
  how many labelled boxes a prediction covered, so a whole-page prediction over
  one balloon came back a clean hit — and three of the five panels in
  `notes.md`'s cropping table hold exactly one balloon. Classification is
  directional now; association is not. The line that stood here read
  "**Nothing is labelled yet**, so no figure about the detector has changed and
  sub-project B has not begun", and it was true when written and is no longer:
  478 boxes over nine editions are labelled, and the figures above — the
  incumbent's 2 of 478 and the promoted candidate's 48 — are what the labelling
  produced.
  **Phase 3's spike ran on 2026-08-12 and answered all three of its questions,
  plus one it was not asked.** `tract` runs comic-text-detector unchanged —
  seventeen operators, all supported, opset 11, no `NonMaxSuppression` in the
  file at all because upstream suppresses in Python — so the `ort` fallback was
  never reached and the offline stance holds, at 3.8–4.0 s per page on CPU, flat
  across page sizes. The weights are redistributable and carry an **attribution
  obligation** the design did not know about: the training data is Manga109-s,
  the commercially-approved subset, not the academic-only Manga109 whose clause a
  reader meets first. And the question the spike was scoped to add: over the
  20 pages of the four Japanese `raw-mono` editions, where the promoted classical
  candidate scores **0 of 167** dialogue boxes, the model's block head proposes
  **147 blocks against 167 dialogue boxes**, sitting on the balloons. That is a
  count of proposals and a look at four overlays — **not a score**.
  **Phase 3 produced the score the same day, and the model clears the rule on
  both terms.** `model_experiment.rs` ran both candidates over the same 45
  pages, the same nine editions, the same 478 scored boxes, through the same
  `score_page` at the same provisional `COVERAGE = 0.5`:

  | outcome | `detect_page` (L2b) | `model/blk` |
  | --- | ---: | ---: |
  | 1:1 | 48 | **130** |
  | merged | 73 | 97 |
  | split | 152 | 6 |
  | fragment | 81 | **137** |
  | missed | 124 | 108 |
  | spurious | 303 (6.73/page) | 3 (**0.07**/page) |

  **The model row above is the second measurement of it, and the first was
  wrong.** It read `118 87 32 133 108 3` until `14cb71c`, which fixed a defect
  in this project's port: `suppress` was class-aware, faithfully to YOLOv5, so a
  block whose grid cells disagreed about which of the two class channels won
  survived **twice**, as two boxes at an IoU of 0.94 to 0.99. Upstream gets away
  with that because `inference.py` follows NMS with `group_output(...)`, which
  regroups the survivors; this port has no such stage, so the duplicates reached
  the scorer. The fix is `agnostic=True` on an existing YOLOv5 flag — the other
  setting of something already there, not a new mechanism and not a new
  constant. The classical row did not move. **The score could not see this**; the
  cross-edition grouping measurement below found it, and the pixels confirmed it
  (`gotoubun-v01` raw 165 against `en-mono` 157: one balloon, two boxes), at
  **40 of 161 English regions carrying such a twin against 0 of 147 Japanese
  ones**. **The reasoning was written down before the re-run**, deliberately,
  because the numbers move in the direction that flatters the model. **`missed`
  and `spurious` did not move** — 108 and 3 in both measurements — and that is
  the check, since removing duplicate boxes cannot change what was never found.
  **Every one of the twelve extra dialogue hits is English**, which is the
  second check: the Japanese pool is 47 of 167 before and after, per edition as
  well as pooled, while the English pool moves 65 of 222 to 77 of 222 — exactly
  where the twins were, so the arithmetic corroborates the diagnosis rather than
  merely failing to contradict it.
  **The verdict, stated against the rule fixed on 2026-08-11 rather than
  against the outcome:** term 1 is 130 against 48, term 2 is 0.07 against 6.73,
  and both sit inside the ceiling of 10.7 truth boxes per page. **Both pass.**
  The **0 of 167** dialogue boxes this entry has quoted since L2b is **47 of
  167** for the model — untouched by the fix above — and it is not bought by
  trading the English side away: the English pool moves 43 of 222 to 77 of 222
  at the same time, and the model takes more 1:1 than the classical detector in
  every one of the nine editions.
  **Three things are recorded beside that, because a summary without them would
  be dishonest.** `fragment` rose from 81 to 137: the model's failure shape is
  different, not absent. Sfx recall moved the **wrong way**, 5 of 63 to 2 of 63
  — the classical detector finds more sound effects than the model does. And
  the model's 0.07 spurious per page is numerically the **incumbent's** figure
  from the "8.6% of the page" paragraph above, reached from the opposite cause:
  the incumbent's 0.07 was that it barely proposed anything at all, while 137
  fragments prove this one proposes plenty. The rule's second term cannot tell
  those apart; the first term is what does.
  **Promoting the model into the binary remains a separate decision** (design
  §9), and it now carries two things with it: the **Manga109-s attribution
  obligation** — anything shipped with these weights must state clearly that
  Manga109-s was used — and a `COVERAGE` this result is more sensitive to than
  any before it, since `blk` hugs the glyphs where a hand-drawn box carries
  margin, so the model's predictions are systematically *tighter* than the
  truth. That interaction is predictable in direction and unmeasured in size.
  **That decision was taken on 2026-08-12, and the answer is not yet — for a
  reason that is not about the measurement.** The model stays a `#[cfg(test)]`
  experiment; `detect.rs` keeps shipping; nothing gains a 95 MB dependency. The
  reasoning, recorded because a deferral without one decays into a to-do:
  **a text detector may not be this project's to own.** Panels, the iOS comic
  reader, has a text-selection mode that appears to lean on a native iOS
  capability rather than a bundled model — Apple's on-device text recognition —
  and if the platforms that matter already carry text detection, then shipping
  95 MB of weights, a fetch step, a licence obligation and a 4 s/page inference
  is a large cost to take on for something the operating system may hand over
  for nothing. Nobody has looked. Until somebody has, committing to a
  text-finding model would be choosing a mechanism before surveying the
  alternatives, which is the failure this whole sub-project has been correcting
  for.
  **This does not block anything, and that is the point.** L2b's pairing works
  against `Region`s and does not care what produced them; the model is
  measurable today, gated, and reproducible in one command. What a survey
  would have to answer: what the browser PWA can use, since a native iOS
  capability does not reach it and the reader is a PWA first; whether a native
  shell is close enough to change that; and what such an API returns — lines,
  words, or the *blocks* pairing needs. A native text API that returns word
  boxes solves a different problem from the one measured here.
  **That survey ran on 2026-08-12 and all three questions are answered, in the
  direction that removes the alternative rather than supplying it.** `notes.md`,
  "What the platforms already do, and the two halves no public API puts
  together"; spec §4.3 carries the dated settlement. **The browser: nothing.**
  Shape Detection's `TextDetector` is flag-only in Chrome, absent from Safari
  and Firefox, on no standards track — and its specification scopes it to
  **Latin-1 text**, so Japanese is not badly supported, it is out of scope by
  definition. WebNN is origin-trial only, estimated at 2027. Safari's Live Text
  on an `<img>` is user-invoked and hands the page no coordinate.
  **The headline is on the native side: Apple has two text APIs and each has
  exactly one of the two properties L2 needs.** `Vision`'s
  `RecognizeTextRequest` returns bounding boxes and **cannot read vertical
  Japanese** — Apple's own DTS answer is to rotate the page upright yourself
  first, which presumes the detection this sub-project exists to do.
  `VisionKit`'s `ImageAnalyzer`, the Live Text framework, **does** read vertical
  CJK and **exposes no positions at all**; the geometry is private to
  `ImageAnalysisOverlayView`. The API that can read the writing system returns
  no coordinates; the API that returns coordinates cannot read the writing
  system. **So the Panels comparison does not transfer**: Panels needs
  *selection*, which Apple's overlay draws without ever handing the app a
  rectangle, and taiyaku needs a rectangle on both editions or there is nothing
  to pair. Android's ML Kit v2 does return `TextBlock`→`Line`→`Element` with
  boxes and supports the Japanese script — the right shape, on a platform this
  reader does not target, silent about writing direction and unmeasured on
  manga.
  **And the question's own premise was the wrong shape**, which is the part
  worth keeping: "what the browser PWA can use" assumed detection has to happen
  where the reading happens. It does not — the PWA is served by `taiyaku serve`,
  which already decodes, caches and serves per-work JSON, so a `regions` route
  beside `/truth` is the same shape as what is there and 4 s/page is a cached
  server cost. The OS-API question binds only in an architecture that is neither
  built nor planned: a static PWA against Stump, or a native app.
  **What it changes about the decision is less than it looks.** The deferral's
  first half — the platforms may do this for nothing — does not survive. Its
  second half is untouched: 95 MB, the fetch, the Manga109-s obligation, the
  dependency and 4 s/page cost exactly what they did. There is simply no cheaper
  alternative left to wait for, and that is **not** an argument to promote,
  because the model still finds only 47 of 167 Japanese dialogue boxes and the
  confidence sweep showed the other 120 are under no threshold. **No route is
  free.** The cheapest unexplored one is unchanged: the `seg` head, out of the
  same forward pass at no extra cost, firing on lettering `blk` proposes nothing
  for. **This survey is weaker evidence than anything else cited here and says
  so** — no API was run, on any device, against any page of this corpus; it
  settles what the vendors document and nothing about how it behaves on a manga
  raw.
  The 30 sealed held-out pages were **not** opened. `notes.md`,
  "comic-text-detector through `tract`: four questions, and the head that
  answers them", "The model candidate, scored against the same 45 pages" and
  "The model against itself across the two editions: does it group the same way";
  spec §4.1 and §4.2 carry the dated settlements.
  **The confidence floor was swept on 2026-08-12 and `BLK_CONF` stays at 0.4**,
  which is a refusal rather than an oversight. The premise was sound — the rule
  caps spurious at 10.7 per page and the model sits at 0.07, so there was ~150x
  of unused precision to trade for recall — and the trade is real: at a floor of
  0.02, `missed` falls 108 to 57 and 1:1 rises 130 to 147, still 19x under the
  ceiling. **But Japanese dialogue is flat at 47 of 167 across seven of the
  eight rows.** Every extra hit is English. The 120 Japanese boxes the model
  misses are not low-confidence detections under the floor; `blk` does not
  propose them at any floor, so no threshold reaches them. The gain also worsens
  the failure *shape* — `split` 6 to 36, `fragment` 137 to 162 — so it would
  trade 17 English matches for 55 more wrongly-shaped boxes and zero Japanese
  ones. **The promotion rule cannot arbitrate this**: its second term is slack
  across the whole table, so by term 1 alone the rule would adopt 0.02, and
  applying it here is using a rule written to compare a candidate against an
  incumbent to pick a parameter inside one candidate. `notes.md`, "Where the
  confidence floor goes, and the thing it turns out not to buy". The sweep is a
  dev-set overfit and says so; the held-out 30 stay sealed.
  **The `merged` 97 above was then asked the only question that decides whether
  it costs anything, on 2026-08-12.** The model draws one box over two adjacent
  balloons where the labeller drew two. That is free if it merges the *same* two
  on the raw and on the English page — L2 pairs region to region, so both sides
  agree and a reader still lands on the right balloon — and it breaks pairing if
  it merges on one side only, which no relabelling could fix because it is model
  behaviour rather than a labelling convention. A second gated test in
  `model_experiment.rs` runs the model over the five labelled `raw-mono` pages of
  each of the four works and over each page's English counterpart, resolved
  through `Sidecar::runs_of` and `counterpart` in **slots** rather than file
  pages, and compares by **centroid containment** — not IoU, which would be
  measuring the writing system, since a vertical Japanese balloon and its
  horizontal English counterpart share a centre and share almost no area.
  **19 of the 20 pairs compared**, one skipped because its counterpart slot fell
  on a joined spread and splitting that page is L2b's problem, not this one's;
  147 raw regions against 141 English. Of 147 raw centroids, **124 land in
  exactly one English region**, 23 in none and 0 in two or more; of 141 English
  centroids, 115 land in exactly one raw region, 25 in none and 1 in two or more.
  **The headline is 5 of 147 raw regions holding two or more English centroids,
  all 5 of them real grouping disagreements, and 0 of 141 the other way.**
  **This measurement found a defect before it answered its own question.** Its
  headline first read 23 of 147 of which only 5 were real, and the other 18 were
  the class-aware suppression fixed at `14cb71c` and recorded above — one balloon
  carrying two English boxes. After the fix the two printed figures, raw
  containment and containment after discounting near-identical twins, are **5 and
  5: they converged**. That convergence is the check, and a stronger one than the
  number merely falling: the two routes measure the same thing and disagreed only
  because of the artefact, so their agreeing says it is *gone* where a smaller
  number would only have said it was *smaller*. Both figures still print, and so
  does the twin line — now 0 of 147 and 0 of 141 — so a regression shows up as
  them parting again.
  **What it says: the labelling-convention mismatch costs almost nothing.** About
  3% of raw regions are genuine grouping disagreements and the mirror case is
  zero, so when the model draws one box over two touching balloons it
  overwhelmingly does the same on both editions and pairing maps region to region
  cleanly. That **retires relabelling as something this project needs to do for
  that reason** — the labelling convention itself is written down at `f165521`,
  in `crates/taiyaku-core/src/truth.rs`, and the open question it left was
  whether L2 pairing needs a unit consistent *across editions* rather than one
  per balloon. It does not, here. `kobayashi-v01` raw 36 against `en-mono` 38 is
  one of the five real ones: two adjacent balloons boxed separately in English
  and together on the raw. The test asserts none of this — whether 5 in 147 is
  tolerable is a person's call, and a check that fails when the model improves is
  not a measurement. **What this does not settle:** page fractions correspond only
  approximately across editions of different aspect (0.669 against 0.703 on one
  real pair, deltas of ±0.02 to ±0.04 printed per pair), which the test
  deliberately does not correct for — fitting that transform is L2b's job — and
  one page pair is unmeasured because its counterpart landed on a joined spread.
- **In-volume chapter detection.** `Sidecar::chapter_at` answers "which chapter
  is this slot in" only by routing the slot into a *chapter-organised* edition
  and reading off which member it lands in, so it is silent for the common
  case, which its own doc says plainly: `None` covers "this work has no
  chapter-organised edition (the common case, and not an error)". It also
  **names a member, not a chapter number** — recorded as a shortcoming where
  the readout shipped, and separate from the `~` that readout carries, which
  marks the *alignment*'s standing rather than the label's. Four things want
  the boundaries a volume already carries: a readout on a work with no chapter
  edition, a filmstrip and a key that step by chapter, the chapter-to-volume
  map `notes.md` argues a tracker needs, and sampling that steps over front
  matter because it knows where chapter one starts rather than because
  `spread_sample` spreads evenly to avoid the front of a volume.
  **Three tiers, in order of cost and certainty, and the first two are nearly
  free.** (1) *Entry names inside the archive*: `notes.md`, "What the archives
  actually contain", records that the EN official encodes chapter and role in
  every filename — `c001 (v01) - p000 [Cover]`, `[ToC]` — and that the EN Full
  Colour nests pages under per-chapter directories; it calls this "a plausible
  source of coarse chapter boundaries" and nothing has taken it up.
  `sidecar.md` measured the nesting case at **2 of the 7** archives that nest,
  Go-Toubun's Full Colour under `Chapter 15 - What They Have Accumulated/`.
  **12 of 57** archives also carry a `ComicInfo.xml`, which rule 1
  deliberately excludes from the page count and nothing has ever opened.
  (2) *The alignment*, which is `chapter_at` today and already exact wherever
  a chapter edition exists. (3) *A content signal*, only where neither is.
  **Where it goes: a `chapters` field on `Edition`, beside `spreads` and
  `credits`** — but a struct rather than a bare index list, a start page plus
  an optional name, because both the tracker map and a readout saying `c005.5`
  instead of `member 6 of 7` need the chapter's identity and not just its
  position. Written by a command of its own that prints and writes nothing
  without `--write`, the discipline `credits`, `inventory` and `duplicates`
  set. The name is optional by construction: tiers 1 and 2 yield one, tier 3
  yields only a position, and a mixed edition has both.
  **Four hazards, each already measured somewhere in this repository.**
  *Declared metadata is wrong here* — `notes.md`, "It also derives the
  chapter-to-volume map", found both corpus volumes mislabelled by their own
  filenames, so reading `ComicInfo.xml` for chapter starts inherits a source
  this project has already caught lying. *The front of a volume is not chapter
  one* — `credits` measured that what accumulates at a member's *start* is
  front matter, and page 0 is a cover. *A recurrence signal will not classify
  this either* — `credits::recurrences` is a diagnostic and never a classifier
  because its two tightest matches in a whole edition are the volume cover,
  and `sidecar.md` makes the same point from the other side: "a chapter
  divider that resembles all nine of its siblings scores 0.98 and means
  nothing". *The unit needs deciding* — `chapters` in pages, like `spreads`
  and `credits`, cannot express a boundary falling at a joined spread's
  midpoint, and projecting a fan release's member boundary through the
  alignment lands on a slot rather than a page.
  **What it must not repeat is L2's lesson.** Every acceptance rule stated for
  the text-block detector before the ground-truth instrument existed failed
  *as a rule*, and nobody found out until boxes were cropped by hand. Here the
  ground truth is free and wants no labeller: where a chapter-organised
  edition is aligned to a volume the boundaries are already known, so a tier-3
  detector can be scored against them before any rule is stated. The primary
  corpus supplies seven such files against one raw volume. Two works is not
  the population, which is the same bound `MAX_CREDIT_RUN` carries.

- **Art book linking**, if it is worth it — needs a different hash, and roughly one
  art book page in three has any manga counterpart at all.

- **Every edition on screen at once, turned together.** Raised 2026-09-30.
  The first design said L0 would unlock "whole-page swap and synchronised
  side-by-side" (`2026-07-28-taiyaku-design.md`), and only the swap was
  built: the reader draws one edition at a time. The ask is wider than the
  pair that sentence imagined — two, three or four editions in panes across
  the screen (the raw, the official release, a fan release, a colour
  edition), with one page turn moving every pane to the same place. Much of
  it exists already. `ReadingState` (`reading.ts`) keeps the `authority`, the
  edition that owns the position, apart from the edition being viewed, so N
  panes are N viewed editions over one authority; `display` says what an
  edition draws for a slot, gap included; `withBorrow` (`borrow.ts`) says
  what fills a gap; and `taiyaku_core::route::route` resolves any pair
  through the hub. What does not exist is everything a one-pane reader never
  had to decide. **Which pane drives**: an arrow steps by what is drawn, and
  a joined spread in one edition is two pages in another, so "the same
  place" is the driving pane's group projected into the others. **What a
  follower shows at a gap**: `Gap`, a borrowed page, or an empty pane —
  borrowing into a pane whose purpose is to show *this* edition may be
  exactly wrong. **How it composes with two-page mode**: §8's pairing agrees
  across editions, so N panes of two pages is well defined, and 2N pages is
  not readable on a phone. **Whether L1's zoom follows across panes.** And
  **what it costs**: N times the page fetches and prefetches, on the device
  with the least memory. Each pane also needs its edition named to a screen
  reader, not only placed on the screen. **Decided 2026-09-30: a phone shows
  one edition at a time**, as it does now; panes are for the larger screens.

- **Books: two editions side by side, and an apparatus set properly.** Raised
  2026-09-30, and wider than §10's "project 3" row in
  `2026-08-14-library-store-design.md`, which asks only for reflowable EPUB,
  "a spine-and-CFI position model and a second reader". Two asks sit on top
  of that. **Alignment**: two editions of one book — two translations in one
  language, or the same book in two languages — to swap between or read side
  by side, kept in step, as manga editions are. **Typesetting**: critical and
  annotated editions whose notes, commentary and variants a reader can reach
  without losing their place. A read-only survey of fourteen EPUBs on the NAS
  (`docs/notes.md`, "Books: what the library's EPUBs hold") found four things
  that shape both. **None marks its notes semantically**: each publisher
  names its own classes, so a note is found by its links, not its markup.
  **The apparatus comes in at least four shapes**, from linked footnotes to
  commentary keyed only to line numbers printed in the text. **Some books
  carry their own alignment**: verse line numbers on an original and its
  translation, per-tercet links between them, print page lists. **And a table
  of contents is not a skeleton to align on**: one of two translations of the
  same novel leaves a chapter out of its contents and never marks the
  chapter's heading as a heading, and paragraph counts are what found it.
  That last is the manga problem again, one level down — structure is a hint,
  and content is what aligns.
  **Decided 2026-09-30, by the user.** Books make taiyaku an everyday reader
  as well as a comparison one, with the comparison features behind settings
  that turn them on and off. **Alignment is measured before the reader is
  designed**, so the reader is built around what the measurement says a
  block and an anchor must be. Same-language and cross-language pairs both
  matter, and may be aligned by different means. The unit a reader keeps in
  step is generally the paragraph, looser where a pair needs it. Scrolling
  comes first and pagination after. Alignments are hand-correctable, as
  manga's are. **PDF editions are in scope**, and for testing may enter as
  page images; the two Emmas on the NAS are both scans — Cambridge's with no
  text layer at all, Belknap's with an OCR one. **Phase 1, the measurement,
  is designed** in
  `docs/superpowers/specs/2026-09-30-book-alignment-measurement-design.md`
  and not yet built.

**L1 geometry left this list on 2026-08-08, and the line it left behind is kept
rather than deleted.** It read:

> **L1 geometry** — affine transform per pair, so a swap preserves zoom and pan.

Wrong on both counts, which is exactly why it is worth keeping: a list of future
work is also a record of what the project expected of that work. There was no
zoom for a swap to preserve — building it was most of the milestone, and the
design records that inversion as the largest thing it settled — and no per-pair
transform is stored, in the sidecar or beside it. §5 above is what shipped.

- **Libraries whose directories overlap.** Asked for by the user on 2026-10-18,
  while trying O219's Libraries pages: today a directory inside or around one
  already held is refused (`scan::vetted`). Allowing it could reach whatever
  assumes a file is reached through one directory, in one library: the walk, the
  derive, a work's library, grants, the `?library=` narrowing, and what Move
  and a removal count. The first step is a survey of those assumptions and of
  what an overlap should mean for each, sized before anything is built; the
  user expects it to be large, and separate from O219.

- **A sidebar and readability redesign. Its first PR shipped on 2026-10-19**
  on `sidebar-readability`; spec `2026-10-18-sidebar-readability-about-design.md`
  (§1–§3) is the record, and this entry now says what shipped. **The
  sidebar** is the wordmark, a "Libraries" heading that is itself the link to
  every library (`/`) and is drawn even for an account that sees none, the
  libraries under it, and at its foot the build's label, More, then Theme,
  last so that no link sits in the window's bottom-left corner, where a
  browser draws a hovered link's address (the user's ruling of 2026-10-27);
  its Library and Search links and the browse settings are gone
  (`Sidebar.tsx`, `Shell.tsx`).
  **More**, for an administrator, is a disclosure button that opens upward
  onto Proposals, Configuration and Account, and carries the open-proposal
  count as a badge and in its name; opening it leaves focus on the button,
  Tab enters the panel, and Escape, a click outside or any navigation closes
  it (`AccountMenu.tsx`, `popover.ts`). For anyone else More is a plain link
  to `/account`. Either form sits in a `nav` of its own labelled More, with
  Theme outside it, and is drawn in the accent on `/proposals`, `/config/*`
  and `/account`. **The browse preferences** are a View button at the right
  of a page's top row, a disclosure by the same rules: Layout, Edition
  selector, Panel and Density on a series page, Continue strip and Density on
  the Library view, and on both a Reset that restores all five
  (`ViewMenu.tsx`). **Search is a bar on the Library view** (O221, O223): the
  box, then View, scoped to the library the view shows and naming it in its
  placeholder. While it holds a query the hits replace the grid, and the
  tabs, the sort and the continue strip go; `/search` redirects to it with its
  query and its library (`SearchBar.tsx`, `searchHits.ts`, `Search.tsx`). The
  box follows what the grid shows: there is no box, and no search is run,
  where `?library=` names no library, for an account that sees none, or on a
  server with no library at all. It keeps its own text, and `?q=` sets it
  only on a change the box did not make, told apart by a tag in the router's
  navigation state. A fresh load of `/?q=` goes straight to search mode, so
  the tabs and the grid are not drawn first, and the call waits until the
  view knows it can search. **Text** that is read has a 14px floor
  (`--text-meta`), and `--text-badge` keeps 12px for count and kind badges
  and chips; the word buttons `.compare`, `.dismiss` and `.reopen` are
  14px and the one-glyph tile controls `a.get` and `.panel-open` 12px.
  Ink's colours are lifted until
  every text token reaches APCA Lc 60 on bg, surface and field
  (`tokens.css`, `index.css.test.ts`), and Ink's UI text is weight 500
  wherever Ink is in force — Ink chosen, or System with a dark preference —
  titles excepted. Paper's colours are unchanged, and the reader keeps its
  old values and weight. *As filed, when the user asked for it on
  2026-10-18:* the sidebar holds Library, Search and Libraries, and the rest
  goes in a settings fold or menu. Text in dark mode is made easier to read,
  by its colour and its font, and by grouping it on the screens that carry
  the most text. The grouping is the next entry.

- **Group the Configuration screens, carrying Stump's motifs over. All three
  PRs have landed.** Spec
  `docs/superpowers/specs/2026-10-20-config-grouping-design.md`, in three
  PRs. **PR 1, the rename, has landed**: "folder" is "directory" in prose
  and "dir" in names, routes, keys, addresses, `--dirs` and `TAIYAKU_DIRS`,
  with a clean break: no old name has an alias, and `serve` refuses to start
  while `TAIYAKU_FOLDERS` is set. **PR 2, the grouping, has landed**:
  Libraries, Users and Backup and every page under them are drawn on
  `settingsLayout.tsx`'s components, a page header over sections of rows,
  each section's heading outside its bordered box, and on a wide screen an
  account's page or New account opens beside the Users table and a
  directory's page beside its library's. A library's page and an account's
  are at their names, New library and New account are at
  `/config/new-library` and `/config/new-account`, and the server refuses
  `.` and `..` as names, which no address could hold, and an empty account
  name, whose address would be the Users table's. A library is renamed in
  its title's place and Move… opens in a dialog, so neither moves the page
  beneath it; a name another library or account holds, or `.` or `..`, is
  said as it is typed, in the rename's box and in New library's and New
  account's, and holds the button that would send it, while a name another
  administrator has freed since the list was read keeps the button held
  until the list is read again, as a change of page does, which is known and
  accepted; a save, a make, a rename, a move, a removal and a backup, and a
  removal's line saying what it would do, are drawn waiting only once they
  have lasted 200 ms, while the directory browser still draws "Reading
  directories…" at 400 ms, a scan's start holds a directory's Scan
  directory, Move… and Remove…, or a library's Scan library and Remove…,
  from the send, the picker's Add directory on a library's page is held from
  the send while its add is out, and the line saying a scan is running is
  drawn as soon as its start answers; the scan report says "Directories
  walked" and "Members whose directory was removed mid-derive"; and an
  account's "Password set", and "Password changed" on `/account` and in the
  reader's account panel, are said from a polite live region, so each is
  read when it lands. A control that a wait or a hold takes while it has
  focus keeps focus (the user's ruling of 2026-10-23), where a `disabled`
  one would drop focus to the page: it is held by `aria-disabled`, through
  `HeldButton` for a button, and turns a press, a choice or an edit down, as
  O159's checkboxes and radios do. An account's Remove… stays drawn and held
  while its confirm is open, as a library's and a directory's do. The
  exceptions are Save, Make the library and Add, their Cancels, the picker's
  Add directory, Set password while its box is empty, as a set leaves it,
  and See what this would do once the library chosen leaves the list: these
  are held by `disabled`, and move the focus they had to the name, path or
  password box, or to Move to. **PR 3 has landed**: Rules, `/account`, About
  and Licence are drawn on the same components and look, without side by
  side. Rules keeps no measure: where its list and the rule being edited fit
  side by side, each of its boxes is as wide as what it holds, starting
  where the other Configuration pages' content starts, with the room past
  them on its right. Its list and the rule being edited are each an untitled
  box, side by side, the list as wide as its widest rule up to the settings
  measure, its names broken inside a word only where a piece is wider than
  its line, and the rule's pane as wide as its sentence or its form; the
  preview is a box titled Preview beneath them, as wide as the two together
  and wider only where a tab holds something wider, whose heading's line
  says "Previewing…"; and Save, Import and Export stay at the foot, on one
  baseline wherever they share a line, Export beside Import and Import's
  help under the three. `/account`, About and Licence are each a visible h1
  over h2 sections, and the reader's account panel keeps its look. A made
  API key is announced without its secret (O228), and Rules' and
  `/account`'s waits are drawn at 200 ms, their held controls keeping focus,
  as PR 2's do. What the user asked for at its checkpoint and its re-try, on
  2026-10-25, is in it too, with the controller's calls on how named as
  such: what a create or a password change says moves nothing, "Key made"
  and "Password changed" being said from visually hidden polite regions (the
  key's region was the controller's call, the password's the user's); a
  refused Create is said at New key, moving nothing (the callout it is said
  in, laid over the page, and Change password's refusals said the same way,
  were the controller's calls); a made key's box and its row come together,
  as a revoked one's go; a new password is typed twice, on `/account` and in
  the reader's account panel, on an account's page and on New account, and
  "The two passwords do not match." is said once the confirm is neither
  empty nor the start of the password, wherever focus is; a press of the
  shown key's box copies it where the clipboard can be written (a button,
  the controller's call), and revoking that key takes the box away (that
  key's alone, the controller's reading); each licence has "↑ Back to top"
  on its heading's line (and at its box's foot too, the controller's call);
  and About links GPL-3.0-or-later and OFL-1.1 each to its own licence (two
  links, the controller's call). After the whole-branch review the user
  ruled, on 2026-10-26, where the callouts lie: each inside its own section,
  right under the row it answers and over the rows below it, a padding's
  width from the box's edge, and on New account over Role's select alone,
  with the small scroll kept that shows a refused Create whole where it
  would not fit. On New account the line saying the two passwords differ
  lies over Role's select alone where its words take one line; where they
  take two, as on a narrow phone or with larger text, it lies over the foot
  of Role's help as well, though over no box. A refusal with no room under
  its row, as a long one has, lies over the rows above it, not over the row
  it answers, wherever those have room, a refused Change password's
  included; and the catalog address's row is drawn whether `/api/about`
  answered or failed, saying so where it failed (the page waits for that
  read since the loading work's first PR), an unseen Copy
  address keeping a button's room wherever none is drawn, so a refused
  Create keeps that room above New key (the controller's calls). The user
  ruled too that the shown key's box draws "Copied" and its caption names
  its key, that a press of a button the two passwords hold says why, and
  that `/account`'s Copy address sits where the boxes' other buttons do. On
  a re-try the same day the user ruled that a press on the line saying the
  two passwords differ reaches nothing beneath it, as a press on a refusal
  does, while Tab still reaches what it covers; that Rules stops short of
  the right edge as it starts short of the left, at a measure of its own on
  a wide screen; and that the sidebar's wordmark is a link to `/`, named
  "taiyaku, Libraries", drawn at rest as before, while Login's stays a
  heading's words. After that round's review the user ruled that Rules
  starts where the other Configuration pages do, past its measure leaving
  the room on its right, and that the row under Confirm password keeps the
  room the line saying the two passwords differ takes there, said or not, so
  that the line lies over no box at any width and moves nothing; the phone
  bar's wordmark is the same link, named the same, both wordmark links reach
  44px to the touch on a coarse pointer, moving nothing, and opening the
  drawer focuses the Libraries heading's link, as before (the controller's
  calls). Looking at Rules again, the user ruled that its boxes are each as
  wide as what they hold rather than at a measure: the list as wide as its
  widest rule up to a cap, the rule's pane as its sentence or its form, and
  the preview at least as wide as the two together (the user's pick), wider
  only where a tab needs it; and that Export sits beside Import, with
  Import's help under the three. Looking at it with a rule picked, the user
  ruled that the four boxes under Sets are as wide as what they hold, 12
  characters, and that where the list and a picked rule's form do not both
  fit, the list gives way first, wrapping its names, so the form keeps its
  own width, the list reflowing when a rule is picked. The cap at the
  settings measure, the 20rem the rule's pane keeps for its sentence before
  the list gives way while no rule is picked, the list never giving way past
  its longest piece unless that piece is too wide for the cap or to leave the
  rule's pane 14rem, the two side by side only where Rules has 36rem, room
  for the seeded rules' list whole beside that 14rem under either pointer
  from a 17px root font up, so that a larger text size makes Rules one column on
  windows wider than the breakpoint, and the note's row of its own are the
  controller's calls. Rules' and About's failures say what was not done and
  then why. On `/account` a refused Create or Change password is said in the
  server's own words, as what was not done and the status where it has none,
  or in the browser's words where no answer came, and a revocation's or a
  sign-out's always says first what was not done. The licence page says its
  texts were not loaded, never in the browser's words. A Revoke whose row
  goes hands its focus to the nearest row left (spec §4). The user found the
  Configuration screens hard to read and likes Stump's configuration UI,
  whose motifs are carried over, not copied.

- **A version label, an About page and a licence page. The sidebar and
  readability spec's second PR shipped on 2026-10-20** on `about-licence`;
  the spec's §4 is the record, and this entry now says what shipped. The
  build label's PR, on `build-label` (#207, merged 2026-10-27), made a build
  without `TAIYAKU_COMMIT` name git's HEAD, with a `*` where tracked files
  differed from it (spec `2026-10-26-build-label-design.md`).
  **`GET /api/about`** is one of `routes::guarded_routes`, so a request to
  it needs a session cookie, or a password or an API key over HTTP Basic.
  It answers the build's commit and `dirty`, the repository, the OPDS
  catalog's path and each format taiyaku writes, the five `<stem>` files
  next to each other, each with its version and a sentence saying what it
  is; `<stem>.notes.json`, the HTTP API and the wasm module are unversioned
  (`about.rs`). **The commit** is compiled in through `option_env!` from
  three values, and `about::build_label` decides which wins:
  `TAIYAKU_COMMIT` when it is a full 40-character lowercase hash, never
  marked; otherwise git's HEAD, which `taiyaku-serve`'s build script hands
  over as `TAIYAKU_GIT_HEAD`, marked when the script also set
  `TAIYAKU_GIT_DIRTY`; otherwise no commit. Only a full hash counts for
  either, so an invalid `TAIYAKU_COMMIT` falls through to HEAD.
  **`dirty`** is true only beside a commit from HEAD: the script sets
  `TAIYAKU_GIT_DIRTY` when
  `git status --porcelain --untracked-files=no` prints anything, so
  untracked files never count. It runs git with `GIT_OPTIONAL_LOCKS=0`, so
  `git status` does not refresh the index it watches, and it reports
  nothing where the workspace is not the top of its own repository, so a
  source tree unpacked inside another repository does not take that one's
  HEAD (`build.rs`). **The commit and the mark are worked out again** only
  when the script runs again: when HEAD, HEAD's reflog, the branch's ref
  (or, where the branch has no ref file of its own, the directory under
  `refs/heads` that will hold it), `packed-refs`, the `reftable` format's
  `tables.list` or git's index changes (a commit, a checkout, a stage), or
  a crate's `src` or `Cargo.toml` or the workspace's `Cargo.toml` or
  `Cargo.lock` does, not on every build, which would recompile the crate
  every time. So an edit anywhere else, under `web/`, `docs/` or a crate's
  `tests/` say, can go unseen until then, and an edit undone can keep its
  `*`. Git's paths are watched whenever the workspace is the top of its
  repository, whether or not HEAD gave a commit, so a first commit after
  `git init`, and a commit that writes no index on a branch whose ref is
  only in `packed-refs`, are both seen. Where git is missing, refuses the
  repository, or there was no repository yet, none of git's state is
  watched, so once git can answer, the label reads "unknown build" until one of those Rust files
  changes. Cargo
  also rebuilds the crate when `TAIYAKU_COMMIT` changes. The image's build
  context holds no `.git`, so there the script finds no repository and
  `TAIYAKU_COMMIT` is the only source: the Dockerfile's server stage takes
  it as a build argument, and CI's docker job passes the full commit to
  both its builds and checks the image's binary holds it (`docs/ci.md`,
  "The image job"). **The
  repository** is now `https://github.com/Davey-Hughes/taiyaku`, in
  `Cargo.toml`'s `repository`, which the route reads as
  `CARGO_PKG_REPOSITORY`; it named the forge before. The GitHub URL
  answered 404 to an anonymous request on 2026-10-18, so About's source
  link works once the GitHub repository is public, and its commit link
  once that repository also holds the commit.
  **The label** at the top of the sidebar's foot, above More, in the column
  and the drawer, is the short commit, with a `*` after it when `dirty`
  (`abc1234*`, its link named "About, build abc1234, with uncommitted
  changes"), or "unknown build", a link to `/about` for every account
  (`VersionLabel.tsx`). **`/about`** shows the commit, linked to
  `<repository>/commit/<hash>` when there is one and followed by " with
  uncommitted changes" when `dirty`, the source link, the
  OPDS catalog's address, the format table, with each format's sentence
  under its name, and taiyaku's licence and the fonts', each a link to its
  own on the licence page, the fonts' to the first font's (`About.tsx`).
  **The catalog's address** is the page's origin before the path the route
  answers, on About and beside the OPDS
  line on Account, with a Copy address button, described by the address,
  only in a secure context that has `navigator.clipboard`; elsewhere it is
  text to select (`CatalogAddress.tsx`). **`/about/licence`** says taiyaku is
  GPL-3.0-or-later and that Shippori Mincho B1, Zen Kaku Gothic New and the
  marks subset are OFL-1.1, each above its full text, which
  `licenceTexts.ts` bundles from its file and the app imports only from
  that page (`Licence.tsx`); the Dockerfile's web stage copies `LICENSE` in
  for it. A list of contents at its top links to each licence by its
  heading's fragment, and a load of the page with one lands on that
  heading. *As filed:* the
  same spec's second PR (§4): the short commit as the version, built in
  through `TAIYAKU_COMMIT`; `GET /api/about`; `/about`, with the format
  versions and the OPDS catalog address; and `/about/licence`.

- **More in the sidebar.** Items the user adds to it, or more quick items
  in it: the user noted that the trimmed sidebar leaves room for both. Left
  out of the sidebar and readability spec (§0).

- **A version field for `<stem>.notes.json`**, the one sidecar file
  without one. Left out of the same spec (§0); its About page lists the file
  as unversioned.

- **Licence notices for every Rust and npm dependency, generated at build.**
  Left out of the same spec (§0), whose licence page names taiyaku's own
  licence and the fonts'.

- **Semantic versioning.** The short commit is the version "for now", in the
  user's words; left out of the same spec (§0).

## Corpus

Eleven series with both a raw and at least one English edition. Held on a NAS, never
committed, located at build time. Beyond the four the design started with, the
material now covers repagination at 2:1 and 4:1, editions published entirely as
joined spreads, letterboxed scans, archival TIFFs at 430 MB per page, three
independent scans of one volume, and at least two files that are not what their
names claim.

It now also holds **negative** material, which it lacked: `incoming/needles/`
keeps four short unrelated pairings that the solver accepts and should not. A
corpus of things that ought to work can only measure one half of a guard rail.

The awkward material is the valuable material. Every guard rail in the solver exists
because something in this corpus broke without it.
