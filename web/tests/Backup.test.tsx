import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as backupApi from '../src/backup'
import { Backup } from '../src/Backup'
import { deferred, flush } from './support/async'
import { holdOf } from './support/held'
import { mount, rerender } from './support/mount'
import { expectRowsWired, helpOf, section } from './support/settingsRows'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

vi.mock('../src/backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/backup')>()),
  listBackups: vi.fn(),
  takeBackup: vi.fn(),
}))

const entry = (over: Partial<backupApi.Backup> = {}): backupApi.Backup => ({
  name: 'library-20260906T143502Z.db',
  bytes: 1024 * 1024 * 12,
  taken: '20260906T143502Z',
  ...over,
})

async function settle() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Backup />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

async function press(host: HTMLElement, selector: string) {
  const button = host.querySelector(selector) as HTMLButtonElement
  if (!button) throw new Error(`no ${selector}`)
  await act(async () => {
    button.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** The backups listed in the Backups box, one `li` each. */
const listed = (host: HTMLElement) => [
  ...section(host, 'Backups').querySelectorAll(
    ':scope > .settings-box > .backup-rows > li',
  ),
]

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [] })
})

describe('Backup', () => {
  it('lists what exists, in the order the server sent, with a link each', async () => {
    vi.mocked(backupApi.listBackups).mockResolvedValue({
      backups: [
        entry(),
        entry({
          name: 'library-20260901T090000Z.db',
          taken: '20260901T090000Z',
        }),
      ],
    })
    const host = await settle()
    // Per element rather than over the page: the newest-first order and the
    // per-row link are the two things a whole-page match cannot see. The
    // link is the row's label and the time and size are its help.
    expect(
      listed(host).map((li) => {
        const link = li.querySelector(
          ':scope > .settings-row > .settings-row-text > .settings-row-label > a',
        )
        return {
          name: link?.textContent,
          href: link?.getAttribute('href'),
          download: link?.hasAttribute('download'),
          help: helpOf(li.querySelector('.settings-row') as Element)
            ?.textContent,
        }
      }),
    ).toEqual([
      {
        name: 'library-20260906T143502Z.db',
        href: '/api/library/backups/library-20260906T143502Z.db',
        download: true,
        help: '2026-09-06 14:35:02 UTC\u00a0·\u00a012.0 MiB',
      },
      {
        name: 'library-20260901T090000Z.db',
        href: '/api/library/backups/library-20260901T090000Z.db',
        download: true,
        help: '2026-09-01 09:00:00 UTC\u00a0·\u00a012.0 MiB',
      },
    ])
  })

  it('says so when there are none, rather than drawing an empty list', async () => {
    const host = await settle()
    const box = section(host, 'Backups').querySelector(
      ':scope > .settings-box',
    ) as HTMLElement
    // The line is the box's one child: no list, empty or not, beside it.
    expect([...box.children].map((child) => child.className)).toEqual(['empty'])
    expect(box.textContent).toBe(
      'None yet. Back up now takes one with VACUUM INTO on the live database, so nothing has to stop.',
    )
  })

  /**
   * What follows the header, in order: each box by its title, the
   * disclosure by its summary, and the wait line by its words.
   */
  const parts = (host: HTMLElement) =>
    JSON.stringify(
      [
        ...host.querySelectorAll(
          '.backup > :is(section, details, p[role="status"])',
        ),
      ].map((part) =>
        part.matches('p')
          ? part.textContent
          : part.querySelector(
              ':scope > .settings-section-head > .settings-section-title, :scope > summary',
            )?.textContent,
      ),
    )

  it('heads itself at once on a first visit, draws Loading… in its body’s place from 200 ms, and Backups with How backups work in one step once the list lands', async () => {
    holdTheClock()
    const list = deferred<{ backups: backupApi.Backup[] }>()
    vi.mocked(backupApi.listBackups).mockReturnValue(list.promise)
    const host = await settle()
    expect(host.querySelector('.backup .page-title')?.textContent).toBe(
      'Backup',
    )
    expect(parts(host)).toBe(JSON.stringify([]))
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(parts(host)).toBe(JSON.stringify([]))
    pass(1)
    expect(parts(host)).toBe(JSON.stringify(['Loading…']))

    const drawn = watchDrawn(() => parts(host))
    await flush(() => list.resolve({ backups: [entry()] }))
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify(['Loading…']),
      JSON.stringify(['Backups', 'How backups work']),
    ])
    expect(listed(host)).toHaveLength(1)
  })

  it('draws the list at once on a return, reads it again, and draws its answer in one step with no wait', async () => {
    holdTheClock()
    vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [entry()] })
    await settle()
    rerender(<div />)
    const again = deferred<{ backups: backupApi.Backup[] }>()
    vi.mocked(backupApi.listBackups).mockClear()
    vi.mocked(backupApi.listBackups).mockReturnValue(again.promise)

    rerender(<Backup />)
    const host = document.querySelector('.backup')?.parentElement as HTMLElement
    expect(parts(host)).toBe(JSON.stringify(['Backups', 'How backups work']))
    expect(listed(host)).toHaveLength(1)
    expect(backupApi.listBackups).toHaveBeenCalledTimes(1)
    pass(WAIT_DRAWN_AFTER_MS)
    const drawn = watchDrawn(() =>
      JSON.stringify([parts(host), listed(host).length]),
    )
    await flush(() =>
      again.resolve({
        backups: [entry(), entry({ name: 'library-20260901T090000Z.db' })],
      }),
    )
    drawn.stop()
    expect([...drawn.seen]).toEqual([
      JSON.stringify([JSON.stringify(['Backups', 'How backups work']), 1]),
      JSON.stringify([JSON.stringify(['Backups', 'How backups work']), 2]),
    ])
  })

  it('draws the list’s failure and no body, when the list cannot be read', async () => {
    holdTheClock()
    vi.mocked(backupApi.listBackups).mockRejectedValue(
      new Error('/api/library/backups: 403 only an administrator may do this'),
    )
    const host = await settle()
    await flush()
    pass(WAIT_DRAWN_AFTER_MS)
    expect(
      [...host.querySelectorAll('.backup > [role="alert"]')].map(
        (p) => p.textContent,
      ),
    ).toEqual(['/api/library/backups: 403 only an administrator may do this'])
    expect(parts(host)).toBe(JSON.stringify([]))
  })

  it('takes one and shows it without a page reload', async () => {
    vi.mocked(backupApi.takeBackup).mockResolvedValue(entry())
    vi.mocked(backupApi.listBackups)
      .mockResolvedValueOnce({ backups: [] })
      .mockResolvedValueOnce({ backups: [entry()] })
    const host = await settle()
    await press(host, 'button[name="take"]')
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
    expect(listed(host)).toHaveLength(1)
  })

  it('keeps the newest list when a slower read lands after it', async () => {
    // Two reads are routinely in flight — the mount's and the one a finished
    // POST fires — and nothing makes them answer in order. The mount's
    // answer, landing last, would put the pre-backup list back over the file
    // that had just been written; the store keeps the newest request's.
    let settleMount:
      | ((body: { backups: backupApi.Backup[] }) => void)
      | undefined
    vi.mocked(backupApi.listBackups)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            settleMount = resolve
          }),
      )
      .mockResolvedValueOnce({ backups: [entry()] })
    vi.mocked(backupApi.takeBackup).mockResolvedValue(entry())

    const host = await settle()
    await press(host, 'button[name="take"]')
    // The mount's read answers *now*, with the list as it was before the
    // backup existed.
    await act(async () => {
      settleMount?.({ backups: [] })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(listed(host)).toHaveLength(1)
  })

  it('draws the same-second collision as the timing it is', async () => {
    // The server's own sentence names the whole-second timestamp. The screen
    // renders it rather than adding a word like "failed" over it — and it
    // cannot classify by status, because the in-memory refusal below is also
    // a 409.
    vi.mocked(backupApi.takeBackup).mockRejectedValue(
      new Error(
        '/api/library/backups: 409 a backup named library-20260906T143502Z.db is already there; the name carries a whole-second timestamp, so a second backup inside the same second has nowhere to go',
      ),
    )
    const host = await settle()
    await press(host, 'button[name="take"]')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'whole-second timestamp',
    )
  })

  it('keeps the refusal when the list read that follows a take lands', async () => {
    // The list read's answer must not clear the take's refusal: the 409
    // this screen is written around would be drawn and then wiped a moment
    // later by a list read already in flight, leaving a screen that looks as
    // though nothing happened.
    let settleList:
      | ((body: { backups: backupApi.Backup[] }) => void)
      | undefined
    vi.mocked(backupApi.listBackups).mockImplementation(
      () =>
        new Promise((resolve) => {
          settleList = resolve
        }),
    )
    vi.mocked(backupApi.takeBackup).mockRejectedValue(
      new Error(
        '/api/library/backups: 409 a backup named library-20260906T143502Z.db is already there; the name carries a whole-second timestamp',
      ),
    )
    const host = await settle()
    await press(host, 'button[name="take"]')
    expect(host.textContent).toContain('whole-second timestamp')

    await act(async () => {
      settleList?.({ backups: [entry()] })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('whole-second timestamp')
  })

  it('draws the in-memory library refusal, which is the other 409', async () => {
    vi.mocked(backupApi.listBackups).mockRejectedValue(
      new Error(
        '/api/library/backups: 409 this library is in memory and has no file to back up',
      ),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'in memory and has no file to back up',
    )
  })

  it('sends one POST for two presses before the first answers', async () => {
    vi.mocked(backupApi.takeBackup).mockImplementation(
      () => new Promise(() => undefined),
    )
    const host = await settle()
    const take = host.querySelector('button[name="take"]') as HTMLButtonElement
    // The ref turns a second press down from the send until the wait is
    // drawn, `WAIT_DRAWN_AFTER_MS` later: until then the button is not held.
    // Two presses inside one `act`, before React re-renders, are the
    // earliest case of that. A press once the wait is drawn lands on a held
    // button, which turns it down with or without the ref.
    await act(async () => {
      take.click()
      take.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
  })

  describe('its wait, drawn only once it has lasted', () => {
    const take = (host: HTMLElement) =>
      host.querySelector('button[name="take"]') as HTMLButtonElement
    /** Back up now's words, and whether it is drawn held. */
    const state = (host: HTMLElement) => [
      take(host).textContent,
      holdOf(take(host)),
    ]

    it('draws no wait and no held button for a backup answered inside the wait’s delay', async () => {
      holdTheClock()
      const taken = deferred<backupApi.Backup>()
      vi.mocked(backupApi.takeBackup).mockReturnValue(taken.promise)
      const host = await settle()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await press(host, 'button[name="take"]')
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() => taken.resolve(entry()))
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(backupApi.listBackups).toHaveBeenCalledTimes(2)
      expect([...drawn.seen]).toEqual([JSON.stringify(['Back up now', 'free'])])
    })

    it('draws Backing up… and holds Back up now from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(backupApi.takeBackup).mockReturnValue(new Promise(() => {}))
      const host = await settle()
      await press(host, 'button[name="take"]')
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Back up now', 'free'])
      pass(1)
      expect(state(host)).toEqual(['Backing up…', 'held'])
      await press(host, 'button[name="take"]')
      expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
    })

    it('sends one backup for a second press inside the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(backupApi.takeBackup).mockReturnValue(new Promise(() => {}))
      const host = await settle()
      await press(host, 'button[name="take"]')
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Back up now', 'free'])
      await press(host, 'button[name="take"]')
      expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
    })
  })

  it('carries all four of C4 §5 sentences', async () => {
    // Load-bearing copy, asserted one clause at a time and where it is
    // drawn: each of the four says something a person cannot find out from
    // the buttons. The hashes are the header's line, outside the
    // disclosure, so they are read before Back up now is pressed.
    const host = await settle()
    const line = host.querySelector('.page-header .page-description')
    expect(line?.textContent).toContain('password hashes')
    const behind = host.querySelector('details.settings-details')
    expect(behind?.querySelector(':scope > summary')?.textContent).toBe(
      'How backups work',
    )
    const told = behind?.textContent ?? ''
    expect(told).toContain('Stop the server')
    expect(told).toContain('A sidecar export is not a backup')
    expect(told).toContain('taiyaku library backup')
    expect(told).toContain('--keep')
  })

  it('heads itself Backup, the whole-database warning its line and Back up now its action, over Backups and How backups work', async () => {
    vi.mocked(backupApi.listBackups).mockResolvedValue({ backups: [entry()] })
    const host = await settle()
    const header = host.querySelector('.backup > .page-header') as HTMLElement
    expect(header.querySelector('.page-title')?.tagName).toBe('H2')
    expect(header.querySelector('.page-title')?.textContent).toBe('Backup')
    expect(header.querySelector('.page-description')?.textContent).toBe(
      'A backup is the whole database, password hashes included: anyone holding one can read every account’s stored hash offline.',
    )
    expect(
      [...header.querySelectorAll('.page-actions button')].map((b) => [
        b.getAttribute('name'),
        b.textContent,
      ]),
    ).toEqual([['take', 'Back up now']])
    // What follows the header, in order, by what each is.
    expect(
      [...host.querySelectorAll('.backup > :is(section, details)')].map(
        (part) =>
          part.querySelector(
            ':scope > .settings-section-head > .settings-section-title, :scope > summary',
          )?.textContent,
      ),
    ).toEqual(['Backups', 'How backups work'])
    expect(section(host, 'Backups').querySelector('h3')?.textContent).toBe(
      'Backups',
    )
  })

  it('holds each backup’s row to the rows’ wiring', async () => {
    vi.mocked(backupApi.listBackups).mockResolvedValue({
      backups: [entry(), entry({ name: 'library-20260901T090000Z.db' })],
    })
    const host = await settle()
    // Both rows reached, so the check is not over fewer than are drawn. A
    // row only reports today; a control added to one is held from then on.
    expect(host.querySelectorAll('.settings-row')).toHaveLength(2)
    expectRowsWired(host)
  })
})
