import { act } from 'react'
import {
  type Location,
  MemoryRouter,
  type NavigateFunction,
  useLocation,
  useNavigate,
} from 'react-router'
import { afterEach, describe, expect, it } from 'vitest'
import {
  type Destination,
  forgetBack,
  useBack,
  useBackTracker,
} from '../src/back'
import { RESTORE_SCROLL } from '../src/scrollKeeper'
import { mount } from './support/mount'

/**
 * The record `useBackTracker` keeps and the way `useBack` takes, under
 * `MemoryRouter`, with the entry index `BrowserRouter` writes into the
 * window's history set by hand: CI runs this, and not
 * `back.browser.test.tsx`, which takes the same steps under the router the
 * app runs.
 */

afterEach(() => {
  window.history.replaceState(null, '')
  forgetBack()
})

interface Drawn {
  back: Destination & { go: () => void }
  location: Location
  navigate: NavigateFunction
}

/**
 * A router at `entries`, the last one shown, on a window entry whose index
 * is `idx`, drawing the tracker and the way back as `AppRoutes` and a
 * screen do, and handing back what each render saw.
 */
function at(entries: string[], idx: number): () => Drawn {
  window.history.replaceState({ idx }, '')
  let drawn: Drawn | undefined
  function Probe() {
    useBackTracker()
    drawn = {
      back: useBack(),
      location: useLocation(),
      navigate: useNavigate(),
    }
    return null
  }
  const ui = (
    <MemoryRouter initialEntries={entries} initialIndex={entries.length - 1}>
      <Probe />
    </MemoryRouter>
  )
  mount(ui)
  return () => {
    if (drawn === undefined) throw new Error('not drawn')
    return drawn
  }
}

const go = async (to: () => void) => {
  await act(async () => {
    to()
  })
}

const named = ({ back }: Drawn) => [back.word, back.phrase]

describe('the way back', () => {
  it('names each push for the address it was pushed from', async () => {
    const drawn = at(['/?q=kaguya'], 1)
    await go(() => drawn().navigate('/series/7'))
    const series = named(drawn())
    await go(() => drawn().navigate('/read/work-1'))
    expect([series, named(drawn())]).toEqual([
      ['Search', 'the search'],
      ['Series', 'the series'],
    ])
  })

  it('hands a replaced entry’s name on to the entry that takes its place', async () => {
    const drawn = at(['/?q=kaguya'], 1)
    await go(() => drawn().navigate('/series/7'))
    await go(() => drawn().navigate('/series/7?edition=en', { replace: true }))
    expect(named(drawn())).toEqual(['Search', 'the search'])
  })

  it('keeps each entry’s name through Back and Forward, recording nothing for either', async () => {
    const drawn = at(['/'], 1)
    await go(() => drawn().navigate('/series/7'))
    await go(() => drawn().navigate('/read/work-1'))
    await go(() => drawn().navigate(-1))
    const series = [drawn().location.pathname, ...named(drawn())]
    await go(() => drawn().navigate(1))
    expect([series, [drawn().location.pathname, ...named(drawn())]]).toEqual([
      ['/series/7', 'Library', 'the library'],
      ['/read/work-1', 'Series', 'the series'],
    ])
  })

  it('is called Back, with no phrase, where an entry comes before and no record says which', () => {
    const drawn = at(['/series/7', '/read/work-1'], 1)
    expect(named(drawn())).toEqual(['Back', ''])
  })

  it('forgets every record', async () => {
    const drawn = at(['/'], 1)
    await go(() => drawn().navigate('/series/7'))
    const before = named(drawn())
    forgetBack()
    await go(() => drawn().navigate('/series/7?edition=en', { replace: true }))
    expect([before, named(drawn())]).toEqual([
      ['Library', 'the library'],
      ['Back', ''],
    ])
  })

  it('goes a step back where an entry comes before', async () => {
    const drawn = at(['/series/7', '/read/work-1'], 1)
    await go(() => drawn().back.go())
    expect(drawn().location.pathname).toBe('/series/7')
  })

  it('pushes the library, marked to land at its kept scroll, where no entry comes before', async () => {
    const drawn = at(['/series/7', '/read/work-1'], 0)
    const name = named(drawn())
    await go(() => drawn().back.go())
    const { pathname, state } = drawn().location
    expect([name, pathname, state]).toEqual([
      ['Library', 'the library'],
      '/',
      { [RESTORE_SCROLL]: true },
    ])
  })
})
