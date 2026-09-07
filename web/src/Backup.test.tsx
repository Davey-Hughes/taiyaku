import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as backupApi from './backup'
import { Backup } from './Backup'
import { mount } from './testing/mount'

vi.mock('./backup', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./backup')>()),
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
    const rows = [...host.querySelectorAll('.backup-list > li')]
    expect(rows).toHaveLength(2)
    // Per element rather than over the page: the newest-first order and the
    // per-row link are the two things a whole-page match cannot see.
    expect(rows[0]?.querySelector('a')?.getAttribute('href')).toBe(
      '/api/library/backups/library-20260906T143502Z.db',
    )
    expect(rows[0]?.querySelector('a')?.hasAttribute('download')).toBe(true)
    expect(rows[0]?.textContent).toContain('2026-09-06 14:35:02 UTC')
    expect(rows[0]?.textContent).toContain('12.0 MiB')
    expect(rows[1]?.textContent).toContain('2026-09-01 09:00:00 UTC')
  })

  it('says so when there are none, rather than drawing an empty list', async () => {
    const host = await settle()
    expect(host.textContent).toContain('No backups yet')
  })

  it('takes one and shows it without a page reload', async () => {
    vi.mocked(backupApi.takeBackup).mockResolvedValue(entry())
    vi.mocked(backupApi.listBackups)
      .mockResolvedValueOnce({ backups: [] })
      .mockResolvedValueOnce({ backups: [entry()] })
    const host = await settle()
    await press(host, 'button[name="take"]')
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
    expect(host.querySelectorAll('.backup-list > li')).toHaveLength(1)
  })

  it('keeps the newest list when a slower read lands after it', async () => {
    // Two reads are routinely in flight — the mount's and the one a finished
    // POST fires — and nothing makes them answer in order. Without the
    // sequence counter the mount's stale answer put the pre-backup list back
    // over the file that had just been written.
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
    expect(host.querySelectorAll('.backup-list > li')).toHaveLength(1)
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
    // One slot served both, and `reload`'s success path clears it — so the
    // 409 this screen is written around was drawn and then wiped a moment
    // later by a list read already in flight, leaving a screen that looks as
    // though nothing happened. `Users.tsx` split its two for this reason.
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
    // **Both presses inside one `act`, before React re-renders** — the window
    // the ref guards. A second press after a render lands on a disabled
    // button, which passes with or without the guard.
    await act(async () => {
      take.click()
      take.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(backupApi.takeBackup).toHaveBeenCalledTimes(1)
  })

  it('carries all four of C4 §5 sentences', async () => {
    // Load-bearing copy, asserted one clause at a time: each of the four says
    // something a person cannot find out from the buttons.
    const host = await settle()
    const text = host.textContent ?? ''
    expect(text).toContain('Stop the server')
    expect(text).toContain('password hashes')
    expect(text).toContain('A sidecar export is not a backup')
    expect(text).toContain('taiyaku library backup')
    expect(text).toContain('--keep')
  })
})
