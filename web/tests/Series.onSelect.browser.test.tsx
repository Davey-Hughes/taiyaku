import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import * as metadata from '../src/metadata'
import { deferred, settleUntil } from './support/async'
import { mouse } from './support/mouse'
import { pointerRulesAs } from './support/pointer'
import { seriesPage } from './support/seriesPage'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))
vi.mock('../src/metadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/metadata')>()),
  patchWork: vi.fn(),
}))

/** 96 characters, as long as a series title with its edition runs. */
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

/** Ten volumes: two rows at 1280 px, so the first panel lies over a tile. */
const WORKS = [
  work(1, LONG_TITLE),
  ...Array.from({ length: 9 }, (_, n) => work(n + 2, `Volume ${n + 2}`)),
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
      unread: WORKS.length,
      cover: null,
      titles: [],
    },
    works: WORKS,
  })
  vi.mocked(library.work).mockImplementation(async (slug) => ({
    ...(WORKS.find((w) => w.slug === slug) as library.WorkCard),
    alignments: [],
  }))
  // Reset, not only cleared: a once-answer a case left unused would
  // otherwise answer the next case's first call.
  vi.mocked(metadata.patchWork).mockReset()
  vi.mocked(metadata.patchWork).mockImplementation(async (_, fields) =>
    stored(fields.title ?? ''),
  )
})

/** The answer to a title save that stored `title`. */
const stored = (title: string): metadata.PatchResult => ({
  taiyaku_library: 1,
  updated: '2026-11-14T00:00:00Z',
  title,
  persisted: true,
})

/** A real input, the React updates it causes drawn inside `act`. */
const real = (input: () => Promise<unknown>) =>
  act(async () => {
    await input()
  })

const rect = (el: Element | null | undefined) => {
  if (el == null) throw new Error('nothing drawn')
  return el.getBoundingClientRect()
}

/**
 * The Series page in `on-select` mode, §3's panel that follows the pointer
 * and the focus, for an admin at 1280 px, in `.library > main` as the shell
 * draws it; the first volume's panel revealed by a pointer on its tile,
 * and `under`, a tile of the next row that the panel lies over.
 */
async function firstPanelOpen() {
  const shown = await seriesPage({
    width: 1280,
    panel: 'on-select',
    who: {
      user_id: 1,
      admin: true,
      can_download: false,
      can_stream: true,
      can_stamp: false,
    },
  })
  const tiles = () => [...shown.querySelectorAll('.volumes > li')]
  await settleUntil('the volumes', () => tiles().length === WORKS.length, 1000)
  await document.fonts.ready
  await real(() => userEvent.hover(tiles()[0].querySelector('a') as Element))
  await settleUntil(
    'the first volume’s editor',
    () => tiles()[0].querySelector('.metadata-editor') != null,
    1000,
  )
  const box = tiles()[0].querySelector<HTMLTextAreaElement>("[name='title']")
  if (box === null) throw new Error('no title box')
  const panel = rect(tiles()[0].querySelector('.tile-panel'))
  const under = tiles().find((tile) => {
    const at = rect(tile)
    return (
      at.top > rect(tiles()[0]).bottom &&
      at.left < panel.right &&
      at.right > panel.left &&
      at.top < panel.bottom
    )
  })
  // The fixture tells: a tile of the next row lies under the first panel.
  expect(under, 'a tile under the first panel').toBeDefined()
  return { host: shown, tiles, box, under: under as Element }
}

/** Which tiles draw an editor, in order. */
const editorIn = (tiles: Element[]) =>
  tiles.map((tile) => tile.querySelector('.metadata-editor') !== null)

/** Whether a pointer has come onto `el`: the browser's own `mouseenter`. */
const entered = (el: Element) => {
  const seen = { yes: false }
  el.addEventListener('mouseenter', () => {
    seen.yes = true
  })
  return seen
}

describe('the Series page’s on-select panel while its Title field is edited (O283)', () => {
  it('keeps the panel, the focus and the restored title on the first Escape in the Title field, with the pointer resting on the panel over the next row', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles, box, under } = await firstPanelOpen()
    await real(() => userEvent.click(box))
    await real(() => userEvent.keyboard('{Control>}{End}{/Control} (2)'))
    expect(box.value).toBe(`${LONG_TITLE} (2)`)

    // The pointer onto the middle of where the panel and the lower tile
    // meet.
    const panel = tiles()[0].querySelector('.tile-panel')
    const over = rect(panel)
    const tile = rect(under)
    const x =
      (Math.max(over.left, tile.left) + Math.min(over.right, tile.right)) / 2
    const y =
      (Math.max(over.top, tile.top) + Math.min(over.bottom, tile.bottom)) / 2
    await real(() => mouse.moveTo(x, y))
    // The fixture tells: the point is in the panel and in the lower tile.
    expect({
      inTile:
        x > tile.left && x < tile.right && y > tile.top && y < tile.bottom,
      onPanel:
        document.elementFromPoint(x, y)?.closest('.tile-panel') === panel,
    }).toEqual({ inTile: true, onPanel: true })

    await real(() => userEvent.keyboard('{Escape}'))
    expect({
      editors: editorIn(tiles()),
      focused: document.activeElement === box,
      value: box.value,
    }).toEqual({
      editors: tiles().map((_, n) => n === 0),
      focused: true,
      value: LONG_TITLE,
    })
  })

  it('saves the typed title, then moves the panel, on Tab from the Title field to the next tile’s link', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { tiles, box } = await firstPanelOpen()
    await real(() => userEvent.click(box))
    await real(() => userEvent.keyboard('{Control>}{End}{/Control} (2)'))
    // Off every tile, so that only focus chooses the panel.
    await real(() => mouse.moveTo(0, 0))

    await real(() => userEvent.keyboard('{Tab}'))
    const next = tiles()[1].querySelector(':scope > a')
    expect({
      focused: document.activeElement === next,
      editors: editorIn(tiles()),
      calls: vi.mocked(metadata.patchWork).mock.calls,
    }).toEqual({
      focused: true,
      editors: tiles().map((_, n) => n === 0),
      calls: [['kagami-v01', { title: `${LONG_TITLE} (2)` }]],
    })

    await real(async () => {
      save.resolve(stored(`${LONG_TITLE} (2)`))
    })
    await settleUntil(
      'the first editor gone or the second drawn',
      () => !editorIn(tiles())[0] || editorIn(tiles())[1],
      1000,
    )
    expect({
      editors: editorIn(tiles()),
      calls: vi.mocked(metadata.patchWork).mock.calls.length,
    }).toEqual({ editors: tiles().map((_, n) => n === 1), calls: 1 })
  })

  it('reveals the volume under the pointer again once a press outside has closed the panel', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, tiles, box } = await firstPanelOpen()
    await real(() => userEvent.click(box))
    const third = tiles()[2].querySelector('a') as Element
    const seen = entered(tiles()[2])
    await real(() => userEvent.hover(third))
    await settleUntil('the pointer on the third tile', () => seen.yes, 1000)
    // While focus is in the panel, the pointer reveals nothing.
    expect(tiles()[2].querySelector('.alignment-panel')).toBeNull()

    // A press outside closes the panel, then the pointer comes back.
    await real(() => userEvent.click(host.querySelector('h1') as Element))
    expect(editorIn(tiles()), 'after the press').toEqual(
      tiles().map(() => false),
    )
    await real(() => userEvent.hover(third))
    await settleUntil(
      'the third volume’s panel',
      () => tiles()[2].querySelector('.metadata-editor') != null,
      1000,
    )
    expect(tiles()[0].querySelector('.metadata-editor')).toBeNull()
  })
})
