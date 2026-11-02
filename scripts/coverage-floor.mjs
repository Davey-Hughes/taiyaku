#!/usr/bin/env node
// The half of both coverage gates that neither tool's own thresholds can
// express: a floor **per file**, and a minimum **number** of files.
//
// --- why a per-file floor at all (the web side) -------------------------------
//
// `web/vite.config.ts`'s thresholds are **aggregate**, and an aggregate cannot
// see a single module go dark. Skipping the whole of `pairing.test.ts` leaves
// the total almost unchanged, because `App.test.tsx` exercises the same lines
// transitively through a mounted reader, and every aggregate threshold still
// passes. A developer could delete a unit suite and the gate would say nothing.
//
// vitest cannot do both halves: a glob entry in `thresholds` is aggregated over
// the files it matches, and `perFile` inside a glob entry is ignored.
// `perFile: true` at the top level does apply per file, but then one number
// has to serve both roles, and the aggregate bar (96) is far above the lowest
// file's honest figure, so it would fail on a clean tree.
//
// --- and the Rust side, which has the same hole ------------------------------
//
// `cargo llvm-cov --fail-under-lines 93` is workspace-aggregate and has exactly
// the same blind spot: a module could go entirely dark and the aggregate still
// pass, as long as it accounted for no more covered lines than the slack below.
//
// **That slack is derived rather than written down, and this run prints
// it.** `covered - ceil(93% of total)`, and every file at or under it, come out
// of the same report this script already walks — see `reportAggregate` at the
// bottom of this file, and `docs/ci.md` for what the figure means and one dated
// measurement of it.
//
// The shape of the number is the part worth carrying in prose, because it is
// what makes the per-file floor below necessary: the slack **widens as coverage
// rises**, so a branch that improves coverage enlarges the set of modules the
// aggregate would let go entirely dark. **This file's own threshold is the
// part that does not move with the margin**, which is exactly why the
// aggregate alone is not enough.
//
// The test modules live in their own files, and cargo-llvm-cov 0.8.7 drops a
// source file from the report when its basename contains `tests`, with no flag
// involved. A `test_support` module has no `s`, so that default exclusion does
// not see it: those modules are named in the `--ignore-filename-regex` instead.
//
// The two-point rule, the measured figure less two points, is not applied to
// the aggregate threshold of 93: docs/ci.md derives that rule from v8 counting
// what a run loaded, so an unrelated import moves the *web* figures by
// fractions, and the reason does not transfer. If the gate does start costing
// honest branches their afternoon, 92 is the number the rule would have given.
//
// **`cargo llvm-cov` has a `--fail-under-file-lines` flag and it is not used.**
// It works — it exits 1 — but it says *nothing*: it writes the ordinary
// coverage table to stdout and nothing to stderr, and names neither the file
// that tripped it nor the threshold it tripped. In CI that is a red step above
// a table with no indication which row is the reason. Checked here instead,
// where the failure can name the file and say what to open first.
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
// Without a minimum count, a report containing no files at all would pass both
// gates: this script would find nothing below the floor and exit 0, and the
// aggregate half passes too. With `coverage.include` pointed at a directory
// that does not exist, vitest prints `Unknown% ( 0/0 )` for all four metrics
// and **exits 0**. So a typo in one glob would leave both halves green while
// measuring nothing. Hence `minFiles` below, per format, with its arithmetic
// beside it.
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
    // Detected on `total.lines.total` and **not** on `total.lines.pct`: on a
    // report with no files, istanbul writes `"pct": "Unknown"` — the string —
    // so a `typeof pct === 'number'` sniff would reject the collapsed report
    // as not an istanbul summary, and the gate would fail saying the wrong
    // thing. `total` is a count and is `0` there.
    detect: (report) => typeof report?.total?.lines?.total === 'number',
    // **The clean report's file count, less seven of margin.** The margin has
    // only to cover a deliberate exclusion, and the `exclude` list in the block
    // this guards gives each file it names a reason.
    //
    // Seven rather than fifty, because the failure to catch is not only the bare
    // collapse. Mutating `coverage.include` and re-running gives:
    //
    //   ['nope/**/*.ts']       →  0 files. Vitest prints `Unknown% ( 0/0 )` for
    //                             all four metrics and **exits 0**.
    //   ['src/*.ts']           →  Every file of `src/modules` leaves the gate.
    //                             Vitest passes every threshold, because its
    //                             thresholds are checked against an average
    //                             over what remains and what left was well
    //                             covered; this script's
    //                             per-file floor passes too, because a file
    //                             that is absent is a file no per-file check
    //                             visits. `src/modules` is the only
    //                             subdirectory of `src` in the report, so this
    //                             row is what "drop a directory" means here.
    //   ['src/**/*.ts']        →  The clean tree's count. Dropping the `.tsx`
    //                             glob costs nothing: vitest's coverage
    //                             `include` resolves `*.ts` across its
    //                             extension list, so it still matches `.tsx`.
    //
    // The lesson is in the arithmetic, not the number. A fixed `minFiles` against
    // a growing tree loosens on its own, silently, and nothing here derives it —
    // so re-run the three rows above when web/src grows by roughly ten files, and
    // treat "the near-miss still trips it" as the property to check rather than
    // "the count is still above the floor". A count above the floor does not
    // show the floor still works: a clean tree passes the same whether the
    // near-miss would trip the floor or clear it by fifty files. `docs/ci.md`'s
    // `minFiles` section has the arithmetic.
    //
    // If a deliberate exclusion ever does trip this, that is the check working:
    // re-derive the number beside the exclusion rather than widening it on
    // reflex.
    minFiles: 161,
    /**
     * Two points below the lowest measured figure for each metric when they
     * were set. They are left where they are because every one still passes
     * and lowering a floor to restore a margin is how a gate stops gating.
     *
     * **The margins are the argument for re-deriving these rather than
     * reading them.** A file can sit within a branch or two of its floor and no
     * run says so while it passes.
     */
    floors: {
      statements: 91, // derived from Labeller.tsx 91.85
      branches: 78, // derived from api.ts 79.41
      functions: 91, // derived from decode.ts 93.33
      lines: 92, // derived from api.ts 93.02
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
          // **string** `"Unknown"`. Skipping every non-number is the only safe
          // read, because `"Unknown"` coerced to a number is `NaN` and
          // `NaN < floor` is `false` — a naive comparison would quietly pass
          // such a file rather than flag it.
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
    // The file count can only fall three ways: the `--ignore-filename-regex`
    // widens, a crate leaves the workspace, or files are merged. Thirteen files
    // are excluded by name today — counted from the reports, against the same
    // nine alternations in the regex, because `/corpus` matches three
    // (`taiyaku-cli`, `taiyaku-serve` and `taiyaku-library`) and `test_support`
    // three, one of them in `taiyaku-read` — so `minFiles` leaves room for that
    // list to grow a little without a coverage edit becoming a gate edit.
    minFiles: 40,
    // The **aggregate** line threshold, which this script does not enforce —
    // `cargo llvm-cov --fail-under-lines` does, one command earlier. It is here
    // only to derive the slack printed at the end of a run: `covered - ceil(93%
    // of total)`, the number of covered lines a module could lose with the
    // aggregate still green, and therefore the size of module the aggregate is
    // structurally blind to.
    //
    // **A mirrored number, and checked rather than trusted** — see
    // `aggregateFloorFrom` below and `assertAggregateFloorMatches`. A run
    // prints the current figures; `docs/ci.md` holds one dated measurement.
    aggregateFloor: 93,
    aggregateFloorFrom: 'scripts/check-all.sh',
    // **Lines only, and the other three are refusals, not oversights.**
    // `branches` is `count: 0` on every file — llvm-cov emits no branch data
    // without a nightly flag — so any floor on it would fail every file.
    // `regions` and `instantiations` read far below lines on `match`-heavy and
    // generic code, so they would need their own numbers for no gain the line
    // floor does not already give. Lines is also what `--fail-under-lines`
    // uses, for the reason recorded in ci.yml.
    floors: {
      // **Well under the lowest measured, and loose by construction.** The
      // floor is a collapse detector, not a regression detector, and wide
      // slack under the worst file is what it needs to be. It is not raised
      // toward the worst file: that is how a collapse detector turns into a
      // regression detector nobody asked for.
      //
      // So it does **not** catch a module sliding from 95% to 80%; the spread
      // between the best-covered file and the worst is too wide for one number
      // to do that. What it catches is a module that stops being tested *at
      // all* — the failure `--fail-under-lines` cannot see in a module within
      // its slack.
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
 * **The point is that nothing here is written down anywhere.** The aggregate
 * figures — lines, covered, percentage, slack and the files at or under it —
 * are all derivable from the JSON already on disk, so they are derived, and
 * `docs/ci.md` holds one dated measurement rather than live claims.
 *
 * The **slack** is the interesting figure and the reason this whole script
 * exists: `covered - ceil(floor% of total)` is how many covered lines the
 * workspace could lose with the aggregate threshold still green, so every file
 * at or under it is a file that could go entirely dark unnoticed by
 * `--fail-under-lines`. It is printed, not gated: the number moves every
 * commit and by a few lines of measurement noise besides, so a threshold on it
 * would be red for reasons that are not defects. What is gated is the per-file
 * floor above, which does not move with the margin.
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
 * A mirror nobody checks is one more copy of the problem this function's
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
