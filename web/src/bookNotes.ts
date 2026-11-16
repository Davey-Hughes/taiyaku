/**
 * Notes on a book: the page reader's notes document, a note anchored to a
 * block (its slot) and, as a highlight, to a run of that block's text.
 *
 * A note is read where it was made, and in another edition on the block a
 * swap from it lands on, as a paragraph's note rather than a run of words,
 * since a run of one edition's words has no counterpart in another's.
 */
import type { DictionaryEntry } from './api'
import { type Book, counterpart } from './book'
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

/** A run of a block's text, and the highlight over it if any. */
export interface Segment {
  text: string
  note?: Note
}

/**
 * `text` cut where `highlights` begin and end, each run under the newest
 * highlight covering it, so a highlight drawn over another shows on top.
 */
export function segments(text: string, highlights: Note[]): Segment[] {
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
  const out: Segment[] = []
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
  /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text

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
 * runs of `segments` that fall inside it, so a highlight crossing a
 * sentence's end carries on into the next.
 */
export function pieces(
  text: string,
  sentences: [number, number][],
  highlights: Note[],
): Piece[] {
  const runs = segments(text, highlights)
  const cut = (from: number, to: number): Segment[] => {
    const out: Segment[] = []
    let at = 0
    for (const run of runs) {
      const [start, end] = [at, at + run.text.length]
      at = end
      const [a, b] = [Math.max(start, from), Math.min(end, to)]
      if (a < b) {
        out.push({ ...run, text: run.text.slice(a - start, b - start) })
      }
    }
    return out
  }
  const out: Piece[] = []
  let at = 0
  for (const [sentence, [start, end]] of sentences.entries()) {
    if (at < start) out.push({ runs: cut(at, start) })
    out.push({ sentence, runs: cut(start, end) })
    at = end
  }
  if (at < text.length) out.push({ runs: cut(at, text.length) })
  return out
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
  at: { x: number; y: number }
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
    at: { x: box?.left ?? 0, y: (box?.bottom ?? 0) + 8 },
  }
}
