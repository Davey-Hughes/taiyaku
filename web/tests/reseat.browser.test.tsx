import { act } from 'react'
import { BrowserRouter, MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import * as aboutApi from '../src/about'
import type { Me } from '../src/api'
import { COVER_LAST } from '../src/browseModule'
import '../src/index.css'
import { Library } from '../src/Library'
import * as library from '../src/library'
import { PHONE_LAYOUT } from '../src/phoneLayout'
import { frameTime, NEAR_PX, SNAP_PX } from '../src/reseatTiming'
import { Series } from '../src/Series'
import { SeriesGrid } from '../src/SeriesGrid'
import { Shell } from '../src/Shell'
import { ABOUT } from './support/about'
import { inProgress, offGrid } from './support/continueRow'
import { BROWSE_KEY, homePage } from './support/homePage'
import {
  afterMotion,
  dragged,
  eachFrame,
  frameBox,
  frames,
  ramp,
  reducedMotion,
  screenTurns,
  scripted,
  scriptedIn,
  scrollerOf,
  wheeledTo,
} from './support/motion'
import { beforeUnmount, mount, rerender } from './support/mount'
import { mouse } from './support/mouse'
import { seriesPage } from './support/seriesPage'

/**
 * Where a cover's glide starts (spec O304: "each cover moves from where it
 * is drawn at that frame"), measured in the frame a column change is laid
 * in, by an observer made after the page's, so after the lists' and the
 * scroller swap's have run and before the frame is painted. A cover is
 * drawn at its tile's box plus the transform its content runs, which the
 * content's client rect is.
 */

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
}))

/** Each listener `reseat.ts` holds on the signals it is told by, until its answer is called. */
const signalled = vi.hoisted(() => new Set<() => void>())

vi.mock('../src/reseatTiming', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../src/reseatTiming')>()
  const held =
    (on: (listener: () => void) => () => void) => (listener: () => void) => {
      const off = on(listener)
      // The scroller swap's, held as long as the shell, is not a list's.
      if (new Error().stack?.includes('/src/reseat.ts') !== true) return off
      const answer = () => {
        signalled.delete(answer)
        off()
      }
      signalled.add(answer)
      return answer
    }
  return {
    ...actual,
    onRowLaid: held(actual.onRowLaid),
    onScrollPutBack: held(actual.onScrollPutBack),
  }
})

const series = (id: number): library.SeriesCard => ({
  id,
  title: `Series ${id}`,
  works: 3,
  editions: 3,
  aligned: 0,
  added: id,
  unread: 3,
  cover: null,
})

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: [],
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: 60 }, (_, i) => series(i + 1)),
    unfiled: [],
  })
})

/** What a cover's glide moves: its tile's content (`reseat.ts`). */
function contentOf(tile: Element): Element {
  const content = tile.firstElementChild
  if (content === null) throw new Error('a tile with nothing in it')
  return content
}

const need = (host: Element, selector: string) => {
  const found = host.querySelector<HTMLElement>(selector)
  if (found === null) throw new Error(`no ${selector}`)
  return found
}

/** The view's top: `<main>`'s beside the sidebar, the phone bar's foot in the phone layout. */
function lineOf(host: HTMLElement): number {
  if (matchMedia(PHONE_LAYOUT).matches) {
    return need(host, '.phone-bar').getBoundingClientRect().bottom
  }
  const main = need(host, 'main')
  return main.getBoundingClientRect().top + main.clientTop
}

/** A frame the home grid was laid at a new size in, as painted. */
interface Laid {
  width: number
  phone: boolean
  scroll: number
  columns: number
  line: number
  covers: DOMRect[]
}

/** The home grid as drawn now. */
function laidNow(host: HTMLElement): Laid {
  const list = need(host, 'main .grid')
  return {
    width: window.innerWidth,
    phone: matchMedia(PHONE_LAYOUT).matches,
    scroll: scrollerOf(host).scrollTop,
    // The grid's own tracks, whatever the cover size.
    columns: getComputedStyle(list).gridTemplateColumns.split(' ').length,
    line: lineOf(host),
    covers: [...list.children].map((li) =>
      contentOf(li).getBoundingClientRect(),
    ),
  }
}

/** Each frame the grid is laid at a new size in, read after the page's observers. */
function watched(host: HTMLElement): Laid[] {
  const seen: Laid[] = []
  const observer = new ResizeObserver(() => {
    seen.push(laidNow(host))
  })
  observer.observe(need(host, 'main .grid'))
  beforeUnmount(() => observer.disconnect())
  return seen
}

/** The first cover whose box reaches below the view's top. */
const topIn = (frame: Laid) =>
  frame.covers.findIndex((cover) => cover.bottom > frame.line + 1)

/** How far `cover` is drawn from `was`, each within a px: 0, or how far off. */
const offBy = (cover: DOMRect, was: DOMRect) =>
  [cover.left - was.left, cover.top - was.top].map((off) =>
    Math.abs(off) <= 1 ? 0 : off,
  )

describe('a glide starts where each cover is drawn', () => {
  it('through the scroll Chromium anchors in the frame the columns change', async () => {
    // Seven 131 px columns across the grid; at 132, six.
    const host = await homePage({ width: 1280, coverSize: 131 })
    await act(async () => {
      await userEvent.click(need(host, '.view-button'))
    })
    await wheeledTo(host, 1200)
    const seen = watched(host)
    await frames(2)
    const before = laidNow(host)
    const range = need(host, '.view-panel input[type="range"]')
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test. The panel is in `<main>`, which
    // focus would scroll to its top.
    await act(async () => range.focus({ preventScroll: true }))
    await act(async () => {
      await userEvent.keyboard('{ArrowRight}')
    })
    await frames(2)
    const changed = seen.find((frame) => frame.columns !== before.columns)
    if (changed === undefined) throw new Error('no frame changed the columns')
    const top = topIn(before)
    // The fixture tells: the columns changed in that frame, and the
    // browser scrolled the page in it to hold what it anchors.
    expect({
      columns: changed.columns !== before.columns,
      anchored: Math.abs(changed.scroll - before.scroll) > 1,
      top: top > 0,
    }).toEqual({ columns: true, anchored: true, top: true })
    expect(offBy(changed.covers[top], before.covers[top])).toEqual([0, 0])
    await afterMotion()
  })

  it('after the scroller swap put the place back, at a column change in the frame after', async () => {
    // A list attached before the shell makes the lists' observer the older
    // one, so in a frame it runs before the swap's, and the put-back moves
    // the lists after they were read: the order the first page drawn has.
    const early = mount(
      <MemoryRouter>
        <SeriesGrid series={[series(1)]} unfiled={[]} />
      </MemoryRouter>,
    )
    early.hidden = true
    // Wider than tall throughout: a window that turns square snaps, as a
    // phone's turn does.
    const host = await homePage({ width: 1000, height: 600, coverSize: 144 })
    await wheeledTo(host, 1200)
    const seen = watched(host)
    await frames(2)
    // Into the phone layout, then a column fewer the next frame.
    let drawn: number[] = []
    await act(async () => {
      drawn = (await dragged([{ width: 880 }, { width: 700 }])).map(
        ({ width }) => width,
      )
      await frames(1)
    })
    const swapped = seen.find((frame) => frame.width === 880)
    const changed = seen.find((frame) => frame.width === 700)
    if (swapped === undefined || changed === undefined) {
      throw new Error('a step drew no frame')
    }
    const top = topIn(swapped)
    // The fixture tells: the first step swapped the scroller and put the
    // place back down the document, which starts at its top, and the
    // second changed the columns in the frame right after; a frame between
    // would read the list again without the put-back's signal.
    expect({
      swapped: swapped.phone && swapped.scroll > 100,
      next: drawn.indexOf(700) === drawn.indexOf(880) + 1,
      columns: changed.columns !== swapped.columns,
      top: top > 0,
    }).toEqual({ swapped: true, next: true, columns: true, top: true })
    expect(offBy(changed.covers[top], swapped.covers[top])).toEqual([0, 0])
    await afterMotion()
  })
})

/** Where a cover is in the view: its top-left corner and its size. */
type Box = { left: number; top: number; width: number; height: number }

/** A cover as one frame drew it. */
interface Seen {
  /** Its client rect, which carries the reseat it runs. */
  drawn: Box
  /** Its layout box: the client rect without that reseat. */
  box: Box
  reseat: Animation | undefined
}

/** One frame, read after the page's observers. */
interface Frame {
  time: number
  width: number
  phone: boolean
  columns: number
  /** How far down the list's page is scrolled. */
  scroll: number
  covers: Seen[]
}

/** How far down the page that draws `list` is scrolled. */
const scrollOf = (list: HTMLElement) =>
  matchMedia(PHONE_LAYOUT).matches
    ? (document.scrollingElement ?? document.documentElement).scrollTop
    : (list.closest('main')?.scrollTop ?? 0)

const boxOf = (rect: DOMRect): Box => ({
  left: rect.left,
  top: rect.top,
  width: rect.width,
  height: rect.height,
})

/** How far apart two boxes are, by their corner or their width. */
const apart = (one: Box, other: Box) =>
  Math.max(
    Math.abs(one.left - other.left),
    Math.abs(one.top - other.top),
    Math.abs(one.width - other.width),
  )

/** The cover's reseat, on the element it moves. */
const reseatOf = (tile: Element) =>
  contentOf(tile).getAnimations().find(scripted)

/** A transform's matrix, `none` as the identity. */
const matrixOf = (transform: string) =>
  new DOMMatrixReadOnly(transform === 'none' ? undefined : transform)

function coversOf(list: HTMLElement): Seen[] {
  return [...list.children].map((tile) => {
    const content = contentOf(tile)
    const rect = content.getBoundingClientRect()
    // Scaled about the content's top-left corner, so the corner moves by
    // the translation alone and the width by the scale.
    const m = matrixOf(getComputedStyle(content).transform)
    return {
      drawn: boxOf(rect),
      box: {
        left: rect.left - m.e,
        top: rect.top - m.f,
        width: rect.width / m.a,
        height: rect.height / m.d,
      },
      reseat: reseatOf(tile),
    }
  })
}

const columnsOfList = (list: HTMLElement) =>
  getComputedStyle(list).gridTemplateColumns.split(' ').length

/**
 * Every frame from here to the test's end, as drawn (`eachFrame`), and
 * each reseat's start time, kept as it is first known.
 */
function recorded(list: HTMLElement) {
  const frames: Frame[] = []
  const starts = new Map<Animation, number>()
  eachFrame(() => {
    const covers = coversOf(list)
    for (const { reseat } of covers) {
      if (reseat?.startTime != null)
        starts.set(reseat, Number(reseat.startTime))
    }
    const frame = {
      time: frameTime(),
      width: window.innerWidth,
      phone: matchMedia(PHONE_LAYOUT).matches,
      columns: columnsOfList(list),
      scroll: scrollOf(list),
      covers,
    }
    if (frames.at(-1)?.time === frame.time) frames[frames.length - 1] = frame
    else frames.push(frame)
  })
  return { frames, starts }
}

type Recording = ReturnType<typeof recorded>

/** The frames the columns changed in. */
const changesIn = (frames: Frame[]) =>
  frames.flatMap((frame, at) =>
    at > 0 && frame.columns !== frames[at - 1].columns ? [at] : [],
  )

/**
 * Where `seen`'s cover is drawn at `time`: its box under its reseat as the
 * browser interpolates it then, replayed on a probe held at that time.
 */
function drawnAt(
  seen: Pick<Seen, 'box' | 'reseat'>,
  time: number,
  starts: Map<Animation, number>,
): Box {
  const { box, reseat } = seen
  if (reseat === undefined) return box
  const start = starts.get(reseat)
  if (start === undefined) throw new Error('a reseat that never started')
  const effect = reseat.effect as KeyframeEffect
  const probe = document.createElement('div')
  probe.style.cssText = 'position: fixed; visibility: hidden'
  document.body.append(probe)
  const replay = probe.animate(effect.getKeyframes(), effect.getTiming())
  replay.pause()
  replay.currentTime = time - start
  const m = matrixOf(getComputedStyle(probe).transform)
  replay.cancel()
  probe.remove()
  return {
    left: box.left + m.e,
    top: box.top + m.f,
    width: box.width * m.a,
    height: box.height * m.d,
  }
}

/**
 * Each cover on screen in the frame `at`, which changed the columns: how
 * far it starts from where the frame before drew it, both taken at the
 * time its new reseat starts (spec O304, as the glide report measured it).
 * `glides` where it starts one, `late` by how much that one starts after
 * the frame's time; `gliding` where it ran one the frame before.
 */
function startsOff({ frames, starts }: Recording, at: number) {
  const now = frames[at]
  const was = frames[at - 1]
  return now.covers.flatMap((cover, index) => {
    const before = was.covers[index]
    const { top, height } = before?.drawn ?? { top: 0, height: 0 }
    if (before === undefined || top >= innerHeight || top + height <= 0) {
      return []
    }
    const fresh = cover.reseat === before.reseat ? undefined : cover.reseat
    const time = fresh === undefined ? now.time : starts.get(fresh)
    if (time === undefined) throw new Error('a reseat that never started')
    const start =
      fresh === undefined ? cover.drawn : drawnAt(cover, time, starts)
    return [
      {
        cover: index,
        off: apart(start, drawnAt(before, time, starts)),
        late: time - now.time,
        glides: fresh !== undefined,
        gliding: before.reseat !== undefined,
      },
    ]
  })
}

/**
 * S1's bound at each change `record` saw: each reseat starts at the time
 * of the frame it starts in, as `frameTime()` gives it (one the browser
 * picks lands a frame late at times, and the cover then stalls a frame);
 * a cover that glides starts within 0.5 px of where it was drawn, and one
 * on screen that snaps moved under `SNAP_PX`, as the spec lets it; and
 * some cover glides.
 */
function expectStartsWhereDrawn(record: Recording, changes: number[]) {
  for (const at of changes) {
    const offs = startsOff(record, at)
    // Equal but for the float rounding a time written to `startTime` and
    // read back carries; a time the browser picks is off by orders of
    // magnitude more.
    expect(
      offs.flatMap(({ cover, late, glides }) =>
        glides && Math.abs(late) > 1e-6 ? [{ cover, late }] : [],
      ),
    ).toEqual([])
    expect(
      offs.flatMap(({ cover, off, glides }) =>
        (glides ? off > 0.5 : off >= SNAP_PX) ? [{ cover, off, glides }] : [],
      ),
    ).toEqual([])
    expect(offs.some((each) => each.glides)).toBe(true)
  }
}

/**
 * Each reseat started in the frame `at`, where it draws its cover as it
 * ends: within 0.5 px of the box `boxes` gives that cover, or how far off.
 */
function landings({ frames, starts }: Recording, at: number, boxes: Box[]) {
  return frames[at].covers.flatMap((cover, index) => {
    const { reseat } = cover
    if (
      reseat === undefined ||
      reseat === frames[at - 1].covers[index]?.reseat
    ) {
      return []
    }
    const start = starts.get(reseat) ?? Number.NaN
    const end = start + Number(reseat.effect?.getTiming().duration) - 0.01
    const off = apart(
      drawnAt({ box: boxes[index], reseat }, end, starts),
      boxes[index],
    )
    return off > 0.5 ? [{ cover: index, off }] : []
  })
}

/** A list as it rests, per cover: what runs in its tile, where its content is drawn. */
function restOf(list: HTMLElement) {
  const tiles = [...list.children]
  return {
    reseating: list.hasAttribute('data-reseating'),
    animations: tiles.map(scriptedIn),
    transforms: tiles.map(
      (tile) => getComputedStyle(contentOf(tile)).transform,
    ),
    boxes: tiles.map((tile) => boxOf(contentOf(tile).getBoundingClientRect())),
  }
}

/**
 * S2's end: nothing running, carried or marked, and each cover where the
 * same drive leaves it without the glide (`still`), within 0.5 px.
 */
function expectEndsClean(rest: ReturnType<typeof restOf>, still: Box[]) {
  expect(rest.reseating).toBe(false)
  expect(rest.animations).toEqual(rest.animations.map(() => 0))
  expect(rest.transforms).toEqual(rest.transforms.map(() => 'none'))
  expect(rest.boxes.length).toBe(still.length)
  expect(
    rest.boxes.flatMap((box, cover) => {
      const off = apart(box, still[cover])
      return off > 0.5 ? [{ cover, off }] : []
    }),
  ).toEqual([])
}

/** A page drawn, and its list of covers. */
interface Drawn {
  host: HTMLElement
  list: HTMLElement
}

/**
 * Each cover's box once `drive` has run on a page `open` draws afresh
 * under reduced motion, where nothing glides.
 */
async function stillAfter(
  open: () => Promise<Drawn>,
  drive: (drawn: Drawn) => Promise<void>,
): Promise<Box[]> {
  onTestFinished(async () => {
    await reducedMotion(false)
  })
  await reducedMotion(true)
  const drawn = await open()
  await drive(drawn)
  await afterMotion()
  const { boxes } = restOf(drawn.list)
  await reducedMotion(false)
  return boxes
}

/** Runs `input` with React scheduling its work as the app does, not held for `act`. */
async function outsideAct<T>(input: () => Promise<T>): Promise<T> {
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  try {
    return await input()
  } finally {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  }
}

const ADMIN: Me = {
  user_id: 1,
  admin: true,
  can_download: false,
  can_stream: false,
  can_stamp: false,
}

/** Any series `oneSeries` is asked for, with `count` volumes. */
function serveVolumes(count = 60) {
  vi.mocked(library.oneSeries).mockResolvedValue({
    series: { ...series(7), titles: [] },
    works: Array.from({ length: count }, (_, n) => ({
      id: n + 1,
      slug: `v${n + 1}`,
      title: `Volume ${n + 1}`,
      volume: n + 1,
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
    })),
  })
}

/**
 * The pages that draw covers, at 144 px covers, each with the window widths
 * its columns change at: one more column at `edges[0]` and again at
 * `edges[1]`, its list's own 60 covers.
 */
const PAGES: Record<
  string,
  { edges: [number, number]; open: (width: number) => Promise<Drawn> }
> = {
  'the home grid': {
    edges: [1215, 1375],
    open: async (width) => {
      const host = await homePage({ width, coverSize: 144 })
      return { host, list: need(host, 'main .grid') }
    },
  },
  'a series page': {
    edges: [1231, 1391],
    open: async (width) => {
      serveVolumes()
      const host = await seriesPage({
        width,
        panel: 'off',
        coverSize: 144,
        who: ADMIN,
      })
      host.style.height = '100%'
      await expect
        .poll(() => host.querySelector('.volumes > li'))
        .not.toBeNull()
      return { host, list: need(host, '.volumes') }
    },
  },
}

/**
 * The window drags each case makes, a width a frame in each leg, the
 * covers let come to rest between legs; each changes the columns twice.
 * `interrupted` where the second change comes while the first glides.
 * `scroll`, how far down the page each starts: far enough that the cover
 * at the view's top is laid on another row at the second change, so the
 * browser must scroll the page to hold it.
 */
const DRIVES: Record<
  string,
  (edges: [number, number]) => {
    legs: number[][]
    interrupted: boolean
    scroll: number
  }
> = {
  'one column each way': ([edge]) => ({
    legs: [
      [edge - 10, edge - 6, edge - 2, edge + 2, edge + 6, edge + 10],
      [edge + 6, edge + 2, edge - 2, edge - 6, edge - 10],
    ],
    interrupted: false,
    scroll: 600,
  }),
  'a reversal 3 frames into a glide': ([edge]) => ({
    legs: [[edge - 8, edge - 4, edge + 2, edge + 4, edge + 6, edge - 2]],
    interrupted: true,
    scroll: 600,
  }),
  'a second change 4 frames into a glide': ([edge, next]) => ({
    legs: [
      [edge - 8, edge - 4, edge + 2, edge + 4, edge + 6, edge + 8, next + 2],
    ],
    interrupted: true,
    scroll: 1600,
  }),
}

/**
 * Whether change `at` lays the cover the browser anchors to on another
 * row, which it then scrolls the page to hold: the first cover in view, the
 * view's top at `line`, when the drive began, which it keeps through the
 * scrolls it makes itself.
 */
function movesRow({ frames }: Recording, at: number, line: number) {
  const [first] = frames
  const top = first.covers.findIndex(({ box }) => box.top + box.height > line)
  const rowIn = (columns: number) => Math.floor(top / columns)
  return rowIn(frames[at - 1].columns) !== rowIn(frames[at].columns)
}

/** Drags the window through `legs`, letting the covers rest between them. */
async function dragging(legs: number[][]) {
  for (const [at, leg] of legs.entries()) {
    if (at > 0) await afterMotion()
    await act(async () => {
      await dragged(leg.map((width) => ({ width })))
    })
  }
}

describe.each(Object.entries(PAGES))('on %s', (_, { edges, open }) => {
  it.each(Object.entries(DRIVES))(
    '%s: each cover starts where it was drawn, and the glide ends clean',
    async (_, drive) => {
      const { legs, interrupted, scroll } = drive(edges)
      const scrolled: boolean[] = []
      const opened = async () => {
        const drawn = await open(legs[0][0] - 2)
        await wheeledTo(drawn.host, scroll)
        scrolled.push(scrollerOf(drawn.host).scrollTop >= scroll - 1)
        return drawn
      }
      const glided = await opened()
      const record = recorded(glided.list)
      await frames(2)
      await dragging(legs)
      await afterMotion()
      const changes = changesIn(record.frames)
      // The fixture tells: scrolled down, two changes, the second mid-glide
      // where asked and needing the browser to scroll the page.
      expect({
        scrolled,
        changes: changes.length,
        interrupted: startsOff(record, changes[1]).some(
          (each) => each.gliding && each.glides,
        ),
        moved: movesRow(record, changes[1], lineOf(glided.host)),
      }).toEqual({ scrolled: [true], changes: 2, interrupted, moved: true })
      // The browser scrolls the page in the frame the second change lands,
      // the frame S1 reads where each cover starts.
      const [before, at] = record.frames.slice(changes[1] - 1)
      expect(Math.abs(at.scroll - before.scroll)).toBeGreaterThan(1)
      expectStartsWhereDrawn(record, changes)
      const rest = restOf(glided.list)
      expect(landings(record, changes[1], rest.boxes)).toEqual([])
      glided.host.hidden = true
      const still = await stillAfter(opened, () => dragging(legs))
      expect(scrolled).toEqual([true, true])
      expectEndsClean(rest, still)
    },
  )
})

describe('a glide ends clean', () => {
  it('after a chip filter changes the list mid-glide', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: Array.from({ length: 60 }, (_, i) => ({
        ...series(i + 1),
        editions: i % 3 === 0 ? 4 : 3,
      })),
      unfiled: [],
    })
    const [edge] = PAGES['the home grid'].edges
    const opened = () => PAGES['the home grid'].open(edge - 12)
    const pressedMoving: boolean[] = []
    const drive = async ({ list }: Drawn) => {
      await dragging([[edge - 10, edge - 6, edge - 2, edge + 2]])
      const chip = page.getByRole('button', { name: 'Multi-edition' })
      chip.element().addEventListener(
        'click',
        () => {
          pressedMoving.push([...list.children].some((tile) => reseatOf(tile)))
        },
        { once: true },
      )
      await outsideAct(async () => {
        await userEvent.click(chip)
        await expect.poll(() => list.children.length).toBe(20)
        // The chip writes the address (`Library.tsx`'s `choose`), and the
        // location's change runs effects that set state: `usePopover`'s
        // close in AccountMenu and ViewMenu, Shell's menu close, SearchBar's
        // text sync. They land here, outside `act` as in the app; once
        // `act`'s flag is back, each would warn.
        await frames(2)
      })
    }
    const glided = await opened()
    await drive(glided)
    await afterMotion()
    const rest = restOf(glided.list)
    glided.host.hidden = true
    const still = await stillAfter(opened, drive)
    // The fixture tells: the chip was pressed while covers glided, and
    // pressed under reduced motion while none did.
    expect(pressedMoving).toEqual([true, false])
    expectEndsClean(rest, still)
  })
})

/** The View panel open, by a real click, which focuses the page too; and its slider. */
async function slider() {
  await act(async () => {
    await userEvent.click(page.getByRole('button', { name: 'View' }))
  })
  return page.getByRole('slider', { name: /cover size/i }).element()
}

describe('a glide on the Cover size slider', () => {
  it('glides on a Cover size slider step that adds a column, and ends clean', async () => {
    // Six 132 px columns across the grid at 1280 px; at 131, seven.
    const opened = async () => {
      const host = await homePage({ width: 1280, coverSize: 132 })
      return { host, list: need(host, 'main .grid') }
    }
    const glides: boolean[] = []
    const drive = async ({ list }: Drawn) => {
      ;(await slider()).focus()
      await act(async () => {
        await userEvent.keyboard('{ArrowLeft}')
      })
      await frames(2)
      glides.push([...list.children].some((tile) => reseatOf(tile)))
    }
    const glided = await opened()
    await drive(glided)
    await afterMotion()
    const rest = restOf(glided.list)
    glided.host.hidden = true
    const still = await stillAfter(opened, drive)
    expect(glides).toEqual([true, false])
    expectEndsClean(rest, still)
  })

  it('starts where drawn through a mouse drag along the slider, across two column changes', async () => {
    // Seven columns at 124 px covers and 1280 px, six from 132, five from 158.
    const opened = async () => {
      const host = await homePage({ width: 1280, coverSize: 124 })
      return { host, list: need(host, 'main .grid') }
    }
    const drive = async () => {
      const range = (await slider()).getBoundingClientRect()
      const at = (position: number) =>
        range.left + (range.width * position) / COVER_LAST
      const y = range.top + range.height / 2
      await outsideAct(async () => {
        await mouse.moveTo(at(28), y)
        await mouse.press()
        for (let x = at(28); x <= at(60); x += 2) {
          await mouse.moveTo(x, y)
          await frames(1)
        }
        await mouse.release()
      })
    }
    const glided = await opened()
    const record = recorded(glided.list)
    await drive()
    await afterMotion()
    const changes = changesIn(record.frames)
    expect(changes.length).toBe(2)
    expectStartsWhereDrawn(record, changes)
    const rest = restOf(glided.list)
    expect(landings(record, changes[1], rest.boxes)).toEqual([])
    glided.host.hidden = true
    expectEndsClean(rest, await stillAfter(opened, drive))
  })
})

/**
 * The window listeners `reseat.ts` holds, by type: told by the stack each
 * is added from, and dropped when removed.
 */
function reseatWindowListeners(): () => string[] {
  type Held = {
    type: string
    listener: EventListenerOrEventListenerObject
    capture: boolean
  }
  const held: Held[] = []
  const add = window.addEventListener.bind(window)
  const remove = window.removeEventListener.bind(window)
  const captureOf = (options?: boolean | EventListenerOptions) =>
    typeof options === 'boolean' ? options : options?.capture === true
  const adding = vi.spyOn(window, 'addEventListener').mockImplementation(((
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: boolean | AddEventListenerOptions,
  ) => {
    if (new Error().stack?.includes('/src/reseat.ts') === true) {
      held.push({ type, listener, capture: captureOf(options) })
    }
    add(type, listener, options)
  }) as typeof window.addEventListener)
  const removing = vi.spyOn(window, 'removeEventListener').mockImplementation(((
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: boolean | EventListenerOptions,
  ) => {
    const at = held.findIndex(
      (each) =>
        each.type === type &&
        each.listener === listener &&
        each.capture === captureOf(options),
    )
    if (at >= 0) held.splice(at, 1)
    remove(type, listener, options)
  }) as typeof window.removeEventListener)
  onTestFinished(() => {
    adding.mockRestore()
    removing.mockRestore()
  })
  return () => held.map((each) => each.type).sort()
}

/**
 * The app's home and series routes inside the shell, under `BrowserRouter`
 * at `/` in this frame's own history, put back when the test ends, at 144
 * px covers, in a window `width` wide.
 */
async function app(width: number): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
  })
  history.replaceState(null, '', '/')
  onTestFinished(() => localStorage.removeItem(BROWSE_KEY))
  localStorage.setItem(BROWSE_KEY, JSON.stringify({ coverSize: 144 }))
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, 844)
  const host = mount(
    <BrowserRouter>
      <Routes>
        <Route
          path="/"
          element={
            <Shell>
              <Library />
            </Shell>
          }
        />
        <Route
          path="/series/:id"
          element={
            <Shell>
              <Series />
            </Shell>
          }
        />
      </Routes>
    </BrowserRouter>,
  )
  // The height chain `base.css` gives `#root`, as `homePage` says.
  host.style.height = '100%'
  // The search bar's debounce, as `homePage` waits it out.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
  })
  return host
}

describe('a glide and the pages around it', () => {
  it('follows a click on a cover mid-glide, and Back', async () => {
    serveVolumes()
    const listening = reseatWindowListeners()
    const [edge] = PAGES['the home grid'].edges
    const [volumesEdge] = PAGES['a series page'].edges
    const host = await app(edge - 8)
    const grid = need(host, 'main .grid')
    await act(async () => {
      await dragged([{ width: edge - 4 }, { width: edge + 2 }])
    })
    // A cover on screen that glides, a short way, so it stays under the
    // pointer from its press to its release.
    const tile = [...grid.children].find((each) => {
      const reseat = reseatOf(each)
      const [from] =
        (reseat?.effect as KeyframeEffect | undefined)?.getKeyframes() ?? []
      const { e, f } = matrixOf(String(from?.transform ?? 'none'))
      const { top, bottom } = contentOf(each).getBoundingClientRect()
      return reseat && top > 0 && bottom < innerHeight && Math.hypot(e, f) < 40
    })
    if (tile === undefined) throw new Error('no cover glides a short way')
    const title = tile.querySelector('.title')?.textContent ?? ''
    const link = page
      .getByRole('link', { name: new RegExp(`\\b${title}\\b`) })
      .element()
    const pressedMoving: boolean[] = []
    link.addEventListener('click', () => {
      pressedMoving.push(reseatOf(tile) !== undefined)
    })
    const { left, top, width, height } = link.getBoundingClientRect()
    await outsideAct(async () => {
      await mouse.moveTo(left + width / 2, top + height / 2)
      await mouse.press()
      await mouse.release()
      await expect
        .poll(() => host.querySelector('.volumes > li'))
        .not.toBeNull()
    })
    // A frame in which the list's observer could have started a glide.
    await frames(2)
    const volumes = need(host, '.volumes')
    expect({
      pressedMoving,
      series: location.pathname.startsWith('/series/'),
      animations: [...volumes.children].map(scriptedIn),
    }).toEqual({
      pressedMoving: [true],
      series: true,
      animations: [...volumes.children].map(() => 0),
    })

    // Scrolled first, so its covers start from where the scroll left them.
    await outsideAct(() => wheeledTo(host, 300))
    const record = recorded(volumes)
    await frames(2)
    const drawnListening = { window: listening(), signals: signalled.size }
    await act(async () => {
      await dragged([{ width: volumesEdge + 2 }])
    })
    await frames(1)
    const changes = changesIn(record.frames)
    expect(changes.length).toBe(1)
    expectStartsWhereDrawn(record, changes)
    await afterMotion()

    // The table: a page with no cover list.
    await outsideAct(async () => {
      await userEvent.click(page.getByRole('button', { name: 'View' }))
      await userEvent.selectOptions(
        page.getByRole('combobox', { name: /layout/i }),
        'table',
      )
      await expect.poll(() => host.querySelector('.volumes')).toBeNull()
    })
    expect({
      drawn: drawnListening,
      gone: { window: listening(), signals: signalled.size },
    }).toEqual({
      drawn: { window: ['resize', 'scroll'], signals: 2 },
      gone: { window: [], signals: 0 },
    })

    await outsideAct(async () => {
      const popped = new Promise((resolve) => {
        window.addEventListener('popstate', resolve, { once: true })
      })
      history.back()
      await popped
      await expect
        .poll(() => host.querySelector('main .grid > li'))
        .not.toBeNull()
    })
    const again = recorded(need(host, 'main .grid'))
    await frames(2)
    await act(async () => {
      await dragged([{ width: edge - 2 }])
    })
    await frames(1)
    const changed = changesIn(again.frames)
    expect(changed.length).toBe(1)
    expect(startsOff(again, changed[0]).some((each) => each.glides)).toBe(true)
    await afterMotion()
  })
})

describe('a glide and React', () => {
  it('renders nothing through a glide', async () => {
    const onCommit = vi.fn()
    const [edge] = PAGES['the home grid'].edges
    const host = await homePage({ width: edge - 8, coverSize: 144, onCommit })
    const record = recorded(need(host, 'main .grid'))
    await frames(2)
    onCommit.mockClear()
    await dragging([[edge - 4, edge + 2, edge + 6]])
    await afterMotion()
    const changes = changesIn(record.frames)
    expect({
      glided:
        changes.length === 1 &&
        startsOff(record, changes[0]).some((each) => each.glides),
      commits: onCommit.mock.calls.length,
    }).toEqual({ glided: true, commits: 0 })
  })
})

/** The frames of `frames` in which some cover glides. */
const glidingIn = (frames: Frame[]) =>
  frames.flatMap((frame, at) =>
    frame.covers.some(({ reseat }) => reseat !== undefined) ? [at] : [],
  )

/** The home grid's series, a third of them with more editions than works. */
const someMulti = () =>
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: 60 }, (_, i) => ({
      ...series(i + 1),
      editions: i % 3 === 0 ? 4 : 3,
    })),
    unfiled: [],
  })

describe('what snaps', () => {
  it("snaps across the phone layout's edge by width", async () => {
    const host = await homePage({ width: 924, coverSize: 144 })
    const record = recorded(need(host, 'main .grid'))
    await frames(2)
    await act(async () => {
      await dragged(ramp(924, 876, 12).map((width) => ({ width })))
    })
    await afterMotion()
    const { frames: seen } = record
    expect({
      phone: [seen[0].phone, seen.at(-1)?.phone],
      gliding: glidingIn(seen),
    }).toEqual({ phone: [false, true], gliding: [] })
  })

  it('snaps across it by height where the column count holds', async () => {
    // 240 px covers lay as many columns at 1280 × 800 as at 1280 × 480.
    const host = await homePage({ width: 1280, height: 800, coverSize: 240 })
    const record = recorded(need(host, 'main .grid'))
    await frames(2)
    await act(async () => {
      await dragged(ramp(800, 480, 8).map((height) => ({ height })))
    })
    await afterMotion()
    const { frames: seen } = record
    expect({
      phone: [seen[0].phone, seen.at(-1)?.phone],
      columns: [...new Set(seen.map(({ columns }) => columns))].length,
      gliding: glidingIn(seen),
    }).toEqual({ phone: [false, true], columns: 1, gliding: [] })
  })

  it('snaps a cover beyond 200 px of the view before and after, and glides one on screen or within 200 px', async () => {
    const [edge] = PAGES['the home grid'].edges
    const host = await homePage({ width: edge - 8, coverSize: 144 })
    const record = recorded(need(host, 'main .grid'))
    await frames(2)
    await act(async () => {
      await dragged([{ width: edge - 4 }, { width: edge + 2 }])
    })
    await frames(1)
    const [at] = changesIn(record.frames)
    const was = record.frames[at - 1].covers
    const now = record.frames[at].covers
    /** Whether `box` reaches within `margin` px of the view. */
    const within = ({ top, height }: Box, margin: number) =>
      top + height > -margin && top < innerHeight + margin
    const far = now.flatMap(({ box }, index) =>
      within(was[index].box, NEAR_PX + 1) || within(box, NEAR_PX + 1)
        ? []
        : [index],
    )
    const moved = now.flatMap(({ box }, index) =>
      apart(was[index].box, box) >= SNAP_PX + 0.5 ? [index] : [],
    )
    const seen = moved.filter((index) => within(was[index].box, 0))
    const margin = moved.filter(
      (index) =>
        !within(was[index].box, 0) &&
        !within(now[index].box, 0) &&
        (within(was[index].box, NEAR_PX - 1) ||
          within(now[index].box, NEAR_PX - 1)),
    )
    const glides = (index: number) => now[index].reseat !== undefined
    expect({
      found: [far.length > 0, seen.length > 0, margin.length > 0],
      far: far.filter(glides),
      seen: seen.filter((index) => !glides(index)),
      margin: margin.filter((index) => !glides(index)),
    }).toEqual({
      found: [true, true, true],
      far: [],
      seen: [],
      margin: [],
    })
    await afterMotion()
  })

  it('snaps every change under reduced motion, through a drag over several column counts', async () => {
    onTestFinished(async () => {
      await reducedMotion(false)
    })
    await reducedMotion(true)
    const [edge, next] = PAGES['the home grid'].edges
    const host = await homePage({ width: edge - 8, coverSize: 144 })
    const record = recorded(need(host, 'main .grid'))
    await frames(2)
    await act(async () => {
      await dragged(ramp(edge - 8, next + 8, 30).map((width) => ({ width })))
    })
    await afterMotion()
    expect({
      changes: changesIn(record.frames).length,
      gliding: glidingIn(record.frames),
    }).toEqual({ changes: 2, gliding: [] })
  })

  it.each([
    ['alone, pressed while the covers glide', 0],
    ['landing in the same frame as a column change', 2],
  ])(
    'snaps when the number of covers changes: a chip filter %s',
    async (_, past) => {
      someMulti()
      const [edge] = PAGES['the home grid'].edges
      const host = await homePage({ width: edge - 8, coverSize: 144 })
      const list = need(host, 'main .grid')
      const record = recorded(list)
      await frames(2)
      if (past === 0) {
        // A glide under way: a count change alone moves no cover laid by
        // index, so only a reseat it leaves running can show it unsnapped.
        await act(async () => {
          await dragged([{ width: edge - 4 }, { width: edge + 2 }])
        })
      } else {
        // Sized as React draws the filtered list, before the next frame,
        // which then lays both.
        const filtered = new MutationObserver(() => {
          filtered.disconnect()
          frameBox().style.width = `${edge + past}px`
        })
        filtered.observe(list, { childList: true })
        onTestFinished(() => filtered.disconnect())
      }
      await outsideAct(async () => {
        await userEvent.click(
          page.getByRole('button', { name: 'Multi-edition' }),
        )
        await expect.poll(() => list.children.length).toBe(20)
        // The location's change sets state in effects, as the chip case above says.
        await frames(2)
      })
      await afterMotion()
      const { frames: seen } = record
      const at = seen.findIndex(({ covers }) => covers.length === 20)
      // The fixture tells: the columns changed with the count or held, and
      // a glide ran into the change where asked. From the frame the count
      // changes in, no cover glides, and each is drawn on its box.
      expect({
        columns: seen[at].columns !== seen[at - 1].columns,
        before: glidingIn(seen.slice(0, at)).includes(at - 1),
        gliding: glidingIn(seen.slice(at)),
        off: seen[at].covers.flatMap(({ drawn, box }, cover) => {
          const off = apart(drawn, box)
          return off > 0.5 ? [{ cover, off }] : []
        }),
      }).toEqual({
        columns: past > 0,
        before: past === 0,
        gliding: [],
        off: [],
      })
    },
  )
})

/**
 * The home page on a phone held upright, 440 × 956, its screen turned to
 * match, with a Continue row of 12 cards over the grid. At 121 px covers
 * the grid lays 3 columns upright and 6 on its side, and 7 there at 120.
 */
async function upright() {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(12),
    added: [],
    read: [],
  })
  const host = await homePage({ width: 440, height: 956, coverSize: 121 })
  const turn = await screenTurns()
  await turn('portrait-primary')
  await expect
    .poll(() => host.querySelectorAll('.continue ul > li').length)
    .toBe(12)
  await document.fonts.ready
  return { host, list: need(host, 'main .grid'), turn }
}

/**
 * Turns the phone on its side: the screen the page reports, and the frame's
 * box to 956 × 440, changed in one task.
 */
async function sideways(turn: Awaited<ReturnType<typeof upright>>['turn']) {
  const box = frameBox()
  await act(async () => {
    const turning = turn('landscape-primary')
    box.style.width = '956px'
    box.style.height = '440px'
    await turning
  })
}

/**
 * Each frame from here as drawn (`eachFrame`): the view and the screen,
 * the grid's columns, how many of its covers glide, where the Continue
 * row's cards miss the grid's columns (`offGrid`), and the row's top.
 */
function turnFrames(host: HTMLElement, list: HTMLElement) {
  const row = need(host, '.continue ul')
  const seen: {
    time: number
    width: number
    height: number
    upright: boolean
    screen: OrientationType
    phone: boolean
    columns: number
    gliding: number
    off: string[]
    row: number
  }[] = []
  eachFrame(() => {
    const frame = {
      time: frameTime(),
      width: innerWidth,
      height: innerHeight,
      upright: matchMedia('(orientation: portrait)').matches,
      screen: screen.orientation.type,
      phone: matchMedia(PHONE_LAYOUT).matches,
      columns: columnsOfList(list),
      gliding: [...list.children].filter((tile) => scriptedIn(tile) > 0).length,
      off: offGrid(host),
      row: row.getBoundingClientRect().top,
    }
    if (seen.at(-1)?.time === frame.time) seen[seen.length - 1] = frame
    else seen.push(frame)
  })
  return seen
}

describe('a phone turn', () => {
  it('snaps the covers and the Continue row on a turn', async () => {
    const { host, list, turn } = await upright()
    const seen = turnFrames(host, list)
    await frames(2)
    await sideways(turn)
    await afterMotion()
    const at = seen.findIndex(({ width }) => width === 956)
    if (at < 1) {
      const sizes = seen.map(({ width, height }) => `${width} × ${height}`)
      throw new Error(`no frame upright then on its side: ${sizes.join(', ')}`)
    }
    const before = seen[at - 1]
    const last = seen[seen.length - 1]
    const after = seen.slice(at, at + 6)
    // The fixture tells: the frame saw both the screen and its own view
    // turn, stayed in the phone layout, and laid other columns at the turn;
    // and the row stood on screen, where it moves with the covers.
    expect({
      upright: [before.upright, before.screen, last.upright, last.screen],
      phone: seen.every(({ phone }) => phone),
      columns: [before.columns, after[0].columns],
      row: before.row < before.height,
      frames: after.length,
      gliding: after.map(({ gliding }) => gliding),
      off: after.flatMap(({ off }) => off),
    }).toEqual({
      upright: [true, 'portrait-primary', false, 'landscape-primary'],
      phone: true,
      columns: [3, 6],
      row: true,
      frames: 6,
      gliding: [0, 0, 0, 0, 0, 0],
      off: [],
    })
  })

  it('glides a column change that is not a turn', async () => {
    const { host, list, turn } = await upright()
    await sideways(turn)
    await afterMotion()
    ;(await slider()).focus()
    const seen = turnFrames(host, list)
    await frames(2)
    await act(async () => {
      await userEvent.keyboard('{ArrowLeft}')
    })
    await afterMotion()
    const [first] = seen
    const last = seen[seen.length - 1]
    // The fixture tells: the phone lay on its side throughout, and the
    // step added a column.
    expect({
      sideways: [first.upright, first.screen],
      sizes: [
        ...new Set(seen.map(({ width, height }) => `${width} × ${height}`)),
      ],
      columns: [first.columns, last.columns],
      glided: seen.some(({ gliding }) => gliding > 0),
      slid: seen.some(({ off }) => off.length > 0),
    }).toEqual({
      sideways: [false, 'landscape-primary'],
      sizes: ['956 × 440'],
      columns: [6, 7],
      glided: true,
      slid: true,
    })
  })
})

/**
 * Counts each box read and each style read from here until the answer is
 * called, by the frame it is made in; only those whose stack names `from`,
 * where given. The answer gives each frame's count.
 */
function readsByFrame(from?: string): () => number[] {
  const reads = new Map<number, number>()
  const count = () => {
    if (from !== undefined && new Error().stack?.includes(from) !== true) {
      return
    }
    const time = frameTime()
    reads.set(time, (reads.get(time) ?? 0) + 1)
  }
  const rect = Element.prototype.getBoundingClientRect
  const style = window.getComputedStyle
  const rects = vi
    .spyOn(Element.prototype, 'getBoundingClientRect')
    .mockImplementation(function (this: Element) {
      count()
      return rect.call(this)
    })
  const styles = vi
    .spyOn(window, 'getComputedStyle')
    .mockImplementation((el: Element, pseudo?: string | null) => {
      count()
      return style.call(window, el, pseudo)
    })
  let on = true
  const stop = () => {
    if (!on) return
    on = false
    rects.mockRestore()
    styles.mockRestore()
  }
  onTestFinished(stop)
  return () => {
    stop()
    return [...reads.values()]
  }
}

/**
 * Unmounts the page `host` draws, the newest mounted, and takes its host
 * out: a shell left drawn reads which element scrolls at each resize, as
 * the next page's does, and an empty host, the window's height, would push
 * the next page down past the view.
 */
function gone(host: HTMLElement) {
  rerender(null)
  host.remove()
}

/** The home grid with `count` covers at 144 px, in a window `width` wide. */
async function gridOf(count: number, width: number): Promise<HTMLElement> {
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: count }, (_, i) => series(i + 1)),
    unfiled: [],
  })
  const host = await homePage({ width, coverSize: 144 })
  await expect.poll(() => need(host, 'main .grid').children.length).toBe(count)
  return host
}

/**
 * The window width, at 144 px covers, under which the home grid lays a
 * column fewer: the nearest such above the phone layout's edge.
 */
const ABOVE_PHONE = 1055

/**
 * The window width, at 144 px covers, from which the home grid lays a
 * column more in the phone layout, where covers fill their columns: a
 * change there scales each cover.
 */
const PHONE_COLUMN = 816

describe('what a frame reads', () => {
  const [edge, next] = PAGES['the home grid'].edges
  it.each([
    [
      'beside the sidebar, with no column change',
      edge + 4,
      ramp(edge + 4, next - 4, 60),
      false,
    ],
    [
      "in the phone layout, with no column change, the list's style read each frame",
      PHONE_COLUMN - 124,
      ramp(PHONE_COLUMN - 124, PHONE_COLUMN - 4, 60),
      false,
    ],
    ['across a column change', edge - 8, ramp(edge - 8, edge + 8, 8), true],
  ])(
    "reads as much per frame at the fixture's covers as at about 500, %s",
    async (_, from, widths, changes) => {
      const most: number[] = []
      const drawn: [number, boolean, boolean][] = []
      for (const count of [60, 500]) {
        const host = await gridOf(count, from)
        const list = need(host, 'main .grid')
        await frames(2)
        const columns = columnsOfList(list)
        const seen = list.getBoundingClientRect().top < innerHeight
        const reads = readsByFrame()
        await act(async () => {
          await dragged(widths.map((width) => ({ width })))
        })
        most.push(Math.max(...reads()))
        drawn.push([
          list.children.length,
          seen,
          columnsOfList(list) !== columns,
        ])
        gone(host)
      }
      // The fixture tells: each page drew its covers, its list started in
      // view, and its drag changed the columns where asked.
      expect(drawn).toEqual([
        [60, true, changes],
        [500, true, changes],
      ])
      expect(most[0]).toBeGreaterThan(0)
      expect(most[1]).toBe(most[0])
    },
  )

  it("reads as much per scroll event at the fixture's covers as at about 500", async () => {
    const each: number[] = []
    for (const count of [60, 500]) {
      const host = await gridOf(count, 1280)
      await frames(2)
      let events = 0
      const scrolled = () => {
        events += 1
      }
      window.addEventListener('scroll', scrolled, {
        capture: true,
        passive: true,
      })
      const reads = readsByFrame('/src/reseat.ts')
      // From where it starts: the scroll keeper puts back the last page's.
      await wheeledTo(host, scrollerOf(host).scrollTop + 600)
      const total = reads().reduce((sum, frame) => sum + frame, 0)
      window.removeEventListener('scroll', scrolled, { capture: true })
      each.push(events > 0 ? total / events : Number.NaN)
      gone(host)
    }
    expect(each[0]).toBeGreaterThan(0)
    expect(each[1]).toBe(each[0])
  })
})

describe('the self-check', () => {
  it('snaps a list not laid where its arithmetic says', async () => {
    const [edge] = PAGES['the home grid'].edges
    const host = await homePage({ width: edge - 8, coverSize: 144 })
    const list = need(host, 'main .grid')
    // Covers stand this far in from the list's edge, which the arithmetic,
    // laying them from its box's corner, does not model.
    list.style.paddingLeft = '6px'
    await frames(2)
    const record = recorded(list)
    await frames(2)
    const warned = vi.spyOn(console, 'warn').mockImplementation(() => {})
    onTestFinished(() => warned.mockRestore())
    await act(async () => {
      await dragged(
        [edge - 2, edge + 4, edge + 10, edge + 16].map((width) => ({ width })),
      )
    })
    await afterMotion()
    expect({
      changes: changesIn(record.frames).length,
      gliding: glidingIn(record.frames),
      warned: warned.mock.calls.map(([message]) => message),
    }).toEqual({
      changes: 1,
      gliding: [],
      warned: ['reseat: the list is not laid where its arithmetic says'],
    })
  })
})

/** The first cover on the top row of the home grid seen, by its tile's box. */
function firstOnTopRow(host: HTMLElement): Element {
  const line = lineOf(host)
  const covers = [...need(host, 'main .grid').children]
  const seen = covers.find((li) => li.getBoundingClientRect().bottom > line)
  if (seen === undefined) throw new Error('no cover below the top')
  const { top } = seen.getBoundingClientRect()
  return covers.find(
    (li) => Math.abs(li.getBoundingClientRect().top - top) < 0.5,
  ) as Element
}

/**
 * F1's drags: the width each starts at, its widths a frame each, the
 * window's height, wider than tall throughout (a window that turns square
 * snaps, as a phone's turn does), and whether the anchor is put where the
 * view's top crosses it half way down.
 */
const INTERRUPTED = {
  into: {
    from: ABOVE_PHONE + 8,
    widths: [ABOVE_PHONE + 4, ABOVE_PHONE - 4, 1010, 970, 930, 880],
    height: 844,
    straddled: false,
  },
  outOf: {
    from: PHONE_COLUMN - 6,
    widths: [PHONE_COLUMN - 4, PHONE_COLUMN + 2, 860, 904],
    height: 600,
    straddled: true,
  },
}

describe("a glide the phone layout's edge interrupts", () => {
  it.each([
    [
      'on the first page drawn, whose swap puts the place back after the lists are read',
      true,
      INTERRUPTED.into,
    ],
    [
      'on a page reached by a click, whose swap puts it back before',
      false,
      INTERRUPTED.into,
    ],
    [
      'out of the phone layout while its covers scale, the anchor among them, on a page reached by a click',
      false,
      INTERRUPTED.outOf,
    ],
  ])('cancels the glide and keeps the anchor, %s', async (_, early, drive) => {
    if (early) {
      // A list attached before the shell, as in the put-back case above.
      const before = mount(
        <MemoryRouter>
          <SeriesGrid series={[series(1)]} unfiled={[]} />
        </MemoryRouter>,
      )
      before.hidden = true
    }
    const host = await homePage({
      width: drive.from,
      height: drive.height,
      coverSize: 144,
    })
    const list = need(host, 'main .grid')
    const scroller = scrollerOf(host)
    await wheeledTo(host, (scroller.scrollHeight - scroller.clientHeight) / 2)
    await frames(2)
    if (drive.straddled) {
      // Half of the next row's first cover above the top: the place keeps
      // its share of the cover's height, which a glide's scale changes.
      const line = lineOf(host)
      const below = [...list.children].find(
        (li) => li.getBoundingClientRect().top > line + 1,
      )
      if (below === undefined) throw new Error('no cover below the top')
      const { top, height } = below.getBoundingClientRect()
      await wheeledTo(host, scroller.scrollTop + top - line + height / 2)
      await frames(2)
    }
    const anchor = firstOnTopRow(host)
    const { top, height } = anchor.getBoundingClientRect()
    const share = Math.max(0, (lineOf(host) - top) / height)
    const seen: {
      phone: boolean
      columns: number
      gliding: number
      anchorGliding: boolean
      marked: number
      off: number
    }[] = []
    // Made after the page's, so it reads each frame after the lists' and
    // the swap's observers, before it is painted.
    const observer = new ResizeObserver(() => {
      const box = anchor.getBoundingClientRect()
      const off = box.top - (lineOf(host) - share * box.height)
      seen.push({
        phone: matchMedia(PHONE_LAYOUT).matches,
        columns: columnsOfList(list),
        gliding: [...list.children].filter((tile) => scriptedIn(tile) > 0)
          .length,
        anchorGliding: scriptedIn(anchor) > 0,
        marked: document.querySelectorAll('[data-reseating]').length,
        off: Math.abs(off) <= 1 ? 0 : off,
      })
    })
    observer.observe(list)
    beforeUnmount(() => observer.disconnect())
    await act(async () => {
      await dragged(drive.widths.map((width) => ({ width })))
    })
    const at = seen.findIndex(({ phone }) => phone !== seen[0].phone)
    const changed = seen.findIndex(
      (frame, i) => i > 0 && frame.columns !== seen[i - 1].columns,
    )
    // The fixture tells: the columns changed, and the crossing came while
    // that change's covers glided; where asked, the anchor's among them,
    // with the view's top well down it.
    expect({
      changed: changed > 0 && changed < at,
      crossedGliding: (seen[at - 1]?.gliding ?? 0) > 0,
      anchor: drive.straddled && {
        share: share > 0.2,
        gliding: seen[at - 1]?.anchorGliding,
      },
      first: seen[at] && {
        gliding: seen[at].gliding,
        marked: seen[at].marked,
        off: seen[at].off,
      },
    }).toEqual({
      changed: true,
      crossedGliding: true,
      anchor: drive.straddled && { share: true, gliding: true },
      first: { gliding: 0, marked: 0, off: 0 },
    })
    await afterMotion()
  })
})

describe('a glide after a scroll', () => {
  // [when the wheel turns, how far the page must have scrolled by the next
  // change]: at rest, waited out; or two frames into the first glide, with
  // the next change made at once, while the covers still glide and the
  // page may still be scrolling.
  it.each([
    ['between two changes', false, 300],
    ['while the covers glide, then the next change', true, 100],
  ] as const)(
    'starts where drawn after a scroll %s',
    async (_, gliding, least) => {
      const [edge] = PAGES['the home grid'].edges
      const host = await homePage({ width: edge - 2, coverSize: 144 })
      const record = recorded(need(host, 'main .grid'))
      await frames(2)
      await act(async () => {
        await dragged([{ width: edge + 2 }])
      })
      if (gliding) {
        await frames(2)
        await userEvent.wheel(need(host, 'main'), { delta: { y: 400 } })
      } else {
        await afterMotion()
        await wheeledTo(host, scrollerOf(host).scrollTop + 400)
        await frames(2)
      }
      // On from where the window held: the drag's first frame is the
      // change, so no frame between reads the list again.
      await act(async () => {
        await dragged([{ width: edge - 2 }])
      })
      await frames(1)
      const changes = changesIn(record.frames)
      expect({
        changes: changes.length,
        scrolled:
          record.frames[changes[1] - 1].scroll -
            record.frames[changes[0]].scroll >
          least,
        interrupted: startsOff(record, changes[1]).some(
          (each) => each.gliding && each.glides,
        ),
      }).toEqual({ changes: 2, scrolled: true, interrupted: gliding })
      expectStartsWhereDrawn(record, changes)
      await afterMotion()
    },
  )

  it('reads where a list is drawn after a scroll, where its box alone changed size', async () => {
    const [edge] = PAGES['the home grid'].edges
    const host = await homePage({ width: edge - 2, coverSize: 144 })
    const list = need(host, 'main .grid')
    // Room below the covers: the list's box grows, the box its covers are
    // laid in does not.
    list.style.paddingBottom = '40px'
    await frames(2)
    const record = recorded(list)
    await frames(2)
    await wheeledTo(host, 400)
    await frames(2)
    await act(async () => {
      await dragged([{ width: edge + 2 }])
    })
    await frames(1)
    const changes = changesIn(record.frames)
    expect({
      changes: changes.length,
      scrolled: record.frames[changes[0] - 1].scroll > 300,
    }).toEqual({ changes: 1, scrolled: true })
    expectStartsWhereDrawn(record, changes)
    await afterMotion()
  })
})
