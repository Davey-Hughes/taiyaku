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
  return words.length > length ? `${words.slice(0, length - 1)}…` : words
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
  const words = (node: Node) =>
    (node instanceof Element ? node : node.parentElement)?.closest<HTMLElement>(
      '[data-text]',
    )
  const own = words(picked.startContainer)
  const block = own?.closest<HTMLElement>('[data-block]')
  if (!own || !block || !text.contains(own)) return undefined
  const nodes = wordNodes(own)
  const offset = (node: Node, at: number) => {
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
  const start = offset(picked.startContainer, picked.startOffset)
  const end =
    words(picked.endContainer) === own
      ? offset(picked.endContainer, picked.endOffset)
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
