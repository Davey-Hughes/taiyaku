#!/usr/bin/env node
// The test count in `docs/status.md`, checked against the tests that ran.
//
// --- why this exists -----------------------------------------------------------
//
// `status.md` opens "What is built" with a sentence of the form
// "Five crates, N tests", and repeats N once more a few lines below. Nothing
// derived either figure, so both went stale every time a branch added a test.
// Measured on two consecutive branches in one day: 639 stated against 656
// actual, then 656 against 658 — caught by hand both times, and only because
// someone happened to look.
//
// That is the same failure this repository has logged in three other places:
// the open-items preamble's count word (which drifted three times on its own),
// the cache-root design's §6 site count, and its §7 test breakdown. The lesson
// recorded there is that **a per-site list is checkable and a count is not** —
// so this file makes the count checkable rather than asking anyone to re-derive
// it by hand a fourth time.
//
// --- where the number comes from, and the 14 seconds this used to cost --------
//
// From `target/nextest/default/junit.xml`, which the test run **already
// happening two steps above this** writes as a side effect. `.config/nextest.toml`
// turns it on with one line. The marginal cost of this check is therefore
// approximately zero: no extra build, no extra run, no extra process.
//
// **The first version of this script did not do that, and cost 14.3 seconds a
// push.** It shelled out to `cargo nextest list --message-format json` for a
// static enumeration, on the argument that a *list* is more robust than a *run*:
// it reads no environment, runs nothing, and cannot be perturbed by a failure.
// That argument is true and it was not worth 14.3 seconds. `cargo llvm-cov
// nextest` builds with coverage instrumentation and a plain `nextest list`
// cannot reuse those artifacts, so it rebuilt the whole workspace — 31% of the
// `test` job's wall clock, to check one number in one sentence.
//
// The three objections that justified it do not survive being looked at:
//
// 1. *"A run has two output formats."* In CI there is only ever one — the
//    fallback to plain `cargo test` exists only in `check-all.sh`, on a machine
//    missing `cargo-nextest`, and that branch now skips this check the same way
//    it already skips the coverage floor. One format, one parser.
// 2. *"A run's totals move with the environment."* Both callers run it with
//    `TAIYAKU_CORPUS` removed, deliberately and for other reasons, and CI has no
//    corpus at all. The variable cannot reach it.
// 3. *"A run's totals move when something fails."* True, and now handled rather
//    than avoided: this script refuses to compare at all when the report shows
//    a failure or an error, and says why. A red suite gets one clear message
//    instead of a second confusing one about documentation.
//
// The honest summary is that the expensive design was chosen first and defended
// afterwards. It is recorded here rather than quietly replaced, because the
// reasoning error is more reusable than the fix.
//
// --- why a regex and not an XML parser ----------------------------------------
//
// node has no XML parser in its standard library, and this workspace's
// dependency set is short enough to quote in `docs/status.md`. The three
// attributes read here live on the single root `<testsuites>` element that
// JUnit's schema requires to be first, so a regex over the head of the file is
// exact rather than approximate. A malformed or unexpected document fails
// closed, at exit 2, rather than matching loosely.

import { readFileSync } from 'node:fs'

const JUNIT_XML = process.argv[2]
const STATUS_MD = process.argv[3] ?? 'docs/status.md'

// Each live claim, with the prose that identifies it. Anchored on distinctive
// wording rather than on a line number, which every edit above them shifts.
const CLAIMS = [
  // The leading count word is matched as `\w+` rather than spelled out, for
  // the same reason the comment on the next claim gives: it is exactly the
  // thing that drifts. This one used to spell it out (`Five crates, …`), and
  // the day a sixth crate landed the checker reported the claim as *missing*
  // rather than as wrong — the confusing failure the next claim's comment
  // already warned about, applied to only one of the two claims.
  { name: 'the crate summary', pattern: /\w+ crates, ([\d,]+) tests/ },
  // The leading count word is matched as `\w+` rather than spelled out.
  // That word is exactly the thing that drifts — `status.md`'s own open-items
  // section records the same word going wrong three times — so anchoring on it
  // makes this check fail with "claim not found" every time the number of
  // standing checks changes, which is a false alarm about the wrong thing.
  { name: 'the corpus-check sentence', pattern: /\w+ of the ([\d,]+) are standing checks/ },
]

function fail(code, lines) {
  for (const line of lines) console.error(line)
  process.exit(code)
}

let xml
try {
  xml = readFileSync(JUNIT_XML, 'utf8')
} catch (e) {
  fail(2, [
    `status-test-count: could not read ${JUNIT_XML} (${e.message}).`,
    'It is written by the test run itself — `.config/nextest.toml` sets `junit.path`',
    'under [profile.default], so a run through `cargo nextest` or `cargo llvm-cov nextest`',
    'produces it. A run that did not happen, or a different --profile, means no file here.',
  ])
}

const root = xml.match(/<testsuites\b[^>]*>/)

// Exit 2 rather than 0 on a shape this does not recognise, the rule
// `coverage-floor.mjs` established: a gate that cannot find what it measures
// must say so, not pass. A tool upgrade that changed this element would
// otherwise silently retire the check.
if (!root) {
  fail(2, [
    `status-test-count: ${JUNIT_XML} has no <testsuites> element.`,
    'Expected nextest\'s JUnit report; a coverage report or a newer schema will not do.',
  ])
}

const attr = (name) => {
  const m = root[0].match(new RegExp(`\\b${name}="(\\d+)"`))
  return m ? Number(m[1]) : null
}

const actual = attr('tests')
const failures = attr('failures')
const errors = attr('errors')

if (actual === null || failures === null || errors === null) {
  fail(2, [
    `status-test-count: ${JUNIT_XML}'s <testsuites> is missing tests/failures/errors.`,
    `  read: ${root[0]}`,
  ])
}

// **This is objection 3, handled rather than argued away.** The count reports
// what ran, so a suite that went red under-reports it — and a documentation
// complaint stacked on top of a real failure is noise at exactly the moment
// someone needs signal. Refuse, and name the real problem instead.
if (failures > 0 || errors > 0) {
  fail(2, [
    `status-test-count: the run this report describes had ${failures} failure(s) and ${errors} error(s).`,
    'Its test count reflects only what ran, so comparing it against the documentation',
    'would be meaningless. Fix the failing tests; this check has nothing to say until then.',
  ])
}

// A report with no tests in it is the collapse the coverage gate's `minFiles`
// exists to catch, in the same shape: a run that matched nothing would
// otherwise "agree" with any figure by measuring none of it.
if (actual === 0) {
  fail(2, [
    `status-test-count: ${JUNIT_XML} reports 0 tests.`,
    'That is a broken run rather than a workspace with no tests; refusing to compare.',
  ])
}

const statusLines = readFileSync(STATUS_MD, 'utf8').split('\n')
const found = []
const missing = []

for (const claim of CLAIMS) {
  const index = statusLines.findIndex((line) => claim.pattern.test(line))
  if (index === -1) {
    missing.push(claim)
    continue
  }
  const stated = Number(statusLines[index].match(claim.pattern)[1].replace(/,/g, ''))
  found.push({ ...claim, stated, line: index + 1 })
}

// A claim that has vanished is exit 2, not exit 1: the sentence was reworded or
// deleted, which means this script is now checking something that is not there
// rather than checking something that is wrong. Those need different fixes.
if (missing.length > 0) {
  fail(2, [
    `status-test-count: ${missing.length} claim(s) not found in ${STATUS_MD}:`,
    ...missing.map((c) => `  ${c.name} — expected a line matching ${c.pattern}`),
    'Either the wording changed (update CLAIMS above) or the sentence went away.',
  ])
}

const wrong = found.filter((c) => c.stated !== actual)

if (wrong.length > 0) {
  fail(1, [
    `status-test-count: ${STATUS_MD} states a test count that is out of date.`,
    `  actual: ${actual} tests, from the run recorded in ${JUNIT_XML}`,
    ...wrong.map((c) => `  ${STATUS_MD}:${c.line} says ${c.stated} — ${c.name}`),
    '',
    `Fix: change ${wrong.map((c) => c.stated).join(' and ')} to ${actual} at the line(s) above.`,
    'Do not change the 639 inside O39\'s record — that one is a dated measurement, not a live claim.',
  ])
}

console.log(
  `status-test-count: ${actual} tests, and ${STATUS_MD} says ${actual} at ` +
    `${found.map((c) => `line ${c.line}`).join(' and ')}`,
)
