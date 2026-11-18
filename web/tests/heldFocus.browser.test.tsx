import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import '../src/index.css'
import { AccountPage } from '../src/AccountPage'
import * as backupApi from '../src/backup'
import { Backup } from '../src/Backup'
import * as config from '../src/config'
import { DirPage } from '../src/DirPage'
import { WAIT_DRAWN_AFTER_MS } from '../src/hooks'
import * as librariesApi from '../src/libraries'
import { LibraryPage } from '../src/LibraryPage'
import { FEED_IDLE_MS, type ScanFeed } from '../src/scanFeed'
import { deferred, settleUntil } from './support/async'
import { expectCallout, liesOver, pressAt, pressOn } from './support/callout'
import { holdOf } from './support/held'
import { counted, dir, idle } from './support/libraries'
import { tapWhereOver } from './support/touch'
import { typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { feedJob, serveFeed } from './support/scanFeed'
import * as usersApi from '../src/users'

/**
 * A control that a wait or a hold takes while it has focus keeps focus, in a
 * real browser: jsdom never moves focus off a disabled control, and Chromium
 * moves it to the page. Each case reaches the control with real keys and
 * presses it with Enter, so what the keyboard does to a held control is
 * what is read. Where focus is is read before how the control is held, so
 * that a control held by `disabled` fails on focus left on the page.
 */

vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
  takeBackup: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
  removeLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
  unlinkCost: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
  revokeUserKey: vi.fn(),
  revokeUserSession: vi.fn(),
}))

const MANGA = {
  id: 1,
  name: 'Manga',
  works: 12,
  books: 0,
  match_by_content: true,
  dirs: [
    dir(1, '/srv/manga/volumes', [{ id: 1, name: 'Manga' }]),
    dir(2, '/srv/manga/chapters', [{ id: 1, name: 'Manga' }]),
  ],
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
  vi.mocked(librariesApi.libraryScanStatus).mockResolvedValue(idle(1))
  vi.mocked(librariesApi.libraryRemovalCost).mockResolvedValue(counted(12))
  vi.mocked(config.removalCost).mockResolvedValue(counted(3))
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

/** Real keys, inside `act` so that React sees what they start. */
const keys = (text: string) =>
  act(async () => {
    await userEvent.keyboard(text)
  })

async function settle(ms = 120) {
  await act(async () => {
    await new Promise((r) => setTimeout(r, ms))
  })
}

/**
 * Gives the test's frame focus with a real click on words that take none,
 * so that the keys pressed next reach the page and not the frame, then puts
 * focus on `el`.
 */
async function focusFirst(on: Element, el: HTMLElement) {
  await act(async () => {
    await userEvent.click(on)
  })
  act(() => el.focus())
  expect(document.activeElement).toBe(el)
}

async function libraryPage(): Promise<HTMLElement> {
  await page.viewport(1280, 800)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/config/libraries/Manga']}>
        <LibraryPage
          library={MANGA}
          libraries={[MANGA]}
          onAdded={() => undefined}
          onChanged={() => undefined}
          onRemoved={() => undefined}
        />
      </MemoryRouter>,
    )
  })
  if (host === undefined) throw new Error('nothing mounted')
  await settle()
  return host
}

const named = (host: HTMLElement, name: string) =>
  host.querySelector(`button[name="${name}"]`) as HTMLButtonElement
const words = (host: HTMLElement) => host.querySelector('.holds') as HTMLElement

describe('a library’s page, held controls in a real browser', () => {
  it('keeps focus on Scan library, pressed with Enter, while its start holds it, as it reads Cancel, and once that scan ends; Enter on it sends nothing while it is held', async () => {
    const feed = serveFeed({ ended: 0, libraries: [] })
    const started = deferred<librariesApi.LibraryScanStarted>()
    vi.mocked(librariesApi.startLibraryScan).mockReturnValue(started.promise)
    const host = await libraryPage()
    const scan = named(host, 'scan-library')
    await focusFirst(words(host), scan)

    await keys('{Enter}')
    expect(librariesApi.startLibraryScan).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(scan)
    expect(holdOf(scan)).toBe('held')
    await keys('{Enter}')
    expect(librariesApi.startLibraryScan).toHaveBeenCalledTimes(1)

    feed.answer({
      ended: 0,
      libraries: [{ id: 1, step: 'resolving' }],
      jobs: [
        feedJob({
          dirs: MANGA.dirs.map(({ id, path }) => ({ id, path })),
          libraries: [{ id: 1, name: 'Manga' }],
        }),
      ],
    })
    await act(async () => {
      started.resolve({
        library_id: 1,
        status_url: '/api/x',
        root_ids: MANGA.dirs.map((one) => one.id),
      })
    })
    await settleUntil('Cancel', () => scan.textContent === 'Cancel', 1000)
    expect(host.querySelector('.scanning')).not.toBeNull()
    expect(document.activeElement).toBe(scan)
    expect(holdOf(scan)).toBe('free')

    // The feed finds the scan ended.
    feed.answer({ ended: 1, libraries: [] })
    await settleUntil(
      'Scan library, free',
      () => scan.textContent === 'Scan library' && holdOf(scan) === 'free',
      config.SCAN_POLL_MS * 3,
    )
    expect(host.querySelector('.scanning')).toBeNull()
    expect(document.activeElement).toBe(scan)
  })

  it('opens nothing for a real mouse press on Remove… while a derive holds the lease, and opens the removal once a poll finds it free (O51)', async () => {
    vi.mocked(librariesApi.libraryScanStatus).mockResolvedValueOnce({
      ...idle(1),
      derive_running: true,
    })
    const host = await libraryPage()
    const remove = named(host, 'remove-library')
    expect(holdOf(remove)).toBe('held')
    await pressOn(remove)
    await settle()
    expect(host.querySelector('.remove-library')).toBeNull()
    expect(librariesApi.libraryRemovalCost).not.toHaveBeenCalled()

    await settleUntil(
      'Remove…, free',
      () => holdOf(remove) === 'free',
      config.SCAN_POLL_MS * 3,
    )
    await pressOn(remove)
    await settle()
    expect(host.querySelector('.remove-library')).not.toBeNull()
  })

  it('keeps focus on Remove…, pressed with Enter, while the removal it opens holds it', async () => {
    const host = await libraryPage()
    const remove = named(host, 'remove-library')
    await focusFirst(words(host), remove)

    await keys('{Enter}')
    await settle()
    expect(host.querySelector('.remove-library .counts')).not.toBeNull()
    expect(document.activeElement).toBe(remove)
    expect(holdOf(remove)).toBe('held')
  })

  it('keeps focus on the removal’s confirm, pressed with Enter, through its drawn wait and a refusal; Enter on it sends nothing while it is held', async () => {
    const sent = deferred<config.RemovalAnswer>()
    vi.mocked(librariesApi.removeLibrary).mockReturnValue(sent.promise)
    const host = await libraryPage()
    await focusFirst(words(host), named(host, 'remove-library'))
    await keys('{Enter}')
    await settle()
    const confirm = host.querySelector(
      '.remove-library .confirm',
    ) as HTMLButtonElement
    act(() => confirm.focus())

    await keys('{Enter}')
    expect(librariesApi.removeLibrary).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(confirm)
    expect([confirm.textContent, holdOf(confirm)]).toEqual([
      'Removing…',
      'held',
    ])
    await keys('{Enter}')
    expect(librariesApi.removeLibrary).toHaveBeenCalledTimes(1)

    await act(async () => {
      sent.reject(
        new Error('/api/library/libraries/1: 500 {"error":"the disk is full"}'),
      )
    })
    await settle()
    expect(
      host.querySelector('.remove-library [role="alert"]')?.textContent,
    ).toBe('the disk is full')
    expect(document.activeElement).toBe(confirm)
    expect([confirm.textContent, holdOf(confirm)]).toEqual([
      'Remove Manga',
      'free',
    ])
  })

  it('keeps focus on the removal’s Cancel, tabbed to inside the wait’s delay, through the drawn wait and a refusal; Enter on it does nothing while it is held', async () => {
    const sent = deferred<config.RemovalAnswer>()
    vi.mocked(librariesApi.removeLibrary).mockReturnValue(sent.promise)
    const host = await libraryPage()
    await focusFirst(words(host), named(host, 'remove-library'))
    await keys('{Enter}')
    await settle()
    const confirm = host.querySelector(
      '.remove-library .confirm',
    ) as HTMLButtonElement
    const cancel = [...host.querySelectorAll('.remove-library button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    act(() => confirm.focus())
    // Enter on the confirm, then Tab to Cancel beside it, before the wait is
    // drawn.
    await keys('{Enter}{Tab}')
    expect(document.activeElement).toBe(cancel)

    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(cancel)
    expect(holdOf(cancel)).toBe('held')
    await keys('{Enter}')
    expect(host.querySelector('.remove-library')).not.toBeNull()

    await act(async () => {
      sent.reject(
        new Error('/api/library/libraries/1: 500 {"error":"the disk is full"}'),
      )
    })
    await settle()
    expect(document.activeElement).toBe(cancel)
    expect(holdOf(cancel)).toBe('free')
  })

  it('keeps focus on the removal’s confirm when a scan the feed finds under the open removal holds it; Enter on it sends nothing then', async () => {
    const feed = serveFeed({ ended: 0, libraries: [] })
    const host = await libraryPage()
    await focusFirst(words(host), named(host, 'remove-library'))
    await keys('{Enter}')
    await settle()
    const confirm = host.querySelector(
      '.remove-library .confirm',
    ) as HTMLButtonElement
    act(() => confirm.focus())
    expect(holdOf(confirm)).toBe('free')

    // The feed's next idle poll finds a scan of one of its directories.
    feed.answer({
      ended: 0,
      libraries: [{ id: 1, step: 'resolving' }],
      jobs: [
        feedJob({
          id: 9,
          dirs: [{ id: 2, path: MANGA.dirs[1].path }],
          libraries: [{ id: 1, name: 'Manga' }],
        }),
      ],
    })
    await settleUntil(
      'the confirm, held',
      () => holdOf(confirm) === 'held',
      FEED_IDLE_MS * 2,
    )
    expect(document.activeElement).toBe(confirm)
    await keys('{Enter}')
    expect(librariesApi.removeLibrary).not.toHaveBeenCalled()
  })
})

/** The first directory's page, with `others` the libraries it can move to. */
const dirPageOf = (others = [{ id: 2, name: 'Light novels' }]) => (
  <MemoryRouter initialEntries={['/config/libraries/Manga/dirs/1']}>
    <DirPage
      library={MANGA}
      root={MANGA.dirs[0]}
      others={others}
      onChanged={() => undefined}
      onRemoved={() => undefined}
      onUnlinked={() => undefined}
      onMoved={() => undefined}
    />
  </MemoryRouter>
)

async function dirPage(): Promise<HTMLElement> {
  await page.viewport(1280, 800)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(dirPageOf())
  })
  if (host === undefined) throw new Error('nothing mounted')
  await settle()
  return host
}

const called = (host: HTMLElement, text: string) =>
  [...host.querySelectorAll<HTMLButtonElement>('.dir-page button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement
const regime = (host: HTMLElement) =>
  host.querySelector('.regime') as HTMLElement
/** The feed's answer while job 7, of Manga, holds the directories named by id. */
const scanningOf = (...ids: number[]): ScanFeed => ({
  ended: 0,
  libraries: [{ id: MANGA.id, step: 'resolving' }],
  jobs: [
    feedJob({
      dirs: MANGA.dirs
        .filter((one) => ids.includes(one.id))
        .map(({ id, path }) => ({ id, path })),
      libraries: [{ id: MANGA.id, name: MANGA.name }],
    }),
  ],
})

describe('a directory’s page, held controls in a real browser', () => {
  it('keeps focus on Move… while another directory’s scan holds it and once that scan ends; Enter on it opens nothing while it is held', async () => {
    // The feed's first answer lands once Move… has focus: a job holds the
    // other directory, for which the server refuses a move. A poll after it
    // finds the job ended.
    const feed = serveFeed(scanningOf(2))
    const answer = feed.hold()
    const host = await dirPage()
    const move = called(host, 'Move…')
    await focusFirst(regime(host), move)
    expect(holdOf(move)).toBe('free')

    await act(async () => {
      answer()
    })
    await settle()
    expect(document.activeElement).toBe(move)
    expect(holdOf(move)).toBe('held')
    await keys('{Enter}')
    expect(host.querySelector('.move-root')).toBeNull()

    feed.answer({ ended: 1, libraries: [] })
    await settleUntil(
      'Move…, free',
      () => holdOf(move) === 'free',
      config.SCAN_POLL_MS * 3,
    )
    expect(document.activeElement).toBe(move)
  })

  it('keeps focus on Scan directory as a scan of its whole library turns it to Cancel, free, through a cancel pressed on it with Enter, and once that scan ends; Enter on it sends nothing while the cancel holds it', async () => {
    // The feed's first answer lands once Scan directory has focus: a job,
    // which this page did not start, holds this directory and its sibling.
    // A poll after the cancel finds the job ended.
    const feed = serveFeed(scanningOf(1, 2))
    const answer = feed.hold()
    const host = await dirPage()
    const scan = called(host, 'Scan directory')
    await focusFirst(regime(host), scan)
    expect(holdOf(scan)).toBe('free')

    await act(async () => {
      answer()
    })
    await settle()
    expect(document.activeElement).toBe(scan)
    expect([scan.textContent, holdOf(scan)]).toEqual(['Cancel', 'free'])

    await keys('{Enter}')
    await settle()
    expect(config.startScan).not.toHaveBeenCalled()
    expect(feed.cancels).toEqual([7])
    expect(document.activeElement).toBe(scan)
    expect([scan.textContent, holdOf(scan)]).toEqual(['Cancel', 'held'])
    await keys('{Enter}')
    expect(feed.cancels).toEqual([7])

    feed.answer({ ended: 1, libraries: [] })
    await settleUntil(
      'Scan directory, free',
      () => scan.textContent === 'Scan directory' && holdOf(scan) === 'free',
      config.SCAN_POLL_MS * 3,
    )
    expect(document.activeElement).toBe(scan)
    expect(config.startScan).not.toHaveBeenCalled()
  })

  it('keeps focus on Remove… while a derive’s hold comes on, and a real mouse press on it opens nothing until a poll finds the lease free (O51)', async () => {
    const status = deferred<librariesApi.LibraryScanStatus>()
    vi.mocked(librariesApi.libraryScanStatus).mockReturnValueOnce(
      status.promise,
    )
    const host = await dirPage()
    const remove = called(host, 'Remove…')
    await focusFirst(regime(host), remove)

    await act(async () => {
      status.resolve({ ...idle(1), derive_running: true })
    })
    await settle()
    expect(document.activeElement).toBe(remove)
    expect(holdOf(remove)).toBe('held')
    await pressOn(remove)
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(config.removalCost).not.toHaveBeenCalled()

    await settleUntil(
      'Remove…, free',
      () => holdOf(remove) === 'free',
      config.SCAN_POLL_MS * 3,
    )
    await pressOn(remove)
    await settle()
    expect(host.querySelector('.remove-root')).not.toBeNull()
  })

  it('hands focus to Remove…, held, when a scan around the directory cancels the removal its confirm had focus in', async () => {
    const status = deferred<librariesApi.LibraryScanStatus>()
    vi.mocked(librariesApi.libraryScanStatus).mockReturnValueOnce(
      status.promise,
    )
    vi.mocked(config.removalCost).mockResolvedValue(counted(3))
    const host = await dirPage()
    const remove = called(host, 'Remove…')
    await focusFirst(regime(host), remove)
    await keys('{Enter}')
    await settle()
    expect(document.activeElement).toBe(remove)
    const confirm = host.querySelector(
      '.remove-root .confirm',
    ) as HTMLButtonElement
    act(() => confirm.focus())

    await act(async () => {
      status.resolve({ ...idle(1), held_removals: [1] })
    })
    await settle()
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(document.activeElement).toBe(remove)
    expect(holdOf(remove)).toBe('held')
  })

  it('keeps focus on Remove from Manga…, pressed with Enter, held while the dialog it opens is open and when another directory’s scan closes that; Enter on it asks nothing more', async () => {
    const both = [
      { id: 1, name: 'Manga' },
      { id: 3, name: 'Kids' },
    ]
    const shared = {
      ...MANGA,
      dirs: [dir(1, '/srv/manga/volumes', both), MANGA.dirs[1]],
    }
    // The feed's first answer lands once the dialog is open: a job holds
    // the other directory, for which the server refuses an unlink.
    const answer = serveFeed(scanningOf(2)).hold()
    vi.mocked(librariesApi.unlinkCost).mockResolvedValue({
      unlinked: {
        loses_sight: [],
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      dry_run: true,
    })
    await page.viewport(1280, 800)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/config/libraries/Manga/dirs/1']}>
          <DirPage
            library={shared}
            root={shared.dirs[0]}
            others={[{ id: 2, name: 'Light novels' }]}
            onChanged={() => undefined}
            onRemoved={() => undefined}
            onUnlinked={() => undefined}
            onMoved={() => undefined}
          />
        </MemoryRouter>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    await settle()
    const unlink = called(host, 'Remove from Manga…')
    await focusFirst(regime(host), unlink)
    await keys('{Enter}')
    await settle()
    expect(document.activeElement).toBe(unlink)
    expect(holdOf(unlink)).toBe('held')
    expect(host.querySelector('.unlink-root .confirm')).not.toBeNull()
    await keys('{Enter}')
    expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(1)

    await act(async () => {
      answer()
    })
    await settle()
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(document.activeElement).toBe(unlink)
    expect(holdOf(unlink)).toBe('held')
    await keys('{Enter}')
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(1)
  })

  it.each([1280, 390])(
    'lays a refusal of Remove from Manga…, pressed with Enter, over that button’s row and not Remove this directory’s, its focus and ring kept beneath it, and takes it away as the hold that refused it ends, so that a real mouse press on the button opens its dialog again, the 409 having its library’s status read again, at %i',
    async (width) => {
      const both = [
        { id: 1, name: 'Manga' },
        { id: 3, name: 'Kids' },
      ]
      const shared = {
        ...MANGA,
        dirs: [dir(1, '/srv/manga/volumes', both), MANGA.dirs[1]],
      }
      const LOCKED =
        'a scan or another library job is running; remove the directory from this library once it has finished'
      // The page's first read of its library's status finds nothing
      // holding. The 409 has it read again, which answers once the refusal
      // is drawn: the lock is held, and the poll that follows finds it free.
      const status = deferred<librariesApi.LibraryScanStatus>()
      vi.mocked(librariesApi.libraryScanStatus)
        .mockResolvedValueOnce(idle(1))
        .mockReturnValueOnce(status.promise)
      vi.mocked(librariesApi.unlinkCost)
        .mockRejectedValueOnce(
          new Error(
            `/api/library/libraries/1/dirs/1?dry_run=true: 409 ${JSON.stringify({ error: LOCKED })}`,
          ),
        )
        .mockResolvedValue({
          unlinked: {
            loses_sight: [],
            library_removed: null,
            grants_removed: 0,
            left_with_no_library: [],
          },
          dry_run: true,
        })
      await page.viewport(width, 800)
      let host: HTMLElement | undefined
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={['/config/libraries/Manga/dirs/1']}>
            <DirPage
              library={shared}
              root={shared.dirs[0]}
              others={[{ id: 2, name: 'Light novels' }]}
              onChanged={() => undefined}
              onRemoved={() => undefined}
              onUnlinked={() => undefined}
              onMoved={() => undefined}
            />
          </MemoryRouter>,
        )
      })
      if (host === undefined) throw new Error('nothing mounted')
      await settle()
      const unlink = called(host, 'Remove from Manga…')
      const pressOn = async (el: Element) => {
        const at = el.getBoundingClientRect()
        await pressAt((at.left + at.right) / 2, (at.top + at.bottom) / 2)
        await settle()
      }

      await focusFirst(regime(host), unlink)
      await keys('{Enter}')
      await settle()
      expect(host.querySelector('.unlink-root')).toBeNull()
      const said = host.querySelector(
        '.callout > [role="alert"]',
      ) as HTMLElement
      expect(said.textContent).toBe(LOCKED)
      // Drawn after Remove this directory's row, and laid over the row above
      // it: Remove from Manga's own, button and all. Remove this directory,
      // which a refusal on the last library names, stays in view.
      const remove = called(host, 'Remove…')
      await expectCallout(said, remove.closest('.settings-row') as Element, {
        above: true,
      })
      expect([liesOver(said, unlink), liesOver(said, remove)]).toEqual([
        true,
        false,
      ])
      expect(document.activeElement).toBe(unlink)
      expect(said.parentElement?.hasAttribute('data-over-focus')).toBe(true)

      await act(async () => {
        status.resolve({ ...idle(1), moves_held: true })
      })
      await settle()
      expect(holdOf(unlink)).toBe('held')
      expect(host.querySelector('.callout')).not.toBeNull()

      await settleUntil(
        'Remove from Manga…, free',
        () => holdOf(unlink) === 'free',
        config.SCAN_POLL_MS * 3,
      )
      expect(host.querySelector('.callout')).toBeNull()
      await pressOn(unlink)
      expect(host.querySelector('.unlink-root .confirm')).not.toBeNull()
      expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(2)
    },
  )
})

/** Manga's page, at `width`, of its first directory, at `path`, which Kids holds too. */
async function sharedDirPage(path: string, width: number) {
  const both = [
    { id: 1, name: 'Manga' },
    { id: 3, name: 'Kids' },
  ]
  const shared = { ...MANGA, dirs: [dir(1, path, both), MANGA.dirs[1]] }
  await page.viewport(width, 800)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/config/libraries/Manga/dirs/1']}>
        <DirPage
          library={shared}
          root={shared.dirs[0]}
          others={[{ id: 2, name: 'Light novels' }]}
          onChanged={() => undefined}
          onRemoved={() => undefined}
          onUnlinked={() => undefined}
          onMoved={() => undefined}
        />
      </MemoryRouter>,
    )
  })
  if (host === undefined) throw new Error('nothing mounted')
  await settle()
  return host
}

/** A real tap where `over` lies over `under`, and what it started settled. */
async function tapOver(over: Element, under: Element) {
  await tapWhereOver(over, under)
  await settle()
}

const lastLink = (path: string) =>
  `\`${path}\` is in no other library, so it can only be removed: use Remove this directory.`

const refusedWith = (sentence: string) =>
  new Error(
    `/api/library/libraries/1/dirs/1?dry_run=true: 400 ${JSON.stringify({ error: sentence })}`,
  )

const UNLINKED: librariesApi.UnlinkAnswer = {
  unlinked: {
    loses_sight: [],
    library_removed: null,
    grants_removed: 0,
    left_with_no_library: [],
  },
  dry_run: true,
}

describe('a refusal of Remove from Manga…, in a real browser', () => {
  it('goes at a real mouse press on it, focus beneath it or in a box elsewhere, which presses nothing beneath, and the next press on the button opens its dialog', async () => {
    const path = '/srv/manga/volumes'
    vi.mocked(librariesApi.unlinkCost)
      .mockRejectedValueOnce(refusedWith(lastLink(path)))
      .mockResolvedValue(UNLINKED)
    const host = await sharedDirPage(path, 1280)
    const unlink = called(host, 'Remove from Manga…')
    const refusal = () =>
      host.querySelector<HTMLElement>('.callout > [role="alert"]')

    // Focus beneath it: the press blurs the button, to the page.
    await focusFirst(regime(host), unlink)
    await keys('{Enter}')
    await settle()
    expect(document.activeElement).toBe(unlink)
    await pressOn(refusal() as HTMLElement)
    expect(refusal()).toBeNull()
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(1)
    await pressOn(unlink)
    expect(host.querySelector('.unlink-root .confirm')).not.toBeNull()
    await act(async () => {
      called(host, 'Cancel').click()
    })
    expect(host.querySelector('.unlink-root')).toBeNull()

    // Focus in a box, moved there while the rehearsal was out: the button
    // has no focus to lose.
    const out = deferred<librariesApi.UnlinkAnswer>()
    vi.mocked(librariesApi.unlinkCost).mockReturnValueOnce(out.promise)
    await pressOn(unlink)
    const box = host.querySelector(
      'input[name="edition_key"]',
    ) as HTMLInputElement
    await pressOn(box)
    expect(document.activeElement).toBe(box)
    await act(async () => {
      out.reject(refusedWith(lastLink(path)))
    })
    await settle()
    expect(document.activeElement).toBe(box)
    await pressOn(refusal() as HTMLElement)
    expect(refusal()).toBeNull()
    expect(host.querySelector('.unlink-root')).toBeNull()
    await pressOn(unlink)
    expect(host.querySelector('.unlink-root .confirm')).not.toBeNull()
    expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(4)
  })

  it('goes at a real tap on it where it lies over Remove from Manga…, focus beneath it or in a box elsewhere, and the tap presses nothing beneath', async () => {
    const path = '/srv/manga/volumes'
    vi.mocked(librariesApi.unlinkCost)
      .mockRejectedValueOnce(refusedWith(lastLink(path)))
      .mockResolvedValue(UNLINKED)
    const host = await sharedDirPage(path, 390)
    const unlink = called(host, 'Remove from Manga…')
    const refusal = () =>
      host.querySelector<HTMLElement>('.callout > [role="alert"]')

    // Focus beneath it.
    await focusFirst(regime(host), unlink)
    await keys('{Enter}')
    await settle()
    expect(document.activeElement).toBe(unlink)
    await tapOver(refusal() as HTMLElement, unlink)
    expect(refusal()).toBeNull()
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(1)

    // Focus in a box, moved there while the rehearsal was out.
    const out = deferred<librariesApi.UnlinkAnswer>()
    vi.mocked(librariesApi.unlinkCost).mockReturnValueOnce(out.promise)
    await pressOn(unlink)
    const box = host.querySelector(
      'input[name="edition_key"]',
    ) as HTMLInputElement
    await pressOn(box)
    expect(document.activeElement).toBe(box)
    await act(async () => {
      out.reject(refusedWith(lastLink(path)))
    })
    await settle()
    await tapOver(refusal() as HTMLElement, unlink)
    expect(refusal()).toBeNull()
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(librariesApi.unlinkCost).toHaveBeenCalledTimes(2)
  })

  it('lies over Remove from Manga’s row and not Remove this directory’s at 390, for the last-library refusal of a long path', async () => {
    const path = '/mnt/user/media/Manga/Shared'
    vi.mocked(librariesApi.unlinkCost).mockRejectedValue(
      refusedWith(lastLink(path)),
    )
    const host = await sharedDirPage(path, 390)
    const unlink = called(host, 'Remove from Manga…')
    await focusFirst(regime(host), unlink)
    await keys('{Enter}')
    await settle()
    const said = host.querySelector('.callout > [role="alert"]') as HTMLElement
    expect(said.textContent).toBe(lastLink(path))
    const remove = called(host, 'Remove…')
    await expectCallout(said, remove.closest('.settings-row') as Element, {
      above: true,
    })
    expect([liesOver(said, unlink), liesOver(said, remove)]).toEqual([
      true,
      false,
    ])
  })
})

const moved = (dry_run: boolean): librariesApi.MoveAnswer => ({
  moved: {
    library_id: 2,
    library: 'Light novels',
    gains_sight: [],
    loses_sight: [],
    library_removed: null,
    grants_removed: 0,
    left_with_no_library: [],
  },
  dry_run,
})

const REFUSED = new Error(
  '/api/library/roots/1/move: 409 {"error":"a scan or another library job is running; move the directory once it has finished","volumes":[]}',
)

describe('Move…’s dialog, held controls in a real browser', () => {
  /**
   * Opens Move… with real keys and rehearses a move to Light novels, or to
   * a new library called Volumes; focus is left on the move's confirm.
   */
  async function rehearsed(to: 'library' | 'new' = 'library') {
    vi.mocked(librariesApi.moveCost).mockResolvedValue(moved(true))
    const host = await dirPage()
    const move = called(host, 'Move…')
    await focusFirst(regime(host), move)
    await keys('{Enter}')
    const dialog = host.querySelector('dialog.move-root') as HTMLDialogElement
    const select = dialog.querySelector('select') as HTMLSelectElement
    expect(document.activeElement).toBe(select)
    // The select's options are walked with the arrow keys: Light novels is
    // the first after the prompt, and a new library the last.
    await keys(to === 'library' ? '{ArrowDown}' : '{ArrowDown}{ArrowDown}')
    if (to === 'new') {
      await keys('{Tab}Volumes')
    }
    await keys('{Tab}{Enter}')
    await settle()
    const confirm = dialog.querySelector('.confirm') as HTMLButtonElement
    act(() => confirm.focus())
    return { host, move, dialog, select, confirm }
  }

  it('keeps focus on the move’s confirm, pressed with Enter, through its drawn wait; Enter and Escape on it do nothing then', async () => {
    const sent = deferred<librariesApi.MoveAnswer>()
    vi.mocked(librariesApi.moveDir).mockReturnValue(sent.promise)
    const { dialog, confirm } = await rehearsed()

    await keys('{Enter}')
    expect(librariesApi.moveDir).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(confirm)
    expect([confirm.textContent, holdOf(confirm)]).toEqual(['Moving…', 'held'])
    await keys('{Enter}{Escape}')
    expect(librariesApi.moveDir).toHaveBeenCalledTimes(1)
    expect(dialog.matches(':modal')).toBe(true)
    expect(document.activeElement).toBe(confirm)
  })

  it('keeps focus on Move to, reached by Shift+Tab inside the wait’s delay, through the drawn wait and a refusal; its value does not change while it is held', async () => {
    const sent = deferred<librariesApi.MoveAnswer>()
    vi.mocked(librariesApi.moveDir).mockReturnValue(sent.promise)
    const { dialog, select } = await rehearsed()
    expect(select.value).toBe('2')

    // Back past See what this would do.
    await keys('{Enter}{Shift>}{Tab}{Tab}{/Shift}')
    expect(document.activeElement).toBe(select)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(select)
    expect(holdOf(select)).toBe('held')
    await keys('{ArrowDown}')
    expect(select.value).toBe('2')
    await keys('{Escape}')
    expect(dialog.matches(':modal')).toBe(true)

    await act(async () => {
      sent.reject(REFUSED)
    })
    await settle()
    expect(dialog.querySelector('[role="alert"]')?.textContent).toContain(
      'is running',
    )
    expect(document.activeElement).toBe(select)
    expect(holdOf(select)).toBe('free')
  })

  it('keeps focus on Its name, reached by Shift+Tab inside the wait’s delay, through the drawn wait and a refusal; typing changes nothing while it is held', async () => {
    const sent = deferred<librariesApi.MoveAnswer>()
    vi.mocked(librariesApi.moveDir).mockReturnValue(sent.promise)
    const { dialog } = await rehearsed('new')
    const box = dialog.querySelector(
      'input[name="new-library"]',
    ) as HTMLInputElement
    expect(box.value).toBe('Volumes')

    await keys('{Enter}{Shift>}{Tab}{Tab}{/Shift}')
    expect(document.activeElement).toBe(box)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(box)
    expect(holdOf(box)).toBe('held')
    await keys('{End}{Backspace} 2')
    expect(box.value).toBe('Volumes')

    await act(async () => {
      sent.reject(REFUSED)
    })
    await settle()
    expect(document.activeElement).toBe(box)
    expect(holdOf(box)).toBe('free')
    await keys('{End} 2')
    expect(box.value).toBe('Volumes 2')
  })

  it('keeps focus on See what this would do, pressed with Enter, through its drawn wait and the answer', async () => {
    const asked = deferred<librariesApi.MoveAnswer>()
    vi.mocked(librariesApi.moveCost).mockReturnValue(asked.promise)
    const host = await dirPage()
    await focusFirst(regime(host), called(host, 'Move…'))
    await keys('{Enter}{ArrowDown}{Tab}')
    const see = host.querySelector(
      'button[name="rehearse"]',
    ) as HTMLButtonElement
    expect(document.activeElement).toBe(see)

    await keys('{Enter}')
    expect(librariesApi.moveCost).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(see)
    expect(holdOf(see)).toBe('held')
    await keys('{Enter}')
    expect(librariesApi.moveCost).toHaveBeenCalledTimes(1)

    await act(async () => {
      asked.resolve(moved(true))
    })
    await settle()
    expect(host.querySelector('.move-root .confirm')).not.toBeNull()
    expect(document.activeElement).toBe(see)
    expect(holdOf(see)).toBe('free')
  })

  it('hands focus to Move to when a read of the list takes away the library chosen while See what this would do has focus', async () => {
    const host = await dirPage()
    await focusFirst(regime(host), called(host, 'Move…'))
    await keys('{Enter}{ArrowDown}{Tab}')
    const dialog = host.querySelector('dialog.move-root') as HTMLDialogElement
    const select = dialog.querySelector('select') as HTMLSelectElement
    const see = dialog.querySelector(
      'button[name="rehearse"]',
    ) as HTMLButtonElement
    expect(select.value).toBe('2')
    expect(document.activeElement).toBe(see)

    // Light novels was removed elsewhere, and the list read again says so.
    await act(async () => {
      rerender(dirPageOf([]))
    })
    await settle()
    expect(document.activeElement).toBe(select)
    expect([select.value, holdOf(see)]).toEqual(['', 'disabled'])
  })

  /**
   * Opens Move… and chooses Light novels with real keys, asks what the move
   * would do with Enter on See what this would do, and presses `onward`
   * once the answer is drawn.
   */
  async function askedThen(onward: string) {
    const host = await dirPage()
    await focusFirst(regime(host), called(host, 'Move…'))
    await keys('{Enter}{ArrowDown}{Tab}{Enter}')
    await settle()
    await keys(onward)
    const dialog = host.querySelector('dialog.move-root') as HTMLDialogElement
    const select = dialog.querySelector('select') as HTMLSelectElement
    expect(select.value).toBe('2')
    return { dialog, select }
  }

  /** Light novels was removed elsewhere, and the list read again says so. */
  async function lightNovelsGone() {
    await act(async () => {
      rerender(dirPageOf([]))
    })
    await settle()
  }

  it('hands focus to Move to when a read of the list takes away the library chosen while the move’s confirm has focus', async () => {
    vi.mocked(librariesApi.moveCost).mockResolvedValue(moved(true))
    const { dialog, select } = await askedThen('{Tab}')
    expect(document.activeElement).toBe(dialog.querySelector('.confirm'))

    await lightNovelsGone()
    expect(document.activeElement).toBe(select)
    expect([select.value, dialog.querySelector('.confirm')]).toEqual(['', null])
  })

  it('leaves focus on Cancel when a read of the list takes away the library chosen and what was rehearsed for it', async () => {
    vi.mocked(librariesApi.moveCost).mockResolvedValue(moved(true))
    const { dialog } = await askedThen('{Tab}{Tab}')
    const cancel = dialog.querySelector(
      '.settings-actions > button:last-child',
    ) as HTMLButtonElement
    expect(cancel.textContent).toBe('Cancel')
    expect(document.activeElement).toBe(cancel)

    await lightNovelsGone()
    expect(dialog.querySelector('.confirm')).toBeNull()
    expect(document.activeElement).toBe(cancel)
  })

  it('hands focus to Move…, held, when another directory’s scan cancels the dialog', async () => {
    // The feed's first answer lands once the dialog is open: a job holds
    // the other directory.
    const answer = serveFeed(scanningOf(2)).hold()
    const host = await dirPage()
    const move = called(host, 'Move…')
    await focusFirst(regime(host), move)
    await keys('{Enter}')
    expect(host.querySelector('dialog.move-root')).not.toBeNull()

    await act(async () => {
      answer()
    })
    await settle()
    expect(host.querySelector('dialog.move-root')).toBeNull()
    expect(document.activeElement).toBe(move)
    expect(holdOf(move)).toBe('held')
  })
})

describe('Backup, a held control in a real browser', () => {
  it('keeps focus on Back up now, pressed with Enter, through its drawn wait and the answer; Enter on it sends nothing while it is held', async () => {
    const taken = deferred<backupApi.Backup>()
    vi.mocked(backupApi.takeBackup).mockReturnValue(taken.promise)
    await page.viewport(1280, 800)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(<Backup />)
    })
    if (host === undefined) throw new Error('nothing mounted')
    await settle()
    const take = host.querySelector('button[name="take"]') as HTMLButtonElement
    await focusFirst(
      host.querySelector('.page-description') as HTMLElement,
      take,
    )

    await keys('{Enter}')
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(take)
    expect([take.textContent, holdOf(take)]).toEqual(['Backing up…', 'held'])
    await keys('{Enter}')
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)

    await act(async () => {
      taken.resolve({
        name: 'library-20261023T101500Z.db',
        bytes: 1024,
        taken: '20261023T101500Z',
      })
    })
    await settle()
    expect(document.activeElement).toBe(take)
    expect([take.textContent, holdOf(take)]).toEqual(['Back up now', 'free'])
  })
})

const DAVEY: usersApi.User = {
  id: 1,
  name: 'davey',
  role: 'admin',
  can_download: true,
  can_stream: true,
  can_stamp: true,
  can_log_in: true,
  wrong_passwords: 0,
  wrong_password_alarm: false,
  sessions: [],
  keys: [],
  every_library: true,
  libraries: [],
}
const SAM: usersApi.User = {
  ...DAVEY,
  id: 2,
  name: 'sam',
  role: 'user',
  can_stamp: false,
  sessions: [
    {
      id: 'aaa',
      created: '2026-09-01 10:00:00',
      last_seen: '2026-09-06 09:00:00',
      user_agent: 'Firefox',
      ip: '10.0.0.2',
      current: false,
    },
  ],
  keys: [
    {
      id: 9,
      name: 'the tablet',
      created: '2026-08-01 10:00:00',
      last_used: null,
    },
  ],
  every_library: false,
  libraries: [{ id: 1, name: 'Manga' }],
}
const SAM_COST: usersApi.RemovalAnswer = {
  removed: {
    name: 'sam',
    positions: 0,
    notes: 0,
    sessions: 1,
    keys: 1,
    grants: 1,
  },
  dry_run: true,
}
const STORE_LOCKED = new Error(
  '/api/library/users/2: 500 {"error":"the store is locked"}',
)

/** Sam's page, seen by davey, with every write it sends left to the test. */
async function accountPage(): Promise<HTMLElement> {
  await page.viewport(1280, 800)
  vi.mocked(usersApi.removalCost).mockResolvedValue(SAM_COST)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/config/users/sam']}>
        <AccountPage
          user={SAM}
          users={[DAVEY, SAM]}
          libraries={[MANGA]}
          actingAs={1}
          onChanged={() => undefined}
        />
      </MemoryRouter>,
    )
  })
  if (host === undefined) throw new Error('nothing mounted')
  await settle()
  return host
}

const summary = (host: HTMLElement) =>
  host.querySelector('.page-description') as HTMLElement
const account = <T extends Element = HTMLButtonElement>(
  host: HTMLElement,
  selector: string,
) => host.querySelector(selector) as T

describe('an account’s page, held controls in a real browser', () => {
  it('keeps focus on Role, changed with an arrow key, through the write’s drawn wait and a refusal; an arrow key changes nothing while it is held', async () => {
    const sent = deferred<usersApi.User>()
    vi.mocked(usersApi.patchUser).mockReturnValue(sent.promise)
    const host = await accountPage()
    const role = account<HTMLSelectElement>(host, 'select[name="role"]')
    await focusFirst(summary(host), role)

    await keys('{ArrowDown}')
    expect(vi.mocked(usersApi.patchUser).mock.calls).toEqual([
      [2, { role: 'admin' }],
    ])
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(role)
    expect(holdOf(role)).toBe('held')
    await keys('{ArrowUp}{ArrowDown}')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(role.value).toBe('user')

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'the store is locked',
    )
    expect(document.activeElement).toBe(role)
    expect(holdOf(role)).toBe('free')
  })

  it('keeps focus in New password, submitted with Enter in it once Confirm password matches, through the drawn wait and a refusal; typing and Enter do nothing while it is held', async () => {
    const sent = deferred<usersApi.User>()
    vi.mocked(usersApi.patchUser).mockReturnValue(sent.promise)
    const host = await accountPage()
    const box = account<HTMLInputElement>(host, 'input[name="password"]')
    const confirm = account<HTMLInputElement>(
      host,
      'input[name="confirm-password"]',
    )
    const set = account(host, 'button[name="reset"]')
    await focusFirst(summary(host), confirm)

    await keys('hunter22{Shift>}{Tab}{/Shift}')
    expect(document.activeElement).toBe(box)
    await keys('hunter22{Enter}')
    expect(vi.mocked(usersApi.patchUser).mock.calls).toEqual([
      [2, { password: 'hunter22' }],
    ])
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(box)
    expect([holdOf(box), holdOf(confirm), holdOf(set)]).toEqual([
      'held',
      'held',
      'held',
    ])
    await keys('{End}{Backspace}9{Enter}')
    expect(box.value).toBe('hunter22')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(document.activeElement).toBe(box)
    expect([holdOf(box), holdOf(confirm), holdOf(set)]).toEqual([
      'free',
      'free',
      'free',
    ])
    await keys('9')
    expect(box.value).toBe('hunter229')
  })

  it('keeps focus on Set password, pressed with Enter, through the drawn wait and a refusal; Enter on it sends nothing while it is held', async () => {
    const sent = deferred<usersApi.User>()
    vi.mocked(usersApi.patchUser).mockReturnValue(sent.promise)
    const host = await accountPage()
    const box = account<HTMLInputElement>(host, 'input[name="password"]')
    const set = account(host, 'button[name="reset"]')
    await focusFirst(summary(host), box)
    await keys('hunter22{Tab}hunter22{Tab}')
    expect(document.activeElement).toBe(set)

    await keys('{Enter}')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(set)
    expect(holdOf(set)).toBe('held')
    await keys('{Enter}')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(document.activeElement).toBe(set)
    expect(holdOf(set)).toBe('free')
  })

  it('puts focus in New password when a set pressed on Set password with Enter works, past its drawn wait, and empties both boxes', async () => {
    const sent = deferred<usersApi.User>()
    vi.mocked(usersApi.patchUser).mockReturnValue(sent.promise)
    const host = await accountPage()
    const box = account<HTMLInputElement>(host, 'input[name="password"]')
    const confirm = account<HTMLInputElement>(
      host,
      'input[name="confirm-password"]',
    )
    const set = account(host, 'button[name="reset"]')
    await focusFirst(summary(host), box)
    await keys('hunter22{Tab}hunter22{Tab}{Enter}')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(set)

    await act(async () => {
      sent.resolve(SAM)
    })
    await settle()
    expect(host.querySelector('.done')?.textContent).toBe(
      'Password set. sam is signed out everywhere.',
    )
    expect(document.activeElement).toBe(box)
    expect([box.value, confirm.value, holdOf(box), holdOf(set)]).toEqual([
      '',
      '',
      'free',
      'disabled',
    ])
    await keys('x')
    expect(box.value).toBe('x')
  })

  it('holds Set password while the two passwords differ, says so under Confirm password for Enter in either box or on Set password, the confirm empty or not, and sends nothing then', async () => {
    vi.mocked(usersApi.patchUser).mockResolvedValue(SAM)
    const host = await accountPage()
    const box = account<HTMLInputElement>(host, 'input[name="password"]')
    const confirm = account<HTMLInputElement>(
      host,
      'input[name="confirm-password"]',
    )
    const set = account(host, 'button[name="reset"]')
    const line = confirm
      .closest('.settings-row')
      ?.nextElementSibling?.querySelector(':scope > [role="status"]')
    await focusFirst(summary(host), box)

    await keys('hunter22{Enter}')
    expect(line?.textContent).toBe('The two passwords do not match.')
    await keys('{Tab}hunter')
    expect(line?.textContent).toBe('')
    await keys('{Enter}')
    expect(line?.textContent).toBe('The two passwords do not match.')
    await keys('{Backspace}d{Enter}')
    expect(document.activeElement).toBe(confirm)
    expect(holdOf(set)).toBe('held')
    expect(line?.textContent).toBe('The two passwords do not match.')
    // Set password, held, is reached on Tab, and keeps the focus Enter on
    // it turns down.
    await keys('{Tab}{Enter}')
    expect(document.activeElement).toBe(set)
    // The line lies over Set password and takes a press there, so the
    // mouse reaches Set password once the line is quiet again, the confirm
    // the start of the password: it keeps the focus that press turns down
    // too, and the press says the line.
    act(() => confirm.focus())
    await keys('{End}{Backspace}')
    expect(line?.textContent).toBe('')
    await act(async () => {
      await userEvent.click(set, { force: true })
    })
    expect(document.activeElement).toBe(set)
    expect(line?.textContent).toBe('The two passwords do not match.')
    expect(usersApi.patchUser).not.toHaveBeenCalled()

    act(() => confirm.focus())
    await keys('{End}r22{Enter}')
    expect(line?.textContent).toBe('')
    expect(vi.mocked(usersApi.patchUser).mock.calls).toEqual([
      [2, { password: 'hunter22' }],
    ])
  })

  it('keeps focus on Set password when a fill of Confirm password holds it while Set password has focus', async () => {
    const host = await accountPage()
    const box = account<HTMLInputElement>(host, 'input[name="password"]')
    const confirm = account<HTMLInputElement>(
      host,
      'input[name="confirm-password"]',
    )
    const set = account(host, 'button[name="reset"]')
    await focusFirst(summary(host), box)
    await keys('hunter22{Tab}hunter22{Tab}')
    expect(document.activeElement).toBe(set)

    // As a password manager fills a box, with focus left where it was.
    await typeInto(confirm, 'hunter2')
    expect(holdOf(set)).toBe('held')
    expect(document.activeElement).toBe(set)
  })

  it('keeps focus on a session’s Revoke, pressed with Enter, through the drawn wait and a refusal; Enter on it sends nothing while it is held', async () => {
    const sent = deferred<undefined>()
    vi.mocked(usersApi.revokeUserSession).mockReturnValue(sent.promise)
    const host = await accountPage()
    const revoke = account(host, 'button[name="revoke-session-aaa"]')
    await focusFirst(summary(host), revoke)

    await keys('{Enter}')
    expect(usersApi.revokeUserSession).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(revoke)
    expect(holdOf(revoke)).toBe('held')
    await keys('{Enter}')
    expect(usersApi.revokeUserSession).toHaveBeenCalledTimes(1)

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(document.activeElement).toBe(revoke)
    expect(holdOf(revoke)).toBe('free')
  })

  it('keeps focus on a key’s Revoke, tabbed to inside the delay of a session’s revocation, through its drawn wait and a refusal; Enter on it sends nothing then', async () => {
    const sent = deferred<undefined>()
    vi.mocked(usersApi.revokeUserSession).mockReturnValue(sent.promise)
    const host = await accountPage()
    const key = account(host, 'button[name="revoke-key-9"]')
    await focusFirst(
      summary(host),
      account(host, 'button[name="revoke-session-aaa"]'),
    )

    await keys('{Enter}{Tab}')
    expect(document.activeElement).toBe(key)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(key)
    expect(holdOf(key)).toBe('held')
    await keys('{Enter}')
    expect(usersApi.revokeUserKey).not.toHaveBeenCalled()

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(document.activeElement).toBe(key)
    expect(holdOf(key)).toBe('free')
  })

  /** Opens the question that lifting sam's limit asks, with real keys. */
  async function lifting() {
    const host = await accountPage()
    const only = account<HTMLInputElement>(host, 'input[value="only"]')
    await focusFirst(summary(host), only)
    await keys('{ArrowUp}')
    const keep = account(host, 'button[name="keep-limit"]')
    expect(document.activeElement).toBe(keep)
    return { host, lift: account(host, 'button[name="lift-limit"]'), keep }
  }

  it('keeps focus on See every library, pressed with Enter, through the drawn wait and a refusal; Enter on it sends nothing while it is held', async () => {
    const sent = deferred<usersApi.User>()
    vi.mocked(usersApi.patchUser).mockReturnValue(sent.promise)
    const { host, lift } = await lifting()
    await keys('{Shift>}{Tab}{/Shift}')
    expect(document.activeElement).toBe(lift)

    await keys('{Enter}')
    expect(vi.mocked(usersApi.patchUser).mock.calls).toEqual([
      [2, { every_library: true }],
    ])
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(lift)
    expect(holdOf(lift)).toBe('held')
    await keys('{Enter}')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(host.querySelector('.lift-limit')).not.toBeNull()
    expect(document.activeElement).toBe(lift)
    expect(holdOf(lift)).toBe('free')
  })

  it('keeps focus on Keep the limit, tabbed to inside the delay of the lift, through its drawn wait and a refusal; Enter on it does nothing then', async () => {
    const sent = deferred<usersApi.User>()
    vi.mocked(usersApi.patchUser).mockReturnValue(sent.promise)
    const { host, keep } = await lifting()

    await keys('{Shift>}{Tab}{/Shift}{Enter}{Tab}')
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(keep)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(keep)
    expect(holdOf(keep)).toBe('held')
    await keys('{Enter}')
    expect(host.querySelector('.lift-limit')).not.toBeNull()

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(document.activeElement).toBe(keep)
    expect(holdOf(keep)).toBe('free')
  })

  it('keeps focus on Remove…, pressed with Enter, drawn and held while the confirm it opens is open; Enter on it asks nothing more', async () => {
    const host = await accountPage()
    const remove = account(host, 'button[name="remove"]')
    await focusFirst(summary(host), remove)

    await keys('{Enter}')
    await settle()
    expect(host.querySelector('.remove-user .counts')).not.toBeNull()
    expect(document.activeElement).toBe(remove)
    expect(holdOf(remove)).toBe('held')
    await keys('{Enter}')
    expect(usersApi.removalCost).toHaveBeenCalledTimes(1)
  })

  /** Opens the removal with real keys and leaves focus on its confirm. */
  async function confirming() {
    const host = await accountPage()
    await focusFirst(summary(host), account(host, 'button[name="remove"]'))
    await keys('{Enter}')
    await settle()
    const confirm = account(host, 'button[name="confirm-remove"]')
    act(() => confirm.focus())
    return {
      host,
      confirm,
      cancel: account(host, 'button[name="cancel-remove"]'),
    }
  }

  it('keeps focus on the removal’s confirm, pressed with Enter, through its drawn wait and a refusal; Enter on it sends nothing while it is held', async () => {
    const sent = deferred<usersApi.RemovalAnswer>()
    vi.mocked(usersApi.removeUser).mockReturnValue(sent.promise)
    const { host, confirm } = await confirming()

    await keys('{Enter}')
    expect(usersApi.removeUser).toHaveBeenCalledTimes(1)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(confirm)
    expect([confirm.textContent, holdOf(confirm)]).toEqual([
      'Removing…',
      'held',
    ])
    await keys('{Enter}')
    expect(usersApi.removeUser).toHaveBeenCalledTimes(1)

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'the store is locked',
    )
    expect(document.activeElement).toBe(confirm)
    expect([confirm.textContent, holdOf(confirm)]).toEqual([
      'Remove sam',
      'free',
    ])
  })

  it('keeps focus on the removal’s Cancel, tabbed to inside the wait’s delay, through the drawn wait and a refusal; Enter on it does nothing while it is held', async () => {
    const sent = deferred<usersApi.RemovalAnswer>()
    vi.mocked(usersApi.removeUser).mockReturnValue(sent.promise)
    const { host, cancel } = await confirming()

    await keys('{Enter}{Tab}')
    expect(document.activeElement).toBe(cancel)
    await settle(WAIT_DRAWN_AFTER_MS + 100)
    expect(document.activeElement).toBe(cancel)
    expect(holdOf(cancel)).toBe('held')
    await keys('{Enter}')
    expect(host.querySelector('.remove-user')).not.toBeNull()

    await act(async () => {
      sent.reject(STORE_LOCKED)
    })
    await settle()
    expect(document.activeElement).toBe(cancel)
    expect(holdOf(cancel)).toBe('free')
  })
})

/**
 * A control held while focus is elsewhere, by another control's press or
 * because it has nothing to do yet, stays `disabled`, out of the tab order.
 */
describe('a control that cannot have focus as its hold comes on, in a real browser', () => {
  it('passes over See what this would do on Tab from Move to before a library is chosen', async () => {
    const host = await dirPage()
    await focusFirst(regime(host), called(host, 'Move…'))
    await keys('{Enter}')
    const dialog = host.querySelector('dialog.move-root') as HTMLDialogElement
    const see = dialog.querySelector(
      'button[name="rehearse"]',
    ) as HTMLButtonElement
    const cancel = [...dialog.querySelectorAll('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    expect(document.activeElement).toBe(dialog.querySelector('select'))

    await keys('{Tab}')
    expect(document.activeElement).not.toBe(see)
    expect(document.activeElement).toBe(cancel)
    expect(holdOf(see)).toBe('disabled')
  })

  it('passes over Scan library on Shift+Tab from Remove… while the removal it opened is open', async () => {
    const host = await libraryPage()
    const remove = named(host, 'remove-library')
    await focusFirst(words(host), remove)
    await keys('{Enter}')
    await settle()
    expect(document.activeElement).toBe(remove)

    await keys('{Shift>}{Tab}{/Shift}')
    expect(document.activeElement).not.toBe(named(host, 'scan-library'))
    expect(document.activeElement?.textContent).toContain('Libraries')
  })

  it('passes over Scan directory and Move… on Tab from the way back while the removal is open', async () => {
    const host = await dirPage()
    await focusFirst(regime(host), called(host, 'Remove…'))
    await keys('{Enter}')
    await settle()
    const back = host.querySelector('.dir-page a') as HTMLAnchorElement
    expect(back.textContent).toContain('Manga')
    act(() => back.focus())

    await keys('{Tab}')
    expect(document.activeElement).not.toBe(called(host, 'Scan directory'))
    expect(document.activeElement).not.toBe(called(host, 'Move…'))
    // The Scan schedule's radio that is checked: Tab stops at a radio
    // group's choice.
    const every = host.querySelector<HTMLInputElement>(
      'input[name^="scan_schedule-"][value="every"]',
    )
    expect(every?.checked).toBe(true)
    expect(document.activeElement).toBe(every)
  })
})
