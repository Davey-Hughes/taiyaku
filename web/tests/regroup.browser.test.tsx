import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { GateContext, type Gated } from '../src/gate'
import { fakeResolver, whole } from './support/fixtures'
import '../src/index.css'
import { mount } from './support/mount'

/**
 * `b` in two-page mode, with real keys in a real browser (O426): the pair on
 * screen splits at its second slot, so its first page stands alone and
 * pairing restarts at the next; a second `b` from the lone page undoes it,
 * and a press that would change no pair stores nothing.
 *
 * Read off the screen: each cell of the page area, left to right, as its
 * page's alt text, a hidden cell in parentheses.
 *
 * The mocks are `App.browser.test.tsx`'s, for the reasons its header gives.
 * CI does not run this project; the gate does.
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

vi.mock('../src/resolver', () => ({ wasmResolver: ports.wasmResolver }))

vi.mock('../src/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api')>()),
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

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  work: ports.workDetail,
}))

const { default: App } = await import('../src/App')

const WORK = 'vol-01'

/** Twelve pages, so a stored break leaves pairs on both sides of it. */
const PAGES = 12

const REGROUP = 'taiyaku.prefs.regroup'

const administrator: Gated = {
  who: {
    user_id: 1,
    admin: true,
    can_download: true,
    can_stream: true,
    can_stamp: false,
  },
  signOut: () => {},
}

beforeEach(() => {
  localStorage.clear()
  vi.clearAllMocks()
  localStorage.setItem('taiyaku.prefs.layout', JSON.stringify({ mode: 'two' }))
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
  const edition = (key: string, label: string) => ({
    key,
    label,
    members: 1,
    pages: PAGES,
    missing: 0,
    cover: `/api/works/${WORK}/${key}/pages/0/thumb?v=c0ffee-0-1`,
  })
  ports.workDetail.mockResolvedValue({
    id: 1,
    slug: WORK,
    title: 'Volume One',
    volume: 1,
    aligned: true,
    pages: PAGES,
    editions: [edition('raw', 'Raw'), edition('en', 'English')],
    opens: 'raw',
    alignments: [],
    series: null,
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

/**
 * The reader at 1440×900 in two-page mode, read `reading`, on `slot` of
 * `raw`, with `breaks` stored as the reader's own, and keyboard focus on its
 * page: until something in the test frame has focus, a real key goes to the
 * frame.
 */
async function reader(reading: 'ltr' | 'rtl', slot: number, breaks?: number[]) {
  ports.getSidecarWithEtag.mockResolvedValue({
    json: JSON.stringify({
      work: { reading },
      editions: { en: {}, raw: {} },
      hub: 'raw',
      alignments: [{ from: 'raw', to: 'en', quality: { verdict: 'sound' } }],
    }),
    etag: 's1',
  })
  ports.getProgress.mockResolvedValue({ edition: 'raw', slot })
  if (breaks) {
    localStorage.setItem(
      REGROUP,
      JSON.stringify({ breaks: { [WORK]: { raw: breaks } } }),
    )
  }
  await page.viewport(1440, 900)
  mount(
    <MemoryRouter>
      <div style={{ position: 'fixed', inset: 0 }}>
        <GateContext value={administrator}>
          <App work={WORK} onLeave={() => {}} />
        </GateContext>
      </div>
    </MemoryRouter>,
  )
  const deadline = performance.now() + 2000
  while (document.querySelector('[data-reader-page] img') === null) {
    if (performance.now() > deadline) throw new Error('no page drawn')
    await act(async () => {
      await new Promise((r) => setTimeout(r, 10))
    })
  }
  ;(document.querySelector('[data-reader-page]') as HTMLElement).focus()
}

/** The page area's cells, left to right: each one's page, in parentheses when hidden. */
function onScreen(): string {
  const area = document.querySelector('[data-reader-page]') as HTMLElement
  return [...area.querySelectorAll('img')]
    .map((img) => ({ img, left: img.getBoundingClientRect().left }))
    .sort((a, b) => a.left - b.left)
    .map(({ img }) =>
      img.closest('[aria-hidden="true"]') ? `(${img.alt})` : img.alt,
    )
    .join(' | ')
}

/**
 * Up to two seconds for the screen to read `expected`, with nothing in
 * `act`, then the assertion, so a miss shows what is drawn instead.
 */
async function reads(expected: string, step: string) {
  const deadline = performance.now() + 2000
  while (onScreen() !== expected && performance.now() < deadline) {
    await new Promise((r) => setTimeout(r, 10))
  }
  expect(onScreen(), step).toBe(expected)
}

const stored = () => JSON.parse(localStorage.getItem(REGROUP) ?? '{}').breaks

/**
 * Real keys, outside `act`, as a reader presses them: the pairing changes on
 * a preference write and a render, and nothing here waits for it but the
 * screen.
 */
async function outsideAct(run: () => Promise<void>) {
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await run()
  } finally {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  }
}

describe('b realigns two-page pairs from the pair on screen, with real keys (O426)', () => {
  it('left to right: on a pair an arrow reached, its first page stands alone and pairing restarts at the next; back and b again restore it', async () => {
    await reader('ltr', 2)
    await outsideAct(async () => {
      await reads('page 3 | page 4', 'start')
      await userEvent.keyboard('{ArrowRight}')
      await reads(
        'page 5 | page 6',
        'the arrow lands on the pair at slots 4 and 5',
      )
      for (let round = 0; round < 2; round++) {
        await userEvent.keyboard('b')
        await reads('page 5 | (page 5)', `round ${round}: b`)
        expect(stored(), `round ${round}: b`).toEqual({ [WORK]: { raw: [5] } })
        await userEvent.keyboard('{ArrowRight}')
        await reads('page 6 | page 7', `round ${round}: →`)
        await userEvent.keyboard('{ArrowLeft}')
        await reads('page 5 | (page 5)', `round ${round}: ←`)
        await userEvent.keyboard('b')
        await reads('page 5 | page 6', `round ${round}: b again`)
        expect(stored(), `round ${round}: b again`).toEqual({
          [WORK]: { raw: [] },
        })
      }
    })
  })

  it('right to left, the mirror: the first page read stands alone and pairing restarts at the next', async () => {
    await reader('rtl', 2)
    await outsideAct(async () => {
      await reads('page 4 | page 3', 'start')
      await userEvent.keyboard('{ArrowLeft}')
      await reads(
        'page 6 | page 5',
        'the arrow lands on the pair at slots 4 and 5',
      )
      await userEvent.keyboard('b')
      await reads('page 5 | (page 5)', 'b')
      expect(stored(), 'b').toEqual({ [WORK]: { raw: [5] } })
      await userEvent.keyboard('{ArrowLeft}')
      await reads('page 7 | page 6', '←')
      await userEvent.keyboard('{ArrowRight}')
      await reads('page 5 | (page 5)', '→')
      await userEvent.keyboard('b')
      await reads('page 6 | page 5', 'b again')
      expect(stored(), 'b again').toEqual({ [WORK]: { raw: [] } })
    })
  })

  /**
   * A restore, a switch to two-page mode or a swap can leave the reader on
   * a pair's second slot: `b` there stands the pair's first page alone and
   * keeps it on screen, and a second `b` undoes it.
   */
  it('from a pair’s second slot, a restored place, b stands the pair’s first page alone, and b again undoes it', async () => {
    await reader('ltr', 5)
    await outsideAct(async () => {
      await reads('page 5 | page 6', 'start, on the pair at slots 4 and 5')
      await userEvent.keyboard('b')
      await reads('page 5 | (page 5)', 'b')
      expect(stored(), 'b').toEqual({ [WORK]: { raw: [5] } })
      await userEvent.keyboard('b')
      await reads('page 5 | page 6', 'b again')
      expect(stored(), 'b again').toEqual({ [WORK]: { raw: [] } })
    })
  })

  it('a pair at an odd slot realigns at its own second slot', async () => {
    // A break at 5: `(0,1), (2,3), 4, (5,6), (7,8), (9,10), 11`.
    await reader('ltr', 7, [5])
    await outsideAct(async () => {
      await reads('page 8 | page 9', 'start')
      await userEvent.keyboard('b')
      await reads('page 8 | (page 8)', 'b')
      expect(stored()).toEqual({ [WORK]: { raw: [5, 8] } })
    })
  })

  it('at the last page standing alone, b stores nothing', async () => {
    await reader('ltr', 11, [5])
    await outsideAct(async () => {
      await reads('page 12 | (page 12)', 'start')
      const before = localStorage.getItem(REGROUP)
      await userEvent.keyboard('b')
      await new Promise((r) => setTimeout(r, 200))
      expect(localStorage.getItem(REGROUP)).toBe(before)
      expect(onScreen()).toBe('page 12 | (page 12)')
    })
  })
})
