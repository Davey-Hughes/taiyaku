import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { act } from 'react'
import { MemoryRouter, type NavigateFunction, useNavigate } from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import { Licence } from './Licence'
import { messageOf } from './message'
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

const OFL = 'licensed under the SIL Open Font License, version 1.1 (OFL-1.1).'

/** The card whose own heading is `name`. */
function section(host: HTMLElement, name: string): HTMLElement {
  const heading = [...host.querySelectorAll('h2')].find(
    (h2) => h2.textContent === name,
  )
  const card = heading?.closest('section')
  if (!card || card === host.querySelector('section[aria-label="Licence"]')) {
    throw new Error(`no section headed "${name}"`)
  }
  return card
}

/** What `card` says its work is licensed under, and that licence's text. */
function parts(card: HTMLElement) {
  const texts = card.querySelectorAll('pre')
  if (texts.length !== 1) throw new Error(`${texts.length} texts, not one`)
  return {
    terms: card.querySelector('p')?.textContent ?? '',
    text: texts[0]?.textContent ?? '',
  }
}

describe('Licence', () => {
  it('draws taiyaku’s licence first and then each font’s', async () => {
    const host = await settle()
    expect(
      [...host.querySelectorAll('.about-card h2')].map((h2) => h2.textContent),
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
   * Each card's own sentence: what the face is for, and its licence.
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
   * **The contents, between the title and the first card**: a link per
   * card, in the cards' order, each naming the card's work and its licence
   * and leading by its fragment to that card's own heading, which a
   * fragment written once into a link or an address always names.
   */
  it('lists every card under the title, in the cards’ order, each linking to its card’s heading', async () => {
    const host = await settle()
    const navs = host.querySelectorAll('nav')
    expect(navs).toHaveLength(1)
    const nav = navs[0] as HTMLElement
    expect(nav.getAttribute('aria-label')).toBe('Contents')
    const title = host.querySelector('h1')
    const cards = [...host.querySelectorAll('.about-card')]
    expect([
      title?.compareDocumentPosition(nav),
      cards[0]?.compareDocumentPosition(nav),
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
          named[0] === cards[i]?.querySelector('h2'),
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

  it('draws no contents until the texts have drawn the cards it leads to', () => {
    const host = mount(
      <MemoryRouter initialEntries={['/about/licence']}>
        <Licence />
      </MemoryRouter>,
    )
    expect(host.querySelector('h1')?.textContent).toBe('Licence')
    expect(host.querySelector('.about-card')).toBeNull()
    expect(host.querySelector('nav')).toBeNull()
  })

  /**
   * **A load of the page with a card's fragment lands on that card** once
   * the texts have drawn it: before then the browser finds nothing the
   * fragment names, so the page goes there itself. Focus goes to the card's
   * heading, as following its link from the contents puts it.
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
   * as a load does: on the card a fragment names, and nowhere new for the
   * page's own address. A change of location the history did not make, a
   * push, moves nothing. Where the view is put is the browser project's to
   * see (`Shell.browser.test.tsx`).
   */
  it.each([
    [
      'Back to another card’s fragment, puts focus on its heading',
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
      'a push of a card’s fragment, leaves focus where it was',
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

  it('leads up to About the way the settings area’s pages lead up', async () => {
    const host = await settle()
    const up = host.querySelectorAll('.back a')
    expect(up).toHaveLength(1)
    expect(up[0]?.textContent).toBe('‹ About')
    expect(up[0]?.getAttribute('href')).toBe('/about')
  })

  it('says a chunk of texts that does not load the way it was refused, and draws no licence', async () => {
    vi.resetModules()
    vi.doMock('./licenceTexts', () => {
      throw new Error('offline')
    })
    try {
      // The rejection the page's own `import()` meets, through the same mock.
      const refused = await import('./licenceTexts').then(
        () => 'loaded',
        messageOf,
      )
      expect(refused).not.toBe('loaded')
      const { Licence: Refused } = await import('./Licence')
      const host = await settle(Refused)
      const alerts = host.querySelectorAll('[role="alert"]')
      expect(alerts).toHaveLength(1)
      expect(alerts[0]?.textContent).toBe(refused)
      expect(host.querySelector('.about-card')).toBeNull()
      expect(host.querySelector('nav')).toBeNull()
      expect(host.querySelector('h1')?.textContent).toBe('Licence')
    } finally {
      vi.doUnmock('./licenceTexts')
    }
  })
})
