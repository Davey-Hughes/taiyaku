import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as aboutApi from './about'
import type { Me } from './api'
import * as curation from './curation'
import { GateContext } from './gate'
import * as library from './library'
import { Shell } from './Shell'
import { ABOUT, DIRTY_BUILD, UNKNOWN_BUILD } from './testing/about'
import { deferred, flush } from './testing/async'
import { mount } from './testing/mount'
import { fakeStorage } from './testing/storage'

/**
 * The label as the shell draws it, at the sidebar's foot above More. Mounted
 * through `Shell` rather than alone, so a label that is not in the foot, or
 * not in the shell at all, fails here as it would in the app.
 */

// `shortCommit` stays real: it is what the label's text is checked against.
vi.mock('./about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./about')>()),
  about: vi.fn(),
}))

// The shell's own two reads, answered so neither leaves the test.
vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
}))
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))

const who = (admin: boolean): Me => ({
  user_id: 1,
  admin,
  can_download: false,
  can_stream: false,
  can_stamp: false,
})

/** The shell at `path`, on a route that matches every path, once its reads have landed. */
async function shellAt(path: string, admin = false): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[path]}>
        <GateContext.Provider value={{ who: who(admin), signOut: () => {} }}>
          <Routes>
            <Route
              path="*"
              element={
                <Shell>
                  <p>x</p>
                </Shell>
              }
            />
          </Routes>
        </GateContext.Provider>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/** The label's line, which is drawn whether or not it holds a link. */
function labelIn(host: HTMLElement): HTMLElement {
  const label = host.querySelector<HTMLElement>(
    '.sidebar-foot > .version-label',
  )
  if (!label) throw new Error('no label line at the sidebar’s foot')
  return label
}

function linkIn(host: HTMLElement): HTMLAnchorElement {
  const link = labelIn(host).querySelector('a')
  if (!link) throw new Error('the label draws no link')
  return link
}

beforeEach(() => {
  vi.clearAllMocks()
  // The shell persists the browse preferences, and jsdom has no storage.
  vi.stubGlobal('localStorage', fakeStorage())
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Manga', scanned: true }],
    restricted: false,
  })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the version label', () => {
  it('draws nothing until the answer arrives, then a link to About named by the short commit', async () => {
    const answer = deferred<aboutApi.About>()
    vi.mocked(aboutApi.about).mockReturnValue(answer.promise)
    const host = await shellAt('/')
    expect(labelIn(host).childNodes).toHaveLength(0)

    await flush(() => {
      answer.resolve(ABOUT)
    })
    const link = linkIn(host)
    expect(link.getAttribute('href')).toBe('/about')
    expect(link.textContent).toBe('5ee5159')
    expect(link.getAttribute('aria-label')).toBe('About, build 5ee5159')
    expect(vi.mocked(aboutApi.about)).toHaveBeenCalledTimes(1)
  })

  it('reads "unknown build" when the build carries no commit, and still leads to About', async () => {
    vi.mocked(aboutApi.about).mockResolvedValue(UNKNOWN_BUILD)
    const host = await shellAt('/')
    const link = linkIn(host)
    expect(link.getAttribute('href')).toBe('/about')
    expect(link.textContent).toBe('unknown build')
    expect(link.getAttribute('aria-label')).toBe('About, unknown build')
  })

  it('marks a build whose tracked files had changed with a star, and says so in its name', async () => {
    vi.mocked(aboutApi.about).mockResolvedValue(DIRTY_BUILD)
    const host = await shellAt('/')
    const link = linkIn(host)
    expect(link.getAttribute('href')).toBe('/about')
    expect(link.textContent).toBe('5ee5159*')
    expect(link.getAttribute('aria-label')).toBe(
      'About, build 5ee5159, with uncommitted changes',
    )
  })

  // The route answers `dirty` only beside a commit; a label that read it
  // first would draw a star with no commit before it.
  it('reads "unknown build", with no star and no words, for an answer marked changed with no commit', async () => {
    vi.mocked(aboutApi.about).mockResolvedValue({
      ...UNKNOWN_BUILD,
      dirty: true,
    })
    const host = await shellAt('/')
    const link = linkIn(host)
    expect(link.textContent).toBe('unknown build')
    expect(link.getAttribute('aria-label')).toBe('About, unknown build')
  })

  it('draws nothing when the answer fails, and raises no alert of its own', async () => {
    vi.mocked(aboutApi.about).mockRejectedValue(
      new Error('/api/about: 503 down'),
    )
    const host = await shellAt('/')
    expect(labelIn(host).childNodes).toHaveLength(0)
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it.each([
    ['/about', 'page'],
    ['/about/licence', 'page'],
    ['/', null],
    ['/account', null],
    ['/config/libraries', null],
    ['/aboutness', null],
  ])('is current on %s: %s', async (path, current) => {
    const host = await shellAt(path)
    expect(linkIn(host).getAttribute('aria-current')).toBe(current)
  })

  it.each([
    ['/about', true],
    ['/about', false],
    ['/about/licence', true],
    ['/about/licence', false],
  ])(
    'leaves More unmarked on %s (admin: %s), which is not behind it',
    async (path, admin) => {
      const host = await shellAt(path, admin)
      const more = host.querySelector('.sidebar-foot .account-button')
      if (!more) throw new Error('no More at the sidebar’s foot')
      expect(more.hasAttribute('data-here')).toBe(false)
      expect(linkIn(host).getAttribute('aria-current')).toBe('page')
    },
  )
})
