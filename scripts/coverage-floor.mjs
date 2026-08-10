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
// the same blind spot, larger because the workspace is larger. Measured: 15041
// lines, 14402 covered. If `taiyaku-read/src/pages.rs` (432 lines, 406 covered)
// lost every one of its tests, the workspace lands at 13996/15041 = 93.05% and
// the gate stays green. Any module up to roughly 410 covered lines can go
// entirely dark unnoticed.
//
// **`cargo llvm-cov` has a `--fail-under-file-lines` flag and it is not used,
// for a measured reason.** It works — it exits 1 — but it says *nothing*: with
// `--fail-under-file-lines 80` against this tree it wrote the ordinary coverage
// table to stdout, zero bytes to stderr, and named neither the file that tripped
// it nor the threshold it tripped. In CI that is a red step above a 37-row table
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
    // **57 files measured on a clean tree; seven of margin.** Growth only ever
    // moves this the safe way, so the margin has only to cover a deliberate
    // exclusion, and the block this guards names its exclusions one file at a
    // time on purpose — two named files and one directory today, beside three
    // test-and-declaration wildcards. Counted rather than recalled: an earlier
    // version of this comment said "three entries and one directory", which
    // matched no reading of the list.
    //
    // Seven rather than seventeen, because the failure to catch is not only the
    // bare collapse. Measured, by mutating `coverage.include` and re-running:
    //
    //   ['nope/**/*.ts']       →  0 files. Vitest prints `Unknown% ( 0/0 )` for
    //                             all four metrics and **exits 0**.
    //   ['src/*.ts']           → 44 files. One deleted `*`, and all 13 files of
    //                             `src/modules` leave the gate. Vitest reports
    //                             97.79/93.90/99.20/99.48 — every threshold
    //                             passed, and so did the per-file floor below,
    //                             because a file that is absent is a file no
    //                             per-file check visits.
    //   ['src/**/*.ts']        → 57 files, unchanged. Dropping the `.tsx` glob
    //                             costs nothing: vitest's coverage `include`
    //                             resolves `*.ts` across its extension list, so
    //                             it still matches `.tsx`. Assumed otherwise
    //                             first; measured, and it was wrong.
    //
    // 50 catches the first two. 40 would have caught only the first.
    //
    // If a deliberate exclusion ever does trip this, that is the check working:
    // re-derive the number beside the exclusion rather than widening it on
    // reflex.
    minFiles: 50,
    /** Two points below the lowest measured figure for each metric. */
    floors: {
      statements: 91, // lowest measured: fixtures.ts, 93.10
      branches: 78, // lowest measured: api.ts, 80.76
      functions: 91, // lowest measured: decode.ts, 93.33
      lines: 92, // lowest measured: api.ts, 94.28
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
    // **37 files measured on a clean tree, across five crates; five of margin.**
    // The count can only fall three ways: the `--ignore-filename-regex` widens,
    // a crate leaves the workspace, or files are merged. Four files are excluded
    // by name today, so five leaves room for that list to grow a little without
    // a coverage edit becoming a gate edit.
    //
    // It trips when a crate of six files or more leaves the report —
    // `taiyaku-serve` is 15, `taiyaku-cli` 11, `taiyaku-core` 7 — and this is
    // also the check ci.yml's exclusion comment used to ask a *reader* to
    // perform by hand ("check the file count when adding a crate").
    minFiles: 32,
    // **Lines only, and the other three are measured refusals, not oversights.**
    // `branches` is `count: 0` on all 37 files — llvm-cov emits no branch data
    // without a nightly flag — so any floor on it would fail every file.
    // `regions` and `instantiations` read far below lines on `match`-heavy and
    // generic code (lowest measured: 66.80% and 39.39%, both
    // `taiyaku-serve/src/stump/http.rs`), so they would need their own numbers
    // for no gain the line floor does not already give. Lines is also what
    // `--fail-under-lines` uses, for the reason recorded in ci.yml.
    floors: {
      // **Two points under the lowest measured, and loose by construction.**
      // Lowest on a clean tree: taiyaku-serve/src/stump/http.rs, 59.44%; then
      // taiyaku-wasm/src/lib.rs 72.64%, taiyaku-cli/src/inventory.rs 75.82%,
      // taiyaku-serve/src/lib.rs 83.09%, taiyaku-read/src/naming.rs 93.02%.
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
