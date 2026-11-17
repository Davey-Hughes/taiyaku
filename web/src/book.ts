/**
 * A book: editions read as text blocks, and how they line up.
 *
 * `taiyaku-serve`'s `GET /api/works/{work}/book` sends every book edition the
 * account sees, and the hub's alignment to each other edition. A swap between
 * two editions neither of which is the hub goes through it, as the manga
 * reader's does.
 */
import { getJson, sendJson } from './api'
import type { ReaderBinding } from './keys'

/** Where a link goes: a block of the edition it is drawn from, or a web page. */
export interface LinkTo {
  /** The block it goes to, in this edition. */
  block?: number
  url?: string
  /** Whether it calls a footnote rather than sending the reader on. */
  note: boolean
}

/**
 * What a link holding only an image is called where the image has no alt
 * text to name it: where it goes, or the link would be read out as nothing.
 */
export function imageLinkLabel(link: LinkTo, alt: string): string | undefined {
  if (alt) return undefined
  if (link.url !== undefined) return siteOf(link.url)
  return link.note ? 'Note' : 'Linked passage'
}

/**
 * The site a web address goes to, `x.org`, which is shorter to hear than the
 * whole address. An address with no host (`mailto:`) or one that does not
 * parse is given whole, as it still says where it goes.
 */
function siteOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, '') || url
  } catch {
    return url
  }
}

/**
 * A run of a block's text set apart, `[start, end)` in UTF-16 units as its
 * sentences are counted: a style, a reading over its base, a link, or an
 * image set inside the paragraph, which takes no text (`start == end`). A
 * mark never adds to `text`, so a note's offsets count the same with it. An
 * image set in a link carries the link itself, since a link mark covers text
 * and the image takes none.
 */
export type Mark =
  | { kind: 'italic' | 'bold' | 'sup' | 'sub'; start: number; end: number }
  | { kind: 'ruby'; start: number; end: number; reading: string }
  | ({ kind: 'link'; start: number; end: number } & LinkTo)
  | {
      kind: 'image'
      start: number
      end: number
      image: number
      alt: string
      link?: LinkTo
    }

/** An image the book sets between blocks, before block `before` (its block count where it ends the book), and the link it is set in, if any. */
export interface Figure {
  before: number
  image: number
  alt: string
  link?: LinkTo
}

export interface Block {
  text: string
  heading: boolean
  /** Its sentences, each `[start, end)` of `text` in UTF-16 units, as a note's run of text is counted. */
  sentences: [number, number][]
  /** The verse line's number, where the book prints one. */
  line?: number | null
  /** Where the book styles, glosses or links its text; none sent where it does nowhere. */
  marks?: Mark[]
}

/**
 * One link of an alignment: `[aStart, aEnd, bStart, bEnd]`, blocks `aStart`
 * up to `aEnd` of the hub against `bStart` up to `bEnd` of the other edition.
 * One side is empty where the other's blocks have no counterpart.
 */
export type Link = [number, number, number, number]

export interface Edition {
  label: string
  /** The EPUB's `dc:language`, or null where it declares none. */
  language: string | null
  blocks: Block[]
  /** Its images between blocks, in order; none sent where it has none. */
  figures?: Figure[]
}

export interface Book {
  title: string
  hub: string
  editions: Record<string, Edition>
  /**
   * The hub's alignment to each other edition, block by block and, inside
   * those links, sentence by sentence (each sentence numbered through its
   * edition, as `firstSentences` counts), and the links a person pinned in
   * it, `[hubBlock, block]`.
   */
  alignments: {
    to: string
    links: Link[]
    sentences: Link[]
    pins: [number, number][]
  }[]
}

export function getBook(work: string): Promise<Book> {
  return getJson<Book>(`/api/works/${encodeURIComponent(work)}/book`)
}

/**
 * Where `edition`'s image `image` is served: its place among the archive's
 * images, as a mark or figure numbers it, since the book's own paths name
 * files the server does not hand out by name.
 */
export function bookImageUrl(
  work: string,
  edition: string,
  image: number,
): string {
  return `/api/works/${encodeURIComponent(work)}/${encodeURIComponent(edition)}/book/images/${image}`
}

/** The editions in the order a swap cycles through them, the hub first. */
export function editionOrder(book: Book): string[] {
  const others = Object.keys(book.editions).filter((key) => key !== book.hub)
  return [book.hub, ...others.sort()]
}

/** The edition a swap from `current` goes to. */
export function nextEdition(book: Book, current: string): string {
  const order = editionOrder(book)
  return order[(order.indexOf(current) + 1) % order.length]
}

/**
 * The block of `to` that shows what `from`'s block `block` shows: the first
 * block of its counterpart. A block with no counterpart lands where one would
 * be, the next block the other side has, or its last at the end of the book.
 */
export function counterpart(
  book: Book,
  from: string,
  to: string,
  block: number,
): number {
  if (from === to) return block
  const inHub =
    from === book.hub ? block : across(linksTo(book, from), block, 'b')
  return to === book.hub ? inHub : across(linksTo(book, to), inHub, 'a')
}

/**
 * The blocks of `to` that show what `from`'s block `block` shows, as `[start,
 * end)`, empty where `to` has nothing for it. A swap lights them, and a peek
 * shows them.
 */
export function span(
  book: Book,
  from: string,
  to: string,
  block: number,
): [number, number] {
  if (from === to) return [block, block + 1]
  const inHub: [number, number] =
    from === book.hub
      ? [block, block + 1]
      : over(linksTo(book, from), [block, block + 1], 'b')
  return to === book.hub ? inHub : over(linksTo(book, to), inHub, 'a')
}

/**
 * Where each block's sentences start, numbered through the edition: block
 * `i`'s are `[at[i], at[i + 1])`.
 */
export function firstSentences(blocks: Block[]): number[] {
  const at = [0]
  for (const block of blocks)
    at.push(at[at.length - 1] + block.sentences.length)
  return at
}

/**
 * The sentence holding character `at` of block `block`, numbered through the
 * edition: the block's first ending past it, or its first where none does.
 */
export function sentenceAt(blocks: Block[], block: number, at: number): number {
  const within = blocks[block]?.sentences.findIndex(([, end]) => end > at) ?? -1
  return firstSentences(blocks)[block] + Math.max(within, 0)
}

/**
 * The line block `block` is, counted on from the nearest number the book
 * prints at or before it, each paragraph a line and a heading none; none
 * where no number comes before it.
 */
export function lineAt(blocks: Block[], block: number): number | undefined {
  let counted = 0
  for (let i = Math.min(block, blocks.length - 1); i >= 0; i--) {
    const at = blocks[i]
    if (at.line != null) return at.line + counted
    if (!at.heading) counted++
  }
  return undefined
}

/** A stretch of a block's text with the marks that fall in it, counted from its own start. */
export interface Excerpt {
  text: string
  marks: Mark[]
}

/**
 * `block`'s text from `from` to `to` with its marks clipped to that and
 * shifted to start at 0, so it is drawn as the block is. A zero-width mark (an
 * image) is kept where it sits from `from` up to `to`, and at `to` only where
 * that is the block's end: abutting sentences drawn together would otherwise
 * set an image on their boundary twice.
 */
export function excerpt(block: Block, [from, to]: [number, number]): Excerpt {
  const marks: Mark[] = []
  for (const m of block.marks ?? []) {
    if (m.start === m.end) {
      if (m.start < from || m.start > to) continue
      if (m.start === to && to < block.text.length) continue
    } else if (m.end <= from || m.start >= to) continue
    marks.push({
      ...m,
      start: Math.max(m.start, from) - from,
      end: Math.min(m.end, to) - from,
    })
  }
  return { text: block.text.slice(from, to), marks }
}

/** Every sentence of `edition`, in order, numbered as `firstSentences` counts. */
export function sentenceTexts(edition: Edition): string[] {
  return edition.blocks.flatMap((block) =>
    block.sentences.map(([from, to]) => block.text.slice(from, to)),
  )
}

/**
 * `sentenceSpan` for every sentence of `from` at once, each `to`'s
 * `[start, end)` for it, in one pass over the links rather than one a
 * sentence: an interlinear text asks it of every sentence in the book.
 */
export function sentenceMap(
  book: Book,
  from: string,
  to: string,
): [number, number][] {
  const count = (key: string) =>
    book.editions[key].blocks.reduce((n, b) => n + b.sentences.length, 0)
  /** Each sentence of side `side` of `links`, mapped to the other side. */
  const across = (links: Link[], n: number, side: 'a' | 'b') => {
    const out: [number, number][] = Array.from({ length: n }, () => [0, 0])
    const [start, end, otherStart, otherEnd] =
      side === 'a' ? [0, 1, 2, 3] : [2, 3, 0, 1]
    for (const l of links) {
      if (l[otherStart] >= l[otherEnd]) continue
      for (let s = l[start]; s < l[end] && s < n; s++) {
        out[s] = [l[otherStart], l[otherEnd]]
      }
    }
    return out
  }
  if (from === to) {
    return Array.from({ length: count(from) }, (_, s) => [s, s + 1])
  }
  const toHub =
    from === book.hub
      ? Array.from({ length: count(from) }, (_, s): [number, number] => [
          s,
          s + 1,
        ])
      : across(sentencesTo(book, from), count(from), 'b')
  if (to === book.hub) return toHub
  const fromHub = across(sentencesTo(book, to), count(book.hub), 'a')
  return toHub.map(([h0, h1]) => {
    const met = fromHub.slice(h0, h1).filter(([a, b]) => a < b)
    if (met.length === 0) return [0, 0]
    return [
      Math.min(...met.map(([a]) => a)),
      Math.max(...met.map(([, b]) => b)),
    ]
  })
}

/** `sentenceTexts`, each sentence with its marks. */
export function sentenceExcerpts(edition: Edition): Excerpt[] {
  return edition.blocks.flatMap((block) =>
    block.sentences.map((range) => excerpt(block, range)),
  )
}

/** `sentencesIn`, each sentence with its marks. */
export function excerptsIn(
  edition: Edition,
  [start, end]: [number, number],
): Excerpt[] {
  const out: Excerpt[] = []
  let at = 0
  for (const block of edition.blocks) {
    for (const range of block.sentences) {
      if (at >= start && at < end) out.push(excerpt(block, range))
      at++
    }
    if (at >= end) break
  }
  return out
}

/** The text of sentences `[start, end)` of `edition`, each as its block holds it. */
export function sentencesIn(
  edition: Edition,
  [start, end]: [number, number],
): string[] {
  const out: string[] = []
  let at = 0
  for (const block of edition.blocks) {
    for (const [from, to] of block.sentences) {
      if (at >= start && at < end) out.push(block.text.slice(from, to))
      at++
    }
    if (at >= end) break
  }
  return out
}

/**
 * The sentences of `to` that show what `from`'s sentence `sentence` shows, as
 * `[start, end)`, empty where `to` has nothing for it: `span`, a sentence at a
 * time.
 */
export function sentenceSpan(
  book: Book,
  from: string,
  to: string,
  sentence: number,
): [number, number] {
  if (from === to) return [sentence, sentence + 1]
  const one: [number, number] = [sentence, sentence + 1]
  const inHub =
    from === book.hub ? one : over(sentencesTo(book, from), one, 'b')
  return to === book.hub ? inHub : over(sentencesTo(book, to), inHub, 'a')
}

/** The other side of the links whose side `side` meets `[start, end)`, as one range. */
function over(
  links: Link[],
  [start, end]: [number, number],
  side: 'a' | 'b',
): [number, number] {
  const [from, to, otherFrom, otherTo] =
    side === 'a' ? [0, 1, 2, 3] : [2, 3, 0, 1]
  const met = links.filter(
    (l) => l[from] < end && start < l[to] && l[otherFrom] < l[otherTo],
  )
  if (met.length === 0) return [0, 0]
  return [
    Math.min(...met.map((l) => l[otherFrom])),
    Math.max(...met.map((l) => l[otherTo])),
  ]
}

/**
 * One row of a comparison: blocks `[l0, l1)` of the left edition beside
 * blocks `[r0, r1)` of the right, as `[l0, l1, r0, r1]`. One side is empty
 * where the other's blocks have no counterpart.
 */
export type Row = [number, number, number, number]

/**
 * `left` and `right` row by row, each row the least run of each that shows
 * the same text, from start to end of both. Read off each edition's links to
 * the hub, a row closing wherever the two reach the same hub block.
 */
export function rows(book: Book, left: string, right: string): Row[] {
  return columns(book, [left, right]).map(([[l0, l1], [r0, r1]]) => [
    l0,
    l1,
    r0,
    r1,
  ])
}

/**
 * `editions` row by row, as `rows` does two: each row a `[start, end)` of
 * each edition's blocks in the order given, the least run of each that shows
 * the same text, closing wherever every one reaches the same hub block. Each
 * step takes the link that ends soonest in the hub, the earliest edition's on
 * a tie.
 */
export function columns(book: Book, editions: string[]): [number, number][][] {
  const links = editions.map((e) => hubLinks(book, e))
  const next = editions.map(() => 0)
  const hub = editions.map(() => 0)
  const start = editions.map(() => 0)
  const end = editions.map(() => 0)
  const out: [number, number][][] = []
  for (;;) {
    let k = -1
    for (const [e, own] of links.entries()) {
      if (next[e] >= own.length) continue
      if (k < 0 || own[next[e]][1] < links[k][next[k]][1]) k = e
    }
    if (k < 0) return out
    const link = links[k][next[k]++]
    hub[k] = link[1]
    end[k] = link[3]
    if (hub.every((h) => h === hub[0]) && end.some((x, e) => x > start[e])) {
      out.push(editions.map((_, e) => [start[e], end[e]]))
      for (const e of editions.keys()) start[e] = end[e]
    }
  }
}

/** `edition`'s links to the hub, the hub's own being each block to itself. */
function hubLinks(book: Book, edition: string): Link[] {
  if (edition !== book.hub) return linksTo(book, edition)
  return book.editions[edition].blocks.map((_, i) => [i, i + 1, i, i + 1])
}

function linksTo(book: Book, edition: string): Link[] {
  return book.alignments.find((a) => a.to === edition)?.links ?? []
}

function sentencesTo(book: Book, edition: string): Link[] {
  return book.alignments.find((a) => a.to === edition)?.sentences ?? []
}

/** Where `block`, on side `side` of `links`, lands on the other side. */
function across(links: Link[], block: number, side: 'a' | 'b'): number {
  const [start, end, otherStart, otherEnd] =
    side === 'a' ? [0, 1, 2, 3] : [2, 3, 0, 1]
  const link = links.find((l) => block >= l[start] && block < l[end])
  const last = Math.max(0, ...links.map((l) => l[otherEnd])) - 1
  return link ? Math.max(0, Math.min(link[otherStart], last)) : 0
}

/**
 * One entry of a book's contents: a run of headings in a row, such as a
 * chapter's number and its name, at the run's first block.
 */
export interface Entry {
  block: number
  lines: string[]
}

export function contents(blocks: Block[]): Entry[] {
  const entries: Entry[] = []
  blocks.forEach((block, i) => {
    if (!block.heading) return
    const last = entries.at(-1)
    if (last && last.block + last.lines.length === i)
      last.lines.push(block.text)
    else entries.push({ block: i, lines: [block.text] })
  })
  return entries
}

/** The index of the entry `block` is in, the last at or before it, or -1. */
export function entryAt(entries: Entry[], block: number): number {
  return entries.findLastIndex((entry) => entry.block <= block)
}

/**
 * Where each block starts, in characters, and the book's length last. How far
 * through is measured in characters, since a block runs from one line to a
 * page.
 */
export function starts(blocks: Block[]): number[] {
  const at = [0]
  for (const block of blocks) at.push(at[at.length - 1] + block.text.length)
  return at
}

/** How far through the text from block `from` up to block `to` block `block` starts, from 0 to 1. */
export function through(
  at: number[],
  block: number,
  from = 0,
  to = at.length - 1,
): number {
  const length = at[to] - at[from]
  return length > 0 ? (at[block] - at[from]) / length : 0
}

/**
 * What `PUT /api/works/{work}/book/pins` asks: a pin, or with `block` null,
 * its removal, `hub_block` counted in the blocks of `hub`, the hub the book
 * was sent with, which the server refuses where it is not the work's own.
 */
export interface PinRequest {
  edition: string
  hub: string
  hub_block: number
  block: number | null
}

export function putPin(work: string, pin: PinRequest): Promise<unknown> {
  return sendJson(
    `/api/works/${encodeURIComponent(work)}/book/pins`,
    'PUT',
    pin,
  )
}

/** `edition`'s pins, `[hubBlock, block]`, none for the hub. */
function pinsOf(book: Book, edition: string): [number, number][] {
  return book.alignments.find((a) => a.to === edition)?.pins ?? []
}

/**
 * The request that pins `left`'s block `leftBlock` to `right`'s `rightBlock`,
 * or with `rightBlock` null unpins it, one of the two being the hub; none
 * between two other editions, or to unpin a block no pin starts.
 */
export function pinRequest(
  book: Book,
  left: string,
  right: string,
  leftBlock: number,
  rightBlock: number | null,
): PinRequest | undefined {
  const hub = book.hub
  if (left === hub) {
    return { edition: right, hub, hub_block: leftBlock, block: rightBlock }
  }
  if (right !== hub) return undefined
  if (rightBlock !== null) {
    return { edition: left, hub, hub_block: rightBlock, block: leftBlock }
  }
  const pin = pinsOf(book, left).find(([, block]) => block === leftBlock)
  return pin && { edition: left, hub, hub_block: pin[0], block: null }
}

/** `left`'s blocks that start a link pinned between it and `right`. */
export function pinnedBlocks(
  book: Book,
  left: string,
  right: string,
): Set<number> {
  if (left === book.hub) return new Set(pinsOf(book, right).map(([hub]) => hub))
  if (right === book.hub) {
    return new Set(pinsOf(book, left).map(([, block]) => block))
  }
  return new Set()
}

/** The block in which the character a `fraction` of the way through `at`'s text falls. */
export function blockAt(at: number[], fraction: number): number {
  const blocks = at.length - 1
  if (blocks <= 0) return 0
  const char = fraction * at[blocks]
  let lo = 0
  let hi = blocks - 1
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1
    if (at[mid] <= char) lo = mid
    else hi = mid - 1
  }
  return lo
}

/**
 * The book reader's own keys, listed by its help as the page reader's are.
 * Escape is not one: `BookReader`'s key handler gives it an order of its own.
 */
export const BOOK_KEYS: ReaderBinding[] = [
  {
    keys: ['ArrowRight', 'PageDown'],
    label: 'the next page, or a screen down',
    resolve: () => ({ kind: 'step', by: 1 }),
  },
  {
    keys: ['ArrowLeft', 'PageUp'],
    label: 'the page before, or a screen up',
    resolve: () => ({ kind: 'step', by: -1 }),
  },
  {
    keys: [' '],
    label: 'switch to the next edition, at the same place',
    resolve: () => ({ kind: 'toggle' }),
  },
  {
    keys: ['1', '2', '3', '4', '5', '6', '7', '8', '9'],
    label: 'switch to the nth edition',
    resolve: (key, editions) => {
      const edition = editions[Number(key) - 1]
      return edition === undefined ? undefined : { kind: 'view', edition }
    },
  },
  {
    keys: ['A'],
    label: 'account: password, API keys, and what is signed in',
    resolve: () => ({ kind: 'account' }),
  },
  {
    keys: ['L'],
    label: 'back to the library',
    resolve: () => ({ kind: 'library' }),
  },
]
