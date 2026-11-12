import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from '../src/config'
import * as api from '../src/libraries'
import { UnlinkRoot } from '../src/UnlinkRoot'
import { deferred } from './support/async'
import { mount, rerender } from './support/mount'

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  unlinkCost: vi.fn(),
  unlinkDir: vi.fn(),
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
}

const MANGA = { id: 7, name: 'Manga' }

const answer = (
  over: Partial<api.Unlinked> = {},
  dry_run = true,
): api.UnlinkAnswer => ({
  unlinked: {
    loses_sight: [],
    library_removed: null,
    grants_removed: 0,
    left_with_no_library: [],
    ...over,
  },
  dry_run,
})

/** The 400 the server answers on a directory's last library. */
const LAST =
  '`/srv/manga` is in no other library, so it can only be removed: use Remove this directory.'
const lastLink = (dry_run: boolean) =>
  new Error(
    `/api/library/libraries/7/dirs/3${dry_run ? '?dry_run=true' : ''}: 400 ${JSON.stringify({ error: LAST })}`,
  )

function shown(
  props: Partial<Parameters<typeof UnlinkRoot>[0]> = {},
): Parameters<typeof UnlinkRoot>[0] {
  return {
    root,
    from: MANGA,
    keptBy: ['Kids'],
    onCancel: vi.fn(),
    onUnlinked: vi.fn(),
    onRefused: vi.fn(),
    onLateRefusal: vi.fn(),
    ...props,
  }
}

async function settle(props = shown()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<UnlinkRoot {...props} />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, props }
}

const confirm = (host: HTMLElement) =>
  host.querySelector('.unlink-root .confirm') as HTMLButtonElement | null

const words = (host: HTMLElement) =>
  [...(host.querySelector('.unlink-root')?.children ?? [])].map((one) =>
    one.className === 'settings-actions'
      ? [...one.querySelectorAll('button')].map((b) => b.textContent)
      : [one.className, one.textContent],
  )

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.unlinkCost).mockResolvedValue(answer())
  vi.mocked(api.unlinkDir).mockResolvedValue(answer({}, false))
})

describe('UnlinkRoot', () => {
  it('asks its rehearsal of the page’s library and the directory, and offers no confirm until it answers', async () => {
    const rehearsal = deferred<api.UnlinkAnswer>()
    vi.mocked(api.unlinkCost).mockReturnValue(rehearsal.promise)
    const { host } = await settle()
    expect(api.unlinkCost).toHaveBeenCalledWith(7, 3)
    expect(confirm(host)).toBeNull()
    expect(api.unlinkDir).not.toHaveBeenCalled()
    await act(async () => {
      rehearsal.resolve(answer())
    })
    expect(confirm(host)).not.toBeNull()
  })

  it('lays its confirm out as every removal’s: what it does, what it keeps, then the confirm and Cancel, and no disclosure', async () => {
    const { host } = await settle()
    expect(words(host)).toEqual([
      [
        'counts',
        'Removing this directory from Manga: no account will lose sight of it.',
      ],
      [
        'kept',
        'Kids keeps the directory, its works and everything recorded against them; none of it is deleted.',
      ],
      ['Remove from Manga', 'Cancel'],
    ])
    expect(host.querySelector('details')).toBeNull()
    expect(
      host.querySelector('[role="group"]')?.getAttribute('aria-label'),
    ).toBe('Remove /srv/manga from Manga')
  })

  it('says who loses sight, that the library goes where this is its last directory, with its grants, and who is left seeing none', async () => {
    vi.mocked(api.unlinkCost).mockResolvedValue(
      answer({
        loses_sight: ['alice', 'bob'],
        library_removed: 'Manga',
        grants_removed: 2,
        left_with_no_library: ['alice'],
      }),
    )
    const { host } = await settle(shown({ keptBy: ['Kids', 'Teens'] }))
    expect(words(host)).toEqual([
      [
        'counts',
        'Removing this directory from Manga: alice and bob will lose sight of it.',
      ],
      [
        'last-dir',
        'Manga has no other directory, so it is removed, with 2 grants.',
      ],
      [
        'left-with-no-library',
        'alice can see only Manga. After this, alice will see no library until an administrator grants one.',
      ],
      [
        'kept',
        'Kids and Teens keep the directory, its works and everything recorded against them; none of it is deleted.',
      ],
      ['Remove from Manga', 'Cancel'],
    ])
  })

  it('names no grants where none go with the library', async () => {
    vi.mocked(api.unlinkCost).mockResolvedValue(
      answer({ library_removed: 'Manga' }),
    )
    const { host } = await settle()
    expect(host.querySelector('.last-dir')?.textContent).toBe(
      'Manga has no other directory, so it is removed.',
    )
  })

  it('sends one unlink for two presses, and hands its answer up', async () => {
    const out = deferred<api.UnlinkAnswer>()
    vi.mocked(api.unlinkDir).mockReturnValue(out.promise)
    const { host, props } = await settle()
    await act(async () => {
      confirm(host)?.click()
      confirm(host)?.click()
    })
    expect(api.unlinkDir).toHaveBeenCalledTimes(1)
    expect(api.unlinkDir).toHaveBeenCalledWith(7, 3)
    await act(async () => {
      out.resolve(answer({}, false))
    })
    expect(props.onUnlinked).toHaveBeenCalledWith(answer({}, false))
    expect(props.onRefused).not.toHaveBeenCalled()
  })

  it('hands a refused rehearsal up as its sentence, which names Remove this directory on the last library, and its status', async () => {
    vi.mocked(api.unlinkCost).mockRejectedValue(lastLink(true))
    const { props } = await settle()
    expect(props.onRefused).toHaveBeenCalledWith(LAST, 400)
  })

  it('hands a refused unlink up while drawn, and to the screen once gone, named for the directory and the library', async () => {
    vi.mocked(api.unlinkDir).mockRejectedValue(lastLink(false))
    const drawn = await settle()
    await act(async () => {
      confirm(drawn.host)?.click()
    })
    expect(drawn.props.onRefused).toHaveBeenCalledWith(LAST, 400)
    expect(drawn.props.onLateRefusal).not.toHaveBeenCalled()

    const out = deferred<api.UnlinkAnswer>()
    vi.mocked(api.unlinkDir).mockReturnValue(out.promise)
    const gone = await settle(shown())
    await act(async () => {
      confirm(gone.host)?.click()
    })
    await act(async () => {
      rerender(<p />)
    })
    await act(async () => {
      out.reject(lastLink(false))
    })
    expect(gone.props.onRefused).not.toHaveBeenCalled()
    expect(gone.props.onLateRefusal).toHaveBeenCalledWith(
      `/srv/manga was not removed from Manga (${LAST}).`,
    )
  })

  it('cancels itself when held, and not while its unlink is out', async () => {
    const quiet = await settle()
    await act(async () => {
      rerender(<UnlinkRoot {...quiet.props} held />)
    })
    expect(quiet.props.onCancel).toHaveBeenCalledTimes(1)

    vi.mocked(api.unlinkDir).mockReturnValue(new Promise(() => {}))
    const out = await settle(shown())
    await act(async () => {
      confirm(out.host)?.click()
    })
    await act(async () => {
      rerender(<UnlinkRoot {...out.props} held />)
    })
    expect(out.props.onCancel).not.toHaveBeenCalled()
  })
})
