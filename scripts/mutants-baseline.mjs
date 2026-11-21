#!/usr/bin/env node
// Diff a `cargo mutants` run against the survivors this repository has already
// looked at and accepted.
//
// --- why a baseline rather than a raw survivor list ---------------------------
//
// A survivor list is only actionable if a new one is distinguishable from an old
// one. A run that reprints every untriaged survivor trains the reader to skim —
// which is the failure mode a scheduled job is most prone to and the exact
// reason `.forgejo/workflows/mutants.yml` gates nothing.
//
// So the accepted survivors live in `docs/mutants-baseline.md`, each with the
// reason it is accepted, and this compares against them. **A survivor not in the
// baseline is the whole of what is worth reading.**
//
// It also makes the harness a *ratchet*: a committed baseline remembers what
// has already been checked, for the one dimension a machine can hold it in.
//
// --- what the key is, and the annoying half of that choice --------------------
//
// The key is the mutant's own `file:line:col: description` line, verbatim from
// cargo-mutants. So moving a function invalidates its baseline entries and the
// survivors reappear as new.
//
// That is deliberate. Matching on description alone would be stabler and would
// carry an accepted survivor across a rewrite that turned it into a real defect
// — the entry would still say "equivalent mutant, `percentile` is order-
// independent here" about code that no longer is. A line that moved is a line
// somebody touched, and re-reading one baseline entry is cheap next to that.
//
// --- writing the baseline -----------------------------------------------------
//
// `--write` regenerates the file, and **only for the files this run actually
// examined**. A scoped run (`--file`, `--crate`, `--since`) must not silently
// empty the entries for everything it did not look at, which is what a naive
// rewrite would do — and it would look like an improvement.
//
// Reasons already recorded are carried across for entries that survive
// unchanged. A newly accepted survivor is written with an explicit placeholder,
// because a baseline entry with no reason is indistinguishable from a defect
// somebody stopped thinking about.
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'

// Exit 1 means survivors outside the baseline or, under --write, an entry
// written with the placeholder reason, so a crash, an outcomes.json that does
// not parse among them, exits 2 as every other run this cannot judge does.
process.on('uncaughtException', (err) => {
  console.error(`mutants-baseline: ${err?.stack ?? err}`)
  process.exit(2)
})

const PLACEHOLDER = 'TODO: why is this survivor accepted? An entry with no reason is a defect nobody finished reading.'

// **A third state, between "accepted with a reason" and "new".** Writing a
// verified reason for every survivor of a workspace-wide sweep in one sitting
// is not a thing anybody does honestly — the result would be a pile of
// sentences that sound like reasons. An entry whose reason begins with this
// marker means *recorded, not yet looked at*: it is silenced in the new-survivor
// diff, because it is not new, and counted out loud on every run, because
// nothing about it has been established.
//
// The distinction matters because the two states carry opposite claims.
// "Accepted" asserts somebody checked and this mutant cannot be killed, or
// should not be yet. "Untriaged" asserts nothing at all. Collapsing them is how
// a baseline becomes a place defects go to be forgotten, which is the failure
// this whole file exists to avoid — and the reason the count is printed rather
// than tucked into the file.
const UNTRIAGED = 'UNTRIAGED:'

const args = process.argv.slice(2)
const write = args[0] === '--write'
const [missedPath, baselinePath] = write ? args.slice(1) : args

if (!missedPath || !baselinePath) {
  console.error('usage: mutants-baseline.mjs [--write] <missed.txt> <baseline.md> [scope]')
  process.exit(2)
}

/** The mutant lines cargo-mutants wrote, one per line, blank lines dropped. */
const readList = (path) =>
  existsSync(path)
    ? readFileSync(path, 'utf8').split('\n').map((l) => l.trim()).filter(Boolean)
    : []

const missed = readList(missedPath)

// **Every mutant this run actually tested**, as the union of all four outcome
// lists — not every *file* it touched.
//
// The difference is the whole correctness of `--since`. A file with zero
// survivors appears in `caught.txt` rather than in `missed.txt`, so a set of
// file paths would let it clear its baseline entries. That is right for a
// whole-file or whole-crate run and wrong for `--in-diff`, which tests only the
// mutants inside changed hunks: every other baseline entry in a
// partially-examined file would then look like a survivor that had been
// killed, and the run would advise dropping entries it never tested.
//
// Keyed on the mutant line rather than the path, it is exact under every scope.
const outDir = dirname(missedPath)
const ran = new Set(
  ['missed.txt', 'caught.txt', 'unviable.txt', 'timeout.txt'].flatMap((name) =>
    readList(join(outDir, name)),
  ),
)

// **Whether the run reached a verdict at all**, from cargo-mutants' own record
// of it. A run whose unmutated baseline failed, or whose every mutant failed to
// build, leaves the same empty missed.txt as a clean run, and reading that as
// "nothing survived" reports a sweep that tested nothing as clean (O414). Exit
// 2, as the repository's other checks do when they cannot judge.
const noVerdict = (why) => {
  console.error(`mutants-baseline: ${why}\nmutants-baseline: so this run has no verdict, clean or otherwise`)
  process.exit(2)
}
const outcomesPath = join(outDir, 'outcomes.json')
// cargo-mutants writes no outcomes.json when, its output directory made, it
// finds no mutant to test, as for a --file matching nothing or a shard left
// empty, and that is an honest answer rather than a failure. Only cargo-mutants'
// own words in its log say so; any other absence has no verdict.
const NO_MUTANTS = /\bWARN cargo_mutants::lab: .*: No mutants found under the active filters$/m
if (!existsSync(outcomesPath)) {
  const debugLog = join(outDir, 'debug.log')
  if (!(existsSync(debugLog) && NO_MUTANTS.test(readFileSync(debugLog, 'utf8')))) {
    noVerdict(`${outcomesPath} does not exist, and cargo mutants did not say it found no mutant`)
  }
  console.log('mutants-baseline: no mutant in scope: cargo mutants found none to test, so there is nothing to report')
  process.exit(0)
}
const lab = JSON.parse(readFileSync(outcomesPath, 'utf8'))
// mutants.sh stops on cargo-mutants' exit 4 first, so through the script this
// is a second guard, for output read some other way.
const unmutated = lab.outcomes?.find((o) => o.scenario === 'Baseline')
if (unmutated?.summary !== 'Success') {
  noVerdict(
    `the unmutated baseline did not pass (${unmutated?.summary ?? 'not run'}), so no mutant was tested; ` +
      `its log is ${join(outDir, unmutated?.log_path ?? 'log')}`,
  )
}
if ((lab.caught ?? 0) + (lab.missed ?? 0) + (lab.timeout ?? 0) === 0) {
  noVerdict(`no mutant's tests ran: of ${lab.total_mutants} mutant(s), ${lab.unviable} failed to build`)
}

/**
 * Parse the baseline. An accepted survivor is a list item whose whole content
 * is a backticked mutant line; the indented lines under it are its reason.
 */
const parseBaseline = (text) => {
  const entries = new Map()
  let current = null
  for (const line of text.split('\n')) {
    const item = line.match(/^-\s+`(.+)`\s*$/)
    if (item) {
      current = item[1]
      entries.set(current, [])
      continue
    }
    if (current && /^\s+\S/.test(line)) {
      entries.get(current).push(line.trim())
      continue
    }
    // A blank line inside a reason is kept as an empty string rather than
    // skipped, so `--write` can put the paragraph break back. Dropping it would
    // make every rewrite reflow a multi-paragraph reason into one blob, a
    // little more unreadable each time — the failure mode of a file nobody
    // edits by hand is that a tool quietly degrades it.
    if (line.trim() === '') {
      if (current) entries.get(current).push('')
      continue
    }
    current = null
  }
  return entries
}

const baselineText = existsSync(baselinePath) ? readFileSync(baselinePath, 'utf8') : ''
const baseline = parseBaseline(baselineText)

const fileOf = (mutant) => mutant.split(':')[0]

if (write) {
  const kept = new Map()
  // Mutants this run did not test keep whatever the baseline already said —
  // per mutant, not per file, so a `--since` run cannot empty the entries for
  // the untouched remainder of a file it happened to reach.
  for (const [mutant, reason] of baseline) {
    if (!ran.has(mutant)) kept.set(mutant, reason)
  }
  for (const mutant of missed) {
    kept.set(mutant, baseline.get(mutant) ?? [PLACEHOLDER])
  }

  const byFile = new Map()
  for (const [mutant, reason] of [...kept].sort(([a], [b]) => a.localeCompare(b))) {
    const file = fileOf(mutant)
    if (!byFile.has(file)) byFile.set(file, [])
    byFile.get(file).push([mutant, reason])
  }

  const head = baselineText.split('\n<!-- entries -->\n')[0]
  const body = [...byFile]
    .map(([file, items]) => {
      const lines = [`### \`${file}\``, '']
      for (const [mutant, reason] of items) {
        lines.push(`- \`${mutant}\``)
        // Trailing blanks belong to the gap before the next heading, not to the
        // reason, and keeping them would grow the file by one line per rewrite.
        const trimmed = [...reason]
        while (trimmed.at(-1) === '') trimmed.pop()
        for (const r of trimmed) lines.push(r === '' ? '' : `  ${r}`)
      }
      lines.push('')
      return lines.join('\n')
    })
    .join('\n')

  writeFileSync(baselinePath, `${head}\n<!-- entries -->\n\n${body}`)
  const placeheld = [...kept.values()].filter((r) => r[0] === PLACEHOLDER).length
  const stillUntriaged = [...kept.values()].filter((r) => r[0]?.startsWith(UNTRIAGED)).length
  if (stillUntriaged > 0) {
    console.log(`mutants-baseline: ${stillUntriaged} carried across as untriaged`)
  }
  console.log(`mutants-baseline: wrote ${kept.size} accepted survivor(s) across ${byFile.size} file(s)`)
  if (placeheld > 0) {
    console.log(`mutants-baseline: ${placeheld} of them have no reason yet — write one before committing`)
    process.exit(1)
  }
  process.exit(0)
}

const fresh = missed.filter((m) => !baseline.has(m))
// Tested this run and did not survive it — so a test now sees it. Anything the
// run did not test says nothing either way.
const gone = [...baseline.keys()].filter((m) => ran.has(m) && !missed.includes(m))

const untriaged = [...baseline].filter(([, reason]) => reason[0]?.startsWith(UNTRIAGED))
const accepted = baseline.size - untriaged.length

console.log(
  `mutants-baseline: ${missed.length} survivor(s) this run; ${baselinePath} holds ` +
    `${accepted} accepted and ${untriaged.length} untriaged`,
)

if (untriaged.length > 0) {
  // Named per file rather than listed per mutant: the point is to keep the
  // debt visible at a glance, not to reprint the file.
  const byFile = new Map()
  for (const [mutant] of untriaged) {
    const file = fileOf(mutant)
    byFile.set(file, (byFile.get(file) ?? 0) + 1)
  }
  const worst = [...byFile].sort((a, b) => b[1] - a[1]).slice(0, 5)
  console.log(
    `\nmutants-baseline: ${untriaged.length} survivor(s) are recorded but NOT examined — ` +
      `nothing is known about them:\n` +
      worst.map(([f, n]) => `  ${n}  ${f}`).join('\n') +
      (byFile.size > worst.length ? `\n  … and ${byFile.size - worst.length} more file(s)` : ''),
  )
}

if (gone.length > 0) {
  console.log(`\nmutants-baseline: ${gone.length} accepted survivor(s) no longer survive — a test now sees them:`)
  for (const m of gone) console.log(`  ${m}`)
  console.log(`\n  Re-run with --baseline to drop them, so the file keeps meaning what it says.`)
}

if (fresh.length === 0) {
  console.log('\nmutants-baseline: no survivor outside the baseline')
  process.exit(0)
}

console.log(`\nmutants-baseline: ${fresh.length} survivor(s) NOT in the baseline — this is the output worth reading:`)
for (const m of fresh) console.log(`  ${m}`)
console.log(
  `\n  Each one is a mutation of the source that every test still passed against.` +
    `\n  Either write the assertion that sees it, or add it to ${baselinePath} with the reason it is accepted` +
    `\n  (\`scripts/mutants.sh ... --baseline\` writes the entries; the reasons are yours to write).`,
)
process.exit(1)
