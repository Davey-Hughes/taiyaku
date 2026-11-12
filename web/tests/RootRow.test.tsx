import { act, StrictMode } from 'react'
import { MemoryRouter } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from '../src/config'
import * as api from '../src/libraries'
import { RootRow } from '../src/RootRow'
import { emptyDerive } from './support/derive'
import { deferred, flush } from './support/async'
import './support/dialog'
import { holdOf } from './support/held'
import { mount, rerender } from './support/mount'
import { keptGrid } from './support/reads'
import { helpOf, row, section } from './support/settingsRows'

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
  unlinkCost: vi.fn(),
  unlinkDir: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
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
  next_scan_at: null,
  ...over,
})

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
 * asserts a *negative*, so a helper that fired nothing would leave it green
 * while checking nothing. The native value setter is the same fix
 * `Library.test.tsx` and `Account.test.tsx` carry, and for the same reason:
 * React's per-instance value tracker swallows a plain assignment.
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
 * the equality guard it claims to be about. The value has to genuinely move
 * and come back for the second guard to be the one that stops the write.
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
    removed: {
      works: 412,
      positions: 3,
      notes: 27,
      left_with_no_library: [],
      libraries_removed: [],
      kept: [],
    },
    derive: emptyDerive,
    dry_run: true,
  })
  vi.mocked(config.removeRoot).mockResolvedValue({
    removed: {
      works: 412,
      positions: 3,
      notes: 27,
      left_with_no_library: [],
      libraries_removed: [],
      kept: [],
    },
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
  it('states each fact under its own name', async () => {
    const host = await settle()
    const pairs = [...host.querySelectorAll('.regime > div')].map((row) => [
      row.querySelector('dt')?.textContent,
      row.querySelector('dd')?.textContent,
    ])
    // Asserted per pair, not with a whole-element `toContain`: a page-wide
    // string cannot see a label attached to the wrong value. Covers both a
    // root that has never been scanned (drawn as `never`, not a null) and
    // one this card was not handed a probe for (drawn as `unknown`, not
    // defaulted to a claim either way).
    expect(pairs).toEqual([
      ['watch', 'unknown — probed when a directory is added'],
      ['scan every', '24h'],
      ['last scan', 'never'],
      ['next scan', 'due'],
    ])
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
        '/api/library/roots/3/scan: 409 a scan of /srv/manga is already running',
      ),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already running',
    )
  })

  it('draws the library its removal’s dry run says goes with the directory', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 412,
        positions: 3,
        notes: 27,
        left_with_no_library: [],
        libraries_removed: ['Manga'],
        kept: [],
      },
      derive: emptyDerive,
      dry_run: true,
    })
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.last-dir')?.textContent).toBe(
      'This is the only directory of the library Manga, so removing it removes the library too.',
    )
  })

  it('hands focus back to Remove… when its confirm is cancelled', async () => {
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    const cancel = [...host.querySelectorAll('.remove-root button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    act(() => cancel.focus())
    await act(async () => {
      cancel.click()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(document.activeElement).toBe(press(host, 'Remove…'))
  })

  it('hands focus to Remove…, held, when a hold cancels its confirm while focus is in it', async () => {
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    const cancel = [...host.querySelectorAll('.remove-root button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    act(() => cancel.focus())
    await act(async () => {
      rerender(<RootRow root={root()} held onChanged={vi.fn()} />)
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(document.activeElement).toBe(press(host, 'Remove…'))
    expect(holdOf(press(host, 'Remove…'))).toBe('held')
  })

  it('leaves focus in a box elsewhere on the page when a hold cancels its confirm', async () => {
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    const box = host.querySelector(
      'input[name="edition_key"]',
    ) as HTMLInputElement
    act(() => box.focus())
    await act(async () => {
      rerender(<RootRow root={root()} held onChanged={vi.fn()} />)
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(document.activeElement).toBe(box)
  })

  it('draws no library sentence in the removal dialog where its dry run empties none', async () => {
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).not.toBeNull()
    expect(host.querySelector('.last-dir')).toBeNull()
  })

  it('closes an open removal dialog when its own mount read finds a scan running', async () => {
    let answer: (v: config.ScanStatus) => void = () => {}
    vi.mocked(config.scanStatus).mockReturnValue(
      new Promise((resolve) => {
        answer = resolve
      }),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root .confirm')).not.toBeNull()
    act(() =>
      (host.querySelector('.remove-root .confirm') as HTMLElement).focus(),
    )
    await act(async () => {
      answer({
        root_id: 3,
        running: true,
        elapsed_secs: 7,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.remove-root')).toBeNull()
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 7s')
    expect(config.removeRoot).not.toHaveBeenCalled()
    // Focus was in the confirm the scan took away: it goes back to Remove…,
    // which the scan holds.
    expect(document.activeElement).toBe(press(host, 'Remove…'))
    expect(holdOf(press(host, 'Remove…'))).toBe('held')
  })

  it('is held while its library’s status says it is being scanned: Scan directory and Remove… are held and send nothing', async () => {
    const host = await settle()
    await act(async () => {
      rerender(<RootRow root={root()} held onChanged={vi.fn()} />)
      await Promise.resolve()
    })
    for (const label of ['Scan directory', 'Remove…']) {
      expect(holdOf(press(host, label)), label).toBe('held')
      await act(async () => {
        press(host, label).click()
        await Promise.resolve()
      })
    }
    expect(config.startScan).not.toHaveBeenCalled()
    // The removal asks what it would cost as it opens. The row's hold closes
    // a removal it finds open, so the dialog's absence alone cannot show
    // that Remove… did nothing.
    expect(config.removalCost).not.toHaveBeenCalled()
    expect(host.querySelector('.remove-root')).toBeNull()
  })

  it('says after Scanning… that its scan waits while its status reads waiting, and nothing more once it reads deriving (O51)', async () => {
    const scanning = (elapsed_secs: number, last_scan_status: string) => ({
      root_id: 3,
      running: true,
      elapsed_secs,
      report: null,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status,
    })
    vi.mocked(config.scanStatus).mockResolvedValue(scanning(12, 'waiting'))
    const host = await settle()
    expect(host.querySelector('.scanning')?.textContent).toBe(
      'Scanning… 12s · waiting',
    )

    vi.mocked(config.scanStatus).mockResolvedValue(scanning(13, 'deriving'))
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 13s')
  })

  it('says no waiting after Scanning… for a scan pressed once the watch of a waiting one was given up (O51)', async () => {
    vi.mocked(config.scanStatus).mockResolvedValueOnce({
      root_id: 3,
      running: true,
      elapsed_secs: 12,
      report: null,
      last_scan_at: '2026-11-10 12:00:00',
      last_scan_status: 'waiting',
    })
    const host = await settle()
    expect(host.querySelector('.scanning')?.textContent).toBe(
      'Scanning… 12s · waiting',
    )
    vi.mocked(config.scanStatus).mockRejectedValue(
      new Error('/api/library/roots/3/scan: 500 the store is locked'),
    )
    for (const _ of [1, 2, 3]) {
      await act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
    }
    expect(host.querySelector('.scanning')).toBeNull()

    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning…')
  })

  describe('Move…', () => {
    const library = { id: 5, name: 'Chapters' }
    const moveTo = [{ id: 7, name: 'Manga' }]
    // The rehearsal and the move answer differently: what is handed up can
    // only be the move's.
    const answered = (dry_run: boolean): api.MoveAnswer => ({
      moved: {
        library_id: 7,
        library: 'Manga',
        gains_sight: dry_run ? [] : ['kim'],
        loses_sight: [],
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      dry_run,
    })
    const mounted = async (
      props: Partial<Parameters<typeof RootRow>[0]> = {},
    ) => {
      let host: HTMLElement | undefined
      await act(async () => {
        host = mount(
          <RootRow
            root={root()}
            library={library}
            moveTo={moveTo}
            onChanged={vi.fn()}
            {...props}
          />,
        )
        await Promise.resolve()
        await Promise.resolve()
      })
      if (!host) throw new Error('nothing mounted')
      return host
    }
    const choose = async (host: HTMLElement, value: string) => {
      await act(async () => {
        const box = host.querySelector(
          'select[name="move-target"]',
        ) as HTMLSelectElement
        Object.getOwnPropertyDescriptor(
          HTMLSelectElement.prototype,
          'value',
        )?.set?.call(box, value)
        box.dispatchEvent(new Event('change', { bubbles: true }))
      })
    }

    it('is offered only where the row was told where to, and holds Scan directory and Remove… while open', async () => {
      const bare = await settle()
      expect(
        [...bare.querySelectorAll('button')].some(
          (b) => b.textContent === 'Move…',
        ),
      ).toBe(false)

      const host = await mounted()
      await act(async () => {
        press(host, 'Move…').click()
        await Promise.resolve()
      })
      const dialog = host.querySelector('dialog') as HTMLDialogElement
      expect(dialog.open).toBe(true)
      expect(
        document.getElementById(dialog.getAttribute('aria-labelledby') ?? '')
          ?.textContent,
      ).toBe('Move /srv/manga')
      // Move… by its own press, the others out of the tab order: focus was
      // on Move… as the dialog opened.
      expect(
        ['Scan directory', 'Remove…', 'Move…'].map((label) =>
          holdOf(press(host, label)),
        ),
      ).toEqual(['disabled', 'disabled', 'held'])
    })

    it('hands focus back to Move… when its dialog is cancelled, by Cancel and by the browser', async () => {
      const host = await mounted()
      const move = press(host, 'Move…')
      const cancel = () =>
        [
          ...(host.querySelector('dialog') as HTMLElement).querySelectorAll(
            'button',
          ),
        ].find((b) => b.textContent === 'Cancel') as HTMLButtonElement
      await act(async () => {
        move.click()
        await Promise.resolve()
      })
      await act(async () => {
        cancel().click()
        await Promise.resolve()
      })
      expect(host.querySelector('dialog')).toBeNull()
      expect(document.activeElement).toBe(move)

      ;(host.querySelector('input[name="edition_key"]') as HTMLElement).focus()
      await act(async () => {
        move.click()
        await Promise.resolve()
      })
      await act(async () => {
        host
          .querySelector('dialog')
          ?.dispatchEvent(new Event('cancel', { cancelable: true }))
        await Promise.resolve()
      })
      expect(host.querySelector('dialog')).toBeNull()
      expect(document.activeElement).toBe(move)
    })

    it('holds Move… while a removal is being confirmed, and opens nothing from it', async () => {
      const host = await mounted()
      await act(async () => {
        press(host, 'Remove…').click()
        await Promise.resolve()
      })
      expect(holdOf(press(host, 'Move…'))).toBe('disabled')
      await act(async () => {
        press(host, 'Move…').click()
        await Promise.resolve()
      })
      expect(host.querySelector('dialog')).toBeNull()
    })

    it('closes its dialog when a scan the mount read finds begins', async () => {
      const status = deferred<config.ScanStatus>()
      vi.mocked(config.scanStatus).mockReturnValue(status.promise)
      const host = await mounted()
      await act(async () => {
        press(host, 'Move…').click()
        await Promise.resolve()
      })
      expect(host.querySelector('.move-root')).not.toBeNull()
      // Where the browser puts focus as the modal opens.
      act(() =>
        (host.querySelector('.move-root select') as HTMLElement).focus(),
      )
      await flush(() =>
        status.resolve({
          root_id: 3,
          running: true,
          elapsed_secs: 1,
          report: null,
          last_scan_at: null,
          last_scan_status: null,
        }),
      )
      expect(host.querySelector('.move-root')).toBeNull()
      expect(document.activeElement).toBe(press(host, 'Move…'))
      expect(holdOf(press(host, 'Move…'))).toBe('held')
    })

    it('closes its dialog when the row becomes held', async () => {
      const host = await mounted()
      await act(async () => {
        press(host, 'Move…').click()
        await Promise.resolve()
      })
      expect(host.querySelector('.move-root')).not.toBeNull()
      await act(async () => {
        rerender(
          <RootRow
            root={root()}
            library={library}
            moveTo={moveTo}
            held
            onChanged={vi.fn()}
          />,
        )
        await Promise.resolve()
      })
      expect(host.querySelector('.move-root')).toBeNull()
    })

    it('reloads once the directory has moved', async () => {
      vi.mocked(api.moveCost).mockResolvedValue(answered(true))
      vi.mocked(api.moveDir).mockResolvedValue(answered(false))
      const onChanged = vi.fn()
      const onMoved = vi.fn()
      const host = await mounted({ onChanged, onMoved })
      await act(async () => {
        press(host, 'Move…').click()
        await Promise.resolve()
      })
      await choose(host, '7')
      await act(async () => {
        press(host, 'See what this would do').click()
        await Promise.resolve()
        await Promise.resolve()
      })
      await act(async () => {
        ;(host.querySelector('.move-root .confirm') as HTMLElement).click()
        await Promise.resolve()
        await Promise.resolve()
      })
      expect(api.moveDir).toHaveBeenCalledWith(3, 5, { library: 7 })
      expect(onChanged).toHaveBeenCalledTimes(1)
      expect(onMoved).toHaveBeenCalledWith(answered(false))
      expect(host.querySelector('.move-root')).toBeNull()
    })
  })

  it('hands its removal’s answer up, and draws no report of its own', async () => {
    const onRemoved = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RootRow root={root()} onRemoved={onRemoved} onChanged={vi.fn()} />,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    await act(async () => {
      press(host as HTMLElement, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      host?.querySelector<HTMLElement>('.remove-root .confirm')?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(onRemoved).toHaveBeenCalledWith(
      expect.objectContaining({ dry_run: false }),
    )
    expect(host.querySelector('.removal-report')).toBeNull()
  })

  it('keeps a 409’s sentence only until the scan it named has ended', async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 {"error":"a scan of /srv/manga is already running"}',
      ),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'a scan of /srv/manga is already running',
    )
    expect(host.querySelector('.scanning')).not.toBeNull()

    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: { error: 'the scan panicked' },
      last_scan_at: '2026-09-04 12:00:00',
      last_scan_status: 'failed',
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.scanning')).toBeNull()
    expect(host.textContent).not.toContain('is already running')
  })

  it('asks for the list again, quietly, when its scan ends', async () => {
    const changed = vi.fn()
    const host = await settle({}, changed)
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.scanning')).toBeNull()
    expect(changed).toHaveBeenCalledTimes(1)
    expect(changed).toHaveBeenCalledWith(true)
  })

  // What a scan found changes what the Library views draw; one the server
  // has no record of may have written some of it before a restart.
  it.each([
    [
      'reports what it found',
      {
        scan: {
          root: '/srv/manga',
          dirs: 1,
          files: 2,
          added: 2,
          updated: 0,
          gone: 0,
          refused: null,
          unreadable_dirs: [],
          excluded_dirs: [],
          excluded_files: 0,
          bad_exclude_patterns: [],
          forgotten_by_exclusion: 0,
          taken_over: 0,
        },
        derive: emptyDerive,
      } as config.ScanOutcome,
      false,
    ],
    ['the server has no record of', null, true],
  ] as const)(
    'marks the Library views stale when its scan ends, as one that %s',
    async (_, report, noRecord) => {
      vi.mocked(config.scanStatus).mockResolvedValue({
        root_id: 3,
        running: false,
        elapsed_secs: 2,
        report,
        last_scan_at: null,
        last_scan_status: null,
      })
      const grid = await keptGrid()
      const host = await settle()
      await act(async () => {
        press(host, 'Scan directory').click()
        await Promise.resolve()
        await Promise.resolve()
      })
      expect(host.querySelector('.scanning')).not.toBeNull()
      expect(grid.stale()).toBe(false)
      await act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
      expect(host.querySelector('.scanning')).toBeNull()
      // Which of the two ends the poll took.
      expect(
        [...host.querySelectorAll('[role="alert"]')].some((p) =>
          p.textContent?.includes('no record'),
        ),
      ).toBe(noRecord)
      expect(grid.stale()).toBe(true)
      expect(grid.sent()).toBe(1)
    },
  )

  it('keeps a 409’s sentence through a poll that finds the scan still running', async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 {"error":"a scan of /srv/manga is already running"}',
      ),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    const poll = async () => {
      await act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
    }
    vi.mocked(config.scanStatus).mockResolvedValueOnce({
      root_id: 3,
      running: true,
      elapsed_secs: 4,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    await poll()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'a scan of /srv/manga is already running',
    )
    expect(host.querySelector('.scanning')).not.toBeNull()

    await poll()
    expect(host.querySelector('.scanning')).toBeNull()
    expect(host.textContent).not.toContain('is already running')
  })

  it('drops a 409’s sentence when its poll is abandoned, since nothing can keep it true', async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 {"error":"a scan of /srv/manga is already running"}',
      ),
    )
    const host = await settle()
    vi.mocked(config.scanStatus).mockRejectedValue(
      new Error('/api/library/roots/3/scan: 502 bad gateway'),
    )
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    for (let i = 0; i < 3; i++) {
      await act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
    }
    expect(
      [...host.querySelectorAll('[role="alert"]')].map((a) => a.textContent),
    ).toEqual(['bad gateway'])
    expect(holdOf(press(host, 'Scan directory'))).toBe('free')
  })

  it('draws the sentence of the refusal that ends a run of failed polls', async () => {
    vi.mocked(config.scanStatus)
      .mockResolvedValueOnce({
        root_id: 3,
        running: false,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      })
      .mockRejectedValue(
        new Error(
          '/api/library/roots/3/scan: 500 {"error":"the registry is unreadable"}',
        ),
      )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    for (let i = 0; i < 3; i++) {
      await act(async () => {
        vi.advanceTimersByTime(config.SCAN_POLL_MS)
        await Promise.resolve()
        await Promise.resolve()
      })
    }
    expect(host.querySelector('.scan-notice')?.textContent).toBe(
      'the registry is unreadable',
    )
  })

  it("draws a refusal's sentence, not the response it came in", async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 {"error":"a scan of /srv/manga is already running"}',
      ),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'a scan of /srv/manga is already running',
    )
    // The 409 is still read from the message, so the row still polls.
    expect(host.querySelector('.scanning')).not.toBeNull()
  })

  it('polls while a scan runs and stops when it finishes', async () => {
    vi.mocked(config.scanStatus)
      // **The mount read takes the first answer.** The card asks the status
      // route once when it mounts, so a queued `…Once` chain that does not
      // account for it has the poll reading the answer meant for the mount.
      // Every test in this file that queues answers has to respect that shape.
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
        last_scan_at: '2026-09-04 12:00:00',
        last_scan_status: 'failed',
      })

    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
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
      last_scan_at: '2026-09-04 12:00:00',
      last_scan_status: 'failed',
    })
    const host = await settle({}, onChanged)
    await act(async () => {
      press(host, 'Scan directory').click()
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
        "/api/library/roots/3: 400 the directory's `edition_key` must be a string or null",
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

  it('keeps watching a scan through a single failed poll', async () => {
    // A `.catch` that did `setRunning(false)` would let one blip — a proxy
    // 502, a `SQLITE_BUSY` while the scan's own writes hold the file — tear
    // the interval down for good: no progress, no eventual report, and a
    // re-enabled "Scan directory" whose only answer is a 409 about the scan
    // still running. That is precisely the state this file's mount-read exists
    // to prevent, reached by a different door.
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
      press(host, 'Scan directory').click()
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
      press(host, 'Scan directory').click()
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
    // `RemoveRoot`'s dry run is fetched once, on `[root.id]`, and never
    // refetched — so a scan started from the same card while the dialog is open
    // would import works underneath a sentence that is still quoting the old
    // counts, and the confirm below it would then send a real DELETE into a
    // running derive.
    const host = await settle()
    await act(async () => {
      press(host, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(holdOf(press(host, 'Scan directory'))).toBe('disabled')
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
    })
    expect(config.startScan).not.toHaveBeenCalled()
  })

  it('puts the checkbox back when its save is refused', async () => {
    // An uncontrolled checkbox cannot be reset by a reloaded prop, so a refused
    // PATCH would leave the box asserting the opposite of what migration 012
    // holds for that root — and the only way back would be to click it twice.
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
          taken_over: 0,
        },
        derive: {
          series: 1,
          works: 5,
          editions: 5,
          members: 5,
          unattached_chapter_dirs: 0,
          unreadable: {
            total: 0,
            refused_format: 0,
            by_format: {},
            uncounted: [],
            vanished: [],
            no_pages: 0,
          },
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
          marked_missing: 0,
          found_again: 0,
        },
      },
      last_scan_at: '2026-09-04 12:00:00',
      last_scan_status: 'ok',
    })
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    // After the settings and before the removal box, which stays the page's
    // last box.
    expect(
      [
        ...host.querySelectorAll(
          '.settings-section > .settings-section-head > .settings-section-title',
        ),
      ].map((title) => title.textContent),
    ).toEqual(['How it is scanned', 'How it is read', 'Scan report', 'Removal'])
    const report = section(host, 'Scan report')
    expect(report.querySelector('.scan-report')).not.toBeNull()
    expect(report.textContent).toContain(
      'What derivation made of every library',
    )
    // The report's two headings sit one level beneath its section's h3,
    // which sits beneath the directory's h2.
    expect(report.querySelector('.settings-section-title')?.tagName).toBe('H3')
    expect(
      [...report.querySelectorAll('.scan-report .report-heading')].map(
        (h) => h.tagName,
      ),
    ).toEqual(['H4', 'H4'])
    // The regime line stays in the header beside it.
    expect(
      host.querySelectorAll('.page-header .page-description .regime > div'),
    ).toHaveLength(4)
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
    const watchRow = [...(host?.querySelectorAll('.regime > div') ?? [])].find(
      (row) => row.querySelector('dt')?.textContent === 'watch',
    )
    expect(watchRow?.querySelector('dd')?.textContent).toBe(
      'probe failed: inotify limit reached',
    )
  })

  it('shows the exclude_globs pattern the row holds', async () => {
    // **O92's readable half.** `GET /api/library/roots` sends the column, so
    // the box starts at what is stored instead of empty.
    const host = await settle({ exclude_globs: '**/extras/**' })
    const box = host.querySelector(
      'textarea[name="exclude_globs"]',
    ) as HTMLTextAreaElement
    expect(box.value).toBe('**/extras/**')
  })

  it('patches exclude_globs, the column the scanner now applies', async () => {
    // The column is read back, patched here, and applied by
    // `scan::scan_root`'s walk (O92). This pins that the box is wired to the
    // patch route at all, which is separate from the test above that pins
    // what it reads.
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

  it('clears a stored exclude_globs pattern', async () => {
    // The blur guard compares against `field.from(root)`. Were that `''`,
    // emptying a box that held a pattern would compare `'' === ''`, return
    // early, and leave the column set while the input showed empty. This
    // stages it the way an admin actually meets it: a root that *holds* a
    // pattern, emptied (O92's readable half).
    const host = await settle({ exclude_globs: '**/extras/**' })
    await edit(host, 'exclude_globs', '')
    expect(config.patchRoot).toHaveBeenCalledWith(3, { exclude_globs: null })
  })

  it('picks up a scan that was already running when it mounted', async () => {
    // Without the mount read, `scanStatus` is reached only from the interval,
    // and the interval only exists once `begin()` has run — so an admin who
    // navigated away during a scan and came back would see no progress, no
    // eventual report, and an enabled "Scan directory" that answers 409. The
    // server keeps its registry precisely so a client can poll across requests.
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
    // The registry is in-memory: a `serve` restarted mid-scan answers
    // `{running: false, report: null}`. Writing that through unconditionally
    // would clear "Scanning…" and draw nothing — a died-with-the-process scan
    // reading as a clean finish.
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
      press(host, 'Scan directory').click()
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
      press(host, 'Scan directory').click()
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
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).not.toContain('47s')
  })

  it('sends one POST for a double-pressed Scan directory', async () => {
    // `disabled={running}` with `running` set in the `.then` would let two
    // rapid presses both send — and the second's 409, about the admin's own
    // scan, would land in the alert slot while that scan ran fine.
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(config.startScan).mock.calls).toHaveLength(1)
  })

  it('keeps a scan warning when an unrelated field is saved', async () => {
    // **The single-slot defect `Libraries.tsx` also avoids, by keeping its
    // list failure and its create refusal apart.** A scan interrupted by a
    // restart leaves "no record of that scan" on the card; editing any field
    // then succeeding would wipe the only notice saying the library may
    // be half-derived.
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
      press(host, 'Scan directory').click()
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
    // A 409 is positive proof a scan is in flight. If `begin`'s catch only
    // alerted, the card would stay blind: no progress, no report, no reload
    // when it finished, and the same 409 on every retry. That is verbatim the
    // state `misses`' own doc comment exists to prevent.
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 a scan of /srv/manga is already running',
      ),
    )
    // **The mount read must say "not running", or this passes for the wrong
    // reason**: the card would already be watching before the 409 fired, and
    // the assertion below would hold with `begin`'s catch doing nothing at all.
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
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Scanning')
    expect(holdOf(press(host, 'Scan directory'))).toBe('held')
  })

  it('does not wipe a write-only column just because the box was focused', async () => {
    // Treating "never sent this mount" as *unknown* would skip the equality
    // guard entirely, so a bare focus-and-leave would save `''` and
    // `patch_nullable_text` would write SQL NULL. Tabbing through the form
    // would delete a stored `exclude_globs` pattern, with nothing typed and
    // nothing said. Value equality cannot express this; whether the admin
    // actually edited the box can. Staged on a root that holds a pattern,
    // which is what there is to lose.
    const host = await settle({ exclude_globs: '**/extras/**' })
    await tabThrough(host, 'exclude_globs')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('does not write an abandoned edit when the box is tabbed through again', async () => {
    // **The `edited` guard's only witness.** Staging every tab-through on a box
    // whose displayed value equals `field.from(root)` lets the blur fall
    // through to the *equality* guard and return there, so the tab-through
    // tests would keep passing with `edited` deleted outright.
    //
    // A dirty box is the case only `edited` can see. The `PATCH` is refused,
    // so the box keeps the typed value while `root` still holds the old one;
    // the admin gives up and tabs out. Nothing was edited *this* focus, so
    // nothing is sent — where value comparison would write the abandoned
    // pattern, which is the shape of the test above on a different trigger.
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
      'scan_every',
    ]) {
      await tabThrough(host, name)
    }
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends nothing when exclude_globs is typed back to what the row holds', async () => {
    // **The half of O92's fix that a starting value alone does not prove.**
    // Painting the box from `root.exclude_globs` while leaving `from` at
    // `undefined` would pass the "shows the pattern" test above and still
    // treat every blur as unknown. The guard has to read the same column the
    // box does, and this is what says so.
    //
    // **`retype`, not `edit`.** An `edit` would pass with the equality guard
    // deleted outright; see `retype`'s own doc comment for why. An admin
    // appending a character to a stored pattern and deleting it again is the
    // interaction this covers, and it is the only one the second guard can see.
    const host = await settle({ exclude_globs: '**/extras/**' })
    await retype(host, 'exclude_globs', '**/extras/**x', '**/extras/**')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends nothing when a readable field is typed back to its stored value', async () => {
    // Like the `exclude_globs` test above, this needs `retype`: driven by
    // `edit` it would never reach the equality guard at all.
    const host = await settle()
    await retype(host, 'edition_key', 'enx', 'en')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('follows the stored keeps_documents when the row changes underneath it', async () => {
    // State seeded once and never resynced would leave the tick, after a value
    // changed from anywhere but this card, asserting the opposite of what
    // migration 012 holds — and the next click would then send the value the
    // server already has, taking two clicks to get back.
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
    // `why.includes('409')` would match the substring anywhere — so removing
    // root 409 from another tab and pressing Scan directory would give a 404
    // whose body reads "there is no directory 409", and the card would enter
    // a fake running state and settle on a message about a restart.
    vi.mocked(config.startScan).mockRejectedValue(
      new Error('/api/library/roots/409/scan: 404 there is no directory 409'),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Scan directory').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).not.toContain('Scanning')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'there is no directory 409',
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

    const globs = row(host, 'Exclude globs')
    const box = globs.querySelector('textarea[name="exclude_globs"]')
    expect(box?.getAttribute('aria-label')).toBe(
      'Exclude globs (one gitignore pattern per line)',
    )
    // Neither the words beside the box nor its name say it is only kept.
    expect(globs.textContent).not.toMatch(/not yet applied|recorded/i)
  })

  // --- O92: the directory-vs-contents trap --------------------------------

  it('hints that a trailing slash or bare name skips the whole directory', async () => {
    // **The trap this line exists to name.** `**/extras/**` excludes the files
    // under an `extras` directory but never the directory itself, so a scan
    // still descends into it, still records it as a scan entry, and reports
    // "Directories excluded: 0" — which reads to an admin as "my pattern did
    // nothing" when gitignore did exactly what it was asked. Both shapes have
    // to be named for the hint to be useful, so this checks both rather than
    // just that the row has help.
    const host = await settle({ exclude_globs: null })
    const hint = helpOf(row(host, 'Exclude globs'))?.textContent
    expect(hint).toContain(
      'A trailing slash or a bare name excludes the directory and all under it',
    )
    expect(hint).toContain("a '/**' suffix excludes only what is inside it")
  })

  it('gives the other three text fields no hint', async () => {
    // Per field, not a page-level count: a page-level assertion cannot see
    // which field a hint landed on.
    const host = await settle()
    for (const label of ['Edition key', 'Default kind', 'Default language']) {
      expect(
        helpOf(row(host, label)),
        `${label} should carry no hint`,
      ).toBeNull()
    }
  })

  it('draws the keeps-documents checkbox as a check row, its box kept beside its words', async () => {
    // A checkbox's label is a sentence, not a field name, and a box alone on
    // a line under its words reads as a different control: its row is the
    // one check row, which keeps the box beside the words at every width.
    const host = await settle()
    const checks = [...host.querySelectorAll('.settings-row-check')]
    expect(checks).toEqual([row(host, 'Keeps documents under .taiyaku/')])
    expect(
      checks[0]?.querySelector(
        'input[type="checkbox"][name="keeps_documents"]',
      ),
    ).not.toBeNull()
  })

  it('draws each setting as a row of its section: how it is scanned, then how it is read', async () => {
    const host = await settle()
    // Per section and per row, by the row's label and its control's name, so
    // a setting drawn in the wrong section or under another's label is seen.
    const rowsOf = (title: string) =>
      [...section(host, title).querySelectorAll('.settings-row')].map((r) => [
        r.querySelector('.settings-row-label')?.textContent,
        r.querySelector('input, textarea')?.getAttribute('name'),
      ])
    expect(rowsOf('How it is scanned')).toEqual([
      ['Scan schedule', 'scan_schedule-3'],
      ['Exclude globs', 'exclude_globs'],
      ['Keeps documents under .taiyaku/', 'keeps_documents'],
    ])
    expect(rowsOf('How it is read')).toEqual([
      ['Edition key', 'edition_key'],
      ['Default kind', 'default_kind'],
      ['Default language', 'default_language'],
    ])
  })

  it('names Exclude globs by its label alone and describes it by its hint — O165', async () => {
    const host = await settle()
    // `row` finds the row by its label's words alone, so a hint drawn
    // inside the label would lose it the row.
    const globs = row(host, 'Exclude globs')
    const box = globs.querySelector('textarea[name="exclude_globs"]')
    if (!box) throw new Error('no exclude_globs box')
    // What each attribute points at, by id. `getElementById` rather than a
    // `#` selector: `useId`'s ids are not valid CSS identifiers.
    const text = (attr: string) => {
      const id = box.getAttribute(attr)
      return id === null ? null : document.getElementById(id)?.textContent
    }
    // O175: the name is the box's own `aria-label`. An `aria-labelledby`
    // would outrank it, so there has to be none.
    expect(box.getAttribute('aria-label')).toBe(
      'Exclude globs (one gitignore pattern per line)',
    )
    expect(box.hasAttribute('aria-labelledby')).toBe(false)
    expect(text('aria-describedby')).toBe(helpOf(globs)?.textContent)
    expect(text('aria-describedby')).toBe(
      'One gitignore pattern per line. A trailing slash or a bare name ' +
        "excludes the directory and all under it; a '/**' suffix excludes " +
        'only what is inside it.',
    )
  })

  it('names every text box by its label alone, so a later hint cannot join a name — O165', async () => {
    const host = await settle()
    const named = ['edition_key', 'default_kind', 'default_language'].map(
      (name) => {
        const box = host.querySelector(`input[name="${name}"]`)
        return [
          name,
          box?.getAttribute('aria-label'),
          box?.hasAttribute('aria-labelledby'),
          box?.hasAttribute('aria-describedby'),
        ]
      },
    )
    expect(named).toEqual([
      ['edition_key', 'Edition key', false, false],
      ['default_kind', 'Default kind', false, false],
      ['default_language', 'Default language', false, false],
    ])
  })

  it('gives two roots’ boxes ids of their own — O165', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <>
          <RootRow root={root()} probe={undefined} onChanged={vi.fn()} />
          <RootRow
            root={root({ id: 4, path: '/srv/books' })}
            probe={undefined}
            onChanged={vi.fn()}
          />
        </>,
      )
      await Promise.resolve()
    })
    // The hint's id: it is the one id a box points at (O175).
    const ids = [
      ...(host?.querySelectorAll('textarea[name="exclude_globs"]') ?? []),
    ].map((box) => box.getAttribute('aria-describedby'))
    expect(ids).toHaveLength(2)
    expect(new Set(ids).size).toBe(2)
  })

  it('gives each control under a row label that label’s words as its aria-label — O175', async () => {
    const host = await settle()
    const labelled = [
      ...host.querySelectorAll<HTMLLabelElement>('label.settings-row-label'),
    ]
      .map((label) => ({ written: label.textContent, control: label.control }))
      .filter(({ control }) => control?.hasAttribute('aria-label'))
    // The four text columns, named: a loop over nothing passes. The Scan
    // schedule's label names its radio group, which no control's label
    // does. One name says more than its row's label, the syntax, and each
    // starts with the label's words as written.
    expect(
      labelled.map(({ written, control }) => [
        written,
        control?.getAttribute('aria-label'),
      ]),
    ).toEqual([
      ['Exclude globs', 'Exclude globs (one gitignore pattern per line)'],
      ['Edition key', 'Edition key'],
      ['Default kind', 'Default kind'],
      ['Default language', 'Default language'],
    ])
    for (const { written, control } of labelled) {
      expect(
        control?.getAttribute('aria-label')?.startsWith(written ?? '-'),
        written ?? '',
      ).toBe(true)
    }
  })
})

/** The Scan schedule's three radios, in the order drawn. */
const scheduleRadios = (host: HTMLElement) => [
  ...host.querySelectorAll<HTMLInputElement>(
    'input[type="radio"][name^="scan_schedule-"]',
  ),
]

/** The words of the Scan schedule's radio that is checked. */
const chosen = (host: HTMLElement) =>
  scheduleRadios(host)
    .find((radio) => radio.checked)
    ?.closest('label')?.textContent

/** Every's count, drawn only while Every is chosen. */
const everyBox = (host: HTMLElement) =>
  host.querySelector<HTMLInputElement>('input[name="scan_every"]')

/** Every's unit, drawn only while Every is chosen. */
const everyUnit = (host: HTMLElement) =>
  host.querySelector<HTMLSelectElement>('select[name="scan_every_unit"]')

/**
 * Click the Scan schedule's radio labelled `words` with `HTMLElement.click()`,
 * which moves no focus, so a box being typed in is not left first as a
 * mouse's click would leave it.
 */
async function choose(host: HTMLElement, words: string) {
  const radio = scheduleRadios(host).find(
    (one) => one.closest('label')?.textContent === words,
  )
  if (!radio) throw new Error(`no radio labelled ${words}`)
  await act(async () => {
    radio.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** Pick Every's unit from its list, touching nothing else. */
async function pickUnit(host: HTMLElement, unit: 'minutes' | 'hours') {
  const select = everyUnit(host)
  if (!select) throw new Error('no unit drawn')
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set?.call(select, unit)
    select.dispatchEvent(new Event('change', { bubbles: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** The row read again, its interval `scan_interval_secs`, as its page does. */
async function readAgain(scan_interval_secs: number | null) {
  await act(async () => {
    rerender(
      <RootRow
        root={root({ scan_interval_secs })}
        probe={undefined}
        onChanged={vi.fn()}
      />,
    )
    await Promise.resolve()
  })
}

/** The Scan schedule as drawn: its choice, and Every's count and unit. */
const scheduleOf = (host: HTMLElement) => [
  chosen(host),
  everyBox(host)?.value,
  everyUnit(host)?.value,
]

/** Make Every's box report an entry it cannot read, as `12e` is. */
function unreadable(host: HTMLElement) {
  const box = everyBox(host)
  if (!box) throw new Error('no box drawn')
  Object.defineProperty(box, 'validity', {
    configurable: true,
    value: { badInput: true },
  })
}

/** The row's alert, where it says something. */
const alertOf = (host: HTMLElement) =>
  host.querySelector('[role="alert"]')?.textContent

/** The sentence a count the page will not send gets, written out. */
const UNDER_MINIMUM =
  'Every has to be a whole number of minutes or hours, at least 5 minutes.'

describe('RootRow, the Scan schedule', () => {
  it('draws a row of three radios, Every’s count and its unit after Every’s label, named and helped as written', async () => {
    const host = await settle()
    const schedule = row(section(host, 'How it is scanned'), 'Scan schedule')
    const help = helpOf(schedule)
    expect(help?.textContent).toBe(
      'Every is at least 5 minutes. With Off, the server scans it only when Scan directory or Scan library is pressed.',
    )
    expect(schedule.querySelector('[role="radiogroup"]')).not.toBeNull()
    expect(
      scheduleRadios(host).map((radio) => [
        radio.closest('label')?.textContent,
        radio.name,
      ]),
    ).toEqual([
      ['Automatic', 'scan_schedule-3'],
      ['Every', 'scan_schedule-3'],
      ['Off', 'scan_schedule-3'],
    ])

    const box = everyBox(host)
    const unit = everyUnit(host)
    if (!box || !unit || !help) throw new Error('no box, unit or help drawn')
    expect(
      ['type', 'min', 'step', 'aria-label', 'aria-describedby'].map((name) =>
        box.getAttribute(name),
      ),
    ).toEqual(['number', '1', '1', 'Scan every', help.id])
    expect(
      ['aria-label', 'aria-describedby'].map((name) => unit.getAttribute(name)),
    ).toEqual(['Scan every unit', help.id])
    expect([...unit.options].map((one) => one.textContent)).toEqual([
      'minutes',
      'hours',
    ])
    // After Every's label and outside it, then Off's: a control inside a
    // label is that label's too.
    const [, every, off] = scheduleRadios(host).map(
      (radio) => radio.closest('label') as HTMLLabelElement,
    )
    expect(box.closest('label')).toBeNull()
    expect(unit.closest('label')).toBeNull()
    const order = [every, box, unit, off]
    for (let i = 1; i < order.length; i++) {
      expect(
        order[i - 1].compareDocumentPosition(order[i]) &
          Node.DOCUMENT_POSITION_FOLLOWING,
        String(i),
      ).toBeTruthy()
    }
  })

  it('draws the stored value as its choice, a count in hours where it is whole hours and in minutes rounded down otherwise, and sends nothing', async () => {
    const drawn = async (scan_interval_secs: number | null) => {
      const host = await settle({ scan_interval_secs })
      return [chosen(host), everyBox(host)?.value, everyUnit(host)?.value]
    }
    expect(await drawn(null)).toEqual(['Automatic', undefined, undefined])
    expect(await drawn(0)).toEqual(['Off', undefined, undefined])
    expect(await drawn(-60)).toEqual(['Off', undefined, undefined])
    expect(await drawn(86400)).toEqual(['Every', '24', 'hours'])
    expect(await drawn(900)).toEqual(['Every', '15', 'minutes'])
    expect(await drawn(5400)).toEqual(['Every', '90', 'minutes'])
    expect(await drawn(330)).toEqual(['Every', '5', 'minutes'])
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('saves Off as 0 and Automatic as null at once, and draws no count for either', async () => {
    const host = await settle()
    await choose(host, 'Off')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
    ])
    expect(chosen(host)).toBe('Off')
    expect(everyBox(host)).toBeNull()

    await choose(host, 'Automatic')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: null }],
    ])
    expect(chosen(host)).toBe('Automatic')
    expect(everyBox(host)).toBeNull()
  })

  it('saves 24 hours at once for choosing Every with no interval kept, and draws it in hours, from Automatic and from Off', async () => {
    // A directory each, so that each row's radios are a group of their own.
    for (const [id, scan_interval_secs] of [
      [3, null],
      [4, 0],
      [5, -3600],
    ] as const) {
      const host = await settle({ id, scan_interval_secs })
      await choose(host, 'Every')
      expect(scheduleOf(host), String(scan_interval_secs)).toEqual([
        'Every',
        '24',
        'hours',
      ])
    }
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 86400 }],
      [4, { scan_interval_secs: 86400 }],
      [5, { scan_interval_secs: 86400 }],
    ])
  })

  it('sends Every as seconds when its box is left, in minutes, and in hours when its unit is changed', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    await edit(host, 'scan_every', '20')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 1200 }],
    ])

    vi.mocked(config.patchRoot).mockClear()
    await typeAndStay(host, 'scan_every', '2')
    await pickUnit(host, 'hours')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 7200 }],
    ])
  })

  it('sends nothing for a count under 5 minutes, not whole or unreadable, says why, and sends 5 minutes', async () => {
    const refused = async (
      entry: string,
      over: Partial<config.Root> = { scan_interval_secs: 900 },
      then?: (host: HTMLElement) => void,
    ) => {
      vi.mocked(config.patchRoot).mockClear()
      const host = await settle(over)
      then?.(host)
      await edit(host, 'scan_every', entry)
      return [vi.mocked(config.patchRoot).mock.calls.length, alertOf(host)]
    }
    for (const entry of ['4', '0', '-5', '1.5']) {
      expect(await refused(entry), entry).toEqual([0, UNDER_MINIMUM])
    }
    // A count of hours that is not whole is refused, though it is whole
    // seconds.
    expect(await refused('1.5', { scan_interval_secs: 86400 })).toEqual([
      0,
      UNDER_MINIMUM,
    ])
    expect(await refused('', {}, unreadable)).toEqual([0, UNDER_MINIMUM])

    vi.mocked(config.patchRoot).mockClear()
    const host = await settle({ scan_interval_secs: 900 })
    await edit(host, 'scan_every', '4')
    expect(alertOf(host)).toBe(UNDER_MINIMUM)
    await edit(host, 'scan_every', '5')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 300 }],
    ])
    expect(alertOf(host)).toBeUndefined()
  })

  it('sends nothing and says nothing for an emptied box, nor for one typed back to the stored value', async () => {
    const emptied = await settle({ id: 4, scan_interval_secs: 900 })
    await retype(emptied, 'scan_every', '7', '')
    // `retype`, not `edit`: see its doc comment.
    const stored = await settle({ scan_interval_secs: 900 })
    await retype(stored, 'scan_every', '150', '15')
    expect(config.patchRoot).not.toHaveBeenCalled()
    expect(alertOf(emptied)).toBeUndefined()
    expect(alertOf(stored)).toBeUndefined()
  })

  it('sends nothing and says nothing for a box tabbed through untouched, though its count is the stored value rounded down', async () => {
    // 330 seconds is drawn as 5 minutes, and 90 as 1, so only what was
    // typed can tell a send from a tab through.
    const five = await settle({ scan_interval_secs: 330 })
    await tabThrough(five, 'scan_every')
    const one = await settle({ scan_interval_secs: 90 })
    await tabThrough(one, 'scan_every')
    expect(config.patchRoot).not.toHaveBeenCalled()
    expect(alertOf(five)).toBeUndefined()
    expect(alertOf(one)).toBeUndefined()
  })

  it('draws the server’s refusal in its own words', async () => {
    vi.mocked(config.patchRoot).mockRejectedValue(
      new Error(
        '/api/library/roots/3: 400 {"error":"the directory’s scan interval is refused here"}',
      ),
    )
    const host = await settle({ scan_interval_secs: 900 })
    await edit(host, 'scan_every', '10')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 600 }],
    ])
    expect(alertOf(host)).toBe('the directory’s scan interval is refused here')
  })

  it('goes back to the stored choice when Off or Automatic is refused, and draws the server’s words', async () => {
    vi.mocked(config.patchRoot).mockRejectedValue(
      new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
    )
    const every = await settle()
    await choose(every, 'Off')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
    ])
    expect(chosen(every)).toBe('Every')
    expect(everyBox(every)?.value).toBe('24')
    expect(alertOf(every)).toBe('the store is locked')

    const off = await settle({ scan_interval_secs: 0 })
    await choose(off, 'Automatic')
    expect(chosen(off)).toBe('Off')
  })

  it('sends a choice once the one before it is answered, and keeps it when that one is refused', async () => {
    const first = deferred<undefined>()
    const second = deferred<undefined>()
    // In the order sent: Off, Default language, then Automatic.
    vi.mocked(config.patchRoot)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce(undefined)
      .mockReturnValueOnce(second.promise)
    const host = await settle()
    await choose(host, 'Off')
    await choose(host, 'Automatic')
    // Another setting's write is another column's, and does not wait.
    await edit(host, 'default_language', 'ja')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { default_language: 'ja' }],
    ])

    await flush(() =>
      first.reject(
        new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
      ),
    )
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls.slice(2)).toEqual([
      [3, { scan_interval_secs: null }],
    ])
    expect(alertOf(host)).toBe('the store is locked')
    expect(chosen(host)).toBe('Automatic')

    await flush(() => second.resolve(undefined))
    expect(alertOf(host)).toBeUndefined()
    expect(chosen(host)).toBe('Automatic')
  })

  it('names its radios for its directory, so that two rows in one page keep their own choices', async () => {
    const every = await settle()
    const off = await settle({ id: 4, scan_interval_secs: 0 })
    expect([chosen(every), chosen(off)]).toEqual(['Every', 'Off'])
  })

  it('saves the stored interval again at once, its count refilled, when Every is chosen after this row’s Off, read back or not', async () => {
    const sent = await settle()
    await choose(sent, 'Off')
    await choose(sent, 'Every')
    await flush()
    expect(scheduleOf(sent)).toEqual(['Every', '24', 'hours'])
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 86400 }],
    ])
    await unmountRow()
    vi.mocked(config.patchRoot).mockClear()
    const readBack = await settle({ scan_interval_secs: 420 })
    await choose(readBack, 'Off')
    await readAgain(0)
    await choose(readBack, 'Every')
    await flush()
    expect(scheduleOf(readBack)).toEqual(['Every', '7', 'minutes'])
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 420 }],
    ])
  })

  it('saves again the count it last sent, typed or by its unit, or the interval a read brings from elsewhere, when Every is chosen again', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    await edit(host, 'scan_every', '10')
    await choose(host, 'Off')
    await choose(host, 'Every')
    await flush()
    expect(scheduleOf(host)).toEqual(['Every', '10', 'minutes'])

    await typeAndStay(host, 'scan_every', '3')
    await pickUnit(host, 'hours')
    await choose(host, 'Automatic')
    await choose(host, 'Every')
    await flush()
    expect(scheduleOf(host)).toEqual(['Every', '3', 'hours'])

    await readAgain(5400)
    await choose(host, 'Off')
    await choose(host, 'Every')
    await flush()
    expect(scheduleOf(host)).toEqual(['Every', '90', 'minutes'])
    expect(
      vi
        .mocked(config.patchRoot)
        .mock.calls.map(([, patch]) => patch.scan_interval_secs),
    ).toEqual([600, 0, 600, 10800, null, 10800, 0, 5400])
  })

  it('goes back to Off when Every’s refilled interval is refused, and draws the server’s words', async () => {
    vi.mocked(config.patchRoot)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(
        new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
      )
    const host = await settle()
    await choose(host, 'Off')
    await choose(host, 'Every')
    await flush()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 86400 }],
    ])
    expect(scheduleOf(host)).toEqual(['Off', undefined, undefined])
    expect(alertOf(host)).toBe('the store is locked')
  })

  it('saves again the interval stored, not a count refused, when Every is chosen after Off', async () => {
    vi.mocked(config.patchRoot).mockRejectedValueOnce(
      new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
    )
    const host = await settle()
    await edit(host, 'scan_every', '12')
    await flush()
    expect(alertOf(host)).toBe('the store is locked')
    await choose(host, 'Off')
    await choose(host, 'Every')
    await flush()
    expect(scheduleOf(host)).toEqual(['Every', '24', 'hours'])
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 43200 }],
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 86400 }],
    ])
  })

  it('sends nothing as the row goes for a count typed while Every’s refill is out, once that is refused, and draws Off again', async () => {
    const refill = deferred<undefined>()
    vi.mocked(config.patchRoot)
      .mockResolvedValueOnce(undefined)
      .mockReturnValueOnce(refill.promise)
    const host = await settle({ scan_interval_secs: 420 })
    await choose(host, 'Off')
    await flush()
    await choose(host, 'Every')
    await typeAndStay(host, 'scan_every', '12')
    await flush(() =>
      refill.reject(
        new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
      ),
    )
    await flush()
    expect(scheduleOf(host)).toEqual(['Off', undefined, undefined])
    await unmountRow()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 420 }],
    ])
  })

  it('sends nothing as the row goes for a count typed while Every’s 24 hours is out, once that is refused, and draws Automatic again', async () => {
    const LOCKED = new Error(
      '/api/library/roots/3: 500 {"error":"the store is locked"}',
    )
    const day = deferred<undefined>()
    vi.mocked(config.patchRoot).mockReturnValueOnce(day.promise)
    const automatic = await settle({ scan_interval_secs: null })
    await choose(automatic, 'Every')
    await typeAndStay(automatic, 'scan_every', '12')
    await flush(() => day.reject(LOCKED))
    await flush()
    expect(scheduleOf(automatic)).toEqual(['Automatic', undefined, undefined])
    await unmountRow()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 86400 }],
    ])
  })

  it('goes back to Automatic when its 24 hours and a count left after it are both refused, and keeps a refused count where the store holds Every', async () => {
    const LOCKED = new Error(
      '/api/library/roots/3: 500 {"error":"the store is locked"}',
    )
    const day = deferred<undefined>()
    vi.mocked(config.patchRoot)
      .mockReturnValueOnce(day.promise)
      .mockRejectedValueOnce(LOCKED)
    const host = await settle({ scan_interval_secs: null })
    await choose(host, 'Every')
    await edit(host, 'scan_every', '3')
    await flush(() => day.reject(LOCKED))
    await flush()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 86400 }],
      [3, { scan_interval_secs: 10800 }],
    ])
    expect(scheduleOf(host)).toEqual(['Automatic', undefined, undefined])
    expect(alertOf(host)).toBe('the store is locked')
    await unmountRow()

    vi.mocked(config.patchRoot).mockClear()
    vi.mocked(config.patchRoot).mockRejectedValueOnce(LOCKED)
    const every = await settle({ scan_interval_secs: 900 })
    await edit(every, 'scan_every', '10')
    await flush()
    expect(scheduleOf(every)).toEqual(['Every', '10', 'minutes'])
    expect(alertOf(every)).toBe('the store is locked')
  })

  it('sends nothing as the row goes for a count typed after the 24 hours is refused, once the count before it is refused too', async () => {
    const LOCKED = new Error(
      '/api/library/roots/3: 500 {"error":"the store is locked"}',
    )
    const day = deferred<undefined>()
    const count = deferred<undefined>()
    vi.mocked(config.patchRoot)
      .mockReturnValueOnce(day.promise)
      .mockReturnValueOnce(count.promise)
    const host = await settle({ scan_interval_secs: null })
    await choose(host, 'Every')
    await edit(host, 'scan_every', '3')
    await flush(() => day.reject(LOCKED))
    await flush()
    expect(scheduleOf(host)).toEqual(['Every', '3', 'hours'])
    await typeAndStay(host, 'scan_every', '5')
    await flush(() => count.reject(LOCKED))
    await flush()
    expect(scheduleOf(host)).toEqual(['Automatic', undefined, undefined])
    await unmountRow()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 86400 }],
      [3, { scan_interval_secs: 10800 }],
    ])
  })

  it('saves again the last interval the store took when Every is chosen after counts refused, one or two in turn, or after its refill is refused', async () => {
    const LOCKED = new Error(
      '/api/library/roots/3: 500 {"error":"the store is locked"}',
    )
    const intervals = () =>
      vi
        .mocked(config.patchRoot)
        .mock.calls.map(([, patch]) => patch.scan_interval_secs)
    const first = deferred<undefined>()
    vi.mocked(config.patchRoot)
      .mockReturnValueOnce(first.promise)
      .mockRejectedValueOnce(LOCKED)
    const both = await settle({ scan_interval_secs: 900 })
    await edit(both, 'scan_every', '10')
    await edit(both, 'scan_every', '20')
    await flush(() => first.reject(LOCKED))
    await flush()
    await choose(both, 'Off')
    await flush()
    await choose(both, 'Every')
    await flush()
    expect(intervals()).toEqual([600, 1200, 0, 900])
    expect(scheduleOf(both)).toEqual(['Every', '15', 'minutes'])
    await unmountRow()

    vi.mocked(config.patchRoot).mockClear()
    vi.mocked(config.patchRoot)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(LOCKED)
    const second = await settle({ scan_interval_secs: 900 })
    await edit(second, 'scan_every', '10')
    await flush()
    await edit(second, 'scan_every', '20')
    await flush()
    await choose(second, 'Off')
    await flush()
    await choose(second, 'Every')
    await flush()
    expect(intervals()).toEqual([600, 1200, 0, 600])
    expect(scheduleOf(second)).toEqual(['Every', '10', 'minutes'])
    await unmountRow()

    // Off taken, then Every's refill refused: Off is not an interval kept.
    vi.mocked(config.patchRoot).mockClear()
    vi.mocked(config.patchRoot)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(LOCKED)
    const refill = await settle({ scan_interval_secs: 900 })
    await choose(refill, 'Off')
    await flush()
    await choose(refill, 'Every')
    await flush()
    await flush()
    expect(scheduleOf(refill)).toEqual(['Off', undefined, undefined])
    await choose(refill, 'Every')
    await flush()
    expect(intervals()).toEqual([0, 900, 900])
    expect(scheduleOf(refill)).toEqual(['Every', '15', 'minutes'])
  })

  it('saves 5 minutes when Every is chosen after Off where the store held less, as the scheduler reads it', async () => {
    const host = await settle({ scan_interval_secs: 60 })
    expect(scheduleOf(host)).toEqual(['Every', '1', 'minutes'])
    await choose(host, 'Off')
    await flush()
    await choose(host, 'Every')
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 300 }],
    ])
    expect(scheduleOf(host)).toEqual(['Every', '5', 'minutes'])
  })

  it('takes the stored choice when the row changes underneath', async () => {
    const host = await settle()
    await readAgain(0)
    expect(chosen(host)).toBe('Off')
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('keeps Every and a count typed since when the row is read again with this row’s own Off', async () => {
    const off = deferred<undefined>()
    vi.mocked(config.patchRoot).mockReturnValueOnce(off.promise)
    const host = await settle({ scan_interval_secs: null })
    await choose(host, 'Off')
    await choose(host, 'Every')
    await typeAndStay(host, 'scan_every', '12')
    await flush(() => off.resolve(undefined))
    await readAgain(0)
    expect(scheduleOf(host)).toEqual(['Every', '12', 'hours'])
    // Still typed in, so sent as the row goes.
    await unmountRow()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 86400 }],
      [3, { scan_interval_secs: 43200 }],
    ])
  })

  it('draws no count over Off when a read brings back the count this row wrote before it', async () => {
    // A press on Off leaves the box first, so the count and Off go in turn,
    // and the page reads the row after each answer: the first read can
    // bring back the count while Off is still on its way.
    const host = await settle()
    await edit(host, 'scan_every', '12')
    await choose(host, 'Off')
    await readAgain(43200)
    expect(scheduleOf(host)).toEqual(['Off', undefined, undefined])
    await readAgain(0)
    expect(scheduleOf(host)).toEqual(['Off', undefined, undefined])
  })

  it('goes back to the count this row wrote when Off after it is refused, and keeps it when that count is read back', async () => {
    vi.mocked(config.patchRoot)
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(
        new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
      )
    const host = await settle()
    await edit(host, 'scan_every', '12')
    await choose(host, 'Off')
    await flush()
    expect(scheduleOf(host)).toEqual(['Every', '12', 'hours'])
    expect(alertOf(host)).toBe('the store is locked')
    await readAgain(43200)
    expect(scheduleOf(host)).toEqual(['Every', '12', 'hours'])
  })

  it('takes a change from elsewhere over a write of its own still out, and then the store’s word for that write', async () => {
    const off = deferred<undefined>()
    vi.mocked(config.patchRoot).mockReturnValueOnce(off.promise)
    const host = await settle()
    await choose(host, 'Off')
    await readAgain(900)
    expect(scheduleOf(host)).toEqual(['Every', '15', 'minutes'])
    await flush(() => off.resolve(undefined))
    await readAgain(0)
    expect(scheduleOf(host)).toEqual(['Off', undefined, undefined])
  })

  it('sends a count that equals the row as last read when this row has written another since', async () => {
    // The row still says 24 hours until 12 is read back; the store holds 12.
    const host = await settle()
    await edit(host, 'scan_every', '12')
    await flush()
    await edit(host, 'scan_every', '24')
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 43200 }],
      [3, { scan_interval_secs: 86400 }],
    ])
  })

  it('names the unit to agree with the count: one minute, one hour, and more in the plural', async () => {
    const words = (host: HTMLElement) =>
      [...(everyUnit(host)?.options ?? [])].map((one) => one.textContent)
    const hour = await settle({ scan_interval_secs: 3600 })
    expect(words(hour)).toEqual(['minute', 'hour'])
    await typeAndStay(hour, 'scan_every', '2')
    expect(words(hour)).toEqual(['minutes', 'hours'])
    await typeAndStay(hour, 'scan_every', '1')
    expect(words(hour)).toEqual(['minute', 'hour'])
    await typeAndStay(hour, 'scan_every', '')
    expect(words(hour)).toEqual(['minutes', 'hours'])
    await readAgain(60)
    expect(words(hour)).toEqual(['minute', 'hour'])
    expect(everyUnit(hour)?.value).toBe('minutes')
  })
})

describe('RootRow, a directory another library holds too', () => {
  const MANGA = { id: 7, name: 'Manga' }
  const KIDS = { id: 9, name: 'Kids' }
  const moveTo = [{ id: 8, name: 'Comics' }]
  /** The unlink's rehearsal, or its answer: Manga stays, nobody loses sight. */
  const unlinked = (dry_run: boolean): api.UnlinkAnswer => ({
    unlinked: {
      loses_sight: [],
      library_removed: null,
      grants_removed: 0,
      left_with_no_library: [],
    },
    dry_run,
  })
  /** The row as Manga's page of a directory Kids holds too draws it. */
  const shared = (props: Partial<Parameters<typeof RootRow>[0]> = {}) => (
    <MemoryRouter>
      <RootRow
        root={root()}
        library={MANGA}
        libraries={[MANGA, KIDS]}
        moveTo={moveTo}
        onChanged={vi.fn()}
        {...props}
      />
    </MemoryRouter>
  )
  const mounted = async (
    props: Partial<Parameters<typeof RootRow>[0]> = {},
  ) => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(shared(props))
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    return host
  }
  const redrawn = async (props: Partial<Parameters<typeof RootRow>[0]>) => {
    await act(async () => {
      rerender(shared(props))
      await Promise.resolve()
    })
  }
  const clicked = async (button: Element) => {
    await act(async () => {
      ;(button as HTMLElement).click()
      await Promise.resolve()
      await Promise.resolve()
    })
  }
  const unlink = (host: HTMLElement) => press(host, 'Remove from Manga…')
  /** Each of the four actions, held, disabled or free. */
  const holds = (host: HTMLElement) =>
    Object.fromEntries(
      ['Scan directory', 'Move…', 'Remove from Manga…', 'Remove…'].map(
        (label) => [label, holdOf(press(host, label))],
      ),
    )

  beforeEach(() => {
    vi.mocked(api.unlinkCost).mockResolvedValue(unlinked(true))
    vi.mocked(api.unlinkDir).mockResolvedValue(unlinked(false))
  })

  it('sends the page’s library with Scan directory', async () => {
    const host = await mounted()
    await clicked(press(host, 'Scan directory'))
    expect(config.startScan).toHaveBeenCalledWith(3, 7)
  })

  it('holds Remove from Manga… exactly when Move… is, and Remove… for a scan around or inside it', async () => {
    const host = await mounted()
    expect(holds(host)).toEqual({
      'Scan directory': 'free',
      'Move…': 'free',
      'Remove from Manga…': 'free',
      'Remove…': 'free',
    })
    await redrawn({ moveHeld: true })
    expect(holds(host)).toEqual({
      'Scan directory': 'free',
      'Move…': 'held',
      'Remove from Manga…': 'held',
      'Remove…': 'free',
    })
    await redrawn({ removalHeld: 'around' })
    expect(holds(host)).toEqual({
      'Scan directory': 'free',
      'Move…': 'free',
      'Remove from Manga…': 'free',
      'Remove…': 'held',
    })
    await redrawn({ held: true })
    expect(holds(host)).toEqual({
      'Scan directory': 'held',
      'Move…': 'held',
      'Remove from Manga…': 'held',
      'Remove…': 'held',
    })
  })

  it('holds the other actions by disabled while its dialog is open, and is itself held then', async () => {
    const host = await mounted()
    await clicked(unlink(host))
    expect(host.querySelector('.unlink-root')).not.toBeNull()
    expect(api.unlinkCost).toHaveBeenCalledWith(7, 3)
    expect(holds(host)).toEqual({
      'Scan directory': 'disabled',
      'Move…': 'disabled',
      'Remove from Manga…': 'held',
      'Remove…': 'disabled',
    })
    await clicked(press(host, 'Cancel'))
    await clicked(press(host, 'Remove…'))
    expect(holdOf(unlink(host))).toBe('disabled')
  })

  it('draws both dialogs after both rows, so that opening either moves no row', async () => {
    const host = await mounted()
    const box = section(host, 'Removal').querySelector(
      ':scope > .settings-box',
    ) as HTMLElement
    // The rows are a group's, the dialog the box's own, after it.
    const parts = () =>
      [...box.children].flatMap((one) =>
        one.classList.contains('settings-group')
          ? [...one.children].map(
              (row) => row.querySelector('.settings-row-label')?.textContent,
            )
          : [one.className],
      )
    await clicked(unlink(host))
    expect(parts()).toEqual([
      'Remove from Manga',
      'Remove this directory',
      'unlink-root',
    ])
    await clicked(press(host, 'Cancel'))
    await clicked(press(host, 'Remove…'))
    expect(parts()).toEqual([
      'Remove from Manga',
      'Remove this directory',
      'remove-root',
    ])
  })

  it('hands focus back to Remove from Manga… when its dialog is cancelled', async () => {
    const host = await mounted()
    await clicked(unlink(host))
    const cancel = press(host, 'Cancel')
    act(() => cancel.focus())
    await clicked(cancel)
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(document.activeElement).toBe(unlink(host))
  })

  it('closes its dialog when a hold of the move comes, and hands focus to Remove from Manga…, held', async () => {
    const host = await mounted()
    await clicked(unlink(host))
    act(() => press(host, 'Cancel').focus())
    await redrawn({ moveHeld: true })
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(document.activeElement).toBe(unlink(host))
    expect(holdOf(unlink(host))).toBe('held')
  })

  it('takes its dialog away, and the hold it put on the others, when a list read finds no other library holding the directory', async () => {
    const host = await mounted()
    await clicked(unlink(host))
    expect(holdOf(press(host, 'Remove…'))).toBe('disabled')
    await redrawn({ libraries: [MANGA] })
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(() => unlink(host)).toThrow()
    expect(holdOf(press(host, 'Remove…'))).toBe('free')
    expect(holdOf(press(host, 'Move…'))).toBe('free')
  })

  it('keeps its dialog through a hold of the removal alone', async () => {
    const host = await mounted()
    await clicked(unlink(host))
    await redrawn({ removalHeld: 'around' })
    expect(host.querySelector('.unlink-root')).not.toBeNull()
  })

  it('hands what the unlink did up and asks for the list again', async () => {
    const onUnlinked = vi.fn()
    const onChanged = vi.fn()
    const onRemoved = vi.fn()
    const host = await mounted({ onUnlinked, onChanged, onRemoved })
    await clicked(unlink(host))
    await clicked(host.querySelector('.unlink-root .confirm') as Element)
    expect(api.unlinkDir).toHaveBeenCalledWith(7, 3)
    expect(onUnlinked).toHaveBeenCalledWith(unlinked(false))
    expect(onChanged).toHaveBeenCalledTimes(1)
    expect(onRemoved).not.toHaveBeenCalled()
    expect(host.querySelector('.unlink-root')).toBeNull()
  })

  it('draws a refusal of its rehearsal in a callout after both rows, closes its dialog, and lets the next press take it away', async () => {
    const LAST =
      '`/srv/manga` is in no other library, so it can only be removed: use Remove this directory.'
    vi.mocked(api.unlinkCost).mockRejectedValue(
      new Error(
        `/api/library/libraries/7/dirs/3?dry_run=true: 400 ${JSON.stringify({ error: LAST })}`,
      ),
    )
    const host = await mounted()
    await clicked(unlink(host))
    expect(host.querySelector('.unlink-root')).toBeNull()
    // After the last row, from where it lies over the rows above it.
    const row = press(host, 'Remove…').closest('.settings-row') as HTMLElement
    const callout = row.nextElementSibling as HTMLElement
    expect(callout.className).toBe('callout')
    expect(callout.querySelector('[role="alert"]')?.textContent).toBe(LAST)
    // Focus went with the dialog, and is handed back beneath the refusal.
    expect(document.activeElement).toBe(unlink(host))
    expect(holdOf(press(host, 'Remove…'))).toBe('free')
    await clicked(press(host, 'Remove…'))
    expect(host.querySelector('.callout')).toBeNull()
  })

  it('draws a refusal of the unlink itself as it draws its rehearsal’s', async () => {
    vi.mocked(api.unlinkDir).mockRejectedValue(
      new Error(
        `/api/library/libraries/7/dirs/3: 409 ${JSON.stringify({ error: 'a scan of /srv/manga is reserved' })}`,
      ),
    )
    const onUnlinked = vi.fn()
    const host = await mounted({ onUnlinked })
    await clicked(unlink(host))
    const confirm = host.querySelector('.unlink-root .confirm') as HTMLElement
    act(() => confirm.focus())
    await clicked(confirm)
    expect(onUnlinked).not.toHaveBeenCalled()
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(host.querySelector('.callout [role="alert"]')?.textContent).toBe(
      'a scan of /srv/manga is reserved',
    )
    expect(document.activeElement).toBe(unlink(host))
  })

  /** Remove from Manga… pressed, and its rehearsal refused as the lock is held. */
  const refused = async (
    props: Partial<Parameters<typeof RootRow>[0]> = {},
  ) => {
    vi.mocked(api.unlinkCost).mockRejectedValue(
      new Error(
        `/api/library/libraries/7/dirs/3?dry_run=true: 409 ${JSON.stringify({ error: 'a scan or another library job is running; remove the directory from this library once it has finished' })}`,
      ),
    )
    const host = await mounted(props)
    await clicked(unlink(host))
    expect(host.querySelector('.callout [role="alert"]')).not.toBeNull()
    vi.mocked(api.unlinkCost).mockResolvedValue(unlinked(true))
    return host
  }

  it('takes its refusal away as focus leaves Remove from Manga…', async () => {
    const host = await refused()
    expect(document.activeElement).toBe(unlink(host))
    expect(host.querySelector('.callout')).not.toBeNull()
    act(() => press(host, 'Remove…').focus())
    expect(host.querySelector('.callout')).toBeNull()
  })

  it('takes its refusal away as the hold of it ends, so that a press reaches Remove from Manga… again', async () => {
    const host = await refused()
    await redrawn({ moveHeld: true })
    expect(holdOf(unlink(host))).toBe('held')
    expect(host.querySelector('.callout')).not.toBeNull()
    await redrawn({ moveHeld: false })
    expect(host.querySelector('.callout')).toBeNull()
    await clicked(unlink(host))
    expect(host.querySelector('.unlink-root .confirm')).not.toBeNull()
  })

  it('sends focus to Remove… when a list read takes its dialog away while focus is in it', async () => {
    const host = await mounted()
    await clicked(unlink(host))
    act(() => press(host, 'Cancel').focus())
    await redrawn({ libraries: [MANGA] })
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(document.activeElement).toBe(press(host, 'Remove…'))
  })

  it('sends focus to Remove… when a list read takes Remove from Manga… away while it has focus and no dialog is open', async () => {
    const host = await mounted()
    act(() => unlink(host).focus())
    await redrawn({ libraries: [MANGA] })
    expect(() => unlink(host)).toThrow()
    expect(document.activeElement).toBe(press(host, 'Remove…'))
  })

  it('takes no focus when a list read finds no other library holding the directory while focus is on the page', async () => {
    const host = await mounted()
    act(() => (document.activeElement as HTMLElement | null)?.blur())
    expect(document.activeElement).toBe(document.body)
    await redrawn({ libraries: [MANGA] })
    expect(() => unlink(host)).toThrow()
    expect(document.activeElement).toBe(document.body)
  })

  it('takes no focus when a list read finds no other library holding the directory once focus has left Remove from Manga… for the page', async () => {
    const host = await mounted()
    act(() => unlink(host).focus())
    const box = host.querySelector(
      'input[name="edition_key"]',
    ) as HTMLInputElement
    act(() => box.focus())
    act(() => box.blur())
    expect(document.activeElement).toBe(document.body)
    await redrawn({ libraries: [MANGA] })
    expect(document.activeElement).toBe(document.body)
  })

  it('takes its refusal away at a click on it and not before, where focus moved to a box while the rehearsal was out', async () => {
    const rehearsal = deferred<api.UnlinkAnswer>()
    vi.mocked(api.unlinkCost).mockReturnValue(rehearsal.promise)
    const host = await mounted()
    await clicked(unlink(host))
    const box = host.querySelector(
      'input[name="edition_key"]',
    ) as HTMLInputElement
    act(() => box.focus())
    await act(async () => {
      rehearsal.reject(
        new Error(
          `/api/library/libraries/7/dirs/3?dry_run=true: 400 ${JSON.stringify({ error: 'no' })}`,
        ),
      )
    })
    const said = host.querySelector('.callout [role="alert"]') as HTMLElement
    expect(said.textContent).toBe('no')
    expect(document.activeElement).toBe(box)
    await act(async () => {
      said.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
    })
    expect(host.querySelector('.callout [role="alert"]')).toBe(said)
    await clicked(said)
    expect(host.querySelector('.callout')).toBeNull()
    expect(host.querySelector('.unlink-root')).toBeNull()
  })

  it('tells its page of a refusal for a hold it was not told of, a 409, and of no other refusal', async () => {
    const onScanMoved = vi.fn()
    const onHeldRefusal = vi.fn()
    const host = await mounted({ onScanMoved, onHeldRefusal })
    vi.mocked(api.unlinkCost).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries/7/dirs/3?dry_run=true: 400 ${JSON.stringify({ error: 'last' })}`,
      ),
    )
    await clicked(unlink(host))
    expect(
      [onScanMoved, onHeldRefusal].map((f) => f.mock.calls.length),
    ).toEqual([0, 0])
    vi.mocked(api.unlinkCost).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries/7/dirs/3?dry_run=true: 409 ${JSON.stringify({ error: 'a scan or another library job is running; remove the directory from this library once it has finished' })}`,
      ),
    )
    await clicked(unlink(host))
    expect(
      [onScanMoved, onHeldRefusal].map((f) => f.mock.calls.length),
    ).toEqual([1, 1])
    vi.mocked(api.unlinkCost).mockResolvedValue(unlinked(true))
    vi.mocked(api.unlinkDir).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries/7/dirs/3: 409 ${JSON.stringify({ error: 'a scan of /srv/manga is reserved' })}`,
      ),
    )
    await clicked(unlink(host))
    await clicked(host.querySelector('.unlink-root .confirm') as Element)
    expect(
      [onScanMoved, onHeldRefusal].map((f) => f.mock.calls.length),
    ).toEqual([2, 2])
  })

  it('tells its page of a refusal in Move…’s dialog or Remove…’s for a hold, a 409, and of no other refusal', async () => {
    const onScanMoved = vi.fn()
    const onHeldRefusal = vi.fn()
    const host = await mounted({ onScanMoved, onHeldRefusal })
    const told = () =>
      [onScanMoved, onHeldRefusal].map((f) => f.mock.calls.length)
    const rehearseMove = async () => {
      await act(async () => {
        const box = host.querySelector(
          'select[name="move-target"]',
        ) as HTMLSelectElement
        Object.getOwnPropertyDescriptor(
          HTMLSelectElement.prototype,
          'value',
        )?.set?.call(box, '8')
        box.dispatchEvent(new Event('change', { bubbles: true }))
      })
      await clicked(
        host.querySelector('button[name="rehearse"]') as HTMLElement,
      )
    }
    const cancel = () =>
      clicked(
        [...host.querySelectorAll('.remove-root button')].find(
          (b) => b.textContent === 'Cancel',
        ) as HTMLElement,
      )

    vi.mocked(api.moveCost).mockRejectedValueOnce(
      new Error(
        `/api/library/roots/3/move?dry_run=true: 400 ${JSON.stringify({ error: 'no' })}`,
      ),
    )
    await clicked(press(host, 'Move…'))
    await rehearseMove()
    expect(told()).toEqual([0, 0])
    vi.mocked(api.moveCost).mockRejectedValueOnce(
      new Error(
        `/api/library/roots/3/move?dry_run=true: 409 ${JSON.stringify({ error: 'a scan of /srv/manga is running; move the directory once it has finished', volumes: [] })}`,
      ),
    )
    await rehearseMove()
    expect(told()).toEqual([1, 1])
    await clicked(press(host, 'Cancel'))

    vi.mocked(config.removalCost).mockRejectedValueOnce(
      new Error(
        `/api/library/roots/3?dry_run=true: 500 ${JSON.stringify({ error: 'the disk is full' })}`,
      ),
    )
    await clicked(press(host, 'Remove…'))
    expect(told()).toEqual([1, 1])
    await cancel()
    vi.mocked(config.removalCost).mockRejectedValueOnce(
      new Error(
        `/api/library/roots/3?dry_run=true: 409 ${JSON.stringify({ error: 'a scan of /srv/manga is running; remove the directory once it has finished' })}`,
      ),
    )
    await clicked(press(host, 'Remove…'))
    expect(told()).toEqual([2, 2])
  })

  /** One poll's interval, and what it started settles. */
  const polled = () =>
    act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
  const RUNNING: config.ScanStatus = {
    root_id: 3,
    running: true,
    elapsed_secs: 2,
    report: null,
    last_scan_at: null,
    last_scan_status: null,
  }
  /** Remove from Manga…'s rehearsal refused for a hold, and its own read answered. */
  const refusedForItsScan = async (host: HTMLElement) => {
    vi.mocked(config.scanStatus).mockResolvedValue(RUNNING)
    vi.mocked(api.unlinkCost).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries/7/dirs/3?dry_run=true: 409 ${JSON.stringify({ error: 'a scan or another library job is running; remove the directory from this library once it has finished' })}`,
      ),
    )
    await clicked(unlink(host))
    await flush()
  }

  it('closes its dialog when a scan the mount read finds begins, and hands focus to Remove from Manga…, held', async () => {
    const status = deferred<config.ScanStatus>()
    vi.mocked(config.scanStatus).mockReturnValue(status.promise)
    const host = await mounted()
    await clicked(unlink(host))
    const confirm = host.querySelector('.unlink-root .confirm') as HTMLElement
    act(() => confirm.focus())
    await flush(() => status.resolve(RUNNING))
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 2s')
    expect(host.querySelector('.unlink-root')).toBeNull()
    expect(document.activeElement).toBe(unlink(host))
    expect(holdOf(unlink(host))).toBe('held')
  })

  it('takes a scan up after a refusal with no report of an earlier scan beside Scanning…, as Scan directory does', async () => {
    const host = await mounted()
    await clicked(press(host, 'Scan directory'))
    vi.mocked(config.scanStatus).mockResolvedValue({
      ...RUNNING,
      running: false,
      report: { error: 'the scan panicked' },
    })
    await polled()
    expect(section(host, 'Scan report').textContent).toContain(
      'the scan panicked',
    )

    await refusedForItsScan(host)
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 2s')
    expect(host.textContent).not.toContain('the scan panicked')
  })

  it('takes a scan up after a refusal with no notice of a watch given up, and its failed polls counted afresh, as Scan directory does', async () => {
    const host = await mounted()
    await clicked(press(host, 'Scan directory'))
    vi.mocked(config.scanStatus).mockRejectedValue(
      new Error('/api/library/roots/3/scan: 502 bad gateway'),
    )
    for (let i = 0; i < config.POLL_MISSES_ALLOWED; i++) await polled()
    expect(host.querySelector('.scanning')).toBeNull()
    expect(host.querySelector('.scan-notice')?.textContent).toBe('bad gateway')

    await refusedForItsScan(host)
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 2s')
    expect(host.querySelector('.scan-notice')).toBeNull()
    // One failed poll is a blip, which the watch outlasts.
    vi.mocked(config.scanStatus).mockRejectedValueOnce(
      new Error('/api/library/roots/3/scan: 502 bad gateway'),
    )
    await polled()
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 2s')
    expect(host.querySelector('.scan-notice')).toBeNull()
  })

  it('reads its own status on a refusal for a hold, and leaves itself as it was where no scan of its own is running', async () => {
    const host = await mounted()
    expect(config.scanStatus).toHaveBeenCalledTimes(1)
    // A finished scan's report, which only a scan this row watched is drawn
    // with.
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: { error: 'the scan panicked' },
      last_scan_at: null,
      last_scan_status: null,
    })
    vi.mocked(api.unlinkCost).mockRejectedValueOnce(
      new Error(
        `/api/library/libraries/7/dirs/3?dry_run=true: 409 ${JSON.stringify({ error: 'a scan or another library job is running; remove the directory from this library once it has finished' })}`,
      ),
    )
    await clicked(unlink(host))
    await flush()
    expect(config.scanStatus).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.scanning')).toBeNull()
    expect(
      [
        ...host.querySelectorAll(
          '.settings-section > .settings-section-head > .settings-section-title',
        ),
      ].map((title) => title.textContent),
    ).toEqual(['How it is scanned', 'How it is read', 'Removal'])
    expect(holds(host)).toEqual({
      'Scan directory': 'free',
      'Move…': 'free',
      'Remove from Manga…': 'free',
      'Remove…': 'free',
    })
  })

  it('reads no status of its own on a refusal for a hold while it watches its own scan, so that the answer cannot take that scan up again once it has ended (O252)', async () => {
    const host = await mounted()
    /** Move…'s rehearsal, asked for Comics and refused for a hold. */
    const refusedMove = async () => {
      vi.mocked(api.moveCost).mockRejectedValueOnce(
        new Error(
          `/api/library/roots/3/move?dry_run=true: 409 ${JSON.stringify({ error: 'a scan of /srv/manga is running; move the directory once it has finished', volumes: [] })}`,
        ),
      )
      await clicked(
        host.querySelector('button[name="rehearse"]') as HTMLElement,
      )
      await flush()
    }
    await clicked(press(host, 'Move…'))
    await act(async () => {
      const box = host.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(box, '8')
      box.dispatchEvent(new Event('change', { bubbles: true }))
    })
    vi.mocked(config.scanStatus).mockResolvedValue(RUNNING)
    await refusedMove()
    expect(host.querySelector('.scanning')?.textContent).toBe('Scanning… 2s')

    // The dialog keeps its refusal, and the rehearsal is asked for again
    // while the scan is watched. A read sent for it answers only once the
    // poll has seen the scan end.
    const late = deferred<config.ScanStatus>()
    let polling = false
    vi.mocked(config.scanStatus).mockImplementation(() =>
      polling
        ? Promise.resolve({
            ...RUNNING,
            running: false,
            report: { error: 'the scan panicked' },
          })
        : late.promise,
    )
    const before = vi.mocked(config.scanStatus).mock.calls.length
    await refusedMove()
    polling = true
    await polled()
    polling = false
    expect(host.querySelector('.scanning')).toBeNull()
    expect(section(host, 'Scan report').textContent).toContain(
      'the scan panicked',
    )
    await flush(() => late.resolve(RUNNING))
    expect(host.querySelector('.scanning')).toBeNull()
    expect(section(host, 'Scan report').textContent).toContain(
      'the scan panicked',
    )
    expect(vi.mocked(config.scanStatus).mock.calls.length - before).toBe(1)
  })

  it('takes no focus as a page of a directory no other library holds is first drawn', async () => {
    act(() => (document.activeElement as HTMLElement | null)?.blur())
    const host = await mounted({ libraries: [MANGA] })
    expect(() => unlink(host)).toThrow()
    expect(document.activeElement).toBe(document.body)
  })
})

describe('RootRow, telling its page that the library’s scan status no longer holds (§9’s call 9)', () => {
  /** The row with `onScanMoved` a mock, once its mount read has answered. */
  async function settleTelling() {
    const moved = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RootRow root={root()} onChanged={vi.fn()} onScanMoved={moved} />,
      )
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    return { host, moved }
  }

  /** Lets what a press, a poll or an answer started settle. */
  const settled = (work: () => void) =>
    act(async () => {
      work()
      await Promise.resolve()
      await Promise.resolve()
    })

  const scan = (host: HTMLElement) =>
    settled(() => press(host, 'Scan directory').click())
  const poll = () => settled(() => vi.advanceTimersByTime(config.SCAN_POLL_MS))

  const running: config.ScanStatus = {
    root_id: 3,
    running: true,
    elapsed_secs: 3,
    report: null,
    last_scan_at: null,
    last_scan_status: null,
  }
  const over: config.ScanStatus = {
    root_id: 3,
    running: false,
    report: { error: 'the scan panicked' },
    last_scan_at: '2026-10-27 12:00:00',
    last_scan_status: 'failed',
  }
  const conflict = () =>
    new Error(
      '/api/library/roots/3/scan: 409 {"error":"a scan of /srv/manga is already running"}',
    )

  it('tells it as its scan is asked for, before the start is answered, and again once it is', async () => {
    const start = deferred<{ root_id: number; status_url: string }>()
    vi.mocked(config.startScan).mockReturnValue(start.promise)
    const { host, moved } = await settleTelling()
    await scan(host)
    expect(config.startScan).toHaveBeenCalledTimes(1)
    expect(moved).toHaveBeenCalledTimes(1)
    await settled(() =>
      start.resolve({ root_id: 3, status_url: '/api/library/roots/3/scan' }),
    )
    expect(moved).toHaveBeenCalledTimes(2)
  })

  it.each([
    {
      what: 'tells it again of a start answered 409',
      why: conflict,
      calls: 2,
    },
    {
      what: 'tells it nothing more of a start refused otherwise',
      why: () =>
        new Error('/api/library/roots/3/scan: 500 the store is locked'),
      calls: 1,
    },
  ])('$what', async ({ why, calls }) => {
    vi.mocked(config.startScan).mockRejectedValue(why())
    const { host, moved } = await settleTelling()
    await scan(host)
    expect(moved).toHaveBeenCalledTimes(calls)
  })

  it('tells it of a 409 that answers once the row has gone', async () => {
    const start = deferred<{ root_id: number; status_url: string }>()
    vi.mocked(config.startScan).mockReturnValue(start.promise)
    const { host, moved } = await settleTelling()
    await scan(host)
    await unmountRow()
    expect(moved).toHaveBeenCalledTimes(1)
    await settled(() => start.reject(conflict()))
    expect(moved).toHaveBeenCalledTimes(2)
  })

  it.each([
    {
      what: 'tells it when its mount read takes up a running scan',
      status: running,
      calls: 1,
    },
    {
      what: 'tells it nothing when its mount read finds the scan over',
      status: over,
      calls: 0,
    },
  ])('$what', async ({ status, calls }) => {
    vi.mocked(config.scanStatus).mockResolvedValue(status)
    const { moved } = await settleTelling()
    expect(moved).toHaveBeenCalledTimes(calls)
  })

  it('tells it once when its poll finds the scan over, and not for a poll that finds it running', async () => {
    const { host, moved } = await settleTelling()
    vi.mocked(config.scanStatus).mockResolvedValue(running)
    await scan(host)
    expect(moved).toHaveBeenCalledTimes(2)
    await poll()
    expect(moved).toHaveBeenCalledTimes(2)

    vi.mocked(config.scanStatus).mockResolvedValue(over)
    await poll()
    expect(host.querySelector('.scanning')).toBeNull()
    expect(moved).toHaveBeenCalledTimes(3)
    await poll()
    await unmountRow()
    expect(moved).toHaveBeenCalledTimes(3)
  })

  it('tells it when the row goes while its scan runs, and not when it goes with none running', async () => {
    const idle = await settleTelling()
    await unmountRow()
    expect(idle.moved).not.toHaveBeenCalled()

    vi.mocked(config.scanStatus).mockResolvedValue(running)
    const { host, moved } = await settleTelling()
    expect(moved).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.scanning')).not.toBeNull()
    await unmountRow()
    expect(moved).toHaveBeenCalledTimes(2)
  })

  it('tells it nothing more when its polls fail until it gives up, nor when it goes after that', async () => {
    const { host, moved } = await settleTelling()
    await scan(host)
    expect(moved).toHaveBeenCalledTimes(2)

    vi.mocked(config.scanStatus).mockRejectedValue(
      new Error('/api/library/roots/3/scan: 500 the store is locked'),
    )
    for (let miss = 0; miss < config.POLL_MISSES_ALLOWED; miss++) await poll()
    expect(host.querySelector('.scanning')).toBeNull()
    expect(host.querySelector('.scan-notice')?.textContent).toBe(
      'the store is locked',
    )
    expect(moved).toHaveBeenCalledTimes(2)
    await unmountRow()
    expect(moved).toHaveBeenCalledTimes(2)
  })
})

/**
 * Type into a box and stay in it: the page then goes by a route that blurs
 * nothing, as the browser's Back does.
 */
async function typeAndStay(host: HTMLElement, name: string, value: string) {
  const box = host.querySelector(`[name="${name}"]`) as HTMLInputElement
  await act(async () => {
    box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set?.call(box, value)
    box.dispatchEvent(new Event('input', { bubbles: true }))
    await Promise.resolve()
  })
}

/** The row goes, as its page does, and what it sent on going settles. */
async function unmountRow() {
  await act(async () => {
    rerender(<div />)
    await Promise.resolve()
    await Promise.resolve()
  })
}

describe('RootRow, an edit still in its box when the row goes', () => {
  it('sends each box typed in and not left, emptied as null, and asks for the list again', async () => {
    const onChanged = vi.fn()
    const host = await settle({ scan_interval_secs: 900 }, onChanged)
    await typeAndStay(host, 'default_language', 'ja')
    await typeAndStay(host, 'edition_key', '')
    await typeAndStay(host, 'scan_every', '10')
    expect(config.patchRoot).not.toHaveBeenCalled()

    await unmountRow()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { default_language: 'ja' }],
      [3, { edition_key: null }],
      [3, { scan_interval_secs: 600 }],
    ])
    expect(onChanged).toHaveBeenCalledTimes(3)
  })

  it('sends Every’s count in the unit drawn, hours as hours, and a count typed after choosing Every in the hours it saved', async () => {
    const hours = await settle()
    await typeAndStay(hours, 'scan_every', '2')
    await unmountRow()
    const chosenNow = await settle({ scan_interval_secs: null })
    await choose(chosenNow, 'Every')
    await typeAndStay(chosenNow, 'scan_every', '10')
    await unmountRow()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 7200 }],
      [3, { scan_interval_secs: 86400 }],
      [3, { scan_interval_secs: 36000 }],
    ])
  })

  it('sends nothing for Every’s box emptied', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    await typeAndStay(host, 'scan_every', '')
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends an edit once when its box was left before the row went', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    await edit(host, 'default_language', 'ja')
    await edit(host, 'scan_every', '10')
    expect(config.patchRoot).toHaveBeenCalledTimes(2)
    await unmountRow()
    expect(config.patchRoot).toHaveBeenCalledTimes(2)
  })

  it('sends a count once when its unit was changed before the row went', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    await typeAndStay(host, 'scan_every', '10')
    await pickUnit(host, 'hours')
    expect(config.patchRoot).toHaveBeenCalledTimes(1)
    await unmountRow()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 36000 }],
    ])
  })

  it('sends no count typed in before Off was chosen by a click that left the box focused', async () => {
    // A mouse leaves the box before its click lands, which sends the count
    // first (`Config.browser.test.tsx`). `HTMLElement.click()` moves no
    // focus, so the count is still typed in when Off is chosen.
    const host = await settle({ scan_interval_secs: 900 })
    await typeAndStay(host, 'scan_every', '10')
    await choose(host, 'Off')
    await unmountRow()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
    ])
  })

  it('sends a count typed after Off as the row goes only once Off is answered', async () => {
    const off = deferred<undefined>()
    vi.mocked(config.patchRoot).mockReturnValueOnce(off.promise)
    const host = await settle({ scan_interval_secs: null })
    await choose(host, 'Off')
    await choose(host, 'Every')
    await typeAndStay(host, 'scan_every', '12')
    await unmountRow()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
    ])
    await flush(() => off.resolve(undefined))
    await flush()
    await flush()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { scan_interval_secs: 0 }],
      [3, { scan_interval_secs: 86400 }],
      [3, { scan_interval_secs: 43200 }],
    ])
  })

  it('sends no count typed in a box the row has since drawn afresh or taken away', async () => {
    // A row read again with another interval, from elsewhere, draws a new
    // box, in its own unit, over the count typed; and a row read again as
    // Off draws none.
    const again = (scan_interval_secs: number) =>
      act(async () => {
        rerender(
          <RootRow
            root={root({ scan_interval_secs })}
            probe={undefined}
            onChanged={vi.fn()}
          />,
        )
        await Promise.resolve()
      })
    const redrawn = await settle({ scan_interval_secs: 900 })
    await typeAndStay(redrawn, 'scan_every', '30')
    await again(7200)
    expect([everyBox(redrawn)?.value, everyUnit(redrawn)?.value]).toEqual([
      '2',
      'hours',
    ])
    await unmountRow()
    const taken = await settle({ scan_interval_secs: 900 })
    await typeAndStay(taken, 'scan_every', '30')
    await again(0)
    expect(everyBox(taken)).toBeNull()
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends nothing for a box focused again and not typed in, though it holds a refused edit', async () => {
    const host = await settle({ exclude_globs: '**/extras/**' })
    vi.mocked(config.patchRoot).mockRejectedValue(new Error('502 Bad Gateway'))
    await edit(host, 'exclude_globs', '**/tmp/**')
    expect(config.patchRoot).toHaveBeenCalledTimes(1)
    await act(async () => {
      host
        .querySelector('[name="exclude_globs"]')
        ?.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })
    await unmountRow()
    expect(config.patchRoot).toHaveBeenCalledTimes(1)
  })

  it('sends nothing for a box typed back to what the row holds', async () => {
    const host = await settle()
    await typeAndStay(host, 'edition_key', 'enx')
    await typeAndStay(host, 'edition_key', 'en')
    await typeAndStay(host, 'scan_every', '10')
    await typeAndStay(host, 'scan_every', '24')
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('compares an edit with the row as last drawn, not as first drawn', async () => {
    const host = await settle()
    await typeAndStay(host, 'default_language', 'ja')
    await act(async () => {
      rerender(
        <RootRow root={root({ default_language: 'ja' })} onChanged={vi.fn()} />,
      )
      await Promise.resolve()
    })
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends no count under 5 minutes', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    await typeAndStay(host, 'scan_every', '4')
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends no count the box could not read', async () => {
    const host = await settle({ scan_interval_secs: 900 })
    unreadable(host)
    await typeAndStay(host, 'scan_every', '')
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends nothing when nothing was typed, though StrictMode mounts it twice', async () => {
    await act(async () => {
      mount(
        <StrictMode>
          <RootRow root={root()} onChanged={vi.fn()} />
        </StrictMode>,
      )
      await Promise.resolve()
    })
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('sends a typed edit once under StrictMode', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <StrictMode>
          <RootRow root={root()} onChanged={vi.fn()} />
        </StrictMode>,
      )
      await Promise.resolve()
    })
    await typeAndStay(host as HTMLElement, 'default_language', 'ja')
    await unmountRow()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { default_language: 'ja' }],
    ])
  })

  it('sends nothing to a directory its own removal took, though a box was typed in while the removal was out', async () => {
    const removed = deferred<config.RemovalAnswer>()
    vi.mocked(config.removeRoot).mockReturnValue(removed.promise)
    const onRemoved = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RootRow root={root()} onRemoved={onRemoved} onChanged={vi.fn()} />,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    const row = host as HTMLElement
    await act(async () => {
      press(row, 'Remove…').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      row.querySelector<HTMLElement>('.remove-root .confirm')?.click()
      await Promise.resolve()
    })
    await typeAndStay(row, 'default_language', 'ja')
    await flush(() =>
      removed.resolve({
        removed: {
          works: 1,
          positions: 0,
          notes: 0,
          left_with_no_library: [],
          libraries_removed: [],
          kept: [],
        },
        derive: emptyDerive,
        dry_run: false,
      }),
    )
    expect(onRemoved).toHaveBeenCalledTimes(1)
    await unmountRow()
    expect(config.patchRoot).not.toHaveBeenCalled()
  })

  it('still sends an edit typed while a move was out, since the directory is still there', async () => {
    const answer = (dry_run: boolean): api.MoveAnswer => ({
      moved: {
        library_id: 7,
        library: 'Manga',
        gains_sight: [],
        loses_sight: [],
        library_removed: null,
        grants_removed: 0,
        left_with_no_library: [],
      },
      dry_run,
    })
    vi.mocked(api.moveCost).mockResolvedValue(answer(true))
    const moved = deferred<api.MoveAnswer>()
    vi.mocked(api.moveDir).mockReturnValue(moved.promise)
    const onMoved = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RootRow
          root={root()}
          library={{ id: 5, name: 'Chapters' }}
          moveTo={[{ id: 7, name: 'Manga' }]}
          onMoved={onMoved}
          onChanged={vi.fn()}
        />,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    const row = host as HTMLElement
    await act(async () => {
      press(row, 'Move…').click()
      await Promise.resolve()
    })
    await act(async () => {
      const target = row.querySelector(
        'select[name="move-target"]',
      ) as HTMLSelectElement
      Object.getOwnPropertyDescriptor(
        HTMLSelectElement.prototype,
        'value',
      )?.set?.call(target, '7')
      target.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await act(async () => {
      press(row, 'See what this would do').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    await act(async () => {
      ;(row.querySelector('.move-root .confirm') as HTMLElement).click()
      await Promise.resolve()
    })
    await typeAndStay(row, 'default_language', 'ja')
    await flush(() => moved.resolve(answer(false)))
    expect(onMoved).toHaveBeenCalledTimes(1)
    await unmountRow()
    expect(vi.mocked(config.patchRoot).mock.calls).toEqual([
      [3, { default_language: 'ja' }],
    ])
  })

  it('hands up the sentence of each edit it sent that is refused, named for its box and the directory, and asks for no list', async () => {
    vi.mocked(config.patchRoot).mockRejectedValue(
      new Error('/api/library/roots/3: 500 {"error":"the store is locked"}'),
    )
    const onChanged = vi.fn()
    const onLateRefusal = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RootRow
          root={root()}
          onChanged={onChanged}
          onLateRefusal={onLateRefusal}
        />,
      )
      await Promise.resolve()
    })
    await typeAndStay(host as HTMLElement, 'default_language', 'ja')
    await typeAndStay(host as HTMLElement, 'scan_every', '10')
    await unmountRow()
    expect(config.patchRoot).toHaveBeenCalledTimes(2)
    expect(onChanged).not.toHaveBeenCalled()
    expect(onLateRefusal.mock.calls).toEqual([
      ['Default language for /srv/manga was not saved (the store is locked).'],
      ['Scan schedule for /srv/manga was not saved (the store is locked).'],
    ])
  })
})
