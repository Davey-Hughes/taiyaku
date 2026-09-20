import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import { fakeResolver, whole } from './fixtures'
import './index.css'
import { mount } from './testing/mount'

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
