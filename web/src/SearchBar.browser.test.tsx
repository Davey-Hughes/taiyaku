import { act } from 'react'
import {
  MemoryRouter,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { cdp, page } from 'vitest/browser'
import './index.css'
import * as library from './library'
import { Library } from './Library'
import { SEARCH_BOX_ID } from './searchHits'
import { Series } from './Series'
import { Shell } from './Shell'
import { mount } from './testing/mount'
import { mouse } from './testing/mouse'

/**
 * The Library view's search box in a browser, with real keys and a real
 * pointer — spec §2 and §6. jsdom has no input pipeline and no scheduler of
 * its own, so a box that drops keystrokes typed faster than the router
 * commits the URL (O223) can only be seen here, and a tap is a tap only here.
 */

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
}))

const series = (id: number, title: string): library.SeriesCard => ({
  id,
  title,
  works: 3,
  editions: 3,
  aligned: 0,
  added: id,
  unread: 3,
  cover: `/api/library/series/${id}/cover`,
})

/** Forty series, so each commit of the view has a grid's worth to draw. */
const SERIES = Array.from({ length: 40 }, (_, i) =>
  series(i + 1, `Oshi no Ko ${i + 1}`),
)

/**
 * A child that draws the router's query string, so a test can read what the
 * box put in the URL, and a hidden button that is the browser's Back, which
 * a memory router has no other way to be given. Inside a function for the
 * reason `Shell.test.tsx`'s `reporter()` gives.
 */
function locationProbe() {
  function Probe() {
    const { pathname, search } = useLocation()
    const navigate = useNavigate()
    return (
      <>
        <output data-testid="location">{pathname + search}</output>
        <button
          type="button"
          hidden
          data-testid="back"
          onClick={() => void navigate(-1)}
        />
      </>
    )
  }
  return <Probe />
}

/** The Library view and the series page, each in its shell, at one width. */
async function at(path: string, width: number): Promise<HTMLElement> {
  await page.viewport(width, 844)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route
            path="/"
            element={
              <Shell>
                <Library />
              </Shell>
            }
          />
          <Route
            path="/series/:id"
            element={
              <Shell>
                <Series />
              </Shell>
            }
          />
        </Routes>
        {locationProbe()}
      </MemoryRouter>,
    )
    host.style.height = '100%'
    await Promise.resolve()
    await Promise.resolve()
  })
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

function need<T extends Element = HTMLElement>(
  host: ParentNode,
  selector: string,
): T {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/** Where the router is: the path and the query string. */
const locationOf = (host: HTMLElement) =>
  need(host, '[data-testid="location"]').textContent ?? ''

/** `?q=` as the router holds it. */
const urlQuery = (host: HTMLElement) =>
  new URLSearchParams(locationOf(host).replace(/^[^?]*/, '')).get('q')

/** A press and a release at the middle of `el`, from the real pointer. */
async function tap(el: Element) {
  const box = el.getBoundingClientRect()
  await mouse.moveTo(box.left + box.width / 2, box.top + box.height / 2)
  await act(async () => {
    await mouse.press()
  })
  await act(async () => {
    await mouse.release()
  })
}

/**
 * Type `word` into whatever has focus, every key sent before the first has
 * been answered: the keys reach the page back to back, as a fast hand's do,
 * with no turn of the test's own between them for React to catch up in.
 */
async function typeFast(word: string) {
  const session = cdp()
  // Answered before any key is sent: the first command opens the session,
  // and keys sent while it opens can arrive out of order.
  await session.send('Input.setIgnoreInputEvents', { ignore: false })
  await Promise.all(
    [...word].flatMap((key) => {
      const code = `Key${key.toUpperCase()}`
      const windowsVirtualKeyCode = key.toUpperCase().charCodeAt(0)
      return [
        session.send('Input.dispatchKeyEvent', {
          type: 'keyDown',
          key,
          code,
          text: key,
          unmodifiedText: key,
          windowsVirtualKeyCode,
        }),
        session.send('Input.dispatchKeyEvent', {
          type: 'keyUp',
          key,
          code,
          windowsVirtualKeyCode,
        }),
      ]
    }),
  )
}

beforeEach(() => {
  localStorage.clear()
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: [],
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue({ series: SERIES, unfiled: [] })
  vi.mocked(library.search).mockResolvedValue({
    series: SERIES,
    unfiled: [],
    query: 'oshinoko',
  })
  vi.mocked(library.oneSeries).mockResolvedValue({
    series: {
      ...series(7, 'Oshi no Ko'),
      titles: [{ title: 'Oshi no Ko', kind: 'romaji', primary: true }],
    },
    works: [],
  })
})

describe('the search box, typed into fast — O223', () => {
  it('keeps every character, in the box and in ?q=', async () => {
    const host = await at('/', 1440)
    const box = need<HTMLInputElement>(host, `#${SEARCH_BOX_ID}`)
    // Inside the frame first: a key pressed before anything here has focus
    // goes to the frame around the test.
    await act(async () => box.focus())
    // Outside `act`, as in the app: React's own scheduler runs the box's
    // update and the router's transition as it does for a user's keys. The
    // flag is off for that stretch so React does not report those updates
    // as unwrapped.
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      await typeFast('oshinoko')
      await new Promise((resolve) => setTimeout(resolve, 400))
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(box.value).toBe('oshinoko')
    expect(urlQuery(host)).toBe('oshinoko')
  })
})

describe('the phone bar’s search icon — spec §2', () => {
  it('on a library’s page, keeps the library and focuses the box inside the tap', async () => {
    const host = await at('/?library=1', 390)
    const box = need<HTMLInputElement>(host, `#${SEARCH_BOX_ID}`)
    // Whether the box took focus while the tap's click was being handled:
    // iOS raises its keyboard only for a focus made inside the gesture, and
    // a focus made after the navigation it starts is outside it.
    let inTap = false
    let focusedInTap: boolean | undefined
    const tapStarts = () => {
      inTap = true
    }
    const tapEnds = () => {
      inTap = false
    }
    const focused = () => {
      focusedInTap ??= inTap
    }
    window.addEventListener('click', tapStarts, { capture: true })
    window.addEventListener('click', tapEnds)
    box.addEventListener('focus', focused)
    try {
      await tap(need(host, '.phone-bar a.search'))
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 100))
      })
    } finally {
      window.removeEventListener('click', tapStarts, { capture: true })
      window.removeEventListener('click', tapEnds)
      box.removeEventListener('focus', focused)
    }
    expect(locationOf(host)).toBe('/?library=1')
    expect(document.activeElement).toBe(box)
    expect(focusedInTap).toBe(true)
  })

  it('from another screen, goes to the Library view and focuses the box', async () => {
    const host = await at('/series/7', 390)
    expect(host.querySelector(`#${SEARCH_BOX_ID}`)).toBeNull()
    await tap(need(host, '.phone-bar a.search'))
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100))
    })
    expect(locationOf(host)).toBe('/')
    expect(document.activeElement).toBe(need(host, `#${SEARCH_BOX_ID}`))
  })

  // Back is a return to the grid, not a tap on the icon: a focus there
  // would raise a phone's keyboard over what the reader came back to.
  it('does not focus the box again on Back to the entry it made', async () => {
    const host = await at('/series/7', 390)
    await tap(need(host, '.phone-bar a.search'))
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100))
    })
    expect(document.activeElement).toBe(need(host, `#${SEARCH_BOX_ID}`))
    await act(async () => {
      need(host, '.grid a[href="/series/1"]').click()
    })
    expect(locationOf(host)).toBe('/series/1')
    await act(async () => {
      need(host, '[data-testid="back"]').click()
    })
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 100))
    })
    expect(locationOf(host)).toBe('/')
    expect(document.activeElement).not.toBe(need(host, `#${SEARCH_BOX_ID}`))
  })
})
