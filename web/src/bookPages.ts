/**
 * Where a book's text is, measured off the laid-out page: the block at the
 * top, the page a block is on, and the turn of a page. Scrolled, the blocks
 * run down one column; paged (`data-paged`), they run on in screen-high
 * columns, a page each, scrolled sideways.
 */
import { type Entry, entryAt } from './book'

/** Whether `text` is laid out in pages, its columns running sideways. */
function paged(text: HTMLElement): boolean {
  return text.dataset.paged !== undefined
}

/**
 * The first block not yet scrolled past: paged, the first whose right edge is
 * past the page's left, and otherwise the first whose bottom is below its top.
 * Found by halving, since the blocks run in order. Compared, the column's
 * children are rows, each naming the first of its left edition's blocks.
 */
export function topBlock(text: HTMLElement | null): number {
  if (!text) return 0
  const box = text.getBoundingClientRect()
  const past = paged(text)
    ? (r: DOMRect) => r.right > box.left + 1
    : (r: DOMRect) => r.bottom > box.top + 1
  const blocks = text.children
  let lo = 0
  let hi = blocks.length - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (past(blocks[mid].getBoundingClientRect())) hi = mid
    else lo = mid + 1
  }
  return Number((blocks[lo] as HTMLElement | undefined)?.dataset.block ?? 0)
}

/**
 * A place in the text finer than a block: character `at` of block `block`'s
 * own words. A page often starts inside a paragraph, and a text laid out
 * again at another size or width keeps its reader on the page holding this
 * character rather than the one the paragraph starts on.
 */
export interface Anchor {
  block: number
  at: number
}

/** The first character not yet scrolled past, found as `topBlock` finds its block. */
export function topOf(text: HTMLElement | null): Anchor {
  const block = topBlock(text)
  const words = text && wordsOf(text, block)
  if (!text || !words) return { block, at: 0 }
  const box = text.getBoundingClientRect()
  const past = paged(text)
    ? (r: DOMRect) => r.right > box.left + 1
    : (r: DOMRect) => r.bottom > box.top + 1
  let lo = 0
  let hi = wordNodes(words).reduce((n, node) => n + node.length, 0) - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    const rect = charRect(words, mid)
    if (!rect) return { block, at: 0 }
    if (past(rect)) hi = mid
    else lo = mid + 1
  }
  return { block, at: Math.max(lo, 0) }
}

/** Whether `anchor`'s character is on the page, or the screen, now showing. */
export function shows(text: HTMLElement | null, anchor: Anchor): boolean {
  const words = text && wordsOf(text, anchor.block)
  const rect = words && charRect(words, anchor.at)
  if (!text || !rect) return false
  const box = text.getBoundingClientRect()
  // Reaching in by more than a pixel, as `topBlock` counts, since a scroll
  // lands on a whole pixel and a line can sit a fraction above it.
  return paged(text)
    ? rect.right > box.left + 1 && rect.left < box.right - 1
    : rect.bottom > box.top + 1 && rect.top < box.bottom - 1
}

/**
 * Where to keep the reader as the text is laid out again: `anchor` while its
 * character is still on the page, so sizing the text up and back down returns
 * to the page it began on rather than creeping back a page at a time, and the
 * top of the page once a turn or a scroll has taken it away.
 */
export function kept(text: HTMLElement | null, anchor: Anchor): Anchor {
  return shows(text, anchor) ? anchor : topOf(text)
}

/**
 * Bring block `block` to the top of `text`: paged, the page its first line is
 * on. With `at`, the page or the line its character `at` is on instead.
 */
export function show(text: HTMLElement | null, block: number, at = 0) {
  if (!text) return
  const words = at > 0 ? wordsOf(text, block) : undefined
  const rect = words && charRect(words, at)
  const box = text.getBoundingClientRect()
  if (paged(text)) {
    const page = rect
      ? Math.floor((rect.left - box.left + text.scrollLeft) / text.clientWidth)
      : pageOf(text, block, 'first')
    text.scrollTo({ left: page * text.clientWidth })
  } else if (rect) {
    text.scrollTo({ top: rect.top - box.top + text.scrollTop })
  } else {
    holding(text, block)?.scrollIntoView({ block: 'start' })
  }
}

/**
 * Block `block`'s own words, where `text` draws that block on its own. A
 * compared row holds several blocks and is placed by its block alone.
 */
function wordsOf(text: HTMLElement, block: number): HTMLElement | undefined {
  const held = holding(text, block) as HTMLElement | undefined
  if (held?.dataset.block !== String(block)) return undefined
  return held.querySelector<HTMLElement>(':scope > [data-text]') ?? undefined
}

/**
 * The text nodes of a block's own words, in order: everything in its
 * `[data-text]` but what is set among them and is not theirs, a peek, marked
 * `data-aside`. Every count of a block's characters goes through this, so a
 * note's run and the reader's place mean the same with a peek open or not.
 */
export function wordNodes(words: Element): Text[] {
  const walker = document.createTreeWalker(words, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) =>
      node.parentElement?.closest('[data-aside]')
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT,
  })
  const out: Text[] = []
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    out.push(node as Text)
  }
  return out
}

/**
 * The box of character `at` of `words`, across the marks a highlight splits
 * it into; none where the browser cannot measure a range, as jsdom cannot.
 */
function charRect(words: HTMLElement, at: number): DOMRect | undefined {
  let left = at
  for (const node of wordNodes(words)) {
    const length = node.length
    if (left < length) {
      const range = document.createRange()
      range.setStart(node, left)
      range.setEnd(node, left + 1)
      if (typeof range.getClientRects !== 'function') return undefined
      return range.getClientRects()[0] ?? range.getBoundingClientRect()
    }
    left -= length
  }
  return undefined
}

/**
 * The page, from 0, that block `block`'s first or last line is on: a page is
 * the text's own width, since its column gap is its two side paddings.
 */
function pageOf(text: HTMLElement, block: number, line: 'first' | 'last') {
  const held = holding(text, block)
  if (!held || text.clientWidth <= 0) return 0
  const lines = held.getClientRects()
  const rect =
    (line === 'first' ? lines[0] : lines[lines.length - 1]) ??
    held.getBoundingClientRect()
  const edge = line === 'first' ? rect.left : rect.right - 1
  const x = edge - text.getBoundingClientRect().left + text.scrollLeft
  return Math.floor(x / text.clientWidth)
}

/** Where a paged reader is, counted in pages from 1: in the book, and in its chapter. */
export interface Pages {
  at: number
  of: number
  chapter?: { at: number; of: number }
}

/**
 * `text`'s pages, with block `top` at the top, its chapter from its entry's
 * first line to the last line before the next; none when it is not paged.
 */
export function pagesOf(
  text: HTMLElement | null,
  entries: Entry[],
  top: number,
): Pages | undefined {
  if (!text || !paged(text) || text.clientWidth <= 0) return undefined
  const at = Math.round(text.scrollLeft / text.clientWidth)
  const of = Math.max(1, Math.round(text.scrollWidth / text.clientWidth))
  const reading = entryAt(entries, top)
  const entry = entries[reading]
  if (!entry) return { at: at + 1, of }
  const next = entries[reading + 1]
  const first = pageOf(text, entry.block, 'first')
  const last = next ? pageOf(text, next.block - 1, 'last') : of - 1
  return {
    at: at + 1,
    of,
    chapter: { at: at - first + 1, of: last - first + 1 },
  }
}

/** Turn `by` pages, or scroll `by` screens less a line's worth, so one carries over. */
export function turn(text: HTMLElement | null, by: number) {
  if (!text) return
  if (paged(text)) {
    const page = Math.round(text.scrollLeft / text.clientWidth) + by
    text.scrollTo({ left: page * text.clientWidth })
  } else {
    text.scrollBy({ top: by * text.clientHeight * 0.9 })
  }
}

/** The pages a tap at `x` turns: back on the left third, on on the right, none between or unpaged. */
export function pageTurnedBy(text: HTMLElement | null, x: number): number {
  if (!text || !paged(text)) return 0
  const box = text.getBoundingClientRect()
  const at = (x - box.left) / box.width
  return at < 1 / 3 ? -1 : at > 2 / 3 ? 1 : 0
}

/** The child of `text` that holds block `block`: the last that starts at or before it. */
function holding(text: HTMLElement | null, block: number): Element | undefined {
  return [...(text?.children ?? [])].findLast(
    (child) => Number((child as HTMLElement).dataset.block) <= block,
  )
}
