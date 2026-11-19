/**
 * Where a look-up's panel sits at phone size, where it goes over the text:
 * against the edge of the screen away from the word it looks up.
 */

/** The edge of the screen a look-up's panel sits against at phone size. */
export type LookupSide = 'top' | 'bottom'

/** What a panel at the top of the reader stays under. */
const BARS_ABOVE = '.reader-top-bar, .book-columns, .book-pinning'
/** What a panel at the bottom of the reader stays over. */
const BARS_BELOW = '.reader-bottom-bar'
/** The clear space kept between the panel and the word it looks up. */
const WORD_GAP = 8

/**
 * The boxes of `word` the reader can see in `shown`. A word broken across two
 * pages has boxes on the page not shown too, which say nothing about where
 * the reader sees it.
 */
const drawnBoxes = (word: Range, shown: DOMRect) =>
  [...word.getClientRects()].filter(
    (box) =>
      box.width > 0 &&
      box.height > 0 &&
      box.bottom > shown.top &&
      box.top < shown.bottom &&
      box.right > shown.left &&
      box.left < shown.right,
  )

/** Takes a panel back to where every docked panel opens. */
export function unseatLookup(panel: HTMLElement) {
  delete panel.dataset.side
  for (const name of ['--lookup-above', '--lookup-below', '--lookup-room']) {
    panel.style.removeProperty(name)
  }
}

/**
 * Sets `panel` against the edge of the screen with more room beyond `word`,
 * or against `keep`, no taller than the room left there, and answers the side
 * taken. Written straight to the element, as the sight check measures the
 * panel in the commit that opens it. Each edge is read back from the panel, as
 * only the stylesheet can resolve the safe areas. With no word on the screen,
 * `keep` holds its side and any other panel goes back to where it opens. Where
 * `grown` says the answer lengthened the word, a kept side with no room left
 * gives way: covering the word would be worse than moving once.
 */
export function seatLookup(
  panel: HTMLElement,
  word: Range | undefined,
  keep?: LookupSide,
  grown = false,
): LookupSide | undefined {
  const reader = panel.closest('.book-reader')
  const shown = reader?.querySelector('.book-text')?.getBoundingClientRect()
  const boxes = word && shown ? drawnBoxes(word, shown) : []
  if (!reader || (boxes.length === 0 && !keep)) {
    unseatLookup(panel)
    return undefined
  }
  const screen = reader.getBoundingClientRect()
  const clear = (bars: string, by: (bar: DOMRect) => number) =>
    `${Math.max(
      0,
      ...[...reader.querySelectorAll(bars)].map((bar) =>
        by(bar.getBoundingClientRect()),
      ),
    )}px`
  panel.style.setProperty(
    '--lookup-above',
    clear(BARS_ABOVE, (bar) => bar.bottom - screen.top),
  )
  panel.style.setProperty(
    '--lookup-below',
    clear(BARS_BELOW, (bar) => screen.bottom - bar.top),
  )
  panel.dataset.side = 'top'
  const { top, left, right } = panel.getBoundingClientRect()
  panel.dataset.side = 'bottom'
  const { bottom } = panel.getBoundingClientRect()
  // Only what lies across from the panel can be covered by it. A word broken
  // over two columns can have a piece at each end of the screen, and where
  // the panel is narrower than the window one of them may run clear beside it.
  const under = boxes.filter((box) => box.left < right && left < box.right)
  const near = under.length > 0 ? under : boxes
  const above = Math.min(...near.map((box) => box.top)) - top
  const below = bottom - Math.max(...near.map((box) => box.bottom))
  let side: LookupSide = keep ?? (above > below ? 'top' : 'bottom')
  panel.dataset.side = side
  if (under.length === 0) {
    panel.style.removeProperty('--lookup-room')
    return side
  }
  // A word as tall as the screen leaves no room at all, and a panel that
  // cannot show its Close could not be got out of.
  const head = panel.querySelector('.reader-panel-head')?.clientHeight ?? 0
  const { paddingBottom, borderTopWidth, borderBottomWidth } =
    getComputedStyle(panel)
  const least =
    head +
    Number.parseFloat(paddingBottom) +
    Number.parseFloat(borderTopWidth) +
    Number.parseFloat(borderBottomWidth)
  const roomOn = (on: LookupSide) => (on === 'top' ? above : below) - WORD_GAP
  const other = side === 'top' ? 'bottom' : 'top'
  if (grown && roomOn(side) < least && roomOn(other) > roomOn(side)) {
    side = other
    panel.dataset.side = side
  }
  panel.style.setProperty('--lookup-room', `${Math.max(roomOn(side), least)}px`)
  return side
}
