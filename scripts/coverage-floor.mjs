#!/usr/bin/env node
// The half of both coverage gates that neither tool's own thresholds can
// express: a floor **per file**, and a minimum **number** of files.
//
// --- why a per-file floor at all (the web side) -------------------------------
//
// `web/vite.config.ts`'s thresholds are **aggregate**, and an aggregate cannot
// see a single module go dark. Measured on this repository: skipping the whole
// of `pairing.test.ts` takes `pairing.ts` from 100% to 83.58% while the total
// moves 98.03% → 97.49%, because `App.test.tsx` exercises the same lines
// transitively through a mounted reader. Every aggregate threshold still
// passed. A developer could delete a unit suite and the gate would say nothing.
//
// vitest cannot do both halves: a glob entry in `thresholds` is aggregated over
// the files it matches, and `perFile` inside a glob entry is ignored. Both were
// tried and measured before this file was written. `perFile: true` at the top
// level does apply per file, but then one number has to serve both roles, and
// the aggregate bar (96) is far above the lowest file's honest figure
// (`fixtures.ts`, 93.10%) — so it would fail on a clean tree.
//
// --- and the Rust side, which had the same hole ------------------------------
//
// `cargo llvm-cov --fail-under-lines 93` is workspace-aggregate and has exactly
// the same blind spot. Measured 2026-08-14, after the test modules moved into
// their own files and the two `test_support` modules joined the exclusion list:
// 7169 lines, 6735 covered, 93.95%. A module could go entirely dark and the
// aggregate still passed as long as it accounted for **67 covered lines or
// fewer**, which was 17 of the 43 files in the report.
//
// **That slack is now derived rather than written down, and this run prints
// it.** `covered - ceil(93% of total)`, and every file at or under it, come out
// of the same report this script already walks — see `reportAggregate` at the
// bottom of this file, and `docs/ci.md` for what the figure means and one dated
// measurement of it. The figures used to be restated here, in `check-all.sh`,
// in `docs/ci.md` and in `docs/status.md`, with the at-or-under file list
// spelled out twice; nothing derived any of the four, and they were three
// commits stale by the time anyone looked (delta review M9). A note that
// re-derives itself on every run cannot be.
//
// The shape of the number is the part worth carrying in prose, because it is
// what makes the per-file floor below necessary: the slack **widens as coverage
// rises**, so a branch that improves coverage enlarges the set of modules the
// aggregate would let go entirely dark. It was 8 lines at one commit on
// 2026-08-17 — briefly no blind spot at all — and 64 lines and 20 files a few
// commits later. **This file's own threshold is the part that does not move
// with the margin**, which is exactly why the aggregate alone was never enough.
//
// **That blind spot used to be an order of magnitude wider, and the reason is
// worth knowing.** Until this branch every `#[cfg(test)] mod tests` lived inline
// in the file it tested, so its own lines — executed by definition, and
// therefore ~100% covered — were counted as that file's coverage. The report
// read 19486 lines at 97.40%, of which roughly 12000 were test code measuring
// itself, and a module of up to **857** covered lines could go dark unnoticed
// (39 of the 44 files then in the report). cargo-llvm-cov 0.8.7 drops a source
// file from the report when its basename contains `tests`, with no flag
// involved — demonstrated by two files in one directory, where
// `stump/http/test_support.rs` is present and `stump/http/ureq_tests.rs` is
// not — so moving the modules out took the test code with them and left the
// figure measuring implementation only.
//
// Two `#[cfg(test)] pub(crate) mod test_support` modules survived that by
// accident — `taiyaku-serve/src/test_support.rs` and
// `stump/http/test_support.rs`, no `s`, so the default exclusion does not see
// them — and are named in the `--ignore-filename-regex` instead, which is why
// that list holds eleven files rather than nine. They were counted before the
// split too, folded into `lib.rs` and `http.rs`; excluding them explicitly cost
// 252 lines and 0.19 points (94.14% over 45 files → 93.95% over 43) and makes
// one rule true of the whole report rather than nearly all of it: no
// `#[cfg(test)]` code is measured.
//
// The threshold stayed at 93 across that shift rather than being re-derived
// from the new figure less two points, which would have given 92. It now leaves
// 0.95 points — **68 lines** — where it used to leave 4.40 points and 857
// lines, so the same number bites about twelve times harder: a branch that adds
// a module of any size materially untested is now red, and was not before.
//
// **That margin has about five lines of noise in it, and the noise was measured
// rather than assumed.** Two runs of the same command on the same tree gave
// 6735 and 6740 covered lines — 93.95% and 94.02% — and the difference is
// entirely one file: `taiyaku-serve/src/cover.rs`, lines 257-260, which are
// `Semaphore::acquire`'s `while *available == 0 { … wait … }` contention branch.
// They are covered when two decodes actually collide on the permit and
// uncovered when the scheduler happens not to overlap them. So the figures
// quoted here are the lower of the two runs, and the honest margin is 67 lines
// rather than 68.
//
// The two-point rule was not applied on top of that, and the reason it gives
// does not transfer: docs/ci.md derives it from v8 counting what a run loaded,
// so an unrelated import moves the *web* figures by fractions. The Rust noise
// above is 0.07 points against a 0.95-point margin — real, named, and an order
// of magnitude short of the threshold. If the gate does start costing honest
// branches their afternoon, 92 is the number the rule would have given.
//
// **`cargo llvm-cov` has a `--fail-under-file-lines` flag and it is not used,
// for a measured reason.** It works — it exits 1 — but it says *nothing*: with
// `--fail-under-file-lines 80` against this tree it wrote the ordinary coverage
// table to stdout, zero bytes to stderr, and named neither the file that tripped
// it nor the threshold it tripped. In CI that is a red step above a 43-row table
// with no indication which row is the reason. Checked here instead, where the
// failure can name the file and say what to open first.
//
// --- one script, two report formats ------------------------------------------
//
// A sibling script was the alternative. This is one file because the two checks
// are the same idea twice — same floor comparison, same minimum-file-count, same
// thing to say when they fail — and only the adapter differs: how to enumerate
// files, which metrics are trustworthy, and which config a reader should open
// first. Two files would have meant that prose twice and two chances to fix a
// hole in only one of them.
//
// --- the minimum file count --------------------------------------------------
//
// Both gates used to pass on a report containing no files at all. This script
// printed "0 files, all above the per-file floor" and exited 0, and the
// aggregate half passed too: with `coverage.include` pointed at a directory that
// does not exist, vitest prints `Unknown% ( 0/0 )` for all four metrics and
// **exits 0** — measured, by doing it. So a typo in one glob left both halves
// green while measuring nothing, which is precisely the failure the comment
// above that block says the design exists to prevent. Hence `minFiles` below,
// per format, with its arithmetic beside it.
//
// Three layers, and it is worth being plain about which catches what: the
// aggregate thresholds catch the tree regressing overall; the per-file floors
// catch a file that is *in* the report and has stopped being tested; the file
// count catches files that have left the report entirely. Only the third can see
// a collapse, because a file that is absent is a file no per-file check visits.
import { readFileSync } from 'node:fs'

/**
 * The two report formats, detected by shape rather than by filename — the path
 * is an argument and could be anything.
 */
const FORMATS = [
  {
    name: 'istanbul json-summary',
    tree: 'web/src',
    // Detected on `total.lines.total` and **not** on `total.lines.pct`, which
    // was the first attempt and was wrong in exactly the case this gate is here
    // for: on a report with no files, istanbul writes `"pct": "Unknown"` — the
    // string — so a `typeof pct === 'number'` sniff rejected the collapsed
    // report as "not an istanbul summary". It still failed, but it failed
    // saying the wrong thing. `total` is a count and is `0` there.
    detect: (report) => typeof report?.total?.lines?.total === 'number',
    // **66 files measured on a clean tree, 2026-08-14; seven of margin.** The
    // margin has only to cover a deliberate exclusion, and the block this guards
    // names its exclusions one file at a time on purpose — three named files and
    // one directory today, beside three test-and-declaration wildcards. Counted
    // rather than recalled: an earlier version of this comment said "three
    // entries and one directory", which matched no reading of the list.
    //
    // Seven rather than seventeen, because the failure to catch is not only the
    // bare collapse. Measured, by mutating `coverage.include` and re-running:
    //
    //   ['nope/**/*.ts']       →  0 files. Vitest prints `Unknown% ( 0/0 )` for
    //                             all four metrics and **exits 0**.
    //   ['src/*.ts']           → 52 files. One deleted `*`, and all 14 files of
    //                             `src/modules` leave the gate. Vitest reports
    //                             97.20/93.52/99.00/99.09 — every threshold
    //                             passed, because a file that is absent is a
    //                             file no per-file check visits.
    //   ['src/**/*.ts']        → 66 files, unchanged. Dropping the `.tsx` glob
    //                             costs nothing: vitest's coverage `include`
    //                             resolves `*.ts` across its extension list, so
    //                             it still matches `.tsx`. Assumed otherwise
    //                             first; measured, and it was wrong.
    //
    // **This number was 50 and had gone stale into a hole, which is the thing it
    // exists to prevent.** It was derived when the clean tree held 57 files, so
    // it tolerated seven lost and caught the 44-file near-miss above. web/src has
    // since grown to 66 and the near-miss with it, to 52 — above 50, so the
    // second row of that table passed this check on 2026-08-14: fourteen files
    // left the gate, every vitest threshold stayed green, and so did this script.
    // Re-derived by re-running the experiment rather than by scaling the old
    // number: 66 measured, seven of margin, **59**, which catches 52 with room.
    //
    // The lesson is in the arithmetic, not the number. A fixed `minFiles` against
    // a growing tree loosens on its own, silently, and nothing here derives it —
    // so re-run the three rows above when web/src grows by roughly ten files, and
    // treat "the near-miss still trips it" as the property to check rather than
    // "the count is still above the floor".
    //
    // If a deliberate exclusion ever does trip this, that is the check working:
    // re-derive the number beside the exclusion rather than widening it on
    // reflex.
    minFiles: 59,
    /**
     * Two points below the lowest measured figure for each metric — the rule
     * these were set by, and no longer quite true of all four. Re-measured
     * 2026-08-14: the margins are now 0.85 statements, 1.41 branches, 2.33
     * functions and 1.02 lines. They are left where they are because every one
     * still passes and lowering a floor to restore a margin is how a gate stops
     * gating; `statements` is the one to watch, and the fix when it fires is a
     * test for `Labeller.tsx` rather than an 89 here.
     */
    floors: {
      statements: 91, // lowest measured: Labeller.tsx, 91.85
      branches: 78, // lowest measured: api.ts, 79.41
      functions: 91, // lowest measured: decode.ts, 93.33
      lines: 92, // lowest measured: api.ts, 93.02
    },
    check: 'the `coverage.include` and `coverage.exclude` lists in web/vite.config.ts',
    *entries(report) {
      for (const [file, metrics] of Object.entries(report)) {
        if (file === 'total') continue
        const pcts = {}
        for (const metric of Object.keys(this.floors)) {
          const pct = metrics[metric]?.pct
          // Istanbul is not consistent about a metric with nothing in it: a
          // populated report carries `"pct": 100` (see `total.branchesTrue`,
          // which is 0/0 on every run here), while a collapsed one carries the
          // **string** `"Unknown"`. Both were measured. Skipping every
          // non-number is the only safe read, because `"Unknown"` coerced to a
          // number is `NaN` and `NaN < floor` is `false` — a naive comparison
          // would quietly pass such a file rather than flag it.
          if (typeof pct === 'number') pcts[metric] = pct
        }
        const { total, covered } = metrics.lines ?? {}
        yield {
          file: file.replace(/^.*\/web\//, 'web/'),
          pcts,
          lines: { count: total ?? 0, covered: covered ?? 0 },
        }
      }
    },
  },
  {
    name: 'cargo llvm-cov json export',
    tree: 'the Rust workspace',
    detect: (report) => Array.isArray(report?.data?.[0]?.files),
    // **53 files measured on a clean tree, across six crates; thirteen of
    // margin.** The count can only fall three ways: the
    // `--ignore-filename-regex` widens, a crate leaves the workspace, or files
    // are merged. Twelve files are excluded by name today — counted from the
    // reports, against the same nine alternations in the regex, because
    // `/corpus` now matches three (`taiyaku-cli`, `taiyaku-serve` and the new
    // `taiyaku-library`) and `test_support` still matches two — so the margin
    // leaves room for that list to grow a little without a coverage edit
    // becoming a gate edit.
    //
    // It read 44 immediately before 2026-08-14 — the 38 this comment used to
    // claim was itself four crates and six files stale. Moving the test modules
    // into their own files then moved the count three ways at once: the two
    // `test_support` modules became rows of their own (+2), `taiyaku-read/src/lib.rs`
    // left the report entirely (−1) because once its test module was elsewhere
    // the file held nothing but `mod` declarations, a `pub use` and a `const`,
    // and llvm-cov records no coverage for a file with no executable lines, and
    // then the same two `test_support` rows were excluded by name (−2). Net −1,
    // 44 to 43. The 50 new `tests.rs` files do not appear at all — see the note
    // at the top of this file.
    //
    // **Re-measured 2026-08-16, after the library store landed as a sixth
    // crate.** 43 → 52: `taiyaku-library` contributes 8 measured files
    // (`corpus.rs` excluded, joining the `/corpus` alternation above) and
    // `taiyaku-cli` gained `library.rs`, the CLI half of `library import|export`
    // (14 → 15); the remaining difference is the two smallest crates, unrelated
    // to this change.
    //
    // **Re-measured again 2026-08-17**, after the same review's C3 fix added
    // `taiyaku-library/src/migrate.rs`: 52 → 53, the whole difference being
    // that one file. Counted directly from the report rather than reasoned
    // about, the same way the paragraph above insists on: `taiyaku-cli` 15,
    // `taiyaku-core` 8, `taiyaku-library` 9, `taiyaku-read` 3, `taiyaku-serve`
    // 16, `taiyaku-wasm` 2.
    //
    // It trips when a crate of fourteen files or more leaves the report, which
    // today is `taiyaku-serve` at 16 or `taiyaku-cli` at 15, but not
    // `taiyaku-library` at 9 or `taiyaku-core` at 8 — and this is also the
    // check ci.yml's exclusion comment used to ask a *reader* to perform by
    // hand ("check the file count when adding a crate").
    minFiles: 40,
    // The **aggregate** line threshold, which this script does not enforce —
    // `cargo llvm-cov --fail-under-lines` does, one command earlier. It is here
    // only to derive the slack printed at the end of a run: `covered - ceil(93%
    // of total)`, the number of covered lines a module could lose with the
    // aggregate still green, and therefore the size of module the aggregate is
    // structurally blind to.
    //
    // **A mirrored number, and checked rather than trusted** — see
    // `aggregateFloorFrom` below and `assertAggregateFloorMatches`. Four
    // documents used to state that slack, and the eleven-file list under it,
    // as a present-tense fact, with nothing deriving any of them; a run now
    // prints the current figures and the prose is one dated measurement in
    // `docs/ci.md` (delta review M9).
    aggregateFloor: 93,
    aggregateFloorFrom: 'scripts/check-all.sh',
    // **Lines only, and the other three are measured refusals, not oversights.**
    // `branches` is `count: 0` on all 53 files — llvm-cov emits no branch data
    // without a nightly flag — so any floor on it would fail every file.
    // `regions` and `instantiations` read far below lines on `match`-heavy and
    // generic code (lowest measured 2026-08-14: 72.22% for regions on
    // `taiyaku-serve/src/truth.rs`, and 25.00% for instantiations on
    // `stump/cache.rs`), so they would need their own numbers for no gain the
    // line floor does not already give. Lines is also what `--fail-under-lines`
    // uses, for the reason recorded in ci.yml.
    floors: {
      // **Well under the lowest measured, and loose by construction.**
      // Lowest on a clean tree, 2026-08-14: taiyaku-serve/src/truth.rs 73.68%,
      // then taiyaku-serve/src/lib.rs 77.85%, taiyaku-cli/src/credits.rs
      // 82.51%, taiyaku-core/src/regions.rs 85.71%, stump/resolve.rs 87.61%.
      //
      // *Re-measured 2026-08-16, after the library store moved progress, notes
      // and truth onto rows: the worst five changed with them, not the floor.*
      // `taiyaku-serve/src/truth.rs` went from the worst file to 100% — it is
      // now a thin projection over `taiyaku-library`'s rows — and the new
      // worst file is the crate that replaced it: taiyaku-library/src/lib.rs
      // 73.81%, then taiyaku-serve/src/lib.rs 78.12%, taiyaku-cli/src/credits.rs
      // 82.51% (unmoved), taiyaku-serve/src/works.rs 84.66%, taiyaku-core/src/regions.rs
      // 85.71% (also unmoved). 73.81% is almost exactly where 73.68% was, so the
      // floor's 16-point margin does not move: 73.81 − 16 = 57.81, still 57 —
      // this is the re-derivation the note at the top of this file asks for,
      // done and landing on the same number rather than skipped because it
      // looked unnecessary.
      //
      // *Re-measured 2026-08-17, after the same review's Minor fixes.* The
      // worst five are now taiyaku-serve/src/lib.rs 78.13%,
      // taiyaku-cli/src/credits.rs 82.51% (unmoved for a third measurement),
      // taiyaku-library/src/lib.rs 84.00%, taiyaku-serve/src/works.rs 84.56%
      // and taiyaku-core/src/regions.rs 85.71% (also unmoved).
      // `taiyaku-library/src/lib.rs` left the bottom, 73.81% (31/42) →
      // 84.00% (42/50): it grew and its new lines are covered. The two
      // commits that touched it between the measurements are `8d2c196`
      // (I1's FTS5 probe) and `caa534f` (the busy-timeout lock); which of
      // them moved it was not separated, and the figure is quoted rather
      // than attributed.
      //
      // The margin under the worst file is now 21 points rather than 16,
      // which loosens the floor rather than threatening it. 57 stays: raising
      // it toward the worst file is how a collapse detector turns into a
      // regression detector nobody asked for.
      //
      // Every one of the 2026-08-14 figures fell without a line of
      // implementation changing, because that is when the `#[cfg(test)] mod
      // tests` blocks left these files and stopped counting as their coverage.
      // The three largest drops were taiyaku-serve/src/lib.rs (90.91% →
      // 77.85%), taiyaku-cli/src/credits.rs (93.13% → 82.51%) and
      // taiyaku-core/src/regions.rs (95.65% → 85.71%). The floor was left at 57
      // rather than re-derived that time: it is a collapse detector, not a
      // regression detector, and 16 points of slack under the worst file is
      // what it needs to be. See the note at the top of this file.
      //
      // So it does **not** catch a module sliding from 95% to 80%; the spread
      // between the best-covered file and the worst is too wide for one number
      // to do that. What it catches is a module that stops being tested *at
      // all* — the failure `--fail-under-lines` structurally cannot see, since
      // a 400-line file going dark moves the workspace total by half a point.
      lines: 57,
    },
    check: 'the `--ignore-filename-regex` in scripts/check-all.sh and .forgejo/workflows/ci.yml',
    *entries(report) {
      for (const entry of report.data[0].files) {
        const pcts = {}
        for (const metric of Object.keys(this.floors)) {
          const { count, percent } = entry.summary[metric] ?? {}
          // Unlike istanbul, llvm-cov reports 0% — not 100% — for a metric with
          // nothing in it, so a zero count has to be skipped or every such file
          // is a false alarm.
          if (count > 0 && typeof percent === 'number') pcts[metric] = percent
        }
        const { count, covered } = entry.summary.lines ?? {}
        yield {
          file: entry.filename.replace(/^.*\/(crates\/)/, '$1'),
          pcts,
          lines: { count: count ?? 0, covered: covered ?? 0 },
        }
      }
    },
  },
]

const path = process.argv[2] ?? 'web/coverage/coverage-summary.json'

let report
try {
  report = JSON.parse(readFileSync(path, 'utf8'))
} catch (err) {
  console.error(`coverage-floor: cannot read ${path} — run the coverage suite first`)
  console.error(String(err))
  process.exit(2)
}

const format = FORMATS.find((candidate) => candidate.detect(report))
if (format === undefined) {
  console.error(`coverage-floor: ${path} is neither an istanbul json-summary nor a cargo`)
  console.error('llvm-cov json export. Nothing was checked, so this is a failure rather than a')
  console.error('pass: a gate that cannot read its own input must not report green.')
  process.exit(2)
}

const files = [...format.entries(report)]

// Ahead of the floors, deliberately. If the report collapsed then every
// per-file finding below it is noise, and the count is the real story.
if (files.length < format.minFiles) {
  console.error(
    `coverage-floor: ${format.name} lists ${files.length} file(s) for ${format.tree}, fewer than the ${format.minFiles} this gate requires.`,
  )
  console.error(`\nCheck ${format.check} first.`)
  console.error('That is what decides which files are measured at all, and a file missing from')
  console.error('the report is a file this gate and the aggregate one both silently pass over.')
  console.error('The check exists because a report with no files in it used to take both halves')
  console.error('green while measuring nothing.')
  process.exit(1)
}

const failures = []
for (const { file, pcts } of files) {
  for (const [metric, floor] of Object.entries(format.floors)) {
    const pct = pcts[metric]
    if (typeof pct !== 'number') continue
    if (pct < floor) failures.push(`  ${file}  ${metric} ${pct.toFixed(2)}% < ${floor}%`)
  }
}

if (failures.length > 0) {
  console.error(`coverage-floor: ${failures.length} file/metric pair(s) below the per-file floor:`)
  console.error(failures.join('\n'))
  console.error('\nThe aggregate thresholds can pass while one module goes untested — that is')
  console.error('what this check is for. Raise the coverage, or — if the drop is deliberate')
  console.error('and defensible — move the floor and say why.')
  process.exit(1)
}

console.log(
  `coverage-floor: ${files.length} files (floor requires ${format.minFiles}), all above the per-file floor — ${format.name}`,
)
reportAggregate(format, files)

/**
 * The aggregate figures, derived from the same report this gate just walked.
 *
 * **The point is that nothing here is written down anywhere.** `docs/ci.md`,
 * `docs/status.md`, `scripts/check-all.sh` and this file each used to carry
 * "9628 lines, 9000 covered, 93.48%, 53 files, a slack of 45, eleven at or
 * under it", plus the eleven-file list twice — four hand-maintained copies of
 * one measurement, in a repository that has already been bitten by exactly that
 * (`docs/status.md`'s test count, twice in one day, which is what
 * `scripts/status-test-count.mjs` exists for). They are all derivable from the
 * JSON already on disk, so they are derived, and the prose is one dated
 * measurement rather than four live claims (delta review M9).
 *
 * The **slack** is the interesting figure and the reason this whole script
 * exists: `covered - ceil(floor% of total)` is how many covered lines the
 * workspace could lose with the aggregate threshold still green, so every file
 * at or under it is a file that could go entirely dark unnoticed by
 * `--fail-under-lines`. It is printed, not gated: the number moves every commit
 * and by a few lines of measurement noise besides (see the `cover.rs`
 * `Semaphore` note above), so a threshold on it would be red for reasons that
 * are not defects. What is gated is the per-file floor above, which does not
 * move with the margin.
 *
 * Silent for a format with no `aggregateFloor` — the web side's aggregate is
 * four per-metric vitest thresholds rather than one line figure, and a "slack"
 * over four of them would not mean anything.
 */
function reportAggregate(format, files) {
  const total = files.reduce((n, f) => n + f.lines.count, 0)
  const covered = files.reduce((n, f) => n + f.lines.covered, 0)
  if (total === 0) return
  const pct = ((covered / total) * 100).toFixed(2)
  if (typeof format.aggregateFloor !== 'number') {
    console.log(`coverage-floor: ${total} lines, ${covered} covered, ${pct}%`)
    return
  }
  assertAggregateFloorMatches(format)
  const slack = covered - Math.ceil((format.aggregateFloor / 100) * total)
  const under = files
    .filter((f) => f.lines.covered > 0 && f.lines.covered <= slack)
    .sort((a, b) => a.lines.covered - b.lines.covered)
  console.log(
    `coverage-floor: ${total} lines, ${covered} covered, ${pct}% — a slack of ${slack} covered ` +
      `lines under the ${format.aggregateFloor}% aggregate, and ${under.length} of ${files.length} ` +
      `file(s) at or under it`,
  )
  if (under.length > 0) {
    console.log(
      `  ${under.map((f) => `${f.file} (${f.lines.covered})`).join(', ')}`,
    )
  }
}

/**
 * The mirrored aggregate floor, checked against the file that actually applies
 * it.
 *
 * A mirror nobody checks is the fifth copy of the problem this function's
 * caller exists to remove. `check-all.sh` and `.forgejo/workflows/ci.yml` carry
 * the real `--fail-under-lines N` byte for byte; if the number there moves and
 * this one does not, every slack figure printed afterwards is quietly wrong.
 *
 * Exit 2 rather than 1, the same distinction `status-test-count.mjs` draws: the
 * coverage that was measured is not in question, this script's own constant is,
 * and those need different fixes. A missing or unreadable `check-all.sh` is
 * also exit 2 — a check that cannot read its own input must not report green.
 */
function assertAggregateFloorMatches(format) {
  let text
  try {
    text = readFileSync(format.aggregateFloorFrom, 'utf8')
  } catch (err) {
    console.error(
      `coverage-floor: cannot read ${format.aggregateFloorFrom} to check the aggregate floor`,
    )
    console.error(String(err))
    process.exit(2)
  }
  const stated = text.match(/--fail-under-lines\s+(\d+)/)
  if (stated === null) {
    console.error(
      `coverage-floor: ${format.aggregateFloorFrom} no longer states a --fail-under-lines figure.`,
    )
    console.error('Either the gate moved or it was reworded; the slack below cannot be derived.')
    process.exit(2)
  }
  if (Number(stated[1]) !== format.aggregateFloor) {
    console.error(
      `coverage-floor: the aggregate floor mirrored here (${format.aggregateFloor}) is not the ` +
        `one applied by ${format.aggregateFloorFrom} (${stated[1]}).`,
    )
    console.error('Fix: change `aggregateFloor` in this file to match, so the printed slack is real.')
    process.exit(2)
  }
}
