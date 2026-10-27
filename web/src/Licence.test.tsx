import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { act } from 'react'
import {
  type Location,
  MemoryRouter,
  type NavigateFunction,
  useLocation,
  useNavigate,
} from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import { Licence } from './Licence'
import { focused } from './testing/focused'
import { licenceTextsLanded } from './testing/licence'
import { mount } from './testing/mount'

/**
 * Each licence file as the disk holds it, read here rather than through the
 * bundle the page draws from, so a text the page bundled from another file
 * is not the one it is checked against.
 */
const read = (file: string) => readFileSync(file, 'utf8')
const local = (path: string) =>
  read(fileURLToPath(new URL(path, import.meta.url)))
const resolve = createRequire(import.meta.url).resolve
const GPL = local('../../LICENSE')
const SHIPPORI = read(resolve('@fontsource/shippori-mincho-b1/LICENSE'))
const ZEN = read(resolve('@fontsource/zen-kaku-gothic-new/LICENSE'))
const MARKS = local('./styles/fonts/OFL.txt')

/** Mounts `Page` at `entry` and lets its texts land. */
async function settle(
  Page: () => React.ReactNode = Licence,
  entry = '/about/licence',
) {
  const host = mount(
    <MemoryRouter initialEntries={[entry]}>
      <Page />
    </MemoryRouter>,
  )
  await licenceTextsLanded()
  return host
}

/**
 * The page under a router holding `entries`, at the last of them, its texts
 * landed, and `go`, which moves through that history as the router's own
 * `navigate` does: by a number of entries, a `POP`, or to an address, a
 * push. Declared inside a function, for the reason `Gate.test.tsx`'s
 * `probe` gives.
 */
async function atLast(entries: readonly string[]) {
  let navigate: NavigateFunction | undefined
  function Steer() {
    navigate = useNavigate()
    return null
  }
  const host = mount(
    <MemoryRouter
      initialEntries={[...entries]}
      initialIndex={entries.length - 1}
    >
      <Licence />
      <Steer />
    </MemoryRouter>,
  )
  await licenceTextsLanded()
  const go = async (to: number | string) => {
    await act(async () => {
      if (typeof to === 'number') await navigate?.(to)
      else await navigate?.(to)
    })
  }
  return { host, go }
}

/**
 * The page at `entry`, its texts landed, and `address`, which reads the
 * router's location as it is when called. Declared inside a function, for
 * the reason `Gate.test.tsx`'s `probe` gives.
 */
async function withAddress(entry: string) {
  let now: Location | undefined
  function Where() {
    now = useLocation()
    return null
  }
  const host = mount(
    <MemoryRouter initialEntries={[entry]}>
      <Licence />
      <Where />
    </MemoryRouter>,
  )
  await licenceTextsLanded()
  const address = () => `${now?.pathname}${now?.search}${now?.hash}`
  return { host, address }
}

/** Where a licence draws Back to top: its heading's line, or its box's foot. */
type Place = 'heading' | 'foot'

/** The Back to top links inside `el`. */
const topsIn = (el: Element | null | undefined) =>
  [...(el?.querySelectorAll<HTMLAnchorElement>('a') ?? [])].filter((link) =>
    link.textContent?.endsWith('Back to top'),
  )

/** The Back to top link at `place` in the section headed `name`. */
function toTop(
  host: HTMLElement,
  name: string,
  place: Place,
): HTMLAnchorElement {
  const links = topsIn(
    section(host, name).querySelector(
      place === 'heading'
        ? ':scope > .settings-section-head'
        : ':scope > .settings-box',
    ),
  )
  if (links.length !== 1) {
    throw new Error(`${links.length} Back to top links at "${name}"'s ${place}`)
  }
  return links[0] as HTMLAnchorElement
}

const OFL = 'licensed under the SIL Open Font License, version 1.1 (OFL-1.1).'

/** The section whose own heading is `name`. */
function section(host: HTMLElement, name: string): HTMLElement {
  const heading = [...host.querySelectorAll('h2')].find(
    (h2) => h2.textContent === name,
  )
  const found = heading?.closest('section')
  if (!found || found === host.querySelector('section[aria-label="Licence"]')) {
    throw new Error(`no section headed "${name}"`)
  }
  return found
}

/**
 * What `section`'s box says its work is licensed under, and that licence's
 * text.
 */
function parts(section: HTMLElement) {
  const box = section.querySelector(':scope > .settings-box')
  if (!box) throw new Error('no box')
  const texts = box.querySelectorAll('pre')
  if (texts.length !== 1) throw new Error(`${texts.length} texts, not one`)
  return {
    terms: box.querySelector('p')?.textContent ?? '',
    text: texts[0]?.textContent ?? '',
  }
}

describe('Licence', () => {
  it('draws taiyaku’s licence first and then each font’s', async () => {
    const host = await settle()
    expect(
      [...host.querySelectorAll('.settings-section h2')].map(
        (h2) => h2.textContent,
      ),
    ).toEqual([
      'taiyaku',
      'Shippori Mincho B1',
      'Zen Kaku Gothic New',
      'taiyaku marks',
    ])
  })

  it('says taiyaku is GPL-3.0-or-later, above the full text of the repository’s LICENSE', async () => {
    const { terms, text } = parts(section(await settle(), 'taiyaku'))
    expect(terms).toBe(
      'taiyaku is licensed under the GNU General Public License, version 3 or any later version (GPL-3.0-or-later).',
    )
    expect(text).toContain('GNU GENERAL PUBLIC LICENSE')
    expect(text).toContain('Version 3, 29 June 2007')
    expect(text).toBe(GPL)
  })

  /**
   * Each licence's own sentence: what the face is for, and its licence.
   *
   * **A blind spot: `styles/fonts/OFL.txt` is byte for byte Zen Kaku Gothic
   * New's `LICENSE`**, so the last two sections bundling each other's file
   * draw the same page, and nothing here can tell them apart.
   */
  it.each([
    [
      'Shippori Mincho B1',
      `The face titles are set in, ${OFL}`,
      'Shippori Mincho Project Authors',
      SHIPPORI,
    ],
    [
      'Zen Kaku Gothic New',
      `The face the interface is set in, ${OFL}`,
      'The Zen Project Authors',
      ZEN,
    ],
    [
      'taiyaku marks',
      `Seven characters cut from Zen Kaku Gothic New, ${OFL}`,
      'The Zen Project Authors',
      MARKS,
    ],
  ])(
    'says what %s is for and that it is OFL-1.1, above its own licence’s full text',
    async (name, sentence, authors, file) => {
      const { terms, text } = parts(section(await settle(), name))
      expect(terms).toBe(sentence)
      expect(text).toContain(authors)
      expect(text).toBe(file)
    },
  )

  /**
   * **The contents, between the header and the first licence**: a link per
   * licence, in the order the licences are drawn, each naming the work and
   * its licence and leading by its fragment to that section's own heading,
   * which a fragment written once into a link or an address always names.
   */
  it('lists every licence under the header, in the order they are drawn, each linking to its section’s heading', async () => {
    const host = await settle()
    const navs = host.querySelectorAll('nav')
    expect(navs).toHaveLength(1)
    const nav = navs[0] as HTMLElement
    expect(nav.getAttribute('aria-label')).toBe('Contents')
    const header = host.querySelector('.page-header')
    const sections = [...host.querySelectorAll('.settings-section')]
    expect([
      header?.compareDocumentPosition(nav),
      sections[0]?.compareDocumentPosition(nav),
    ]).toEqual([
      Node.DOCUMENT_POSITION_FOLLOWING,
      Node.DOCUMENT_POSITION_PRECEDING,
    ])
    const links = [...nav.querySelectorAll('a')]
    expect(
      links.map((link) => [link.textContent, link.getAttribute('href')]),
    ).toEqual([
      ['taiyaku (GPL-3.0-or-later)', '#taiyaku'],
      ['Shippori Mincho B1 (OFL-1.1)', '#shippori-mincho-b1'],
      ['Zen Kaku Gothic New (OFL-1.1)', '#zen-kaku-gothic-new'],
      ['taiyaku marks (OFL-1.1)', '#taiyaku-marks'],
    ])
    expect(
      links.map((link, i) => {
        const id = link.getAttribute('href')?.slice(1) ?? ''
        const named = document.querySelectorAll(`[id="${id}"]`)
        return [
          named.length,
          named[0] === sections[i]?.querySelector('h2'),
          named[0]?.textContent,
        ]
      }),
    ).toEqual([
      [1, true, 'taiyaku'],
      [1, true, 'Shippori Mincho B1'],
      [1, true, 'Zen Kaku Gothic New'],
      [1, true, 'taiyaku marks'],
    ])
  })

  it('draws no contents until the texts have drawn the licences it leads to', () => {
    const host = mount(
      <MemoryRouter initialEntries={['/about/licence']}>
        <Licence />
      </MemoryRouter>,
    )
    expect(host.querySelector('h1')?.textContent).toBe('Licence')
    expect(host.querySelector('.settings-section')).toBeNull()
    expect(host.querySelector('nav')).toBeNull()
  })

  /**
   * **A load of the page with a licence's fragment lands on that licence**
   * once the texts have drawn it: before then the browser finds nothing the
   * fragment names, so the page goes there itself. Focus goes to the
   * licence's heading, as following its link from the contents puts it.
   */
  it.each([
    ['#taiyaku', 'taiyaku'],
    ['#shippori-mincho-b1', 'Shippori Mincho B1'],
    ['#zen-kaku-gothic-new', 'Zen Kaku Gothic New'],
    ['#taiyaku-marks', 'taiyaku marks'],
  ])(
    'loaded with %s, puts focus on the heading of %s',
    async (fragment, name) => {
      const host = await settle(Licence, `/about/licence${fragment}`)
      expect(focused()).toBe(name)
      expect(
        document.activeElement === section(host, name).querySelector('h2'),
      ).toBe(true)
    },
  )

  it('loaded with no fragment, leaves focus where it was', async () => {
    await settle()
    expect(focused()).toBe('body')
  })

  /**
   * **Back and Forward put focus where the address they arrive at says**,
   * as a load does: on the licence a fragment names, and nowhere new for the
   * page's own address. A change of location the history did not make, a
   * push, moves nothing. Where the view is put is the browser project's to
   * see (`Shell.browser.test.tsx`).
   */
  it.each([
    [
      'Back to another licence’s fragment, puts focus on its heading',
      [
        '/about/licence#shippori-mincho-b1',
        '/about/licence#zen-kaku-gothic-new',
      ],
      -1,
      'Shippori Mincho B1',
    ],
    [
      'Back to the page’s own address, leaves focus where it was',
      ['/about/licence', '/about/licence#zen-kaku-gothic-new'],
      -1,
      'Zen Kaku Gothic New',
    ],
    [
      'a push of a licence’s fragment, leaves focus where it was',
      ['/about/licence'],
      '#taiyaku',
      'body',
    ],
  ] as const)('%s', async (_, entries, to, name) => {
    const { go } = await atLast(entries)
    await go(to)
    expect(focused()).toBe(name)
  })

  /**
   * **A move through the history that leaves the router's location as it
   * was lands as well.** Between two entries of one address, two
   * `#taiyaku` entries with another between them, the router hands back the
   * location it already had, and only the browser's `popstate` says the
   * history moved. The `MemoryRouter` here hears no `popstate`, so the event
   * is that move with the location unchanged. Where the view is put is the
   * browser project's to see (`Shell.browser.test.tsx`).
   */
  it('a popstate that leaves the router’s location as it was, puts focus back on the heading its fragment names', async () => {
    const { host } = await atLast(['/about/licence#taiyaku'])
    expect(focused()).toBe('taiyaku')
    act(() => {
      host.querySelector<HTMLElement>('.back a')?.focus()
    })
    expect(focused()).toBe('‹ About')
    await act(async () => {
      window.dispatchEvent(new PopStateEvent('popstate'))
    })
    expect(focused()).toBe('taiyaku')
  })

  /**
   * **Each licence has two ways back to the top**: one on its heading's
   * line, where a contents link lands, outside the heading, so the heading
   * keeps its words and its fragment; and one as its box's last row, for
   * the end of a long text. Each leads to the page's own address with no
   * fragment, so the address it leaves names no licence to land on again.
   * Each arrow is drawn for the eye and hidden from a screen reader, which
   * reads the words.
   */
  it('puts two Back to top links in each licence, on its heading’s line and as its box’s last row, to the page’s own address with no fragment', async () => {
    const host = await settle(Licence, '/about/licence#zen-kaku-gothic-new')
    const sections = [...host.querySelectorAll('.settings-section')]
    const parts = (link: Element | undefined) =>
      [...(link?.childNodes ?? [])].map((part) => [
        part.textContent,
        part instanceof Element ? part.getAttribute('aria-hidden') : null,
      ])
    expect(
      sections.map((each) => {
        const heading = each.querySelector('h2')
        const head = topsIn(
          each.querySelector(':scope > .settings-section-head'),
        )
        const last = each.querySelector(
          ':scope > .settings-box',
        )?.lastElementChild
        const foot = topsIn(last)
        return {
          heading: [heading?.textContent, heading?.id],
          links: topsIn(each).length,
          inHeading: topsIn(heading).length,
          head: head.length,
          foot: [foot.length, last?.querySelectorAll('a').length],
          hrefs: [...head, ...foot].map((link) => link.getAttribute('href')),
          parts: [...head, ...foot].map(parts),
        }
      }),
    ).toEqual(
      [
        ['taiyaku', 'taiyaku'],
        ['Shippori Mincho B1', 'shippori-mincho-b1'],
        ['Zen Kaku Gothic New', 'zen-kaku-gothic-new'],
        ['taiyaku marks', 'taiyaku-marks'],
      ].map((heading) => ({
        heading,
        links: 2,
        inHeading: 0,
        head: 1,
        foot: [1, 1],
        hrefs: ['/about/licence', '/about/licence'],
        parts: [0, 1].map(() => [
          ['↑ ', 'true'],
          ['Back to top', null],
        ]),
      })),
    )
  })

  /**
   * **Back to top puts focus on the page's title**, which takes it without
   * being a stop for Tab, and leaves the address with no fragment. Where
   * the view is put is the browser project's to see.
   */
  it.each(['heading', 'foot'] as const)(
    'Back to top at the %s puts focus on the page’s title and takes the fragment off the address',
    async (place) => {
      const { host, address } = await withAddress(
        '/about/licence#taiyaku-marks',
      )
      expect(focused()).toBe('taiyaku marks')
      act(() => {
        toTop(host, 'taiyaku marks', place).click()
      })
      const title = host.querySelector('h1')
      expect({
        focus: document.activeElement === title,
        tabIndex: title?.getAttribute('tabindex'),
        address: address(),
      }).toEqual({ focus: true, tabIndex: '-1', address: '/about/licence' })
    },
  )

  /**
   * **A click with a modifier key is left to the browser**: the page runs
   * nothing of its own for it, so a browser that opens such a click in a
   * new tab or window does so. The listener stands in for the browser,
   * which acts on the click once the page has had it: it
   * listens where the click ends, after React's own listener, so the page
   * sees the click as a browser would hand it over, not prevented.
   */
  it.each(['ctrlKey', 'metaKey', 'shiftKey', 'altKey'] as const)(
    'Back to top clicked with %s held moves neither focus nor the address',
    async (key) => {
      const { host, address } = await withAddress(
        '/about/licence#taiyaku-marks',
      )
      const elsewhere = (event: Event) => event.preventDefault()
      document.addEventListener('click', elsewhere)
      try {
        act(() => {
          toTop(host, 'taiyaku marks', 'foot').dispatchEvent(
            new MouseEvent('click', {
              bubbles: true,
              cancelable: true,
              [key]: true,
            }),
          )
        })
      } finally {
        document.removeEventListener('click', elsewhere)
      }
      expect({ focus: focused(), address: address() }).toEqual({
        focus: 'taiyaku marks',
        address: '/about/licence#taiyaku-marks',
      })
    },
  )

  it('leads up to About the way the settings area’s pages lead up', async () => {
    const host = await settle()
    const up = host.querySelectorAll('.back a')
    expect(up).toHaveLength(1)
    expect(up[0]?.textContent).toBe('‹ About')
    expect(up[0]?.getAttribute('href')).toBe('/about')
  })

  it('says a chunk of texts that does not arrive in a fixed sentence, never in the browser’s words or by the chunk’s address, and draws no licence', async () => {
    // What Chromium rejects a dynamic import with when its chunk does not
    // arrive.
    const chunk = 'https://taiyaku.example/assets/licenceTexts-C2b8Kd1x.js'
    const browsers = new TypeError(
      `Failed to fetch dynamically imported module: ${chunk}`,
    )
    // A mock factory's own throw reaches the importer wrapped in vitest's
    // error, so the mock is instead a module with a `then`, which `import()`
    // adopts as it adopts any thenable: the import rejects with exactly
    // what `then` rejects with. It is given its `then` once vitest has made
    // the mock, since vitest waits on a factory's answer that has one.
    const failing: {
      then?: (landed: unknown, refused: (err: unknown) => void) => void
    } = {}
    vi.resetModules()
    vi.doMock('./licenceTexts', () => failing)
    try {
      await import('./licenceTexts')
      // biome-ignore lint/suspicious/noThenProperty: a thenable, as said above
      failing.then = (_landed, refused) => refused(browsers)
      // The case's premise: an import of the chunk meets the browser's error.
      expect(
        await import('./licenceTexts').then(
          () => 'loaded',
          (err: unknown) => err,
        ),
      ).toBe(browsers)
      const { Licence: Refused } = await import('./Licence')
      const host = await settle(Refused)
      expect(
        [...host.querySelectorAll('[role="alert"]')].map((p) => p.textContent),
      ).toEqual([
        'The licence texts were not loaded; reload the page to try again.',
      ])
      // Nowhere on the page, which the hidden regions are part of.
      expect(host.textContent).not.toContain(chunk)
      expect(host.textContent).not.toContain('Failed to fetch')
      expect(host.querySelector('.settings-section')).toBeNull()
      expect(host.querySelector('nav')).toBeNull()
      expect(host.querySelector('h1')?.textContent).toBe('Licence')
    } finally {
      vi.doUnmock('./licenceTexts')
    }
  })
})
