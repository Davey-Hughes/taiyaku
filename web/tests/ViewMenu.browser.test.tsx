import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
import * as aboutApi from '../src/about'
import '../src/index.css'
import * as library from '../src/library'
import { ABOUT } from './support/about'
import { tokenColour } from './support/colour'
import { homePage, storedBrowse } from './support/homePage'
import { afterMotion } from './support/motion'
import { mouse } from './support/mouse'
import { coarseRulesOn } from './support/pointer'

/**
 * The home page's View panel, its cover-size slider (spec O285 items 5 to 8)
 * and its rows (items 11 and 12), with real keys and a real mouse: jsdom lays
 * nothing out, draws no pseudo-element, selects no words and moves no range
 * under a pointer, and these are what the owner tried the panel by.
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
}))

const series = (id: number): library.SeriesCard => ({
  id,
  title: `Series ${id}`,
  works: 3,
  editions: 3,
  aligned: 0,
  added: id,
  unread: 3,
  cover: `/api/works/series-${id}-v01/raw/pages/0/thumb?v=c0ffee-0-1`,
})

/** One volume in progress, for a home page that draws its Continue strip. */
const inProgress: library.WorkCard = {
  id: 101,
  slug: 'series-1-v02',
  title: 'Series 1 v02',
  volume: 2,
  aligned: false,
  pages: 180,
  editions: [
    {
      key: 'raw',
      label: 'Raw',
      members: 1,
      pages: 180,
      missing: 0,
      cover: '/api/works/series-1-v02/raw/pages/0/thumb?v=c0ffee-0-1',
    },
  ],
  opens: 'raw',
}

beforeEach(() => {
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
    series: Array.from({ length: 12 }, (_, i) => series(i + 1)),
    unfiled: [],
  })
})

function need<T extends Element = HTMLElement>(
  scope: ParentNode,
  selector: string,
): T {
  const found = scope.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/** The home page at 1280 px, its View panel open. */
async function openView(coverSize?: number) {
  const host = await homePage({ width: 1280, coverSize })
  await expect.poll(() => host.querySelector('.grid > li')).not.toBeNull()
  await act(async () => {
    await userEvent.click(need(host, '.view-button'))
  })
  const panel = need(host, '.view-panel')
  expect(panel.checkVisibility()).toBe(true)
  return { host, panel }
}

/** The home page at 1280 px, its View panel open on the cover-size slider. */
async function slider(coverSize?: number) {
  const { host, panel } = await openView(coverSize)
  return {
    panel,
    range: need<HTMLInputElement>(panel, '.field-slider input[type="range"]'),
    output: need(panel, '.field-slider output'),
    track: need(panel, '.field-slider-track'),
    label: need(panel, '.field-slider-head label'),
    tileWidth: () => need(host, '.grid > li').getBoundingClientRect().width,
  }
}

const middleOf = (el: Element) => {
  const box = el.getBoundingClientRect()
  return { x: box.left + box.width / 2, y: box.top + box.height / 2 }
}

/** The middle mark, the track's `::after`, as the window has it. */
const markOf = (track: Element) => {
  const mark = getComputedStyle(track, '::after')
  const box = track.getBoundingClientRect()
  return {
    mark,
    centre: box.left + Number.parseFloat(mark.left) + 0.5,
    top: box.top + Number.parseFloat(mark.top),
  }
}

describe('the cover-size slider in the View panel', () => {
  it('moves one position per arrow key', async () => {
    const { range, output } = await slider(144)
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => range.focus())
    await act(async () => {
      await userEvent.keyboard('{ArrowRight}')
    })
    expect(output.textContent).toBe('146 px')
    await expect.poll(() => storedBrowse().coverSize).toBe(146)
    await act(async () => {
      await userEvent.keyboard('{ArrowLeft}{ArrowLeft}')
    })
    expect(output.textContent).toBe('143 px')
    await expect.poll(() => storedBrowse().coverSize).toBe(143)
  })

  it.each([
    ['the thumb', 240, (width: number) => width - 4],
    ['the track', 96, (width: number) => (width * 3) / 4],
  ])('resets to 144 on a double-click on %s', async (_, from, x) => {
    const { range, output, tileWidth } = await slider(from)
    expect(output.textContent).toBe(`${from} px`)
    const { width, height } = range.getBoundingClientRect()
    await act(async () => {
      await userEvent.dblClick(range, {
        position: { x: x(width), y: height / 2 },
      })
    })
    expect(output.textContent).toBe('144 px')
    await expect.poll(() => storedBrowse().coverSize).toBe(144)
    await afterMotion()
    expect(tileWidth()).toBe(144)
  })

  it('sets the name and the size on one row and the slider the panel’s width under them', async () => {
    const { panel, range, output, label } = await slider()
    expect(Math.abs(middleOf(label).y - middleOf(output).y)).toBeLessThan(1)
    expect(output.getBoundingClientRect().left).toBeGreaterThan(
      label.getBoundingClientRect().right,
    )
    expect(range.getBoundingClientRect().top).toBeGreaterThanOrEqual(
      Math.max(
        label.getBoundingClientRect().bottom,
        output.getBoundingClientRect().bottom,
      ),
    )
    const style = getComputedStyle(panel)
    const content =
      panel.getBoundingClientRect().width -
      ['borderLeftWidth', 'borderRightWidth', 'paddingLeft', 'paddingRight']
        .map((side) => Number.parseFloat(style[side as 'paddingLeft']))
        .reduce((sum, one) => sum + one, 0)
    expect(range.getBoundingClientRect().width).toBeCloseTo(content, 2)
  })

  it('marks the middle under the thumb at 144', async () => {
    const { range, output, track } = await slider(96)
    const { mark, centre, top } = markOf(track)
    expect([mark.width, mark.height]).toEqual(['1px', '6px'])
    expect(mark.backgroundColor).toBe(tokenColour(track, '--line-strong'))
    expect(Math.abs(centre - middleOf(range).x)).toBeLessThanOrEqual(0.5)
    // This Chromium's box under a fine pointer is the thumb's own height, so
    // below the box is below the thumb.
    expect(top).toBeGreaterThanOrEqual(range.getBoundingClientRect().bottom)
    expect(top + 6).toBeLessThanOrEqual(track.getBoundingClientRect().bottom)
    await act(async () => {
      await mouse.moveTo(centre, middleOf(range).y)
      await mouse.press()
      await mouse.release()
    })
    expect(range.value).toBe('48')
    expect(output.textContent).toBe('144 px')
  })

  it('keeps the mark as near the track under a coarse pointer as under a fine one', async () => {
    const { range, track } = await slider()
    const below = () => markOf(track).top - middleOf(range).y
    const fine = below()
    onTestFinished(coarseRulesOn())
    // The switch took: the coarse block's floor is what moves the box.
    expect(range.getBoundingClientRect().height).toBe(44)
    expect(Math.abs(below() - fine)).toBeLessThanOrEqual(1)
    expect(markOf(track).mark.pointerEvents).toBe('none')
  })

  it('saves and draws each step of a drag, before the release', async () => {
    const { range, output, tileWidth } = await slider(144)
    const { x, y } = middleOf(range)
    await act(async () => {
      await mouse.moveTo(x, y)
      await mouse.press()
      await mouse.along(x, y, 0, 30)
    })
    await expect.poll(() => storedBrowse().coverSize).not.toBe(144)
    await afterMotion()
    const shown = Number.parseInt(output.textContent ?? '', 10)
    expect(storedBrowse().coverSize).toBe(shown)
    expect(tileWidth()).toBe(shown)
    await act(async () => {
      await mouse.release()
    })
  })
})

/** How many `type` events `target` has had since this was called. */
function counted(target: EventTarget, type: string) {
  let seen = 0
  const count = () => {
    seen += 1
  }
  target.addEventListener(type, count)
  onTestFinished(() => target.removeEventListener(type, count))
  return () => seen
}

/** A real press at (x, y), carried `by` px to the right before it lets go. */
async function pressAt(x: number, y: number, by = 0) {
  await act(async () => {
    await mouse.moveTo(x, y)
    await mouse.press()
    if (by) await mouse.moveTo(x + by, y)
    await mouse.release()
  })
}

describe('the View panel’s rows', () => {
  beforeEach(() => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [inProgress],
      added: [],
      read: [],
    })
  })

  /** The panel open beside a drawn Continue strip, and the strip's row. */
  async function continueRow() {
    const { host, panel } = await openView()
    const strip = () => host.querySelector('[data-testid="continue"]')
    await expect.poll(strip).not.toBeNull()
    const words = [...panel.querySelectorAll('label > span')].find(
      (span) => span.textContent === 'Continue strip',
    )
    if (!(words?.parentElement instanceof HTMLLabelElement)) {
      throw new Error('no Continue strip row')
    }
    const row = words.parentElement
    const box = need<HTMLInputElement>(row, ':scope > input[type="checkbox"]')
    return { panel, row, words, box, strip, changes: counted(box, 'change') }
  }

  it.each([1, 4])(
    'toggles the Continue strip once for a press on its words moved %i px',
    async (by) => {
      const { words, box, strip, changes } = await continueRow()
      expect(box.checked).toBe(true)
      const { x, y } = middleOf(words)
      await pressAt(x, y, by)
      expect(changes()).toBe(1)
      expect(box.checked).toBe(false)
      await expect.poll(strip).toBeNull()
      await expect.poll(() => storedBrowse().continueStrip).toBe(false)
      await pressAt(x, y, by)
      expect(changes()).toBe(2)
      expect(box.checked).toBe(true)
      await expect.poll(strip).not.toBeNull()
      await expect.poll(() => storedBrowse().continueStrip).toBe(true)
    },
  )

  it('takes a click on a row’s words over from the label', async () => {
    const { words, box } = await continueRow()
    // Chromium passes a moved press on to the box with or without the
    // takeover, so this case is what holds it here; Firefox, where a moved
    // press needs it, has no project. On `window`, the listener reads the
    // click after every listener in the page.
    const clicks = counted(box, 'click')
    const prevented: boolean[] = []
    const record = (event: Event) => {
      if (event.target === words) prevented.push(event.defaultPrevented)
    }
    window.addEventListener('click', record)
    onTestFinished(() => window.removeEventListener('click', record))
    const { x, y } = middleOf(words)
    await pressAt(x, y)
    expect(prevented).toEqual([true])
    expect(clicks()).toBe(1)
    expect(box.checked).toBe(false)
    // The focus the cancelled activation would have given the box, so a
    // key acts on the row just pressed.
    expect(document.activeElement).toBe(box)
  })

  it.each(['left', 'right'] as const)(
    'toggles from the panel’s %s edge',
    async (edge) => {
      const { panel, row, box, changes } = await continueRow()
      const outer = panel.getBoundingClientRect()
      const style = getComputedStyle(panel)
      const x =
        edge === 'left'
          ? outer.left + Number.parseFloat(style.borderLeftWidth) + 2
          : outer.right - Number.parseFloat(style.borderRightWidth) - 2
      const { y } = middleOf(row)
      await pressAt(x, y)
      expect(changes()).toBe(1)
      expect(box.checked).toBe(false)
      expect(document.elementFromPoint(x, y)).toBe(row)
      expect(getComputedStyle(row).cursor).toBe('pointer')
    },
  )

  it.each([
    [
      'the Cover size row',
      (_: Element, words: Element) => ({ x: middleOf(words).x, on: words }),
    ],
    [
      'the panel’s padding',
      (panel: Element) => ({
        x:
          panel.getBoundingClientRect().left +
          Number.parseFloat(getComputedStyle(panel).borderLeftWidth) +
          2,
        on: panel,
      }),
    ],
  ])('selects no words on a press that moves from %s', async (_, start) => {
    const { panel } = await openView()
    const words = need(panel, '.field-slider-head label')
    const { x, on } = start(panel, words)
    const { y } = middleOf(words)
    expect(document.elementFromPoint(x, y)).toBe(on)
    getSelection()?.removeAllRanges()
    await act(async () => {
      await mouse.moveTo(x, y)
      await mouse.press()
      await mouse.along(x, y, 0, 60)
      await mouse.release()
    })
    expect(getSelection()?.toString()).toBe('')
  })
})
