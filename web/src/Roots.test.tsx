import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { Roots } from './Roots'
import { fields } from './testing/fields'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  addRoot: vi.fn(),
  listRoots: vi.fn(),
  patchRoot: vi.fn(),
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

async function settle() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Roots />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

async function addPath(host: HTMLElement, path: string) {
  const box = host.querySelector('input[name="path"]') as HTMLInputElement
  await act(async () => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, path)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
  const form = host.querySelector('form') as HTMLFormElement
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await Promise.resolve()
    await Promise.resolve()
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [root()] })
  // Each `RootRow` asks the status route once on mount — see its own doc
  // comment. Without an answer the mock resolves `undefined` and the `.then`
  // throws inside every test that renders a row.
  vi.mocked(config.scanStatus).mockResolvedValue({
    root_id: 3,
    running: false,
    report: null,
    last_scan_at: null,
    last_scan_status: null,
  })
  // Without this `patchRoot` resolves `undefined`, the save's `.then` never
  // runs, and no reload happens — so the "keeps an add refusal" test below
  // passes without ever exercising the path it is about.
  vi.mocked(config.patchRoot).mockResolvedValue(undefined)
})

describe('Roots', () => {
  it('lists the roots the server sent', async () => {
    const host = await settle()
    expect(host.textContent).toContain('/srv/manga')
  })

  it('draws the server refusal for a nested root, in both directions', async () => {
    // Spec §3.2: the screen renders the server's own sentence and adds
    // nothing. A client that guessed at nesting would be a second
    // implementation of a check whose whole point is that it runs as the
    // server process.
    vi.mocked(config.addRoot).mockRejectedValue(
      new Error(
        '/api/library/roots: 400 /srv/manga/sub sits inside the existing root /srv/manga',
      ),
    )
    const host = await settle()
    await addPath(host, '/srv/manga/sub')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'sits inside the existing root',
    )
  })

  it('draws the refusal for a path the server cannot read', async () => {
    vi.mocked(config.addRoot).mockRejectedValue(
      new Error('/api/library/roots: 400 /mnt/remote cannot be read'),
    )
    const host = await settle()
    await addPath(host, '/mnt/remote')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'cannot be read',
    )
  })

  it('shows the watch probe for a root it just added', async () => {
    // The third validation, and the only one that produces something on a
    // *successful* add. C4 §2 wants the regime known before the first scan.
    const added: config.Added = {
      root: root({ id: 9, path: '/mnt/remote/manga' }),
      watching: false,
      watch_reason: 'inotify limit reached',
      interval_secs: 900,
    }
    vi.mocked(config.addRoot).mockResolvedValue(added)
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [root(), added.root],
    })
    const host = await settle()
    await addPath(host, '/mnt/remote/manga')
    expect(host.textContent).toContain('inotify limit reached')
    expect(host.textContent).toContain('15m')
  })

  it('reloads the list after an add', async () => {
    vi.mocked(config.addRoot).mockResolvedValue({
      root: root({ id: 9 }),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    const host = await settle()
    const before = vi.mocked(config.listRoots).mock.calls.length
    await addPath(host, '/srv/other')
    expect(vi.mocked(config.listRoots).mock.calls.length).toBeGreaterThan(
      before,
    )
  })

  it('draws the 403 sentence rather than a bare status', async () => {
    vi.mocked(config.listRoots).mockRejectedValue(
      new Error(
        '/api/library/roots: 403 configuring the library is an administrator’s',
      ),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'an administrator',
    )
  })

  it('says a library with no roots has none, rather than drawing nothing', async () => {
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
    const host = await settle()
    expect(host.textContent).toContain('No roots')
  })

  // --- whole-branch review, 2026-09-04 -----------------------------------

  it('does not hand a new root the probe of a removed one that shared its id', async () => {
    // **Review finding 6.** `root.id` is `INTEGER PRIMARY KEY` with no
    // `AUTOINCREMENT`, so SQLite reissues a freed id — `delete_root`'s own
    // MINOR 6 comment says so, and the server clears its `state.scans` slot
    // for exactly this reason. A probe map keyed by id did not, so a new root
    // handed a recycled id inherited the removed root's watch state.
    vi.mocked(config.addRoot).mockResolvedValue({
      root: root({ id: 9, path: '/srv/first' }),
      watching: false,
      watch_reason: 'inotify limit reached',
      interval_secs: 900,
    })
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [root({ id: 9, path: '/srv/first' })],
    })
    const host = await settle()
    await addPath(host, '/srv/first')
    expect(host.textContent).toContain('inotify limit reached')

    // The same id, a different path — what a removal plus an add produces.
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [root({ id: 9, path: '/srv/second' })],
    })
    await act(async () => {
      vi.mocked(config.listRoots).mock.calls.length
      await Promise.resolve()
    })
    const rows = [...host.querySelectorAll('.root-row')]
    const second = rows.find((r) => r.textContent?.includes('/srv/second'))
    expect(second?.textContent ?? '').not.toContain('inotify limit reached')
  })

  it('drops the add banner once that root is gone from the list', async () => {
    // **Review finding 7.** Nothing cleared `justAdded`, so the page went on
    // announcing a root that had since been removed — and a later *failed*
    // add drew the stale success banner directly above the new refusal.
    const added = root({ id: 9, path: '/srv/other' })
    vi.mocked(config.addRoot).mockResolvedValue({
      root: added,
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [root(), added] })
    const host = await settle()
    await addPath(host, '/srv/other')
    expect(host.querySelector('.probe')).not.toBeNull()

    // The root is removed; the next list no longer holds it.
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [root()] })
    vi.mocked(config.addRoot).mockRejectedValue(new Error('400 nope'))
    await addPath(host, '/srv/third')
    expect(host.querySelector('.probe')).toBeNull()
  })

  it('sends one POST for a double-submitted add', async () => {
    // **Review pass 2, finding 3 — the same class as `RootRow.begin`'s, which
    // the first fix round closed in one of the three places it occurs.**
    // `disabled={adding}` is state-driven, so two submits dispatched before
    // the next render both read `adding === false`. The second POST passes
    // `add_root_checked` (the first has not committed on its own connection
    // yet) and then trips `path TEXT NOT NULL UNIQUE`, so it answers 500 and
    // renders raw SQLite text above the success banner for the root that was
    // just added.
    vi.mocked(config.addRoot).mockResolvedValue({
      root: root({ id: 9 }),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    const host = await settle()
    const box = host.querySelector('input[name="path"]') as HTMLInputElement
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, '/srv/other')
      box.dispatchEvent(new Event('input', { bubbles: true }))
    })
    const form = host.querySelector('form') as HTMLFormElement
    await act(async () => {
      form.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      form.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(config.addRoot).mock.calls).toHaveLength(1)
  })

  it('keeps an add refusal on screen when another row reloads the list', async () => {
    // **Review pass 2, finding 5.** One `failure` slot served both the list
    // load and the add form, and `reload().then` clears it unconditionally —
    // so a scan finishing in another row, a field save, or a removal wiped the
    // one message that names *which* of the three validations refused the add,
    // leaving an unexplained empty form.
    vi.mocked(config.addRoot).mockRejectedValue(
      new Error('400 /srv/manga/sub is inside existing root /srv/manga'),
    )
    const host = await settle()
    await addPath(host, '/srv/manga/sub')
    expect(host.textContent).toContain('is inside existing root')

    // Whatever a row does next reloads the list.
    await act(async () => {
      const row = host.querySelector('.root-row') as HTMLElement
      const box = row.querySelector(
        'input[name="default_kind"]',
      ) as HTMLInputElement
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'comic')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('is inside existing root')
  })

  it('keeps the probe on screen when the reload after an add fails', async () => {
    // **Pass 3, finding 4.** The banner was derived from the reloaded list,
    // so a reload that rejected left `roots` at the pre-add value, the
    // predicate false, and the probe answer lost outright — and nothing
    // re-probes, so it is the only chance to know the regime "before the
    // first scan", which is what C4 §2 asks for.
    vi.mocked(config.addRoot).mockResolvedValue({
      root: root({ id: 9, path: '/srv/other' }),
      watching: false,
      watch_reason: 'inotify limit reached',
      interval_secs: 900,
    })
    const host = await settle()
    vi.mocked(config.listRoots).mockRejectedValue(
      new Error('500 database is locked'),
    )
    await addPath(host, '/srv/other')
    expect(host.textContent).toContain('inotify limit reached')
  })

  // --- whole-branch review pass 4, 2026-09-04 ----------------------------

  it('does not reuse a card across two roots that shared an id', async () => {
    // **Pass 4, finding 1 — the same SQLite id-reuse hazard the `probes` map
    // was re-keyed for, left on the row key itself.** React matches on `key`,
    // so a recycled id makes it *reuse the RootRow instance*: its report,
    // scan notice, running flag and four `defaultValue` inputs all survive
    // into a different root, and the mount read (deps `[root.id]`) does not
    // re-run, so a scan actually running on the new root is never adopted.
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [root({ id: 3, path: '/srv/first', edition_key: 'first' })],
    })
    const host = await settle()
    expect(
      (host.querySelector('input[name="edition_key"]') as HTMLInputElement)
        .value,
    ).toBe('first')

    // The id is reissued to a different root.
    vi.mocked(config.listRoots).mockResolvedValue({
      roots: [root({ id: 3, path: '/srv/second', edition_key: 'second' })],
    })
    await act(async () => {
      const box = host.querySelector(
        'input[name="default_kind"]',
      ) as HTMLInputElement
      box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'comic')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('/srv/second')
    expect(
      (host.querySelector('input[name="edition_key"]') as HTMLInputElement)
        .value,
    ).toBe('second')
  })

  it('drops the add banner once a successful list says the root is gone', async () => {
    // **Pass 4, finding 6.** Pass 3 added a `failure !== undefined` disjunct
    // so a failed reload would not discard the probe — which made the
    // predicate unconditionally true after *any* later failure, so the banner
    // came back for a root that had since been removed.
    const added = root({ id: 9, path: '/srv/other' })
    vi.mocked(config.addRoot).mockResolvedValue({
      root: added,
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [root(), added] })
    const host = await settle()
    await addPath(host, '/srv/other')
    expect(host.querySelector('.probe')).not.toBeNull()

    // A successful list without it: the root is genuinely gone.
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [root()] })
    await act(async () => {
      const box = host.querySelector(
        'input[name="default_kind"]',
      ) as HTMLInputElement
      box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'comic')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.probe')).toBeNull()

    // And a later failure must not bring it back.
    vi.mocked(config.listRoots).mockRejectedValue(new Error('500 locked'))
    await act(async () => {
      const box = host.querySelector(
        'input[name="default_kind"]',
      ) as HTMLInputElement
      box.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
      const setter = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set
      setter?.call(box, 'novel')
      box.dispatchEvent(new Event('input', { bubbles: true }))
      box.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.probe')).toBeNull()
  })

  it('ignores a stale list that lands after a newer one', async () => {
    // **Pass 4, finding 7.** `reload` is every row's `onChanged`, so several
    // `listRoots` calls are routinely in flight; nothing sequenced them, so an
    // earlier response landing later wrote the stale array and a just-removed
    // root reappeared. `Search.tsx` carries the same counter for the same
    // reason.
    const slow = root({ id: 1, path: '/srv/stale' })
    const fresh = root({ id: 2, path: '/srv/fresh' })
    let releaseSlow: (v: { roots: config.Root[] }) => void = () => {}
    vi.mocked(config.listRoots)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          releaseSlow = resolve
        }),
      )
      .mockResolvedValue({ roots: [fresh] })

    vi.mocked(config.addRoot).mockResolvedValue({
      root: fresh,
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    })

    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(<Roots />)
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')

    // The mount's own `listRoots` is still pending. An add now starts a
    // second one, which resolves first.
    await addPath(host, '/srv/fresh')
    expect(host.textContent).toContain('/srv/fresh')

    // The first, slower response lands afterwards and must be ignored.
    await act(async () => {
      releaseSlow({ roots: [slow] })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).not.toContain('/srv/stale')
    expect(host.textContent).toContain('/srv/fresh')
  })

  it('gives the add-form box its label’s words as its aria-label — O175', async () => {
    const host = await settle()
    // The form's own box, not the card above it: `RootRow.test.tsx` holds
    // the card's. Counted, because a loop over nothing passes.
    const labelled = fields(host.querySelectorAll('form.add-root label.field'))
    expect(labelled).toHaveLength(1)
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })
})
