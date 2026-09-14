import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import { fakeResolver, whole } from './fixtures'
import './index.css'
import { mount } from './testing/mount'

/**
 * The reader's chrome and the reader's own toolbar, laid out together — the
 * only test that can observe O65.
 *
 * `Chrome` positions itself against `App.tsx`'s root, and the layout-mode
 * buttons it collided with are `App.tsx`'s own, in normal flow. So the
 * collision belongs to neither component's test: `Chrome.test.tsx` never
 * mounts the toolbar, and `App.test.tsx` runs in jsdom, where
 * `getBoundingClientRect` is all zeroes, so no box can be told from another.
 * The browser pass that found it (`docs/notes.md`, "O65") asked
 * `document.elementFromPoint` what sat at each mode button's centre with the
 * chrome open; this asks the same question of the mounted reader.
 *
 * The mocks are `App.test.tsx`'s, for the reasons that file's header gives:
 * `vi.mock('./resolver')` is what lets `App.tsx` load at all without the
 * generated wasm package. Only the network calls are replaced in `./api`, so
 * the `<img src>`s are the real ones. Unless a server is listening behind
 * vite's dev proxy, they fail there and the proxy prints `ECONNREFUSED` to
 * stderr; `docs/notes.md` has the same measurement taken against real pages.
 *
 * CI does not run this project (`vite.config.ts`: no Chromium), so this is not
 * a gate. `Chrome.test.tsx` pins the declared geometry for the gate; this is
 * the measurement, kept runnable.
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
 * The reader at `width`×`height`, loaded, with the chrome shown. The wrapper
 * fills the viewport the way `#root` does under `index.css`, so `App`'s
 * `height: 100%` column — and the chrome docked to its bottom — lays out at
 * the viewport's size rather than at an unsized host's content height.
 */
async function readerWithChrome(width: number, height: number) {
  await page.viewport(width, height)
  mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <App work={WORK} onLeave={() => {}} />
    </div>,
  )
  await until(() => button('scroll') !== undefined)
  act(() => {
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Tab', bubbles: true }),
    )
  })
  await until(() => button('Library') !== undefined)
}

// 1440×900 is the browser pass's own size; 768×1024 is a portrait tablet's,
// where the strip is narrower and the viewport taller.
const SIZES: [number, number][] = [
  [1440, 900],
  [768, 1024],
]

describe('the reader chrome over the toolbar, in a browser', () => {
  it.each(SIZES)(
    'at %i×%i, each layout button is what a click at its centre reaches',
    async (width, height) => {
      await readerWithChrome(width, height)
      for (const mode of ['single', 'two', 'scroll']) {
        expect(hitAtCentreOf(button(mode))?.textContent, mode).toBe(mode)
      }
    },
  )

  it.each(SIZES)(
    'at %i×%i, the Library button overlaps no other control and is reachable itself',
    async (width, height) => {
      await readerWithChrome(width, height)
      const leave = button('Library')
      expect(hitAtCentreOf(leave)).toBe(leave)
      const box = leave.getBoundingClientRect()
      const covered = [...document.querySelectorAll('button')]
        .filter((b) => b !== leave)
        .filter((b) => overlaps(box, b.getBoundingClientRect()))
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
})
