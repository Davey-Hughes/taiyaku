import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { RootRow } from './RootRow'
import { mount, rerender } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))

const root = (over: Partial<config.Root> = {}): config.Root => ({
  id: 3,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  ...over,
})

/**
 * A whole `DeriveReport`, because `RemoveRoot` renders one.
 *
 * `{} as Derived` passed while the dialog drew only its three counts and
 * stopped passing the moment it drew the report too — a cast that lies about a
 * shape, caught by the component that started reading it.
 */
const emptyDerive: config.Derived = {
  series: 0,
  works: 0,
  editions: 0,
  members: 0,
  unattached_chapter_dirs: 0,
  unreadable: 0,
  suppressed: 0,
  rekeyed: 0,
  kept_editions: 0,
  emptied_works: 0,
  unfolded_works: 0,
  unfoldable: [],
  unreadable_hand: [],
  repaired_documents: 0,
  unwritable_documents: [],
  orphan_documents: [],
  declined_documents: [],
  stale_documents: [],
  moved_positions: 0,
  lost_positions: 0,
  moved_notes: 0,
  dropped_notes: 0,
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
  uncarried_alignments: 0,
  series_holding_no_work: 0,
  merged: 0,
  unrecognised_kinds: 0,
  raised: 0,
  root_removed_mid_derive: 0,
}

function press(host: HTMLElement, label: string) {
  const button = [...host.querySelectorAll('button')].find((b) =>
    b.textContent?.includes(label),
  )
  if (!button) throw new Error(`no button matching ${label}`)
  return button
}

/**
 * Set a text box and blur it, the way a person committing an edit does.
 *
 * **`focusout`, not `blur`.** React attaches `onBlur` to the `focusout`
 * event — `blur` does not bubble, and React has delegated through `focusout`
 * since v17 — so a dispatched `blur` reaches no handler at all. That is not a
 * detail: the "sends nothing when a field is blurred unchanged" test below
 * asserts a *negative*, so it passed against a helper that fired nothing,
 * which is a green test that checked nothing. The native value setter is the
 * same fix `Search.test.tsx` and `Account.test.tsx` already carry, and for
 * the same reason: React's per-instance value tracker swallows a plain
 * assignment.
 */
async function edit(host: HTMLElement, name: string, value: string) {
  const box = host.querySelector(`[name="${name}"]`) as
    | HTMLInputElement
    | HTMLTextAreaElement
  await act(async () => {
    box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    const setter = Object.getOwnPropertyDescriptor(
      box instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, value)
    box.dispatchEvent(new Event('input', { bubbles: true }))
    box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

/**
 * Type a value, then type the box back to what it already showed, and leave —
 * all inside one focus session.
 *
 * **`edit` cannot stage this, and a test written with it passes against a
 * deleted guard.** `edit` drives the box straight to its target value, and
 * React's per-instance value tracker was seeded at mount with `defaultValue`
 * — so setting a box to the value it already displays fires no `input` event
 * at all. `edited` stays empty and the blur returns at the *first* guard,
 * which makes such a test indistinguishable from `tabThrough` and blind to
 * the equality guard it claims to be about. Verified by replacing
 * `if (next === field.from(root)) return` with a constant no box can hold:
 * the whole jsdom project stayed green. The value has to genuinely move and
 * come back for the second guard to be the one that stops the write.
 */
async function retype(
  host: HTMLElement,
  name: string,
  via: string,
  back: string,
) {
  const box = host.querySelector(`[name="${name}"]`) as
    | HTMLInputElement
    | HTMLTextAreaElement
  const setter = Object.getOwnPropertyDescriptor(
    box instanceof HTMLTextAreaElement
      ? HTMLTextAreaElement.prototype
      : HTMLInputElement.prototype,
    'value',
  )?.set
  await act(async () => {
    box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    for (const value of [via, back]) {
      setter?.call(box, value)
      box.dispatchEvent(new Event('input', { bubbles: true }))
    }
    box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** Focus a box and leave it, touching nothing — tabbing through the form. */
async function tabThrough(host: HTMLElement, name: string) {
  const box = host.querySelector(`[name="${name}"]`) as
    | HTMLInputElement
    | HTMLTextAreaElement
  await act(async () => {
    box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

async function settle(over: Partial<config.Root> = {}, onChanged = vi.fn()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RootRow root={root(over)} probe={undefined} onChanged={onChanged} />,
    )
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

beforeEach(() => {
  // `mockResolvedValue` sets an implementation and leaves the call log
  // alone, and this project sets no `clearMocks` in `vite.config.ts` — so a
  // test asserting a *negative* would otherwise read the previous test's
  // calls and fail, or worse, pass on them.
  vi.clearAllMocks()
  vi.useFakeTimers({ shouldAdvanceTime: true })
  vi.mocked(config.patchRoot).mockResolvedValue(undefined)
  // `RemoveRoot` is mounted by this component, so its two routes are mocked
  // here as well — otherwise the dialog reaches the real client, `fetch` is
  // not stubbed in this suite, and the dialog renders its failure state with
  // no confirm button at all.
  vi.mocked(config.removalCost).mockResolvedValue({
    removed: { works: 412, positions: 3, notes: 27 },
    derive: emptyDerive,
    dry_run: true,
  })
  vi.mocked(config.removeRoot).mockResolvedValue({
    removed: { works: 412, positions: 3, notes: 27 },
    derive: emptyDerive,
    dry_run: false,
  })
  vi.mocked(config.startScan).mockResolvedValue({
    root_id: 3,
    status_url: '/api/library/roots/3/scan',
  })
  // **The card asks the status route once on mount**, so every test needs an
  // answer for it whether or not it is about scanning — without one the mock
  // resolves `undefined` and the `.then` throws. The default is "nothing
  // running, nothing recorded", which is what a freshly-served root answers.
  vi.mocked(config.scanStatus).mockResolvedValue({
    root_id: 3,
    running: false,
    report: null,
    last_scan_at: null,
    last_scan_status: null,
  })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('RootRow', () => {
  it('says a root has never been scanned rather than drawing a null', async () => {
    const host = await settle()
    expect(host.textContent).toContain('never scanned')
    expect(host.textContent).not.toContain('null')
  })

  it('says the watch state is unknown for a root it did not just add', async () => {
    const host = await settle()
    expect(host.textContent).toContain('watch state unknown')
  })

  it('patches only the field that changed', async () => {
    const host = await settle()
    await edit(host, 'default_language', 'ja')
    expect(config.patchRoot).toHaveBeenCalledWith(3, { default_language: 'ja' })
  })

  it('clears a field to null rather than to an empty string', async () => {
    // `patch_nullable_text` reads an absent key as "leave it alone" and an
    // explicit null as "clear it". An empty string is neither, and would set
    // the column to '' — a value nothing in the store ever means.
    const host = await settle()
    await edit(host, 'edition_key', '')
    expect(config.patchRoot).toHaveBeenCalledWith(3, { edition_key: null })
  })

  it('sends nothing when a field is blurred unchanged', async () => {
    // This is the `edited` guard, not the equality one: driving a box to the
    // value it already shows fires no `input` event, so it is `tabThrough`
    // by another route. The equality guard has its own test, with `retype`.
    const host = await settle()
    await edit(host, 'edition_key', 'en')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('draws the 409 naming the scan already running', async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 a scan of root 3 (/srv/manga) is already running',
      ),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already running',
    )
  })

  it('polls while a scan runs and stops when it finishes', async () => {
    vi.mocked(config.scanStatus)
      // **The mount read takes the first answer.** The card asks the status
      // route once when it mounts, so a queued `…Once` chain that did not
      // account for it had the poll reading the answer meant for the mount —
      // and this test then saw the finished report where it expected
      // "Scanning". Stated here because it is the shape every test in this
      // file that queues answers has to respect.
      .mockResolvedValueOnce({
        root_id: 3,
        running: false,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValueOnce({
        root_id: 3,
        running: true,
        elapsed_secs: 1.2,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValue({
        root_id: 3,
        running: false,
        elapsed_secs: 4.5,
        report: { error: 'the scan panicked' },
        last_scan_at: '2026-09-04T12:00:00Z',
        last_scan_status: 'failed',
      })

    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Scanning')

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('the scan panicked')

    const calls = vi.mocked(config.scanStatus).mock.calls.length
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS * 3)
      await Promise.resolve()
    })
    expect(vi.mocked(config.scanStatus).mock.calls.length).toBe(calls)
  })

  it('reloads the list once a scan finishes', async () => {
    const onChanged = vi.fn()
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      elapsed_secs: 4.5,
      report: { error: 'nope' },
      last_scan_at: '2026-09-04T12:00:00Z',
      last_scan_status: 'failed',
    })
    const host = await settle({}, onChanged)
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(onChanged).toHaveBeenCalled()
  })

  it('draws the failure when a patch is refused', async () => {
    vi.mocked(config.patchRoot).mockRejectedValue(
      new Error(
        "/api/library/roots/3: 400 the root's `edition_key` must be a string or null",
      ),
    )
    const host = await settle()
    await edit(host, 'default_kind', 'comic')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'must be a string or null',
    )
  })

  it('toggles keeps_documents as a boolean, not as a string', async () => {
    const host = await settle()
    const box = host.querySelector(
      'input[name="keeps_documents"]',
    ) as HTMLInputElement
    await act(async () => {
      box.click()
      await Promise.resolve()
    })
    expect(config.patchRoot).toHaveBeenCalledWith(3, { keeps_documents: true })
  })

  it('clears the interval to null and sets it as a number', async () => {
    const host = await settle()
    await edit(host, 'scan_interval_secs', '900')
    expect(config.patchRoot).toHaveBeenCalledWith(3, {
      scan_interval_secs: 900,
    })

    vi.clearAllMocks()
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    await edit(host, 'scan_interval_secs', '')
    expect(config.patchRoot).toHaveBeenCalledWith(3, {
      scan_interval_secs: null,
    })
  })

  it('sends nothing when the interval is blurred unchanged', async () => {
    const host = await settle()
    await edit(host, 'scan_interval_secs', '86400')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('keeps watching a scan through a single failed poll', async () => {
    // **Review pass 2, finding 2 — and the test this replaces asserted the
    // defect.** The `.catch` used to `setRunning(false)`, so one blip — a
    // proxy 502, a `SQLITE_BUSY` while the scan's own writes hold the file —
    // tore the interval down for good: no progress, no eventual report, and a
    // re-enabled "Scan now" whose only answer is a 409 about the scan still
    // running. That is precisely the state this file's mount-read exists to
    // prevent, reached by a different door. The old test pinned it as
    // intended behaviour, which is worse than not having tested it.
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: false,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockRejectedValueOnce(new Error('502 Bad Gateway'))
      .mockResolvedValue({
        root_id: 3,
        running: true,
        elapsed_secs: 3,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })

    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The failing poll.
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    // Still watching, and the next poll recovers.
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Scanning')
  })

  it('gives up after a run of failed polls, and says which error', async () => {
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: false,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockRejectedValue(new Error('500 reading the registry'))
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    for (let i = 0; i < 4; i++) {
      await act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
    }
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'reading the registry',
    )
    const calls = vi.mocked(config.scanStatus).mock.calls.length
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS * 3)
      await Promise.resolve()
    })
    expect(vi.mocked(config.scanStatus).mock.calls.length).toBe(calls)
  })

  it('will not start a scan while the removal dialog is quoting counts', async () => {
    // **Review pass 2, finding 1.** `RemoveRoot`'s dry run is fetched once,
    // on `[root.id]`, and never refetched — so a scan started from the same
    // card while the dialog is open imports works underneath a sentence that
    // is still quoting the old counts, and the confirm below it then sends a
    // real DELETE into a running derive.
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(press(host, 'Scan now').disabled).toBe(true)
  })

  it('puts the checkbox back when its save is refused', async () => {
    // **Review pass 2, finding 6.** An uncontrolled checkbox cannot be reset
    // by a reloaded prop, so a refused PATCH left the box asserting the
    // opposite of what migration 012 actually holds for that root — and the
    // only way back was to click it twice.
    vi.mocked(config.patchRoot).mockRejectedValue(new Error('500 disk full'))
    const host = await settle()
    const box = host.querySelector(
      'input[name="keeps_documents"]',
    ) as HTMLInputElement
    expect(box.checked).toBe(false)
    await act(async () => {
      box.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'disk full',
    )
    expect(box.checked).toBe(false)
  })

  it('draws a finished report, and the regime line beside it', async () => {
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      elapsed_secs: 4.5,
      report: {
        scan: {
          root: '/srv/manga',
          dirs: 2,
          files: 5,
          added: 5,
          updated: 0,
          gone: 0,
          refused: null,
          unreadable_dirs: [],
          excluded_dirs: [],
          excluded_files: 0,
          bad_exclude_patterns: [],
          forgotten_by_exclusion: 0,
        },
        derive: {
          series: 1,
          works: 5,
          editions: 5,
          members: 5,
          unattached_chapter_dirs: 0,
          unreadable: 0,
          suppressed: 0,
          rekeyed: 0,
          kept_editions: 0,
          emptied_works: 0,
          unfolded_works: 0,
          unfoldable: [],
          unreadable_hand: [],
          repaired_documents: 0,
          unwritable_documents: [],
          orphan_documents: [],
          declined_documents: [],
          stale_documents: [],
          moved_positions: 0,
          lost_positions: 0,
          moved_notes: 0,
          dropped_notes: 0,
          moved_truth_pages: 0,
          moved_truth_collisions: 0,
          discarded_rows: 0,
          uncarried_alignments: 0,
          series_holding_no_work: 0,
          merged: 0,
          unrecognised_kinds: 0,
          raised: 0,
          root_removed_mid_derive: 0,
        },
        note: 'a startup snapshot',
      },
      last_scan_at: '2026-09-04T12:00:00Z',
      last_scan_status: 'ok',
    })
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.scan-report')).not.toBeNull()
    expect(host.textContent).toContain('What derivation made of it')
  })

  it('opens the removal dialog and closes it again without removing', async () => {
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).not.toBeNull()
    await act(async () => {
      press(host, 'Cancel').click()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).toBeNull()
  })

  it('draws the watch state for a root it was handed a probe for', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RootRow
          root={root()}
          probe={{ watching: false, watch_reason: 'inotify limit reached' }}
          onChanged={vi.fn()}
        />,
      )
      await Promise.resolve()
    })
    expect(host?.textContent).toContain('inotify limit reached')
  })

  it('shows the exclude_globs pattern the row holds', async () => {
    // **O92's readable half.** `GET /api/library/roots` sends the column now,
    // so the box starts at what is stored instead of empty — which is what
    // made it write-only, and what three passes of this branch's review had
    // to design around without ever being able to see the value.
    const host = await settle({ exclude_globs: '**/extras/**' })
    const box = host.querySelector(
      'textarea[name="exclude_globs"]',
    ) as HTMLTextAreaElement
    expect(box.value).toBe('**/extras/**')
  })

  it('patches exclude_globs, the column the scanner now applies', async () => {
    // O92, closed: the column is read back, patched here, and applied by
    // `scan::scan_root`'s walk. This pins that the box is wired to the patch
    // route at all, which is separate from the two tests above that pin what
    // it reads.
    const host = await settle()
    await edit(host, 'exclude_globs', '**/extras/**')
    expect(config.patchRoot).toHaveBeenCalledWith(3, {
      exclude_globs: '**/extras/**',
    })
  })

  it('closes the dialog and reloads once a removal goes through', async () => {
    const onChanged = vi.fn()
    const host = await settle({}, onChanged)
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(onChanged).toHaveBeenCalled()
  })

  // --- whole-branch review, 2026-09-04 -----------------------------------

  it('clears a stored exclude_globs pattern', async () => {
    // **Review finding 1, restaged on the row that now reports the value.**
    // The blur guard compares against `field.from(root)`, which was hard-coded
    // `''` while the list route did not send the column — so emptying a box
    // that held a pattern compared `'' === ''`, returned early, and left the
    // column set while the input showed empty. O92's readable half is what
    // lets this be staged the way an admin actually meets it: a root that
    // *holds* a pattern, emptied.
    const host = await settle({ exclude_globs: '**/extras/**' })
    await edit(host, 'exclude_globs', '')
    expect(config.patchRoot).toHaveBeenCalledWith(3, { exclude_globs: null })
  })

  it('picks up a scan that was already running when it mounted', async () => {
    // **Review finding 2.** `scanStatus` was reached only from the interval,
    // and the interval only existed once `begin()` had run — so an admin who
    // navigated away during a scan and came back saw no progress, no eventual
    // report, and an enabled "Scan now" that answers 409. The server keeps its
    // registry precisely so a client can poll across requests.
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: true,
      elapsed_secs: 12.4,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    const host = await settle()
    expect(config.scanStatus).toHaveBeenCalledWith(3)
    expect(host.textContent).toContain('Scanning')
    expect(host.textContent).toContain('12s')
  })

  it('says so when the server has no record of the scan it was watching', async () => {
    // **Review finding 3.** The registry is in-memory: a `serve` restarted
    // mid-scan answers `{running: false, report: null}`. Writing that through
    // unconditionally cleared "Scanning…" and drew nothing — a died-with-the-
    // process scan reading as a clean finish.
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: true,
        elapsed_secs: 1,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValue({
        root_id: 3,
        running: false,
        elapsed_secs: 2,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no record',
    )
  })

  it('does not show the previous scan elapsed time when a new one starts', async () => {
    // **Review finding 4.**
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      elapsed_secs: 47,
      report: { error: 'done' },
      last_scan_at: null,
      last_scan_status: null,
    })
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: true,
      elapsed_secs: 0.1,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).not.toContain('47s')
  })

  it('sends one POST for a double-pressed Scan now', async () => {
    // **Review finding 5.** `disabled={running}` and `running` was set in the
    // `.then`, so two rapid presses both sent — and the second's 409, about
    // the admin's own scan, landed in the alert slot while that scan ran fine.
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(config.startScan).mock.calls).toHaveLength(1)
  })

  it('refuses an unparseable interval rather than clearing the column', async () => {
    // **Review finding 8.** For `<input type=number>` a browser reports an
    // unparseable entry as `value === ''`, so `12e` blurred sent
    // `{scan_interval_secs: null}` and wiped an interval nobody meant to touch.
    const host = await settle()
    const box = host.querySelector(
      'input[name="scan_interval_secs"]',
    ) as HTMLInputElement
    Object.defineProperty(box, 'validity', {
      configurable: true,
      value: { badInput: true },
    })
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, '')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
    })
    expect(config.patchRoot).not.toHaveBeenCalled()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'whole number',
    )
  })

  it('refuses to store an interval of zero or less', async () => {
    const host = await settle()
    await edit(host, 'scan_interval_secs', '0')
    expect(config.patchRoot).not.toHaveBeenCalled()
    expect(host.querySelector('[role="alert"]')).not.toBeNull()
  })

  // --- whole-branch review pass 3, 2026-09-04 ----------------------------

  it('keeps a scan warning when an unrelated field is saved', async () => {
    // **Pass 3, finding 1 — the same single-slot defect this branch already
    // fixed one level up in `Roots.tsx`, left standing here.** A scan
    // interrupted by a restart leaves "no record of that scan" on the card;
    // editing any field then succeeded and wiped the only notice saying the
    // library may be half-derived.
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: false,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValue({
        root_id: 3,
        running: false,
        elapsed_secs: 2,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('no record')

    await edit(host, 'default_language', 'ja')
    expect(config.patchRoot).toHaveBeenCalled()
    expect(host.textContent).toContain('no record')
  })

  it('starts watching when a 409 says a scan is already running', async () => {
    // **Pass 3, finding 2.** A 409 is positive proof a scan is in flight, and
    // `begin`'s catch only alerted — so the card stayed blind: no progress, no
    // report, no reload when it finished, and the same 409 on every retry.
    // That is verbatim the state `misses`' own doc comment exists to prevent.
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 a scan of root 3 (/srv/manga) is already running',
      ),
    )
    // **The mount read must say "not running", or this passes for the wrong
    // reason**: the card would already be watching before the 409 fired, and
    // the assertion below would hold with `begin`'s catch doing nothing at
    // all. Caught by the test passing on the unfixed component.
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: false,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockResolvedValue({
        root_id: 3,
        running: true,
        elapsed_secs: 8,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
    const host = await settle()
    expect(host.textContent).not.toContain('Scanning')
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Scanning')
    expect(press(host, 'Scan now').disabled).toBe(true)
  })

  // --- whole-branch review pass 4, 2026-09-04 ----------------------------

  it('does not wipe a write-only column just because the box was focused', async () => {
    // **Pass 4, finding 2 — a regression pass 3 introduced.** Making "never
    // sent this mount" mean *unknown* skipped the equality guard entirely, so
    // `save(field, '')` fired on a bare focus-and-leave and `patch_nullable_
    // text` wrote SQL NULL. Tabbing through the form deleted a stored
    // `exclude_globs` pattern, with nothing typed and nothing said. Value
    // equality cannot express this; whether the admin actually edited the box
    // can — and it is still the rule now that the column *is* readable,
    // because an edit is the fact both wrong models were reaching for. Staged
    // on a root that holds a pattern, which is what there was to lose.
    const host = await settle({ exclude_globs: '**/extras/**' })
    await tabThrough(host, 'exclude_globs')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('does not write an abandoned edit when the box is tabbed through again', async () => {
    // **The `edited` guard's only witness, and the review of O92's own fix
    // round is what found it missing.** Staging every tab-through on a box
    // whose displayed value equals `field.from(root)` lets the blur fall
    // through to the *equality* guard and return there, so all three
    // tab-through tests kept passing with `edited` deleted outright — 47
    // files, 927 tests, green. Before the row could report `exclude_globs`
    // its `from` was `undefined`, and that is what had been holding this up.
    //
    // A dirty box is the case only `edited` can see. The `PATCH` is refused,
    // so the box keeps the typed value while `root` still holds the old one;
    // the admin gives up and tabs out. Nothing was edited *this* focus, so
    // nothing is sent — where value comparison would write the abandoned
    // pattern, which is the pass-4 shape on a different trigger.
    const host = await settle({ exclude_globs: '**/extras/**' })
    vi.mocked(config.patchRoot).mockRejectedValue(new Error('502 Bad Gateway'))
    await edit(host, 'exclude_globs', '**/tmp/**')
    expect(config.patchRoot).toHaveBeenCalledWith(3, {
      exclude_globs: '**/tmp/**',
    })

    vi.clearAllMocks()
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    await tabThrough(host, 'exclude_globs')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('tabbing through every box writes nothing at all', async () => {
    const host = await settle()
    for (const name of [
      'edition_key',
      'default_kind',
      'default_language',
      'exclude_globs',
      'scan_interval_secs',
    ]) {
      await tabThrough(host, name)
    }
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends nothing when exclude_globs is typed back to what the row holds', async () => {
    // **The half of O92's fix that a starting value alone does not prove.**
    // Painting the box from `root.exclude_globs` while leaving `from` at
    // `undefined` would pass the "shows the pattern" test above and still
    // treat every blur as unknown — the pass-4 regression, which wrote SQL
    // NULL over a stored pattern on a bare focus-and-leave. The guard has to
    // read the same column the box does, and this is what says so.
    //
    // **`retype`, not `edit`.** The first version of this test used `edit`
    // and passed with the equality guard deleted outright; see `retype`'s own
    // doc comment for why. An admin appending a character to a stored pattern
    // and deleting it again is the interaction this covers, and it is the
    // only one the second guard can see.
    const host = await settle({ exclude_globs: '**/extras/**' })
    await retype(host, 'exclude_globs', '**/extras/**x', '**/extras/**')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends nothing when a readable field is typed back to its stored value', async () => {
    // Pre-existing, and it had the same hole as the `exclude_globs` test
    // above: driven by `edit` it never reached the equality guard at all.
    const host = await settle()
    await retype(host, 'edition_key', 'enx', 'en')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('follows the stored keeps_documents when the row changes underneath it', async () => {
    // **Pass 4, finding 4.** The state was seeded once and never resynced, so
    // a value changed from anywhere but this card left the tick asserting the
    // opposite of what migration 012 holds — and the next click then sent the
    // value the server already had, taking two clicks to get back.
    let mounted: HTMLElement | undefined
    await act(async () => {
      mounted = mount(<RootRow root={root()} onChanged={vi.fn()} />)
      await Promise.resolve()
    })
    if (!mounted) throw new Error('nothing mounted')
    const host = mounted
    const tick = () =>
      (host.querySelector('input[name="keeps_documents"]') as HTMLInputElement)
        .checked

    expect(tick()).toBe(false)
    await act(async () => {
      rerender(
        <RootRow root={root({ keeps_documents: true })} onChanged={vi.fn()} />,
      )
      await Promise.resolve()
    })
    expect(tick()).toBe(true)
  })

  it('reads the status from the error, not from a 409 anywhere in the text', async () => {
    // **Pass 4, finding 5.** `why.includes('409')` matched the substring
    // anywhere — so removing root 409 from another tab and pressing Scan now
    // gave a 404 whose body reads "no root with id 409", and the card entered
    // a fake running state and settled on a message about a restart.
    vi.mocked(config.startScan).mockRejectedValue(
      new Error('/api/library/roots/409/scan: 404 there is no root 409'),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan now').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).not.toContain('Scanning')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'there is no root 409',
    )
  })

  it('draws exclude_globs as a textarea holding every line', async () => {
    const host = await settle({ exclude_globs: '**/extras/**\n*.nfo' })

    const box = host.querySelector('textarea[name="exclude_globs"]')
    expect(
      box,
      'the column is multi-line and its box must be too',
    ).not.toBeNull()
    expect((box as HTMLTextAreaElement).value).toBe('**/extras/**\n*.nfo')
    expect(
      host.querySelector('input[name="exclude_globs"]'),
      'and it is no longer a single-line input',
    ).toBeNull()
  })

  it('patches a two-line exclude_globs with its newline intact', async () => {
    vi.mocked(config.patchRoot).mockResolvedValue(undefined)
    const host = await settle({ exclude_globs: null })

    await edit(host, 'exclude_globs', 'extras\n*.nfo')

    expect(config.patchRoot).toHaveBeenCalledWith(3, {
      exclude_globs: 'extras\n*.nfo',
    })
  })

  it('no longer says exclude_globs is unapplied', async () => {
    const host = await settle({ exclude_globs: null })

    const label = host
      .querySelector('textarea[name="exclude_globs"]')
      ?.closest('label')
      ?.querySelector('span')?.textContent
    expect(label).toBe('Exclude globs (one gitignore pattern per line)')
  })

  // --- O92: the directory-vs-contents trap, 2026-09-09 -------------------

  it('hints that a trailing slash or bare name skips the whole directory', async () => {
    // **The trap this line exists to name.** Measured against the `ignore`
    // crate, 2026-09-09: `**/extras/**` excludes the files under an `extras`
    // directory but never the directory itself, so a scan still descends
    // into it, still records it as a scan entry, and reports "Directories
    // excluded: 0" — which reads to an admin as "my pattern did nothing"
    // when gitignore did exactly what it was asked. Both shapes have to be
    // named for the hint to be useful, so this checks both rather than just
    // that a `<small>` exists.
    const host = await settle({ exclude_globs: null })
    const hint = host
      .querySelector('textarea[name="exclude_globs"]')
      ?.closest('label')
      ?.querySelector('small')?.textContent
    expect(hint).toContain('trailing slash or bare name')
    expect(hint).toContain('/**')
  })

  it('gives the other three text fields no hint', async () => {
    // Per field, not a page-level count: a page-level assertion cannot see
    // which field a hint landed on, and this repository has been bitten by
    // exactly that shape of test before.
    const host = await settle()
    for (const name of ['edition_key', 'default_kind', 'default_language']) {
      const hint = host
        .querySelector(`input[name="${name}"]`)
        ?.closest('label')
        ?.querySelector('small')
      expect(hint, `${name} should carry no hint`).toBeNull()
    }
  })
})
