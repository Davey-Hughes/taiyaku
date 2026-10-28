import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from '../src/config'
import * as api from '../src/libraries'
import { MoveRoot } from '../src/MoveRoot'
import { deferred, flush } from './support/async'
import { emptyDerive } from './support/derive'
import './support/dialog'
import { holdOf } from './support/held'
import { press, typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { expectRowsWired, row } from './support/settingsRows'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  moveCost: vi.fn(),
  moveDir: vi.fn(),
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
    left_with_no_library: [],
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
    const dialog = host.querySelector('dialog') as HTMLDialogElement
    expect(dialog.open).toBe(true)
    const title = document.getElementById(
      dialog.getAttribute('aria-labelledby') ?? '',
    )
    expect(title?.tagName).toBe('H2')
    expect(title?.textContent).toBe('Move /media/Manga/Chapters')
    expect([...target(host).options].map((o) => o.textContent)).toEqual([
      'Choose a library…',
      'Manga',
      'Comics',
      'A new library…',
    ])
    expect(holdOf(see(host))).toBe('disabled')
    expect(confirm(host)).toBeNull()
    expect(api.moveCost).not.toHaveBeenCalled()
  })

  it('draws Move to, and Its name once a new library is chosen, as rows of one box wired to their labels — O175', async () => {
    const { host } = shown()
    const dialog = host.querySelector('dialog') as HTMLDialogElement
    const box = dialog.querySelector(':scope > .settings-box') as HTMLElement
    const labels = () =>
      [
        ...box.querySelectorAll(
          ':scope > .settings-row > .settings-row-text > .settings-row-label',
        ),
      ].map((label) => label.textContent)
    expect(labels()).toEqual(['Move to'])
    expect(row(box, 'Move to').querySelector('select')).toBe(target(host))
    expectRowsWired(dialog)

    await choose(host, 'new')
    expect(labels()).toEqual(['Move to', 'Its name'])
    const name = row(box, 'Its name').querySelector(
      'input[name="new-library"]',
    ) as HTMLInputElement
    expect(name).not.toBeNull()
    expectRowsWired(dialog)
    // Each name is its label's words as written.
    expect([
      target(host).getAttribute('aria-label'),
      name.getAttribute('aria-label'),
    ]).toEqual(['Move to', 'Its name'])
  })

  it('draws See what this would do under the fields, what a rehearsal said after it, and the move once rehearsed and Cancel alone in one row at the box’s foot', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ left_with_no_library: ['alice'] }),
    )
    const { host } = shown()
    const box = host.querySelector('dialog > .settings-box') as HTMLElement
    /**
     * The box's parts in order, each by what it is: a row by its label, or
     * by its words where it has none, anything else by its role or class.
     */
    const parts = () =>
      [...box.children].map((part) =>
        part.classList.contains('settings-row')
          ? (part.querySelector('.settings-row-label')?.textContent ??
            part.textContent)
          : (part.getAttribute('role') ?? part.className),
      )
    const foot = () =>
      [...box.querySelectorAll(':scope > .settings-actions > button')].map(
        (button) => button.textContent,
      )
    expect(parts()).toEqual([
      'Move to',
      'See what this would do',
      'settings-actions',
    ])
    expect(foot()).toEqual(['Cancel'])

    await choose(host, '7')
    await press(see(host))
    expect(foot()).toEqual(['Move', 'Cancel'])
    expect(parts()).toEqual([
      'Move to',
      'See what this would do',
      'move-counts',
      'left-with-no-library',
      'removal-derive',
      'settings-actions',
    ])

    vi.mocked(api.moveCost).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move?dry_run=true: 409 {"error":"x","volumes":["A v01"]}',
      ),
    )
    await press(see(host))
    expect(foot()).toEqual(['Cancel'])
    expect(parts()).toEqual([
      'Move to',
      'See what this would do',
      'alert',
      'move-volumes',
      'settings-actions',
    ])
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
    expect(said).toContain('Moving this directory from Chapters to Manga: ')
    expect(confirm(host)?.textContent).toBe('Move')
  })

  it('says what is dropped and what goes with the directory’s last library', async () => {
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
      'Chapters has no other directory, so it is removed, with 2 grants.',
    )
    expect(said).not.toContain('gain volumes')
  })

  it('names the accounts left with no library under the counts, and still offers the confirm', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({
        library_removed: 'Chapters',
        grants_removed: 3,
        left_with_no_library: ['alice', 'bob', 'carol'],
      }),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(host.querySelector('.left-with-no-library')?.textContent).toBe(
      "alice, bob and carol can see only this directory's library. After this, they will see no library until an administrator grants one.",
    )
    expect(
      host.querySelector('.move-counts + .left-with-no-library'),
    ).not.toBeNull()
    expect(confirm(host)?.textContent).toBe('Move')
    expect(holdOf(confirm(host) as Element)).toBe('free')
  })

  it('names nobody where no account is left with no library', async () => {
    vi.mocked(api.moveCost).mockResolvedValue(
      answer({ library_removed: 'Chapters', grants_removed: 2 }),
    )
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    expect(host.querySelector('.move-counts')?.textContent).toContain(
      'Chapters has no other directory, so it is removed, with 2 grants.',
    )
    expect(host.querySelector('.left-with-no-library')).toBeNull()
    expect(host.textContent).not.toContain('will see no library')
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
    expect(said).toContain('Chapters has no other directory, so it is removed.')
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
      'Moving this directory from Chapters to Extras: 12 works keep their address and 0 works are split between the two libraries.',
    )
    expect(said).not.toContain('join')
  })

  it('makes the move once, for the target it rehearsed, and hands back what it did', async () => {
    // Counts that differ from the rehearsal's: the move's own are the ones kept.
    const done = answer({ positions: 9, notes: 4 }, false)
    const held = deferred<api.MoveAnswer>()
    vi.mocked(api.moveDir).mockReturnValue(held.promise)
    const { host, onMoved } = shown()
    await choose(host, '7')
    await press(see(host))

    act(() => {
      confirm(host)?.click()
      confirm(host)?.click()
    })
    await flush(() => held.resolve(done))

    expect(api.moveDir).toHaveBeenCalledTimes(1)
    expect(api.moveDir).toHaveBeenCalledWith(3, { library: 7 })
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
    expect(holdOf(see(host))).toBe('disabled')
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      'Extras',
    )
    await press(see(host))
    expect(api.moveCost).toHaveBeenCalledWith(3, { new_library: 'Extras' })
    expect(host.querySelector('.move-counts')?.textContent).toContain(
      'Moving this directory from Chapters to Extras: ',
    )
    expect(confirm(host)?.textContent).toBe('Move')
  })

  it('draws, under the fields, why a new library cannot be called . or ..', async () => {
    // The server refuses both names, since no address can hold either; the
    // dialog sends the name and draws the server's sentence.
    const why = 'An address cannot hold “.” or “..” as a name.'
    vi.mocked(api.moveCost).mockRejectedValue(
      new Error(
        `/api/library/roots/3/move?dry_run=true: 400 ${JSON.stringify({ error: why })}`,
      ),
    )
    const { host } = shown()
    await choose(host, 'new')
    await typeInto(
      host.querySelector('input[name="new-library"]') as HTMLInputElement,
      '.',
    )
    await press(see(host))
    expect(api.moveCost).toHaveBeenCalledWith(3, { new_library: '.' })
    expect(
      [...host.querySelectorAll('[role="alert"]')].map((a) => [
        a.previousElementSibling?.matches('.move-rehearse'),
        a.textContent,
      ]),
    ).toEqual([[true, why]])
    expect(confirm(host)).toBeNull()
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
    const drawn = () => host.querySelector('.move-counts')?.textContent ?? ''
    expect(drawn()).toContain('Moving this directory from Chapters to Comics: ')
    expect(confirm(host)?.textContent).toBe('Move')

    await flush(() => slow.resolve(answer()))
    expect(drawn()).toContain('Moving this directory from Chapters to Comics: ')
    expect(confirm(host)?.textContent).toBe('Move')
  })

  it('drops the rehearsal when the move itself is refused, and keeps the refusal', async () => {
    vi.mocked(api.moveDir).mockRejectedValue(
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
    expect(holdOf(see(host))).toBe('free')
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
    expect(holdOf(see(host))).toBe('disabled')
  })

  it('keeps the dialog and says why when the move itself fails', async () => {
    vi.mocked(api.moveDir).mockRejectedValue(
      new Error(
        '/api/library/roots/3/move: 409 {"error":"a scan or a rule save is running; move the directory when it has finished","volumes":[]}',
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
    expect(holdOf(see(host))).toBe('disabled')
  })

  it('holds the rehearsal button, and says why, once the answer has been awaited a while', async () => {
    holdTheClock()
    const slow = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValue(slow.promise)
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect([see(host).textContent, holdOf(see(host))]).toEqual([
      'See what this would do',
      'free',
    ])
    pass(1)
    expect([see(host).textContent, holdOf(see(host))]).toEqual([
      'Working out what this would do…',
      'held',
    ])
    await flush(() => slow.resolve(answer()))
    expect(holdOf(see(host))).toBe('free')
    expect(see(host).textContent).toBe('See what this would do')
  })

  it('never draws the rehearsal waiting, nor its button held, for an answer inside the wait’s delay', async () => {
    holdTheClock()
    const quick = deferred<api.MoveAnswer>()
    vi.mocked(api.moveCost).mockReturnValue(quick.promise)
    const { host } = shown()
    await choose(host, '7')
    const drawn = watchDrawn(
      () => `${see(host).textContent} ${holdOf(see(host))}`,
    )
    await press(see(host))
    pass(WAIT_DRAWN_AFTER_MS - 1)
    await flush(() => quick.resolve(answer()))
    pass(WAIT_DRAWN_AFTER_MS)
    drawn.stop()
    expect(confirm(host)).not.toBeNull()
    expect([...drawn.seen]).toEqual(['See what this would do free'])
  })

  it('asks for one rehearsal for two presses inside the wait’s delay', async () => {
    holdTheClock()
    vi.mocked(api.moveCost).mockReturnValue(new Promise(() => {}))
    const { host } = shown()
    await choose(host, '7')
    await press(see(host))
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(holdOf(see(host))).toBe('free')
    await press(see(host))
    expect(api.moveCost).toHaveBeenCalledTimes(1)
  })

  describe('while the move is out', () => {
    /** Rehearsed for a new library, so that every field the move holds is drawn. */
    async function rehearsed() {
      const { host, onCancel } = shown()
      await choose(host, 'new')
      await typeInto(
        host.querySelector('input[name="new-library"]') as HTMLInputElement,
        'Volumes',
      )
      await press(see(host))
      return { host, onCancel }
    }
    const cancel = (host: HTMLElement) =>
      [...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Cancel',
      ) as HTMLButtonElement
    const nameBox = (host: HTMLElement) =>
      host.querySelector('input[name="new-library"]') as HTMLInputElement
    /** Move's words, then how each control the move holds is held. */
    const state = (host: HTMLElement) => [
      confirm(host)?.textContent,
      ...[
        target(host),
        nameBox(host),
        see(host),
        confirm(host),
        cancel(host),
      ].map((el) => (el === null ? undefined : holdOf(el))),
    ]
    const idle = ['Move', 'free', 'free', 'free', 'free', 'free']

    it('draws no wait and holds no control drawn for a move refused inside the wait’s delay', async () => {
      holdTheClock()
      const sent = deferred<api.MoveAnswer>()
      vi.mocked(api.moveDir).mockReturnValue(sent.promise)
      const { host } = await rehearsed()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await press(confirm(host) as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() =>
        sent.reject(
          new Error(
            '/api/library/roots/3/move: 409 {"error":"a scan or a rule save is running; move the directory when it has finished","volumes":[]}',
          ),
        ),
      )
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('[role="alert"]')?.textContent).toContain(
        'is running',
      )
      // The refusal takes the confirm away (`null` once read as JSON); until
      // then nothing was held.
      expect([...drawn.seen].map((one) => JSON.parse(one))).toEqual([
        idle,
        [null, 'free', 'free', 'free', null, 'free'],
      ])
    })

    it('draws Moving… and every control it holds as held from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(api.moveDir).mockReturnValue(new Promise(() => {}))
      const { host } = await rehearsed()
      await press(confirm(host) as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(idle)
      pass(1)
      expect(state(host)).toEqual([
        'Moving…',
        'held',
        'held',
        'held',
        'held',
        'held',
      ])
      expect(nameBox(host).readOnly).toBe(true)
    })

    it('holds the move, Cancel, the fields and the rehearsal from the send, before any is drawn held', async () => {
      holdTheClock()
      vi.mocked(api.moveDir).mockReturnValue(new Promise(() => {}))
      const { host, onCancel } = await rehearsed()
      await press(confirm(host) as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(idle)

      await press(confirm(host) as Element)
      expect(api.moveDir).toHaveBeenCalledTimes(1)
      await press(cancel(host))
      expect(onCancel).not.toHaveBeenCalled()
      await press(see(host))
      expect(api.moveCost).toHaveBeenCalledTimes(1)
      await choose(host, '7')
      expect(target(host).value).toBe('new')
      await typeInto(nameBox(host), 'Chapters')
      expect(nameBox(host).value).toBe('Volumes')
      expect(host.querySelector('.move-counts')).not.toBeNull()
    })

    it('sends nothing and changes nothing from a control the drawn wait holds', async () => {
      holdTheClock()
      vi.mocked(api.moveDir).mockReturnValue(new Promise(() => {}))
      const { host, onCancel } = await rehearsed()
      await press(confirm(host) as Element)
      pass(WAIT_DRAWN_AFTER_MS)
      expect(state(host)).toEqual([
        'Moving…',
        'held',
        'held',
        'held',
        'held',
        'held',
      ])

      await press(confirm(host) as Element)
      expect(api.moveDir).toHaveBeenCalledTimes(1)
      await press(cancel(host))
      expect(onCancel).not.toHaveBeenCalled()
      await press(see(host))
      expect(api.moveCost).toHaveBeenCalledTimes(1)
      await choose(host, '7')
      expect(target(host).value).toBe('new')
      await typeInto(nameBox(host), 'Chapters')
      expect(nameBox(host).value).toBe('Volumes')
      expect(host.querySelector('.move-counts')).not.toBeNull()
    })
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
      vi.mocked(api.moveDir).mockReturnValue(new Promise(() => {}))
      const host = mount(dialog(false, onCancel))
      await choose(host, '7')
      await press(see(host))
      await press(confirm(host) as Element)
      await act(async () => rerender(dialog(true, onCancel)))
      expect(onCancel).not.toHaveBeenCalled()
    })
  })

  /**
   * What the browser asks of the dialog arrives as events, and jsdom sends
   * none of them, so each is dispatched here. What a real Escape does, and
   * that the page behind is inert, is the browser project's.
   */
  describe('as a dialog', () => {
    const escapeKey = () =>
      new KeyboardEvent('keydown', {
        key: 'Escape',
        bubbles: true,
        cancelable: true,
      })
    /** The browser's request to close `dialog`, as Escape sends it. */
    const cancelOf = (dialog: HTMLDialogElement) => {
      const asked = new Event('cancel', { cancelable: true })
      act(() => {
        dialog.dispatchEvent(asked)
      })
      return asked
    }
    /**
     * Sends the move from `host` and leaves it unanswered, short of the
     * wait's delay: what holds the dialog is the send, not the drawn wait.
     */
    async function midMove(host: HTMLElement) {
      holdTheClock()
      vi.mocked(api.moveDir).mockReturnValue(new Promise(() => {}))
      await choose(host, '7')
      await press(see(host))
      await press(confirm(host) as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(api.moveDir).toHaveBeenCalledTimes(1)
      expect(confirm(host)?.textContent).toBe('Move')
      return host.querySelector('dialog') as HTMLDialogElement
    }

    it('asks to be closed when the browser asks to close it, and does not let the browser close it under its owner', () => {
      const { host, onCancel } = shown()
      const asked = cancelOf(host.querySelector('dialog') as HTMLDialogElement)
      expect(onCancel).toHaveBeenCalledTimes(1)
      expect(asked.defaultPrevented).toBe(true)
    })

    it('refuses the browser’s request to close it, and Escape anywhere in the document, while its move is out', async () => {
      const { host, onCancel } = shown()
      const before = escapeKey()
      document.body.dispatchEvent(before)
      expect(before.defaultPrevented).toBe(false)

      const dialog = await midMove(host)
      expect(cancelOf(dialog).defaultPrevented).toBe(true)
      const during = escapeKey()
      document.body.dispatchEvent(during)
      expect(during.defaultPrevented).toBe(true)

      pass(1)
      expect(confirm(host)?.textContent).toBe('Moving…')
      expect(cancelOf(dialog).defaultPrevented).toBe(true)
      const drawn = escapeKey()
      document.body.dispatchEvent(drawn)
      expect(drawn.defaultPrevented).toBe(true)
      expect(onCancel).not.toHaveBeenCalled()
    })

    it('opens again when it is closed while its move is out', async () => {
      const { host, onCancel } = shown()
      const dialog = await midMove(host)
      act(() => {
        dialog.open = false
        dialog.dispatchEvent(new Event('close'))
      })
      expect(dialog.open).toBe(true)
      expect(onCancel).not.toHaveBeenCalled()
    })

    it('lets Escape through again, and asks to be closed, once its move is refused', async () => {
      holdTheClock()
      const sent = deferred<api.MoveAnswer>()
      vi.mocked(api.moveDir).mockReturnValue(sent.promise)
      const { host, onCancel } = shown()
      await choose(host, '7')
      await press(see(host))
      await press(confirm(host) as Element)
      // Drawn mid-move first, so Escape was being refused.
      pass(WAIT_DRAWN_AFTER_MS)
      expect(confirm(host)?.textContent).toBe('Moving…')
      await flush(() =>
        sent.reject(
          new Error(
            '/api/library/roots/3/move: 409 {"error":"a scan or a rule save is running; move the directory when it has finished","volumes":[]}',
          ),
        ),
      )
      expect(host.querySelector('[role="alert"]')?.textContent).toContain(
        'is running',
      )
      const after = escapeKey()
      document.body.dispatchEvent(after)
      expect(after.defaultPrevented).toBe(false)
      cancelOf(host.querySelector('dialog') as HTMLDialogElement)
      expect(onCancel).toHaveBeenCalledTimes(1)
    })
  })
})
