import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import { BrowsePrefsContext, browseModule } from '../src/browseModule'
import { GateContext } from '../src/gate'
import * as library from '../src/library'
import { Series } from '../src/Series'
import { settleUntil } from './support/async'
import { mount } from './support/mount'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))

/** 96 characters: open, the Title box hangs over the next tile. */
const LONG_TITLE =
  'Kagami no Kuni no Harisugawa: The Complete Digital Edition, Volume 01 (Collector’s Box Set, Ed.)'

const work = (id: number, title: string): library.WorkCard => ({
  id,
  slug: `kagami-v0${id}`,
  title,
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
})

const WORKS = [
  work(1, LONG_TITLE),
  work(2, 'Volume 2'),
  work(3, 'Volume 3'),
  work(4, 'Volume 4'),
]

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(library.oneSeries).mockResolvedValue({
    series: {
      id: 7,
      title: 'Kagami no Kuni no Harisugawa',
      works: WORKS.length,
      editions: WORKS.length,
      aligned: 0,
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

/**
 * The Series page in `on-select` mode, §3's panel that follows the pointer
 * and the focus, for an admin at 1280 px, in `.library > main` as the shell
 * draws it; the first volume's panel revealed by a pointer on its tile.
 */
async function firstPanelOpen() {
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(1280, 800)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <BrowsePrefsContext.Provider
        value={{ ...browseModule.defaults, panel: 'on-select' }}
      >
        <GateContext.Provider
          value={{
            who: {
              user_id: 1,
              admin: true,
              can_download: false,
              can_stream: true,
              can_stamp: false,
            },
            signOut: () => {},
          }}
        >
          <div className="library">
            <div className="sidebar-column" />
            <main>
              <MemoryRouter initialEntries={['/series/7']}>
                <Routes>
                  <Route path="/series/:id" element={<Series />} />
                </Routes>
              </MemoryRouter>
            </main>
          </div>
        </GateContext.Provider>
      </BrowsePrefsContext.Provider>,
    )
  })
  const shown = host as HTMLElement
  const tiles = () => [...shown.querySelectorAll('.volumes > li')]
  await settleUntil('the volumes', () => tiles().length === 4, 1000)
  await document.fonts.ready
  await act(async () => {
    await userEvent.hover(tiles()[0].querySelector('a') as Element)
  })
  await settleUntil(
    'the first volume’s editor',
    () => tiles()[0].querySelector('.metadata-editor') != null,
    1000,
  )
  const box = tiles()[0].querySelector<HTMLTextAreaElement>("[name='title']")
  if (box === null) throw new Error('no title box')
  return { host: shown, tiles, box }
}

/** Whether a pointer has come onto `el`: the browser's own `mouseenter`. */
const entered = (el: Element) => {
  const seen = { yes: false }
  el.addEventListener('mouseenter', () => {
    seen.yes = true
  })
  return seen
}

/**
 * Click into the open first volume's Title box, type, and rest the pointer
 * on the part of the open box that hangs over the second tile.
 */
async function typedWithPointerOnOverhang() {
  const opened = await firstPanelOpen()
  const { tiles, box } = opened
  await act(async () => {
    await userEvent.click(box)
  })
  await act(async () => {
    await userEvent.keyboard('{Control>}{End}{/Control} (typed)')
  })
  const next = tiles()[1].getBoundingClientRect()
  const open = box.getBoundingClientRect()
  const at = { x: next.left + next.width / 2, y: open.top + open.height / 2 }
  // The fixture tells: the point is on the open box, and over the second
  // tile.
  expect(open.right).toBeGreaterThan(at.x)
  expect(document.elementFromPoint(at.x, at.y)).toBe(box)
  await act(async () => {
    await userEvent.hover(box, {
      position: { x: at.x - open.left, y: at.y - open.top },
    })
  })
  return { ...opened, seen: entered(tiles()[1]) }
}

describe('the Series page’s on-select panel while its Title box is edited (O274)', () => {
  it('keeps the panel, the focus and the restored title on Escape with the pointer on the box’s overhang', async () => {
    const { tiles, box, seen } = await typedWithPointerOnOverhang()
    expect(box.value).toBe(`${LONG_TITLE} (typed)`)

    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    // The fixture tells: closed, the box uncovers the second tile under the
    // pointer, and the browser says the pointer came onto it.
    await settleUntil('the pointer on the second tile', () => seen.yes, 1000)

    expect({
      editorIn: tiles().map(
        (tile) => tile.querySelector('.metadata-editor') != null,
      ),
      focused: document.activeElement === box,
      value: box.isConnected ? box.value : 'unmounted',
    }).toEqual({
      editorIn: [true, false, false, false],
      focused: true,
      value: LONG_TITLE,
    })
  })

  it('keeps the panel and the typed title on Tab to Save with the pointer on the box’s overhang', async () => {
    const { tiles, box, seen } = await typedWithPointerOnOverhang()

    await act(async () => {
      await userEvent.tab()
    })
    await settleUntil('the pointer on the second tile', () => seen.yes, 1000)

    const save = tiles()[0].querySelector(
      ".metadata-editor button[type='submit']",
    )
    expect({
      editorIn: tiles().map(
        (tile) => tile.querySelector('.metadata-editor') != null,
      ),
      onSave: save !== null && document.activeElement === save,
      value: box.isConnected ? box.value : 'unmounted',
    }).toEqual({
      editorIn: [true, false, false, false],
      onSave: true,
      value: `${LONG_TITLE} (typed)`,
    })
  })

  it('reveals the volume under the pointer again once focus has left the open panel', async () => {
    const { host, tiles, box } = await firstPanelOpen()
    await act(async () => {
      await userEvent.click(box)
    })
    const third = tiles()[2].querySelector('a') as Element
    const seen = entered(tiles()[2])
    await act(async () => {
      await userEvent.hover(third)
    })
    await settleUntil('the pointer on the third tile', () => seen.yes, 1000)
    // While focus is in the panel, the pointer reveals nothing.
    expect(tiles()[2].querySelector('.alignment-panel')).toBeNull()

    // Focus leaves the panel for the page, then the pointer comes back.
    await act(async () => {
      await userEvent.click(host.querySelector('h1') as Element)
    })
    expect(document.activeElement).toBe(document.body)
    await act(async () => {
      await userEvent.hover(third)
    })
    await settleUntil(
      'the third volume’s panel',
      () => tiles()[2].querySelector('.metadata-editor') != null,
      1000,
    )
    expect(tiles()[0].querySelector('.metadata-editor')).toBeNull()
  })
})
