import { act } from 'react'
import {
  type InitialEntry,
  type Location,
  MemoryRouter,
  type NavigateFunction,
  type NavigationType,
  UNSAFE_createMemoryHistory as createMemoryHistory,
  unstable_HistoryRouter as HistoryRouter,
  useLocation,
  useNavigate,
  useNavigationType,
} from 'react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  BENEATH,
  CAME_FROM,
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
  arrival: NavigationType
  back: Destination & { go: () => void }
  location: Location
  navigate: NavigateFunction
}

/**
 * A router at `entries`, the last one shown, on a window entry whose index
 * is `idx`, drawing the tracker and the way back as `AppRoutes` and a
 * screen do, the way back given `seriesPage` and `putsUnder` as a reader's
 * route gives them, and handing back what each render saw.
 */
function at(
  entries: InitialEntry[],
  idx: number,
  seriesPage?: string,
  putsUnder?: boolean,
): () => Drawn {
  window.history.replaceState({ idx }, '')
  let drawn: Drawn | undefined
  function Probe() {
    useBackTracker()
    drawn = {
      arrival: useNavigationType(),
      back: useBack(seriesPage, putsUnder),
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

/** The address on screen and the move that drew it. */
const where = ({ location, arrival }: Drawn) => [
  `${location.pathname}${location.search}`,
  arrival,
]

const SERIES = '/series/41'

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

  it('names a series page that has no record for where the reader that put it there came from', async () => {
    const drawn = at(
      ['/', { pathname: SERIES, state: { [CAME_FROM]: '/' } }],
      1,
    )
    const name = named(drawn())
    await go(() => drawn().back.go())
    expect([name, where(drawn())]).toEqual([
      ['Library', 'the library'],
      ['/', 'POP'],
    ])
  })

  it('hands a series page’s name for where the reader that put it there came from on through a replace, as a record’s is', async () => {
    const drawn = at(
      ['/', { pathname: SERIES, state: { [CAME_FROM]: '/' } }],
      1,
    )
    await go(() => drawn().navigate(`${SERIES}?edition=en`, { replace: true }))
    expect(named(drawn())).toEqual(['Library', 'the library'])
  })

  it('steps back again from an entry it left, once Forward has brought it back (O434)', async () => {
    const drawn = at(['/series/7', '/series/8'], 1)
    await go(() => drawn().back.go())
    const back = drawn().location.pathname
    await go(() => drawn().navigate(1))
    const forward = drawn().location.pathname
    await go(() => drawn().back.go())
    expect([back, forward, drawn().location.pathname]).toEqual([
      '/series/7',
      '/series/8',
      '/series/7',
    ])
  })
})

describe('a reader’s way back to its series page (O431)', () => {
  it('steps back where the reader’s entry is marked as over its series page', async () => {
    const drawn = at(['/'], 1, SERIES)
    await go(() =>
      drawn().navigate('/read/work-1', { state: { [BENEATH]: SERIES } }),
    )
    const name = named(drawn())
    await go(() => drawn().back.go())
    expect([name, where(drawn())]).toEqual([
      ['Series', 'the series'],
      ['/', 'POP'],
    ])
  })

  it('steps back to the series page the reader was pushed from, at that page’s query', async () => {
    const drawn = at(['/'], 1, SERIES)
    await go(() => drawn().navigate('/series/41?edition=en&library=2'))
    await go(() => drawn().navigate('/read/work-1'))
    await go(() => drawn().back.go())
    expect(where(drawn())).toEqual(['/series/41?edition=en&library=2', 'POP'])
  })

  it('puts the series page in the place of a reader opened from elsewhere', async () => {
    const drawn = at(['/'], 1, SERIES)
    await go(() => drawn().navigate('/read/work-1'))
    await go(() => drawn().back.go())
    const left = where(drawn())
    await go(() => drawn().navigate(-1))
    expect([left, where(drawn())]).toEqual([
      [SERIES, 'REPLACE'],
      ['/', 'POP'],
    ])
  })

  it('puts its own series page in the place of a reader pushed from another series’ page', async () => {
    const drawn = at(['/'], 1, SERIES)
    await go(() => drawn().navigate('/series/60'))
    await go(() => drawn().navigate('/read/work-1'))
    await go(() => drawn().back.go())
    expect(where(drawn())).toEqual([SERIES, 'REPLACE'])
  })

  it('puts the series page in the reader’s place after a reload that left neither mark nor record', async () => {
    const drawn = at(['/', '/read/work-1'], 1, SERIES)
    await go(() => drawn().back.go())
    expect(where(drawn())).toEqual([SERIES, 'REPLACE'])
  })

  it('steps back after a reload where the reader’s entry carries the mark', async () => {
    const drawn = at(
      ['/', { pathname: '/read/work-1', state: { [BENEATH]: SERIES } }],
      1,
      SERIES,
    )
    await go(() => drawn().back.go())
    expect(where(drawn())).toEqual(['/', 'POP'])
  })

  it('puts the series page in the place of a reader with no entry before it', async () => {
    const drawn = at(['/read/work-1'], 0, SERIES)
    const name = named(drawn())
    await go(() => drawn().back.go())
    expect([name, where(drawn())]).toEqual([
      ['Series', 'the series'],
      [SERIES, 'REPLACE'],
    ])
  })
})

/**
 * A reader's route as `Routes.tsx` draws it, on a window entry whose index is
 * 1: its way back given `seriesPage` and `putsUnder` on a reader's address
 * only, beside a way back given none, as the series page's own is. Every
 * entry of its history is kept for the case to read, and every address drawn
 * is logged.
 */
function reading(
  entries: InitialEntry[],
  seriesPage?: string,
  putsUnder?: boolean,
) {
  window.history.replaceState({ idx: 1 }, '')
  const history = createMemoryHistory({
    initialEntries: entries,
    v5Compat: true,
  })
  // With no listener yet, `go` only moves the index.
  const kept = entries.map((_, i) => {
    history.go(i - history.index)
    return history.location
  })
  const { push, replace, go: step } = history
  history.push = (to, state) => {
    push(to, state)
    kept.splice(history.index, kept.length, history.location)
  }
  let replaced: (() => void) | undefined
  history.replace = (to, state) => {
    replace(to, state)
    kept[history.index] = history.location
    const then = replaced
    replaced = undefined
    then?.()
  }
  let held: number[] | undefined
  history.go = (delta) => {
    if (held === undefined) step(delta)
    else held.push(delta)
  }
  const paths: string[] = []
  let drawn: (Drawn & { plain: Drawn['back'] }) | undefined
  function Probe() {
    useBackTracker()
    const location = useLocation()
    paths.push(location.pathname)
    const reader = location.pathname.startsWith('/read/')
    drawn = {
      arrival: useNavigationType(),
      back: useBack(reader ? seriesPage : undefined, putsUnder),
      plain: useBack(),
      location,
      navigate: useNavigate(),
    }
    return null
  }
  mount(
    <HistoryRouter history={history}>
      <Probe />
    </HistoryRouter>,
  )
  return {
    drawn: () => {
      if (drawn === undefined) throw new Error('not drawn')
      return drawn
    },
    paths,
    /** Each entry's address and navigation state, and the index on screen. */
    history: () => ({
      entries: kept.map((l) => [`${l.pathname}${l.search}`, l.state]),
      index: history.index,
    }),
    /** Runs `then` once the next replace is taken, before it is drawn. */
    afterReplace: (then: () => void) => {
      replaced = then
    },
    /** Holds each step through the history, as a browser lands one a moment after it is asked for. */
    hold: () => {
      held = []
    },
    /** Lands the steps held, and how many there were. */
    land: async () => {
      const steps = held ?? []
      held = undefined
      await act(async () => {
        for (const delta of steps) step(delta)
      })
      return steps.length
    },
  }
}

/**
 * Dispatches each event from the page's body, as the browser does an input
 * with nothing focused, all before React draws again.
 */
const input = async (...events: Event[]) => {
  await act(async () => {
    for (const event of events) document.body.dispatchEvent(event)
  })
}
const keydown = (key: string) =>
  new KeyboardEvent('keydown', { key, bubbles: true })
const pointer = (type: string, pointerType: string) =>
  new PointerEvent(type, { pointerType, bubbles: true })
const touchend = () => new TouchEvent('touchend', { bubbles: true })

/** The name of the way back drawn beside the reader's, given no series page. */
const plainly = ({ plain }: { plain: Destination }) => [
  plain.word,
  plain.phrase,
]

const READER = '/read/work-1'
const AT_HOME: [string, unknown] = ['/', null]
const OPENED: [string, unknown] = [READER, null]
const UNDER: [string, unknown] = [SERIES, { [CAME_FROM]: '/' }]
const OVER: [string, unknown] = [READER, { [BENEATH]: SERIES }]

describe('a reader puts its series page under itself (O431)', () => {
  afterEach(() => {
    Reflect.deleteProperty(navigator, 'userActivation')
    vi.restoreAllMocks()
  })

  it('stamps its entry where it was opened from its series page, by a replace', async () => {
    const r = reading(['/', '/series/41?edition=en'], SERIES)
    await go(() => r.drawn().navigate(READER))
    expect([r.history(), r.drawn().arrival]).toEqual([
      {
        entries: [AT_HOME, ['/series/41?edition=en', null], OVER],
        index: 2,
      },
      'REPLACE',
    ])
  })

  it('puts the series page under a reader opened from elsewhere at the first input, never drawing it', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    const armed = r.history()
    await input(keydown('ArrowRight'))
    const rewritten = [r.history(), r.paths.includes(SERIES)]
    await go(() => r.drawn().navigate(-1))
    expect([armed, rewritten, where(r.drawn()), plainly(r.drawn())]).toEqual([
      { entries: [AT_HOME, OPENED], index: 1 },
      [{ entries: [AT_HOME, UNDER, OVER], index: 2 }, false],
      [SERIES, 'POP'],
      ['Library', 'the library'],
    ])
  })

  it('names the reader it pushed again for its series page', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await input(keydown('ArrowRight'))
    expect(plainly(r.drawn())).toEqual(['Series', 'the series'])
  })

  it('puts it there once, though a tap is heard twice', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await input(pointer('pointerup', 'touch'), touchend())
    const tapped = r.history()
    await input(keydown('ArrowRight'))
    expect([tapped, r.history()]).toEqual([
      { entries: [AT_HOME, UNDER, OVER], index: 2 },
      { entries: [AT_HOME, UNDER, OVER], index: 2 },
    ])
  })

  it('puts nothing under it once a way out is taken, though an input comes before it is drawn', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await act(async () => {
      r.drawn().back.go()
      document.body.dispatchEvent(keydown('ArrowRight'))
    })
    expect(r.history()).toEqual({
      entries: [AT_HOME, [SERIES, null]],
      index: 1,
    })
  })

  it('puts nothing under it once a step through the history has begun', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await act(async () => {
      window.dispatchEvent(new PopStateEvent('popstate'))
    })
    await input(keydown('ArrowRight'))
    expect(r.history()).toEqual({ entries: [AT_HOME, OPENED], index: 1 })
  })

  it('puts nothing under it on Escape, and on L puts it there and steps back to it', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await input(keydown('Escape'))
    const escaped = r.history()
    // As `App.tsx` hears its keys: bubbling, on `window`, from the last drawing.
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'l') r.drawn().back.go()
    }
    window.addEventListener('keydown', onKey)
    try {
      await input(keydown('l'))
    } finally {
      window.removeEventListener('keydown', onKey)
    }
    expect([escaped, r.history(), where(r.drawn())]).toEqual([
      { entries: [AT_HOME, OPENED], index: 1 },
      { entries: [AT_HOME, UNDER, OVER], index: 1 },
      [SERIES, 'POP'],
    ])
  })

  it('puts nothing under a reader with no series page, on any input', async () => {
    // Reloaded with the mark, its answer not yet in; then a volume opened.
    const r = reading(['/', { pathname: READER, state: { [BENEATH]: SERIES } }])
    const inputs = () => [
      keydown('ArrowRight'),
      pointer('pointerdown', 'mouse'),
      pointer('pointerup', 'touch'),
      touchend(),
    ]
    await input(...inputs())
    const reloaded = r.history()
    await go(() => r.drawn().navigate('/read/work-2'))
    await input(...inputs())
    expect([reloaded, r.history()]).toEqual([
      { entries: [AT_HOME, OVER], index: 1 },
      { entries: [AT_HOME, OVER, ['/read/work-2', null]], index: 2 },
    ])
  })

  it('puts it under a second volume opened under the same route, too', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await input(keydown('ArrowRight'))
    await go(() => r.drawn().navigate('/read/work-2'))
    await input(keydown('ArrowRight'))
    expect(r.history()).toEqual({
      entries: [
        AT_HOME,
        UNDER,
        OVER,
        [SERIES, { [CAME_FROM]: READER }],
        ['/read/work-2', { [BENEATH]: SERIES }],
      ],
      index: 4,
    })
  })

  it('puts the series page in the place of a second volume left before an input', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await input(keydown('ArrowRight'))
    await go(() => r.drawn().navigate('/read/work-2'))
    await go(() => r.drawn().back.go())
    expect([where(r.drawn()), r.history().index]).toEqual([
      [SERIES, 'REPLACE'],
      3,
    ])
  })

  it('puts it there as the answer lands where the opening click is fresh, for each volume', async () => {
    Object.defineProperty(navigator, 'userActivation', {
      configurable: true,
      value: { isActive: true },
    })
    let later = 0
    const now = performance.now.bind(performance)
    vi.spyOn(performance, 'now').mockImplementation(() => now() + later)
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    const first = r.history()
    later = 6000
    await go(() => r.drawn().navigate('/read/work-2'))
    expect([first, r.history()]).toEqual([
      { entries: [AT_HOME, UNDER, OVER], index: 2 },
      {
        entries: [
          AT_HOME,
          UNDER,
          OVER,
          [SERIES, { [CAME_FROM]: READER }],
          ['/read/work-2', { [BENEATH]: SERIES }],
        ],
        index: 4,
      },
    ])
  })

  it('waits for an input in Firefox, fresh click or not', async () => {
    Object.defineProperty(navigator, 'userActivation', {
      configurable: true,
      value: { isActive: true },
    })
    vi.spyOn(Navigator.prototype, 'userAgent', 'get').mockReturnValue(
      'Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0',
    )
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    expect(r.history()).toEqual({ entries: [AT_HOME, OPENED], index: 1 })
  })
})

describe('a press of the way out, while its step is still to land (O431)', () => {
  it('steps back once for L pressed twice and the button once, the first L putting the series page under the reader', async () => {
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    r.hold()
    // As `App.tsx` hears its keys: bubbling, on `window`, with the way out of
    // the drawing it was set up on.
    const leave = r.drawn().back.go
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'l') leave()
    }
    window.addEventListener('keydown', onKey)
    try {
      await input(keydown('l'))
      const pressed = [r.history(), where(r.drawn())]
      await input(keydown('l'))
      await go(() => r.drawn().back.go())
      expect([pressed, await r.land(), r.history(), where(r.drawn())]).toEqual([
        [{ entries: [AT_HOME, UNDER, OVER], index: 2 }, [READER, 'PUSH']],
        1,
        { entries: [AT_HOME, UNDER, OVER], index: 1 },
        [SERIES, 'POP'],
      ])
    } finally {
      window.removeEventListener('keydown', onKey)
    }
  })

  it('steps back once for a press as the reader stamps its entry and a press after', async () => {
    const r = reading(['/', '/series/41?edition=en'], SERIES)
    r.hold()
    r.afterReplace(() => r.drawn().back.go())
    await go(() => r.drawn().navigate(READER))
    const stamped = [r.history(), where(r.drawn())]
    await go(() => r.drawn().back.go())
    expect([stamped, await r.land(), where(r.drawn())]).toEqual([
      [
        {
          entries: [AT_HOME, ['/series/41?edition=en', null], OVER],
          index: 2,
        },
        [READER, 'REPLACE'],
      ],
      1,
      ['/series/41?edition=en', 'POP'],
    ])
  })

  it('steps back from the reader’s marked entry once a press on a page over it has stepped back there (O434)', async () => {
    // One hook draws both entries here: the step lands by POP on an entry
    // that carries the mark, as the reader's own stamp does, and is still
    // another entry.
    const r = reading(['/'], SERIES)
    await go(() => r.drawn().navigate(READER))
    await input(keydown('ArrowRight'))
    await go(() => r.drawn().navigate('/about'))
    await go(() => r.drawn().back.go())
    const back = where(r.drawn())
    await go(() => r.drawn().back.go())
    expect([back, where(r.drawn())]).toEqual([
      [READER, 'POP'],
      [SERIES, 'POP'],
    ])
  })
})

describe('a reader its series page is not under keeps O422’s rule, when its route asks (O431)', () => {
  afterEach(() => {
    Reflect.deleteProperty(navigator, 'userActivation')
  })

  it.each([
    ['', false],
    [', though the click that opened it is fresh', true],
  ])(
    'puts nothing under a reader opened from elsewhere, on any input%s, and steps back to where it was opened from',
    async (_, fresh) => {
      if (fresh)
        Object.defineProperty(navigator, 'userActivation', {
          configurable: true,
          value: { isActive: true },
        })
      const r = reading(['/'], SERIES, false)
      await go(() => r.drawn().navigate(READER))
      const answered = r.history()
      await input(
        keydown('ArrowRight'),
        pointer('pointerdown', 'mouse'),
        touchend(),
      )
      const after = [r.history(), named(r.drawn())]
      await go(() => r.drawn().back.go())
      expect([answered, after, where(r.drawn())]).toEqual([
        { entries: [AT_HOME, OPENED], index: 1 },
        [{ entries: [AT_HOME, OPENED], index: 1 }, ['Library', 'the library']],
        ['/', 'POP'],
      ])
    },
  )

  it('stamps its entry where it was opened from its own series page, and steps back to that page', async () => {
    const r = reading(['/', '/series/41?edition=en'], SERIES, false)
    await go(() => r.drawn().navigate(READER))
    const stamped = [r.history(), r.drawn().arrival, named(r.drawn())]
    await go(() => r.drawn().back.go())
    expect([stamped, where(r.drawn())]).toEqual([
      [
        {
          entries: [AT_HOME, ['/series/41?edition=en', null], OVER],
          index: 2,
        },
        'REPLACE',
        ['Series', 'the series'],
      ],
      ['/series/41?edition=en', 'POP'],
    ])
  })

  it('steps back to another series’ page it was pushed from, named for it', async () => {
    const r = reading(['/', '/series/60'], SERIES, false)
    await go(() => r.drawn().navigate(READER))
    const opened = [r.history(), named(r.drawn())]
    await go(() => r.drawn().back.go())
    expect([opened, where(r.drawn())]).toEqual([
      [
        { entries: [AT_HOME, ['/series/60', null], OPENED], index: 2 },
        ['Series', 'the series'],
      ],
      ['/series/60', 'POP'],
    ])
  })

  it.each([
    ['neither mark nor record, named Back', undefined, ['Back', '']],
    ['the mark, named Series', { [BENEATH]: SERIES }, ['Series', 'the series']],
  ])('steps back after a reload that left %s', async (_, state, name) => {
    const drawn = at(['/', { pathname: READER, state }], 1, SERIES, false)
    const called = named(drawn())
    await go(() => drawn().back.go())
    expect([called, where(drawn())]).toEqual([name, ['/', 'POP']])
  })

  it('pushes the library, marked to land at its kept scroll, where no entry comes before it', async () => {
    const drawn = at([READER], 0, SERIES, false)
    const name = named(drawn())
    await go(() => drawn().back.go())
    expect([name, where(drawn()), drawn().location.state]).toEqual([
      ['Library', 'the library'],
      ['/', 'PUSH'],
      { [RESTORE_SCROLL]: true },
    ])
  })

  it('on L steps back to where it was opened from, the reader forward of it', async () => {
    const r = reading(['/'], SERIES, false)
    await go(() => r.drawn().navigate(READER))
    // As `App.tsx` hears its keys: bubbling, on `window`, from the last drawing.
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'l') r.drawn().back.go()
    }
    window.addEventListener('keydown', onKey)
    try {
      await input(keydown('l'))
    } finally {
      window.removeEventListener('keydown', onKey)
    }
    expect([r.history(), where(r.drawn())]).toEqual([
      { entries: [AT_HOME, OPENED], index: 0 },
      ['/', 'POP'],
    ])
  })
})
