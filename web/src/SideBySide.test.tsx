import {
  Link,
  MemoryRouter,
  type NavigateFunction,
  Route,
  Routes,
  useNavigate,
  useParams,
} from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useFitsPair } from './fitsPair'
import { PageHeader } from './settingsLayout'
import { SideBySide } from './SideBySide'
import { flush } from './testing/async'
import { press } from './testing/input'
import { mount, rerender } from './testing/mount'

vi.mock('./fitsPair', () => ({ useFitsPair: vi.fn(() => false) }))

const LIST = '/things'

/** The router's navigate, handed out to the test, which is outside it. */
let steer: NavigateFunction | undefined

/**
 * A screen of two things over `SideBySide`, as Users is: the list's own
 * address draws the list, and a thing's address opens its page, whose back
 * link is `PageHeader`'s. Thing 9 has no row. An entry may carry the state
 * a navigation left, as a reloaded history entry does. Its components are
 * made once per call, so a test that draws again keeps them, and here
 * because `useComponentExportOnlyModules` refuses an unexported component
 * at module scope.
 */
function screen() {
  function Things() {
    return (
      <ul className="things">
        <li>
          <Link to={`${LIST}/1`}>One</Link>
        </li>
        <li>
          <Link to={`${LIST}/2`}>Two</Link>
        </li>
      </ul>
    )
  }
  function Thing({ id }: { id: string }) {
    return (
      <div className="thing">
        <PageHeader
          back={{ to: LIST, label: 'Things' }}
          title={`Thing ${id}`}
        />
      </div>
    )
  }
  function Screen() {
    const rest = useParams()['*'] ?? ''
    return (
      <SideBySide
        list={<Things />}
        page={rest === '' ? null : <Thing id={rest} />}
      />
    )
  }
  function Steering() {
    steer = useNavigate()
    return null
  }
  return (entry: string | { pathname: string; state: unknown }) => (
    <MemoryRouter initialEntries={[entry]}>
      <button type="button" className="elsewhere">
        Elsewhere
      </button>
      <Routes>
        <Route path={`${LIST}/*`} element={<Screen />} />
      </Routes>
      <Steering />
    </MemoryRouter>
  )
}

/** The one element `selector` finds under `root`, or a throw naming it. */
function one<E extends Element = HTMLElement>(
  root: ParentNode,
  selector: string,
): E {
  const found = root.querySelectorAll<E>(selector)
  if (found.length !== 1) {
    throw new Error(`${found.length} elements match ${selector}`)
  }
  return found[0]
}

/** The link whose text is `text`. */
const link = (host: HTMLElement, text: string) => {
  const found = [...host.querySelectorAll('a')].find(
    (a) => a.textContent === text,
  )
  if (found === undefined) throw new Error(`no link "${text}"`)
  return found
}

/** What the split holds, in order, by class. */
const panes = (host: HTMLElement) =>
  [...one(host, '.split').children].map((child) => child.className)

const fitting = (fits: boolean) => vi.mocked(useFitsPair).mockReturnValue(fits)

beforeEach(() => {
  fitting(false)
})

describe('SideBySide, what it draws', () => {
  it('takes an element or null for its page, so nothing that draws nothing reads as an open one', () => {
    // Held by `pnpm run typecheck`, not by this run: each directive fails
    // the typecheck the day the element below it compiles.
    // @ts-expect-error `false` draws nothing, so it is no page.
    const unset = <SideBySide list={null} page={false} />
    expect(unset.type).toBe(SideBySide)
    // @ts-expect-error Nor is `undefined` a page.
    const missing = <SideBySide list={null} page={undefined} />
    expect(missing.type).toBe(SideBySide)
  })

  it('measures the split against the breakpoint in a probe hidden from assistive technology', () => {
    const host = mount(screen()(LIST))
    const [split, probe] = vi.mocked(useFitsPair).mock.calls[0]
    expect(split.current).toBe(one(host, '.split'))
    expect(probe.current).toBe(
      one(host, '.split > .split-probe > .split-breakpoint'),
    )
    expect(one(host, '.split-probe').getAttribute('aria-hidden')).toBe('true')
    expect(one(host, '.split-probe').textContent).toBe('')
  })

  for (const fits of [false, true]) {
    it(`draws the list alone, at full width, with no page open (${fits ? 'wide' : 'narrow'})`, () => {
      fitting(fits)
      const host = mount(screen()(LIST))
      expect(panes(host)).toEqual(['split-probe', 'split-list'])
      expect(one(host, '.split-list > ul.things')).toBeTruthy()
      expect(one(host, '.split').className).toBe('split')
    })
  }

  it('draws the list then the page as a pair when they fit', () => {
    fitting(true)
    const host = mount(screen()(`${LIST}/2`))
    expect(panes(host)).toEqual(['split-probe', 'split-list', 'split-detail'])
    expect(one(host, '.split').className).toBe('split split-pair')
    expect(one(host, '.split-list > ul.things')).toBeTruthy()
    expect(one(host, '.split-detail .page-title').textContent).toBe('Thing 2')
  })

  it('keeps the one list while the first row opens and another replaces it', async () => {
    fitting(true)
    const host = mount(screen()(LIST))
    const list = one(host, '.split-list') as HTMLElement & { marked?: true }
    list.marked = true
    await press(link(host, 'One'))
    expect(one(host, '.split-detail .page-title').textContent).toBe('Thing 1')
    expect(one(host, '.split-list')).toBe(list)
    await press(link(host, 'Two'))
    expect(one(host, '.split-detail .page-title').textContent).toBe('Thing 2')
    expect(one(host, '.split-list')).toBe(list)
    expect(list.marked).toBe(true)
  })

  it('draws the page alone, with no list at all, when the two do not fit', () => {
    const host = mount(screen()(`${LIST}/2`))
    expect(panes(host)).toEqual(['split-probe', 'split-detail'])
    expect(one(host, '.split').className).toBe('split')
    expect(host.querySelector('ul.things')).toBeNull()
    expect(one(host, '.split-detail .page-title').textContent).toBe('Thing 2')
  })
})

describe('SideBySide, focus when a page is closed', () => {
  for (const fits of [true, false]) {
    it(`gives focus to the closed page's row when its back link is followed (${fits ? 'wide' : 'narrow'})`, async () => {
      fitting(fits)
      const host = mount(screen()(`${LIST}/2`))
      await press(link(host, '‹ Things'))
      expect(host.querySelector('.split-detail')).toBeNull()
      expect(document.activeElement).toBe(link(host, 'Two'))
    })
  }

  it('gives focus once per close, not again when the list draws again', async () => {
    fitting(true)
    const draw = screen()
    const host = mount(draw(`${LIST}/2`))
    await press(link(host, '‹ Things'))
    expect(document.activeElement).toBe(link(host, 'Two'))
    const elsewhere = one(host, 'button.elsewhere')
    elsewhere.focus()
    rerender(draw(LIST))
    expect(document.activeElement).toBe(elsewhere)
  })

  for (const fits of [true, false]) {
    it(`leaves focus where it is on a browser Back to the list a back link closed a page to (${fits ? 'wide' : 'narrow'})`, async () => {
      fitting(fits)
      const host = mount(screen()(`${LIST}/2`))
      await press(link(host, '‹ Things'))
      expect(document.activeElement).toBe(link(host, 'Two'))
      // Another row opened, and Back to the entry the back link made, whose
      // state still names Thing 2: the page closed now is Thing 1's.
      await press(link(host, 'One'))
      const elsewhere = one(host, 'button.elsewhere')
      elsewhere.focus()
      await flush(() => steer?.(-1))
      expect(host.querySelector('.split-detail')).toBeNull()
      expect(document.activeElement).toBe(elsewhere)
    })
  }

  it('leaves focus where it is when an entry a back link made is drawn again on a reload', () => {
    fitting(true)
    mount(screen()({ pathname: LIST, state: { closed: `${LIST}/2` } }))
    expect(document.activeElement).toBe(document.body)
  })

  it('leaves focus where it is when the closed page has no row', async () => {
    fitting(true)
    const host = mount(screen()(`${LIST}/9`))
    const elsewhere = one(host, 'button.elsewhere')
    elsewhere.focus()
    await press(link(host, '‹ Things'))
    expect(host.querySelector('.split-detail')).toBeNull()
    expect(document.activeElement).toBe(elsewhere)
  })
})
