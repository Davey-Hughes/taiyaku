import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { About } from '../src/About'
import * as aboutApi from '../src/about'
import { Licence } from '../src/Licence'
import { ABOUT, DIRTY_BUILD, UNKNOWN_BUILD } from './support/about'
import { clipboardContext } from './support/clipboard'
import { focused } from './support/focused'
import { licenceTextsLanded } from './support/licence'
import { deferred, flush } from './support/async'
import { mount, rerender } from './support/mount'
import { cellValue } from './support/table'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

// `shortCommit` stays real: the commit link's text is checked against it.
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))

async function settle(): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter>
        <About />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/** The value beside the term `term` in the build's facts. */
function fact(host: HTMLElement, term: string): HTMLElement {
  const dt = [...host.querySelectorAll('.about-facts dt')].find(
    (el) => el.textContent === term,
  )
  const dd = dt?.nextElementSibling
  if (!(dd instanceof HTMLElement) || dd.tagName !== 'DD') {
    throw new Error(`no fact named "${term}"`)
  }
  return dd
}

/** The one link inside `el`. */
function linkIn(el: HTMLElement): HTMLAnchorElement {
  const links = el.querySelectorAll('a')
  if (links.length !== 1) throw new Error(`${links.length} links, not one`)
  return links[0] as HTMLAnchorElement
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
})

describe('About', () => {
  it('links the short commit to the repository’s page for the full hash', async () => {
    const host = await settle()
    const row = fact(host, 'Commit')
    const commit = linkIn(row)
    expect(commit.textContent).toBe('5ee5159')
    expect(commit.getAttribute('href')).toBe(
      'https://github.com/Davey-Hughes/taiyaku/commit/5ee51597e6269deff221b5954e72296b697d71b2',
    )
    expect(row.textContent).toBe('5ee5159')
  })

  it('follows the commit’s link with " with uncommitted changes" when the build’s tracked files had changed', async () => {
    vi.mocked(aboutApi.about).mockResolvedValue(DIRTY_BUILD)
    const host = await settle()
    const row = fact(host, 'Commit')
    const commit = linkIn(row)
    expect(commit.textContent).toBe('5ee5159')
    expect(commit.getAttribute('href')).toBe(
      'https://github.com/Davey-Hughes/taiyaku/commit/5ee51597e6269deff221b5954e72296b697d71b2',
    )
    expect(row.textContent).toBe('5ee5159 with uncommitted changes')
  })

  it('says "unknown build" as plain text, and links no commit, when the build carries none', async () => {
    vi.mocked(aboutApi.about).mockResolvedValue(UNKNOWN_BUILD)
    const host = await settle()
    const commit = fact(host, 'Commit')
    expect(commit.textContent).toBe('unknown build')
    expect(commit.querySelector('a')).toBeNull()
    expect(host.querySelector('a[href*="/commit"]')).toBeNull()
  })

  // The route answers `dirty` only beside a commit; a row that read it
  // apart from the commit would say the words after "unknown build".
  it('says "unknown build" alone for an answer marked changed with no commit', async () => {
    vi.mocked(aboutApi.about).mockResolvedValue({
      ...UNKNOWN_BUILD,
      dirty: true,
    })
    const host = await settle()
    const commit = fact(host, 'Commit')
    expect(commit.textContent).toBe('unknown build')
    expect(commit.querySelector('a')).toBeNull()
  })

  it('links Source to the repository', async () => {
    const host = await settle()
    expect(linkIn(fact(host, 'Source')).getAttribute('href')).toBe(
      'https://github.com/Davey-Hughes/taiyaku',
    )
  })

  /**
   * **The build's facts are the commit, the source and the catalog**, and
   * no issues link of their own: the repository's page, which Source links
   * to, leads to its issues. A build with no commit keeps its Commit row,
   * which says "unknown build".
   */
  it.each([
    ['a commit', ABOUT],
    ['no commit', UNKNOWN_BUILD],
  ])(
    'names the build’s facts Commit, Source and OPDS catalog, with %s',
    async (_, build) => {
      vi.mocked(aboutApi.about).mockResolvedValue(build)
      const host = await settle()
      const facts = host.querySelector('.about-facts')
      if (!facts) throw new Error('no facts')
      expect(
        [...facts.querySelectorAll('dt')].map((dt) => [
          dt.textContent,
          dt.nextElementSibling?.tagName,
        ]),
      ).toEqual([
        ['Commit', 'DD'],
        ['Source', 'DD'],
        ['OPDS catalog', 'DD'],
      ])
      expect(
        [...facts.querySelectorAll('a')].map((a) => a.getAttribute('href')),
      ).toEqual([
        ...(build.commit === null
          ? []
          : [`https://github.com/Davey-Hughes/taiyaku/commit/${build.commit}`]),
        'https://github.com/Davey-Hughes/taiyaku',
      ])
    },
  )

  /**
   * **The Licence section is drawn once the route has settled, last, either
   * way.** It uses nothing the route answers, so a failure keeps it, beside
   * the failure said; and it waits for the answer so that it never stands
   * alone at the top of the page to be pushed down by the sections an
   * answer brings. Each section is headed at h2, under the page's h1. Its
   * two sentences, each a row of its own, link each licence they name to
   * that licence on the licence page: taiyaku's to taiyaku's, and the
   * fonts' to the first of the three fonts, which follow one another there.
   */
  it.each([
    [
      'answers',
      () => vi.mocked(aboutApi.about).mockResolvedValue(ABOUT),
      ['This build', 'Formats', 'Licence'],
      [],
    ],
    [
      'fails',
      () =>
        vi
          .mocked(aboutApi.about)
          .mockRejectedValue(new Error('/api/about: 503 down')),
      ['Licence'],
      ['This build’s details were not loaded: down'],
    ],
  ])(
    'links each licence it names to that licence on the licence page, in the last section, when the route %s',
    async (_, answer, sections, alerts) => {
      answer()
      const host = await settle()
      const drawn = [...host.querySelectorAll('.settings-section')]
      expect(
        drawn.map((el) => el.querySelector('.settings-section-title')?.tagName),
      ).toEqual(sections.map(() => 'H2'))
      expect(drawn.map((el) => el.querySelector('h2')?.textContent)).toEqual(
        sections,
      )
      expect(
        [...host.querySelectorAll('[role="alert"]')].map(
          (alert) => alert.textContent,
        ),
      ).toEqual(alerts)
      const section = drawn.at(-1)
      if (!(section instanceof HTMLElement)) {
        throw new Error('no Licence section')
      }
      const box = section.querySelector<HTMLElement>('.settings-box')
      if (!box) throw new Error('no box in the Licence section')
      expect(
        [...box.children].map((sentence) => ({
          tag: sentence.tagName,
          text: sentence.textContent,
          links: [...sentence.querySelectorAll('a')].map((link) => [
            link.textContent,
            link.getAttribute('href'),
          ]),
        })),
      ).toEqual([
        {
          tag: 'P',
          text: 'taiyaku is GPL-3.0-or-later.',
          links: [['GPL-3.0-or-later', '/about/licence#taiyaku']],
        },
        {
          tag: 'P',
          text: 'Its fonts are OFL-1.1.',
          links: [['OFL-1.1', '/about/licence#shippori-mincho-b1']],
        },
      ])
    },
  )

  /**
   * **A failed request is said as a sentence**: what was not drawn, then
   * why, in the server's own words where its answer carries some, by its
   * status where the answer is a page of markup, and as thrown where nothing
   * answered. Never as an address and a status with a body.
   */
  it.each([
    [
      'a sentence',
      '/api/about: 503 the server is starting',
      'This build’s details were not loaded: the server is starting',
    ],
    [
      'markup',
      '/api/about: 502 <html><body><h1>502 Bad Gateway</h1></body></html>',
      'This build’s details were not loaded: the server answered 502',
    ],
    [
      'nothing',
      'Failed to fetch',
      'This build’s details were not loaded: Failed to fetch',
    ],
  ])(
    'says a request answered with %s as what was not loaded and why',
    async (_, thrown, said) => {
      vi.mocked(aboutApi.about).mockRejectedValue(new Error(thrown))
      const host = await settle()
      expect(
        [...host.querySelectorAll('[role="alert"]')].map(
          (alert) => alert.textContent,
        ),
      ).toEqual([said])
    },
  )

  /**
   * **Each licence link lands on its licence**, through the licence page's
   * own landing on a fragment: followed under a router holding both pages,
   * the licence page puts focus on the heading the link's fragment names.
   * So a fragment the licence page does not draw fails here.
   */
  it.each([
    ['GPL-3.0-or-later', 'taiyaku'],
    ['OFL-1.1', 'Shippori Mincho B1'],
  ])(
    'followed, %s puts focus on the licence page’s heading %s',
    async (words, heading) => {
      let host: HTMLElement | undefined
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={['/about']}>
            <Routes>
              <Route path="/about" element={<About />} />
              <Route path="/about/licence" element={<Licence />} />
            </Routes>
          </MemoryRouter>,
        )
        await Promise.resolve()
        await Promise.resolve()
      })
      const link = [...(host?.querySelectorAll('a') ?? [])].find(
        (each) => each.textContent === words,
      )
      if (!link) throw new Error(`no link ${words}`)
      act(() => {
        link.click()
      })
      await licenceTextsLanded()
      expect(focused()).toBe(heading)
    },
  )

  /** Each part of the page, in order: the header by its class, each section by its title, and the wait line by its words. */
  const parts = (host: HTMLElement) =>
    [...host.querySelectorAll('section.about > *')].map((part) =>
      part.matches('[role="status"]')
        ? part.textContent
        : (part.querySelector('.settings-section-title')?.textContent ??
          part.className),
    )

  it('heads itself at once, and draws Loading… in its body’s place from 200 ms until the route answers', async () => {
    holdTheClock()
    const answer = deferred<aboutApi.About>()
    vi.mocked(aboutApi.about).mockReturnValue(answer.promise)
    const host = await settle()
    expect(parts(host)).toEqual(['page-header'])
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(parts(host)).toEqual(['page-header'])
    pass(1)
    expect(parts(host)).toEqual(['page-header', 'Loading…'])
    const drawn = watchDrawn(() => JSON.stringify(parts(host)))
    await flush(() => {
      answer.resolve(ABOUT)
    })
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify(['page-header', 'Loading…']),
      JSON.stringify(['page-header', 'This build', 'Formats', 'Licence']),
    ])
  })

  it('draws the answer kept at once on a return, reads it again, and draws a new answer in one step with no wait', async () => {
    holdTheClock()
    await settle()
    rerender(<div />)
    const again = deferred<aboutApi.About>()
    vi.mocked(aboutApi.about).mockClear()
    vi.mocked(aboutApi.about).mockReturnValue(again.promise)

    rerender(
      <MemoryRouter>
        <About />
      </MemoryRouter>,
    )
    const host = document.querySelector('section.about')
      ?.parentElement as HTMLElement
    const sections = ['page-header', 'This build', 'Formats', 'Licence']
    expect(parts(host)).toEqual(sections)
    expect(fact(host, 'Commit').textContent).toBe('5ee5159')
    expect(aboutApi.about).toHaveBeenCalledTimes(1)
    pass(WAIT_DRAWN_AFTER_MS)
    const drawn = watchDrawn(() =>
      JSON.stringify([parts(host), fact(host, 'Commit').textContent]),
    )
    await flush(() => {
      again.resolve(DIRTY_BUILD)
    })
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify([sections, '5ee5159']),
      JSON.stringify([sections, '5ee5159 with uncommitted changes']),
    ])
  })

  it('draws nothing under its header while the route has not answered', async () => {
    vi.mocked(aboutApi.about).mockReturnValue(new Promise(() => {}))
    const host = await settle()
    const page = host.querySelector('section.about')
    expect([...(page?.children ?? [])].map((el) => el.className)).toEqual([
      'page-header',
    ])
    expect(
      [...(page?.querySelectorAll('h1, h2, h3, h4, h5, h6') ?? [])].map(
        (heading) => heading.outerHTML,
      ),
    ).toEqual(['<h1 class="page-title">About</h1>'])
  })

  it('shows the catalog’s address as this page’s origin before the path the route answers', async () => {
    // Another version than today's, so an address the page wrote for itself
    // cannot pass for the one it was handed.
    vi.mocked(aboutApi.about).mockResolvedValue({
      ...ABOUT,
      catalog: '/opds/v1.3/catalog',
    })
    const host = await settle()
    const address = fact(host, 'OPDS catalog')
    expect(address.querySelector('code')?.textContent).toBe(
      `${location.origin}/opds/v1.3/catalog`,
    )
  })

  it('offers the catalog’s address to copy where the browser can write it', async () => {
    clipboardContext(true, { writeText: vi.fn() })
    const host = await settle()
    const buttons = fact(host, 'OPDS catalog').querySelectorAll('button')
    expect([...buttons].map((button) => button.textContent)).toEqual([
      'Copy address',
    ])
  })

  /**
   * **Each row is a format's name, the sentence under it and its version**,
   * read row by row, so a sentence drawn under another format's name fails
   * where a whole-table read would not. The name and the sentence are the
   * name cell's only parts, in that order.
   */
  it('lists every format in the order the route sends, each with its sentence, a null version reading "unversioned"', async () => {
    const host = await settle()
    const rows = [...host.querySelectorAll('.about table tbody tr')].map(
      (row) => {
        const cells = [...row.querySelectorAll('td')]
        const [format, version] = cells
        if (cells.length !== 2 || !format || !version) {
          throw new Error(`${cells.length} cells, not two`)
        }
        return {
          parts: [...format.childNodes].map((part) => part.textContent),
          name: format.querySelector('.about-format-name')?.textContent,
          sentence: format.querySelector('.about-format-description')
            ?.textContent,
          version: cellValue(version),
        }
      },
    )
    const row = (name: string, sentence: string, version: string) => ({
      parts: [name, sentence],
      name,
      sentence,
      version,
    })
    expect(rows).toEqual([
      row(
        'taiyaku.json',
        'Sits beside a work’s archives and records how its editions correspond to each other.',
        '1',
      ),
      row(
        '<stem>.library.json',
        'A work’s metadata, kept in a file so fixes made by hand outlive the database.',
        '1',
      ),
      row(
        '<stem>.progress.json',
        'One reader’s place in a work, edition by edition.',
        '1',
      ),
      row(
        '<stem>.truth.json',
        'Boxes a person drew around the text on pages, to score text detection against.',
        '1',
      ),
      row(
        '<stem>.regions.json',
        'Where the text is on each page, as the regions command found it.',
        '1',
      ),
      row('<stem>.notes.json', 'Notes one reader left on a work’s pages.', '1'),
      row(
        'Database schema',
        'The layout of the SQLite database taiyaku keeps its library in.',
        '17',
      ),
      row(
        'OPDS, with the Page Streaming Extension',
        'The catalog that reading apps browse and stream pages from.',
        '1.2',
      ),
      row(
        'HTTP API',
        'The /api routes this web app uses to talk to the server.',
        'unversioned',
      ),
      row(
        'wasm module',
        'taiyaku’s core built for the browser, which the reader uses to match pages between editions.',
        'unversioned',
      ),
    ])
    expect(
      [...host.querySelectorAll('.about table thead th')].map(
        (th) => th.textContent,
      ),
    ).toEqual(['Format', 'Version'])
  })

  /**
   * The narrow layout hides the headings (`admin.css`, `.config-table`), so
   * a versioned row's cell says "Version" itself, in its `.cell-label`. An
   * unversioned row says "unversioned", which needs no label.
   */
  it('labels a versioned row’s version for the narrow layout, and no unversioned one', async () => {
    const host = await settle()
    const cells = [...host.querySelectorAll('.about table tbody tr')].map(
      (row) => {
        const version = row.querySelectorAll('td')[1]
        return [
          [...(version?.querySelectorAll('.cell-label') ?? [])].map(
            (label) => label.textContent,
          ),
          version?.textContent,
        ]
      },
    )
    const versioned = (v: string) => [['Version '], `Version ${v}`]
    const unversioned = [[], 'unversioned']
    expect(cells).toEqual([
      versioned('1'),
      versioned('1'),
      versioned('1'),
      versioned('1'),
      versioned('1'),
      versioned('1'),
      versioned('17'),
      versioned('1.2'),
      unversioned,
      unversioned,
    ])
  })

  it('shows a failed answer the way other pages do, and draws none of the facts', async () => {
    vi.mocked(aboutApi.about).mockRejectedValue(
      new Error('/api/about: 503 down'),
    )
    const host = await settle()
    const alerts = host.querySelectorAll('[role="alert"]')
    expect(alerts).toHaveLength(1)
    expect(alerts[0]?.textContent).toBe(
      'This build’s details were not loaded: down',
    )
    expect(host.querySelector('.about-facts')).toBeNull()
    expect(host.querySelector('table')).toBeNull()
  })
})
