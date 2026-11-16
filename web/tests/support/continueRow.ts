// Shared by the browser tests of the home page's Continue row. Each file
// keeps its own `vi.mock` of `src/library` and answers its own reads with
// these cards.
import type { SeriesCard, WorkCard } from '../../src/library'

/**
 * A series tile for the grid. No cover: a thumb that failed to load would
 * set `Cover`'s state, a commit that is not the row's.
 */
export const seriesCard = (id: number): SeriesCard => ({
  id,
  title: `Series ${id}`,
  works: 3,
  editions: 3,
  aligned: 0,
  added: id,
  unread: 3,
  cover: null,
})

/** `count` volumes in progress, for the Continue row; no covers, as above. */
export const inProgress = (count: number): WorkCard[] =>
  Array.from({ length: count }, (_, i) => {
    const id = i + 1
    const slug = `kagami-v${String(id).padStart(2, '0')}`
    return {
      id: 100 + id,
      slug,
      title: `Kagami no Kuni no Harisugawa v${String(id).padStart(2, '0')}`,
      volume: id,
      aligned: false,
      pages: 180,
      editions: [
        {
          key: 'raw',
          label: 'Raw',
          members: 1,
          pages: 180,
          missing: 0,
          cover: null,
        },
      ],
      opens: 'raw',
    }
  })

/** The grid's first row of tiles as drawn: one box per column. */
export function gridColumns(host: HTMLElement): DOMRect[] {
  const tiles = [...host.querySelectorAll('main .grid > li')].map((tile) =>
    tile.getBoundingClientRect(),
  )
  return tiles.filter((tile) => tile.top === tiles[0]?.top)
}

/**
 * Where the Continue row's cards miss the grid's columns, as drawn: a card
 * not the width of the grid's first tile, or not starting where its column
 * does. A card past the grid's columns, in a row that overflows, is held to
 * the columns' step carried on: the step between the first two, or with one
 * column, the tile and the grid's gap. Empty where every card lies within
 * 0.5 px, so a test can poll it and a failure names the cards.
 */
export function offGrid(host: HTMLElement): string[] {
  const row = host.querySelector('.continue ul')
  const grid = host.querySelector('main .grid')
  const columns = gridColumns(host)
  if (row === null || grid === null || columns.length === 0) {
    return ['no Continue row or no grid drawn']
  }
  const cards = [...row.children].map((card) => card.getBoundingClientRect())
  if (cards.length === 0) return ['no cards in the Continue row']
  const width = columns[0].width
  const step =
    columns.length > 1
      ? columns[1].left - columns[0].left
      : width + Number.parseFloat(getComputedStyle(grid).columnGap)
  const off: string[] = []
  cards.forEach((card, i) => {
    const left =
      i < columns.length ? columns[i].left : columns[0].left + i * step
    if (Math.abs(card.width - width) > 0.5) {
      off.push(`card ${i + 1} is ${card.width} px wide, a tile ${width}`)
    }
    if (Math.abs(card.left - left) > 0.5) {
      off.push(`card ${i + 1} starts at ${card.left}, its column at ${left}`)
    }
  })
  return off
}
