import { act } from 'react'
import { MemoryRouter, useLocation, useNavigationType } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AccountMenu } from '../src/AccountMenu'
import { SCROLL_TO_TOP } from '../src/scrollKeeper'
import { Sidebar } from '../src/Sidebar'
import { mount, rerender } from './support/mount'

const at = (ui: React.ReactNode, path = '/') =>
  mount(<MemoryRouter initialEntries={[path]}>{ui}</MemoryRouter>)

describe('the sidebar', () => {
  it('draws the Libraries heading as the link to every library, with no library to list too', () => {
    // O220: `/` is where an account with no library is told it has none, so
    // the heading that leads there is drawn for that account as well. The
    // column has no other link.
    const host = at(<Sidebar libraries={[]} />)
    const heading = host.querySelector('h2.sidebar-heading')
    expect(heading?.textContent).toBe('Libraries')
    expect(heading?.querySelector('a')?.getAttribute('href')).toBe('/')
    expect(
      [...host.querySelectorAll('a')].map((a) => a.getAttribute('href')),
    ).toEqual(['/'])
    expect(host.querySelector('.sidebar-libraries')).toBeNull()
  })

  it('lists every library it is given, each linking to its own page', () => {
    const host = at(
      <Sidebar
        libraries={[
          { id: 4, name: 'Manga', scanned: true },
          { id: 9, name: 'Comics', scanned: false },
        ]}
      />,
    )
    const links = [...host.querySelectorAll('.sidebar-libraries a')]
    expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
      ['Manga', '/?library=4'],
      ['Comics', '/?library=9'],
    ])
    expect(host.querySelector('h2')?.textContent).toBe('Libraries')
  })

  it('marks the Libraries heading only on / with no library, and a library page its own library', () => {
    const on = (path: string) =>
      at(
        <Sidebar
          libraries={[
            { id: 4, name: 'Manga', scanned: true },
            { id: 9, name: 'Comics', scanned: true },
          ]}
        />,
        path,
      )
    const current = (host: HTMLElement) =>
      [...host.querySelectorAll('[aria-current="page"]')].map((a) =>
        a.getAttribute('href'),
      )
    expect(current(on('/'))).toEqual(['/'])
    expect(current(on('/?q=oshi'))).toEqual(['/'])
    expect(current(on('/?library=9'))).toEqual(['/?library=9'])
    // A `?library=` that names no library is the library page's "no such
    // library", which is neither every library nor one of these.
    expect(current(on('/?library=abc'))).toEqual([])
    // More stands in for the screens behind it (`AccountMenu.tsx`).
    expect(current(on('/config/libraries'))).toEqual([])
    expect(current(on('/series/3'))).toEqual([])
  })

  it('marks every link SCROLL_TO_TOP, and makes the one to the address shown a replace', async () => {
    // The keeper keeps a replace's scroll, as a chip's press needs; the mark
    // is what lands a click on the view already drawn at the top.
    function Where() {
      const location = useLocation()
      const type = useNavigationType()
      return (
        <output>
          {JSON.stringify([
            type,
            location.pathname + location.search,
            location.state,
          ])}
        </output>
      )
    }
    const host = at(
      <>
        <Sidebar
          libraries={[
            { id: 4, name: 'Manga', scanned: true },
            { id: 9, name: 'Comics', scanned: true },
          ]}
        />
        <Where />
      </>,
      '/?library=9&show=unread',
    )
    const follow = async (selector: string) => {
      await act(async () => {
        host.querySelector<HTMLAnchorElement>(selector)?.click()
      })
      return JSON.parse(host.querySelector('output')?.textContent ?? 'null')
    }
    const top = { [SCROLL_TO_TOP]: true }
    expect([
      await follow('a[href="/?library=9&show=unread"]'),
      await follow('h2 a'),
      await follow('h2 a'),
      await follow('a[href="/?library=4"]'),
    ]).toEqual([
      ['REPLACE', '/?library=9&show=unread', top],
      ['PUSH', '/', top],
      ['REPLACE', '/', top],
      ['PUSH', '/?library=4', top],
    ])
  })

  describe('beside the Library view, its chip and sort on each library’s link', () => {
    const on = (path: string) =>
      at(
        <Sidebar
          libraries={[
            { id: 4, name: 'Manga', scanned: true },
            { id: 9, name: 'Comics', scanned: true },
          ]}
        />,
        path,
      )
    const hrefs = (host: HTMLElement) =>
      [...host.querySelectorAll('.sidebar-libraries a')].map((a) =>
        a.getAttribute('href'),
      )
    const heading = (host: HTMLElement) =>
      host.querySelector('h2.sidebar-heading a')?.getAttribute('href')

    it('carries ?show= and ?sort= to each library, and nothing to the heading', () => {
      const multi = on('/?show=multi')
      expect(hrefs(multi)).toEqual([
        '/?library=4&show=multi',
        '/?library=9&show=multi',
      ])
      expect(heading(multi)).toBe('/')
      const both = on('/?library=9&show=unread&sort=added&q=oshi')
      expect(hrefs(both)).toEqual([
        '/?library=4&show=unread&sort=added',
        '/?library=9&show=unread&sort=added',
      ])
      expect(heading(both)).toBe('/')
    })

    it('carries no value that names no chip or sort, and nothing from a screen that is not the Library view', () => {
      expect(hrefs(on('/?show=bogus&sort=added'))).toEqual([
        '/?library=4&sort=added',
        '/?library=9&sort=added',
      ])
      expect(hrefs(on('/?show=multi&sort=bogus'))).toEqual([
        '/?library=4&show=multi',
        '/?library=9&show=multi',
      ])
      expect(hrefs(on('/series/3?show=multi&sort=added'))).toEqual([
        '/?library=4',
        '/?library=9',
      ])
    })
  })
})

/** The panel More controls, found by the id its button names. */
function panelOf(host: HTMLElement): HTMLElement {
  const id = host
    .querySelector('button.account-button')
    ?.getAttribute('aria-controls')
  const panel = id ? document.getElementById(id) : null
  if (!panel || !host.contains(panel)) throw new Error('More controls nothing')
  return panel
}

function moreButton(host: HTMLElement): HTMLButtonElement {
  const button = host.querySelector<HTMLButtonElement>('button.account-button')
  if (!button) throw new Error('More is not a button')
  return button
}

/** Opens More and hands back its panel, failing if it did not open. */
function openMore(host: HTMLElement): HTMLElement {
  const panel = panelOf(host)
  expect(panel.hidden).toBe(true)
  act(() => {
    moreButton(host).click()
  })
  expect(moreButton(host).getAttribute('aria-expanded')).toBe('true')
  expect(panel.hidden).toBe(false)
  return panel
}

/** Each link in `root`, as its text and where it goes. */
const links = (root: ParentNode) =>
  [...root.querySelectorAll('a')].map((a) => [
    a.textContent,
    a.getAttribute('href'),
  ])

describe('More, at the sidebar’s foot', () => {
  it('opens an administrator’s Proposals, Configuration and Account', () => {
    const host = at(<AccountMenu admin={true} />)
    expect(links(openMore(host))).toEqual([
      ['Proposals', '/proposals'],
      ['Configuration', '/config'],
      ['Account', '/account'],
    ])
  })

  it('is a disclosure: a button naming its panel, and a panel of links that is no dialog', () => {
    // Spec §1: the panel holds only links, so neither a menu's arrow keys
    // nor a dialog's semantics are owed. `aria-haspopup` would announce one.
    const host = at(<AccountMenu admin={true} />)
    const button = moreButton(host)
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(button.hasAttribute('aria-haspopup')).toBe(false)
    const panel = openMore(host)
    expect(panel.hasAttribute('role')).toBe(false)
    expect(panel.hasAttribute('aria-label')).toBe(false)
    expect(
      [...panel.querySelectorAll('li > *')].map((el) => el.tagName),
    ).toEqual(['A', 'A', 'A'])
  })

  it('gives anyone else More as a plain link to the account screen', () => {
    // O94: every account has a password to change, so this is the one entry
    // that follows no role. A menu of that one entry would be a click for
    // nothing, so there is no button, no panel and no badge.
    const host = at(<AccountMenu admin={false} openProposals={7} />)
    const more = host.querySelector('.account-button')
    expect(more?.tagName).toBe('A')
    expect(more?.getAttribute('href')).toBe('/account')
    expect(more?.textContent).toBe('More')
    expect(more?.hasAttribute('aria-expanded')).toBe(false)
    expect(more?.hasAttribute('aria-haspopup')).toBe(false)
    expect(links(host)).toEqual([['More', '/account']])
    expect(host.querySelector('button')).toBeNull()
    expect(host.querySelector('.count')).toBeNull()
  })

  it('carries the open count on the button and on the Proposals link once read', () => {
    // C3 §3: somebody learns the inbox is non-empty without going to look,
    // so the count is on the button the closed panel leaves showing. Each
    // number is read through its own element, so a count drawn in the wrong
    // place, or the label drawn twice, cannot pass as `'Proposals7'`.
    const host = at(<AccountMenu admin={true} openProposals={7} />)
    expect(moreButton(host).querySelector('.count')?.textContent).toBe('7')
    expect(
      openMore(host).querySelector('a[href="/proposals"] .count')?.textContent,
    ).toBe('7')
  })

  it('draws no badge on More at zero, and keeps the 0 on the Proposals entry (O424)', () => {
    // More asks for a visit only while the inbox holds something. `0` and
    // "not read yet" are still different facts, so the entry inside says the
    // 0, one press away.
    const host = at(<AccountMenu admin={true} openProposals={0} />)
    expect(moreButton(host).querySelector('.count')).toBeNull()
    expect(moreButton(host).getAttribute('aria-label')).toBe('More')
    const proposals = openMore(host).querySelector('a[href="/proposals"]')
    expect(proposals?.querySelector('.count')?.textContent).toBe('0')
    expect(proposals?.getAttribute('aria-label')).toBe('Proposals, 0 open')
  })

  it('draws no count at all until one has been read', () => {
    // Never fetched, still in flight and failed are one value, as
    // `useLibraryStatus` reads them for the same three states.
    const host = at(<AccountMenu admin={true} />)
    expect(moreButton(host).querySelector('.count')).toBeNull()
    const proposals = openMore(host).querySelector('a[href="/proposals"]')
    expect(proposals?.querySelector('.count')).toBeNull()
    expect(proposals?.textContent).toBe('Proposals')
  })

  it.each([
    [17, 'Proposals, 17 open', 'More, 17 open proposals', '17'],
    [5, 'Proposals, 5 open', 'More, 5 open proposals', '5'],
    [0, 'Proposals, 0 open', 'More', undefined],
  ])(
    'names the link, and the button while it has a badge, with what the count %i counts — O164, O424',
    (count, link, button, badge) => {
      // Without a label the number runs into the word ("Proposals17"); the
      // labels say what it counts.
      const host = at(<AccountMenu admin={true} openProposals={count} />)
      expect(moreButton(host).querySelector('.count')?.textContent).toBe(badge)
      expect(moreButton(host).getAttribute('aria-label')).toBe(button)
      expect(
        openMore(host)
          .querySelector('a[href="/proposals"]')
          ?.getAttribute('aria-label'),
      ).toBe(link)
    },
  )

  it('leaves the link and the button named by their text until a count is read — O164', () => {
    const host = at(<AccountMenu admin={true} />)
    expect(moreButton(host).getAttribute('aria-label') ?? 'More').toBe('More')
    expect(
      openMore(host)
        .querySelector('a[href="/proposals"]')
        ?.hasAttribute('aria-label'),
    ).toBe(false)
  })

  it.each([
    ['/proposals', true],
    ['/config', true],
    ['/config/libraries', true],
    ['/account', true],
    ['/', false],
    ['/?library=1', false],
    ['/series/3', false],
    ['/configure', false],
  ])('marks an administrator’s More on %s: %s', (path, here) => {
    // Spec §1: the screens behind More are marked on More.
    const host = at(<AccountMenu admin={true} />, path)
    expect(moreButton(host).hasAttribute('data-here')).toBe(here)
  })

  it.each([
    ['/account', true],
    ['/proposals', true],
    ['/config', true],
    ['/config/libraries', true],
    ['/', false],
    ['/configure', false],
  ])('marks anyone else’s More on %s: %s', (path, here) => {
    // The same screens as an administrator's: a non-admin can reach
    // `/proposals` and `/config` too, and is shown the server's refusal
    // there. `aria-current` stays the link's own, true only where it leads.
    const host = at(<AccountMenu admin={false} />, path)
    const more = host.querySelector('a.account-button')
    expect(more?.hasAttribute('data-here')).toBe(here)
    expect(more?.getAttribute('aria-current')).toBe(
      path === '/account' ? 'page' : null,
    )
  })

  it.each([
    ['/config/libraries', ['/config']],
    ['/proposals', ['/proposals']],
    ['/account', ['/account']],
  ])(
    'marks the screen it is on inside the open panel, on %s',
    (path, marked) => {
      // Every configuration tab marks Configuration, so its link takes no
      // `end`.
      const host = at(<AccountMenu admin={true} />, path)
      const panel = openMore(host)
      expect(
        [...panel.querySelectorAll('[aria-current="page"]')].map((a) =>
          a.getAttribute('href'),
        ),
      ).toEqual(marked)
    },
  )

  it('puts every link More holds inside a navigation landmark of its own', () => {
    const navOf = (root: ParentNode) =>
      [...root.querySelectorAll('a')].map((a) =>
        a.closest('nav')?.getAttribute('aria-label'),
      )
    const admin = at(<AccountMenu admin={true} />)
    expect(navOf(openMore(admin))).toEqual(['More', 'More', 'More'])
    const reader = at(<AccountMenu admin={false} />)
    expect(navOf(reader)).toEqual(['More'])
  })

  it('closes when a link in it is followed', async () => {
    // `click()` sends no pointer press and moves no focus, so only the
    // navigation itself can close the panel here.
    const host = at(<AccountMenu admin={true} />)
    const panel = openMore(host)
    const config = panel.querySelector<HTMLAnchorElement>('a[href="/config"]')
    if (!config) throw new Error('no Configuration link')
    await act(async () => {
      config.click()
    })
    expect(moreButton(host).getAttribute('aria-expanded')).toBe('false')
    expect(panel.hidden).toBe(true)
    expect(moreButton(host).hasAttribute('data-here')).toBe(true)
  })

  it('leaves focus on the button when it opens', () => {
    // The disclosure pattern: the panel follows the button, and Tab from the
    // button enters it (`Shell.browser.test.tsx` presses the real key).
    const host = at(<AccountMenu admin={true} />)
    act(() => {
      moreButton(host).focus()
    })
    openMore(host)
    expect(document.activeElement).toBe(moreButton(host))
  })

  it('closes on Escape from inside the panel and gives focus back to the button', () => {
    const host = at(<AccountMenu admin={true} />)
    const panel = openMore(host)
    const config = need<HTMLElement>(panel, 'a[href="/config"]')
    act(() => {
      config.focus()
    })
    act(() => {
      config.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
      )
    })
    expect(moreButton(host).getAttribute('aria-expanded')).toBe('false')
    expect(panel.hidden).toBe(true)
    expect(document.activeElement).toBe(moreButton(host))
  })

  it('closes on Escape from the button', () => {
    const host = at(<AccountMenu admin={true} />)
    act(() => {
      moreButton(host).focus()
    })
    const panel = openMore(host)
    act(() => {
      moreButton(host).dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }),
      )
    })
    expect(moreButton(host).getAttribute('aria-expanded')).toBe('false')
    expect(panel.hidden).toBe(true)
    expect(document.activeElement).toBe(moreButton(host))
  })

  it('stays open for any key but Escape', () => {
    const host = at(<AccountMenu admin={true} />)
    openMore(host)
    act(() => {
      document.activeElement?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    expect(moreButton(host).getAttribute('aria-expanded')).toBe('true')
  })

  /**
   * jsdom has no pointer to press with, so the press is dispatched where it
   * would land. `Shell.browser.test.tsx` presses with a real mouse; this
   * half is the one CI runs.
   */
  it('closes on a press outside it, and not on one inside it or on its button', () => {
    const host = at(<AccountMenu admin={true} />)
    const panel = openMore(host)
    const press = (target: Element) => {
      act(() => {
        target.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
      })
    }
    press(need(panel, 'a[href="/config"]'))
    expect(panel.hidden).toBe(false)
    press(moreButton(host))
    expect(panel.hidden).toBe(false)
    press(document.body)
    expect(panel.hidden).toBe(true)
    expect(moreButton(host).getAttribute('aria-expanded')).toBe('false')
  })

  it('closes when focus moves outside it, and not while it moves inside', () => {
    const host = at(<AccountMenu admin={true} />)
    const panel = openMore(host)
    const outside = document.createElement('button')
    document.body.append(outside)
    try {
      act(() => {
        need<HTMLElement>(panel, 'a[href="/account"]').focus()
      })
      expect(panel.hidden).toBe(false)
      act(() => {
        moreButton(host).focus()
      })
      expect(panel.hidden).toBe(false)
      act(() => {
        outside.focus()
      })
      expect(panel.hidden).toBe(true)
    } finally {
      outside.remove()
    }
  })
})

/**
 * jsdom lays nothing out, so the edges More's panel reads are stubbed onto
 * the boxes it reads them from, and the observer is one the test answers
 * itself, as the browser does after a change of size. That the panel then
 * fits a short window, and scrolls, is `Shell.browser.test.tsx`'s to show.
 */
class StubResizeObserver {
  static made: StubResizeObserver[] = []
  readonly changed: () => void
  readonly observed: Element[] = []
  connected = true
  constructor(changed: () => void) {
    this.changed = changed
    StubResizeObserver.made.push(this)
  }
  observe(target: Element) {
    this.observed.push(target)
  }
  unobserve() {}
  disconnect() {
    this.connected = false
  }
}

/** Where the stubbed layout draws the panel's foot and the column's top. */
const edges = { panelBottom: 0, columnTop: 0 }

/** Stubs the panel's foot onto it, at `edges.panelBottom`. */
function measuredPanel(host: HTMLElement): HTMLElement {
  const panel = panelOf(host)
  panel.getBoundingClientRect = () => ({ bottom: edges.panelBottom }) as DOMRect
  return panel
}

/**
 * More at the foot of a column that clips it, nested as `Shell.tsx` nests
 * them, the column with 12px of padding at its top and the panel closed.
 */
function inColumn() {
  const host = at(
    <div id="library-menu" style={{ overflowY: 'auto', paddingTop: '12px' }}>
      <div className="sidebar-foot">
        <AccountMenu admin={true} />
      </div>
    </div>,
  )
  const column = need<HTMLElement>(host, '#library-menu')
  column.getBoundingClientRect = () => ({ top: edges.columnTop }) as DOMRect
  return { host, column, panel: measuredPanel(host) }
}

/** The room the panel is held to, or `''` while nothing holds it. */
const room = (panel: HTMLElement) =>
  panel.style.getPropertyValue('--room-above')

/** A watched box changed size: each observer still watching reports it. */
function resized() {
  act(() => {
    for (const observer of StubResizeObserver.made) {
      if (observer.connected) observer.changed()
    }
  })
}

/** Whether each observer made so far is still watching. */
const connected = () => StubResizeObserver.made.map((each) => each.connected)

describe('More’s panel, held to the room above More', () => {
  const held = globalThis.ResizeObserver
  beforeEach(() => {
    StubResizeObserver.made = []
    globalThis.ResizeObserver =
      StubResizeObserver as unknown as typeof ResizeObserver
    edges.columnTop = 40
    edges.panelBottom = 400
  })
  afterEach(() => {
    globalThis.ResizeObserver = held
    vi.restoreAllMocks()
  })

  it.each([
    ['up to the column’s top, past its padding', 40, 400, '348px'],
    ['up to the window’s top, below the column’s', -30, 400, '400px'],
    ['of none, where its foot is above the column’s top', 40, 30, '0px'],
  ])(
    'holds an open panel to the room above its foot: %s',
    (_, columnTop, panelBottom, expected) => {
      edges.columnTop = columnTop
      edges.panelBottom = panelBottom
      const { host, panel } = inColumn()
      expect(room(panel)).toBe('')
      openMore(host)
      expect(room(panel)).toBe(expected)
    },
  )

  it('holds an open panel to the room up to the window’s top where nothing above More clips it, and watches every box up to the root', () => {
    edges.panelBottom = 300
    const host = at(<AccountMenu admin={true} />)
    const panel = measuredPanel(host)
    openMore(host)
    expect(room(panel)).toBe('300px')
    expect(StubResizeObserver.made.map((each) => each.observed)).toEqual([
      [need(host, 'nav'), host, document.body, document.documentElement],
    ])
  })

  it('watches the boxes that set the room, More’s, the foot’s and the column’s, and none the column sits in', () => {
    const { host, column } = inColumn()
    openMore(host)
    expect(StubResizeObserver.made.map((each) => each.observed)).toEqual([
      [need(host, 'nav'), need(host, '.sidebar-foot'), column],
    ])
  })

  it('measures again when a box it watches changes size, as larger text raises More', () => {
    const { host, panel } = inColumn()
    openMore(host)
    expect(room(panel)).toBe('348px')
    edges.panelBottom = 360
    resized()
    expect(room(panel)).toBe('308px')
    edges.columnTop = 0
    resized()
    expect(room(panel)).toBe('348px')
  })

  it('writes the room only when it changes, so the report an observer makes as it starts to watch writes nothing', () => {
    const { host, panel } = inColumn()
    openMore(host)
    const write = vi.spyOn(panel.style, 'setProperty')
    resized()
    expect(write).not.toHaveBeenCalled()
    edges.panelBottom = 360
    resized()
    resized()
    expect(write.mock.calls).toEqual([['--room-above', '308px']])
  })

  it('stops watching when the panel closes, and measures afresh when it opens again', () => {
    const { host, panel } = inColumn()
    openMore(host)
    act(() => {
      moreButton(host).click()
    })
    expect(panel.hidden).toBe(true)
    expect(connected()).toEqual([false])
    edges.panelBottom = 200
    resized()
    expect(room(panel)).toBe('348px')
    openMore(host)
    expect(room(panel)).toBe('148px')
    expect(connected()).toEqual([false, true])
  })

  it('stops watching when More goes while its panel is open', () => {
    const { host } = inColumn()
    openMore(host)
    rerender(<div />)
    expect([connected(), host.querySelector('nav')]).toEqual([[false], null])
  })
})

function need<T extends Element = Element>(
  root: ParentNode,
  selector: string,
): T {
  const found = root.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}
