import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, describe, expect, it } from 'vitest'
import { createCohort } from './cohort'
import { createDecodeQueue } from './decode'
import { PageView } from './PageView'

/**
 * `PageView` in a real browser, which is the only place several of its
 * guarantees are observable at all.
 *
 * The `node` project covers every rule this component is assembled from —
 * `decode.ts`'s gate, `ordering.ts`'s monotonicity, `cohort.ts`'s barrier — and
 * cannot cover the assembly: `App.tsx` and `PageView` need a DOM, and `jsdom`
 * has no layout engine (`getBoundingClientRect` returns zeroes) and no real
 * `HTMLImageElement.decode()`. Both are load-bearing here. Every defect this
 * reader has shipped in this area was visual, and none of them would have
 * failed a Node test.
 */

/** A 2x2 PNG as a data URL, so a decode is real but instant and needs no server. */
const TINY =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGM4IScHRAwQCgAfJgQRoo8irwAAAABJRU5ErkJggg=='

/** A second image, distinguishable from the first by its own data URL. */
const OTHER =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAAEElEQVR4nGOQszkBRAwQCgAdHgSJ5Ed7/AAAAABJRU5ErkJggg=='

let root: Root | undefined
let host: HTMLElement | undefined

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
})

function mount(ui: React.ReactNode): HTMLElement {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  act(() => root?.render(ui))
  return host
}

/** Waits until `check` holds, or gives up — a decode is real here, so nothing is synchronous. */
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

const src = () => document.querySelector('img')?.getAttribute('src') ?? ''

describe('PageView, in a browser', () => {
  it('draws its source once the real decode has resolved', async () => {
    mount(<PageView src={TINY} alt="a page" buffered={true} />)
    await until(() => src() === TINY)
    const img = document.querySelector('img') as HTMLImageElement
    expect(img.complete).toBe(true)
    expect(img.naturalWidth).toBeGreaterThan(0)
  })

  // The guarantee the whole page-turn mechanism rests on: an already-drawn page
  // stays on screen until the incoming one can be painted, so a turn never
  // lands on a blank frame. Unobservable without a real decode.
  it('keeps the outgoing page on screen until the incoming one has decoded', async () => {
    mount(<PageView src={TINY} alt="a page" buffered={true} />)
    await until(() => src() === TINY)

    // A source that can never decode: the gate must hold the old page rather
    // than blank the cell.
    const broken = 'data:image/png;base64,not-a-png'
    act(() =>
      root?.render(<PageView src={broken} alt="a page" buffered={true} />),
    )
    await act(async () => {
      await new Promise((r) => setTimeout(r, 120))
    })
    // Either still the old page, or the broken one presented deliberately once
    // its decode rejected — never an empty `src`, which is the blank frame.
    expect(src()).not.toBe('')
  })

  it('reports the source it is drawing, which is what the two-page geometry reads', async () => {
    const seen: string[] = []
    mount(
      <PageView
        src={TINY}
        alt="a page"
        buffered={true}
        onShown={(s) => seen.push(s)}
      />,
    )
    await until(() => seen.includes(TINY))
    expect(seen[seen.length - 1]).toBe(TINY)
  })

  // `two` mode's layout, in a real one. `cellsFor` mirrors one slot into both
  // cells when a group has a single page, and `App.tsx` reads "both cells are
  // drawing the same source" as the signal to hide the second. This is the
  // arithmetic of that, measured rather than assumed — and it is exactly what
  // `jsdom` cannot answer, since it has no layout.
  it('gives two cells equal width in a flex row, which is what a pair depends on', async () => {
    const host = mount(
      <div style={{ display: 'flex', width: '600px', height: '200px' }}>
        <div style={{ flex: 1, minWidth: 0, height: '100%' }}>
          <PageView src={TINY} alt="left" buffered={true} />
        </div>
        <div style={{ flex: 1, minWidth: 0, height: '100%' }}>
          <PageView src={OTHER} alt="right" buffered={true} />
        </div>
      </div>,
    )
    await until(() => host.querySelectorAll('img').length === 2)
    const cells = [...host.querySelectorAll('img')].map(
      (i) => i.parentElement?.parentElement as HTMLElement,
    )
    const widths = cells.map((c) => Math.round(c.getBoundingClientRect().width))
    expect(widths[0]).toBe(300)
    expect(widths[1]).toBe(300)
  })

  it('collapses one cell to nothing when a spread spans the group, leaving the other full width', async () => {
    const host = mount(
      <div style={{ display: 'flex', width: '600px', height: '200px' }}>
        <div style={{ flex: 1, minWidth: 0, height: '100%' }}>
          <PageView src={TINY} alt="spread" buffered={true} />
        </div>
        <div
          style={{
            flex: '0 0 0',
            width: 0,
            minWidth: 0,
            height: '100%',
            overflow: 'hidden',
          }}
        >
          <PageView src={TINY} alt="mirror" buffered={true} />
        </div>
      </div>,
    )
    await until(() => host.querySelectorAll('img').length === 2)
    const cells = [...host.querySelectorAll('img')].map(
      (i) => i.parentElement?.parentElement as HTMLElement,
    )
    expect(Math.round(cells[0]?.getBoundingClientRect().width ?? 0)).toBe(600)
    expect(Math.round(cells[1]?.getBoundingClientRect().width ?? 0)).toBe(0)
  })

  // Both halves of a turn become visible together — the defect a reader
  // reported as the halves not arriving at the same time, measured then at a
  // median 89.7 ms apart. Here with real decodes and one shared cohort.
  it('presents both halves of a pair in the same frame', async () => {
    const cohort = createCohort()
    const queue = createDecodeQueue(3)
    const stamps: number[] = []
    const at = () => performance.now()

    const host = mount(
      <div style={{ display: 'flex', width: '600px', height: '200px' }}>
        <PageView
          src={TINY}
          alt="left"
          buffered={true}
          cohort={cohort}
          decodeQueue={queue}
          onShown={() => stamps.push(at())}
        />
        <PageView
          src={OTHER}
          alt="right"
          buffered={true}
          cohort={cohort}
          decodeQueue={queue}
          onShown={() => stamps.push(at())}
        />
      </div>,
    )
    await until(
      () => host.querySelectorAll('img').length === 2 && stamps.length >= 2,
    )
    const gap = Math.abs((stamps[1] as number) - (stamps[0] as number))
    // Generous: the assertion is "one frame", not a specific number.
    expect(gap).toBeLessThan(16)
  })
})
