import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { WorkNotFound } from './api'
import { fakeResolver, whole } from './fixtures'
import './index.css'
import { paintedBox } from './notes'
import { focused } from './testing/focused'
import { mount } from './testing/mount'
import { mouse } from './testing/mouse'

/**
 * The reader's two bars and what docks between them, laid out in a real
 * browser (spec 2026-09-17 §8) — the only place these can be observed.
 * `Chrome.test.tsx`, `TopBar.test.tsx` and `NoteList.test.tsx` pin what each
 * style declares, in jsdom, which CI runs; jsdom has no layout, so a box
 * cannot be told from another there, and `var()` stays a string. This asks
 * Chromium where each box actually lands, at a desktop, a portrait tablet and
 * a phone.
 *
 * O65 was first measured here: with the chrome open, `Library` sat over the
 * `single` and `two` buttons. Both are in the top bar now (§8.3), and the
 * first two cases ask the same question of it. The Ink-theme block is last.
 *
 * The mocks are `App.test.tsx`'s, for the reasons that file's header gives:
 * `vi.mock('./resolver')` is what lets `App.tsx` load at all without the
 * generated wasm package. Only the network calls are replaced in `./api` and
 * `./library`, so the `<img src>`s are the real ones. Unless a server is
 * listening behind vite's dev proxy they fail there, and the proxy prints
 * `ECONNREFUSED` to stderr.
 *
 * CI does not run this project (`vite.config.ts`: no Chromium); the gate
 * does.
 */

const ports = vi.hoisted(() => ({
  getSidecarWithEtag: vi.fn(),
  getProgress: vi.fn(),
  getNotes: vi.fn(),
  putNotes: vi.fn(),
  putProgress: vi.fn(),
  patchAlignment: vi.fn(),
  me: vi.fn(),
  logout: vi.fn(),
  listKeys: vi.fn(),
  listSessions: vi.fn(),
  wasmResolver: vi.fn(),
  workDetail: vi.fn(),
}))

vi.mock('taiyaku-wasm', () => ({
  default: async () => undefined,
  WasmSidecar: {
    fromJson: () => {
      throw new Error(
        'taiyaku-wasm is mocked: nothing here may build a real sidecar',
      )
    },
  },
}))

vi.mock('./resolver', () => ({ wasmResolver: ports.wasmResolver }))

vi.mock('./api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./api')>()),
  getSidecarWithEtag: ports.getSidecarWithEtag,
  getProgress: ports.getProgress,
  getNotes: ports.getNotes,
  putNotes: ports.putNotes,
  putProgress: ports.putProgress,
  patchAlignment: ports.patchAlignment,
  me: ports.me,
  logout: ports.logout,
  listKeys: ports.listKeys,
  listSessions: ports.listSessions,
}))

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  work: ports.workDetail,
}))

const { default: App } = await import('./App')

const WORK = 'vol-01'

/**
 * Read `ltr`, so the strip lays its first tile out at its left end, under
 * where a `Library` button docked over the strip would sit. This fixture was
 * first written `rtl`, and a mutation docking the button over the strip
 * passed every test below; read `ltr`, the same mutation reports `page 1`
 * onward as covered.
 */
const PAGES = 6

beforeEach(() => {
  localStorage.clear()
  vi.clearAllMocks()
  ports.getSidecarWithEtag.mockResolvedValue({
    json: JSON.stringify({
      work: { reading: 'ltr' },
      editions: { en: {}, raw: {} },
      hub: 'raw',
      alignments: [{ from: 'raw', to: 'en', quality: { verdict: 'sound' } }],
    }),
    etag: 's1',
  })
  ports.getProgress.mockResolvedValue({ edition: 'raw', slot: 2 })
  ports.getNotes.mockResolvedValue({
    doc: { work: WORK, notes: [] },
    etag: 'n1',
  })
  ports.putProgress.mockResolvedValue(undefined)
  ports.logout.mockResolvedValue(undefined)
  ports.listKeys.mockResolvedValue([])
  ports.listSessions.mockResolvedValue([])
  ports.me.mockResolvedValue({
    user_id: 1,
    admin: true,
    can_download: true,
    can_stream: true,
  })
  ports.workDetail.mockResolvedValue({
    id: 1,
    slug: WORK,
    title: 'Volume One',
    volume: 1,
    aligned: true,
    pages: PAGES,
    editions: [
      { key: 'raw', label: 'Raw', members: 1, pages: PAGES, missing: 0 },
      { key: 'en', label: 'English', members: 1, pages: PAGES, missing: 0 },
    ],
    opens: 'raw',
    alignments: [],
  })
  const pairs: Record<string, number> = {}
  for (let i = 0; i < PAGES; i++) {
    pairs[`raw|en|${i}`] = i
    pairs[`en|raw|${i}`] = i
  }
  ports.wasmResolver.mockImplementation(async () =>
    fakeResolver({ raw: whole(PAGES), en: whole(PAGES) }, pairs),
  )
})

/** Waits until `check` holds, or gives up — `PageView.browser.test.tsx`'s helper. */
async function until(check: () => boolean, ms = 2000): Promise<void> {
  const deadline = performance.now() + ms
  while (performance.now() < deadline) {
    if (check()) return
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10))
    })
  }
  throw new Error('condition never held')
}

const button = (text: string) =>
  [...document.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

/** The button a click at the centre of `el` would actually reach. */
function hitAtCentreOf(el: Element): HTMLButtonElement | null {
  const box = el.getBoundingClientRect()
  return (
    document
      .elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)
      ?.closest('button') ?? null
  )
}

const overlaps = (a: DOMRect, b: DOMRect) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom

/**
 * The reader at `width`×`height`, loaded, the chrome hidden. The wrapper
 * fills the viewport the way `#root` does under `index.css`, so `App`'s
 * `height: 100%` column lays out at the viewport's size.
 */
async function reader(width: number, height: number) {
  await page.viewport(width, height)
  mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <App work={WORK} onLeave={() => {}} />
    </div>,
  )
  await until(() => document.querySelector('.reader') !== null)
}

function press(key: string) {
  act(() => {
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key, bubbles: true }),
    )
  })
}

/** The same, with the chrome shown. */
async function readerWithChrome(width: number, height: number) {
  await reader(width, height)
  press('Tab')
  await until(() => button('Library') !== undefined)
}

const box = (selector: string) =>
  (document.querySelector(selector) as HTMLElement).getBoundingClientRect()

// 1440×900 is the browser pass's desktop; 768×1024 a portrait tablet's,
// below the breakpoint; 390×844 a phone's.
const SIZES: [number, number][] = [
  [1440, 900],
  [768, 1024],
  [390, 844],
]

/** The strip's height at a width: 160 px tiles from 900 px up, 96 below, plus 16. */
const stripAt = (width: number) => (width >= 900 ? 176 : 112)

describe('the reader’s two bars, in a browser', () => {
  it.each(SIZES)(
    'at %i×%i, each top-bar control is what a click at its centre reaches',
    async (width, height) => {
      await readerWithChrome(width, height)
      for (const name of ['Library', 'single', 'two', 'scroll', 'Settings']) {
        const control = button(name)
        expect(hitAtCentreOf(control), name).toBe(control)
      }
    },
  )

  it.each(SIZES)(
    'at %i×%i, the Library button overlaps no other control',
    async (width, height) => {
      await readerWithChrome(width, height)
      const leave = button('Library')
      const rect = leave.getBoundingClientRect()
      const covered = [...document.querySelectorAll('button')]
        .filter((b) => b !== leave)
        // The turn buttons draw nothing and take no pointer (spec
        // 2026-09-26 §3.4); what this guards is one drawn control covering
        // another.
        .filter((b) => !b.classList.contains('reader-turn'))
        .filter((b) => overlaps(rect, b.getBoundingClientRect()))
        .map(
          (b) =>
            b.textContent ||
            b.getAttribute('aria-label') ||
            b.querySelector('img')?.getAttribute('alt') ||
            b.outerHTML.slice(0, 80),
        )
      expect(covered).toEqual([])
    },
  )

  it.each(SIZES)(
    'at %i×%i, the bars meet the reader’s edges',
    async (width, height) => {
      await readerWithChrome(width, height)
      const top = box('.reader-top-bar')
      const bottom = box('.reader-bottom-bar')
      expect([top.top, top.left, top.right, top.height]).toEqual([
        0,
        0,
        width,
        52,
      ])
      expect([bottom.bottom, bottom.left, bottom.right]).toEqual([
        height,
        0,
        width,
      ])
      // Its 1 px rule, the strip, the 2 px progress line and the 32 px readout.
      expect(bottom.height).toBe(1 + stripAt(width) + 2 + 32)
    },
  )

  it.each(SIZES)(
    'at %i×%i, the strip stands 176 px from 900 px up and 112 below',
    async (width, height) => {
      await readerWithChrome(width, height)
      const strip = (
        document.querySelector('.reader-bottom-bar') as HTMLElement
      ).firstElementChild as HTMLElement
      expect(strip.getBoundingClientRect().height).toBe(stripAt(width))
      // The strip's own tiles (spec §8.6): 160 px at and above 900 px, 96
      // below — the rail's height minus its padding and border, measured on
      // the tile rather than assumed from the rail.
      const tile = document.querySelector(
        '.reader-bottom-bar img',
      ) as HTMLElement
      expect(tile.getBoundingClientRect().height).toBe(width >= 900 ? 160 : 96)
    },
  )

  it.each(SIZES)(
    'at %i×%i, the page area is the reader’s whole height, whether or not the bars are shown',
    async (width, height) => {
      await reader(width, height)
      const hidden = box('.page-area')
      expect([hidden.top, hidden.height, hidden.width]).toEqual([
        0,
        height,
        width,
      ])
      press('Tab')
      await until(() => button('Library') !== undefined)
      const shown = box('.page-area')
      expect([shown.top, shown.height, shown.width]).toEqual([0, height, width])
    },
  )

  it.each(SIZES)(
    'at %i×%i, the keyboard help docks 16 px inside both bars and the right edge',
    async (width, height) => {
      await readerWithChrome(width, height)
      press('h')
      await until(
        () => document.querySelector('[aria-label="Keyboard help"]') !== null,
      )
      const help = box('[aria-label="Keyboard help"]')
      const top = box('.reader-top-bar')
      const bottom = box('.reader-bottom-bar')
      expect(help.top).toBe(top.bottom + 16)
      expect(help.bottom).toBeLessThanOrEqual(bottom.top - 16)
      expect(help.right).toBe(width - 16)
      // `.docked-panel`'s `max-width` clamps the panel's width on a phone —
      // at 390 px this is what keeps its left edge clear of the opposite
      // edge rather than running the panel edge-to-edge.
      expect(help.left).toBeGreaterThanOrEqual(16)
    },
  )
})

describe('O112: the note list under the page, in a browser', () => {
  beforeEach(() => {
    localStorage.setItem(
      'taiyaku.prefs.annotations',
      JSON.stringify({
        markers: true,
        travel: true,
        reveal: true,
        notesList: 'below',
      }),
    )
    // Nine notes on the slot on screen. The list runs to the reader's bottom
    // edge and its rows sit 27 px apart, so growing it by one note shifts
    // every row up by that much. Measured in Chromium: at eight notes the
    // list stood 227 px tall at every size in `SIZES`, and at 1440×900 —
    // where the bar is 211 px, from the 176 px strip, the tallest of the
    // three — no row cleared its top edge at all, so "no row takes a click
    // meant for them" could not fail there: every row was already
    // unreachable for a reason this test was not checking. (At 768×1024 and
    // 390×844, where the bar is 147 px, two rows already cleared it, so
    // those sizes were already testing the click half.) Nine notes (254 px)
    // clears the bar everywhere: one row at 1440×900, three at 768×1024 and
    // three at 390×844. Ten overflowed the list's own scrollable area at
    // 390×844 instead — the tenth row's unclipped
    // `getBoundingClientRect()` fell below the viewport, which
    // `elementFromPoint` cannot reach regardless of the bar — so nine is the
    // largest count that clears the bar at every size while staying clear of
    // that.
    ports.getNotes.mockResolvedValue({
      doc: {
        work: WORK,
        notes: Array.from({ length: 9 }, (_, i) => i + 1).map((n) => ({
          id: `note-${n}`,
          edition: 'raw',
          slot: 2,
          body: `note ${n}`,
          created: '2026-01-01T00:00:00.000Z',
          updated: '2026-01-01T00:00:00.000Z',
        })),
      },
      etag: 'n1',
    })
  })

  const rows = () =>
    [
      ...document.querySelectorAll(
        '[aria-label="Notes on this page"] [role="listitem"]',
      ),
    ] as HTMLButtonElement[]

  it.each(SIZES)(
    'at %i×%i, every row takes a click at its centre while the bars are away',
    async (width, height) => {
      await reader(width, height)
      await until(() => rows().length === 9)
      for (const row of rows()) expect(hitAtCentreOf(row)).toBe(row)
    },
  )

  it.each(SIZES)(
    'at %i×%i, the list steps aside under the bars, and no row takes a click meant for them',
    async (width, height) => {
      await reader(width, height)
      await until(() => rows().length === 9)
      const before = box('.page-area').height
      press('Tab')
      await until(() => button('Library') !== undefined)
      const list = document.querySelector(
        '[aria-label="Notes on this page"]',
      ) as HTMLElement
      expect(getComputedStyle(list).visibility).toBe('hidden')
      for (const row of rows()) expect(hitAtCentreOf(row)).not.toBe(row)
      // O112's other half: `visibility: hidden` keeps the list mounted and
      // taking its space, so the page area does not change size when the
      // bars come up. `display: none` would shrink the list to nothing and
      // grow the page area to fill it.
      expect(box('.page-area').height).toBe(before)
    },
  )
})

describe('the reader in Ink whatever the theme, in a browser', () => {
  /**
   * The mechanism spec §8 rests on, in the app rather than in a probe: the
   * tokens are `light-dark()` pairs, and `color-scheme: dark` on `.reader`
   * picks their Ink half inside it even with Paper pinned on `<html>`. The
   * body is the control — outside the reader, Paper's text.
   */
  it('draws the reader in Ink on black with Paper pinned', async () => {
    document.documentElement.setAttribute('data-theme', 'paper')
    try {
      await reader(1440, 900)
      const root = getComputedStyle(
        document.querySelector('.reader') as HTMLElement,
      )
      expect(root.color).toBe('rgb(236, 228, 212)')
      expect(root.backgroundColor).toBe('rgb(0, 0, 0)')
      expect(getComputedStyle(document.body).backgroundColor).toBe(
        'rgb(0, 0, 0)',
      )
      press('Tab')
      await until(() => button('Library') !== undefined)
      expect(
        getComputedStyle(
          document.querySelector('.reader-top-bar') as HTMLElement,
        ).color,
      ).toBe('rgb(236, 228, 212)')
      const outside = document.createElement('p')
      document.body.append(outside)
      expect(getComputedStyle(outside).color).toBe('rgb(35, 30, 23)')
      outside.remove()
    } finally {
      document.documentElement.removeAttribute('data-theme')
    }
  })
})

// ---- the gestures, laid out -------------------------------------------------

/**
 * What jsdom cannot answer about ④'s gestures: the thirds are thirds of the
 * page area's *laid-out* width, and a swipe is measured against a real box.
 * `App.test.tsx` pins the routing against a stubbed rect; this asks Chromium
 * where the boundaries actually fall, and whether a bar is sitting on one.
 *
 * **Driven with a mouse, deliberately.** Spec 2026-09-17 §9.1 put every
 * pointer in scope, so a mouse press exercises every rule here.
 * `@vitest/browser-playwright` would allow a touch context through
 * `contextOptions.hasTouch`, but `vite.config.ts` gives this project a single
 * instance — enabling it would put all fifty-eight existing cases into a touch
 * context to reach the two things only a finger has, the double tap and the
 * pinch. Both are ④'s device pass's.
 */
function pointerAt(type: string, at: { x: number; y: number }) {
  // Dispatched on whatever is actually under the point, so a bar drawn over
  // the page area intercepts the press here exactly as it would for a reader.
  const target = document.elementFromPoint(at.x, at.y) ?? document.body
  act(() => {
    target.dispatchEvent(
      new PointerEvent(type, {
        bubbles: true,
        cancelable: true,
        clientX: at.x,
        clientY: at.y,
      }),
    )
  })
}

const tapAt = (at: { x: number; y: number }) => {
  pointerAt('pointerdown', at)
  pointerAt('pointerup', at)
}

/** Where the reader stands, as the progress line draws it. */
const fill = () =>
  (document.querySelector('.reader-progress-fill') as HTMLElement | null)?.style
    .width ?? null

describe('the gestures on the page, laid out', () => {
  it.each(SIZES)(
    'at %i×%i, the outer thirds of the real page area turn and the middle toggles',
    async (width, height) => {
      await readerWithChrome(width, height)
      const area = box('.page-area')
      const y = area.top + area.height / 2
      const third = area.width / 3
      const start = fill()
      // Just inside the right third, against the measured boundary rather
      // than an assumed one.
      tapAt({ x: area.right - third + 1, y })
      await until(() => fill() !== start)
      const forward = fill()
      tapAt({ x: area.left + third - 1, y })
      await until(() => fill() === start)
      expect(forward).not.toBe(start)
      // The middle third takes the bars down, so the fill leaves the DOM.
      tapAt({ x: area.left + area.width / 2, y })
      await until(() => fill() === null)
    },
  )

  it('reads a drag across the real page area as a swipe', async () => {
    await readerWithChrome(1440, 900)
    const area = box('.page-area')
    const y = area.top + area.height / 2
    const from = area.left + area.width / 2
    const start = fill()
    pointerAt('pointerdown', { x: from, y })
    pointerAt('pointermove', { x: from - 120, y })
    pointerAt('pointerup', { x: from - 120, y })
    // Dragged leftwards: the physical `→`.
    await until(() => fill() !== start)
  })

  it('does not turn on a drag too short to be a swipe', async () => {
    await readerWithChrome(1440, 900)
    const area = box('.page-area')
    const y = area.top + area.height / 2
    const from = area.left + area.width / 2
    const start = fill()
    // 30 px: past the tap's slop, short of the swipe's minimum. It is
    // neither, and it is the pan it always was.
    pointerAt('pointerdown', { x: from, y })
    pointerAt('pointermove', { x: from - 30, y })
    pointerAt('pointerup', { x: from - 30, y })
    expect(fill()).toBe(start)
  })

  it('does not turn once the reader has zoomed in', async () => {
    await readerWithChrome(1440, 900)
    const area = box('.page-area')
    const y = area.top + area.height / 2
    const from = area.left + area.width / 2
    press('=')
    const start = fill()
    pointerAt('pointerdown', { x: from, y })
    pointerAt('pointermove', { x: from - 120, y })
    pointerAt('pointerup', { x: from - 120, y })
    // Above fit-whole every drag is the pan's, and `isWhole` is what says so
    // — by value, because `clampRect` has already replaced `WHOLE` by now.
    expect(fill()).toBe(start)
  })

  /**
   * `Settings.tsx`'s own `onClick` handler, dispatched the way `click` fires
   * for a real down-then-up — `pointerAt`'s pair does not synthesise one; a
   * script-dispatched `PointerEvent` pair never does, in a real browser any
   * more than in jsdom (`Settings.test.tsx`'s own note).
   */
  function clickAt(at: { x: number; y: number }) {
    const target = document.elementFromPoint(at.x, at.y) ?? document.body
    act(() => {
      target.dispatchEvent(
        new MouseEvent('click', {
          bubbles: true,
          clientX: at.x,
          clientY: at.y,
        }),
      )
    })
  }

  it.each(SIZES)(
    'at %i×%i, a press on Settings’ scrim closes it and turns nothing',
    async (width, height) => {
      await readerWithChrome(width, height)
      press('s')
      await until(
        () => document.querySelector('[aria-label="Settings"]') !== null,
      )
      const start = fill()
      const area = box('.page-area')
      const panel = box('.reader-dialog-panel')
      // `.reader-dialog` is the whole viewport (measured 2026-09-20): the
      // outer-third point the cases above tap can land inside the panel
      // rather than beside it once the panel is wide relative to a narrower
      // viewport. The strip between the panel's own edge and the viewport's
      // is where the scrim is guaranteed to be, at every size tested.
      const at = {
        x: panel.right + (area.right - panel.right) / 2,
        y: area.top + area.height / 2,
      }
      pointerAt('pointerdown', at)
      pointerAt('pointerup', at)
      clickAt(at)
      await until(
        () => document.querySelector('[aria-label="Settings"]') === null,
      )
      expect(fill()).toBe(start)
    },
  )

  it.each(SIZES)(
    'at %i×%i, a press in the painted margin finishes the note',
    async (width, height) => {
      await reader(width, height)
      press('n')
      await until(() => document.querySelector('[aria-label="Note"]') !== null)
      const img = document.querySelector('.page-area img') as HTMLImageElement
      // C1's fix reads `draftImage`'s own match — `src.endsWith(pageAt(...))`
      // — to tell this image from a facing one, so the swap below has to
      // survive it: the real page URL, captured before it is replaced, so a
      // press on this element is still read as a press on the draft's own
      // page rather than on a stranger `draftImage` cannot place.
      const realSrc = img.src
      // A tall page, decoded for real rather than served — no backend runs
      // behind this project's dev proxy (this describe block's own doc).
      // `object-fit: contain` still letterboxes it against the element's own
      // laid-out box, exactly as a served page would.
      img.src =
        "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='500' height='1000'/%3E"
      await img.decode()
      // The real load already happened above — this only shadows what a
      // later JS read of `.src` reports, which is all `draftImage` does.
      Object.defineProperty(img, 'src', {
        configurable: true,
        get: () => realSrc,
      })
      const rect = img.getBoundingClientRect()
      // The margin, computed from this fixture's own decode against the
      // element's laid-out box — not the 420/130 the real library measured,
      // which are that fixture's numbers at those two sizes only.
      const painted = paintedBox(
        { w: img.naturalWidth, h: img.naturalHeight },
        { w: rect.width, h: rect.height },
      )
      // Off the painted page, on whichever axis this size actually
      // letterboxes: beside it when the box is wider than the page's aspect,
      // above it when the box is taller.
      const at =
        painted.x > 0
          ? {
              x: rect.left + painted.x / 2,
              y: rect.top + painted.y + painted.h / 2,
            }
          : {
              x: rect.left + painted.x + painted.w / 2,
              y: rect.top + painted.y / 2,
            }
      pointerAt('pointerdown', at)
      await until(() => document.querySelector('[aria-label="Note"]') === null)
    },
  )
})

/**
 * O166 (spec 2026-09-26 §3.2): a key pressed on one of Settings' selects
 * belongs to the select. Real key presses, because a select's own handling —
 * typeahead, arrows — is the browser's and a dispatched event carries none of
 * it.
 */
describe('O166: a key pressed on a Settings select', () => {
  /** `layout`'s `mode` select — the one whose options include `scroll`. */
  const layoutSelect = () =>
    [...document.querySelectorAll('select')].find((s) =>
      [...s.options].some((o) => o.value === 'scroll'),
    ) as HTMLSelectElement
  const settingsOpen = () =>
    document.querySelector('[role="dialog"][aria-label="Settings"]') !== null

  it('a letter goes to the select, not to the reader', async () => {
    await reader(1440, 900)
    press('s')
    await until(settingsOpen)
    layoutSelect().focus()
    // `s` is Settings' own key: taken by the reader, it closes the panel
    // under the select and the typeahead never runs.
    await userEvent.keyboard('s')
    expect(settingsOpen()).toBe(true)
    // Chromium's typeahead moves from `single` to the next option starting
    // with `s`. The point is that the select changed at all, which it cannot
    // while the reader prevents the key's default.
    expect(layoutSelect().value).toBe('scroll')
  })

  it('an arrow goes to the select, not to the reader', async () => {
    await reader(1440, 900)
    press('s')
    await until(settingsOpen)
    layoutSelect().focus()
    await userEvent.keyboard('{ArrowRight}')
    // A turn writes progress after `useProgressWrite`'s 400 ms debounce.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 1000))
    })
    expect(ports.putProgress).not.toHaveBeenCalled()
  })
})

/**
 * The reader, with keyboard focus on its page area. Vitest runs these tests
 * in an iframe, and until something inside it takes focus a real key press
 * goes to the frame around it: run on its own, a walk's first `Tab` only
 * moved focus into the frame, and the ring started one press late. A reader
 * who has just opened the page has focus on `<body>` instead, which the ring
 * treats as the page too; `App.test.tsx`'s walks start from there, and the
 * real-library drive (`docs/notes.md`, §3) walked a fresh load in a real
 * browser.
 */
async function readerFocused(width: number, height: number) {
  await reader(width, height)
  ;(document.querySelector('[data-reader-page]') as HTMLElement).focus()
}

const barsUp = () => document.querySelector('.reader-bottom-bar') !== null

describe('the keyboard ring, with real keys (spec 2026-09-26 §3.1)', () => {
  it.each(SIZES)(
    'at %i×%i, Tab walks the bars and comes back to the page, and never leaves it',
    async (width, height) => {
      await readerFocused(width, height)
      const walk: string[] = []
      for (let i = 0; i < 14; i++) {
        await userEvent.tab()
        walk.push(`${focused()}${barsUp() ? ' +bars' : ''}`)
      }
      const lap = [
        'Library +bars',
        'single +bars',
        'two +bars',
        'scroll +bars',
        'Settings +bars',
        'page 3 +bars',
        'page',
      ]
      expect(walk).toEqual([...lap, ...lap])
    },
  )

  it.each(SIZES)(
    'at %i×%i, Shift+Tab walks it backwards',
    async (width, height) => {
      await readerFocused(width, height)
      const walk: string[] = []
      for (let i = 0; i < 7; i++) {
        await userEvent.tab({ shift: true })
        walk.push(`${focused()}${barsUp() ? ' +bars' : ''}`)
      }
      expect(walk).toEqual([
        'page 3 +bars',
        'Settings +bars',
        'scroll +bars',
        'two +bars',
        'single +bars',
        'Library +bars',
        'page',
      ])
    },
  )

  it('Escape from ⚙ takes the bars away and leaves focus on the page', async () => {
    await readerFocused(1440, 900)
    for (let i = 0; i < 5; i++) await userEvent.tab()
    expect(focused()).toBe('Settings')
    await userEvent.keyboard('{Escape}')
    expect(barsUp()).toBe(false)
    expect(focused()).toBe('page')
  })

  it('→ on the strip turns the page and focus follows it', async () => {
    await readerFocused(1440, 900)
    for (let i = 0; i < 6; i++) await userEvent.tab()
    expect(focused()).toBe('page 3')
    await userEvent.keyboard('{ArrowRight}')
    await until(() => focused() === 'page 4')
    expect(barsUp()).toBe(true)
  })
})

/**
 * The rest of §3.6's real-key cases. Each starts from `readerFocused` — a
 * press before anything in the test iframe has focus goes to the frame, not
 * the page — and each is at 1440×900 unless it says why not: nothing below
 * depends on layout, only on which element takes the key.
 */
describe('the keyboard ring, with real keys: every stop and every region (spec 2026-09-26 §3.6)', () => {
  /** The live region's page number — what a screen reader is told after a turn. */
  const spokenPage = () =>
    Number(
      /page (\d+)/.exec(
        document.querySelector('[aria-live="polite"]')?.textContent ?? '',
      )?.[1],
    )

  const tabs = async (n: number, shift = false) => {
    for (let i = 0; i < n; i++) await userEvent.tab({ shift })
  }

  it('Escape from every stop in the bars takes them down and leaves focus on the page', async () => {
    await readerFocused(1440, 900)
    const stops = ['Library', 'single', 'two', 'scroll', 'Settings', 'page 3']
    for (const [i, name] of stops.entries()) {
      // Each Escape lands on the page, so the walk to the next stop restarts.
      await tabs(i + 1)
      expect(focused(), `stop ${i}`).toBe(name)
      await userEvent.keyboard('{Escape}')
      expect(barsUp(), `${name}: bars`).toBe(false)
      expect(focused(), `${name}: focus`).toBe('page')
    }
  })

  it('→ on every stop in the bars turns one page, with the bars up and focus on the stop', async () => {
    await readerFocused(1440, 900)
    // The browser fixture reads `ltr`, so `→` is a step forward. Stored
    // progress is slot 2, page 3.
    const stops = ['Library', 'single', 'two', 'scroll', 'Settings']
    await tabs(1)
    await until(() => spokenPage() === 3)
    for (const name of stops) {
      expect(focused()).toBe(name)
      await userEvent.keyboard('{ArrowRight}')
      await until(() => spokenPage() === 4)
      expect(focused(), `${name} keeps focus`).toBe(name)
      expect(barsUp(), `${name}: bars`).toBe(true)
      // Put the page back, from the same stop, and move to the next.
      await userEvent.keyboard('{ArrowLeft}')
      await until(() => spokenPage() === 3)
      expect(focused(), `${name} keeps focus going back`).toBe(name)
      await tabs(1)
    }
    // The strip's stop, where focus follows the new current tile.
    expect(focused()).toBe('page 3')
    await userEvent.keyboard('{ArrowRight}')
    await until(() => spokenPage() === 4)
    expect(focused()).toBe('page 4')
    expect(barsUp()).toBe(true)
  })

  /**
   * The page a gap is reached on: `raw` slot 5 has no `en` counterpart, and
   * the borrow module is off so the gap is drawn as a gap (`App.test.tsx`'s
   * `swapOntoUnpaired`, which the same keys reach).
   */
  async function readerInAGap() {
    localStorage.setItem(
      'taiyaku.prefs.borrow',
      JSON.stringify({ enabled: false }),
    )
    ports.getProgress.mockResolvedValue({ edition: 'raw', slot: 5 })
    const pairs: Record<string, number> = {}
    for (let i = 0; i < 5; i++) {
      pairs[`raw|en|${i}`] = i
      pairs[`en|raw|${i}`] = i
    }
    pairs['en|raw|5'] = 5
    ports.wasmResolver.mockImplementation(async () =>
      fakeResolver({ raw: whole(PAGES), en: whole(PAGES) }, pairs),
    )
    await readerFocused(1440, 900)
    press('2')
    await until(() => document.querySelector('.reader-gap-title') !== null)
    ;(document.querySelector('[data-reader-page]') as HTMLElement).focus()
  }

  /** What focus is on, and whether it is inside the page area. */
  const where = () =>
    `${focused()}${document.activeElement?.closest('[data-reader-page]') && !document.activeElement.matches('[data-reader-page]') ? ' (in page)' : ''}`

  it('Tab reaches a gap’s buttons after the strip and before the page', async () => {
    await readerInAGap()
    const walk: string[] = []
    for (let i = 0; i < 12; i++) {
      await tabs(1)
      walk.push(where())
      if (focused() === 'page') break
    }
    const inPage = walk.flatMap((w, i) => (w.endsWith('(in page)') ? [i] : []))
    const strip = walk.findIndex(
      (w) => /^page \d/.test(w) && !w.includes('(in'),
    )
    expect(inPage.length).toBeGreaterThan(0)
    expect(strip).toBeGreaterThan(-1)
    expect(Math.min(...inPage)).toBeGreaterThan(strip)
    expect(walk.at(-1)).toBe('page')
    expect(inPage.at(-1)).toBe(walk.length - 2)
  })

  it('Tab reaches a note’s margin tab after the strip, before the note list, then the page', async () => {
    localStorage.setItem(
      'taiyaku.prefs.annotations',
      JSON.stringify({
        markers: true,
        travel: true,
        reveal: true,
        notesList: 'below',
      }),
    )
    ports.getNotes.mockResolvedValue({
      doc: {
        work: WORK,
        notes: [
          {
            id: 'note-1',
            edition: 'raw',
            slot: 2,
            body: 'a note already here',
            created: '2026-01-01T00:00:00.000Z',
            updated: '2026-01-01T00:00:00.000Z',
          },
        ],
      },
      etag: 'n1',
    })
    // The page images fail to load here (the file header says why), and
    // `PageAnnotations` draws nothing over an image with no natural size. Give
    // every image one, for this test only.
    const natural = (name: 'naturalWidth' | 'naturalHeight', value: number) => {
      const original = Object.getOwnPropertyDescriptor(
        HTMLImageElement.prototype,
        name,
      )
      Object.defineProperty(HTMLImageElement.prototype, name, {
        configurable: true,
        get: () => value,
      })
      return () => {
        if (original)
          Object.defineProperty(HTMLImageElement.prototype, name, original)
      }
    }
    // Registered before either stub goes in, so a throw between them cannot
    // leave the first one behind for the next test.
    const restore: (() => void)[] = []
    onTestFinished(() => {
      for (const undo of restore) undo()
    })
    restore.push(natural('naturalWidth', 600))
    restore.push(natural('naturalHeight', 900))
    await readerFocused(1440, 900)
    await until(() => document.querySelector('[aria-label^="Note: "]') !== null)
    const walk: string[] = []
    for (let i = 0; i < 12; i++) {
      await tabs(1)
      const el = document.activeElement as HTMLElement
      const region = el.closest('[aria-label="Notes on this page"]')
        ? 'list'
        : el.closest('[data-reader-page]') && !el.matches('[data-reader-page]')
          ? 'margin'
          : 'other'
      walk.push(`${region}:${el.getAttribute('aria-label') ?? focused()}`)
      if (focused() === 'page') break
    }
    const strip = walk.findIndex((w) => /^other:page \d/.test(w))
    const tab = walk.findIndex((w) => w.startsWith('margin:Note: '))
    const row = walk.findIndex((w) => w.startsWith('list:'))
    expect(strip).toBeGreaterThan(-1)
    expect(tab).toBeGreaterThan(strip)
    expect(row).toBeGreaterThan(tab)
    expect(walk.at(-1)).toBe('other:page')
    expect(walk).toHaveLength(row + 2)
  })

  it('while correcting with a counterpart picked, the banner’s buttons come before the strip and the bars stay up', async () => {
    await readerFocused(1440, 900)
    press('c')
    await until(() => document.body.textContent?.includes('Pick the') ?? false)
    const tile = [...document.querySelectorAll('button')].filter((b) =>
      (b.querySelector('img')?.getAttribute('src') ?? '').includes('/thumb'),
    )[3]
    await userEvent.click(tile)
    await until(() => button('Confirm') !== undefined)
    ;(document.querySelector('[data-reader-page]') as HTMLElement).focus()
    const walk: string[] = []
    for (let i = 0; i < 20; i++) {
      await tabs(1)
      walk.push(focused())
      expect(barsUp()).toBe(true)
      if (focused() === 'page') break
    }
    const confirm = walk.indexOf('Confirm')
    const cancel = walk.indexOf('Cancel')
    const first = walk.findIndex((w) => /^page \d/.test(w))
    expect(confirm).toBeGreaterThan(-1)
    expect(cancel).toBeGreaterThan(confirm)
    expect(first).toBeGreaterThan(cancel)
    expect(walk.at(-1)).toBe('page')
  })
})

describe('the turn buttons, laid out (spec 2026-09-26 §3.4)', () => {
  it.each(SIZES)(
    'at %i×%i, a pointer at a turn button’s centre reaches the page, not the button',
    async (width, height) => {
      await reader(width, height)
      for (const name of ['Next page', 'Previous page']) {
        const turn = button(name)
        const at = turn.getBoundingClientRect()
        const hit = document.elementFromPoint(
          at.left + at.width / 2,
          at.top + at.height / 2,
        )
        expect(hit?.closest('.reader-turn'), name).toBeNull()
        expect(hit?.closest('.page-area'), name).not.toBeNull()
      }
    },
  )

  it('each covers its outer third of the page area', async () => {
    await reader(1440, 900)
    const area = box('.page-area')
    const next = button('Next page').getBoundingClientRect()
    const previous = button('Previous page').getBoundingClientRect()
    // `ltr`: Next on the right.
    expect(Math.round(next.right)).toBe(Math.round(area.right))
    expect(Math.round(previous.left)).toBe(Math.round(area.left))
    expect(Math.round(next.width)).toBe(Math.round(area.width / 3))
    expect(Math.round(next.height)).toBe(Math.round(area.height))
  })
})

describe('Settings keeps Tab inside it, with real keys (spec 2026-09-26 §3.3)', () => {
  it('neither Tab nor Shift+Tab leaves the open panel', async () => {
    await reader(1440, 900)
    press('s')
    const panel = () =>
      document.querySelector('[role="dialog"][aria-label="Settings"]')
    await until(() => panel() !== null)
    const escaped: string[] = []
    for (let i = 0; i < 40; i++) {
      await userEvent.tab()
      if (!panel()?.contains(document.activeElement)) {
        escaped.push(document.activeElement?.outerHTML.slice(0, 80) ?? 'none')
      }
    }
    for (let i = 0; i < 40; i++) {
      await userEvent.tab({ shift: true })
      if (!panel()?.contains(document.activeElement)) {
        escaped.push(document.activeElement?.outerHTML.slice(0, 80) ?? 'none')
      }
    }
    expect(escaped).toEqual([])
  })
})

describe('every reader panel keeps Tab inside it, with real keys (spec 2026-09-26 §3.3, §3.6)', () => {
  it.each([
    ['Help', '?', 'Keyboard help'],
    ['Credits', 'C', 'Credit pages in raw'],
    ['All notes', 'a', 'All notes'],
    ['the note editor', 'n', 'Note'],
    ['Account', 'A', 'Account'],
  ])('%s: neither Tab nor Shift+Tab leaves it', async (_name, key, label) => {
    await readerFocused(1440, 900)
    await act(async () => {
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key, bubbles: true }),
      )
    })
    const panel = () =>
      document.querySelector(`[role="dialog"][aria-label="${label}"]`)
    // Account's panel draws its body once its keys, its sessions and
    // `/api/about` have been read, so it is waited for until its forms are
    // drawn: the walk then crosses them, and the reads land inside `until`'s
    // `act`.
    await until(
      () =>
        panel() !== null &&
        (label !== 'Account' || panel()?.querySelector('form') !== null),
    )
    const escaped: string[] = []
    for (let i = 0; i < 40; i++) {
      await userEvent.tab()
      if (!panel()?.contains(document.activeElement)) {
        escaped.push(document.activeElement?.outerHTML.slice(0, 80) ?? 'none')
      }
    }
    for (let i = 0; i < 40; i++) {
      await userEvent.tab({ shift: true })
      if (!panel()?.contains(document.activeElement)) {
        escaped.push(document.activeElement?.outerHTML.slice(0, 80) ?? 'none')
      }
    }
    expect(escaped).toEqual([])
  })
})

/**
 * `ReaderFailed`, as a browser draws it and as a keyboard reaches it — O176.
 * `App.test.tsx` pins its markup and the load effect's rules around it; what
 * is asked here is what jsdom has no answer for: whether the three sentences
 * are legible on the reader's ground with Paper pinned, what the
 * accessibility tree makes of them, whether a real `Tab` gets to the
 * button, and — mounted as `Routes.tsx` mounts it — whether it fits the
 * window and what a long message does to it.
 *
 * **The reader's key listener is not subscribed on this screen** — it waits
 * for a session — so the `Tab` below is the browser's own and nothing claims
 * it. That is why the walk starts from a button put in front of the reader
 * rather than from `readerFocused`: there is no page area to focus, and a
 * press before anything in the test iframe has focus goes to the frame.
 */
describe('O176: a work that fails to load, in a browser', () => {
  /**
   * `App.test.tsx`'s fixture of the same name: the plain `Error` `api.ts`'s
   * `getSidecarWithEtag` throws for a 500, with a body of the fixture's own.
   * The screen shows its message.
   */
  const answered500 = () => new Error('loading vol-01: 500 database is locked')

  async function failed(onLeave: () => void = () => {}) {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    onTestFinished(() => logged.mockRestore())
    ports.getSidecarWithEtag.mockRejectedValue(answered500())
    await page.viewport(1440, 900)
    mount(
      <div style={{ position: 'fixed', inset: 0 }}>
        <App work={WORK} onLeave={onLeave} />
      </div>,
    )
    await until(() => document.querySelector('[role="alert"]') !== null)
  }

  const styleOf = (selector: string) =>
    getComputedStyle(document.querySelector(selector) as HTMLElement)

  it('draws its sentences in Ink on black with Paper pinned, filling the viewport', async () => {
    document.documentElement.setAttribute('data-theme', 'paper')
    try {
      await failed()
      expect(styleOf('main').backgroundColor).toBe('rgb(0, 0, 0)')
      expect(getComputedStyle(document.body).backgroundColor).toBe(
        'rgb(0, 0, 0)',
      )
      // Ink's `--text` for the heading and the button, its `--muted` for the
      // message, and its `--surface` under the button's words.
      expect(styleOf('h1').color).toBe('rgb(236, 228, 212)')
      expect(styleOf('[role="alert"]').color).toBe('rgb(166, 155, 135)')
      expect(styleOf('main button').color).toBe('rgb(236, 228, 212)')
      expect(styleOf('main button').backgroundColor).toBe('rgb(33, 30, 24)')
      // `.reader-failed-title`, not the browser's own `h1`: 1.25rem of 16px.
      expect(styleOf('h1').fontSize).toBe('20px')
      // `minHeight: 100%` of the box it is mounted in. Whether the document
      // scrolls is not this case's to say — a `position: fixed` wrapper
      // never lets it — and is asked in `.reader-route` below.
      expect(
        (document.querySelector('main') as HTMLElement).getBoundingClientRect()
          .height,
      ).toBe(900)
      // The control: outside the reader it is Paper's text, so the Ink above
      // is the screen's own doing and not the page's.
      const outside = document.createElement('p')
      document.body.append(outside)
      // Removed whether or not the line below holds, so that it is never
      // left in a later case's document.
      onTestFinished(() => outside.remove())
      expect(getComputedStyle(outside).color).toBe('rgb(35, 30, 23)')
    } finally {
      document.documentElement.removeAttribute('data-theme')
    }
  })

  it('is one landmark, one heading, an alert and a button that says where it goes', async () => {
    await failed()
    expect(page.getByRole('main').elements()).toHaveLength(1)
    expect(page.getByRole('heading').elements()).toHaveLength(1)
    await expect
      .element(page.getByRole('heading', { level: 1 }))
      .toHaveAccessibleName('This work could not be opened')
    expect(
      page
        .getByRole('alert')
        .elements()
        .map((el) => el.textContent),
    ).toEqual(['loading vol-01: 500 database is locked'])
    await expect
      .element(page.getByRole('button'))
      .toHaveAccessibleName('Back to the library')
  })

  it('Tab reaches the button and Enter leaves', async () => {
    const onLeave = vi.fn()
    const before = document.createElement('button')
    before.textContent = 'before the reader'
    document.body.prepend(before)
    onTestFinished(() => before.remove())
    await failed(onLeave)
    before.focus()
    await userEvent.tab()
    expect(focused()).toBe('Back to the library')
    await userEvent.keyboard('{Enter}')
    expect(onLeave).toHaveBeenCalledOnce()
  })

  /**
   * The screen where `Routes.tsx`'s `Reader` puts it: in `.reader-route`, in
   * a host the height of the window, the way `#root` is. The cases above
   * mount in this file's `position: fixed` wrapper, where the document can
   * never scroll and so says nothing about whether this screen fits it.
   *
   * `thrown` is what the sidecar read rejects with, and `work` the slug the
   * reader is mounted for.
   */
  async function failedInItsRoute(thrown: Error, work: string = WORK) {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    onTestFinished(() => logged.mockRestore())
    ports.getSidecarWithEtag.mockRejectedValue(thrown)
    await page.viewport(390, 844)
    const host = mount(
      <div className="reader-route">
        <App work={work} onLeave={() => {}} />
      </div>,
    )
    host.style.height = '100%'
    await until(() => document.querySelector('[role="alert"]') !== null)
    await document.fonts.ready
    return {
      doc: document.documentElement,
      main: document.querySelector('main') as HTMLElement,
      heading: document.querySelector('h1') as HTMLElement,
    }
  }

  it('fills the window in its route and leaves the document nothing to scroll', async () => {
    const { doc, main } = await failedInItsRoute(answered500())
    // The padding is inside the box (`boxSizing`), so `minHeight: 100%` is
    // the window and not the window and 48px.
    expect(main.getBoundingClientRect().height).toBe(844)
    expect([doc.scrollWidth, doc.scrollHeight]).toEqual([
      doc.clientWidth,
      doc.clientHeight,
    ])
  })

  it('grows with a long message instead of spilling out of the window', async () => {
    // What a proxy's error page or a long slug would put here: one run with
    // nowhere to break, and more words than a phone's window holds.
    const { doc, main, heading } = await failedInItsRoute(
      new Error(
        `loading ${'x'.repeat(160)}: 502 ${'the gateway had a great deal to say. '.repeat(60)}`,
      ),
    )
    // `overflowWrap: anywhere` on the message: nothing sideways.
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
    expect(main.scrollWidth).toBeLessThanOrEqual(main.clientWidth)
    // `minHeight`, not `height`: the box grows and the document scrolls to
    // the rest of it, where a box of fixed height would centre what it
    // cannot hold
    // and put the heading above the top, out of scrolling's reach.
    expect(doc.scrollHeight).toBeGreaterThan(doc.clientHeight)
    expect(heading.getBoundingClientRect().top).toBeGreaterThanOrEqual(0)
    expect(main.scrollHeight).toBeLessThanOrEqual(main.clientHeight)
  })

  /**
   * O179's sentence, for the `WorkNotFound` the sidecar read's 404 throws,
   * with a slug of 160 characters and nowhere to break. The slug is in a
   * `<code>` inside the alert, and the `overflowWrap: anywhere` that the case
   * above measures is set on the alert, not on the `<code>`.
   *
   * The reader is mounted for the same slug the error carries.
   */
  it('says the sidecar read’s 404 in its own sentence, the long slug in a code no wider than the screen', async () => {
    const slug = 'x'.repeat(160)
    const { doc, main } = await failedInItsRoute(
      new WorkNotFound(slug, `loading ${slug}: 404 no work \`${slug}\``),
      slug,
    )
    const alerts = page.getByRole('alert').elements()
    expect(alerts.map((el) => el.textContent)).toEqual([
      `The server found no work named ${slug}.`,
    ])
    const codes = [...alerts[0].querySelectorAll('code')]
    expect(codes.map((el) => el.textContent)).toEqual([slug])
    expect(codes[0].getBoundingClientRect().width).toBeLessThanOrEqual(
      main.getBoundingClientRect().width,
    )
    expect(doc.scrollWidth).toBeLessThanOrEqual(doc.clientWidth)
    expect(main.scrollWidth).toBeLessThanOrEqual(main.clientWidth)
  })
})

/**
 * O216: a drag of the strip in the whole reader, and then a key.
 *
 * `Filmstrip.browser.test.tsx` holds that the strip hands focus back when a
 * press becomes a drag. What only the whole reader shows is what that is
 * for. A mouse focuses the tile it presses, `keyplan.ts` leaves Space and
 * Enter to a button that has focus, and a tile that is pressed turns the page
 * to itself: so a key after a drag would turn to the page the drag began on.
 *
 * A real mouse and real keys, on a volume long enough for its strip to
 * overflow, in both reading directions.
 */
describe('O216: a drag of the strip, then a key', () => {
  const LONG = 60

  /**
   * The reader on slot 30 of a volume of `LONG` pages read `reading`, with
   * the bars put up by a tap on the page, which leaves focus on the page.
   */
  async function readerOnALongVolume(reading: 'ltr' | 'rtl') {
    ports.getSidecarWithEtag.mockResolvedValue({
      json: JSON.stringify({
        work: { reading },
        editions: { en: {}, raw: {} },
        hub: 'raw',
        alignments: [{ from: 'raw', to: 'en', quality: { verdict: 'sound' } }],
      }),
      etag: 's1',
    })
    ports.getProgress.mockResolvedValue({ edition: 'raw', slot: 30 })
    ports.workDetail.mockResolvedValue({
      id: 1,
      slug: WORK,
      title: 'Volume One',
      volume: 1,
      aligned: true,
      pages: LONG,
      editions: [
        { key: 'raw', label: 'Raw', members: 1, pages: LONG, missing: 0 },
        { key: 'en', label: 'English', members: 1, pages: LONG, missing: 0 },
      ],
      opens: 'raw',
      alignments: [],
    })
    const pairs: Record<string, number> = {}
    for (let i = 0; i < LONG; i++) {
      pairs[`raw|en|${i}`] = i
      pairs[`en|raw|${i}`] = i
    }
    ports.wasmResolver.mockImplementation(async () =>
      fakeResolver({ raw: whole(LONG), en: whole(LONG) }, pairs),
    )
    await readerFocused(1440, 900)
    const area = box('.page-area')
    tapAt({ x: area.left + area.width / 2, y: area.top + area.height / 2 })
    await until(() => currentTile() !== undefined)
  }

  /** Which tile of the strip is the page the reader is on. */
  const currentTile = () =>
    document.querySelector<HTMLElement>(
      '.reader-strip-rail [aria-current="true"]',
    )?.dataset.group

  for (const reading of ['ltr', 'rtl'] as const) {
    it(`leaves the reader on its page, and Space and Enter the reader's own, ${reading}`, async () => {
      await readerOnALongVolume(reading)
      const rail = document.querySelector('.reader-strip-rail') as HTMLElement
      const tile = rail.querySelector('[data-group="33"]') as HTMLElement
      const stood = tile.getBoundingClientRect()
      const x = Math.round(stood.left + stood.width / 2)
      const y = Math.round(stood.top + stood.height / 2)
      // THE STATE THIS TEST NAMES: page 31 is open, the tile that will be
      // pressed is another page's, and focus is on the page.
      expect(currentTile()).toBe('30')
      expect(focused()).toBe('page')

      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, 150)
        await mouse.release()
        await new Promise((resolve) => requestAnimationFrame(resolve))
      })
      // The strip went with the pointer, and focus is back where it was.
      expect(
        Math.round(tile.getBoundingClientRect().left - stood.left),
      ).toBeGreaterThan(148)
      expect(focused()).toBe('page')

      await act(async () => {
        await userEvent.keyboard(' ')
        await userEvent.keyboard('{Enter}')
      })
      expect(currentTile()).toBe('30')

      // THE CONTROL: a key does reach the reader, and `→` turns one page from
      // the page it was on. A key that had pressed the tile would have turned
      // to page 34 first, and `→` would have turned from there.
      await act(async () => {
        await userEvent.keyboard('{ArrowRight}')
      })
      await until(() => currentTile() !== '30')
      expect(currentTile()).toBe(reading === 'ltr' ? '31' : '29')
    })
  }
})
