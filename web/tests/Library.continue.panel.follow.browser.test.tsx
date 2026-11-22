import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import * as aboutApi from '../src/about'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import * as metadata from '../src/metadata'
import { invalidateLibraryViews } from '../src/pageReads'
import { PHONE_LAYOUT } from '../src/phoneLayout'
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
  pageArea,
  panelIn,
  rect,
  titleIn,
} from './support/continuePanel'
import { gridColumns, inProgress, seriesCard } from './support/continueRow'
import { homePage } from './support/homePage'
import {
  afterMotion,
  dragged,
  eachFrame,
  frames,
  scrollerOf,
} from './support/motion'
import { mouse } from './support/mouse'
import { pointerRulesAs } from './support/pointer'

/**
 * The Continue row's panel (O417) under a row that moves: the row scrolled
 * sideways under it, laid again frame by frame through a slide, at a cover
 * size or a layout, drawn again from a new answer, or carried by the page's
 * scroll. A panel whose card has wholly left the row's view closes, by
 * O417's ruling. Read from the page as drawn, with the real mouse and keys
 * outside `act`: jsdom lays nothing out and scrolls nothing.
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

vi.mock('../src/metadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/metadata')>()),
  patchWork: vi.fn(),
}))

/** Enough cards that the row overflows at 1280 px. */
const OVERFLOWING = 12

/** The cards' titles, in the row's order. */
const TITLES = inProgress(OVERFLOWING).map((w) => w.title)

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
  vi.mocked(metadata.patchWork).mockImplementation(async (_, fields) => ({
    taiyaku_library: 1,
    updated: '2026-11-22T00:00:00Z',
    title: fields.title ?? '',
    persisted: true,
  }))
})

/**
 * The home page at `width` px (1280 unless given) for an administrator,
 * `OVERFLOWING` cards in the row and the panel in `mode`, once the row's
 * cards are drawn and the fonts are in; `onCommit`, where given, is called
 * for each commit inside `Library`. The pointer is moved off every card
 * first: a card drawn under it would be hovered.
 */
async function drawn(
  mode: 'button' | 'on-select' = 'button',
  { width = 1280, onCommit }: { width?: number; onCommit?: () => void } = {},
) {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(OVERFLOWING),
    added: [],
    read: [],
  })
  await mouse.moveTo(0, 0)
  const host = await homePage({
    width,
    who: ADMIN,
    browse: { panel: mode },
    onCommit,
  })
  await settleUntil(
    'the Continue row',
    () => cardsIn(host).length === OVERFLOWING,
    2000,
  )
  await document.fonts.ready
  return {
    host,
    cards: cardsIn(host),
    row: need(host, 'section.continue > ul'),
  }
}

/**
 * The row's view: its list's padding box, which it scrolls its cards
 * across and past which it clips them.
 */
function viewOf(row: HTMLElement) {
  const left = rect(row).left + row.clientLeft
  return { left, right: left + row.clientWidth }
}

/**
 * Turns the real mouse's wheel `by` px rightward over the row, in the
 * room its list keeps under its cards, so the pointer comes onto no card;
 * then waits until the row has stood still for three frames, as a wheel's
 * scroll ends. Run inside `byHand`.
 */
async function wheeled(row: HTMLElement, by: number) {
  const box = rect(row)
  const x = box.left + box.width / 2
  const y = box.bottom - 2
  if (document.elementFromPoint(x, y) !== row) {
    throw new Error('the wheel is not over the row’s own foot')
  }
  const from = row.scrollLeft
  await mouse.moveTo(x, y)
  await mouse.wheel(0, by)
  const end = performance.now() + 2000
  let still = 0
  let was = row.scrollLeft
  while (still < 3 && performance.now() < end) {
    await new Promise(requestAnimationFrame)
    const moved = row.scrollLeft !== from
    still = moved && row.scrollLeft === was ? still + 1 : 0
    was = row.scrollLeft
  }
}

/**
 * Waits up to 2 s for `holds`, outside `act`, and answers whether it came
 * to: a case then says what it found, rather than only that it timed out.
 */
async function cameTo(holds: () => boolean) {
  const end = performance.now() + 2000
  while (!holds() && performance.now() < end) {
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
  return holds()
}

describe('the Continue row’s panel across a sideways scroll (O417)', () => {
  it('moves with its card as a real wheel scrolls the row one card’s pitch, and still starts at the card’s left edge', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards, row } = await drawn()
    const card = cards[2]
    const pitch = rect(cards[1]).left - rect(cards[0]).left
    const panel = await open(host, card)
    const before = rect(panel).left
    // The fixture tells: the row at its start, the panel from its card.
    expect({ scroll: row.scrollLeft, left: before }).toEqual({
      scroll: 0,
      left: expect.closeTo(rect(card).left, 0),
    })

    await byHand(() => wheeled(row, pitch))
    const view = viewOf(row)
    const box = rect(card)
    // The fixture tells: the row moved one pitch, its card still in view.
    expect({
      scroll: row.scrollLeft,
      inView: box.left >= view.left && box.right <= view.right,
    }).toEqual({ scroll: expect.closeTo(pitch, 0), inView: true })

    const after = rect(panel)
    expect(
      {
        drawn: panelIn(host) === panel,
        moved: after.left - before,
        left: after.left,
      },
      'the panel after the scroll',
    ).toEqual({
      drawn: true,
      moved: expect.closeTo(-pitch, 0),
      left: expect.closeTo(box.left, 0),
    })
  })

  // O417's ruling: the row may be scrolled until the card is cut off, and
  // the panel stays; once nothing of the card is left in view, the panel
  // closes as any close does, after the editor's save.
  it.each(['button', 'on-select'] as const)(
    'lies inside the page area with its card half out of view, and closes once the card is wholly out, after one save, focus back on the card unscrolled, in %s mode',
    async (mode) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { host, cards, row } = await drawn(mode)
      const card = cards[2]
      const gap = rect(cards[1]).left - rect(cards[0]).right
      if (mode === 'button') await open(host, card)
      else {
        await byHand(async () => {
          await pointAt(linkOf(card))
          await until('card 3’s panel', () => titleIn(host) === TITLES[2])
        })
      }
      const typed = `${TITLES[2]} (2)`
      const field = need<HTMLTextAreaElement>(
        host,
        "section.continue > .tile-panel textarea[name='title']",
      )
      await byHand(async () => {
        await clickAt(field)
        await userEvent.keyboard('{Control>}{End}{/Control} (2)')
      })
      // The fixture tells: the title typed and unsaved, focus in its field.
      expect({
        value: field.value,
        focused: document.activeElement === field,
        saves: vi.mocked(metadata.patchWork).mock.calls.length,
      }).toEqual({ value: typed, focused: true, saves: 0 })

      const view = viewOf(row)
      await byHand(async () => {
        const box = rect(card)
        await wheeled(row, box.left + box.width / 2 - view.left)
      })
      const half = rect(card)
      // The fixture tells: the card's middle is on the view's edge.
      expect({
        across: half.left < view.left && half.right > view.left,
        middle: half.left + half.width / 2,
      }).toEqual({ across: true, middle: expect.closeTo(view.left, 0) })
      const area = pageArea(host)
      const panel = panelIn(host)
      const laid = panel === null ? undefined : rect(panel)
      expect(
        {
          title: titleIn(host),
          left: laid?.left,
          right: laid?.right,
          saves: vi.mocked(metadata.patchWork).mock.calls.length,
        },
        'the panel with its card half out',
      ).toEqual({
        title: typed,
        left: expect.closeTo(area.left, 0),
        right: expect.closeTo(area.left + 2 * half.width + gap, 0),
        saves: 0,
      })

      // Wheeled until a pixel of the row passes the card's right edge, in
      // whole pixels, as the row scrolls: the focus given back, scrolled
      // into view, would bring the card back.
      const by = Math.ceil(rect(card).right - view.left) + 1
      const to = row.scrollLeft + by
      let closed = false
      await byHand(async () => {
        await wheeled(row, by)
        closed = await cameTo(() => panelIn(host) === null)
      })
      const back = mode === 'button' ? iOf(card) : linkOf(card)
      expect(
        {
          closed,
          saves: vi.mocked(metadata.patchWork).mock.calls,
          focus: document.activeElement === back,
          scroll: row.scrollLeft,
          out: rect(card).right <= view.left,
        },
        'the panel with its card wholly out',
      ).toEqual({
        closed: true,
        saves: [['kagami-v03', { title: typed }]],
        focus: true,
        scroll: expect.closeTo(to, 0),
        out: true,
      })
    },
  )

  it('stays open on its card’s failed save, and sends it once, as the row scrolls on', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    vi.mocked(metadata.patchWork).mockRejectedValue(new Error('500: no'))
    const { host, cards, row } = await drawn()
    const card = cards[2]
    const pitch = rect(cards[1]).left - rect(cards[0]).left
    const panel = await open(host, card)
    const field = need<HTMLTextAreaElement>(panel, "textarea[name='title']")
    await byHand(async () => {
      await clickAt(field)
      await userEvent.keyboard('{Control>}{End}{/Control} (2)')
    })
    const view = viewOf(row)
    let failed = false
    await byHand(async () => {
      await wheeled(row, Math.ceil(rect(card).right - view.left) + 1)
      failed = await cameTo(() => panel.querySelector('p.failure') !== null)
    })
    // The fixture tells: the card wholly out, and its save answered with
    // the failure, drawn in the panel still open.
    expect({
      out: rect(card).right <= view.left,
      failed,
      saves: vi.mocked(metadata.patchWork).mock.calls.length,
    }).toEqual({ out: true, failed: true, saves: 1 })

    await byHand(() => wheeled(row, pitch))
    expect(
      {
        drawn: panelIn(host) === panel,
        saves: vi.mocked(metadata.patchWork).mock.calls.length,
      },
      'the panel after the row scrolled on',
    ).toEqual({ drawn: true, saves: 1 })
  })
})

/**
 * Whether `useContinueRow` has the row mid-slide: it lays the row by both
 * its column and its gap only then.
 */
const sliding = (row: HTMLElement) =>
  row.style.getPropertyValue('--continue-col') !== '' &&
  row.style.getPropertyValue('--continue-gap') !== ''

/**
 * The open panel against its card: its top on the row's foot, two cards and
 * the gap wide, with its left edge on the card's or, laid back from the
 * area's end, its right edge on the card's; and inside the page area.
 */
function laidUnder(host: HTMLElement, card: HTMLElement, row: HTMLElement) {
  const panel = panelIn(host)
  if (panel === null) return { drawn: false }
  const box = rect(panel)
  const at = rect(card)
  const gap = Number.parseFloat(getComputedStyle(row).columnGap)
  const area = pageArea(host)
  return {
    drawn: true,
    top: box.top - rect(row).bottom,
    width: box.width - (2 * at.width + gap),
    edge: Math.min(
      Math.abs(box.left - at.left),
      Math.abs(box.right - at.right),
    ),
    inside: box.left >= area.left - 0.5 && box.right <= area.right + 0.5,
  }
}

/** What `laidUnder` reads of a panel laid under its card. */
const UNDER = {
  drawn: true,
  top: expect.closeTo(0, 0),
  width: expect.closeTo(0, 0),
  edge: expect.closeTo(0, 0),
  inside: true,
}

describe('the Continue row’s panel under a row laid again (O417)', () => {
  it('starts at its card’s left edge in each frame of the row’s slide across a column change and at rest, rendering nothing (O304)', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    let commits = 0
    const { host, cards, row } = await drawn('button', {
      onCommit: () => {
        commits += 1
      },
    })
    const card = cards[2]
    const panel = await open(host, card)
    const columns = gridColumns(host).length
    const seen: { sliding: boolean; card: number; off: number }[] = []
    // Made after the panel opened, so read after the follow's own
    // observers in each frame, as the frame is drawn.
    eachFrame(() => {
      const at = rect(card).left
      seen.push({ sliding: sliding(row), card: at, off: rect(panel).left - at })
    })
    await byHand(() => frames(2))
    commits = 0
    await byHand(async () => {
      await dragged([{ width: 1200 }])
      await afterMotion()
    })
    const mid = seen.filter((frame) => frame.sliding)
    // The fixture tells: a column fewer, and the row slid, its card drawn
    // at three places or more on the way.
    expect({
      columns: gridColumns(host).length,
      frames: mid.length >= 3,
      places: new Set(mid.map((frame) => Math.round(frame.card))).size >= 3,
    }).toEqual({ columns: columns - 1, frames: true, places: true })
    expect(
      {
        drawn: panelIn(host) === panel,
        midSlide: mid.map((frame) => frame.off),
        atRest: rect(panel).left - rect(card).left,
        commits,
      },
      'the panel through the slide',
    ).toEqual({
      drawn: true,
      midSlide: mid.map(() => expect.closeTo(0, 0)),
      atRest: expect.closeTo(0, 0),
      commits: 0,
    })
  })

  // In `button` mode each way to the View menu's slider, a press or focus
  // arriving there, closes the panel first. In `on-select` mode focus can
  // wait on the slider while the pointer opens a card's panel.
  it('lies under its card and inside the page area after a Cover size step, in on-select mode', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards, row } = await drawn('on-select')
    const card = cards[2]
    await byHand(async () => {
      await clickAt(need(host, '.view-button'))
      await until('the View menu', () => !need(host, '.view-panel').hidden)
    })
    const range = need<HTMLInputElement>(
      host,
      '.view-panel .field-slider-track input[type="range"]',
    )
    // Focused as the row's own Cover size cases focus it: a press on its
    // track would move it there.
    range.focus()
    await byHand(async () => {
      await pointAt(linkOf(card))
      await until('card 3’s panel', () => titleIn(host) === TITLES[2])
      // Off the row, so no card the step moves under the pointer is
      // pointed at.
      await mouse.moveTo(0, 0)
    })
    const before = rect(card).width
    await byHand(async () => {
      await userEvent.keyboard('{ArrowRight}')
      await afterMotion()
    })
    // The fixture tells: the step reached the slider and grew the cards,
    // and the View menu is still open over the page.
    expect({
      focused: document.activeElement === range,
      grown: rect(card).width - before,
      menu: !need(host, '.view-panel').hidden,
    }).toEqual({ focused: true, grown: expect.closeTo(2, 0), menu: true })
    expect(
      { title: titleIn(host), ...laidUnder(host, card, row) },
      'the panel after the step',
    ).toEqual({ title: TITLES[2], ...UNDER })
  })

  it.each([
    [1280, 899],
    [899, 1280],
  ] as const)(
    'lies under its card and inside the page area after a resize across 900 px, from %i px to %i px',
    async (from, to) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { host, cards, row } = await drawn('button', { width: from })
      const card = cards[2]
      const panel = await open(host, card)
      const phone = matchMedia(PHONE_LAYOUT).matches
      const before = rect(card).width
      await byHand(async () => {
        await page.viewport(to, 844)
        await afterMotion()
      })
      // The fixture tells: the layout turned, and the card's width with it.
      expect({
        turned: matchMedia(PHONE_LAYOUT).matches !== phone,
        resized: Math.abs(rect(card).width - before) > 1,
      }).toEqual({ turned: true, resized: true })
      expect(
        { same: panelIn(host) === panel, ...laidUnder(host, card, row) },
        'the panel after the resize',
      ).toEqual({ same: true, ...UNDER })
    },
  )

  it('lies under its card where the card now stands after a re-read whose answer drops the card before it', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, cards, row } = await drawn()
    const card = cards[2]
    const pitch = rect(cards[1]).left - rect(cards[0]).left
    const panel = await open(host, card)
    const before = rect(card).left
    vi.mocked(library.home).mockResolvedValue({
      continuing: inProgress(OVERFLOWING).filter((_, i) => i !== 1),
      added: [],
      read: [],
    })
    // Read again as a scan's end reads the Library views.
    await byHand(async () => {
      invalidateLibraryViews()
      await until('the re-read', () => cardsIn(host).length === OVERFLOWING - 1)
    })
    // The fixture tells: the same card, drawn one pitch back.
    expect({
      second: cardsIn(host)[1] === card,
      moved: rect(card).left - before,
    }).toEqual({ second: true, moved: expect.closeTo(-pitch, 0) })
    expect(
      {
        same: panelIn(host) === panel,
        title: titleIn(host),
        left: rect(panel).left - rect(card).left,
        ...laidUnder(host, card, row),
      },
      'the panel after the re-read',
    ).toEqual({
      same: true,
      title: TITLES[2],
      left: expect.closeTo(0, 0),
      ...UNDER,
    })
  })
})

describe('the Continue row’s panel as the page scrolls (O417)', () => {
  /**
   * Turns the real mouse's wheel `by` px down over the row's heading, off
   * the row and the panel, then waits until what scrolls the page has stood
   * still for three frames after moving. Run inside `byHand`.
   */
  async function pageWheeled(host: HTMLElement, by: number) {
    const scroller = scrollerOf(host)
    const heading = need(host, 'section.continue > h2')
    const box = rect(heading)
    const x = box.left + box.width / 2
    const y = box.top + box.height / 2
    if (document.elementFromPoint(x, y) !== heading) {
      throw new Error('the wheel is not over the row’s heading')
    }
    const from = scroller.scrollTop
    await mouse.moveTo(x, y)
    await mouse.wheel(by)
    const end = performance.now() + 2000
    let still = 0
    let was = scroller.scrollTop
    while (still < 3 && performance.now() < end) {
      await new Promise(requestAnimationFrame)
      const moved = scroller.scrollTop !== from
      still = moved && scroller.scrollTop === was ? still + 1 : 0
      was = scroller.scrollTop
    }
  }

  // The panel lies in the row's section, inside what scrolls the page, so
  // the page's scroll carries it with the row and nothing lays it again.
  it.each([1280, 390])(
    'keeps its place under its card as a real wheel scrolls the page, at %i px',
    async (width) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { host, cards, row } = await drawn('button', { width })
      const card = cards[0]
      const panel = await open(host, card)
      const scroller = scrollerOf(host)
      const placed = () => ({
        top: rect(panel).top - rect(row).bottom,
        left: rect(panel).left - rect(card).left,
        width: rect(panel).width,
      })
      const was = {
        place: placed(),
        row: rect(row).top,
        scrolled: scroller.scrollTop,
      }
      // The fixture tells: the panel lies on the row's foot from its card's
      // left edge, and the page has not scrolled.
      expect({
        top: was.place.top,
        left: was.place.left,
        scrolled: was.scrolled,
      }).toEqual({
        top: expect.closeTo(0, 0),
        left: expect.closeTo(0, 0),
        scrolled: 0,
      })

      await byHand(() => pageWheeled(host, 200))
      const moved = scroller.scrollTop - was.scrolled
      // The fixture tells: the page scrolled down, and carried the row up by
      // as much.
      expect({
        moved: moved > 100,
        row: rect(row).top - was.row,
      }).toEqual({ moved: true, row: expect.closeTo(-moved, 0) })
      expect(
        { same: panelIn(host) === panel, title: titleIn(host), ...placed() },
        'the panel after the page’s scroll',
      ).toEqual({
        same: true,
        title: TITLES[0],
        top: expect.closeTo(0, 0),
        left: expect.closeTo(0, 0),
        width: expect.closeTo(was.place.width, 0),
      })
    },
  )
})
