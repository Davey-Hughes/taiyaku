#!/usr/bin/env node
// The comment guard. A comment holds the why; history and measurements belong
// in the docs. This gate keeps comments from growing back once they are trimmed.
// docs/ci.md, "The comment guard", gives the rules and how to raise a budget.
//
//   node scripts/check-comments.mjs --self-test   # the lexer and the rules, on fixed cases
//   node scripts/check-comments.mjs               # the rules, on this checkout
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

// --- languages ---------------------------------------------------------------

const LANGS = { rs: 'rust', ts: 'ts', mjs: 'ts', js: 'ts', tsx: 'tsx', css: 'css', sh: 'hash' }

function langOf(path) {
  const ext = /\.([^./]+)$/.exec(path)?.[1]
  // Only the workflows' YAML: a whole-line `#` there is a comment, while other
  // YAML in the tree is data whose `#` lines nobody reads as prose. A
  // Dockerfile's comments are whole-line `#` alone, as the hash lexer reads them.
  if ((ext === 'yml' || ext === 'yaml') && path.startsWith('.forgejo/')) return 'hash'
  if (/(?:^|\/)Dockerfile$/.test(path)) return 'hash'
  return Object.hasOwn(LANGS, ext ?? '') ? LANGS[ext] : undefined
}

// --- the lexer ---------------------------------------------------------------
// segments(text, lang) tiles `text` exactly with [kind, start, end] spans, kind
// one of 'code', 'comment', 'string'. Every rule reads comments through it, so a
// string holding `//` or `/*` must never read as a comment; the self-test pins
// that per language.

const IDENT = /[A-Za-z0-9_$]/
const RUST_RAW = /(?:br|cr|r)(#*)"/y
const TS_WORD = /[A-Za-z_$][A-Za-z0-9_$]*|\d[\w.]*/y
const REGEX_AFTER_WORD = new Set([
  'return', 'typeof', 'instanceof', 'in', 'of', 'new', 'delete', 'void', 'throw', 'case', 'do',
  'else', 'yield', 'await',
])

function isIdent(ch) {
  return ch !== undefined && IDENT.test(ch)
}

function isSpace(ch) {
  return ch <= ' ' || (ch > '~' && /\s/.test(ch))
}

class Spans {
  list = []

  push(kind, a, b) {
    if (b <= a) return
    const last = this.list.at(-1)
    if (last !== undefined && last[0] === kind && last[2] === a) last[2] = b
    else this.list.push([kind, a, b])
  }
}

function lineEnd(t, i) {
  const j = t.indexOf('\n', i)
  return j < 0 ? t.length : j
}

function blockCommentEnd(t, i, nested) {
  if (!nested) {
    const j = t.indexOf('*/', i + 2)
    return j < 0 ? t.length : j + 2
  }
  let depth = 0
  let j = i
  while (j < t.length) {
    if (t.startsWith('/*', j)) {
      depth += 1
      j += 2
    } else if (t.startsWith('*/', j)) {
      depth -= 1
      j += 2
      if (depth === 0) return j
    } else {
      j += 1
    }
  }
  return t.length
}

function quotedEnd(t, i, q, stopAtNewline) {
  let j = i + 1
  while (j < t.length && t[j] !== q) {
    if (stopAtNewline && t[j] === '\n') return j
    j += t[j] === '\\' ? 2 : 1
  }
  return Math.min(j + 1, t.length)
}

function lexRust(t) {
  const out = new Spans()
  const n = t.length
  let i = 0
  let cs = 0
  const take = (kind, j) => {
    out.push('code', cs, i)
    out.push(kind, i, j)
    i = cs = j
  }
  while (i < n) {
    const c = t[i]
    const atToken = i === 0 || !isIdent(t[i - 1])
    if (t.startsWith('//', i)) {
      take('comment', lineEnd(t, i))
      continue
    }
    if (t.startsWith('/*', i)) {
      take('comment', blockCommentEnd(t, i, true))
      continue
    }
    RUST_RAW.lastIndex = i
    const raw = (c === 'r' || c === 'b' || c === 'c') && atToken ? RUST_RAW.exec(t) : null
    if (raw !== null) {
      const close = `"${raw[1]}`
      const k = t.indexOf(close, RUST_RAW.lastIndex)
      take('string', k < 0 ? n : k + close.length)
      continue
    }
    if (c === '"' || ((c === 'b' || c === 'c') && atToken && t[i + 1] === '"')) {
      take('string', quotedEnd(t, c === '"' ? i : i + 1, '"', false))
      continue
    }
    if (c === "'" || (c === 'b' && atToken && t[i + 1] === "'")) {
      const q = c === "'" ? i : i + 1
      // A char outside the BMP is two UTF-16 units, so its closing quote sits
      // one further on; misreading it as a lifetime can pair later quotes wrongly.
      const width = /[\uD800-\uDBFF]/.test(t[q + 1] ?? '') ? 2 : 1
      if (t[q + 1] === '\\') {
        const k = t.indexOf("'", q + 3)
        take('string', k < 0 ? n : k + 1)
      } else if (t[q + 1 + width] === "'" && t[q + 1] !== '\n') {
        take('string', q + 2 + width)
      } else {
        i += 1 // a lifetime or a loop label, which never closes
      }
      continue
    }
    i += 1
  }
  out.push('code', cs, n)
  return out.list
}

// Scans template text from j, just after a backtick or the `}` closing a `${`.
// Returns the end (past the closing backtick, or past `${` when one opens).
function templateEnd(t, j) {
  while (j < t.length) {
    if (t[j] === '\\') j += 2
    else if (t[j] === '`') return [j + 1, false]
    else if (t.startsWith('${', j)) return [j + 2, true]
    else j += 1
  }
  return [t.length, false]
}

// A slash opens a regex wherever a value may start, and divides after one.
function opensRegexAfter(prev) {
  return (
    prev === '' ||
    REGEX_AFTER_WORD.has(prev) ||
    (prev.length === 1 && !')]}'.includes(prev) && !isIdent(prev))
  )
}

// JSX text is lexed as code, so an apostrophe or a `//` in it can mislabel the
// rest of that line. Strings and regexes here stop at a newline, which keeps
// such a misreading on its own line.
function lexTs(t) {
  const out = new Spans()
  const n = t.length
  let i = 0
  let cs = 0
  let prev = '' // the last code token: a word, one punctuation character, or 'str'
  let depth = 0
  const templ = [] // the brace depths at which each open `${` began
  const take = (kind, j) => {
    out.push('code', cs, i)
    out.push(kind, i, j)
    i = cs = j
  }
  while (i < n) {
    const c = t[i]
    if (t.startsWith('//', i)) {
      take('comment', lineEnd(t, i))
      continue
    }
    if (t.startsWith('/*', i)) {
      take('comment', blockCommentEnd(t, i, false))
      continue
    }
    if (c === '"' || c === "'") {
      take('string', quotedEnd(t, i, c, true))
      prev = 'str'
      continue
    }
    if (c === '`' || (c === '}' && templ.length > 0 && templ.at(-1) === depth)) {
      if (c === '}') templ.pop()
      const [j, opened] = templateEnd(t, i + 1)
      if (opened) templ.push(depth)
      take('string', j)
      prev = 'str'
      continue
    }
    if (c === '/' && opensRegexAfter(prev)) {
      let j = i + 1
      let inClass = false
      while (j < n && t[j] !== '\n') {
        if (t[j] === '\\') {
          if (t[j + 1] === '\n') break
          j += 2
          continue
        }
        if (t[j] === '[') inClass = true
        else if (t[j] === ']') inClass = false
        else if (t[j] === '/' && !inClass) break
        j += 1
      }
      if (t[j] === '/') {
        j += 1
        while (j < n && /[A-Za-z]/.test(t[j])) j += 1
      }
      take('string', j)
      prev = 'str'
      continue
    }
    if (isSpace(c)) {
      i += 1
      continue
    }
    if (isIdent(c)) {
      TS_WORD.lastIndex = i
      const word = TS_WORD.exec(t)
      if (word !== null) {
        prev = word[0]
        i = TS_WORD.lastIndex
        continue
      }
    }
    if (c === '{') depth += 1
    else if (c === '}') depth -= 1
    prev = c
    i += 1
  }
  out.push('code', cs, n)
  return out.list
}

function lexCss(t) {
  const out = new Spans()
  const n = t.length
  let i = 0
  let cs = 0
  while (i < n) {
    const c = t[i]
    let j = -1
    let kind
    if (t.startsWith('/*', i)) [kind, j] = ['comment', blockCommentEnd(t, i, false)]
    else if (c === '"' || c === "'") [kind, j] = ['string', quotedEnd(t, i, c, true)]
    if (j < 0) {
      i += 1
      continue
    }
    out.push('code', cs, i)
    out.push(kind, i, j)
    i = cs = j
  }
  out.push('code', cs, n)
  return out.list
}

// Only a whole-line `#` is a comment. A trailing one may sit inside a string or
// a `${#var}`, and reading it as code errs to the safe side: an edit to it reads
// as a code change, never as a comment the rules would judge.
function lexHash(t) {
  const out = new Spans()
  let pos = 0
  t.split('\n').forEach((line, k) => {
    const s = line.trimStart()
    const a = pos + line.length - s.length
    const end = pos + line.length
    if (s.startsWith('#') && !(k === 0 && s.startsWith('#!'))) {
      out.push('code', pos, a)
      out.push('comment', a, end)
    } else {
      out.push('code', pos, end)
    }
    out.push('code', end, Math.min(end + 1, t.length))
    pos = end + 1
  })
  return out.list
}

const LEXERS = { rust: lexRust, ts: lexTs, tsx: lexTs, css: lexCss, hash: lexHash }

function segments(text, lang) {
  return LEXERS[lang](text)
}

const CODE = 0
const COMMENT = 1
const STRING = 2

// A JSX comment needs braces around it, `{/* … */}`, and those braces are part
// of the comment rather than code. Only a brace on the comment's own line
// counts, so the closing brace of a block that merely holds a comment stays code.
function markJsxCommentBraces(text, kinds) {
  for (let open = text.indexOf('{'); open >= 0; open = text.indexOf('{', open + 1)) {
    if (kinds[open] !== CODE) continue
    let p = open + 1
    let comments = 0
    for (;;) {
      while (text[p] === ' ' || text[p] === '\t') p += 1
      if (kinds[p] !== COMMENT || !text.startsWith('/*', p)) break
      while (kinds[p] === COMMENT) p += 1
      comments += 1
    }
    if (comments > 0 && text[p] === '}' && kinds[p] === CODE) {
      kinds[open] = COMMENT
      kinds[p] = COMMENT
    }
  }
}

function charKinds(text, lang) {
  const kinds = new Uint8Array(text.length)
  for (const [kind, a, b] of segments(text, lang)) {
    if (kind !== 'code') kinds.fill(kind === 'comment' ? COMMENT : STRING, a, b)
  }
  if (lang === 'tsx') markJsxCommentBraces(text, kinds)
  return kinds
}

// One entry per line of `text` (split on '\n'): 'comment' when everything on it
// that is not whitespace is comment, 'blank' when it is all whitespace, and
// 'code' otherwise. A string counts as code.
function lineKinds(text, lang) {
  const kinds = charKinds(text, lang)
  const out = []
  let comment = false
  let other = false
  for (let p = 0; p <= text.length; p += 1) {
    if (p === text.length || text[p] === '\n') {
      out.push(other ? 'code' : comment ? 'comment' : 'blank')
      comment = other = false
    } else if (!isSpace(text[p])) {
      if (kinds[p] === COMMENT) comment = true
      else other = true
    }
  }
  return out
}

// Fenced code and rustdoc's sections mean something only in a doc comment, and
// `//!` is one only in Rust; elsewhere they are prose like any other.
function isDoc(lang, open) {
  if (lang === 'rust') return /^(?:\/\/\/(?!\/)|\/\/!|\/\*\*(?![*/])|\/\*!)/.test(open)
  return (lang === 'ts' || lang === 'tsx') && /^\/\*\*(?![*/])/.test(open)
}

// lineKinds, plus each line's text, its comment text (its comment runs, joined),
// whether that comment is a doc comment, and whether a blank line sits inside a
// block comment: such a line continues the block around it rather than ending it.
function lineInfo(text, lang) {
  const kinds = charKinds(text, lang)
  const kindOf = lineKinds(text, lang)
  const docs = new Uint8Array(text.length)
  for (const [kind, a, b] of segments(text, lang)) {
    if (kind === 'comment' && isDoc(lang, text.slice(a, a + 4))) docs.fill(1, a, b)
  }
  const out = []
  let start = 0
  for (const kind of kindOf) {
    const end = lineEnd(text, start)
    const runs = []
    let doc = null
    for (let p = start; p < end; p += 1) {
      if (kinds[p] !== COMMENT) continue
      doc ??= docs[p] === 1
      const a = p
      while (p < end && kinds[p] === COMMENT) p += 1
      runs.push(text.slice(a, p).trim())
    }
    const inComment = kind === 'blank' && end < text.length && kinds[end] === COMMENT
    out.push({ kind, line: text.slice(start, end), comment: runs.join(' '), doc: doc === true, inComment })
    start = end + 1
  }
  return out
}

// --- rule 3: the comment share of each area -------------------------------------

const BUDGET_PATH = 'scripts/comment-budget.json'
// A share is compared at the precision the budget is written in, so an
// unchanged tree reproduces its own budget exactly. One unit is 0.0001.
const UNITS = 10_000
// Past this many units below budget, a branch that removed comment lines from
// an area must write its budget down to the share, so a trimming branch's gain
// is locked in rather than left as room to grow back into. A share that fell
// only because code grew faster than comments is no gain to lock.
const STALE_UNITS = 50

const asShare = (units) => (units / UNITS).toFixed(4)

// Areas are the budget's keys, plus every crate and each of AREAS: a new crate
// is judged from its first commit, and an area dropped from the budget fails
// for want of one rather than going unmeasured.
const AREAS = ['web/src', 'web/tests', 'scripts']

function measureAreas(files, budget) {
  const measured = new Map(Object.keys(budget).map((area) => [area, { comment: 0, code: 0 }]))
  for (const { path, lang, text } of files) {
    const fixed = /^crates\/[^/]+(?=\/)/.exec(path)?.[0] ?? AREAS.find((area) => path.startsWith(`${area}/`))
    if (fixed !== undefined && !measured.has(fixed)) measured.set(fixed, { comment: 0, code: 0 })
    const area = [...measured.keys()].find((key) => path.startsWith(`${key}/`))
    if (area === undefined) continue
    const counts = measured.get(area)
    for (const kind of lineKinds(text, lang)) {
      if (kind === 'comment') counts.comment += 1
      else if (kind === 'code') counts.code += 1
    }
  }
  return measured
}

// `before` is measureAreas over the base's tree, which the stale arm reads.
function judgeShares(measured, budget, before) {
  const failures = []
  for (const [area, { comment, code }] of measured) {
    const lines = comment + code
    if (lines === 0) {
      failures.push(`${area}: no source lines under it. Correct the path in ${BUDGET_PATH}, or remove it.`)
      continue
    }
    const share = Math.round((comment * UNITS) / lines)
    const entry = `"${area}": ${asShare(share)}`
    const counts = `${comment} comment lines, ${code} code lines`
    if (!Object.hasOwn(budget, area)) {
      failures.push(`${area}: has no comment budget (share ${asShare(share)}, ${counts}). Add ${entry} to ${BUDGET_PATH}.`)
      continue
    }
    const allowed = Math.round(budget[area] * UNITS)
    if (share > allowed) {
      failures.push(
        `${area}: comment share ${asShare(share)} is above its budget of ${asShare(allowed)} (${counts}).\n` +
          '    Move history and measurements to the docs and keep the why. If the new comments earn\n' +
          `    their place, raise the budget in this same diff, deliberately: ${entry}`,
      )
    } else if (allowed - share > STALE_UNITS && comment < (before.get(area)?.comment ?? 0)) {
      failures.push(
        `${area}: comment share ${asShare(share)} is more than ${asShare(STALE_UNITS)} below its budget of ${asShare(allowed)}, ` +
          `and this branch removed comment lines (${before.get(area).comment} at the base; now ${counts}).\n` +
          `    Lock the gain in: write ${entry} in ${BUDGET_PATH}.`,
      )
    }
  }
  return failures
}

// --- rules 1 and 2: what the branch adds -----------------------------------------
// Both read only comment lines the branch adds, so an existing comment is never
// failed for being there. A line whose comment text the diff also removes
// somewhere is carried, not added: a move or a re-indent brings nothing new.

// Each pattern is a way a comment tells history. Allowed by omission: O-numbers,
// which point at the ledger, and `no longer`, which usually states an invariant.
// A date before a `§` points at a notes section, and a ruling's date names the
// ruling. Among past-tense verbs only `tried`: in this app a file or a library
// that is added, removed, moved or renamed is an event the code handles. A
// backticked `#808080` is a colour, not a pull request.
const HISTORY = [
  ['a date', /(?<![\w-])(?<!\brulings? (?:of )?)\d{4}-[01]\d-[0-3]\d(?![\w-]|\.\w|,?\s*§)/g],
  ['a commit hash', /\b(?=[0-9a-f]*\d)(?=[0-9a-f]*[a-f])[0-9a-f]{7,40}\b/g],
  ['a pull request number', /(?<![\w&`])#\d+\b/g],
  ['a pull request', /\bPR\b/g],
  ['"previously"', /\bpreviously\b/gi],
  ['"originally"', /\boriginally\b/gi],
  ['"formerly"', /\bformerly\b/gi],
  ['a past attempt', /\b(?:was|were) tried\b/gi],
  ['a fix', /\bbefore (?:this|the) (?:fix|change)\b/gi],
  ['struck-out text', /~~/g],
  ['"used to"', /(?<!\b(?:is|are|was|were|be|been|being|get|gets|got)\s+)\bused to\b/gi],
]

// A comment line's opening delimiter, when whitespace follows it.
const LEAD = /^(?:\/\/[/!]?|\/\*[*!]?|\{\/\*|\*|#+)\s+/

// A pointer can wrap, leaving `spec <date>` at a line's end and its `§5.2` on
// the next, or `rulings of` above its date. So the patterns read the line
// between its neighbouring comment lines, and only a hit that starts on the line
// itself counts. In CSS `#000` is a colour, and a line of tildes opens or
// closes a doc's fenced code.
function historyMarker(comment, lang, before = '', after = '') {
  const body = comment.replace(LEAD, '')
  const head = `${before.replace(LEAD, '')} `
  const text = `${head}${body} ${after.replace(LEAD, '')}`
  for (const [label, pattern] of HISTORY) {
    if (label === 'a pull request number' && lang === 'css') continue
    if (label === 'struck-out text' && /^~{3,}/.test(body)) continue
    for (const hit of text.matchAll(pattern)) {
      if (hit.index >= head.length && hit.index < head.length + body.length) return [label, hit[0]]
    }
  }
  return null
}

const BLOCK_CAP = 8
const FENCE = /^(`{3,}|~{3,})/
const HEADING = /^#+\s/
const EXEMPT_SECTION = /^#\s+(?:Errors|Panics|Safety|Examples)\s*$/
const MARKER = /^(?:\/\/[/!]?|\/\*[*!]?|\{\/\*|\*\/?|#)\s?/
const IMPORT = /^\s*(?:import\b|export\s*[*{]|(?:pub(?:\([^)]*\))?\s+)?(?:use|mod)\s|extern\s+crate\b|#!\[)/

// A comment line's text without its delimiters, so a line that is only `/**`,
// `*/` or a bare `//` is empty: the cap counts text, and every style gets 8 lines.
const bodyOf = (comment) => comment.replace(/\*\/\}?$/, '').replace(MARKER, '').trim()

// The leading header is the file's first block, when only a shebang and blank
// lines come before it. In Rust and TS a block that runs straight into code is
// that item's doc, which the cap counts, so there a blank line or an import must
// follow the header. Shell, YAML, a Dockerfile and CSS have no item doc.
function headerOf(info, lang, shebang) {
  let k = shebang ? 1 : 0
  while (k < info.length && info[k].kind === 'blank') k += 1
  const start = k
  while (k < info.length && (info[k].kind === 'comment' || info[k].inComment)) k += 1
  if (lang === 'hash' || lang === 'css') return [start, k]
  const next = info[k]
  return next !== undefined && (next.kind === 'blank' || IMPORT.test(next.line)) ? [start, k] : [0, 0]
}

// A doc comment's fences, as the set of lines they hold. A fence that never
// closes fences nothing, or one stray fence line would exempt the rest.
function fencedLines(info, start, end) {
  const fenced = new Set()
  let open = -1
  let fence = null
  for (let k = start; k <= end; k += 1) {
    if (info[k].kind !== 'comment' || !info[k].doc) continue
    const body = bodyOf(info[k].comment)
    if (fence === null) {
      fence = FENCE.exec(body)?.[1] ?? null
      open = k
    } else if (body.startsWith(fence)) {
      for (let f = open; f <= k; f += 1) if (info[f].doc) fenced.add(f)
      fence = null
    }
  }
  return fenced
}

// Rule 2's unit: a block is a maximal run of whole-line comments. A block line
// is counted when it has text and is not exempt: the leading header, a Rust
// `//!` module doc, or, in a doc comment, fenced code or an `# Errors`,
// `# Panics`, `# Safety` or `# Examples` section (up to the next heading).
function commentBlocks(info, lang, shebang) {
  const [headerStart, headerEnd] = headerOf(info, lang, shebang)
  const counted = info.map(() => false)
  const blocks = []
  const inBlock = (line) => line.kind === 'comment' || line.inComment
  for (let k = 0; k < info.length; ) {
    if (!inBlock(info[k])) {
      k += 1
      continue
    }
    const start = k
    while (k < info.length && inBlock(info[k])) k += 1
    const fenced = fencedLines(info, start, k - 1)
    let size = 0
    let section = false
    for (let j = start; j < k; j += 1) {
      const { kind, comment, doc } = info[j]
      if (kind !== 'comment' || fenced.has(j)) continue
      const body = bodyOf(comment)
      if (doc && HEADING.test(body)) section = EXEMPT_SECTION.test(body)
      const exempt = (doc && (section || comment.startsWith('//!'))) || (j >= headerStart && j < headerEnd)
      if (body === '' || exempt) continue
      counted[j] = true
      size += 1
    }
    blocks.push({ start, end: k - 1, size })
  }
  return { blocks, counted }
}

// Hunks are [oldStart, oldCount, newStart, newCount] as `git diff -U0` writes
// them: 1-based, and a side with no lines gives the line the edit follows.
const removedLines = ([oldStart, oldCount]) => Array.from({ length: oldCount }, (_, k) => oldStart - 1 + k)

function countUp(map, key, by = 1) {
  map.set(key, (map.get(key) ?? 0) + by)
}

// Takes one away from `key` in `map` if any is left; says whether it could.
function takeOne(map, key) {
  const left = map.get(key) ?? 0
  if (left > 0) map.set(key, left - 1)
  return left > 0
}

// changes: [{ path, lang, oldLang, oldText, newText, hunks }], one per changed file.
function judgeChanges(changes) {
  const files = changes.map((change) => {
    const oldLang = change.oldLang ?? change.lang
    const old = lineInfo(change.oldText, oldLang)
    const neu = lineInfo(change.newText, change.lang)
    return {
      ...change,
      old,
      neu,
      oldCounted: commentBlocks(old, oldLang, change.oldText.startsWith('#!')).counted,
      ...commentBlocks(neu, change.lang, change.newText.startsWith('#!')),
    }
  })
  // Rule 1 carries any comment text the diff removes. Rule 2 carries only a
  // removed line the cap counted, so a line exempt where it stood, such as a
  // header line, cannot grow a block it is moved into.
  const removed = new Map()
  const removedCounted = new Map()
  for (const { hunks, old, oldCounted } of files) {
    for (const hunk of hunks) {
      for (const k of removedLines(hunk)) {
        if (old[k]?.comment) countUp(removed, old[k].comment)
        if (oldCounted[k]) countUp(removedCounted, old[k].comment)
      }
    }
  }
  return [...historyFailures(files, removed), ...blockFailures(files, removedCounted)]
}

function historyFailures(files, unclaimed) {
  const failures = []
  for (const { path, lang, hunks, neu } of files) {
    for (const [, , newStart, newCount] of hunks) {
      for (let k = newStart - 1; k < newStart - 1 + newCount; k += 1) {
        const comment = neu[k]?.comment
        if (!comment || takeOne(unclaimed, comment)) continue
        const hit = historyMarker(comment, lang, neu[k - 1]?.comment, neu[k + 1]?.comment)
        if (hit === null) continue
        const quoted = JSON.stringify(hit[1])
        const what = hit[0] === quoted.toLowerCase() ? hit[0] : `${hit[0]}, ${quoted},`
        failures.push(
          `${path}:${k + 1}: ${what} in a new comment: ${comment}\n` +
            '    A comment holds the why. Move the history to the docs, or point at an O-number.',
        )
      }
    }
  }
  return failures
}

// A long block fails only when this branch grew it: its added counted lines,
// less those carried from a removal elsewhere, outnumber the counted comment
// lines removed by the hunks that touch it. Trimming or editing an old long
// block passes; writing a new one, or adding to an old one, does not.
// `removed` holds the counted lines' texts, so it is reduced by those alone.
function blockFailures(files, removed) {
  const failures = []
  for (const { path, hunks, old, oldCounted, neu, blocks, counted } of files) {
    for (const { start, end, size } of blocks) {
      if (size <= BLOCK_CAP) continue
      const touching = hunks.filter(([, , newStart, newCount]) =>
        newCount === 0 ? newStart >= start && newStart <= end + 1 : newStart <= end + 1 && newStart + newCount > start + 1,
      )
      const elsewhere = new Map(removed)
      let cut = 0
      for (const hunk of touching) {
        for (const k of removedLines(hunk)) {
          if (!oldCounted[k]) continue
          countUp(elsewhere, old[k].comment, -1)
          cut += 1
        }
      }
      let added = 0
      for (const [, , newStart, newCount] of touching) {
        for (let k = Math.max(newStart - 1, start); k < Math.min(newStart - 1 + newCount, end + 1); k += 1) {
          if (counted[k] && !takeOne(elsewhere, neu[k].comment)) added += 1
        }
      }
      if (added <= cut) continue
      failures.push(
        `${path}:${start + 1}-${end + 1}: a comment block of ${size} counted lines, over the cap of ${BLOCK_CAP}, ` +
          `and this branch grew it (${added} comment lines added, ${cut} removed).\n` +
          '    Keep the why and move the rest to the docs, or split it where the code it explains splits.',
      )
    }
  }
  return failures
}

// --- the tree ------------------------------------------------------------------

// Tracked files plus untracked ones git would add, read from the working tree:
// a new file must count locally before its first commit, as it will in CI.
function sourceFiles(root) {
  const listed = execFileSync('git', ['ls-files', '-z', '--cached', '--others', '--exclude-standard'], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  })
  const files = []
  for (const path of new Set(listed.split('\0'))) {
    const lang = langOf(path)
    if (lang === undefined) continue
    try {
      files.push({ path, lang, text: readFileSync(join(root, path), 'utf8') })
    } catch (err) {
      if (err.code !== 'ENOENT') throw err // deleted in the working tree, not yet staged
    }
  }
  return files
}

// The source files of the commit `rev`, as sourceFiles gives the working tree's.
function treeFiles(root, rev) {
  const listed = execFileSync('git', ['ls-tree', '-r', '-z', '--name-only', rev], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  })
  const paths = listed.split('\0').filter((path) => langOf(path) !== undefined)
  const texts = readBlobs(root, paths.map((path) => `${rev}:${path}`))
  return paths.map((path, k) => ({ path, lang: langOf(path), text: texts[k] }))
}

// --- the branch ------------------------------------------------------------------

// A condition the gate cannot judge past; main reports it and exits 2.
class GateError extends Error {}

// The base is where the branch left its target, so rules 1 and 2 judge only what
// the branch adds. With none the gate stops: a diff against nothing would pass
// every comment unread. `git` runs git and gives its trimmed output, or null.
function resolveBase(git, given) {
  if (given === '') throw new GateError('check-comments: --base was given an empty revision.')
  const target = given ?? 'origin/main'
  const tip = git(['rev-parse', '--verify', '--quiet', `${target}^{commit}`])
  if (tip === null) {
    const hint = given === undefined ? ' Fetch it, or pass --base <rev>.' : ''
    throw new GateError(`check-comments: cannot find the base: ${target} is not a commit in this clone.${hint}`)
  }
  const base = git(['merge-base', tip, 'HEAD'])
  if (base === null) {
    throw new GateError(
      `check-comments: cannot find the base: ${target} and HEAD share no commit in this clone.` +
        ' A shallow clone must be fetched deep enough to reach their merge base.',
    )
  }
  return base
}

const QUOTED = { a: '\x07', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t', v: '\v' }

// git quotes a path holding a quote, a backslash or a control character, and
// ends a `---`/`+++` path holding a space with a tab.
function patchPath(raw, prefix) {
  let path = raw.endsWith('\t') ? raw.slice(0, -1) : raw
  if (path === '/dev/null') return null
  if (path.startsWith('"')) {
    path = path.slice(1, -1).replace(/\\([0-7]{3}|.)/g, (_, e) =>
      e.length === 3 ? String.fromCharCode(Number.parseInt(e, 8)) : (QUOTED[e] ?? e),
    )
  }
  return path.startsWith(prefix) ? path.slice(prefix.length) : path
}

// One entry per file in `git diff -U0` output: { from, to, hunks }, `from` null
// for a new file and `to` null for a deleted one. A hunk's body is skipped by
// its counts, since a body line may itself begin `--- ` or `+++ `.
function parsePatch(patch) {
  const files = []
  const lines = patch.split('\n')
  let file = null
  for (let k = 0; k < lines.length; k += 1) {
    const line = lines[k]
    if (line.startsWith('diff --git ')) {
      file = { from: null, to: null, hunks: [] }
      files.push(file)
    } else if (file === null) {
      continue
    } else if (line.startsWith('--- ')) {
      file.from = patchPath(line.slice(4), 'a/')
    } else if (line.startsWith('+++ ')) {
      file.to = patchPath(line.slice(4), 'b/')
    } else {
      const h = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line)
      if (h === null) continue
      const hunk = [Number(h[1]), Number(h[2] ?? 1), Number(h[3]), Number(h[4] ?? 1)]
      file.hunks.push(hunk)
      for (let body = hunk[1] + hunk[3]; body > 0 && k + 1 < lines.length; ) {
        k += 1
        if (!lines[k].startsWith('\\')) body -= 1
      }
    }
  }
  return files
}

// Reads each `rev:path` with one `git cat-file --batch`.
function readBlobs(root, specs) {
  if (specs.length === 0) return []
  const out = execFileSync('git', ['cat-file', '--batch'], {
    cwd: root,
    input: specs.map((spec) => `${spec}\n`).join(''),
    maxBuffer: 256 * 1024 * 1024,
  })
  let p = 0
  return specs.map((spec) => {
    const nl = out.indexOf(0x0a, p)
    const header = out.toString('utf8', p, nl)
    const size = /^[0-9a-f]+ blob (\d+)$/.exec(header)?.[1]
    if (size === undefined) throw new GateError(`check-comments: cannot read ${spec}: git says ${header}`)
    p = nl + 1 + Number(size) + 1
    return out.toString('utf8', nl + 1, p - 1)
  })
}

// What the branch changes since `base`, read against the working tree as rule 3
// reads it: the diff, plus each untracked file as wholly added. A deleted file
// stays in, since its removed comments can carry a moved line elsewhere.
function branchChanges(root, base) {
  const patch = execFileSync(
    'git',
    ['-c', 'core.quotePath=false', 'diff', '--unified=0', '--find-renames', '--no-color', '--no-ext-diff']
      .concat(['--no-textconv', '--src-prefix=a/', '--dst-prefix=b/', base, '--']),
    { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 },
  )
  const diffed = parsePatch(patch).filter(({ from, to, hunks }) => hunks.length > 0 && langOf(to ?? from) !== undefined)
  const olds = readBlobs(
    root,
    diffed.filter(({ from }) => from !== null).map(({ from }) => `${base}:${from}`),
  )
  let next = 0
  const changes = diffed.map(({ from, to, hunks }) => ({
    path: to ?? from,
    lang: langOf(to ?? from),
    oldLang: from === null ? undefined : langOf(from),
    oldText: from === null ? '' : olds[next++],
    newText: to === null ? '' : readFileSync(join(root, to), 'utf8'),
    hunks,
  }))
  const untracked = execFileSync('git', ['ls-files', '-z', '--others', '--exclude-standard'], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  })
  for (const path of untracked.split('\0')) {
    const lang = langOf(path)
    if (lang === undefined) continue
    const newText = readFileSync(join(root, path), 'utf8')
    changes.push({ path, lang, oldText: '', newText, hunks: [[0, 0, 1, newText.split('\n').length]] })
  }
  return changes
}

// The three rules over the checkout at `root`. The stale arm measures `base`'s
// own tree: the working tree is the branch, so it cannot say what was removed.
function judgeCheckout(root, base, budget) {
  const changes = branchChanges(root, base)
  const before = measureAreas(treeFiles(root, base), budget)
  const measured = measureAreas(sourceFiles(root), budget)
  return { measured, changes, failures: [...judgeShares(measured, budget, before), ...judgeChanges(changes)] }
}

function readBudget(root) {
  let budget
  try {
    budget = JSON.parse(readFileSync(join(root, BUDGET_PATH), 'utf8'))
  } catch (err) {
    console.error(`check-comments: cannot read ${BUDGET_PATH}`)
    console.error(String(err))
    process.exit(2)
  }
  const bad =
    budget === null || typeof budget !== 'object' || Array.isArray(budget)
      ? ['(the whole file)', budget]
      : Object.entries(budget).find(([, v]) => typeof v !== 'number' || !(v >= 0 && v <= 1))
  if (bad !== undefined) {
    console.error(`check-comments: ${BUDGET_PATH} gives ${bad[0]} ${JSON.stringify(bad[1])}.`)
    console.error('It must be one object mapping each area to a share between 0 and 1.')
    process.exit(2)
  }
  return budget
}

// --- the self-test ---------------------------------------------------------------
// Each lexer case is a file as [kind, line] pairs. Most are built so a lexer
// that gets the construct wrong misreads a *later* line, which a per-line
// expectation then catches.

const LEXER_CASES = [
  {
    name: 'rust: comments, a string holding `/*`, code',
    lang: 'rust',
    lines: [
      ['comment', '// a line comment'],
      ['comment', '/// a doc comment'],
      ['comment', '//! a module doc'],
      ['code', 'let url = "http://example/*x";'],
      ['code', 'let n = 1; // trailing'],
      ['blank', '   '],
    ],
  },
  {
    name: 'rust: a string over three lines holding `/*`',
    lang: 'rust',
    lines: [
      ['code', 'let s = "first'],
      ['code', '/* not a comment'],
      ['code', 'last";'],
      ['comment', '// after the string'],
    ],
  },
  {
    name: 'rust: raw strings, one ending in a backslash and one holding `"#`',
    lang: 'rust',
    lines: [
      ['code', 'let a = r"C:\\dir\\";'],
      ['comment', '// after a raw string'],
      ['code', 'let b = br##"a "# quote'],
      ['code', '// inside a raw string'],
      ['code', '"##;'],
      ['comment', '// after the raw string'],
    ],
  },
  {
    name: 'rust: a string holding an escaped quote',
    lang: 'rust',
    lines: [
      ['code', String.raw`let s = "a \" b";`],
      ['comment', '// after the string'],
      ['code', 'let t = "";'],
    ],
  },
  {
    name: 'rust: a quote ending a line, then a char literal',
    lang: 'rust',
    lines: [
      ['code', "fn f<'"],
      ['code', `'"'; // after a char`],
      ['comment', '// after both'],
    ],
  },
  {
    name: 'rust: a nested block comment',
    lang: 'rust',
    lines: [
      ['comment', '/* outer'],
      ['comment', '   /* inner */'],
      ['comment', '   still outer */'],
      ['code', 'let after = 1;'],
    ],
  },
  {
    name: 'rust: lifetimes against char literals',
    lang: 'rust',
    lines: [
      ['code', "impl<'a> Tr<'a> for S<'a> {}"],
      ['comment', '// after three lifetimes'],
      ['code', `let q = '"';`],
      ['comment', '// after a quote char'],
      ['code', String.raw`let e = '\''; let dq = '\"'; let s = b'/';`],
      ['comment', '// after escaped chars and a byte char'],
      ['code', `let pair = ('😀','"');`],
      ['comment', '// after a char outside the BMP'],
      ['code', 'let z = "";'],
    ],
  },
  {
    name: 'ts: comments, strings holding `//` and `/*`, code',
    lang: 'ts',
    lines: [
      ['code', '#!/usr/bin/env node'],
      ['comment', '// a line comment'],
      ['comment', '/* a block'],
      ['comment', ' * comment */'],
      ['code', "const u = 'http://example';"],
      ['code', 'const t = "/* not a comment";'],
      ['code', 'const n = 1; // trailing'],
    ],
  },
  {
    name: 'ts: a template literal with `${}`, nested templates, braces and a backtick',
    lang: 'ts',
    lines: [
      ['code', "const s = `a ${b ? `c` : '}'} d"],
      ['code', '// inside a template'],
      ['code', '${ {k: 1}.k + "`" } /* also inside */'],
      ['code', '// still inside, past an expression holding braces and a backtick'],
      ['code', '`'],
      ['comment', '// after the template'],
    ],
  },
  {
    name: 'ts: a template literal holding an escaped backtick',
    lang: 'ts',
    lines: [
      ['code', 'const s = `a \\`'],
      ['code', '// inside the template'],
      ['code', '`'],
      ['comment', '// after the template'],
    ],
  },
  {
    name: 'ts: regex literals against division',
    lang: 'ts',
    lines: [
      ['code', 'const r = /[//*]/.test(s)'],
      ['comment', '// after a class holding `/*`'],
      ['code', 'const n = a / b /* a block'],
      ['comment', '   comment, after division */'],
      ['code', 'function f(s) {'],
      ['code', '  return /a\\/*b/.test(s)'],
      ['code', '}'],
      ['comment', '// after a regex'],
    ],
  },
  {
    name: 'tsx: JSX comments, a string holding `/*`, and a block holding a comment',
    lang: 'tsx',
    lines: [
      ['code', 'return ('],
      ['code', '  <div>'],
      ['comment', '    {/* a JSX comment */}'],
      ['comment', '    {/* a JSX comment'],
      ['comment', '        on two lines */}'],
      ['code', '    <a href="https://example/*x">x</a>'],
      ['code', "    <p>Don't panic</p>"],
      ['code', '  </div>'],
      ['code', ')'],
      ['comment', '// after the JSX'],
      ['code', 'if (x) {'],
      ['comment', '  /* a comment in a block */'],
      ['code', '}'],
    ],
  },
  {
    name: 'css: comments, a string holding `/*`, a URL holding `//`',
    lang: 'css',
    lines: [
      ['comment', '/* a comment */'],
      ['code', 'a::after { content: "/*"; }'],
      ['code', '.x { background: url(http://example/x.png); }'],
      ['comment', '/* a block'],
      ['comment', '   comment */'],
      ['code', '.y { color: red; }'],
    ],
  },
  {
    name: 'sh: whole-line comments, a string holding `#`, a trailing `#`',
    lang: 'hash',
    lines: [
      ['code', '#!/usr/bin/env bash'],
      ['comment', '# a comment'],
      ['comment', '  # an indented comment'],
      ['code', 'echo "# not a comment"'],
      ['code', 'x=1 # trailing'],
    ],
  },
  {
    name: 'yaml: whole-line comments, a string holding `#`',
    lang: 'hash',
    lines: [
      ['comment', '# a comment'],
      ['code', 'run: echo "#x"'],
      ['comment', '    # an indented comment'],
    ],
  },
]

const LANG_CASES = [
  ['crates/taiyaku-core/src/lib.rs', 'rust'],
  ['web/src/App.tsx', 'tsx'],
  ['web/src/store.ts', 'ts'],
  ['scripts/coverage-floor.mjs', 'ts'],
  ['web/src/index.css', 'css'],
  ['scripts/check-all.sh', 'hash'],
  ['.forgejo/workflows/ci.yml', 'hash'],
  ['Dockerfile', 'hash'],
  ['deploy/Dockerfile', 'hash'],
  ['docs/Dockerfile.md', undefined],
  ['deploy/compose.yml', undefined],
  ['scripts/permit-wait.py', undefined],
  ['docs/ci.md', undefined],
  ['Makefile', undefined],
]

const file = (path, comment, code) => ({
  path,
  lang: 'ts',
  text: `${'// c\n'.repeat(comment)}${'x()\n'.repeat(code)}`,
})

// Each expects one failure per listed fragment, in order; none means it passes.
// `base` is the tree at the base, the same files when not given.
const SHARE_CASES = [
  { name: 'at its budget', budget: { a: 0.25 }, files: [file('a/x.ts', 25, 75)], fails: [] },
  {
    name: 'rounded to the budget',
    budget: { a: 0.3333 },
    files: [file('a/x.ts', 1, 2)],
    fails: [],
  },
  {
    name: 'one unit above',
    budget: { a: 0.25 },
    files: [file('a/x.ts', 2501, 7499)],
    fails: ['a: comment share 0.2501 is above its budget of 0.2500'],
  },
  {
    name: 'the stale margin exactly, with comment lines removed',
    budget: { a: 0.25 },
    files: [file('a/x.ts', 2450, 7550)],
    base: [file('a/x.ts', 2500, 7500)],
    fails: [],
  },
  {
    name: 'one unit past the stale margin, with comment lines removed',
    budget: { a: 0.25 },
    files: [file('a/x.ts', 2449, 7551)],
    base: [file('a/x.ts', 2500, 7500)],
    fails: ['write "a": 0.2449 in scripts/comment-budget.json'],
  },
  {
    name: 'past the stale margin, with code added and no comment line removed',
    budget: { a: 0.25 },
    files: [file('a/x.ts', 2449, 7551)],
    base: [file('a/x.ts', 2449, 7000)],
    fails: [],
  },
  {
    name: 'a crate the budget does not name',
    budget: {},
    files: [file('crates/new/src/x.ts', 1, 1)],
    fails: ['Add "crates/new": 0.5000 to scripts/comment-budget.json'],
  },
  {
    name: 'an area of AREAS the budget does not name',
    budget: {},
    files: [file('web/tests/x.ts', 1, 3)],
    fails: ['Add "web/tests": 0.2500 to scripts/comment-budget.json'],
  },
  {
    name: 'a budget area with no files',
    budget: { gone: 0.1 },
    files: [file('other/x.ts', 1, 1)],
    fails: ['gone: no source lines under it'],
  },
  {
    name: 'blank lines count as neither, and areas are whole path segments',
    budget: { a: 0.5, ab: 0.5 },
    files: [{ path: 'a/x.ts', lang: 'ts', text: '// c\n\n   \nx()\n' }, file('ab/x.ts', 1, 1)],
    fails: [],
  },
]

// [comment text, the pattern it should trip or null for none, and optionally
// { lang, before, after }: the file's language and the neighbouring comment lines].
const HISTORY_CASES = [
  ['// measured on 2026-10-28', 'a date'],
  ['// spec 2026-09-17 names no section', 'a date'],
  ['// see a92114c4 for the lock', 'a commit hash'],
  ['// shipped in #212', 'a pull request number'],
  ['// the PR that added it', 'a pull request'],
  ['// Previously this read the cache', '"previously"'],
  ['// what the brief originally said', '"originally"'],
  ['// formerly a map', '"formerly"'],
  ['// a minimum was tried and lost', 'a past attempt'],
  ['// slow before this fix', 'a fix'],
  ['// slow before the change', 'a fix'],
  ['// ~~struck~~', 'struck-out text'],
  ['// this used to read the cache first', '"used to"'],
  ['// O148 explains why', null],
  ['// no longer than one frame', null],
  ['// see docs/superpowers/specs/2026-10-28-loading-library-design.md', null],
  ['// spec 2026-09-17 §7.2 gives the order', null],
  ["// the user's ruling of 2026-10-27", null],
  ['// the key is used to sort, and one gets used to it', null],
  ['// a file that was added to the library', null],
  ['// deadbeef and 1234567 are not hashes', null],
  ['// a digest 0123456789abcdef0123456789abcdef0123456789abcdef runs past 40', null],
  ['// the colour `#808080` and the entity &#123;', null],
  ["// (the user's rulings of 2026-10-25)", null],
  ['// 2026-10-25) set the order', null, { before: "// (the user's rulings of" }],
  ['// 2026-10-25 set the order', 'a date', { before: '// measured on the NAS' }],
  ['// the order is the spec 2026-10-28', null, { after: '// §5.2, step one' }],
  ['* the order is the spec 2026-10-28', null, { after: '* §5.2 */' }],
  ['// the order is the spec 2026-10-28', 'a date', { after: '// and its notes' }],
  ['// plain words', null, { before: '// fixed in #212', after: '// previously a map' }],
  ['// notes in docs/notes/2026-10-28.md', null],
  ['// measured on 2026-10-28. It held', 'a date'],
  ['/* the rule draws in #808080 grey */', null, { lang: 'css' }],
  ['/* the rule draws in #808080 grey */', 'a pull request number', { lang: 'ts' }],
  ['/// ~~~', null, { lang: 'rust' }],
  ['/// ~~~rust', null, { lang: 'rust' }],
]

// A line diff for the self-test, giving hunks as `git diff -U0` does.
function diffLines(a, b) {
  const lcs = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0))
  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1])
    }
  }
  const hunks = []
  let [i, j] = [0, 0]
  while (i < a.length || j < b.length) {
    if (a[i] === b[j] && i < a.length && j < b.length) {
      ;[i, j] = [i + 1, j + 1]
      continue
    }
    const [i0, j0] = [i, j]
    while ((i < a.length || j < b.length) && !(i < a.length && j < b.length && a[i] === b[j])) {
      if (j < b.length && (i === a.length || lcs[i][j + 1] >= lcs[i + 1][j])) j += 1
      else i += 1
    }
    hunks.push([i > i0 ? i0 + 1 : i0, i - i0, j > j0 ? j0 + 1 : j0, j - j0])
  }
  return hunks
}

const lines = (n, text = '// line') => Array.from({ length: n }, (_, k) => `${text} ${k + 1}`)
const twenty = lines(20)
const proseAround = (inner) => ['fn a() {}', '/// one', '/// two', ...inner, '/// three', '/// four', 'fn b() {}']

// Each diffs `before` against `after` as one file and expects one failure per
// listed fragment, in order. `oldLang` is the old side's language on a rename.
const CHANGE_CASES = [
  {
    name: 'a marker in an added comment',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '// it used to read the cache'],
    fails: ['"used to"'],
  },
  {
    name: 'a marker on the second line of a three-line addition',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '// one', '// fixed in #212', '// three'],
    fails: ['a pull request number'],
  },
  {
    name: 'a spec pointer whose `§` wraps to the next line',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '// the order is the spec 2026-10-28', '// §5.2, step one', 'y()'],
    fails: [],
  },
  {
    name: 'a ruling whose date wraps to the next line',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', "// the user's rulings of", '// 2026-10-25 set the order', 'y()'],
    fails: [],
  },
  {
    name: 'a spec pointer whose `§` wraps to the next line of a shell comment',
    lang: 'hash',
    before: ['x'],
    after: ['x', '# the order is the spec 2026-10-28', '# §5.2, step one', 'y'],
    fails: [],
  },
  { name: 'a colour in a CSS comment', lang: 'css', before: ['a {}'], after: ['a {}', '/* on #000 */'], fails: [] },
  { name: 'a marker in a string', lang: 'ts', before: [], after: ['const s = "it used to read #12"'], fails: [] },
  {
    name: 'a marker in a trailing comment',
    lang: 'ts',
    before: [],
    after: ['x() // see #212'],
    fails: ['a pull request number'],
  },
  {
    name: 'a marker moved with its line',
    lang: 'ts',
    before: ['a()', '// fixed in #12', 'b()', 'c()'],
    after: ['a()', 'b()', 'c()', '// fixed in #12'],
    fails: [],
  },
  {
    name: 'a block comment re-indented with its marker',
    lang: 'ts',
    before: ['/**', ' * fixed in #12', ' */', 'x()'],
    after: ['  /**', '   * fixed in #12', '   */', 'x()'],
    fails: [],
  },
  {
    name: 'a code edit beside an old marker',
    lang: 'ts',
    before: ['x(1) // see #12'],
    after: ['x(2) // see #12'],
    fails: [],
  },
  {
    name: 'an untouched marker line',
    lang: 'ts',
    before: ['// see #12', 'x(1)'],
    after: ['// see #12', 'x(2)'],
    fails: [],
  },
  {
    name: 'a new 9-line block',
    lang: 'rust',
    before: ['fn a() {}'],
    after: ['fn a() {}', ...lines(9)],
    fails: ['a comment block of 9 counted lines'],
  },
  { name: 'a new 8-line block', lang: 'rust', before: ['fn a() {}'], after: ['fn a() {}', ...lines(8)], fails: [] },
  {
    name: 'a 20-line block trimmed to 12',
    lang: 'rust',
    before: ['fn a() {}', ...twenty, 'fn b() {}'],
    after: ['fn a() {}', ...twenty.slice(0, 12), 'fn b() {}'],
    fails: [],
  },
  {
    name: 'a 20-line block cut by 8 at its top, with 2 lines added inside',
    lang: 'rust',
    before: ['fn a() {}', ...twenty],
    after: ['fn a() {}', ...twenty.slice(8, 14), '// new 1', '// new 2', ...twenty.slice(14)],
    fails: [],
  },
  {
    name: 'a 20-line block cut by 8 at its bottom, with 2 lines added inside',
    lang: 'rust',
    before: ['fn a() {}', ...twenty, 'fn b() {}'],
    after: ['fn a() {}', ...twenty.slice(0, 6), '// new 1', '// new 2', ...twenty.slice(6, 12), 'fn b() {}'],
    fails: [],
  },
  {
    name: 'a 20-line block with one line edited',
    lang: 'rust',
    before: ['fn a() {}', ...twenty],
    after: ['fn a() {}', ...twenty.slice(0, 9), '// edited', ...twenty.slice(10)],
    fails: [],
  },
  {
    name: 'a 20-line block grown by one line',
    lang: 'rust',
    before: ['fn a() {}', ...twenty],
    after: ['fn a() {}', ...twenty, '// one more'],
    fails: ['a comment block of 21 counted lines'],
  },
  {
    name: 'a 30-line `//!` module doc',
    lang: 'rust',
    before: [],
    after: ['mod inner {', ...lines(30, '//! doc'), '}'],
    fails: [],
  },
  {
    name: 'a doc comment with a 12-line fence and 4 prose lines',
    lang: 'rust',
    before: [],
    after: proseAround(['/// ```', ...lines(10, '/// let x ='), '/// ```']),
    fails: [],
  },
  {
    name: 'a fence line of the other kind inside a fence',
    lang: 'rust',
    before: [],
    after: proseAround(['/// ```', '/// ~~~', ...lines(10, '/// let x ='), '/// ```']),
    fails: [],
  },
  {
    name: 'plain `//` lines between the fence lines of a doc',
    lang: 'rust',
    before: ['fn a() {}'],
    after: ['fn a() {}', '/// ```', ...lines(9, '// plain'), '/// ```', 'fn b() {}'],
    fails: ['a comment block of 9 counted lines'],
  },
  {
    name: 'a fence in a Rust `/*!` doc',
    lang: 'rust',
    before: ['fn a() {}'],
    after: ['fn a() {}', '/*!', ' * one', ' * ```', ...lines(10, ' * let x ='), ' * ```', ' */', 'fn b() {}'],
    fails: [],
  },
  {
    name: 'an `# Errors` section, which ends at the next heading',
    lang: 'rust',
    before: [],
    after: proseAround(['/// # Errors', ...lines(3, '/// err'), '/// # Notes', ...lines(4, '/// note')]),
    fails: ['a comment block of 9 counted lines'],
  },
  {
    name: "a file's leading header",
    lang: 'ts',
    before: [],
    after: ['#!/usr/bin/env node', ...lines(12), "import x from 'y'"],
    fails: [],
  },
  {
    name: 'a leading block-comment header with a blank line inside it',
    lang: 'ts',
    before: [],
    after: ['/*', ...lines(5, ' * a'), '', ...lines(9, ' * b'), ' */', '', "import x from 'y'"],
    fails: [],
  },
  {
    name: 'a blank line inside a block comment',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '/*', ...lines(5, ' * a'), '', ...lines(5, ' * b'), ' */'],
    fails: ['a comment block of 10 counted lines'],
  },
  {
    name: 'a JSDoc of 8 text lines between its delimiters',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '/**', ...lines(8, ' * text'), ' */', 'f()'],
    fails: [],
  },
  {
    name: 'a JSDoc of 9 text lines',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '/**', ...lines(9, ' * text'), ' */', 'f()'],
    fails: ['a comment block of 9 counted lines'],
  },
  {
    name: 'a JSX comment of 8 text lines between its delimiters',
    lang: 'tsx',
    before: ['return ('],
    after: ['return (', '  <div>', '    {/*', ...lines(8, '      text'), '    */}', '  </div>'],
    fails: [],
  },
  {
    name: 'a doc of 8 text lines and a bare `///`',
    lang: 'rust',
    before: ['fn a() {}'],
    after: ['fn a() {}', ...lines(4, '/// a'), '///', ...lines(4, '/// b'), 'fn b() {}'],
    fails: [],
  },
  {
    name: 'an unclosed fence in a `//` comment',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '// ```', ...lines(20, '// text')],
    fails: ['a comment block of 21 counted lines'],
  },
  {
    name: 'an unclosed fence in a doc comment',
    lang: 'rust',
    before: ['fn a() {}'],
    after: ['fn a() {}', '/// ```', ...lines(20, '/// text'), 'fn b() {}'],
    fails: ['a comment block of 21 counted lines'],
  },
  {
    name: 'a closed fence in a `//` comment',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '// one', '// ```', ...lines(10, '// code'), '// ```', '// two'],
    fails: ['a comment block of 14 counted lines'],
  },
  {
    name: 'a fence in a JSDoc',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '/**', ' * one', ' * ```ts', ...lines(10, ' * let x ='), ' * ```', ' * two', ' */', 'f()'],
    fails: [],
  },
  {
    name: 'a fence in a `/***` comment, which is no JSDoc',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', '/***', ' * ```', ...lines(8, ' * let x ='), ' * ```', ' */', 'f()'],
    fails: ['a comment block of 11 counted lines'],
  },
  {
    name: '20 lines of `//!` in TypeScript',
    lang: 'ts',
    before: ['x()'],
    after: ['x()', ...lines(20, '//! text')],
    fails: ['a comment block of 20 counted lines'],
  },
  {
    name: 'an `# Errors` heading in a `//` comment',
    lang: 'rust',
    before: ['fn a() {}'],
    after: ['fn a() {}', '// one', '// # Errors', ...lines(8, '// err')],
    fails: ['a comment block of 10 counted lines'],
  },
  {
    name: "an item's 20-line doc at the top of a new file",
    lang: 'rust',
    before: [],
    after: [...lines(20, '/// doc'), 'pub fn f() {}'],
    fails: ['a comment block of 20 counted lines'],
  },
  {
    name: 'a leading header followed by a blank line',
    lang: 'rust',
    before: [],
    after: [...lines(12, '// header'), '', 'pub fn f() {}'],
    fails: [],
  },
  {
    name: "a TS file's first block that runs straight into an export",
    lang: 'ts',
    before: [],
    after: [...lines(12, '// doc'), 'export const x = 1'],
    fails: ['a comment block of 12 counted lines'],
  },
  {
    name: "a TSX file's first block that runs straight into an export",
    lang: 'tsx',
    before: [],
    after: [...lines(12, '// doc'), 'export function EmptyState() {}'],
    fails: ['a comment block of 12 counted lines'],
  },
  {
    name: 'a leading header followed by `export {`',
    lang: 'ts',
    before: [],
    after: [...lines(12), "export { a } from 'b'"],
    fails: [],
  },
  {
    name: 'a leading header followed by `export *`',
    lang: 'ts',
    before: [],
    after: [...lines(12), "export * from 'b'"],
    fails: [],
  },
  {
    name: 'a leading header followed by `pub(crate) use`',
    lang: 'rust',
    before: [],
    after: [...lines(12), 'pub(crate) use a::b;'],
    fails: [],
  },
  {
    name: 'a leading header followed by an inner attribute',
    lang: 'rust',
    before: [],
    after: [...lines(12), '#![allow(dead_code)]'],
    fails: [],
  },
  {
    name: "a shell script's header grown where it runs into `set -euo pipefail`",
    lang: 'hash',
    before: ['#!/usr/bin/env bash', ...lines(10, '# usage'), 'set -euo pipefail', 'run'],
    after: ['#!/usr/bin/env bash', ...lines(11, '# usage'), 'set -euo pipefail', 'run'],
    fails: [],
  },
  {
    name: "a CSS file's header that runs straight into its first rule",
    lang: 'css',
    before: [],
    after: ['/*', ...lines(12, ' * header'), ' */', 'a {}'],
    fails: [],
  },
  {
    name: 'a second block before the first code line',
    lang: 'ts',
    before: [],
    after: [...lines(12, '// header'), '', ...lines(12, '// second'), '', 'x()'],
    fails: ['a comment block of 12 counted lines'],
  },
  {
    name: 'a header line moved into a long block',
    lang: 'ts',
    before: ['// moved', '', 'import y', 'x()', ...lines(10)],
    after: ['', 'import y', 'x()', ...lines(10), '// moved'],
    fails: ['a comment block of 11 counted lines'],
  },
  {
    name: 'a shell header line moved into a long block',
    lang: 'hash',
    before: ['#!/usr/bin/env bash', '# moved', 'set -e', 'x', ...lines(10, '# line')],
    after: ['#!/usr/bin/env bash', 'set -e', 'x', ...lines(10, '# line'), '# moved'],
    fails: ['a comment block of 11 counted lines'],
  },
  {
    name: 'a shell script renamed to a script whose first block runs into code',
    lang: 'ts',
    oldLang: 'hash',
    before: ['#!/usr/bin/env bash', ...lines(10, '# usage'), 'set -e'],
    after: ['#!/usr/bin/env node', ...lines(10, '// usage'), 'x()'],
    fails: ['a comment block of 10 counted lines'],
  },
  {
    name: 'a code line turned into a comment that grows a long block',
    lang: 'rust',
    before: ['fn a() {}', ...lines(14), 'let x = 1;', 'fn b() {}'],
    after: ['fn a() {}', ...lines(14), '// one more', 'fn b() {}'],
    fails: ['a comment block of 15 counted lines'],
  },
  {
    name: 'an exempt comment line replaced by a counted one',
    lang: 'rust',
    before: ['fn a() {}', ...lines(10), '//! module', 'fn b() {}'],
    after: ['fn a() {}', ...lines(10), '// counted', 'fn b() {}'],
    fails: ['a comment block of 11 counted lines'],
  },
  {
    name: 'a hunk that ends just before a grown block',
    lang: 'rust',
    before: ['fn a() {}', '// gone', 'let y = 0;', ...lines(10), 'fn b() {}'],
    after: ['fn a() {}', 'let y = 1;', ...lines(10), '// one more', 'fn b() {}'],
    fails: ['a comment block of 11 counted lines'],
  },
  {
    name: 'a block moved past the code below it',
    lang: 'ts',
    before: ['a()', ...lines(9), ...lines(10, 'b')],
    after: ['a()', ...lines(10, 'b'), ...lines(9)],
    fails: [],
  },
  {
    name: 'a block re-indented and grown by one line',
    lang: 'rust',
    before: ['fn a() {', ...lines(10), '}'],
    after: ['fn a() {', ...lines(10, '    // line'), '    // one more', '}'],
    fails: ['a comment block of 11 counted lines'],
  },
]

const PATCH = [
  'diff --git a/x.rs b/x.rs',
  'index 1111111..2222222 100644',
  '--- a/x.rs',
  '+++ b/x.rs',
  '@@ -3,2 +3 @@ fn a() {',
  '--- a removed line that reads like a header',
  '-// another',
  '+++ an added line that reads like a header',
  '\\ No newline at end of file',
  'diff --git a/old name.ts b/new name.ts',
  'similarity index 90%',
  'rename from old name.ts',
  'rename to new name.ts',
  '--- a/old name.ts\t',
  '+++ b/new name.ts\t',
  '@@ -0,0 +1,2 @@',
  '+a',
  '+b',
  'diff --git a/gone.css b/gone.css',
  'deleted file mode 100644',
  '--- a/gone.css',
  '+++ /dev/null',
  '@@ -1 +0,0 @@',
  '-x',
  'diff --git a/img.png b/img.png',
  'Binary files a/img.png and b/img.png differ',
  'diff --git "a/q\\"t.sh" "b/q\\"t.sh"',
  '--- "a/q\\"t.sh"',
  '+++ "b/q\\"t.sh"',
  '@@ -1 +1 @@',
  '-a',
  '+b',
  '',
].join('\n')

const PATCH_WANT = [
  { from: 'x.rs', to: 'x.rs', hunks: [[3, 2, 3, 1]] },
  { from: 'old name.ts', to: 'new name.ts', hunks: [[0, 0, 1, 2]] },
  { from: 'gone.css', to: null, hunks: [[1, 1, 0, 0]] },
  { from: null, to: null, hunks: [] },
  { from: 'q"t.sh', to: 'q"t.sh', hunks: [[1, 1, 1, 1]] },
]

// [name, the --base given, git's answers, the base wanted or a fragment of the error].
const BASE_CASES = [
  ['a base that resolves', 'origin/main', { 'rev-parse': 'tip', 'merge-base': 'fork' }, 'fork'],
  ['no origin/main', undefined, { 'rev-parse': null }, 'origin/main is not a commit in this clone. Fetch it'],
  ['a shallow clone short of the merge base', 'origin/main', { 'rev-parse': 'tip', 'merge-base': null }, 'share no commit'],
  ['an empty --base', '', {}, 'an empty revision'],
]

// branchChanges over a real repository: a rename with an edit, a deletion not
// yet staged, an edit to a file with no final newline, an untracked file, and
// files of no language it reads. Then judgeCheckout there, with a rule-1 hit
// and a comment line removed in a commit after the base, so the base, HEAD and
// the working tree each measure differently. Returns the failures.
function branchCase() {
  // A GIT_DIR or GIT_INDEX_FILE inherited from a hook would point every git
  // call below at the repository running the self-test.
  for (const key of Object.keys(process.env)) if (key.startsWith('GIT_')) delete process.env[key]
  const dir = mkdtempSync(join(tmpdir(), 'check-comments-'))
  try {
    const config = ['user.name=self-test', 'user.email=self-test@localhost', 'commit.gpgsign=false']
      .concat(['core.hooksPath=/dev/null', 'init.defaultBranch=main'])
      .flatMap((setting) => ['-c', setting])
    const git = (...args) =>
      execFileSync('git', [...config, ...args], { cwd: dir, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim()
    const put = (path, text) => writeFileSync(join(dir, path), text)
    const moved = '// a\nfn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n'
    git('init', '-q')
    put('moved.rs', moved)
    put('gone.css', '/* b */\na {}\n')
    put('tail.ts', 'x()')
    put('notes.md', '# notes\n')
    mkdirSync(join(dir, 'scripts'))
    put('scripts/trim.mjs', '// one\n// two\nx()\n')
    git('add', '.')
    git('commit', '-q', '-m', 'base')
    const base = git('rev-parse', 'HEAD')
    git('mv', 'moved.rs', 'renamed.rs')
    put('renamed.rs', '// a\nfn a() {}\nfn b() {}\n// c\nfn c() {}\nfn d() {}\n')
    rmSync(join(dir, 'gone.css'))
    put('tail.ts', 'x()\n// d')
    put('new.ts', '// e, from #12\n')
    put('new.md', '# e\n')
    put('scripts/trim.mjs', '// one\nx()\n')
    git('commit', '-q', '-m', 'trim', '--', 'scripts/trim.mjs')
    put('scripts/trim.mjs', '// one\nx()\ny()\n')
    const want = [
      { path: 'gone.css', oldLang: 'css', oldText: '/* b */\na {}\n', newText: '', hunks: [[1, 2, 0, 0]] },
      { path: 'renamed.rs', oldLang: 'rust', oldText: moved, newText: readFileSync(join(dir, 'renamed.rs'), 'utf8'), hunks: [[3, 0, 4, 1]] },
      { path: 'scripts/trim.mjs', oldLang: 'ts', oldText: '// one\n// two\nx()\n', newText: '// one\nx()\ny()\n', hunks: [[2, 1, 1, 0], [3, 0, 3, 1]] },
      { path: 'tail.ts', oldLang: 'ts', oldText: 'x()', newText: 'x()\n// d', hunks: [[1, 1, 1, 2]] },
      { path: 'new.ts', oldText: '', newText: '// e, from #12\n', hunks: [[0, 0, 1, 2]] },
    ]
    const failures = []
    const got = branchChanges(dir, base).map(({ path, oldLang, oldText, newText, hunks }) => ({ path, oldLang, oldText, newText, hunks }))
    if (JSON.stringify(got) !== JSON.stringify(want)) {
      failures.push(`branchChanges:\n    want ${JSON.stringify(want)}\n    got  ${JSON.stringify(got)}`)
    }
    const starts = [
      'scripts: comment share 0.3333 is more than 0.0050 below its budget of 0.9000, ' +
        'and this branch removed comment lines (2 at the base; now 1 comment lines, 2 code lines)',
      'new.ts:1: a pull request number, "#12", in a new comment',
    ]
    const judged = judgeCheckout(dir, base, { scripts: 0.9 }).failures
    if (judged.length !== starts.length || !starts.every((start, k) => judged[k].startsWith(start))) {
      failures.push(`judgeCheckout:\n    want ${JSON.stringify(starts)}\n    got  ${JSON.stringify(judged)}`)
    }
    return failures
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

function selfTest() {
  const failures = []
  for (const { name, lang, lines } of LEXER_CASES) {
    const got = lineKinds(lines.map(([, text]) => text).join('\n'), lang)
    lines.forEach(([want, text], k) => {
      if (got[k] !== want) failures.push(`${name}\n    line ${k + 1} ${JSON.stringify(text)}: want ${want}, got ${got[k]}`)
    })
  }
  for (const [path, want] of LANG_CASES) {
    const got = langOf(path)
    if (got !== want) failures.push(`langOf(${path}): want ${want}, got ${got}`)
  }
  for (const { name, budget, files, base, fails } of SHARE_CASES) {
    const got = judgeShares(measureAreas(files, budget), budget, measureAreas(base ?? files, budget))
    const ok = got.length === fails.length && fails.every((fragment, k) => got[k].includes(fragment))
    if (!ok) failures.push(`share: ${name}\n    want ${JSON.stringify(fails)}\n    got  ${JSON.stringify(got)}`)
  }
  for (const [comment, want, { lang = 'ts', before, after } = {}] of HISTORY_CASES) {
    const got = historyMarker(comment, lang, before, after)?.[0] ?? null
    if (got !== want) failures.push(`history: ${JSON.stringify(comment)}: want ${want}, got ${got}`)
  }
  for (const { name, lang, oldLang, before, after, fails } of CHANGE_CASES) {
    const change = { path: 'f', lang, oldLang, oldText: before.join('\n'), newText: after.join('\n'), hunks: diffLines(before, after) }
    const got = judgeChanges([change])
    const ok = got.length === fails.length && fails.every((fragment, k) => got[k].includes(fragment))
    if (!ok) failures.push(`change: ${name}\n    want ${JSON.stringify(fails)}\n    got  ${JSON.stringify(got)}`)
  }
  const parsed = JSON.stringify(parsePatch(PATCH))
  if (parsed !== JSON.stringify(PATCH_WANT)) failures.push(`patch:\n    want ${JSON.stringify(PATCH_WANT)}\n    got  ${parsed}`)
  for (const [name, given, answers, want] of BASE_CASES) {
    let got
    try {
      got = resolveBase((args) => answers[args[0]] ?? null, given)
    } catch (err) {
      got = err instanceof GateError ? err.message : String(err)
    }
    const ok = got === want || (want.includes(' ') && typeof got === 'string' && got.includes(want))
    if (!ok) failures.push(`base: ${name}\n    want ${JSON.stringify(want)}\n    got  ${JSON.stringify(got)}`)
  }
  failures.push(...branchCase())
  const cases =
    LEXER_CASES.length + LANG_CASES.length + SHARE_CASES.length + HISTORY_CASES.length + CHANGE_CASES.length + 1 + BASE_CASES.length + 2
  if (failures.length > 0) {
    console.error(`check-comments: self-test, ${failures.length} failure(s) in ${cases} cases:`)
    for (const failure of failures) console.error(`  ${failure}`)
    return 1
  }
  console.log(`check-comments: self-test, ${cases} cases pass`)
  return 0
}

// --- main ----------------------------------------------------------------------

if (process.argv.includes('--self-test')) process.exit(selfTest())

const root = fileURLToPath(new URL('..', import.meta.url))
const budget = readBudget(root)

const git = (args) => {
  try {
    return execFileSync('git', args, { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim()
  } catch {
    return null
  }
}
const at = process.argv.indexOf('--base')
let base
let judged
try {
  base = resolveBase(git, at < 0 ? undefined : (process.argv[at + 1] ?? ''))
  judged = judgeCheckout(root, base, budget)
} catch (err) {
  if (!(err instanceof GateError)) throw err
  console.error(err.message)
  process.exit(2)
}
const { measured, changes, failures } = judged

if (failures.length > 0) {
  console.error(`check-comments: ${failures.length} failure(s):`)
  for (const failure of failures) console.error(`  ${failure}`)
  process.exit(1)
}
console.log(`check-comments: ${measured.size} areas, each within its comment budget (${BUDGET_PATH})`)
console.log(`check-comments: no history and no grown block in ${changes.length} changed files since ${base.slice(0, 10)}`)
