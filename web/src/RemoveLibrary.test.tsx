import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from './config'
import * as api from './libraries'
import { RemoveLibrary } from './RemoveLibrary'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press } from './testing/input'
import { mount, rerender } from './testing/mount'

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  libraryRemovalCost: vi.fn(),
  removeLibrary: vi.fn(),
}))

const folder = (id: number, path: string): config.Root => ({
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
})

const library: api.Library = {
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  folders: [
    folder(1, '/media/Manga/Volumes'),
    folder(2, '/media/Manga/Chapters'),
  ],
}

const answer = (over: Partial<config.Removal> = {}): config.RemovalAnswer => ({
  removed: { works: 12, positions: 1, notes: 0, ...over },
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
      'Removing Manga removes its 2 folders, 12 works, 1 in-progress position and 0 notes.',
    )
    expect(host.querySelector('.confirm')?.textContent).toBe('Remove Manga')
    expect(api.libraryRemovalCost).toHaveBeenCalledWith(7)
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

  it("draws the refusal's sentence and offers no confirm", async () => {
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      new Error(
        '/api/library/libraries/7?dry_run=true: 409 {"error":"`Manga` is the only library `sam` is granted."}',
      ),
    )
    const host = await shown()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '`Manga` is the only library `sam` is granted.',
    )
    expect(host.querySelector('.confirm')).toBeNull()
    expect(host.textContent).not.toContain('Working out')
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
    expect((host.querySelector('.confirm') as HTMLButtonElement).disabled).toBe(
      false,
    )
  })

  it('cannot be cancelled while the removal is in flight', async () => {
    vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
    const host = await shown()
    const cancel = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    expect(cancel.disabled).toBe(false)
    await press(host.querySelector('.confirm') as Element)
    expect(cancel.disabled).toBe(true)
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

  it('holds its confirm and sends nothing while a folder is busy', async () => {
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
    expect(confirm.disabled).toBe(true)
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
    expect((host.querySelector('.confirm') as HTMLButtonElement).disabled).toBe(
      true,
    )

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
    expect((host.querySelector('.confirm') as HTMLButtonElement).disabled).toBe(
      false,
    )
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
    expect((host.querySelector('.confirm') as HTMLButtonElement).disabled).toBe(
      true,
    )
    const cancel = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Cancel',
    ) as HTMLButtonElement
    expect(cancel.disabled).toBe(false)
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
      vi.mocked(api.removeLibrary).mockReturnValue(new Promise(() => {}))
      const host = await shown()
      await press(host.querySelector('.confirm') as Element)
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
      expect(
        (host.querySelector('.confirm') as HTMLButtonElement).disabled,
      ).toBe(false)
    })
  })

  it('asks again when the library’s folders change, and draws no count beside another asking’s', async () => {
    const second = deferred<config.RemovalAnswer>()
    const host = await shown()
    expect(host.querySelector('.counts')?.textContent).toContain(
      '2 folders, 12 works',
    )
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(second.promise)
    await act(async () => {
      rerender(
        <RemoveLibrary
          library={{ ...library, folders: [library.folders[1]] }}
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
      'Removing Manga removes its 1 folder, 4 works, 1 in-progress position and 0 notes.',
    )
  })

  it('draws no count from an asking that was answered after the folders changed again', async () => {
    const first = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValueOnce(first.promise)
    const host = await shown()
    vi.mocked(api.libraryRemovalCost).mockResolvedValue(answer({ works: 4 }))
    await act(async () => {
      rerender(
        <RemoveLibrary
          library={{ ...library, folders: [library.folders[1]] }}
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
