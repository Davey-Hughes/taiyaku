/**
 * Notes on a book: the page reader's notes document, a note anchored to a
 * block (its slot) and, as a highlight, to a run of that block's text.
 *
 * A note is read where it was made, and in another edition on the block a
 * swap from it lands on, as a paragraph's note rather than a run of words,
 * since a run of one edition's words has no counterpart in another's.
 */
import type { DictionaryEntry } from './api'
import { type Book, counterpart, type Mark } from './book'
import { wordNodes } from './bookPages'
import type { Note, TextRange } from './notes'
import { cryptoRandom, ulid } from './ulid'

/** A note as one edition shows it, on the edition it was made on or carried there. */
export interface Shown {
  note: Note
  own: boolean
}

/** The notes `edition` shows, by block: its own on theirs, another edition's on the block a swap from it lands on. */
export function notesOn(
  book: Book,
  edition: string,
  notes: Note[],
): Map<number, Shown[]> {
  const on = new Map<number, Shown[]>()
  for (const note of notes) {
    if (!(note.edition in book.editions)) continue
    const own = note.edition === edition
    const block = own
      ? note.slot
      : counterpart(book, note.edition, edition, note.slot)
    on.set(block, [...(on.get(block) ?? []), { note, own }])
  }
  return on
}

/**
 * A run of a block's text as it is drawn: the highlight over it if any, and
 * the marks over all of it.
 */
export interface Segment {
  text: string
  note?: Note
  marks: Mark[]
  /** The images set inside the paragraph just before the run's text, each zero-width. */
  images?: Mark[]
  /**
   * Whether the run opens the base of one of its marks' rubies, so a base cut
   * in two by a sentence or highlight has its reading drawn once.
   */
  rubyStart?: boolean
  /** Whether the run is inside the search match being lit; never a note, never saved. */
  found?: true
  /**
   * The numbers in the run's text set upright across a vertical line, as
   * ranges of that text; drawn so, the text as it was.
   */
  upright?: [number, number][]
}

/** A run of a block's text, and the highlight over it if any. */
export type Highlighted = Pick<Segment, 'text' | 'note'>

/**
 * `text` cut where `highlights` begin and end, each run under the newest
 * highlight covering it, so a highlight drawn over another shows on top.
 */
export function segments(text: string, highlights: Note[]): Highlighted[] {
  const marks = highlights
    .filter((note): note is Note & { text: TextRange } => !!note.text)
    .toSorted((a, b) => a.created.localeCompare(b.created))
  const within = (at: number) => Math.max(0, Math.min(text.length, at))
  const cuts = [
    ...new Set([
      0,
      text.length,
      ...marks.flatMap((m) => [within(m.text.start), within(m.text.end)]),
    ]),
  ].toSorted((a, b) => a - b)
  const out: Highlighted[] = []
  for (const [i, from] of cuts.entries()) {
    const to = cuts[i + 1]
    if (to === undefined || to <= from) continue
    const note = marks.findLast((m) => m.text.start <= from && m.text.end >= to)
    const last = out.at(-1)
    if (last && last.note === note) last.text += text.slice(from, to)
    else out.push({ text: text.slice(from, to), note })
  }
  return out
}

/** What a saved word's note says of it: its headword, part of speech and senses. */
export function meaning(entry: DictionaryEntry): string {
  const pos = entry.pos ? ` [${entry.pos}]` : ''
  return `${entry.word}${pos}: ${entry.glosses.join('; ')}`
}

/** A field of a CSV row, quoted where it must be. */
const field = (text: string) =>
  text.includes('"') || /[,\r\n]/.test(text)
    ? `"${text.replaceAll('"', '""')}"`
    : text

/** `text` escaped as HTML text. */
const html = (text: string) =>
  text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')

/**
 * `notes`' highlights on `book` as rows for Anki to import: the words marked
 * on the front, and on the back what the note says of them over the sentence
 * they were read in, so a card is learnt in its context. The back's line
 * break and italics are HTML, which an import reads in every field alike, so
 * each field's own text is escaped.
 */
export function ankiCsv(book: Book, notes: Note[]): string {
  return notes
    .filter((note) => note.text && book.editions[note.edition])
    .map((note) => {
      const block = book.editions[note.edition].blocks[note.slot]
      const range = note.text as TextRange
      if (!block) return undefined
      const words = html(block.text.slice(range.start, range.end))
      const sentence = block.sentences.find(([, end]) => end > range.start) ?? [
        0,
        block.text.length,
      ]
      const context = html(block.text.slice(sentence[0], sentence[1]))
      const back = note.body
        ? `${html(note.body)}<br><i>${context}</i>`
        : `<i>${context}</i>`
      return `${field(words)},${field(back)}`
    })
    .filter((row): row is string => row !== undefined)
    .join('\n')
}

/**
 * One piece of a block as it is drawn: one of its sentences, numbered within
 * the block, or the space between two, with the highlight runs inside it.
 */
export interface Piece {
  sentence?: number
  runs: Segment[]
}

/**
 * `text` cut into its `sentences` and the space between them, each piece the
 * runs of `segments` inside it, so a highlight crossing a sentence's end
 * carries on into the next, and those runs cut again where `marks` begin and
 * end. An image, taking no text, rides on the run after it, or on an empty
 * run closing the last piece where it ends the text. `found`, a match's
 * range, cuts the runs at its ends and marks those inside it. `upright`, in
 * vertical text, marks on each run its part of each number of one or two
 * digits and cuts none, so a ruby's base or a link holding one stays whole.
 */
export function pieces(
  text: string,
  sentences: [number, number][],
  highlights: Note[],
  marks: Mark[] = [],
  found?: [number, number],
  upright = false,
): Piece[] {
  const within = (at: number) => Math.max(0, Math.min(text.length, at))
  const spans = marks.filter((m) => m.kind !== 'image' && m.start < m.end)
  // Found in the whole text, not in a run's, so a run cut inside a number is
  // never taken for a number of its own. Each run still draws its part in an
  // upright span of its own, so a run edge inside a number, a highlight's, a
  // match's, a mark's or a sentence's, stacks its digits (O356). Not one
  // touching any other digit, a full-width one too: two digits of a longer
  // number set across the line would read as a number of their own.
  const numbers = upright
    ? [...text.matchAll(/(?<!\p{Nd})[0-9]{1,2}(?!\p{Nd})/gu)].map(
        (m) => [m.index, m.index + m[0].length] as const,
      )
    : []
  const images = marks.filter((m) => m.kind === 'image')
  const highlighted: { from: number; to: number; note?: Note }[] = []
  let at = 0
  for (const run of segments(text, highlights)) {
    highlighted.push({ from: at, to: at + run.text.length, note: run.note })
    at += run.text.length
  }
  const cuts = [
    ...new Set([
      0,
      text.length,
      ...highlighted.map((run) => run.to),
      ...spans.flatMap((m) => [within(m.start), within(m.end)]),
      ...images.map((m) => within(m.start)),
      ...sentences.flat().map(within),
      ...(found ? found.map(within) : []),
    ]),
  ].toSorted((a, b) => a - b)
  // `runs[i]` is the text from `cuts[i]` to the next cut. Every span and
  // highlighted run starts and ends on a cut, so each lies wholly over or off
  // a run, and one walk along the cuts finds what is over every run.
  const opening = groupBy([...spans.keys()], (k) => within(spans[k].start))
  const imagesAt = groupBy(images, (m) => within(m.start))
  const runs: Segment[] = []
  // The spans over the run being built, by index, so they stay in `marks`'
  // order, the order `same` compares by.
  let open: number[] = []
  let h = 0
  for (let i = 0; i + 1 < cuts.length; i++) {
    const from = cuts[i]
    const to = cuts[i + 1]
    const begun = opening.get(from)
    if (begun) open = [...open, ...begun].toSorted((a, b) => a - b)
    open = open.filter((k) => spans[k].end >= to)
    const over = open.map((k) => spans[k])
    while (highlighted[h].to < to) h++
    const here = imagesAt.get(from)
    runs.push({
      text: text.slice(from, to),
      note: highlighted[h].note,
      marks: over,
      ...(found &&
        found[0] < found[1] &&
        found[0] <= from &&
        found[1] >= to && { found: true as const }),
      ...(here && { images: here }),
      ...(over.some((m) => m.kind === 'ruby' && m.start === from) && {
        rubyStart: true,
      }),
    })
  }
  /** The index of the first cut at or after `from`, searched for since sentences need not come in order. */
  const firstCut = (from: number) => {
    let lo = 0
    let hi = cuts.length
    while (lo < hi) {
      const mid = (lo + hi) >> 1
      if (cuts[mid] < from) lo = mid + 1
      else hi = mid
    }
    return lo
  }
  const same = (a: Segment, b: Segment) =>
    a.note === b.note &&
    a.found === b.found &&
    a.marks.length === b.marks.length &&
    a.marks.every((m, i) => m === b.marks[i])
  // A copy of each run is what is joined onto, since a run can fall in two
  // pieces where sentences overlap.
  const cut = (from: number, to: number): Segment[] => {
    const out: Segment[] = []
    const starts: number[] = []
    for (let i = firstCut(from); i < runs.length && cuts[i + 1] <= to; i++) {
      const next = runs[i]
      const last = out.at(-1)
      if (last && !next.images && !next.rubyStart && same(last, next)) {
        last.text += next.text
      } else {
        out.push({ ...next })
        starts.push(cuts[i])
      }
    }
    return numbers.length === 0
      ? out
      : out.map((run, k) => numbered(run, starts[k]))
  }
  /** `run`, starting at `start` in the text, with the part of each number it holds. */
  const numbered = (run: Segment, start: number): Segment => {
    const end = start + run.text.length
    const held = numbers
      .filter(([from, to]) => from < end && to > start)
      .map(([from, to]): [number, number] => [
        Math.max(from, start) - start,
        Math.min(to, end) - start,
      ])
    return held.length === 0 ? run : { ...run, upright: held }
  }
  const out: Piece[] = []
  at = 0
  for (const [sentence, [start, end]] of sentences.entries()) {
    if (at < start) out.push({ runs: cut(at, start) })
    out.push({ sentence, runs: cut(start, end) })
    at = end
  }
  if (at < text.length) out.push({ runs: cut(at, text.length) })
  const closing = imagesAt.get(text.length)
  if (closing) {
    if (out.length === 0) out.push({ runs: [] })
    out[out.length - 1].runs.push({ text: '', marks: [], images: closing })
  }
  return out
}

/** `items` grouped by `key`, each group in the order of `items`. */
function groupBy<T>(items: T[], key: (item: T) => number): Map<number, T[]> {
  const groups = new Map<number, T[]>()
  for (const item of items) {
    const group = groups.get(key(item))
    if (group) group.push(item)
    else groups.set(key(item), [item])
  }
  return groups
}

/** A new note on `edition`'s block `block` in `colour`, a highlight where `text` is given. */
export function newNote(
  edition: string,
  block: number,
  colour: string,
  now: Date,
  text?: TextRange,
): Note {
  const stamp = now.toISOString()
  return {
    id: ulid(now.getTime(), cryptoRandom),
    edition,
    slot: block,
    ...(text && { text }),
    color: colour,
    body: '',
    created: stamp,
    updated: stamp,
  }
}

/** What a note marks, for a list: its highlighted words, or its block's opening words. */
export function excerpt(book: Book, note: Note, length = 80): string {
  const block = book.editions[note.edition]?.blocks[note.slot]?.text ?? ''
  const words = note.text ? block.slice(note.text.start, note.text.end) : block
  if (words.length <= length) return words
  // A character is kept whole or left out, never cut before the ellipsis:
  // one written as two UTF-16 units, as an emoji or a rarer kanji is, and the
  // marks a font draws as part of it, as a kana's voicing mark or a kanji's
  // variation selector (`characterCut`), which cut off would change it.
  return `${words.slice(0, characterCut(words, length - 1))}…`
}

/** What a selection on the text can become: a run of one block, and where to set the bar that offers it. */
export interface Selected {
  block: number
  range: TextRange
  /** The selection's box on the screen, which the bar is set beside. */
  near: { left: number; top: number; right: number; bottom: number }
}

/**
 * The run of one block's text the page's selection covers, from where it
 * starts to where it ends or its block does; none where nothing in `text`'s
 * blocks is selected. A block's own words are its `[data-text]`, so a note's
 * mark beside them counts for nothing, and its `wordNodes`, so neither does a
 * peek set among them.
 */
export function selectionIn(text: HTMLElement | null): Selected | undefined {
  const selection = window.getSelection()
  if (!text || !selection || selection.isCollapsed || !selection.rangeCount) {
    return undefined
  }
  const picked = selection.getRangeAt(0)
  const own = wordsHolding(picked.startContainer)
  const block = own?.closest<HTMLElement>('[data-block]')
  if (!own || !block || !text.contains(own)) return undefined
  const nodes = wordNodes(own)
  const start = offsetOf(nodes, picked.startContainer, picked.startOffset)
  const end =
    wordsHolding(picked.endContainer) === own
      ? offsetOf(nodes, picked.endContainer, picked.endOffset)
      : nodes.reduce((n, word) => n + word.length, 0)
  if (end <= start) return undefined
  const box = picked.getBoundingClientRect?.()
  return {
    block: Number(block.dataset.block),
    range: { start, end },
    near: {
      left: box?.left ?? 0,
      top: box?.top ?? 0,
      right: box?.right ?? 0,
      bottom: box?.bottom ?? 0,
    },
  }
}

/** The block's own words a DOM point's node is in, its `[data-text]`. */
const wordsHolding = (node: Node) =>
  (node instanceof Element ? node : node.parentElement)?.closest<HTMLElement>(
    '[data-text]',
  )

/**
 * How many characters of `nodes`, a block's `wordNodes`, come before the DOM
 * point `at` in `node`, which need not be one of them: an element, or an
 * aside's text, counts the words before it.
 */
function offsetOf(nodes: Text[], node: Node, at: number): number {
  const point = document.createRange()
  point.setStart(node, at)
  let n = 0
  for (const word of nodes) {
    if (word === node) return n + at
    if (point.comparePoint(word, word.length) > 0) break
    n += word.length
  }
  return n
}

/**
 * The block, and the character of its own words, at the point `x`, `y` of
 * the viewport: the character either side of the browser's caret there that
 * the point is on. None off a block's words, on no character, on an aside set
 * among them, or where the browser has no caret from a point. A ruby's reading
 * counts as the start of its base, a number set upright as its first digit,
 * and a reading in an aside as the aside's place among the words. Where a range
 * cannot be measured, the caret's own offset is taken as it is.
 */
export function offsetAtPoint(
  text: HTMLElement,
  x: number,
  y: number,
): { block: number; at: number } | undefined {
  const caret = caretAt(x, y)
  if (!caret) return undefined
  const pressed = characterByCaret(text, caret, x, y)
  if (pressed) return pressed
  // Beside a reading the browser's caret can land on another line's reading,
  // or on the base next to the character pressed. Half a line's height either
  // way it lands among a line's glyphs, and the point must still be on what it
  // finds there.
  const block = (
    caret.node instanceof Element ? caret.node : caret.node.parentElement
  )?.closest('[data-block]')
  const style = block ? getComputedStyle(block) : undefined
  const half = Number.parseFloat(style?.lineHeight ?? '') / 2
  if (!style || !half) return undefined
  for (const side of [half, -half]) {
    const again = isVertical(style)
      ? caretAt(x + side, y)
      : caretAt(x, y + side)
    const found = again && characterByCaret(text, again, x, y)
    if (found) return found
  }
  return undefined
}

/** The character of `text`'s words that `caret`, the browser's for or near the point `x`, `y`, shows the point to be on. */
function characterByCaret(
  text: HTMLElement,
  caret: { node: Node; offset: number },
  x: number,
  y: number,
): { block: number; at: number } | undefined {
  let { node, offset } = caret
  const aside = (node instanceof Element ? node : node.parentElement)?.closest(
    '[data-aside]',
  )
  if (aside) {
    const ruby = aside.tagName === 'RT' ? aside.closest('ruby') : null
    if (!ruby) return undefined
    node = ruby
    offset = 0
  }
  const own = wordsHolding(node)
  const block = own?.closest<HTMLElement>('[data-block]')
  if (!own || !block || !text.contains(own)) return undefined
  const nodes = wordNodes(own)
  const at = offsetOf(nodes, node, offset)
  const found = (at: number) => ({ block: Number(block.dataset.block), at })
  if (typeof Range.prototype.getClientRects !== 'function') return found(at)
  // A reading can stand outside its base's line, so its own box decides.
  if (aside && [...aside.getClientRects()].some((box) => inside(box, x, y))) {
    return found(at)
  }
  // The caret is the boundary nearest the point: after a character whose far
  // half is pressed, at the end of that character's own text node where it
  // ends one, as a ruby's base or a highlight does; before one whose near half
  // is. A point beside a line or between two still has a nearest boundary, at
  // a Japanese line's end the start of the next line, so the point must be on
  // one of the two characters there.
  for (const side of ['before', 'after'] as const) {
    const character = characterIn(nodes, at, side)
    const drawn = character && drawnWhole(character)
    if (drawn && onCharacter(drawn, block, x, y)) {
      return found(offsetOf(nodes, drawn.startContainer, drawn.startOffset))
    }
  }
  return undefined
}

/**
 * What the line draws `character` as: a digit of a number set upright is the
 * whole number, one character across the line, so neither digit can be told
 * from the other by where a finger lands on it.
 */
function drawnWhole(character: Range): Range {
  const upright =
    character.startContainer.parentElement?.closest('.book-upright')
  if (!upright) return character
  const number = document.createRange()
  number.selectNodeContents(upright)
  return number
}

/**
 * A range over the character just `before` or `after` block offset `at` of
 * `nodes`, a block's `wordNodes`, taken whole (`characterStart`,
 * `characterEnd`); none at the block's start or end.
 */
function characterIn(
  nodes: Text[],
  at: number,
  side: 'before' | 'after',
): Range | undefined {
  let left = at
  for (const node of nodes) {
    if (side === 'before' ? left <= node.length : left < node.length) {
      if (side === 'before' && left === 0) return undefined
      const [from, to] =
        side === 'before'
          ? [characterStart(node.data, left), left]
          : [left, characterEnd(node.data, left)]
      const range = document.createRange()
      range.setStart(node, from)
      range.setEnd(node, to)
      return range
    }
    left -= node.length
  }
  return undefined
}

/** Whether a combining mark starts at `at` of `text`: variation selectors are marks too, and a font draws each as part of the character before it. */
const markAt = (text: string, at: number) =>
  /\p{M}/u.test(String.fromCodePoint(text.codePointAt(at) ?? 0))

/**
 * Where the character that ends at UTF-16 offset `at` of `text` starts: a
 * surrogate pair taken whole, and the base under any marks after it, so a
 * kanji's variation selector is never taken for a character of its own.
 */
export function characterStart(text: string, at: number): number {
  let from = at
  while (from > 0) {
    const low = text.charCodeAt(from - 1)
    const high = text.charCodeAt(from - 2)
    from -=
      low >= 0xdc00 && low <= 0xdfff && high >= 0xd800 && high <= 0xdbff ? 2 : 1
    if (!markAt(text, from)) break
  }
  return from
}

/** Where the character that starts at UTF-16 offset `at` of `text` ends, its pair and marks taken with it (`characterStart`). */
export function characterEnd(text: string, at: number): number {
  let to = at
  do {
    to += (text.codePointAt(to) ?? 0) > 0xffff ? 2 : 1
  } while (to < text.length && markAt(text, to))
  return to
}

/** Where to cut `text` at UTF-16 offset `at` or before so as to part no character: `at`, or the start of the character it falls in. */
export function characterCut(text: string, at: number): number {
  const last = characterStart(text, at)
  return characterEnd(text, last) > at ? last : at
}

const isVertical = (style: CSSStyleDeclaration) =>
  /^(vertical|sideways)/.test(style.writingMode)

const inside = (box: DOMRect, x: number, y: number) =>
  x >= box.left && x <= box.right && y >= box.top && y <= box.bottom

/**
 * Whether the point `x`, `y` is on the character `range` holds, in `block`:
 * within its box along the line, and across the line in its glyph or in the
 * leading beside it. The browser does not share a line's leading out evenly
 * round the glyph, so the whole of it counts on either side: two lines'
 * leading then meet, and no point between them is off both. A margin is not
 * leading, so the block's own box ends a line that no other follows. Vertical
 * text runs its lines down, so there the line runs along y.
 */
function onCharacter(
  range: Range,
  block: Element,
  x: number,
  y: number,
): boolean {
  const style = getComputedStyle(range.startContainer.parentElement as Element)
  const vertical = isVertical(style)
  const line = Number.parseFloat(style.lineHeight) || 0
  const inBlock = [...block.getClientRects()].some((box) => inside(box, x, y))
  const [along, across] = vertical ? [y, x] : [x, y]
  return [...range.getClientRects()].some((box) => {
    const [start, end, top, bottom] = vertical
      ? [box.top, box.bottom, box.left, box.right]
      : [box.left, box.right, box.top, box.bottom]
    const leading = line - (bottom - top)
    const inGlyph = across >= top && across <= bottom
    const inLeading =
      inBlock && across >= top - leading && across <= bottom + leading
    return along >= start && along <= end && (inGlyph || inLeading)
  })
}

/** The DOM node and offset of the caret at a point, by whichever API the browser has. */
function caretAt(
  x: number,
  y: number,
): { node: Node; offset: number } | undefined {
  if (typeof document.caretPositionFromPoint === 'function') {
    const caret = document.caretPositionFromPoint(x, y)
    return caret ? { node: caret.offsetNode, offset: caret.offset } : undefined
  }
  if (typeof document.caretRangeFromPoint === 'function') {
    const caret = document.caretRangeFromPoint(x, y)
    return caret
      ? { node: caret.startContainer, offset: caret.startOffset }
      : undefined
  }
  return undefined
}

/**
 * A DOM range over `range` of block `block`'s own words in `text`, past the
 * asides set among them, as `selectionIn` would read it back; none where the
 * block is not drawn on its own.
 */
export function rangeIn(
  text: HTMLElement | null,
  block: number,
  range: TextRange,
): Range | undefined {
  const own = text?.querySelector(`[data-block="${block}"] > [data-text]`)
  return own ? rangeOf(own, range) : undefined
}

/**
 * A DOM range over `range` of the words `own`, a block's `[data-text]`,
 * holds: `rangeIn` for a caller that has the block's element in hand, and
 * so need not search the whole text for it once for each range.
 */
export function rangeOf(own: Element, range: TextRange): Range | undefined {
  const nodes = wordNodes(own)
  // A start on a boundary between two nodes goes in the later, an end in the
  // earlier, so neither takes in a node it covers none of.
  const point = (at: number, end: boolean): [Node, number] | undefined => {
    let left = at
    for (const node of nodes) {
      if (end ? left <= node.length : left < node.length) return [node, left]
      left -= node.length
    }
    const last = nodes.at(-1)
    return last && left === 0 ? [last, last.length] : undefined
  }
  const from = point(range.start, false)
  const to = point(range.end, true)
  if (!from || !to) return undefined
  const out = document.createRange()
  out.setStart(...from)
  out.setEnd(...to)
  return out
}
