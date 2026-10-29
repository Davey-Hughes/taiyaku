import { act } from 'react'
import {
  MemoryRouter,
  type NavigateFunction,
  type NavigateOptions,
  Route,
  Routes,
  type To,
  useLocation,
  useNavigate,
} from 'react-router'
import { beforeEach, describe, expect, it } from 'vitest'
import {
  forgetScroll,
  lastLibraryAddress,
  RESTORE_SCROLL,
  SCROLL_TO_TOP,
  useKeptScroll,
} from '../src/scrollKeeper'
import { mount, rerender } from './support/mount'

/**
 * The keeper's rules, in jsdom, where nothing is laid out: `<main>` is the
 * scroller here as it is at a desktop's width, an inline `overflow-y` is
 * what makes it one, and its room to scroll is set by hand.
 * `scrollKeeper.browser.test.tsx` checks the same with real scrolling, at
 * both widths.
 */

/** How far `<main>` can scroll: what a restore is clamped to. */
let room = 5000
let go: NavigateFunction = () => {
  throw new Error('no router')
}

/**
 * The Library view at `/`, a series page at `/series/:id`, and pages that
 * keep no scroll at every other address, all inside one `<main>`, as the
 * shell draws them. Declared inside a function for the reason
 * `Gate.test.tsx`'s `probe` gives, and called once, so each render draws
 * the same components.
 */
function components() {
  function Probe() {
    go = useNavigate()
    return null
  }

  /** A view that keeps its scroll, drawing the address it is at. */
  function View({ ready, library }: { ready: boolean; library?: boolean }) {
    useKeptScroll(ready, library ? { library: true } : undefined)
    const { pathname, search } = useLocation()
    return <p data-testid="view">{`${pathname}${search}`}</p>
  }

  /** A view of another kind, so a move between the two unmounts the one left. */
  function SeriesView({ ready }: { ready: boolean }) {
    return <View ready={ready} />
  }

  function Harness({
    ready = true,
    at = '/',
  }: {
    ready?: boolean
    at?: string
  }) {
    return (
      <MemoryRouter initialEntries={[at]}>
        <Probe />
        <main id="content" style={{ overflowY: 'auto' }}>
          <Routes>
            <Route path="/" element={<View ready={ready} library />} />
            <Route path="/series/:id" element={<SeriesView ready={ready} />} />
            <Route path="*" element={<p data-testid="elsewhere" />} />
          </Routes>
        </main>
      </MemoryRouter>
    )
  }

  return { Harness }
}

const { Harness } = components()

function mainOf(host: HTMLElement): HTMLElement {
  const main = host.querySelector('main')
  if (!main) throw new Error('no main')
  Object.defineProperty(main, 'clientHeight', {
    configurable: true,
    get: () => 800,
  })
  Object.defineProperty(main, 'scrollHeight', {
    configurable: true,
    get: () => room + 800,
  })
  return main
}

function show(at = '/') {
  const host = mount(<Harness at={at} />)
  return { host, main: mainOf(host) }
}

/** A frame, inside `act`. The keeper saves on the scroll's report, not on a frame. */
async function aFrame() {
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
  })
}

/** The reader scrolls `<main>` to `y`, and a frame passes. */
async function scroll(main: HTMLElement, y: number) {
  main.scrollTop = y
  main.dispatchEvent(new Event('scroll'))
  await aFrame()
}

/** A push, a replace, or Back and Forward by `to` entries, as the router makes them. */
async function navigate(to: To | number, options?: NavigateOptions) {
  await act(async () => {
    await (typeof to === 'number' ? go(to) : go(to, options))
  })
}

beforeEach(() => {
  room = 5000
  history.scrollRestoration = 'auto'
})

describe('the scroll keeper', () => {
  it('lands Back and Forward at each entry’s scroll, saved per entry', async () => {
    const { main } = show()
    await scroll(main, 300)
    await navigate('/series/1')
    await scroll(main, 500)
    await navigate('/series/2')
    await scroll(main, 700)
    const landed = []
    await navigate(-1)
    landed.push(main.scrollTop)
    await navigate(-1)
    landed.push(main.scrollTop)
    await navigate(1)
    landed.push(main.scrollTop)
    await navigate(1)
    landed.push(main.scrollTop)
    expect(landed).toEqual([500, 300, 500, 700])
  })

  it('saves under each entry of a view that stays drawn', async () => {
    const { main } = show()
    await scroll(main, 300)
    await navigate('/?library=1')
    await scroll(main, 900)
    await navigate(-1)
    const first = main.scrollTop
    await navigate(1)
    expect([first, main.scrollTop]).toEqual([300, 900])
  })

  it('saves the scroll as the view goes, before the scroll is reported', async () => {
    const { main } = show()
    main.scrollTop = 450
    await navigate('/series/1')
    await navigate(-1)
    expect(main.scrollTop).toBe(450)
  })

  it('lands a push at the top, from a page that keeps a scroll or one that does not', async () => {
    const { main } = show()
    await scroll(main, 300)
    await navigate('/series/1')
    const fromView = main.scrollTop
    await navigate('/about')
    main.scrollTop = 600
    await navigate('/')
    expect([fromView, main.scrollTop]).toEqual([0, 0])
  })

  it('clamps a restore to the room there is', async () => {
    const { main } = show()
    await scroll(main, 3000)
    await navigate('/series/1')
    room = 1200
    await navigate(-1)
    expect(main.scrollTop).toBe(1200)
  })

  it('restores once the body is drawn, and once only', async () => {
    const host = mount(<Harness />)
    const main = mainOf(host)
    await scroll(main, 800)
    await navigate('/series/1')
    rerender(<Harness ready={false} />)
    await navigate(-1)
    const waiting = main.scrollTop
    rerender(<Harness ready />)
    const drawn = main.scrollTop
    await scroll(main, 150)
    rerender(<Harness ready={false} />)
    rerender(<Harness ready />)
    expect([waiting, drawn, main.scrollTop]).toEqual([0, 800, 150])
  })

  it('keeps an entry’s scroll while its body waits, through the scroll the wait makes', async () => {
    const host = mount(<Harness />)
    const main = mainOf(host)
    await scroll(main, 800)
    await navigate('/series/1')
    rerender(<Harness ready={false} />)
    await navigate(-1)
    await scroll(main, 0)
    await navigate(1)
    await navigate(-1)
    rerender(<Harness ready />)
    expect(main.scrollTop).toBe(800)
  })

  it('moves nothing on a replace, and saves on under the new entry', async () => {
    const { main } = show()
    await scroll(main, 300)
    await navigate('/?show=multi', { replace: true })
    const replaced = main.scrollTop
    await scroll(main, 350)
    await navigate('/series/1')
    await navigate(-1)
    expect([replaced, main.scrollTop]).toEqual([300, 350])
  })

  it('lands a replace marked SCROLL_TO_TOP at the top while the view is drawn, and saves on under it', async () => {
    const { main } = show('/?show=multi')
    await scroll(main, 300)
    await navigate('/?show=multi', {
      replace: true,
      state: { [SCROLL_TO_TOP]: true },
    })
    const replaced = main.scrollTop
    await scroll(main, 350)
    await navigate('/series/1')
    await navigate(-1)
    expect([replaced, main.scrollTop]).toEqual([0, 350])
  })

  it('saves the scroll a replace lands at, before any scroll, for Back after a push in the view', async () => {
    const { main } = show()
    await scroll(main, 300)
    await navigate('/?show=multi', { replace: true })
    await navigate('/?library=1')
    await navigate(-1)
    expect(main.scrollTop).toBe(300)
  })

  it('lands a replace that draws the view at the top', async () => {
    const { main } = show('/about')
    main.scrollTop = 600
    await navigate('/?q=kaguya', { replace: true })
    expect(main.scrollTop).toBe(0)
  })

  it('lands a push marked RESTORE_SCROLL to the last Library view at its scroll', async () => {
    const { main } = show('/?show=multi')
    await scroll(main, 400)
    await navigate('/series/1')
    await scroll(main, 900)
    await navigate('/read/work-1')
    const address = lastLibraryAddress()
    await navigate(address ?? '/', { state: { [RESTORE_SCROLL]: true } })
    expect([address, main.scrollTop]).toEqual(['/?show=multi', 400])
  })

  it('lands that push at the top without the mark', async () => {
    const { main } = show('/?show=multi')
    await scroll(main, 400)
    await navigate('/read/work-1')
    await navigate('/?show=multi')
    expect(main.scrollTop).toBe(0)
  })

  it('lands the mark at the top on another address, and on a series page', async () => {
    const { main } = show('/?show=multi')
    await scroll(main, 400)
    await navigate('/read/work-1')
    await navigate('/?show=unread', { state: { [RESTORE_SCROLL]: true } })
    const elsewhere = main.scrollTop
    await scroll(main, 700)
    await navigate('/series/1', { state: { [RESTORE_SCROLL]: true } })
    expect([elsewhere, main.scrollTop]).toEqual([0, 0])
  })

  it('holds the last Library view’s address, a replace’s included, and no series page’s', async () => {
    const before = lastLibraryAddress()
    show('/?library=2')
    const drawn = lastLibraryAddress()
    await navigate('/?library=2&show=multi', { replace: true })
    const replaced = lastLibraryAddress()
    await navigate('/series/1')
    expect([before, drawn, replaced, lastLibraryAddress()]).toEqual([
      undefined,
      '/?library=2',
      '/?library=2&show=multi',
      '/?library=2&show=multi',
    ])
  })

  it('forgets every scroll and the address', async () => {
    const { main } = show('/?show=multi')
    await scroll(main, 400)
    await navigate('/series/1')
    forgetScroll()
    const address = lastLibraryAddress()
    await navigate(-1)
    expect([address, main.scrollTop]).toEqual([undefined, 0])
  })

  it('turns the browser’s restoration off while drawn, and on for the entry it leaves for', async () => {
    const modes = [history.scrollRestoration]
    show()
    modes.push(history.scrollRestoration)
    await navigate('/about')
    modes.push(history.scrollRestoration)
    // Found off, as it is on an entry arrived at by Back that a view had
    // turned off: the entry a push makes from it starts off too.
    history.scrollRestoration = 'manual'
    await navigate('/series/1')
    await navigate('/about')
    modes.push(history.scrollRestoration)
    expect(modes).toEqual(['auto', 'manual', 'auto', 'auto'])
  })
})
