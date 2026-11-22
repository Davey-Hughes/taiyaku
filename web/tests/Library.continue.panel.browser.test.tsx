import { Link, Route, useParams } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import * as aboutApi from '../src/about'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import { ABOUT } from './support/about'
import { settleUntil } from './support/async'
import { byHand, clickAt, pointAt, until } from './support/byHand'
import {
  ADMIN,
  cardsIn,
  iOf,
  linkOf,
  need,
  open,
  openFor,
  pageArea,
  panelIn,
  rect,
  titleIn,
} from './support/continuePanel'
import { inProgress, seriesCard } from './support/continueRow'
import { homePage } from './support/homePage'
import { mouse } from './support/mouse'
import { pointerRulesAs } from './support/pointer'

/**
 * The Continue row's panel (O417) as a browser styles it. The panel is a
 * child of the row's section beside the row's list, so a rule written for
 * the list, a card's link or a card's title that reaches the whole section
 * would reach the panel too; jsdom matches selectors but lays nothing out,
 * so only a browser shows what such a rule does to it.
 */

// The build's label at the sidebar's foot asks for the build; answered here.
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
  work: vi.fn(),
}))

/** Few enough cards that the row does not overflow at the width drawn. */
const ENTRIES = 4

/** Enough cards that the row overflows at 1280 px. */
const OVERFLOWING = 12

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: 8 }, (_, i) => seriesCard(i + 1)),
    unfiled: [],
  })
  // A verdict and a content match, so the panel draws both of its lists.
  vi.mocked(library.work).mockImplementation(async (slug) => {
    const card = inProgress(OVERFLOWING).find((w) => w.slug === slug)
    if (card === undefined) throw new Error(`no card ${slug}`)
    return {
      ...card,
      editions: card.editions.map((e) => ({
        ...e,
        matched: { id: 9, score: 0.97 },
      })),
      series: null,
      alignments: [
        {
          from: 'raw',
          to: 'en',
          verdict: 'confident',
          slots: 180,
          paired: 178,
        },
      ],
      kept_apart: [],
    }
  })
})

/** `el`'s style once its transitions have run to their end. */
const settled = (el: Element) => {
  for (const a of el.getAnimations()) a.finish()
  return getComputedStyle(el)
}

/** Whether `el` is seen and takes a press, as its style has it. */
const shownOf = (el: Element) => {
  const style = settled(el)
  return { opacity: style.opacity, pointerEvents: style.pointerEvents }
}

const SHOWN = { opacity: '1', pointerEvents: 'auto' }
const WAITING = { opacity: '0', pointerEvents: 'none' }

/**
 * The home page at `width` px for an administrator, the cover size
 * `coverSize` where given, `entries` cards in the row (`ENTRIES` unless
 * given) and `page`'s browse preferences and routes, once the row's cards
 * are drawn and the fonts are in. The pointer is moved off every card
 * first: a card drawn under it would be hovered.
 */
async function drawn(
  width: number,
  coverSize?: number,
  entries = ENTRIES,
  page: Pick<Parameters<typeof homePage>[0], 'browse' | 'routes'> = {},
) {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(entries),
    added: [],
    read: [],
  })
  await mouse.moveTo(0, 0)
  const host = await homePage({ width, coverSize, who: ADMIN, ...page })
  await settleUntil(
    'the Continue row',
    () => cardsIn(host).length === entries,
    2000,
  )
  await document.fonts.ready
  return {
    host,
    cards: cardsIn(host),
    heading: need(host, 'section.continue > h2'),
  }
}

/** What the row's own list rules lay out: its fade, its flow, its scroll. */
const laid = (el: Element) => {
  const style = getComputedStyle(el)
  return {
    masked: style.maskImage !== 'none',
    flow: style.gridAutoFlow,
    overflowX: style.overflowX,
  }
}

describe('the Continue row’s panel beside the row’s list (O417)', () => {
  it('takes none of the row’s rules for its list, its cards’ links or their titles', async () => {
    const { host, cards } = await drawn(1280)
    const card = cards[1]
    const link = linkOf(card)
    // As a hand opens it: the pointer onto the card, then a press on its `i`.
    await byHand(async () => {
      await pointAt(link)
      await clickAt(iOf(card))
      await until(
        'the panel’s match marks',
        () =>
          host.querySelector('section.continue > .tile-panel .match-marks') !==
          null,
      )
    })
    await document.fonts.ready
    const panel = need(host, 'section.continue > .tile-panel')

    // The row's list, as drawn with the rules this case looks for.
    expect(laid(need(host, 'section.continue > ul')), 'the row').toEqual({
      masked: true,
      flow: 'column',
      overflowX: 'auto',
    })
    expect(laid(panel).masked, 'the panel').toBe(false)
    const lists = [...panel.querySelectorAll('ul')]
    expect(
      lists.map((ul) => ul.parentElement?.className),
      'the panel’s lists',
    ).toEqual(['alignment-panel', 'match-marks'])
    for (const ul of lists) {
      expect(laid(ul), `.${ul.parentElement?.className} ul`).toEqual({
        masked: false,
        flow: 'row',
        overflowX: 'visible',
      })
    }

    // The Title field in the panel's face, unclamped, as the card's title,
    // in the title face and clamped to two lines, is not.
    const title = getComputedStyle(need(link, '.title'))
    const face = getComputedStyle(panel).fontFamily
    expect(
      {
        face: title.fontFamily === face,
        clamp: title.getPropertyValue('-webkit-line-clamp'),
      },
      'the card’s title',
    ).toEqual({ face: false, clamp: '2' })
    const field = getComputedStyle(need(panel, 'label.field.title'))
    expect(
      {
        face: field.fontFamily,
        clamp: field.getPropertyValue('-webkit-line-clamp'),
        overflow: field.overflow,
      },
      'the Title field',
    ).toEqual({ face, clamp: 'none', overflow: 'visible' })

    // ↓ Download a row, as the card's link, a column, is not.
    expect(getComputedStyle(link).flexDirection, 'the card’s link').toBe(
      'column',
    )
    const download = getComputedStyle(need(panel, 'a.get'))
    expect(
      { direction: download.flexDirection, gap: download.rowGap },
      '↓ Download',
    ).toEqual({ direction: 'row', gap: 'normal' })
  })
})

// Each card's `i` waits, shows and sits on its cover by the rules a volume
// tile's `i` keeps (`library.css`), and jsdom draws no hover: so a real
// mouse, outside `act`, under each device's pointer rules.
describe('i on a Continue card at rest, held with a real mouse (O417)', () => {
  it('hides every i but the hovered card’s, where every pointer hovers', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { cards, heading } = await drawn(1280)
    /** Each card's `i` shown where its card is `hovered`, waiting elsewhere. */
    const each = (hovered?: number) => {
      for (const [n, card] of cards.entries()) {
        expect
          .soft(shownOf(iOf(card)), `card ${n + 1}’s i`)
          .toEqual(n === hovered ? SHOWN : WAITING)
      }
    }

    await byHand(() => pointAt(heading))
    // Waiting, not gone: laid out, so Tab still reaches it.
    for (const [n, card] of cards.entries()) {
      const i = iOf(card)
      expect
        .soft(
          { connected: i.isConnected, laid: rect(i).width > 0 },
          `card ${n + 1}’s i`,
        )
        .toEqual({ connected: true, laid: true })
    }
    each()

    await byHand(() => pointAt(linkOf(cards[1])))
    each(1)

    await byHand(() => pointAt(heading))
    each()
  })

  it.each([
    ['touch laptop', 1280],
    ['phone', 390],
  ] as const)(
    'shows every i, no card hovered, on a %s',
    async (device, width) => {
      onTestFinished(pointerRulesAs(device))
      const { cards, heading } = await drawn(width)
      await byHand(() => pointAt(heading))
      for (const [n, card] of cards.entries()) {
        const i = iOf(card)
        expect.soft(shownOf(i), `card ${n + 1}’s i`).toEqual(SHOWN)
        // A finger's target (spec 2026-09-17 §7.1).
        if (device === 'phone') {
          const box = rect(i)
          expect
            .soft(box.width, `card ${n + 1}’s i across`)
            .toBeGreaterThanOrEqual(44)
          expect
            .soft(box.height, `card ${n + 1}’s i down`)
            .toBeGreaterThanOrEqual(44)
        }
      }
    },
  )

  it('hides i once the mouse leaves its card, after the mouse opened and closed its panel (O420)', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards, heading } = await drawn(1280)
    const panel = () => host.querySelector('section.continue > .tile-panel')
    const i = iOf(cards[1])
    await byHand(async () => {
      await pointAt(linkOf(cards[1]))
      await clickAt(i)
      await until('the panel', () => panel() !== null)
      await clickAt(i)
      await until('the panel closed', () => panel() === null)
      await pointAt(heading)
    })
    // The fixture tells: i keeps the focus the press gave it.
    expect(document.activeElement).toBe(i)
    expect(i.getAttribute('aria-expanded')).toBe('false')
    expect(shownOf(i)).toEqual(WAITING)
  })

  it.each([
    ['96 px covers', 1280, 96, 'mouse'],
    ['144 px covers', 1280, 144, 'mouse'],
    ['240 px covers', 1280, 240, 'mouse'],
    ['390 px', 390, undefined, 'phone'],
  ] as const)(
    'sets each i inside its cover, 0.25rem in from the bottom-right corner, at %s',
    async (_, width, coverSize, device) => {
      onTestFinished(pointerRulesAs(device))
      const { cards } = await drawn(width, coverSize)
      for (const [n, card] of cards.entries()) {
        const cover = rect(need(linkOf(card), ':scope > .cover-missing'))
        const i = rect(iOf(card))
        expect
          .soft(
            {
              right: cover.right - i.right,
              bottom: cover.bottom - i.bottom,
              left: i.left >= cover.left,
              top: i.top >= cover.top,
            },
            `card ${n + 1}’s i`,
          )
          .toEqual({
            right: expect.closeTo(4, 0),
            bottom: expect.closeTo(4, 0),
            left: true,
            top: true,
          })
      }
    },
  )
})

/**
 * How far `el` scrolls sideways: none where it holds what it draws. The
 * root's `scrollWidth`, which its gutter narrows, can read under its
 * `clientWidth`.
 */
const sideways = (el: Element) => Math.max(0, el.scrollWidth - el.clientWidth)

// Where the panel lies is read from the layout: the row's laid gap, the
// cards' boxes and the page area's edges, none of which jsdom lays out.
describe('the Continue row’s panel laid at rest (O417)', () => {
  it('lies under the row from the first card’s left edge, and back from the right edge of the last card wholly in view, two cards and the laid gap wide', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards } = await drawn(1280, undefined, OVERFLOWING)
    const row = need(host, 'section.continue > ul')
    const area = pageArea(host)
    const first = rect(cards[0])
    const gap = rect(cards[1]).left - first.right
    const least = Number.parseFloat(
      getComputedStyle(row).getPropertyValue('--tile-gap'),
    )
    // At rest the cards in view are the row's first.
    const last =
      cards.filter((card) => rect(card).right <= area.right + 0.5).length - 1
    const end = rect(cards[last])
    // The fixture tells: the row overflows, spread over the grid's columns,
    // so its laid gap is not the least gap; the first card starts the area,
    // and from the last card in view two cards and the gap run past its end.
    expect({
      overflows: sideways(row) > 0,
      spread: gap > least + 1,
      first: first.left,
      past: end.left + 2 * end.width + gap > area.right + 0.5,
      next: rect(cards[last + 1]).right > area.right,
    }).toEqual({
      overflows: true,
      spread: true,
      first: expect.closeTo(area.left, 0),
      past: true,
      next: true,
    })

    // `top` at the row's foot is S6, a starting value the owner's try may
    // move (TRY): a later change of where the panel sits edits it here.
    const foot = rect(row).bottom
    let box = rect(await open(host, cards[0]))
    expect(
      { left: box.left, width: box.width, top: box.top },
      'the first card’s panel',
    ).toEqual({
      left: expect.closeTo(first.left, 0),
      width: expect.closeTo(2 * first.width + gap, 0),
      top: expect.closeTo(foot, 0),
    })

    box = rect(await open(host, cards[last]))
    expect(
      { right: box.right, width: box.width, top: box.top },
      'the last card in view’s panel',
    ).toEqual({
      right: expect.closeTo(end.right, 0),
      width: expect.closeTo(2 * end.width + gap, 0),
      top: expect.closeTo(foot, 0),
    })
  })

  it.each([
    ['start', 3],
    ['end', 8],
  ] as const)(
    'lies inside the page area at full width, its card scrolled half past the area’s %s',
    async (edge, index) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { host, cards } = await drawn(1280, undefined, OVERFLOWING)
      const row = need(host, 'section.continue > ul')
      const main = need(host, '.library > main')
      const area = pageArea(host)
      const card = cards[index]
      const gap = rect(cards[1]).left - rect(cards[0]).right
      const at = edge === 'start' ? area.left : area.right
      await byHand(async () => {
        const box = rect(card)
        row.scrollLeft += box.left + box.width / 2 - at
      })
      const box = rect(card)
      // The fixture tells: the card's middle is on the edge, so half of it
      // lies past it.
      expect({
        across: box.left < at && box.right > at,
        middle: box.left + box.width / 2,
      }).toEqual({ across: true, middle: expect.closeTo(at, 0) })

      // The `i` of a card half past the row's end is cut off with its far
      // half, where no pointer reaches it; where the panel lies is the case.
      await byHand(async () => {
        iOf(card).click()
        await until('the panel', () => openFor(host, card))
      })
      const panel = rect(need(host, 'section.continue > .tile-panel'))
      const full = 2 * box.width + gap
      const left = edge === 'start' ? area.left : area.right - full
      expect({
        left: panel.left,
        right: panel.right,
        sideways: sideways(main),
      }).toEqual({
        left: expect.closeTo(left, 0),
        right: expect.closeTo(left + full, 0),
        sideways: 0,
      })
    },
  )

  it.each([
    ['the left column, 144 px covers', 144, 0],
    ['the right column, 144 px covers', 144, 1],
    ['the one column, 240 px covers', 240, 0],
  ] as const)(
    'is the page area wide at 390 px from %s',
    async (_, size, index) => {
      onTestFinished(pointerRulesAs('phone'))
      const { host, cards } = await drawn(390, size)
      const area = pageArea(host)
      const card = rect(cards[index])
      const side = index === 0 ? 'left' : 'right'
      // The fixture tells: the columns the row lays at this size, and the
      // card in the one named, on that edge of the area.
      expect({
        columns: cards.filter((c) => rect(c).right <= area.right + 0.5).length,
        [side]: card[side],
      }).toEqual({
        columns: size === 144 ? 2 : 1,
        [side]: expect.closeTo(area[side], 0),
      })

      const box = rect(await open(host, cards[index]))
      expect({
        left: box.left,
        right: box.right,
        sideways: sideways(document.documentElement),
      }).toEqual({
        left: expect.closeTo(area.left, 0),
        right: expect.closeTo(area.right, 0),
        sideways: 0,
      })
    },
  )

  it.each([
    ['overflows', OVERFLOWING],
    ['fits', 3],
  ] as const)(
    'moves nothing as it opens, in a row that %s, and is what is drawn over the chips',
    async (fit, entries) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { host, cards } = await drawn(1280, undefined, entries)
      const tiles = () => [...host.querySelectorAll('main .grid > li')]
      await settleUntil('the grid', () => tiles().length === 8, 2000)
      const row = need(host, 'section.continue > ul')
      const chips = [
        ...host.querySelectorAll<HTMLElement>('.library-chips button'),
      ]
      const boxes = () => ({
        cards: cards.map((card) => rect(card).toJSON()),
        tiles: tiles().map((tile) => rect(tile).toJSON()),
        chips: [need(host, '.library-chips'), ...chips].map((chip) =>
          rect(chip).toJSON(),
        ),
        section: rect(need(host, 'section.continue')).toJSON(),
      })
      // The fixture tells: the row overflows, or fits, as named.
      expect(sideways(row) > 0).toBe(fit === 'overflows')

      const before = boxes()
      const panel = await open(host, cards[0])
      const after = boxes()
      for (const key of Object.keys(before) as (keyof typeof before)[]) {
        expect.soft(after[key], key).toEqual(before[key])
      }

      const box = rect(panel)
      const under = chips.flatMap((chip) => {
        const at = rect(chip)
        const x = at.left + at.width / 2
        const y = at.top + at.height / 2
        return x > box.left && x < box.right && y > box.top && y < box.bottom
          ? [{ name: chip.textContent, x, y }]
          : []
      })
      // The fixture tells: the panel lies over a chip's middle.
      expect(under.length, 'chips under the panel').toBeGreaterThan(0)
      for (const { name, x, y } of under) {
        const hit = document.elementFromPoint(x, y)
        expect
          .soft(
            hit?.closest('.tile-panel') === panel ? 'the panel' : hit?.tagName,
            `what is drawn at ${name}’s middle`,
          )
          .toBe('the panel')
      }
    },
  )
})

/** The cards' titles, in the row's order. */
const TITLES = inProgress(OVERFLOWING).map((w) => w.title)

/** A page whose panel follows the pointer and the focus (§3's `on-select`). */
const ON_SELECT = { browse: { panel: 'on-select' } }

/** Whether a pointer has come onto `el`: the browser's own `mouseenter`. */
const entered = (el: Element) => {
  const seen = { yes: false }
  el.addEventListener('mouseenter', () => {
    seen.yes = true
  })
  return seen
}

/**
 * The reader's route, drawing the volume it opens and a way home. Declared
 * inside a function for the reason `Library.test.tsx`'s `locationProbe()`
 * gives.
 */
function readerRoute() {
  function Opened() {
    return (
      <>
        <p data-testid="opened">{useParams().slug}</p>
        <Link to="/" data-testid="home">
          Home
        </Link>
      </>
    )
  }
  return <Route path="/read/:slug" element={<Opened />} />
}

// `on-select` draws no `i`: the pointer and the focus choose whose panel is
// open, as on a series page (`Series.onSelect.browser.test.tsx`), and jsdom
// moves no pointer.
describe('the Continue row’s on-select panel, held with a real mouse (O417)', () => {
  it('opens under the row for the card the pointer comes onto, stays while the pointer is on it, moves to the next card, and stays while focus is in its Title field', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards } = await drawn(1280, undefined, ENTRIES, ON_SELECT)
    const section = need(host, 'section.continue')
    const row = need(section, ':scope > ul')
    // The fixture tells: no card draws an `i` in this mode.
    expect(section.querySelectorAll('.panel-open')).toHaveLength(0)

    await byHand(async () => {
      await pointAt(linkOf(cards[0]))
      await until('card 1’s panel', () => titleIn(host) === TITLES[0])
    })
    const first = rect(need(section, ':scope > .tile-panel'))
    expect({ left: first.left, top: first.top }, 'card 1’s panel').toEqual({
      left: expect.closeTo(rect(cards[0]).left, 0),
      top: expect.closeTo(rect(row).bottom, 0),
    })

    // Down off the card into its panel, then across the panel under the
    // next card's column, as a hand reaches for what the panel holds.
    const x = rect(cards[0]).left + rect(cards[0]).width / 2
    const y = first.top + first.height / 2
    const across = rect(cards[1]).left + rect(cards[1]).width / 2 - x
    await byHand(async () => {
      await mouse.moveTo(x, y)
      await mouse.along(x, y, 0, across)
    })
    const under = rect(cards[1])
    // The fixture tells: the pointer rests where the panel lay, under card 2.
    expect({
      inPanel:
        x + across > first.left &&
        x + across < first.right &&
        y > first.top &&
        y < first.bottom,
      underCard2: x + across > under.left && x + across < under.right,
    }).toEqual({ inPanel: true, underCard2: true })
    const resting = document.elementFromPoint(x + across, y)
    expect(
      {
        drawn:
          panelIn(host) !== null &&
          resting?.closest('.tile-panel') === panelIn(host),
        title: titleIn(host),
      },
      'the panel with the pointer on it',
    ).toEqual({ drawn: true, title: TITLES[0] })

    await byHand(async () => {
      await pointAt(linkOf(cards[2]))
      await until('card 3’s panel', () => titleIn(host) === TITLES[2])
    })
    expect(
      rect(need(section, ':scope > .tile-panel')).left,
      'card 3’s panel',
    ).toBeCloseTo(rect(cards[2]).left, 0)

    const field = need<HTMLTextAreaElement>(
      section,
      ":scope > .tile-panel textarea[name='title']",
    )
    await byHand(() => clickAt(field))
    // The fixture tells: the press put focus in the Title field.
    expect(document.activeElement, 'focus after the press').toBe(field)
    const seen = entered(cards[0])
    await byHand(async () => {
      await pointAt(linkOf(cards[0]))
      await until('the pointer on card 1', () => seen.yes)
    })
    expect(
      { title: titleIn(host), focused: document.activeElement === field },
      'the panel with focus in its Title field',
    ).toEqual({ title: TITLES[2], focused: true })
  })

  it('opens the reader on a press on the card whose panel the pointer opened, round after round', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host } = await drawn(1280, undefined, ENTRIES, {
      ...ON_SELECT,
      routes: readerRoute(),
    })
    const opened: (string | null | undefined)[] = []
    // Each round from the home page drawn again, its row and grid both in:
    // the row is laid over the grid's columns.
    await byHand(async () => {
      for (let round = 1; round <= 3; round++) {
        await until(
          `round ${round}: the row and the grid`,
          () =>
            cardsIn(host).length === ENTRIES &&
            host.querySelectorAll('main .grid > li').length === 8,
        )
        const card = cardsIn(host)[round - 1]
        await pointAt(linkOf(card))
        await until(
          `round ${round}: card ${round}’s panel`,
          () => titleIn(host) === TITLES[round - 1],
        )
        await clickAt(linkOf(card))
        await until(
          `round ${round}: the reader`,
          () => host.querySelector('[data-testid="opened"]') !== null,
        )
        opened.push(host.querySelector('[data-testid="opened"]')?.textContent)
        await clickAt(need(host, '[data-testid="home"]'))
      }
    })
    expect(opened).toEqual(['kagami-v01', 'kagami-v02', 'kagami-v03'])
  })
})

/** A series page's route, where a press on a grid tile leads. */
const SERIES_ROUTE = (
  <Route path="/series/:id" element={<p data-testid="series">A series</p>} />
)

/**
 * A real press at (`x`, `y`): the button down, then up. Answers whether a
 * panel was drawn while the button was down, before the click the press
 * makes, which on a link leaves the page.
 */
async function pressAt(host: Element, x: number, y: number) {
  await byHand(async () => {
    await mouse.moveTo(x, y)
    await mouse.press()
  })
  const held = panelIn(host) !== null
  await byHand(() => mouse.release())
  return held
}

// The panel's closes by a press (`volumePanel.ts`), as a hand makes them.
describe('the Continue row’s panel and a real press (O417)', () => {
  /**
   * Card 1's panel open in `panel` mode, as a hand opens it there, with the
   * grid drawn below it.
   */
  async function opened(panel: 'button' | 'on-select' = 'button') {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards } = await drawn(1280, undefined, ENTRIES, {
      browse: { panel },
      routes: SERIES_ROUTE,
    })
    const tiles = () => [...host.querySelectorAll('main .grid > li')]
    await settleUntil('the grid', () => tiles().length === 8, 2000)
    if (panel === 'button') await open(host, cards[0])
    else {
      await byHand(async () => {
        await pointAt(linkOf(cards[0]))
        await until('card 1’s panel', () => titleIn(host) === TITLES[0])
      })
    }
    return {
      host,
      cards,
      panel: need(host, 'section.continue > .tile-panel'),
      tiles: tiles(),
    }
  }

  // In `button` mode the focus the press gives the tile's link closes the
  // panel too; `on-select` has only the press.
  it.each(['button', 'on-select'] as const)(
    'closes on a real press on a grid tile, and the tile still opens its series, in %s mode',
    async (mode) => {
      const { host, panel, tiles } = await opened(mode)
      const box = rect(panel)
      const tile = tiles.find((t) => rect(t).left >= box.right)
      if (tile === undefined) throw new Error('no grid tile clear of the panel')
      const at = rect(need(tile, 'a'))
      const x = at.left + at.width / 2
      const y = at.top + at.height / 2
      // The fixture tells: the press lands on the tile.
      expect(document.elementFromPoint(x, y)?.closest('main .grid > li')).toBe(
        tile,
      )

      const held = await pressAt(host, x, y)
      expect(held, 'the panel while the button is down').toBe(false)
      // The router draws the series page in a transition, which can land
      // frames after the click.
      await byHand(() =>
        until(
          'the series page',
          () => host.querySelector('[data-testid="series"]') !== null,
        ),
      )
    },
  )

  it('closes on a real press on the page area’s blank space beside the grid', async () => {
    const { host, cards, tiles } = await opened()
    const i = iOf(cards[0])
    const main = need(host, '.library > main')
    const x = rect(main).left + main.clientLeft + 6
    const y = rect(tiles[0]).top + rect(tiles[0]).height / 2
    // The fixture tells: the press lands on `main`, in its padding.
    expect(document.elementFromPoint(x, y)).toBe(main)

    const held = await pressAt(host, x, y)
    expect({
      held,
      after: panelIn(host) !== null,
      expanded: i.getAttribute('aria-expanded'),
    }).toEqual({ held: false, after: false, expanded: 'false' })
  })

  it('stays open through a real press inside it', async () => {
    const { host, cards, panel } = await opened()
    const i = iOf(cards[0])
    const box = rect(panel)
    const x = box.left + 3
    const y = box.top + 3
    // The fixture tells: the press lands in the panel.
    expect(document.elementFromPoint(x, y)?.closest('.tile-panel')).toBe(panel)

    const held = await pressAt(host, x, y)
    expect({
      held,
      after: panelIn(host) === panel,
      expanded: i.getAttribute('aria-expanded'),
    }).toEqual({ held: true, after: true, expanded: 'true' })
  })
})
