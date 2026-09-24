import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { RemoveRoot } from './RemoveRoot'
import { mount, rerender } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
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
}

/**
 * A whole `DeriveReport`, because the dialog now renders one.
 *
 * `{} as Derived` was enough while only `removed` was drawn; it stops being
 * enough the moment a component reads the thirty-one fields, and a cast that
 * lies about a shape is exactly the fixture this repo keeps getting caught by.
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
  ...over,
})

const answer = (over: Partial<config.Removal> = {}): config.RemovalAnswer => ({
  removed: { works: 412, positions: 3, notes: 27, ...over },
  derive: derived(),
  dry_run: true,
})

async function settle(onRemoved = vi.fn()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RemoveRoot root={root} onCancel={vi.fn()} onRemoved={onRemoved} />,
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
  it('states all three counts before it acts', async () => {
    // The whole sentence, not three loose digits: `toContain('3')` is
    // satisfied by any string holding a 3 anywhere, including the root's own
    // id in the aria-label, so it would pass against a dialog that never
    // drew the positions count at all.
    expect((await settle()).textContent).toContain(
      'This removes 412 works, 3 in-progress positions and 27 notes.',
    )
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
    // **Pass 4, finding 3.** `removeRoot(...).then(() => onRemoved())` threw
    // away a full `RemovalAnswer` — the real counts and the real
    // `DeriveReport` the server added so that "nothing a removal's re-derive
    // found ever reached a caller" would stop being true. The dialog then
    // unmounts, so a removal whose re-derive reported `unwritable_documents`
    // or `lost_positions` reached nobody; the admin had only ever seen the
    // rolled-back rehearsal, whose figures can differ.
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
    // with nothing said: the root is still there and the person needs both
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
    expect(press(host, 'Remove /srv/manga').disabled).toBe(false)
  })

  it('does not write back a dry run that arrived after it closed', async () => {
    // The `cancelled` flag in the effect. Without it, a person who opens the
    // dialog and closes it before the 281 ms dry run returns gets a state
    // write into an unmounted component — the same guard `Library`, `Shell`
    // and `Series` all carry on their own fetch effects.
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
    // **Review finding 9.** `RemovalAnswer.derive` exists because the server's
    // own whole-branch review added the key so that "nothing a removal's
    // re-derive found ever reached a caller" would stop being true. Rendering
    // only `removed` put it straight back — on the one screen built around
    // naming what a pass did not do.
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 412, positions: 3, notes: 27 },
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
    // **Review pass 2, finding 4 — the third place the same class occurs, and
    // the one where an unguarded second dispatch is least worth leaving to
    // luck.** `disabled={removing}` is state-driven; two presses before the
    // next render both send the real, committed DELETE.
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
    // Found by rendering the dialog in a real browser: jsdom has no opinion
    // about prose, and every fixture until now happened to use plurals. This
    // is the most destructive action in the product; it should not read like
    // a debug print.
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 1, positions: 1, notes: 1 },
      derive: derived(),
      dry_run: true,
    })
    expect((await settle()).textContent).toContain(
      'This removes 1 work, 1 in-progress position and 1 note.',
    )
  })

  it('still says zero in the plural', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
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
})
