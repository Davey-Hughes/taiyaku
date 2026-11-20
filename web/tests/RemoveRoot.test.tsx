import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from '../src/config'
import { RemoveRoot } from '../src/RemoveRoot'
import { deferred, flush } from './support/async'
import { holdOf } from './support/held'
import { mount, rerender } from './support/mount'
import {
  holdTheClock,
  pass,
  sentenceDrawn,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
}))

const root: config.Root = {
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
  paused_at: null,
}

/**
 * A whole `DeriveReport`, because the dialog renders one.
 *
 * A `{} as Derived` cast stops being enough the moment a component reads
 * the thirty-three fields.
 */
const derived = (over: Partial<config.Derived> = {}): config.Derived => ({
  series: 1,
  works: 2,
  editions: 2,
  members: 2,
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
  joined: 0,
  followed: 0,
  unwritten_alignments: 0,
  moved_editions: 0,
  unmoved_editions: 0,
  ...over,
})

const answer = (over: Partial<config.Removal> = {}): config.RemovalAnswer => ({
  removed: {
    works: 412,
    positions: 3,
    notes: 27,
    positions_page_gone: 0,
    notes_page_gone: 0,
    left_with_no_library: [],
    libraries_removed: [],
    kept: [],
    ...over,
  },
  derive: derived(),
  dry_run: true,
})

async function settle(onRemoved = vi.fn(), holders?: string[]) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RemoveRoot
        root={root}
        holders={holders}
        onCancel={vi.fn()}
        onRemoved={onRemoved}
      />,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

function press(host: HTMLElement, label: string) {
  const button = [...host.querySelectorAll('button')].find((b) =>
    b.textContent?.includes(label),
  )
  if (!button) throw new Error(`no button matching ${label}`)
  return button
}

beforeEach(() => {
  // `mockResolvedValue` sets an implementation and leaves the call log
  // alone, and this project sets no `clearMocks` in `vite.config.ts` — so a
  // test asserting a *negative* would otherwise read the previous test's
  // calls and fail, or worse, pass on them.
  vi.clearAllMocks()
  vi.mocked(config.removalCost).mockResolvedValue(answer())
  vi.mocked(config.removeRoot).mockResolvedValue({
    ...answer(),
    dry_run: false,
  })
})

describe('RemoveRoot', () => {
  it('draws the rehearsal’s report headings one level beneath the directory’s path', async () => {
    // The dialog sits in a card whose heading is an h4.
    const host = await settle()
    const headings = [
      ...host.querySelectorAll('.removal-derive .report-heading'),
    ]
    expect(headings.map((h) => h.textContent)).toEqual([
      'What derivation made of every library',
    ])
    expect(headings.map((h) => h.tagName)).toEqual(['H5'])
  })

  it('states all three counts before it acts', async () => {
    // The whole sentence, not three loose digits: `toContain('3')` is
    // satisfied by any string holding a 3 anywhere, including the directory's
    // own id in the aria-label, so it would pass against a dialog that never
    // drew the positions count at all.
    expect((await settle()).textContent).toContain(
      'This removes 412 works, 3 in-progress positions and 27 notes.',
    )
  })

  it('says under the counts how many positions and notes on works that stay it takes the page of (O244)', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ positions_page_gone: 2, notes_page_gone: 1 }),
    )
    const host = await settle()
    expect(host.querySelector('.pages-gone')?.textContent).toBe(
      'On works that stay, this takes the page of 2 in-progress positions and 1 note.',
    )
    expect(host.querySelector('.counts + .pages-gone')).not.toBeNull()
  })

  // The (2, 1) case draws under `&&` too, and under a guard on one figure
  // alone: these two are what tell "either above 0" from "both".
  it.each([
    [0, 1, 'On works that stay, this takes the page of 1 note.'],
    [
      1,
      0,
      'On works that stay, this takes the page of 1 in-progress position.',
    ],
  ])(
    'draws the sentence at positions_page_gone %i and notes_page_gone %i: either figure alone is enough (O244)',
    async (positions, notes, sentence) => {
      vi.mocked(config.removalCost).mockResolvedValue(
        answer({ positions_page_gone: positions, notes_page_gone: notes }),
      )
      const host = await settle()
      expect(host.querySelector('.pages-gone')?.textContent).toBe(sentence)
    },
  )

  it('says nothing of pages taken where it takes none', async () => {
    const host = await settle()
    expect(host.querySelector('.counts')).not.toBeNull()
    expect(host.querySelector('.pages-gone')).toBeNull()
  })

  it('says a removal takes only what no other directory holds, changes no archive, and says where a staying work’s documents go (O256)', async () => {
    const host = await settle()
    expect(host.querySelector('.irreversible')?.textContent).toBe(
      'Nothing here can be undone from this screen. What goes is everything derived from files no other directory holds, and everything anybody recorded against a work that goes. No archive on disk is changed. A work that stays takes its documents under .taiyaku/ to the directory that now holds its files, if that directory keeps documents; the report after the removal lists any left behind as orphaned documents.',
    )
  })

  it('says the library goes too where its dry run says this is its only directory', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ libraries_removed: ['Manga'] }),
    )
    const host = await settle()
    expect(host.querySelector('.last-dir')?.textContent).toBe(
      'This is the only directory of the library Manga, so removing it removes the library too.',
    )
    expect(
      host.querySelector('[role="group"]')?.getAttribute('aria-label'),
    ).toBe('Remove the directory at /srv/manga')
  })

  it('says nothing about a library where other directories stay', async () => {
    const host = await settle()
    expect(host.querySelector('.last-dir')).toBeNull()
  })

  it('names each library its dry run says the removal empties, in one sentence for several', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ libraries_removed: ['Kids', 'Teens'] }),
    )
    const host = await settle(vi.fn(), ['Manga', 'Kids', 'Teens'])
    expect(host.querySelector('.last-dir')?.textContent).toBe(
      'This is the only directory of the libraries Kids and Teens, so removing it removes them too.',
    )
  })

  it('names every library holding the directory where several do, and only the counts where one does', async () => {
    const shared = await settle(vi.fn(), ['Manga', 'Kids'])
    expect(shared.querySelector('.counts')?.textContent).toBe(
      'This removes it from Manga and Kids, and 412 works, 3 in-progress positions and 27 notes.',
    )
    const one = await settle(vi.fn(), ['Manga'])
    expect(one.querySelector('.counts')?.textContent).toBe(
      'This removes 412 works, 3 in-progress positions and 27 notes.',
    )
  })

  it('names the library an account could see where another library holds the directory, and says of several only that each is among them', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ left_with_no_library: ['alice'], libraries_removed: ['Kids'] }),
    )
    const one = await settle(vi.fn(), ['Manga', 'Kids'])
    expect(one.querySelector('.left-with-no-library')?.textContent).toBe(
      'alice can see only Kids. After this, alice will see no library until an administrator grants one.',
    )
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({
        left_with_no_library: ['alice'],
        libraries_removed: ['Manga', 'Kids'],
        kept: [],
      }),
    )
    const two = await settle(vi.fn(), ['Manga', 'Kids'])
    expect(two.querySelector('.left-with-no-library')?.textContent).toBe(
      'alice can see only libraries this removes. After this, alice will see no library until an administrator grants one.',
    )
  })

  it('names the accounts left with no library under the counts, and still offers the confirm', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({
        left_with_no_library: ['alice', 'bob'],
        libraries_removed: ['Manga'],
        kept: [],
      }),
    )
    const host = await settle()
    expect(host.querySelector('.left-with-no-library')?.textContent).toBe(
      "alice and bob can see only this directory's library. After this, they will see no library until an administrator grants one.",
    )
    expect(
      host.querySelector('.last-dir + .left-with-no-library'),
    ).not.toBeNull()
    expect(holdOf(press(host, 'Remove /srv/manga'))).toBe('free')
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('names the accounts the server lists, though this dialog was told of no last directory', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ left_with_no_library: ['alice'] }),
    )
    const host = await settle()
    expect(host.querySelector('.last-dir')).toBeNull()
    expect(host.querySelector('.left-with-no-library')?.textContent).toBe(
      "alice can see only this directory's library. After this, alice will see no library until an administrator grants one.",
    )
  })

  it('names nobody where no account is left with no library', async () => {
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ libraries_removed: ['Manga'] }),
    )
    const host = await settle()
    expect(host.querySelector('.last-dir')).not.toBeNull()
    expect(host.querySelector('.left-with-no-library')).toBeNull()
    expect(host.textContent).not.toContain('will see no library')
  })

  it('draws a zero rather than saying nothing about it', async () => {
    // Spec §3.3: "This removes 0 works" is a different sentence from a dialog
    // that mentions no counts, and the second reads as "counts unavailable".
    vi.mocked(config.removalCost).mockResolvedValue(
      answer({ works: 0, positions: 0, notes: 0 }),
    )
    const host = await settle()
    expect(host.textContent).toContain(
      'This removes 0 works, 0 in-progress positions and 0 notes.',
    )
  })

  it('sends no real DELETE until the confirm is pressed', async () => {
    await settle()
    expect(config.removalCost).toHaveBeenCalledWith(3)
    expect(config.removeRoot).not.toHaveBeenCalled()
  })

  it('hands its caller what the real removal actually did', async () => {
    // `removeRoot(...).then(() => onRemoved())` would throw away a full
    // `RemovalAnswer` — the real counts and the real `DeriveReport` the server
    // adds. The dialog then unmounts, so a removal whose re-derive reported
    // `unwritable_documents` or `lost_positions` would reach nobody; the admin
    // would have seen only the rolled-back rehearsal, whose figures can differ.
    const onRemoved = vi.fn()
    const host = await settle(onRemoved)
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(config.removeRoot).toHaveBeenCalledWith(3)
    expect(onRemoved).toHaveBeenCalledWith(
      expect.objectContaining({ dry_run: false }),
    )
  })

  it('draws the server sentence when the dry run is refused', async () => {
    vi.mocked(config.removalCost).mockRejectedValue(
      new Error('/api/library/roots/3: 403 an administrator may do this'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'an administrator may do this',
    )
  })

  it("draws the refusal's sentence when the dry run is refused with one", async () => {
    vi.mocked(config.removalCost).mockRejectedValue(
      new Error('/api/library/roots/3: 403 {"error":"an administrator only"}'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'an administrator only',
    )
  })

  it("draws the refusal's sentence when the real removal is refused", async () => {
    vi.mocked(config.removeRoot).mockRejectedValue(
      new Error('/api/library/roots/3: 409 {"error":"a scan is running"}'),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'a scan is running',
    )
  })

  it('drops its counts and its confirm when the removal itself is refused for a hold, and leaves the refusal and Cancel, as Move’s dialog does (O252)', async () => {
    vi.mocked(config.removeRoot).mockRejectedValue(
      new Error(
        '/api/library/roots/3: 409 {"error":"a scan of /srv/manga is running; remove the directory once it has finished"}',
      ),
    )
    const onCancel = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RemoveRoot root={root} onCancel={onCancel} onRemoved={vi.fn()} />,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    const mounted = host
    expect(mounted.querySelector('.counts')).not.toBeNull()
    await act(async () => {
      press(mounted, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(
      [...mounted.querySelectorAll('[role="alert"]')].map((a) => a.textContent),
    ).toEqual([
      'a scan of /srv/manga is running; remove the directory once it has finished',
    ])
    expect(mounted.querySelector('.counts')).toBeNull()
    expect(mounted.querySelector('.irreversible')).toBeNull()
    expect(mounted.querySelector('.removal-derive')).toBeNull()
    expect(mounted.querySelector('.confirm')).toBeNull()
    expect(
      [...mounted.querySelectorAll('button')].map((b) => b.textContent),
    ).toEqual(['Cancel'])
    expect(mounted.textContent).not.toContain('Working out')
    await act(async () => {
      press(mounted, 'Cancel').click()
      await Promise.resolve()
    })
    expect(onCancel).toHaveBeenCalledTimes(1)
    expect(config.removeRoot).toHaveBeenCalledTimes(1)
  })

  it('puts focus on Cancel when a refusal for a hold takes away the confirm that had it, so that focus does not fall to the page (O252)', async () => {
    vi.mocked(config.removeRoot).mockRejectedValue(
      new Error(
        '/api/library/roots/3: 409 {"error":"a scan of /srv/manga is running; remove the directory once it has finished"}',
      ),
    )
    const host = await settle()
    const confirm = press(host, 'Remove /srv/manga')
    act(() => confirm.focus())
    expect(document.activeElement).toBe(confirm)
    await act(async () => {
      confirm.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.confirm')).toBeNull()
    expect(document.activeElement).toBe(press(host, 'Cancel'))
  })

  it('does not offer a confirm while the counts are unknown', async () => {
    vi.mocked(config.removalCost).mockRejectedValue(new Error('500'))
    const host = await settle()
    expect(
      [...host.querySelectorAll('button')].some((b) =>
        b.textContent?.includes('Remove /srv/manga'),
      ),
    ).toBe(false)
  })

  it('cancels without removing anything', async () => {
    const onCancel = vi.fn()
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RemoveRoot root={root} onCancel={onCancel} onRemoved={vi.fn()} />,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    // Bound to a const: `host` is a `let` assigned inside a closure, so
    // TypeScript drops the narrowing above the moment it is read inside
    // another one.
    const mounted = host
    await act(async () => {
      press(mounted, 'Cancel').click()
      await Promise.resolve()
    })
    expect(onCancel).toHaveBeenCalled()
    expect(config.removeRoot).not.toHaveBeenCalled()
  })

  it('re-offers the confirm when the removal itself fails', async () => {
    // A removal that failed must not leave the button stuck at "Removing…"
    // with nothing said: the directory is still there and the person needs both
    // the reason and a way to try again.
    vi.mocked(config.removeRoot).mockRejectedValue(
      new Error('/api/library/roots/3: 500 deriving after the removal failed'),
    )
    const host = await settle()
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'deriving after the removal failed',
    )
    expect(holdOf(press(host, 'Remove /srv/manga'))).toBe('free')
  })

  it('holds no failure while the next removal is in flight', async () => {
    vi.mocked(config.removeRoot)
      .mockRejectedValueOnce(
        new Error('/api/library/roots/3: 500 {"error":"the disk is full"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const host = await settle()
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'the disk is full',
    )
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(config.removeRoot).toHaveBeenCalledTimes(2)
  })

  it('does not write back a dry run that arrived after it closed', async () => {
    // The `cancelled` flag in the effect. Without it, a person who opens the
    // dialog and closes it before the dry run returns gets a state write into
    // an unmounted component — the same guard `Shell` carries on its own
    // fetch effect.
    let land: (answer: config.RemovalAnswer) => void = () => {}
    vi.mocked(config.removalCost).mockReturnValue(
      new Promise<config.RemovalAnswer>((resolve) => {
        land = resolve
      }),
    )
    const Wrapper = ({ show }: { show: boolean }) =>
      show ? (
        <RemoveRoot root={root} onCancel={vi.fn()} onRemoved={vi.fn()} />
      ) : null

    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(<Wrapper show />)
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')

    await act(async () => {
      rerender(<Wrapper show={false} />)
    })
    await act(async () => {
      land(answer())
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(host.querySelector('.remove-root')).toBeNull()
    expect(host.textContent).not.toContain('412')
  })

  it('draws what the removal re-derive found, not only the three counts', async () => {
    // `RemovalAnswer.derive` exists so that what a removal's re-derive found
    // reaches a caller. Rendering only `removed` would drop it — on the one
    // screen built around naming what a pass did not do.
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 412,
        positions: 3,
        notes: 27,
        positions_page_gone: 0,
        notes_page_gone: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived({
        unwritable_documents: ['/srv/manga/.taiyaku/locked.json'],
        lost_positions: 4,
      }),
      dry_run: true,
    })
    const host = await settle()
    expect(host.textContent).toContain('/srv/manga/.taiyaku/locked.json')
    expect(
      host.querySelector('[data-field="lost_positions"]')?.textContent,
    ).toContain('4')
  })

  it('sends one DELETE for a double-pressed confirm', async () => {
    // An unguarded second dispatch is least worth leaving to luck here: two
    // presses before the next render both read `removing` false, so without a
    // guard both would send the real, committed DELETE.
    const host = await settle()
    await act(async () => {
      press(host, 'Remove /srv/manga').click()
      press(host, 'Remove /srv/manga').click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(config.removeRoot).mock.calls).toHaveLength(1)
  })

  it('counts one work as a work, not as 1 works', async () => {
    // This is the most destructive action in the product; it should not read
    // like a debug print.
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 1,
        positions: 1,
        notes: 1,
        positions_page_gone: 0,
        notes_page_gone: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    expect((await settle()).textContent).toContain(
      'This removes 1 work, 1 in-progress position and 1 note.',
    )
  })

  it('still says zero in the plural', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: {
        works: 0,
        positions: 0,
        notes: 0,
        positions_page_gone: 0,
        notes_page_gone: 0,
        left_with_no_library: [],
        libraries_removed: [],
        kept: [],
      },
      derive: derived(),
      dry_run: true,
    })
    expect((await settle()).textContent).toContain(
      'This removes 0 works, 0 in-progress positions and 0 notes.',
    )
  })

  it('marks the confirm button and not Cancel, before the cost has loaded', async () => {
    // The whole reason this is a class and not `:first-of-type`: until the
    // cost resolves, Cancel is the only button in the block. Follows the
    // same `land` idiom as "does not write back a dry run that arrived
    // after it closed" above, to hold the fetch pending on purpose.
    let land: (answer: config.RemovalAnswer) => void = () => {}
    vi.mocked(config.removalCost).mockReturnValue(
      new Promise<config.RemovalAnswer>((resolve) => {
        land = resolve
      }),
    )
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <RemoveRoot root={root} onCancel={vi.fn()} onRemoved={vi.fn()} />,
      )
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')

    expect(host.querySelectorAll('.remove-root button')).toHaveLength(1)
    expect(host.querySelector('.remove-root .confirm')).toBeNull()

    await act(async () => {
      land(answer())
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(host.querySelector('.remove-root .confirm')?.textContent).toBe(
      'Remove /srv/manga',
    )
    expect(press(host, 'Cancel').classList.contains('confirm')).toBe(false)
  })

  describe('its waits, drawn only once each has lasted', () => {
    const WORKING = 'Working out what this would remove…'
    const asking = async () => {
      const cost = deferred<config.RemovalAnswer>()
      vi.mocked(config.removalCost).mockReturnValue(cost.promise)
      let host: HTMLElement | undefined
      await act(async () => {
        host = mount(
          <RemoveRoot root={root} onCancel={vi.fn()} onRemoved={vi.fn()} />,
        )
        await Promise.resolve()
      })
      if (!host) throw new Error('nothing mounted')
      return { host, cost }
    }

    it('keeps the dry run’s sentence unseen for counts answered inside the wait’s delay', async () => {
      holdTheClock()
      const { host, cost } = await asking()
      const drawn = watchDrawn(() => sentenceDrawn(host, WORKING))
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() => cost.resolve(answer()))
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('.counts')).not.toBeNull()
      expect([...drawn.seen]).toEqual(['unseen', 'none'])
    })

    it('draws the dry run’s sentence from the wait’s delay, in the room it kept', async () => {
      holdTheClock()
      const { host } = await asking()
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(sentenceDrawn(host, WORKING)).toBe('unseen')
      pass(1)
      expect(sentenceDrawn(host, WORKING)).toBe('drawn')
    })

    /** Remove's words, and whether it and Cancel are drawn held. */
    const state = (host: HTMLElement) => {
      const confirm = host.querySelector(
        '.remove-root .confirm',
      ) as HTMLButtonElement
      return [
        confirm.textContent,
        holdOf(confirm),
        holdOf(press(host, 'Cancel')),
      ]
    }

    it('draws no wait and no held button for a removal refused inside the wait’s delay', async () => {
      holdTheClock()
      const sent = deferred<config.RemovalAnswer>()
      vi.mocked(config.removeRoot).mockReturnValue(sent.promise)
      const host = await settle()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await act(async () => {
        press(host, 'Remove /srv/manga').click()
        await Promise.resolve()
      })
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() =>
        sent.reject(new Error('/api/library/roots/3: 500 the disk is full')),
      )
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('[role="alert"]')?.textContent).toContain(
        'the disk is full',
      )
      expect([...drawn.seen]).toEqual([
        JSON.stringify(['Remove /srv/manga', 'free', 'free']),
      ])
    })

    it('draws Removing… and holds Remove and Cancel from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(config.removeRoot).mockReturnValue(new Promise(() => {}))
      const host = await settle()
      await act(async () => {
        press(host, 'Remove /srv/manga').click()
        await Promise.resolve()
      })
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Remove /srv/manga', 'free', 'free'])
      pass(1)
      expect(state(host)).toEqual(['Removing…', 'held', 'held'])
    })

    it('holds Remove and Cancel from the send, before either is drawn held, and once they are', async () => {
      holdTheClock()
      vi.mocked(config.removeRoot).mockReturnValue(new Promise(() => {}))
      const onCancel = vi.fn()
      let host: HTMLElement | undefined
      await act(async () => {
        host = mount(
          <RemoveRoot root={root} onCancel={onCancel} onRemoved={vi.fn()} />,
        )
        await Promise.resolve()
        await Promise.resolve()
      })
      if (!host) throw new Error('nothing mounted')
      const mounted = host
      const confirm = press(mounted, 'Remove /srv/manga')
      await flush(() => confirm.click())
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(mounted)).toEqual(['Remove /srv/manga', 'free', 'free'])
      await flush(() => confirm.click())
      expect(config.removeRoot).toHaveBeenCalledTimes(1)
      await flush(() => press(mounted, 'Cancel').click())
      expect(onCancel).not.toHaveBeenCalled()

      pass(1)
      expect(state(mounted)).toEqual(['Removing…', 'held', 'held'])
      await flush(() => press(mounted, 'Removing…').click())
      expect(config.removeRoot).toHaveBeenCalledTimes(1)
      await flush(() => press(mounted, 'Cancel').click())
      expect(onCancel).not.toHaveBeenCalled()
    })
  })
})
