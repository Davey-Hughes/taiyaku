import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from './config'
import * as api from './libraries'
import { MoveRoot } from './MoveRoot'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press, typeInto } from './testing/input'
import { mount, rerender } from './testing/mount'

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  moveCost: vi.fn(),
  moveFolder: vi.fn(),
}))

const root: config.Root = {
  id: 3,
  path: '/media/Manga/Chapters',
  edition_key: 'chapters',
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: null,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
}

const answer = (
  over: Partial<api.Moved> = {},
  dry_run = true,
): api.MoveAnswer => ({
  moved: {
    library_id: 7,
    library: 'Manga',
    works_kept: 12,
    works_joined: 3,
    works_split: 0,
    series_joined: 2,
    positions: 5,
    notes: 1,
    alignments_dropped: 0,
    library_removed: null,
    grants_removed: 0,
    ...over,
  },
  // The rehearsal and the move find different things: a report kept from
  // the move can only have come from the move.
  derive: { ...emptyDerive, lost_positions: dry_run ? 2 : 6 },
  dry_run,
})

function shown(onMoved = vi.fn(), onCancel = vi.fn()) {
  const host = mount(
    <MoveRoot
      root={root}
      from={{ id: 5, name: 'Chapters' }}
      targets={[
        { id: 7, name: 'Manga' },
        { id: 8, name: 'Comics' },
      ]}
      onCancel={onCancel}
      onMoved={onMoved}
    />,
  )
  return { host, onMoved, onCancel }
}

const target = (host: HTMLElement) =>
  host.querySelector('select[name="move-target"]') as HTMLSelectElement
const see = (host: HTMLElement) =>
  host.querySelector('button[name="rehearse"]') as HTMLButtonElement
const confirm = (host: HTMLElement) =>
  host.querySelector('button.confirm') as HTMLButtonElement | null

async function choose(host: HTMLElement, value: string) {
  await act(async () => {
    const box = target(host)
    Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set?.call(box, value)
    box.dispatchEvent(new Event('change', { bubbles: true }))
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.moveCost).mockResolvedValue(answer())
})

describe('MoveRoot', () => {
  it('offers every other library and a new one, and moves nothing unasked', () => {
    const { host } = shown()
    expect(
      host.querySelector('[role="group"]')?.getAttribute('aria-label'),
    ).toBe('Move the folder at /media/Manga/Chapters')
    expect([...target(host).options].map((o) => o.textContent)).toEqual([
      'Choose a library…',
      'Manga',
      'Comics',
      'A new library…',
    ])
    expect(see(host).disabled).toBe(true)
    expect(confirm(host)).toBeNull()
    expect(api.moveCost).not.toHaveBeenCalled()
  })

  it('says what the move would do before it offers to make it', async () => {
    const { host } = shown()
    await choose(host, '7')
    expect(confirm(host)).toBeNull()
    await press(see(host))

    expect(api.moveCost).toHaveBeenCalledWith(3, { library: 7 })
    const said = host.querySelector('.move-counts')?.textContent ?? ''
    expect(said).toContain('12 works keep their address')
    expect(said).toContain(
      '3 works join a work Manga already holds, as another edition',
    )
    expect(said).toContain('0 works are split between the two libraries')
    expect(said).toContain('5 in-progress positions and 1 note go with them')
    expect(said).toContain('2 series already in Manga gain volumes')
    expect(said).not.toContain('alignment')
    expect(said).not.toContain('is removed')
    expect(confirm(host)?.textContent).toBe(
      'Move /media/Manga/Chapters to Manga',
    )
  })

  it('says what is dropped and what goes with the folder’s last library', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({
        works_kept: 1,
        works_joined: 0,
        works_split: 1,
        series_joined: 0,
        positions: 1,
        notes: 0,
        alignments_dropped: 1,
        library_removed: 'Chapters',
        grants_removed: 2,
      }),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    const said = host.querySelector('.move-counts')?.textContent ?? ''
    expect(said).toContain('1 work keeps its address')
    expect(said).toContain('1 work is split between the two libraries')
    expect(said).toContain('1 in-progress position and 0 notes go with them')
    expect(said).toContain(
      '1 alignment between editions the move separates is dropped',
    )
    expect(said).toContain(
      'Chapters has no other folder, so it is removed, with 2 grants.',
    )
    expect(said).not.toContain('gain volumes')
  })

  it('says one series gains, and leaves out the grants there are none of', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({
        series_joined: 1,
        library_removed: 'Chapters',
        grants_removed: 0,
      }),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    const said = host.querySelector('.move-counts')?.textContent ?? ''
    expect(said).toContain('1 series already in Manga gains volumes.')
    expect(said).toContain('Chapters has no other folder, so it is removed.')
    expect(said).not.toContain('grant')
  })

  it('says a join there is, even where the target is a library the move makes', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ library: 'Extras', library_id: 9, works_joined: 2 }),
    )
    const { host } = shown()
    await choose(host, 'new')
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      'Extras',
    )
    await press(see(host))
    expect(host.querySelector('.move-counts')?.textContent).toContain(
      '2 works join a work Extras already holds',
    )
  })

  it('leaves out the join where the target is a library the move makes', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ library: 'Extras', library_id: 9, works_joined: 0 }),
    )
    const { host } = shown()
    await choose(host, 'new')
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      'Extras',
    )
    await press(see(host))
    const said = host.querySelector('.move-counts')?.textContent ?? ''
    expect(said).toContain(
      'Moving this folder from Chapters to Extras: 12 works keep their address and 0 works are split between the two libraries.',
    )
    expect(said).not.toContain('join')
  })

  it('makes the move once, for the target it rehearsed, and hands back what it did', async () => {
    // Counts that differ from the rehearsal's: the move's own are the ones kept.
    const done = answer({ positions: 9, notes: 4 }, false)
    const held = deferred<api.MoveAnswer>()
    vi.mocked(api.moveFolder).mockReturnValue(held.promise)
    const { host, onMoved } = shown()
    await choose(host, '7')
    await press(see(host))

    act(() => {
      confirm(host)?.click()
      confirm(host)?.click()
    })
    await flush(() => held.resolve(done))

    expect(api.moveFolder).toHaveBeenCalledTimes(1)
    expect(api.moveFolder).toHaveBeenCalledWith(3, { library: 7 })
    expect(onMoved).toHaveBeenCalledWith(done)
  })

  it('takes the rehearsal away when the target changes', async () => {
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(confirm(host)).not.toBeNull()

    await choose(host, '8')

    expect(confirm(host)).toBeNull()
    expect(host.querySelector('.move-counts')).toBeNull()
  })

  it('names a new library, and sends the name', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ library: 'Extras', library_id: 9 }),
    )
    const { host } = shown()
    await choose(host, 'new')
    expect(see(host).disabled).toBe(true)
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      'Extras',
    )
    await press(see(host))
    expect(api.moveCost).toHaveBeenCalledWith(3, { new_library: 'Extras' })
    expect(confirm(host)?.textContent).toBe(
      'Move /media/Manga/Chapters to Extras',
    )
  })

  it("draws a refusal's sentence and its volumes, and offers no confirm", async () => {
    vi.mocked(api.moveCost).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move?dry_run=true: 409 {"error":"`Manga` already holds volumes of /media/Manga/Chapters under the same edition key: A v01 (raw) and 1 more. Nothing was changed","volumes":["A v01 (raw)","B v02 (raw)"]}',
      ),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'under the same edition key',
    )
    expect(
      [...host.querySelectorAll('.move-volumes li')].map(
        (li) => li.textContent,
      ),
    ).toEqual(['A v01 (raw)', 'B v02 (raw)'])
    expect(confirm(host)).toBeNull()
  })

  it('draws the rehearsal asked for last when two answers cross', async () => {
    const slow = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValueOnce(slow.promise)
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    await choose(host, '8')
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ library: 'Comics', library_id: 8 }),
    )
    await press(see(host))
    expect(confirm(host)?.textContent).toBe(
      'Move /media/Manga/Chapters to Comics',
    )

    await flush(() => slow.resolve(answer()))
    expect(confirm(host)?.textContent).toBe(
      'Move /media/Manga/Chapters to Comics',
    )
  })

  it('drops the rehearsal when the move itself is refused, and keeps the refusal', async () => {
    vi.mocked(api.moveFolder).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move: 409 {"error":"`Manga` already holds volumes of /media/Manga/Chapters","volumes":["A v01 (raw)"]}',
      ),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    await press(confirm(host) as Element)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'already holds',
    )
    expect(host.querySelectorAll('.move-volumes li')).toHaveLength(1)
    expect(confirm(host)).toBeNull()
    expect(host.querySelector('.move-counts')).toBeNull()
    expect(host.querySelector('.removal-derive')).toBeNull()
    expect(see(host).disabled).toBe(false)
  })

  it('draws no rehearsal answered after the target changed', async () => {
    const slow = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValueOnce(slow.promise)
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    await choose(host, '8')
    await flush(() => slow.resolve(answer()))
    expect(confirm(host)).toBeNull()
    expect(host.querySelector('.move-counts')).toBeNull()
  })

  it('takes the old list away when the next refusal lists none', async () => {
    const second = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost)
      .mockRejectedValueOnce(
        new Error(
          '/api/library/roots/3/move?dry_run=true: 409 {"error":"x","volumes":["A v01 (raw)"]}',
        ),
      )
      .mockReturnValueOnce(second.promise)
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(host.querySelector('.move-volumes')).not.toBeNull()
    await press(see(host))
    // Gone while the next answer is awaited, not only once it has come.
    expect(host.querySelector('.move-volumes')).toBeNull()
    await flush(() =>
      second.reject(
        new Error(
          '/api/library/roots/3/move?dry_run=true: 409 {"error":"a scan is running","volumes":[]}',
        ),
      ),
    )
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'is running',
    )
    expect(host.querySelector('.move-volumes')).toBeNull()
  })

  it('draws no rehearsal answered after its library left the list', async () => {
    const slow = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValueOnce(slow.promise)
    const targets = [
      { id: 7, name: 'Manga' },
      { id: 8, name: 'Comics' },
    ]
    const dialog = (list: typeof targets) => (
      <MoveRoot
        root={root}
        from={{ id: 5, name: 'Chapters' }}
        targets={list}
        onCancel={vi.fn()}
        onMoved={vi.fn()}
      />
    )
    const host = mount(dialog(targets))
    await choose(host, '7')
    await press(see(host))
    await act(async () => rerender(dialog([targets[1]])))
    await flush(() => slow.resolve(answer()))
    expect(confirm(host)).toBeNull()
    expect(host.querySelector('.move-counts')).toBeNull()
  })

  it('lets go of a choice whose library has gone from the list', async () => {
    const targets = [
      { id: 7, name: 'Manga' },
      { id: 8, name: 'Comics' },
    ]
    const dialog = (list: typeof targets) => (
      <MoveRoot
        root={root}
        from={{ id: 5, name: 'Chapters' }}
        targets={list}
        onCancel={vi.fn()}
        onMoved={vi.fn()}
      />
    )
    const host = mount(dialog(targets))
    await choose(host, '7')
    await press(see(host))
    expect(confirm(host)).not.toBeNull()
    await act(async () => rerender(dialog([targets[1]])))
    expect(target(host).value).toBe('')
    expect(confirm(host)).toBeNull()
    expect(host.querySelector('.move-counts')).toBeNull()
    expect(see(host).disabled).toBe(true)
  })

  it('keeps the dialog and says why when the move itself fails', async () => {
    vi.mocked(api.moveFolder).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move: 409 {"error":"a scan or a rule save is running; move the folder when it has finished","volumes":[]}',
      ),
    )
    const { host, onMoved } = shown()
    await choose(host, '7')
    await press(see(host))
    await press(confirm(host) as Element)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'is running',
    )
    expect(onMoved).not.toHaveBeenCalled()
    expect(host.querySelector('.move-volumes')).toBeNull()
  })

  it('holds the rehearsal while no name is typed, and while a spaces-only one is', async () => {
    const { host } = shown()
    await choose(host, 'new')
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      '   ',
    )
    expect(see(host).disabled).toBe(true)
  })

  it('holds the rehearsal button, and says why, while the answer is awaited', async () => {
    const slow = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValue(slow.promise)
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(see(host).disabled).toBe(true)
    expect(see(host).textContent).toBe('Working out what this would do…')
    await flush(() => slow.resolve(answer()))
    expect(see(host).disabled).toBe(false)
    expect(see(host).textContent).toBe('See what this would do')
  })

  it('draws a list that names one volume twice, once each', async () => {
    vi.mocked(api.moveCost).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move?dry_run=true: 409 {"error":"x","volumes":["A v01 (raw)","A v01 (raw)"]}',
      ),
    )
    // React reports a repeated key by `console.error`.
    const complaint = vi.spyOn(console, 'error').mockImplementation(() => {})
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(host.querySelectorAll('.move-volumes li')).toHaveLength(2)
    expect(complaint).not.toHaveBeenCalled()
    complaint.mockRestore()
  })

  describe('when something else in the library begins', () => {
    const dialog = (held: boolean, onCancel = vi.fn()) => (
      <MoveRoot
        root={root}
        from={{ id: 5, name: 'Chapters' }}
        targets={[{ id: 7, name: 'Manga' }]}
        held={held}
        onCancel={onCancel}
        onMoved={vi.fn()}
      />
    )

    it('cancels itself', async () => {
      const onCancel = vi.fn()
      mount(dialog(false, onCancel))
      expect(onCancel).not.toHaveBeenCalled()
      await act(async () => rerender(dialog(true, onCancel)))
      expect(onCancel).toHaveBeenCalledTimes(1)
    })

    it('does not cancel while its own move is out', async () => {
      const onCancel = vi.fn()
      vi.mocked(api.moveFolder).mockReturnValue(new Promise(() => {}))
      const host = mount(dialog(false, onCancel))
      await choose(host, '7')
      await press(see(host))
      await press(confirm(host) as Element)
      await act(async () => rerender(dialog(true, onCancel)))
      expect(onCancel).not.toHaveBeenCalled()
    })
  })
})
