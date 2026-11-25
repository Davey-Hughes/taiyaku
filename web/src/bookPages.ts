/**
 * Where a book's text is, measured off the laid-out page: the block at the
 * top, the page a block is on, and the turn of a page. Scrolled, the blocks
 * run down one column; paged (`data-paged`), they run on in screen-high
 * columns, a page each, scrolled sideways. Vertical (`data-vertical`), the
 * blocks run leftward from the right edge, and paged, the pages run down.
 */
import { type Entry, entryAt } from './book'

/** How `text` is laid out: set vertically or not, and paged or scrolled. */
export interface Flow {
  vertical: boolean
  paged: boolean
}

export function flowOf(text: HTMLElement): Flow {
  return {
    vertical: text.dataset.vertical !== undefined,
    paged: text.dataset.paged !== undefined,
  }
}

/**
 * Whether the reading axis is sideways, leftward or rightward: scrolled
 * vertical text runs on leftward, and horizontal pages are turned sideways.
 * Vertical pages run down, as a scrolled horizontal text does.
 */
const sideways = (flow: Flow) => flow.vertical !== flow.paged

/**
 * Where `rect` starts and ends along the reading axis, in pixels that grow as
 * reading goes on. Vertical scrolled text reads leftward, so its edges are
 * turned about: a box's right edge is its start.
 */
function blockStart(flow: Flow, rect: DOMRect): number {
  if (!sideways(flow)) return rect.top
  return flow.vertical ? -rect.right : rect.left
}

function blockEnd(flow: Flow, rect: DOMRect): number {
  if (!sideways(flow)) return rect.bottom
  return flow.vertical ? -rect.left : rect.right
}

/**
 * How far `text` is scrolled along its reading axis, never below 0: vertical
 * scrolled text's `scrollLeft` runs from 0 negative.
 */
function progress(text: HTMLElement, flow = flowOf(text)): number {
  if (!sideways(flow)) return text.scrollTop
  return flow.vertical ? Math.max(0, -text.scrollLeft) : text.scrollLeft
}

function setProgress(text: HTMLElement, to: number, flow = flowOf(text)) {
  if (!sideways(flow)) text.scrollTo({ top: to })
  else text.scrollTo({ left: flow.vertical ? -to : to })
}

/** Scroll `text` on `by` pixels along its reading axis, or back where `by` is negative. */
export function addProgress(
  text: HTMLElement,
  by: number,
  flow = flowOf(text),
) {
  if (!sideways(flow)) text.scrollBy({ top: by })
  else text.scrollBy({ left: flow.vertical ? -by : by })
}

/** A screen of `text` along its reading axis: paged, a page. */
function pageSize(text: HTMLElement, flow = flowOf(text)): number {
  return sideways(flow) ? text.clientWidth : text.clientHeight
}

/** All of `text` along its reading axis, the length its pages divide. */
function extent(text: HTMLElement, flow = flowOf(text)): number {
  return sideways(flow) ? text.scrollWidth : text.scrollHeight
}

/** Whether `rect` reaches past the start of `text`'s screen, by more than a pixel. */
function pastStart(text: HTMLElement, flow: Flow) {
  const start = blockStart(flow, text.getBoundingClientRect())
  return (rect: DOMRect) => blockEnd(flow, rect) > start + 1
}

/**
 * The child of `text` the screen starts in: the first whose end along the
 * reading axis is past the screen's start, as paged its right edge is past
 * the page's left. Found by halving, since the children run in order.
 */
function topChild(text: HTMLElement): HTMLElement | undefined {
  const past = pastStart(text, flowOf(text))
  const children = text.children
  let lo = 0
  let hi = children.length - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    if (past(children[mid].getBoundingClientRect())) hi = mid
    else lo = mid + 1
  }
  return children[lo] as HTMLElement | undefined
}

/**
 * The first block not yet scrolled past. Compared, the column's children are
 * rows, each naming the first of its left edition's blocks. A figure between
 * blocks names none and is passed over: one at the top of the screen leaves
 * the block after it the top, which `anchorOf` names the figure from.
 */
export function topBlock(text: HTMLElement | null): number {
  if (!text) return 0
  // Stepped from the child found rather than searched among blocks alone,
  // which would take gathering them all on every scroll. Back, where the
  // figures that end the book are all that is left on screen.
  const child = topChild(text)
  const found = child && (nearBlock(child, 'next') ?? nearBlock(child, 'back'))
  return Number(found?.dataset.block ?? 0)
}

/**
 * The block to save for the page showing, the one a book opened there comes
 * back to this page by. A book opens on the page a block's first line is on,
 * compared its first words in the column read, so paged that is the first
 * block to open on this page: `topBlock` can be one carried over from a page
 * before, and saved it would open a page back. A page no block opens on is
 * named by the block carried onto it, which compared can be inside a row, past
 * the first block the row is named from.
 */
export function resumeBlock(text: HTMLElement | null): number {
  const top = topBlock(text)
  const first = text && topChild(text)
  if (!text || !first || !flowOf(text).paged) return top
  const flow = flowOf(text)
  const size = pageSize(text, flow)
  const page = Math.round(progress(text, flow) / size)
  const carried = keepsRead(text) ? readOnPage(text, flow, first)?.block : top
  // A page of a row holding only a figure or the other column's words is named
  // by the last block of the column read to open before it in that row: the
  // row's first can lie many pages further back. Where none has yet, it is the
  // block the row is named from, since a block of an earlier row would reopen
  // before this row begins.
  let opened = top
  for (
    let child: Element | null = first;
    child;
    child = child.nextElementSibling
  ) {
    for (const { block, rect } of openingsIn(text, child)) {
      const on = Math.floor(along(text, flow, blockStart(flow, rect)) / size)
      if (on === page) return block
      if (on > page) return carried ?? opened
      opened = block
    }
  }
  return carried ?? opened
}

/** The blocks that `child` of `text` opens, each with the box a book opened at it is placed by. */
function openingsIn(
  text: HTMLElement,
  child: Element,
): { block: number; rect: DOMRect }[] {
  if (keepsRead(text)) {
    const column = child.querySelectorAll<HTMLElement>(`[data-pick^="${READ}"]`)
    return [...column].flatMap((words) => {
      const rect = charRect(words, 0)
      const block = Number(words.dataset.pick?.slice(READ.length))
      return rect ? [{ block, rect }] : []
    })
  }
  if (!(child instanceof HTMLElement) || child.dataset.block === undefined) {
    return []
  }
  const rect = child.getClientRects()[0] ?? child.getBoundingClientRect()
  return [{ block: Number(child.dataset.block), rect }]
}

/** `child` where it is a block, or the nearest block on from it, or back. */
function nearBlock(child: HTMLElement, way: 'next' | 'back') {
  let at: Element | null = child
  while (at instanceof HTMLElement && at.dataset.block === undefined) {
    at = way === 'next' ? at.nextElementSibling : at.previousElementSibling
  }
  return at instanceof HTMLElement ? at : undefined
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
  /**
   * Where the screen starts in a figure rather than in words: the figure that
   * many children of the text on from block `block`'s, or back where it is
   * negative, as the figures set before a block are. Placed by its block
   * alone, the reader would be taken past the figure to the words after it.
   */
  figure?: number
  /**
   * Scrolled, how far into its character, figure or compared row the screen
   * starts, or paged, how far into its compared row, a row being the one box
   * that runs on over pages: a fraction of that box's length along the reading
   * axis. Placed again, that point goes back to the screen's start, so a line
   * or a figure part scrolled past stays as it was rather than snapping to its
   * start. It means nothing against a box of another kind or size, so only a
   * docked panel's set-in and the re-place after an image loads carry it: the
   * rest place the anchor `whole`.
   */
  into?: number
  /**
   * Compared and paged: the character of the column read to keep on the page,
   * named in that column's own block, since `into` counts both columns and at
   * another width can name a page no longer holding what was read. The page's
   * first such character after a turn or a scroll, and after a placing the one
   * placed while it is on the page (`readPlaced`). None where the page holds
   * only another column's words, or at a search match. What carries `into`
   * carries this.
   */
  read?: { block: number; at: number }
}

/** The first character not yet scrolled past, found as `topBlock` finds its block. */
export function topOf(text: HTMLElement | null): Anchor {
  const block = topBlock(text)
  const words = text && wordsOf(text, block)
  if (!text || !words) return { block, at: 0 }
  return { block, at: firstPast(words, pastStart(text, flowOf(text))) ?? 0 }
}

/**
 * The first character of `words` that `past` holds for, found by halving, or
 * their last where it holds for none; none where a range cannot be measured.
 */
function firstPast(
  words: HTMLElement,
  past: (rect: DOMRect) => boolean,
): number | undefined {
  let lo = 0
  let hi = wordNodes(words).reduce((n, node) => n + node.length, 0) - 1
  while (lo < hi) {
    const mid = (lo + hi) >> 1
    const rect = charRect(words, mid)
    if (!rect) return undefined
    if (past(rect)) hi = mid
    else lo = mid + 1
  }
  return lo
}

/**
 * The reader's place as the screen now shows it: `topOf`'s character, or the
 * figure the screen starts in, which `topOf` passes over, and scrolled, how
 * far into either the screen starts. Paged, a page is placed whole, so that is
 * left out, except compared: a row's block alone would name the page the row
 * starts on, so the anchor holds how far into the row the page is and the
 * character read there (`Anchor.read`).
 */
export function anchorOf(text: HTMLElement | null): Anchor {
  const child = text && topChild(text)
  const anchor =
    (child?.dataset.figure !== undefined && figureAnchor(child)) || topOf(text)
  const flow = text && flowOf(text)
  if (!text || !flow) return anchor
  if (flow.paged && text.dataset.compared === undefined) return anchor
  const rect = boxOf(text, anchor)
  const length = rect ? blockEnd(flow, rect) - blockStart(flow, rect) : 0
  if (!rect || length <= 0) return anchor
  const start = blockStart(flow, text.getBoundingClientRect())
  const into = (start - blockStart(flow, rect)) / length
  if (!flow.paged) return { ...anchor, into }
  const read = child ? readOnPage(text, flow, child) : undefined
  // A row that starts on this page starts below its padding, short of the
  // screen's start: not into it at all, or narrowed it would reach back a page.
  return { ...anchor, into: Math.max(0, into), ...(read && { read }) }
}

/**
 * Whether `text` is placed by the character read: compared and paged, where a
 * row's start can lie pages before the words being read.
 */
export function keepsRead(text: HTMLElement | null): boolean {
  return text?.dataset.compared !== undefined && flowOf(text).paged
}

/**
 * The character of the column read to keep after a placing at `place`: the
 * one placed while it is on the page, as a block's first words can be part
 * way down it, or else the page's first; none where `keepsRead` does not hold.
 */
export function readPlaced(
  text: HTMLElement | null,
  place: { block: number; at: number },
): Anchor['read'] {
  if (!text || !keepsRead(text)) return undefined
  if (matchShows(text, { block: place.block, start: place.at })) return place
  const child = topChild(text)
  return child && readOnPage(text, flowOf(text), child)
}

/**
 * The first character of the column read on the page showing, from row
 * `first` on; none where this page holds only other columns' words.
 */
function readOnPage(
  text: HTMLElement,
  flow: Flow,
  first: Element,
): Anchor['read'] {
  const past = pastStart(text, flow)
  const end = blockEnd(flow, text.getBoundingClientRect())
  for (let row: Element | null = first; row; row = row.nextElementSibling) {
    const column = row.querySelectorAll<HTMLElement>(`[data-pick^="${READ}"]`)
    for (const words of column) {
      const at = firstPast(words, past) ?? 0
      const rect = charRect(words, at)
      if (!rect || !past(rect)) continue
      if (blockStart(flow, rect) >= end - 1) return undefined
      return { block: Number(words.dataset.pick?.slice(READ.length)), at }
    }
  }
  return undefined
}

/**
 * `anchor` placed from its start: what the text goes back to once it is laid
 * out at another size or width, or in another layout, where `into` would be
 * a fraction of a box that is no longer the one it was measured against.
 */
export function whole(anchor: Anchor): Anchor {
  if (anchor.into === undefined && anchor.read === undefined) return anchor
  const { into: _, read: __, ...placed } = anchor
  return placed
}

/** `figure`, a child of the text, named from the block after it, or before it at the book's end. */
function figureAnchor(figure: HTMLElement): Anchor | undefined {
  const block = nearBlock(figure, 'next') ?? nearBlock(figure, 'back')
  const children = [...(figure.parentElement?.children ?? [])]
  if (!block) return undefined
  return {
    block: Number(block.dataset.block),
    at: 0,
    figure: children.indexOf(figure) - children.indexOf(block),
  }
}

/** The figure `anchor` names, where it is still drawn beside its block. */
function figureOf(text: HTMLElement, anchor: Anchor): HTMLElement | undefined {
  const held = holding(text, anchor.block) as HTMLElement | undefined
  const own = held?.dataset.block === String(anchor.block)
  if (anchor.figure === undefined || !held || !own) return undefined
  const children = [...text.children]
  const figure = children[children.indexOf(held) + anchor.figure] as
    | HTMLElement
    | undefined
  return figure?.dataset.figure !== undefined ? figure : undefined
}

/**
 * The box `anchor` is placed by: its figure's, its character's, or where its
 * block's words are not drawn on their own, as in a compared row, the box of
 * the child holding it.
 */
function boxOf(text: HTMLElement, anchor: Anchor): DOMRect | undefined {
  if (anchor.figure !== undefined) {
    return figureOf(text, anchor)?.getBoundingClientRect()
  }
  const words = wordsOf(text, anchor.block)
  if (words) return charRect(words, anchor.at)
  return holding(text, anchor.block)?.getBoundingClientRect()
}

/** Whether `anchor`'s character or figure is on the page, or the screen, now showing. */
export function shows(text: HTMLElement | null, anchor: Anchor): boolean {
  return onScreen(text, anchor) === true
}

/**
 * Whether `anchor`'s character or figure is measured to be off the page or the
 * screen: not where it cannot be measured, as in a compared row.
 */
export function offScreen(text: HTMLElement | null, anchor: Anchor): boolean {
  return onScreen(text, anchor) === false
}

function onScreen(
  text: HTMLElement | null,
  anchor: Anchor,
): boolean | undefined {
  if (!text) return undefined
  const words = wordsOf(text, anchor.block)
  const rect =
    anchor.figure !== undefined
      ? figureOf(text, anchor)?.getBoundingClientRect()
      : words && charRect(words, anchor.at)
  if (!rect) return undefined
  return reachesScreen(text, rect)
}

/**
 * Whether a match's first character is on the page, or the screen, now
 * showing. Compared, a block's words are not drawn on their own, so it is
 * looked for in the column read, the one a search's matches are lit in.
 */
export function matchShows(
  text: HTMLElement | null,
  { block, start }: { block: number; start: number },
): boolean {
  if (!text) return false
  const words = wordsOf(text, block)
  const rect = words
    ? charRect(words, start)
    : readRect(text, { block, at: start })
  return rect !== undefined && reachesScreen(text, rect)
}

/**
 * The character the reader is at, for a step through a search's matches:
 * `anchor`'s, except compared. There it is the anchor's `read` while that is
 * on the screen, since a panel's narrower page can start on words already read.
 * Failing that, a compared anchor names a row's start, which in a row longer
 * than a screen lies behind the reader, so once that is off the screen too it
 * is the first character of the column read on the screen. Where the screen
 * holds none of that column's words, `anchor` is all there is.
 */
export function readingAt(
  text: HTMLElement | null,
  anchor: Anchor,
): { block: number; at: number } {
  if (text?.dataset.compared === undefined) return anchor
  const { read } = anchor
  if (read && matchShows(text, { block: read.block, start: read.at })) {
    return read
  }
  if (matchShows(text, { block: anchor.block, start: anchor.at })) return anchor
  const child = topChild(text)
  return (child && readOnPage(text, flowOf(text), child)) ?? anchor
}

/** Whether `rect` reaches onto the page, or the screen, now showing. */
function reachesScreen(text: HTMLElement, rect: DOMRect): boolean {
  const flow = flowOf(text)
  const box = text.getBoundingClientRect()
  // Reaching in by more than a pixel, as `topBlock` counts, since a scroll
  // lands on a whole pixel and a line can sit a fraction above it.
  return (
    blockEnd(flow, rect) > blockStart(flow, box) + 1 &&
    blockStart(flow, rect) < blockEnd(flow, box) - 1
  )
}

/**
 * Where to keep the reader as the text is laid out again: `anchor` while its
 * character is still on the page, so sizing the text up and back down returns
 * to the page it began on rather than creeping back a page at a time, and the
 * top of the page once a turn or a scroll has taken it away. Placed `whole`,
 * since what follows lays the text out afresh.
 */
export function kept(text: HTMLElement | null, anchor: Anchor): Anchor {
  return whole(shows(text, anchor) ? anchor : anchorOf(text))
}

/**
 * Bring block `block` to the top of `text`: paged, the page its first line is
 * on. With `at`, the page or the line its character `at` is on instead;
 * with an anchor's `figure`, the page or the place that figure is at; with
 * its `into`, that far into its box; and with its `read`, while that character
 * is drawn, the page holding it, whatever the fraction would name.
 */
export function show(
  text: HTMLElement | null,
  block: number,
  at?: number,
  { figure, into, read }: Pick<Anchor, 'figure' | 'into' | 'read'> = {},
) {
  if (!text) return
  const flow = flowOf(text)
  const character = read && readRect(text, read)
  const fraction = character ? undefined : into
  const rect =
    character ??
    (figure !== undefined || into !== undefined
      ? boxOf(text, { block, at: at ?? 0, figure })
      : charAt(text, block, at))
  const from = rect && along(text, flow, blockStart(flow, rect))
  const past = rect
    ? (fraction ?? 0) * (blockEnd(flow, rect) - blockStart(flow, rect))
    : 0
  if (flow.paged) {
    const size = pageSize(text, flow)
    // A row's `into` comes back to the very start of the page it was taken
    // on, which rounding can leave a hair short of, on the page before.
    const page =
      from === undefined
        ? pageOf(text, block, 'first')
        : Math.floor((from + past + (fraction ? 0.5 : 0)) / size)
    setProgress(text, page * size, flow)
  } else if (rect && from !== undefined) {
    setProgress(text, from + past, flow)
  } else {
    holding(text, block)?.scrollIntoView({ block: 'start' })
  }
}

/**
 * How far a box may lie over an edge and still count as inside it: a scroll
 * lands on a whole pixel, and a line can sit a fraction past one.
 */
const HAIR = 0.5

/** A stretch of the reading axis, as `blockStart` and `blockEnd` measure it. */
type Span = [start: number, end: number]

/** `spans` less the stretch from `from` to `to`. */
const without = (spans: Span[], [from, to]: Span): Span[] =>
  spans
    .flatMap(([start, end]): Span[] => [
      [start, Math.min(end, from)],
      [Math.max(start, to), end],
    ])
    .filter(([start, end]) => end > start)

/**
 * Move `text` by the least that shows `word`. Scrolled, that is the shortest
 * scroll along the reading axis that leaves all of it in the text's box and
 * clear of `covers`, the boxes drawn over the text. Nothing moves where it is
 * in sight already, where it is all on the screen with no cover over any of
 * its boxes, or where no such scroll exists, since one that cannot clear it
 * would lose the reader's place for nothing: a cover as wide as vertical text
 * lies over the same lines however far the text is scrolled. Paged, it is the
 * page `word` starts on, whatever is drawn over that page.
 */
export function bringIntoSight(
  text: HTMLElement | null,
  word: Range,
  covers: DOMRect[],
) {
  // jsdom measures no range.
  if (!text || typeof word.getClientRects !== 'function') return
  const rects = [...word.getClientRects()]
  if (rects.length === 0) return
  const flow = flowOf(text)
  if (flow.paged) {
    // Its own page, where it is on already, is no move at all.
    const size = pageSize(text, flow)
    const page = Math.floor(
      along(text, flow, blockStart(flow, rects[0])) / size,
    )
    setProgress(text, page * size, flow)
    return
  }
  const box = text.getBoundingClientRect()
  const screen: Span = [blockStart(flow, box), blockEnd(flow, box)]
  const from = Math.min(...rects.map((rect) => blockStart(flow, rect)))
  const to = Math.max(...rects.map((rect) => blockEnd(flow, rect)))
  // Only what lies over `word` across the reading axis can hide it there.
  const over = covers.filter((cover) =>
    rects.some((rect) =>
      sideways(flow)
        ? cover.top < rect.bottom && rect.top < cover.bottom
        : cover.left < rect.right && rect.left < cover.right,
    ),
  )
  const free = over.reduce<Span[]>(
    (spans, cover) =>
      without(spans, [blockStart(flow, cover), blockEnd(flow, cover)]),
    [screen],
  )
  const inSight = free.some(
    ([start, end]) => from >= start - HAIR && to <= end + HAIR,
  )
  // A word broken over two lines can have a cover level with one piece and
  // across from the other while lying over neither.
  const uncovered = !rects.some((rect) =>
    covers.some(
      (cover) =>
        cover.left < rect.right &&
        rect.left < cover.right &&
        cover.top < rect.bottom &&
        rect.top < cover.bottom,
    ),
  )
  const onScreen = from >= screen[0] - HAIR && to <= screen[1] + HAIR
  if (inSight || (uncovered && onScreen)) return
  const back = progress(text, flow)
  const on = extent(text, flow) - pageSize(text, flow) - back
  const moves = free
    .filter(([start, end]) => end - start >= to - from)
    .map(([start, end]) =>
      from < start ? Math.floor(from - start) : Math.ceil(to - end),
    )
    .filter((move) => move >= -back - 1 && move <= on + 1)
  if (moves.length === 0) return
  addProgress(
    text,
    moves.reduce((a, b) => (Math.abs(b) < Math.abs(a) ? b : a)),
    flow,
  )
}

/** How far into all of `text` the screen position `edge` is, along its reading axis. */
function along(text: HTMLElement, flow: Flow, edge: number): number {
  return (
    edge - blockStart(flow, text.getBoundingClientRect()) + progress(text, flow)
  )
}

/**
 * The box of the character a place names, where it names one: in `block`'s
 * own words, or in the compared row holding it. With no `at`, or at the very
 * start of a block drawn on its own, the block is placed whole instead.
 */
function charAt(
  text: HTMLElement,
  block: number,
  at: number | undefined,
): DOMRect | undefined {
  if (at === undefined) return undefined
  const words = (at > 0 && wordsOf(text, block)) || rowWords(text, block, at)
  return words ? charRect(words, at) : undefined
}

/**
 * Block `block`'s words in the first column of the compared row holding it,
 * the column read, so a place inside a row taller than the screen is brought
 * onto it. Character 0 of the row's own block is the row's start: that row is
 * placed whole, with the figures it sets before those words.
 */
function rowWords(
  text: HTMLElement,
  block: number,
  at: number,
): HTMLElement | undefined {
  const row = holding(text, block) as HTMLElement | undefined
  if (text.dataset.compared === undefined || !row) return undefined
  if (at === 0 && row.dataset.block === String(block)) return undefined
  return readWords(row, block)
}

/** What `data-pick` starts with on the words of a compared row's first column, the column read. */
const READ = 'left:'

/** Block `block`'s words in the column read of compared row `row`. */
function readWords(
  row: Element | undefined,
  block: number,
): HTMLElement | undefined {
  return (
    row?.querySelector<HTMLElement>(`[data-pick="${READ}${block}"]`) ??
    undefined
  )
}

/** The box of an anchor's `read` character, where it is drawn. */
function readRect(
  text: HTMLElement,
  { block, at }: NonNullable<Anchor['read']>,
): DOMRect | undefined {
  const words = readWords(holding(text, block), block)
  return words && charRect(words, at)
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
      // The digits of an upright number share one column's width between
      // them, so each alone would leave half the column off the screen.
      const upright = node.parentElement?.closest('.book-upright')
      if (upright) return upright.getBoundingClientRect()
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
 * the text's own width, or vertical its own height, since its column gap is
 * its two paddings.
 */
function pageOf(text: HTMLElement, block: number, line: 'first' | 'last') {
  const held = holding(text, block)
  const flow = flowOf(text)
  const size = pageSize(text, flow)
  if (!held || size <= 0) return 0
  const lines = held.getClientRects()
  const rect =
    (line === 'first' ? lines[0] : lines[lines.length - 1]) ??
    held.getBoundingClientRect()
  const edge =
    line === 'first' ? blockStart(flow, rect) : blockEnd(flow, rect) - 1
  return Math.floor(along(text, flow, edge) / size)
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
  if (!text) return undefined
  const flow = flowOf(text)
  const size = pageSize(text, flow)
  if (!flow.paged || size <= 0) return undefined
  const at = Math.round(progress(text, flow) / size)
  const of = Math.max(1, Math.round(extent(text, flow) / size))
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
  const flow = flowOf(text)
  const size = pageSize(text, flow)
  if (flow.paged) {
    const page = Math.round(progress(text, flow) / size) + by
    setProgress(text, page * size, flow)
  } else {
    addProgress(text, by * size * 0.9, flow)
  }
}

/**
 * The pages a tap at `x` turns: back on the left third, on on the right, none
 * between or unpaged. Vertical the other way about, since its lines are read
 * right to left and a printed one's pages are turned leftward.
 */
export function pageTurnedBy(text: HTMLElement | null, x: number): number {
  if (!text) return 0
  const flow = flowOf(text)
  if (!flow.paged) return 0
  const box = text.getBoundingClientRect()
  const at = (x - box.left) / box.width
  const on = flow.vertical ? -1 : 1
  return at < 1 / 3 ? -on : at > 2 / 3 ? on : 0
}

/**
 * Whether `image`, newly loaded, can have moved `anchor` off its page: paged
 * only, since scrolled the browser's own scroll anchoring holds the line the
 * reader is on, and only an image before the row the reader is at or in it.
 * That row is the one holding `read` where the anchor has it, which can be
 * later than the anchor's own, and an image between the two moves what is read.
 */
export function movesAnchor(
  text: HTMLElement | null,
  anchor: Anchor,
  image: Element,
): boolean {
  if (!text || !flowOf(text).paged) return false
  const held = holding(text, anchor.read?.block ?? anchor.block)
  if (!held) return true
  return (
    held.contains(image) ||
    (held.compareDocumentPosition(image) & Node.DOCUMENT_POSITION_PRECEDING) !==
      0
  )
}

/**
 * The child of `text` that holds block `block`: the last that starts at or
 * before it. The children run in block order, blocks or compared rows with
 * figures between them that name no block, so the child is found by halving
 * rather than by reading every one: a placing and a page turn each ask for it
 * several times, and an edition runs to thousands of blocks.
 */
function holding(text: HTMLElement | null, block: number): Element | undefined {
  const children = text?.children
  if (!children) return undefined
  // The block named by the first child from `i` that names one, looked for
  // no further than `end`: the first child from `end` on that names a block
  // names one past `block`, so a run of figures reaching `end` is past it
  // too, and is not read again at every halving.
  const from = (i: number, end: number) => {
    for (let at = i; at < end; at++) {
      const named = Number((children[at] as HTMLElement).dataset.block)
      if (!Number.isNaN(named)) return named
    }
    return Number.POSITIVE_INFINITY
  }
  let low = 0
  let high = children.length
  while (low < high) {
    const middle = Math.floor((low + high) / 2)
    if (from(middle, high) <= block) low = middle + 1
    else high = middle
  }
  // The child before the first whose named block is past `block` names one
  // itself: were it a figure, the block it is followed by would be past too.
  return low > 0 ? children[low - 1] : undefined
}
