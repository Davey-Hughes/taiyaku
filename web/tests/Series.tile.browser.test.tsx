import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { cdp, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import type { Me } from '../src/api'
import * as library from '../src/library'
import { settleUntil } from './support/async'
import { mount } from './support/mount'
import { areaOf, overlapOf, pointerRulesAs } from './support/pointer'
import { seriesPage } from './support/seriesPage'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))

const edition = (missing = 0): library.EditionCard => ({
  key: 'raw',
  label: 'Raw',
  members: 1,
  pages: 180,
  missing,
  cover: null,
})

const work = (
  id: number,
  over: Partial<library.WorkCard> = {},
): library.WorkCard => ({
  id,
  slug: `kagami-v0${id}`,
  title: `Kagami no Kuni no Harisugawa v0${id}`,
  volume: id,
  aligned: false,
  pages: 180,
  editions: [edition()],
  opens: 'raw',
  ...over,
})

/**
 * The first volume aligned, so its cover carries ⇄; the second unreadable,
 * its files gone, so it is drawn without a link (Review Focus 5).
 */
const WORKS = [
  work(1, { aligned: true }),
  work(2, { editions: [edition(3)] }),
  work(3),
  work(4),
]

/** A reader who may download and may not edit, so a panel holds no form. */
const READER: Me = {
  user_id: 1,
  admin: false,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(library.oneSeries).mockResolvedValue({
    series: {
      id: 7,
      title: 'Kagami no Kuni no Harisugawa',
      works: WORKS.length,
      editions: WORKS.length,
      aligned: 1,
      added: 1,
      unread: 4,
      cover: null,
      titles: [],
    },
    works: WORKS,
  })
  vi.mocked(library.work).mockImplementation(async (slug) => ({
    ...(WORKS.find((w) => w.slug === slug) as library.WorkCard),
    alignments: [],
  }))
})

/** A real input, the React updates it causes drawn inside `act`. */
const real = (input: () => Promise<unknown>) =>
  act(async () => {
    await input()
  })

/**
 * The page drawn, its four tiles in, the pointer resting on its heading.
 * `seriesPage` draws it with the pointer off every tile.
 */
async function drawn(options: {
  width: number
  panel: 'off' | 'button' | 'on-select'
  coverSize?: number
}) {
  const host = await seriesPage({ ...options, who: READER })
  const all = () => [...host.querySelectorAll<HTMLElement>('.volumes > li')]
  await settleUntil('the volumes', () => all().length === WORKS.length, 1000)
  await document.fonts.ready
  const heading = host.querySelector('h1') as HTMLElement
  await real(() => userEvent.hover(heading))
  return { host, tiles: all(), heading }
}

const rect = (el: Element | null) => {
  if (el === null) throw new Error('nothing drawn')
  return el.getBoundingClientRect()
}

const coverOf = (tile: Element) => rect(tile.querySelector('.cover-box'))

const iOf = (tile: Element) => {
  const i = tile.querySelector<HTMLButtonElement>(':scope > .panel-open')
  if (i === null) throw new Error('no i on this tile')
  return i
}

/** `el`'s style once its transitions have run to their end. */
const settled = (el: Element) => {
  for (const a of el.getAnimations()) a.finish()
  return getComputedStyle(el)
}

/** What a press at the middle of `el`'s box would land on. */
const hitAt = (el: Element) => {
  const box = rect(el)
  return document.elementFromPoint(
    box.left + box.width / 2,
    box.top + box.height / 2,
  )
}

/** `box` sits 0.25rem in from the bottom-right corner of `cover`. */
const inBottomRight = (box: DOMRect, cover: DOMRect) => {
  expect(box.right).toBeCloseTo(cover.right - 4, 0)
  expect(box.bottom).toBeCloseTo(cover.bottom - 4, 0)
}

describe('a volume tile’s controls on its cover (O283)', () => {
  it.each([
    ['1280 comfortable', 1280, undefined, 'mouse'],
    ['1280 compact', 1280, 104, 'mouse'],
    ['390', 390, undefined, 'phone'],
  ] as const)(
    'sets i 0.25rem in from the cover’s bottom-right corner at %s, on a readable and an unreadable tile',
    async (_, width, coverSize, device) => {
      onTestFinished(pointerRulesAs(device))
      const { tiles } = await drawn({ width, coverSize, panel: 'button' })
      // The unreadable tile draws its cover in a box, not a link.
      expect(tiles[0].querySelector(':scope > a .cover-box')).not.toBeNull()
      expect(
        tiles[1].querySelector(':scope > .tile-unreadable .cover-box'),
      ).not.toBeNull()
      for (const tile of tiles.slice(0, 2)) {
        const i = rect(iOf(tile))
        inBottomRight(i, coverOf(tile))
        if (device === 'phone') {
          expect(i.width).toBeCloseTo(44, 0)
          expect(i.height).toBeCloseTo(44, 0)
        } else {
          expect(i.width).toBeGreaterThanOrEqual(24)
          expect(i.height).toBeGreaterThanOrEqual(24)
        }
      }
    },
  )

  it('draws ⇄ at the cover’s top-right, clear of i', async () => {
    onTestFinished(pointerRulesAs('phone'))
    const { tiles } = await drawn({ width: 390, panel: 'button' })
    const cover = coverOf(tiles[0])
    const mark = rect(tiles[0].querySelector('.cover-box .aligned'))
    expect(mark.top).toBeCloseTo(cover.top + 4, 0)
    expect(mark.right).toBeCloseTo(cover.right - 4, 0)
    expect(overlapOf(areaOf(mark), areaOf(rect(iOf(tiles[0]))))).toBe(0)

    // The library grid's ⇄ stays at its cover's bottom-right.
    const grid = mount(
      <div className="library">
        <div className="sidebar-column" />
        <main>
          <ul className="grid">
            <li>
              <a href="#series">
                <span className="cover-box">
                  <span className="cover-missing" />
                  <span className="aligned">⇄ 2</span>
                </span>
                <span className="title">A series</span>
              </a>
            </li>
          </ul>
        </main>
      </div>,
    )
    const gridCover = rect(grid.querySelector('.grid .cover-box'))
    const gridMark = rect(grid.querySelector('.grid .aligned'))
    expect(gridMark.bottom).toBeCloseTo(gridCover.bottom - 4, 0)
    expect(gridMark.right).toBeCloseTo(gridCover.right - 4, 0)
  })

  it.each([
    ['off', 'phone'],
    ['off', 'mouse'],
    ['on-select', 'phone'],
    ['on-select', 'mouse'],
  ] as const)(
    'puts ↓ on i’s corner, at i’s size, where a tile has no i (%s, %s)',
    async (panel, device) => {
      onTestFinished(pointerRulesAs(device))
      const { tiles } = await drawn({
        width: device === 'phone' ? 390 : 1280,
        panel,
      })
      expect(tiles[0].querySelector('.panel-open')).toBeNull()
      const get = tiles[0].querySelector<HTMLElement>(':scope > a.get')
      if (get === null) throw new Error('no download on the corner')
      const box = rect(get)
      inBottomRight(box, coverOf(tiles[0]))
      if (device === 'phone') {
        expect(box.width).toBeCloseTo(44, 0)
        expect(box.height).toBeCloseTo(44, 0)
        expect(settled(get).opacity).toBe('1')
      } else {
        expect(box.width).toBeCloseTo(24, 0)
        expect(box.height).toBeCloseTo(24, 0)
        // Shown by i's rule: waiting at rest, there on hover.
        expect(settled(get).opacity).toBe('0')
        await real(() =>
          userEvent.hover(tiles[0].querySelector(':scope > a') as Element),
        )
        expect(settled(get).opacity).toBe('1')
      }
    },
  )

  it('hides i until its tile is hovered or holds focus, or its panel is open, where every pointer hovers', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles, heading } = await drawn({ width: 1280, panel: 'button' })
    const tile = tiles[0]
    const link = tile.querySelector(':scope > a') as HTMLAnchorElement
    const i = iOf(tile)

    // Unseen and laid out: a press at its place lands on the cover's link.
    expect(settled(i).opacity).toBe('0')
    expect(settled(i).pointerEvents).toBe('none')
    expect(rect(i).width).toBeGreaterThan(0)
    expect(link.contains(hitAt(i))).toBe(true)

    await real(() => userEvent.hover(link))
    expect(settled(i).opacity).toBe('1')
    expect(settled(i).pointerEvents).toBe('auto')

    await real(() => userEvent.hover(heading))
    link.focus()
    await real(() => userEvent.keyboard('{Tab}'))
    expect(document.activeElement).toBe(i)
    expect(settled(i).opacity).toBe('1')

    await real(() => userEvent.hover(link))
    await real(() => userEvent.click(i))
    await settleUntil(
      'the panel',
      () => tile.querySelector('.tile-panel .alignment-panel') !== null,
      1000,
    )
    await real(() =>
      userEvent.click(
        tile.querySelector('.tile-panel .alignment-panel p') as Element,
      ),
    )
    expect(document.activeElement).toBe(document.body)
    await real(() => userEvent.hover(heading))
    expect(i.getAttribute('aria-expanded')).toBe('true')
    expect(settled(i).opacity).toBe('1')
  })

  it('moves no tile as i fades in', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles } = await drawn({ width: 1280, panel: 'button' })
    const boxes = () =>
      tiles.flatMap((tile) =>
        [tile, tile.firstElementChild, tile.querySelector('.title')].map((el) =>
          rect(el).toJSON(),
        ),
      )
    const i = iOf(tiles[0])
    expect(settled(i).opacity).toBe('0')
    const before = boxes()
    await real(() =>
      userEvent.hover(tiles[0].querySelector(':scope > a') as Element),
    )
    expect(settled(i).opacity).toBe('1')
    expect(boxes()).toEqual(before)
  })

  it.each([
    ['phone', 44, 390],
    ['touch laptop', 24, 1280],
  ] as const)('keeps i shown on a %s, %i px', async (device, size, width) => {
    onTestFinished(pointerRulesAs(device))
    const { tiles } = await drawn({ width, panel: 'button' })
    const i = iOf(tiles[0])
    expect(settled(i).opacity).toBe('1')
    expect(rect(i).width).toBeCloseTo(size, 0)
    expect(rect(i).height).toBeCloseTo(size, 0)
    expect(hitAt(i)).toBe(i)
  })

  it('fades i over 120 ms, and not under reduced motion', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles } = await drawn({ width: 1280, panel: 'button' })
    const i = iOf(tiles[0])
    expect(getComputedStyle(i).transitionDuration).toBe('0.12s')
    const asks = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'prefers-reduced-motion', value }],
      })
    onTestFinished(async () => {
      await asks('')
    })
    await asks('reduce')
    expect(window.matchMedia('(prefers-reduced-motion: reduce)').matches).toBe(
      true,
    )
    expect(getComputedStyle(i).transitionDuration).toBe('0s')
  })

  it.each([
    ['mouse', 24],
    ['phone', 44],
  ] as const)(
    'draws the panel’s ↓ Download as a word button, %s',
    async (device, least) => {
      onTestFinished(pointerRulesAs(device))
      const { tiles } = await drawn({ width: 1280, panel: 'button' })
      await real(() =>
        userEvent.hover(tiles[0].querySelector(':scope > a') as Element),
      )
      await real(() => userEvent.click(iOf(tiles[0])))
      await settleUntil(
        'the panel’s download',
        () => tiles[0].querySelector('.tile-panel a.get') !== null,
        1000,
      )
      const get = tiles[0].querySelector('.tile-panel a.get') as HTMLElement
      const style = getComputedStyle(get)
      expect(style.fontSize).toBe('14px')
      expect(
        Number.parseFloat(style.paddingRight) -
          Number.parseFloat(style.paddingLeft),
      ).toBeCloseTo(4, 1)
      expect(rect(get).height).toBeGreaterThanOrEqual(least)
    },
  )
})
