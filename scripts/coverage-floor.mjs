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
// 7169 lines, 6735 covered, 93.95%. A module can go entirely dark and the
// aggregate still passes as long as it accounts for **67 covered lines or
// fewer**, which is 17 of the 43 files in the report.
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
        yield { file: file.replace(/^.*\/web\//, 'web/'), pcts }
      }
    },
  },
  {
    name: 'cargo llvm-cov json export',
    tree: 'the Rust workspace',
    detect: (report) => Array.isArray(report?.data?.[0]?.files),
    // **43 files measured on a clean tree, across five crates; eleven of
    // margin.** The count can only fall three ways: the
    // `--ignore-filename-regex` widens, a crate leaves the workspace, or files
    // are merged. Eleven files are excluded by name today — counted from the
    // reports, against nine alternations in the regex, because `/corpus` matches
    // two and `test_support` matches two — so the margin leaves room for that
    // list to grow a little without a coverage edit becoming a gate edit.
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
    // It trips when a crate of twelve files or more leaves the report, which
    // today is `taiyaku-serve` at 17 or `taiyaku-cli` at 14 but not
    // `taiyaku-core` at 8 — and this is also the check ci.yml's exclusion
    // comment used to ask a *reader* to perform by hand ("check the file count
    // when adding a crate").
    minFiles: 32,
    // **Lines only, and the other three are measured refusals, not oversights.**
    // `branches` is `count: 0` on all 43 files — llvm-cov emits no branch data
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
      // Every one of those figures fell on 2026-08-14 without a line of
      // implementation changing, because that is when the `#[cfg(test)] mod
      // tests` blocks left these files and stopped counting as their coverage.
      // The three largest drops were taiyaku-serve/src/lib.rs (90.91% →
      // 77.85%), taiyaku-cli/src/credits.rs (93.13% → 82.51%) and
      // taiyaku-core/src/regions.rs (95.65% → 85.71%). The floor was left at 57
      // rather than re-derived: it is a collapse detector, not a regression
      // detector, and 16 points of slack under the worst file is what it needs
      // to be. See the note at the top of this file.
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
        yield { file: entry.filename.replace(/^.*\/(crates\/)/, '$1'), pcts }
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
