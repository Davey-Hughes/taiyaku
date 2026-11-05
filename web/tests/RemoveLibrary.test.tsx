import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from '../src/config'
import * as api from '../src/libraries'
import { RemoveLibrary } from '../src/RemoveLibrary'
import { deferred, flush } from './support/async'
import { emptyDerive } from './support/derive'
import { holdOf } from './support/held'
import { press } from './support/input'
import { mount, rerender } from './support/mount'
import {
  holdTheClock,
  pass,
  sentenceDrawn,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryRemovalCost: vi.fn(),
  removeLibrary: vi.fn(),
}))

const dir = (id: number, path: string): api.LibraryDir => ({
  id,
  path,
  edition_key: null,
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: null,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  nested: [],
  inside: [],
})

const library: api.Library = {
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  dirs: [dir(1, '/media/Manga/Volumes'), dir(2, '/media/Manga/Chapters')],
}

const answer = (over: Partial<config.Removal> = {}): config.RemovalAnswer => ({
  removed: {
    works: 12,
    positions: 1,
    notes: 0,
    left_with_no_library: [],
    ...over,
  },
  derive: emptyDerive,
  dry_run: true,
})

async function shown(onRemoved = vi.fn(), onCancel = vi.fn()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RemoveLibrary
        library={library}
        onCancel={onCancel}
        onRemoved={onRemoved}
      />,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.libraryRemovalCost).mockResolvedValue(answer())
})

describe('RemoveLibrary', () => {
  it('states what goes before it offers to remove anything', async () => {
    const held = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValue(held.promise)
    const host = await shown()
    expect(host.querySelector('.confirm')).toBeNull()
    expect(host.textContent).toContain('Working out what this would remove')

    await flush(() => held.resolve(answer()))

    expect(host.querySelector('.counts')?.textContent).toBe(
      'Removing Manga removes its 2 directories, 12 works, 1 in-progress position and 0 notes.',
    )
    expect(host.querySelector('.confirm')?.textContent).toBe('Remove Manga')
    expect(api.libraryRemovalCost).toHaveBeenCalledWith(7)
  })

  it('says a removal takes only what no other library holds', async () => {
    const host = await shown()
    expect(host.querySelector('.irreversible')?.textContent).toBe(
      "Nothing here can be undone from this screen. The files on disk are untouched; what goes is everything derived from files no other library's directories hold, and everything anybody recorded against a work that goes.",
    )
  })

  it('draws the rehearsal’s report headings one level beneath the library’s name', async () => {
    // The dialog sits in a card whose heading is an h3.
    const host = await shown()
    const headings = [
      ...host.querySelectorAll('.removal-derive .report-heading'),
    ]
    expect(headings.map((h) => h.textContent)).toEqual([
      'What derivation made of every library',
    ])
    expect(headings.map((h) => h.tagName)).toEqual(['H4'])
  })

  it('names the accounts left with no library under the counts, and still offers the confirm', async () => {
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(
      answer({ left_with_no_library: ['sam'] }),
    )
    const host = await shown()
    expect(host.querySelector('.left-with-no-library')?.textContent).toBe(
      'sam can see only this library. After this, sam will see no library until an administrator grants one.',
    )
    expect(host.querySelector('.counts + .left-with-no-library')).not.toBeNull()
    expect(host.querySelector('.confirm')?.textContent).toBe('Remove Manga')
    expect(holdOf(host.querySelector('.confirm') as Element)).toBe('free')
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('names nobody where no account is left with no library', async () => {
    const host = await shown()
    expect(host.querySelector('.counts')).not.toBeNull()
    expect(host.querySelector('.left-with-no-library')).toBeNull()
    expect(host.textContent).not.toContain('will see no library')
  })

  it('draws a refused dry run as the server’s sentence and offers no confirm', async () => {
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      new Error(
        '/api/library/libraries/7?dry_run=true: 409 {"error":"a scan of /media/Manga/Volumes is running; remove the library once it has finished"}',
      ),
    )
    const host = await shown()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'a scan of /media/Manga/Volumes is running; remove the library once it has finished',
    )
    expect(host.querySelector('.counts')).toBeNull()
    expect(host.querySelector('.confirm')).toBeNull()
    expect(host.textContent).not.toContain('Working out')
    const cancel = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    expect(holdOf(cancel)).toBe('free')
  })

  it('sends one removal for two presses, and hands back what it did', async () => {
    const real = { ...answer({ works: 11 }), dry_run: false }
    const held = deferred<config.RemovalAnswer>()
    vi.mocked(api.removeLibrary).mockReturnValue(held.promise)
    const onRemoved = vi.fn()
    const host = await shown(onRemoved)
    const confirm = host.querySelector('.confirm') as HTMLButtonElement

    act(() => {
      confirm.click()
      confirm.click()
    })
    await flush(() => held.resolve(real))

    expect(api.removeLibrary).toHaveBeenCalledTimes(1)
    expect(onRemoved).toHaveBeenCalledWith(real)
  })

  it('keeps the dialog and says why when the removal fails', async () => {
    vi.mocked(api.removeLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 500 the disk is full'),
    )
    const onRemoved = vi.fn()
    const host = await shown(onRemoved)
    await press(host.querySelector('.confirm') as Element)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'the disk is full',
    )
    expect(onRemoved).not.toHaveBeenCalled()
    expect(holdOf(host.querySelector('.confirm') as Element)).toBe('free')
  })

  it('cannot be cancelled while the removal is in flight, from the send, and draws Cancel held once the wait has lasted', async () => {
    holdTheClock()
    vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
    const onCancel = vi.fn()
    const host = await shown(vi.fn(), onCancel)
    const cancel = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    expect(holdOf(cancel)).toBe('free')
    await press(host.querySelector('.confirm') as Element)
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(holdOf(cancel)).toBe('free')
    await press(cancel)
    expect(onCancel).not.toHaveBeenCalled()
    pass(1)
    expect(holdOf(cancel)).toBe('held')
    await press(cancel)
    expect(onCancel).not.toHaveBeenCalled()
  })

  describe('its waits, drawn only once each has lasted', () => {
    const WORKING = 'Working out what this would remove…'
    const asking = async () => {
      const cost = deferred<config.RemovalAnswer>()
      vi.mocked(api.libraryRemovalCost).mockReturnValue(cost.promise)
      return { host: await shown(), cost }
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
      const buttons = [...host.querySelectorAll('button')]
      const confirm = host.querySelector('.confirm') as HTMLButtonElement
      const cancel = buttons.find(
        (b) => b.textContent === 'Cancel',
      ) as HTMLButtonElement
      return [confirm.textContent, holdOf(confirm), holdOf(cancel)]
    }

    it('draws no wait and no held button for a removal refused inside the wait’s delay', async () => {
      holdTheClock()
      const sent = deferred<config.RemovalAnswer>()
      vi.mocked(api.removeLibrary).mockReturnValue(sent.promise)
      const host = await shown()
      const drawn = watchDrawn(() => JSON.stringify(state(host)))
      await press(host.querySelector('.confirm') as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      await flush(() =>
        sent.reject(
          new Error(
            '/api/library/libraries/7: 500 {"error":"the disk is full"}',
          ),
        ),
      )
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('[role="alert"]')?.textContent).toBe(
        'the disk is full',
      )
      expect([...drawn.seen]).toEqual([
        JSON.stringify(['Remove Manga', 'free', 'free']),
      ])
    })

    it('draws Removing… and holds Remove and Cancel from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
      const host = await shown()
      await press(host.querySelector('.confirm') as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Remove Manga', 'free', 'free'])
      pass(1)
      expect(state(host)).toEqual(['Removing…', 'held', 'held'])
      await press(host.querySelector('.confirm') as Element)
      expect(api.removeLibrary).toHaveBeenCalledTimes(1)
    })

    it('sends one removal for a second press inside the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
      const host = await shown()
      await press(host.querySelector('.confirm') as Element)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(state(host)).toEqual(['Remove Manga', 'free', 'free'])
      await press(host.querySelector('.confirm') as Element)
      expect(api.removeLibrary).toHaveBeenCalledTimes(1)
    })
  })

  it('draws the real removal’s failure as the server’s sentence', async () => {
    vi.mocked(api.removeLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 500 {"error":"the disk is full"}'),
    )
    const host = await shown()
    await press(host.querySelector('.confirm') as Element)
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'the disk is full',
    )
  })

  it('holds no failure while the next removal is in flight', async () => {
    vi.mocked(api.removeLibrary)
      .mockRejectedValueOnce(
        new Error('/api/library/libraries/7: 500 {"error":"the disk is full"}'),
      )
      .mockReturnValueOnce(new Promise(() => {}))
    const host = await shown()
    await press(host.querySelector('.confirm') as Element)
    expect(host.querySelector('[role="alert"]')).not.toBeNull()
    await press(host.querySelector('.confirm') as Element)
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('holds its confirm and sends nothing while a directory is busy', async () => {
    const host = await shown()
    await act(async () => {
      rerender(
        <RemoveLibrary
          library={library}
          blocked
          onCancel={vi.fn()}
          onRemoved={vi.fn()}
        />,
      )
      await Promise.resolve()
    })
    const confirm = host.querySelector('.confirm') as HTMLButtonElement
    expect(holdOf(confirm)).toBe('held')
    confirm.click()
    expect(api.removeLibrary).not.toHaveBeenCalled()
  })

  it('asks again when a hold ends, and offers no confirm over the figures from before it', async () => {
    const show = (blocked: boolean) => (
      <RemoveLibrary
        library={library}
        blocked={blocked}
        onCancel={vi.fn()}
        onRemoved={vi.fn()}
      />
    )
    const host = await shown()
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(1)
    await act(async () => {
      rerender(show(true))
      await Promise.resolve()
    })
    // Held: the figures stand, and nothing is asked.
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.counts')).not.toBeNull()
    expect(holdOf(host.querySelector('.confirm') as Element)).toBe('held')

    const second = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(second.promise)
    await act(async () => {
      rerender(show(false))
      await Promise.resolve()
    })
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.counts')).toBeNull()
    expect(host.querySelector('.confirm')).toBeNull()

    await flush(() => second.resolve(answer({ works: 4 })))
    expect(host.querySelector('.counts')?.textContent).toContain('4 works')
    expect(holdOf(host.querySelector('.confirm') as Element)).toBe('free')
  })

  it('keeps Cancel live while its confirm is held', async () => {
    const onCancel = vi.fn()
    const host = await shown(vi.fn(), onCancel)
    await act(async () => {
      rerender(
        <RemoveLibrary
          library={library}
          blocked
          onCancel={onCancel}
          onRemoved={vi.fn()}
        />,
      )
      await Promise.resolve()
    })
    expect(holdOf(host.querySelector('.confirm') as Element)).toBe('held')
    const cancel = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    expect(holdOf(cancel)).toBe('free')
    await press(cancel)
    expect(onCancel).toHaveBeenCalledTimes(1)
  })

  describe('when a hold comes and goes', () => {
    const show = (blocked: boolean) => (
      <RemoveLibrary
        library={library}
        blocked={blocked}
        onCancel={vi.fn()}
        onRemoved={vi.fn()}
      />
    )
    const holdAndRelease = async () => {
      for (const blocked of [true, false]) {
        await act(async () => {
          rerender(show(blocked))
          await Promise.resolve()
        })
      }
    }

    it('does not ask again while its own removal is out', async () => {
      holdTheClock()
      vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
      const host = await shown()
      await press(host.querySelector('.confirm') as Element)
      pass(WAIT_DRAWN_AFTER_MS)
      expect(host.querySelector('.confirm')?.textContent).toBe('Removing…')
      await holdAndRelease()
      expect(api.libraryRemovalCost).toHaveBeenCalledTimes(1)
      expect(host.querySelector('.confirm')?.textContent).toBe('Removing…')
      expect(host.textContent).not.toContain('Working out')
    })

    it('asks again after a refusal when a hold ends, and draws the new figures over a live confirm', async () => {
      vi.mocked(api.removeLibrary).mockRejectedValue(
        new Error('/api/library/libraries/7: 500 {"error":"the disk is full"}'),
      )
      const host = await shown()
      await press(host.querySelector('.confirm') as Element)
      expect(host.querySelector('[role="alert"]')?.textContent).toBe(
        'the disk is full',
      )
      expect(host.querySelector('.counts')?.textContent).toContain('12 works')

      vi.mocked(api.libraryRemovalCost).mockResolvedValue(answer({ works: 99 }))
      await holdAndRelease()
      expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
      expect(host.querySelector('.counts')?.textContent).toContain('99 works')
      expect(holdOf(host.querySelector('.confirm') as Element)).toBe('free')
    })
  })

  it('asks again when the library’s directories change, and draws no count beside another asking’s', async () => {
    const second = deferred<config.RemovalAnswer>()
    const host = await shown()
    expect(host.querySelector('.counts')?.textContent).toContain(
      '2 directories, 12 works',
    )
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(second.promise)
    await act(async () => {
      rerender(
        <RemoveLibrary
          library={{ ...library, dirs: [library.dirs[1]] }}
          onCancel={vi.fn()}
          onRemoved={vi.fn()}
        />,
      )
      await Promise.resolve()
    })
    expect(api.libraryRemovalCost).toHaveBeenCalledTimes(2)
    // Neither the old counts nor a confirm for them while the new are out.
    expect(host.querySelector('.counts')).toBeNull()
    expect(host.querySelector('.confirm')).toBeNull()
    expect(host.textContent).toContain('Working out what this would remove')

    await flush(() => second.resolve(answer({ works: 4 })))
    expect(host.querySelector('.counts')?.textContent).toBe(
      'Removing Manga removes its 1 directory, 4 works, 1 in-progress position and 0 notes.',
    )
  })

  it('draws no count from an asking that was answered after the directories changed again', async () => {
    const first = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(first.promise)
    const host = await shown()
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(answer({ works: 4 }))
    await act(async () => {
      rerender(
        <RemoveLibrary
          library={{ ...library, dirs: [library.dirs[1]] }}
          onCancel={vi.fn()}
          onRemoved={vi.fn()}
        />,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    await flush(() => first.resolve(answer({ works: 99 })))
    expect(host.querySelector('.counts')?.textContent).toContain('4 works')
  })
})
