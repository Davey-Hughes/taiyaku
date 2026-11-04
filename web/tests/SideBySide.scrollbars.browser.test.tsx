import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as backup from '../src/backup'
import {
  accountPath,
  dirPath,
  LIBRARIES,
  libraryPath,
  NEW_ACCOUNT,
  NEW_LIBRARY,
  USERS,
} from '../src/configAddress'
import * as rules from '../src/rules'
import * as splitApi from '../src/splitEditions'
import {
  accounts,
  answerPairs,
  configAt,
  manga,
  need,
  needed,
  resize,
} from './support/sideBySide'

/**
 * Side by side (spec §3) beside a scrollbar that takes its width, as a
 * classic one does. The pair can be taller than the window where the page
 * alone is not, so a scrollbar comes with the pair and goes with the page;
 * within one scrollbar's width above the breakpoint, the width it takes
 * would put the split under the breakpoint, and the width it gives back
 * over it again. `settings.css` keeps the scroller's gutter whether or not
 * it scrolls, so the layout is decided once.
 *
 * **Run only in the launch that shows scrollbars**, `vite.config.ts`'s
 * second `browser` instance: Playwright's headless Chromium hides them
 * otherwise, and a scrollbar of no width cannot narrow anything. Each case
 * first asserts that a scrollbar here takes width.
 *
 * Its own file rather than a case of `SideBySide.browser.test.tsx`, which
 * runs in the other launch; the two share `support/sideBySide.tsx`.
 *
 * Every Configuration screen keeps the same gutter, the ones that draw no
 * split among them, so that going from one tab or page to another moves
 * nothing: the tabs' underline ends where it ended.
 */

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  scanStatus: vi.fn(),
  listRoots: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))
vi.mock('../src/rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
}))
vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
}))
vi.mock('../src/splitEditions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/splitEditions')>()),
  listSplitEditions: vi.fn(),
}))
// The build's label at the sidebar's foot asks for the build; answered here,
// so the request never leaves the test.
vi.mock('../src/about', async (importOriginal) => {
  const { ABOUT } = await import('./support/about')
  return {
    ...(await importOriginal<typeof import('../src/about')>()),
    about: vi.fn().mockResolvedValue(ABOUT),
  }
})

/** Accounts enough that the table is taller than the window. */
const ACCOUNTS = accounts(40)

/** Directories enough that the library's page is taller than the window. */
const MANGA = manga(40)

/** A path with no space or hyphen in it, longer than any box is wide. */
const LONG = `/media/${'AkagaminoShirayukihime'.repeat(12)}/v01.cbz`

/** One split edition, whose file and directory are both `LONG`. */
const SPLIT: splitApi.SplitEdition = {
  work: 'shirayuki',
  title: 'Shirayuki',
  edition: 'en',
  pages: 9,
  members: [
    {
      path: LONG,
      pages: 4,
      directories: [{ root: MANGA.dirs[2].id, path: LONG, library: MANGA.id }],
    },
    { path: 'v02.cbz', pages: 5, directories: [] },
  ],
  libraries: [{ id: MANGA.id, name: MANGA.name, members: 1, pages: 4 }],
}

beforeEach(() => {
  vi.clearAllMocks()
  answerPairs(ACCOUNTS, [MANGA])
  vi.mocked(rules.listRules).mockResolvedValue({ rules: [], complaints: [] })
  vi.mocked(rules.previewRules).mockResolvedValue({
    impact: {
      series_before: 0,
      series_after: 0,
      series_merged: 0,
      works_rehomed: 0,
      works_orphaned: 0,
      proposals_reopened: 0,
      proposals_raised: 0,
    },
    rows: [],
    rows_truncated: false,
    overrides: [],
  })
  vi.mocked(backup.listBackups).mockResolvedValue({ backups: [] })
  vi.mocked(splitApi.listSplitEditions).mockResolvedValue({ editions: [SPLIT] })
})

/** The width a scrollbar takes in this launch: 0 where scrollbars are hidden. */
function scrollbarWidth(): number {
  const probe = document.createElement('div')
  probe.style.cssText =
    'position: absolute; width: 100px; height: 50px; overflow: scroll'
  document.body.append(probe)
  const width = probe.offsetWidth - probe.clientWidth
  probe.remove()
  return width
}

/**
 * The width `scroller` gives its scrollbar or the gutter kept for one now:
 * its own box less its borders and its content's room, or, for the
 * document, the window less the body's width. Not the root's `clientWidth`,
 * which leaves out a scrollbar but not a gutter kept with no scrollbar in
 * it: Chromium gives the window's whole width there.
 */
function gutterOf(scroller: HTMLElement): number {
  if (scroller === document.documentElement) {
    return window.innerWidth - document.body.getBoundingClientRect().width
  }
  const style = getComputedStyle(scroller)
  return (
    scroller.offsetWidth -
    scroller.clientWidth -
    Number.parseFloat(style.borderLeftWidth) -
    Number.parseFloat(style.borderRightWidth)
  )
}

/**
 * Each pair, and the window's height it is held at: the page alone is
 * shorter than it, and the list taller.
 */
const PAIRS = [
  {
    opens: 'an account',
    from: 'the Users table',
    entry: accountPath(ACCOUNTS[2].name),
    height: 1300,
  },
  {
    opens: 'a directory',
    from: 'its library’s page',
    entry: dirPath(MANGA.name, MANGA.dirs[2].id),
    height: 1000,
  },
] as const

/**
 * Where the scrollbar is: `main`, at a desktop width, and the document under
 * 900px, where `main` does not scroll (`shell.css`). A root font size of
 * 12px brings the breakpoint, which is in rem, under 900px.
 */
const SCROLLERS = [
  { scroller: 'main', rootFont: '', from: 1300 },
  { scroller: 'the document', rootFont: '12px', from: 850 },
] as const

/** Each frame's layout for `ms`: the split's class, and whether a list is drawn. */
async function watch(split: HTMLElement, ms: number) {
  const frames: string[] = []
  const records: MutationRecord[] = []
  const observer = new MutationObserver((found) => records.push(...found))
  observer.observe(split, {
    attributes: true,
    attributeFilter: ['class'],
    childList: true,
  })
  // As the app runs, outside `act` and with React told so: a render the
  // layout starts is drawn when the browser would draw it, rather than held
  // until an `act` ends.
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  try {
    const until = performance.now() + ms
    while (performance.now() < until) {
      await new Promise((r) => requestAnimationFrame(r))
      frames.push(
        `${split.className} ${split.querySelector(':scope > .split-list') === null ? 'alone' : 'with its list'}`,
      )
    }
  } finally {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
    observer.disconnect()
  }
  return { frames, records }
}

describe('within a scrollbar’s width above the breakpoint, the layout holds', () => {
  for (const { scroller, rootFont, from } of SCROLLERS) {
    for (const { opens, from: list, entry, height } of PAIRS) {
      it(`draws ${opens} the same way for a second, with ${list} taller than the window and the page shorter, ${scroller} scrolling`, async () => {
        document.documentElement.style.fontSize = rootFont
        onTestFinished(() => {
          document.documentElement.style.fontSize = ''
        })
        const host = await configAt(from, height, entry)
        const bar = scrollbarWidth()
        expect(bar).toBeGreaterThan(0)
        const scrollerEl =
          scroller === 'main' ? need(host, 'main') : document.documentElement
        const split = need(host, '.split')
        // What the split would have with no scrollbar beside it, here and
        // at the window this case is held at.
        const room = () => split.clientWidth + gutterOf(scrollerEl)
        await resize(from - (room() - needed()) + Math.ceil(bar / 2), height)
        const over = room() - needed()
        expect(over).toBeGreaterThanOrEqual(0)
        expect(over).toBeLessThan(bar)
        // The gutter is kept, so the split is under the breakpoint by what
        // the scrollbar would take, scrolling or not.
        expect(gutterOf(scrollerEl)).toBe(bar)

        const { frames, records } = await watch(split, 1000)
        expect(frames.length).toBeGreaterThan(30)
        const changes = frames.filter((f, i) => i > 0 && f !== frames[i - 1])
        expect(changes.length).toBe(0)
        expect(frames[0]).toBe('split alone')
        expect(records.length).toBe(0)

        // The page alone is shorter than the window: nothing scrolls.
        need(host, '.split-detail')
        expect(scrollerEl.scrollHeight).toBeLessThanOrEqual(
          scrollerEl.clientHeight,
        )
        // One scrollbar's width wider, the pair is drawn, and the list makes
        // it taller than the window.
        await resize(window.innerWidth + bar, height)
        const column = need(host, '.split-pair > .split-list')
        expect(column.scrollHeight).toBeGreaterThan(window.innerHeight)
        expect(scrollerEl.scrollHeight).toBeGreaterThan(scrollerEl.clientHeight)
      })
    }
  }
})

/**
 * Each Configuration tab and the pages under it. Libraries, New library,
 * Rules, Backup and Split editions are shorter than the window and draw no
 * split; the Users table, with forty accounts, is taller. Split editions
 * holds `LONG`, which its box is too narrow for.
 */
const SCREENS = [
  LIBRARIES,
  NEW_LIBRARY,
  libraryPath(MANGA.name),
  dirPath(MANGA.name, MANGA.dirs[2].id),
  USERS,
  NEW_ACCOUNT,
  accountPath(ACCOUNTS[2].name),
  '/config/rules',
  '/config/backup',
  '/config/split-editions',
]

describe('every Configuration screen keeps the gutter the pair keeps', () => {
  // `main` scrolls at 1920, and the document at 390 (`shell.css`).
  for (const width of [1920, 390]) {
    it.each(SCREENS)(
      `ends the tabs’ underline a scrollbar’s width short of the window’s edge on %s at ${width} px`,
      async (entry) => {
        const host = await configAt(width, 1200, entry)
        const bar = scrollbarWidth()
        expect(bar).toBeGreaterThan(0)
        const main = need(host, 'main')
        const underline = need(host, '.config-nav ul').getBoundingClientRect()
        const padding = Number.parseFloat(getComputedStyle(main).paddingRight)
        expect(underline.right).toBeCloseTo(
          window.innerWidth - bar - padding,
          0,
        )
      },
    )
  }
})

describe('a split edition’s long path', () => {
  // The gutter holds whether or not the path breaks, so the box is what
  // shows it does: a path that ran on would widen what scrolls past it.
  for (const width of [1920, 390]) {
    it(`breaks inside its box, the file and its directory, at ${width} px`, async () => {
      const host = await configAt(width, 1200, '/config/split-editions')
      const box = need(host, '.split-editions .settings-box')
      const paths = [
        need(box, '.split-edition-file code'),
        need(box, '.split-edition-dirs a code'),
      ]
      expect(paths.map((path) => path.textContent)).toEqual([LONG, LONG])
      for (const path of paths) {
        expect(path.getBoundingClientRect().height).toBeGreaterThan(
          2 * Number.parseFloat(getComputedStyle(path).lineHeight),
        )
      }
      expect(box.scrollWidth).toBeLessThanOrEqual(box.clientWidth)
    })
  }
})
