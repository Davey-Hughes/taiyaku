// The reader's scan line across a leave and a return made through the router
// with the real mouse (O451): a kept feed answer gone stale draws the line's
// room with no words, at the height its words take, so the works below stay
// where they stand while the next answer is read; a scan still running fills
// the room, and one that ended closes it.
//
// Every navigation is a real click outside `act`: the router commits in a
// transition, and `act` would draw the return only as it ended, hiding the
// order of the leaving view's cleanup and the returning view's first frame.
// Read at 1280 px, where the line is one line, and at 360 px, where it wraps,
// which is what tells the room from a one-line placeholder. jsdom lays
// nothing out.

import { act } from 'react'
import { Link, MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
import { SCAN_POLL_MS } from '../src/config'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import type { Grid, LibraryStatus } from '../src/library'
import { Library } from '../src/Library'
import type { ScanFeed } from '../src/scanFeed'
import { StatusContext } from '../src/shell'
import { deferred } from './support/async'
import { byHand, clickAt, until } from './support/byHand'
import { need } from './support/continuePanel'
import { seriesCard } from './support/continueRow'
import { beforeUnmount, mount } from './support/mount'
import { serveFeed } from './support/scanFeed'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
}))

/** One library, read. */
const STATUS: LibraryStatus = {
  libraries: [{ id: 1, name: 'Volumes', scanned: true }],
  restricted: false,
}

/** The feed as a reader is sent it while a scan hashes library 1. */
const HASHING: ScanFeed = { ended: 0, libraries: [{ id: 1, step: 'hashing' }] }

/** What the line says for `HASHING`. */
const WORDS = 'A scan is running: Hashing. More series appear as they are read.'

/** The works the grid is read as: four series. */
const works = (): Grid => ({
  series: Array.from({ length: 4 }, (_, i) => seriesCard(i + 1)),
  unfiled: [],
})

/** The widths read: one line at the first, wrapped at the second. */
const WIDE = 1280
const NARROW = 360

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(library.home).mockResolvedValue({
    continuing: [],
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue(works())
})

const lineIn = (host: Element) => host.querySelector('.scan-line')
const firstCardIn = (host: Element) => host.querySelector('.grid > li')

/** The scan line and the first card as drawn now. */
const reading = (host: Element) => {
  const line = lineIn(host)
  return {
    stale: line?.hasAttribute('data-stale'),
    visibility: line == null ? undefined : getComputedStyle(line).visibility,
    height: line?.getBoundingClientRect().height,
    cardTop: firstCardIn(host)?.getBoundingClientRect().top,
  }
}

/** The line's height and the first card's top, both drawn. */
const measured = (host: Element) => ({
  height: need(host, '.scan-line').getBoundingClientRect().height,
  cardTop: need(host, '.grid > li').getBoundingClientRect().top,
})

/**
 * What a frame shows: the line's words, its room alone, no line, or no
 * cards. Told by the computed visibility, what is seen, rather than by
 * `data-stale`, so a room whose words still show reads as words.
 */
type Frame = 'words' | 'room' | 'none' | 'no cards'

const frameOf = (host: Element): Frame => {
  if (firstCardIn(host) === null) return 'no cards'
  const line = lineIn(host)
  if (line === null) return 'none'
  return getComputedStyle(line).visibility === 'hidden' ? 'room' : 'words'
}

/** `frames` with each run of one state cut to one. */
const runsOf = (frames: Frame[]) =>
  frames.filter((frame, i) => frame !== frames[i - 1])

/**
 * What every animation frame shows from now until `stop`, from the first
 * that draws a card on, and that first frame's reading. A frame after it
 * with no card is kept too, so the cards going for a frame shows in the
 * runs.
 */
function framesFrom(host: Element) {
  const frames: Frame[] = []
  let first: ReturnType<typeof reading> | undefined
  let sampling = true
  const sample = () => {
    if (!sampling) return
    const frame = frameOf(host)
    if (frames.length > 0 || frame !== 'no cards') {
      if (frames.length === 0) first = reading(host)
      frames.push(frame)
    }
    requestAnimationFrame(sample)
  }
  requestAnimationFrame(sample)
  const stop = () => {
    sampling = false
  }
  beforeUnmount(stop)
  return { frames, first: () => first, stop }
}

/**
 * Library's home view at `/` in a `width` px window until the test ends,
 * and `/away`, a page with no feed consumer, with a visible link to each
 * beside the routes; once the line has its words and the cards are drawn.
 */
async function drawn(width: number) {
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, 844)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/']}>
        <nav>
          <Link to="/away" data-testid="leave">
            Away
          </Link>{' '}
          <Link to="/" data-testid="back">
            Library
          </Link>
        </nav>
        <Routes>
          <Route
            path="/"
            element={
              <StatusContext.Provider value={STATUS}>
                <Library />
              </StatusContext.Provider>
            }
          />
          <Route path="/away" element={<p data-testid="away">Away</p>} />
        </Routes>
      </MemoryRouter>,
    )
  })
  if (host === undefined) throw new Error('nothing mounted')
  const drawnHost = host
  await byHand(() =>
    until(
      'the scan line’s words and the cards',
      () =>
        lineIn(drawnHost)?.textContent === WORDS &&
        firstCardIn(drawnHost) !== null,
    ),
  )
  return drawnHost
}

/**
 * Draws the view on `HASHING`, leaves it by a click, waits out the stale
 * bound, and clicks back with the feed's next answer, `next`, held. Hands
 * back the view, the line and card before leaving, the frames from the
 * return click on, and the held answer's release.
 */
async function leaveAndReturn(width: number, next: ScanFeed) {
  const feed = serveFeed(HASHING)
  const host = await drawn(width)
  expect(reading(host), 'the line before leaving').toMatchObject({
    stale: false,
    visibility: 'visible',
  })
  const before = measured(host)

  await byHand(async () => {
    await clickAt(need(host, '[data-testid="leave"]'))
    await until(
      'the page away',
      () => host.querySelector('[data-testid="away"]') !== null,
    )
  })
  expect(lineIn(host), 'the line, once away').toBeNull()
  await new Promise((resolve) => setTimeout(resolve, 1.5 * SCAN_POLL_MS))

  feed.answer(next)
  const release = feed.hold()
  const polls = feed.polls()
  const frames = framesFrom(host)
  await byHand(async () => {
    await clickAt(need(host, '[data-testid="back"]'))
    await until('the cards, back', () => firstCardIn(host) !== null)
  })
  expect(
    host.querySelector('[data-testid="away"]'),
    'the page away, once back',
  ).toBeNull()
  // The answer drawn at the return is the kept one: the return's poll is
  // the one held.
  expect(feed.polls(), 'polls sent by the return').toBe(polls + 1)
  return { host, before, frames, release }
}

describe('the reader’s scan line across a return by the router, the scan still running (O451)', () => {
  it.each([WIDE, NARROW])(
    'at %i px keeps the line’s room at its height, with the cards where they stood, until the words fill it',
    async (width) => {
      const { host, before, frames, release } = await leaveAndReturn(
        width,
        HASHING,
      )
      const room = {
        stale: true,
        visibility: 'hidden',
        height: expect.closeTo(before.height, 0),
        cardTop: expect.closeTo(before.cardTop, 0),
      }
      expect(frames.first(), 'the return’s first frame with cards').toEqual(
        room,
      )
      expect(reading(host), 'while the answer is held').toEqual(room)

      await byHand(async () => {
        release()
        await until(
          'the mark gone',
          () => lineIn(host)?.hasAttribute('data-stale') === false,
        )
      })
      frames.stop()
      expect(lineIn(host)?.textContent).toBe(WORDS)
      expect(reading(host), 'once the answer has landed').toEqual({
        ...room,
        stale: false,
        visibility: 'visible',
      })
      expect(runsOf(frames.frames)).toEqual(['room', 'words'])
    },
  )
})

describe('the reader’s scan line across a return by the router, the scan ended while away (O451)', () => {
  it.each([WIDE, NARROW])(
    'at %i px keeps the line’s room with no words until the end’s reads land, then closes it',
    async (width) => {
      const { host, before, frames, release } = await leaveAndReturn(width, {
        ended: 1,
        libraries: [],
      })
      const room = {
        stale: true,
        visibility: 'hidden',
        height: expect.closeTo(before.height, 0),
        cardTop: expect.closeTo(before.cardTop, 0),
      }
      // Soft, so a run against a line that draws the kept words reports what
      // it drew while the end's read was held, as well as at the return.
      expect
        .soft(frames.first(), 'the return’s first frame with cards')
        .toEqual(room)

      // The end's read of the works, held: the end is published once it lands.
      const reads = vi.mocked(library.grid).mock.calls.length
      const read = deferred<Grid>()
      vi.mocked(library.grid).mockReturnValueOnce(read.promise)
      await byHand(async () => {
        release()
        await until(
          'the works read again for the end',
          () => vi.mocked(library.grid).mock.calls.length > reads,
        )
      })
      expect
        .soft(reading(host), 'while the end’s read of the works is held')
        .toEqual(room)

      await byHand(async () => {
        read.resolve(works())
        await until('the line gone', () => lineIn(host) === null)
      })
      frames.stop()
      expect(runsOf(frames.frames)).toEqual(['room', 'none'])
      // Closed: the grid moves up into the room.
      expect(reading(host).cardTop, 'the first card’s top').toBeLessThan(
        before.cardTop,
      )
    },
  )
})

describe('the fixture', () => {
  it('wraps the line at 360 px, so it is taller there than at 1280 by more than half its one-line height', async () => {
    serveFeed(HASHING)
    const host = await drawn(WIDE)
    const oneLine = measured(host).height
    await page.viewport(NARROW, 844)
    await byHand(async () => {})
    expect(measured(host).height - oneLine).toBeGreaterThan(oneLine / 2)
  })
})
