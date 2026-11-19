import type { Block, Book, Edition } from './book'

/** A hit: UTF-16 offsets into the block's original `text`, never the folded one. */
export interface Match {
  edition: string
  block: number
  start: number
  end: number
}

export interface Folded {
  text: string
  /** `to[i]` is the original offset of folded character `i`; `to[text.length]` is the original length. */
  to: number[]
}

// Letters only, by script: a middle dot (・), a bracket (「) or another mark
// of punctuation the CJK scripts share is no word. 〇, a letter number, is.
const CJK_LETTER =
  /(?=[\p{L}\p{Nl}])[\p{scx=Han}\p{scx=Hiragana}\p{scx=Katakana}\p{scx=Hangul}\p{scx=Bopomofo}]/u
const KANA_G = /(?=\p{L})[\p{scx=Hiragana}\p{scx=Katakana}]/gu
const KANJI_G = /(?=\p{L})\p{scx=Han}/gu
const LETTER_G = /\p{L}/gu
const SPACE_G = /\s/gu
// Half-width voicing marks are not \p{M}, yet NFKC composes them with the kana before.
const JOINS_BASE = /^[\p{M}ﾞﾟ]/u
// Lower case is not case folding: ß lowers to itself though its capital is
// SS, ς and σ are both Σ lowered, and İ lowers to i and a dot above. ı is
// taken as i, so a Turkish capital I finds it, as it finds i: dotted and
// dotless are one letter to a search. A variation selector picks a glyph,
// not a letter, so it folds to nothing: 葛 with one is still 葛.
const UNFOLDED_G = /[ßςı]|i\u{307}|[\u{FE00}-\u{FE0F}\u{E0100}-\u{E01EF}]/gu
const FOLDED: Record<string, string> = {
  ß: 'ss',
  ς: 'σ',
  ı: 'i',
  'i\u{307}': 'i',
}

// Whether `run` can hold a match of `UNFOLDED_G`: every match has one of these
// code units (a selector past U+FFFF, its high surrogate U+DB40). Japanese text
// is a run a character and almost none hold one, so the replace stays off the
// path nearly every run takes.
function mayUnfold(run: string): boolean {
  for (let k = 0; k < run.length; k++) {
    const c = run.charCodeAt(k)
    if (c === 0xdf || c === 0x131 || c === 0x3c2 || c === 0x307) return true
    if ((c >= 0xfe00 && c <= 0xfe0f) || c === 0xdb40) return true
  }
  return false
}

/**
 * NFKC and case folding, a run at a time: a base and the marks after it fold
 * together (ｶ + ﾞ is ガ), and each folded character maps to where its run began.
 */
export function fold(text: string): Folded {
  let out = ''
  const to: number[] = []
  let i = 0
  while (i < text.length) {
    const start = i
    const code = text.charCodeAt(i)
    const after = text.charCodeAt(i + 1)
    // No mark is below U+0300, so ASCII before one of those folds alone and
    // is its own NFKC; a book is mostly this, and normalize is the cost.
    if (code < 0x80 && (Number.isNaN(after) || after < 0x300)) {
      out += code >= 65 && code <= 90 ? String.fromCharCode(code + 32) : text[i]
      to.push(i)
      i++
      continue
    }
    i += text.codePointAt(i)! > 0xffff ? 2 : 1
    while (i < text.length) {
      const next = String.fromCodePoint(text.codePointAt(i)!)
      if (!JOINS_BASE.test(next)) break
      i += next.length
    }
    const run = text.slice(start, i).normalize('NFKC').toLowerCase()
    const folded = mayUnfold(run)
      ? run.replace(UNFOLDED_G, (c) => FOLDED[c] ?? '')
      : run
    for (let k = 0; k < folded.length; k++) to.push(start)
    out += folded
  }
  to.push(text.length)
  return { text: out, to }
}

const cache = new WeakMap<Block, Folded>()

function foldedBlock(block: Block): Folded {
  let f = cache.get(block)
  if (!f) {
    f = fold(block.text)
    cache.set(block, f)
  }
  return f
}

/** Every occurrence of `query` in the chosen editions, overlapping ones included. */
export function searchBook(
  book: Book,
  query: string,
  editions: string[],
): Match[] {
  const needle = fold(query).text
  if (!needle) return []
  const found: Match[] = []
  for (const name of editions) {
    const edition = book.editions[name]
    if (!edition) continue
    edition.blocks.forEach((block, b) => {
      const f = foldedBlock(block)
      let prior: Match | undefined
      for (
        let at = f.text.indexOf(needle);
        at !== -1;
        at = f.text.indexOf(needle, at + 1)
      ) {
        // A match ending inside one original character (a ㈱ half matched) takes all of it.
        let last = at + needle.length
        while (last < f.text.length && f.to[last] === f.to[last - 1]) last++
        const start = f.to[at]
        const end = f.to[last]
        // Two folded starts inside one original character (… is ...) map to the same match.
        if (prior && prior.start === start && prior.end === end) continue
        prior = { edition: name, block: b, start, end }
        found.push(prior)
      }
    })
  }
  return found
}

/**
 * Fold every block of `editions` ahead of a first search, a slice at a time
 * between other work, so the first query typed does not fold the whole book
 * at once. Answers a way to stop.
 */
export function warm(book: Book, editions: string[], slice = 500): () => void {
  const blocks = editions.flatMap((name) => book.editions[name]?.blocks ?? [])
  let next = 0
  let timer: ReturnType<typeof setTimeout> | undefined
  const run = () => {
    const end = Math.min(blocks.length, next + slice)
    for (; next < end; next++) foldedBlock(blocks[next])
    timer = next < blocks.length ? setTimeout(run, 0) : undefined
  }
  timer = setTimeout(run, 0)
  return () => clearTimeout(timer)
}

/**
 * Where the reader is, to step from: character `at` of block `block` of
 * `edition`, and every edition searched in the order its matches are listed.
 */
export interface Here {
  edition: string
  block: number
  at: number
  order: string[]
}

/**
 * The match `by` on from `from` in `matches`, round from either end. From
 * none (none gone to, or the reader has moved away from it), or one no longer
 * listed (the query or the edition changed), the first match on from `here`,
 * or the last back from it; with no `here`, the first or the last.
 */
export function stepFrom(
  matches: Match[],
  from: Match | undefined,
  by: 1 | -1,
  here?: Here,
): Match | undefined {
  if (matches.length === 0) return undefined
  const at = from
    ? matches.findIndex(
        (m) =>
          m.edition === from.edition &&
          m.block === from.block &&
          m.start === from.start &&
          m.end === from.end,
      )
    : -1
  if (at !== -1) return matches[(at + by + matches.length) % matches.length]
  if (!here) return by === 1 ? matches[0] : matches.at(-1)
  // Matches are listed by edition, then block, then start, so the first one
  // not before the place splits them; one at the place is the screen's own,
  // so on, not back.
  const rank = (edition: string) => here.order.indexOf(edition)
  const ahead = (m: Match) =>
    rank(m.edition) - rank(here.edition) ||
    m.block - here.block ||
    m.start - here.at
  const on = matches.findIndex((m) => ahead(m) >= 0)
  if (by === 1) return matches[on === -1 ? 0 : on]
  return on <= 0 ? matches.at(-1) : matches[on - 1]
}

/**
 * One character of a CJK script is enough to search on; others need two.
 * White space counts for nothing, and a character is one however it folds:
 * ß is one though it folds to ss, e and a combining accent one though typed
 * as two.
 */
export function ready(query: string): boolean {
  const typed = query.replace(SPACE_G, '')
  const text = fold(query).text.replace(SPACE_G, '')
  const count = Math.min([...typed].length, [...text].length)
  return count >= (CJK_LETTER.test(text) ? 1 : 2)
}

/**
 * By its language tag's first subtag, `ja` or `jpn`; where it has none, or
 * `und`, by its first 50 blocks: more than half their letters kana or kanji,
 * and some kana, which Chinese, all kanji, has none of.
 */
export function isJapanese(edition: Edition): boolean {
  const tag = (edition.language ?? '').trim().toLowerCase().split(/[-_]/)[0]
  if (tag !== '' && tag !== 'und') return tag === 'ja' || tag === 'jpn'
  const sample = edition.blocks
    .slice(0, 50)
    .map((b) => b.text)
    .join('')
  const letters = sample.match(LETTER_G)?.length ?? 0
  const kana = sample.match(KANA_G)?.length ?? 0
  const kanji = sample.match(KANJI_G)?.length ?? 0
  return kana > 0 && (kana + kanji) * 2 > letters
}

const isLow = (text: string, i: number) => {
  const c = text.charCodeAt(i)
  return c >= 0xdc00 && c <= 0xdfff
}

/** The text around a match, clipped at the block's ends and never inside a surrogate pair. */
export function context(
  text: string,
  m: Match,
  around = 30,
): { before: string; match: string; after: string } {
  let from = Math.max(0, m.start - around)
  if (from > 0 && isLow(text, from)) from++
  let to = Math.min(text.length, m.end + around)
  if (to < text.length && isLow(text, to)) to--
  return {
    before: text.slice(from, m.start),
    match: text.slice(m.start, m.end),
    after: text.slice(m.end, to),
  }
}
