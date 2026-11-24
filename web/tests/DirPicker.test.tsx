import { act, useState } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { DirPicker } from '../src/DirPicker'
import * as api from '../src/libraries'
import { deferred, flush } from './support/async'
import { press, typeInto } from './support/input'
import { mount, rerender } from './support/mount'

vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  listDirs: vi.fn(),
}))

const box = (host: HTMLElement) =>
  host.querySelector('input[name="path"]') as HTMLInputElement
const pick = (host: HTMLElement) =>
  host.querySelector('button[name="pick"]') as HTMLButtonElement

describe('DirPicker', () => {
  it('hands over the typed path and empties the box once it was taken', async () => {
    const onPick = vi.fn()
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    expect(pick(host).textContent).toBe('Add directory')
    expect(pick(host).disabled).toBe(true)

    await typeInto(box(host), '/media/Manga/Volumes')
    await press(pick(host))

    expect(onPick).toHaveBeenCalledWith('/media/Manga/Volumes')
    expect(box(host).value).toBe('')
  })

  it('sends nothing while held, by its button or by Enter, and lets the box be typed in and browsed', async () => {
    vi.mocked(api.listDirs).mockResolvedValue({
      path: null,
      parent: null,
      dirs: [],
    })
    const onPick = vi.fn()
    const host = mount(
      <DirPicker action="Add directory" onPick={onPick} disabled />,
    )
    await typeInto(box(host), '/media/a')
    expect(box(host).value).toBe('/media/a')
    expect(pick(host).disabled).toBe(true)
    await press(pick(host))
    await flush(() => {
      box(host).dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'Enter',
          bubbles: true,
          cancelable: true,
        }),
      )
    })
    expect(onPick).not.toHaveBeenCalled()
    expect(box(host).value).toBe('/media/a')

    await press(host.querySelector('button[name="browse"]') as Element)
    expect(api.listDirs).toHaveBeenCalledTimes(1)
  })

  it('keeps the typed path when the pick was refused', async () => {
    const onPick = vi.fn().mockRejectedValue(new Error('refused'))
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    await typeInto(box(host), '/media/typo')
    await press(pick(host))
    expect(box(host).value).toBe('/media/typo')
    expect(pick(host).disabled).toBe(false)

    await press(pick(host))
    expect(onPick).toHaveBeenCalledTimes(2)
  })

  it('settles like a refusal when onPick throws', async () => {
    const onPick = vi.fn().mockImplementation(() => {
      throw new Error('threw')
    })
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    await typeInto(box(host), '/media/typo')
    await press(pick(host))
    expect(box(host).value).toBe('/media/typo')
    expect(pick(host).disabled).toBe(false)

    // The guard that stops a second press is reset as well, not only the
    // flag that disables the button.
    await press(pick(host))
    expect(onPick).toHaveBeenCalledTimes(2)
  })

  it('picks on Enter, and keeps Enter from the form around it', async () => {
    const onPick = vi.fn()
    const host = mount(<DirPicker action="Add to the list" onPick={onPick} />)
    await typeInto(box(host), '/media/a')
    // jsdom submits no form for a key event, so what is asserted is the
    // event's own default being prevented, which is what stops a browser.
    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    })
    await flush(() => {
      box(host).dispatchEvent(enter)
    })
    expect(onPick).toHaveBeenCalledWith('/media/a')
    expect(enter.defaultPrevented).toBe(true)

    const other = new KeyboardEvent('keydown', {
      key: 'a',
      bubbles: true,
      cancelable: true,
    })
    await flush(() => {
      box(host).dispatchEvent(other)
    })
    expect(other.defaultPrevented).toBe(false)
    expect(onPick).toHaveBeenCalledTimes(1)
  })

  it('sends one pick for two presses and an Enter, and is live again after', async () => {
    const held = deferred<void>()
    const onPick = vi.fn().mockReturnValue(held.promise)
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    await typeInto(box(host), '/media/a')

    await flush(() => {
      pick(host).click()
      pick(host).click()
      box(host).dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    expect(onPick).toHaveBeenCalledTimes(1)
    expect(pick(host).disabled).toBe(true)

    await flush(() => held.resolve())
    expect(box(host).value).toBe('')
    await typeInto(box(host), '/media/b')
    expect(pick(host).disabled).toBe(false)
    onPick.mockReturnValue(undefined)
    await press(pick(host))
    expect(onPick).toHaveBeenCalledTimes(2)
  })

  it('keeps a path typed while a pick was pending', async () => {
    const held = deferred<void>()
    const onPick = vi.fn().mockReturnValue(held.promise)
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    await typeInto(box(host), '/media/a')
    await press(pick(host))
    await typeInto(box(host), '/media/b')

    await flush(() => held.resolve())
    expect(box(host).value).toBe('/media/b')
  })
})

const PREFIXES: api.DirListing = {
  path: null,
  parent: null,
  dirs: [{ name: '/media', path: '/media' }],
}
const MEDIA: api.DirListing = {
  path: '/media',
  parent: null,
  dirs: [
    { name: 'Comics', path: '/media/Comics' },
    { name: 'Manga', path: '/media/Manga' },
  ],
}
const MANGA: api.DirListing = {
  path: '/media/Manga',
  parent: '/media',
  dirs: [{ name: 'Volumes', path: '/media/Manga/Volumes' }],
}

const browse = (host: HTMLElement) =>
  host.querySelector('button[name="browse"]') as HTMLButtonElement
const entries = (host: HTMLElement) =>
  [...host.querySelectorAll('.dir-list button')] as HTMLButtonElement[]
const up = (host: HTMLElement) =>
  host.querySelector('button[name="up"]') as HTMLButtonElement

describe('DirPicker, browsing', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(api.listDirs).mockImplementation((path) =>
      Promise.resolve(
        path === undefined ? PREFIXES : path === '/media' ? MEDIA : MANGA,
      ),
    )
  })

  it('asks for nothing until it is opened, then lists the prefixes', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await flush(() => {})
    expect(api.listDirs).not.toHaveBeenCalled()
    expect(host.querySelector('.dir-browser')).toBeNull()

    await press(browse(host))

    expect(api.listDirs).toHaveBeenCalledTimes(1)
    expect(api.listDirs).toHaveBeenCalledWith(undefined)
    expect(browse(host).getAttribute('aria-expanded')).toBe('true')
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(up(host)).toBeNull()
    expect(box(host).value).toBe('')
  })

  it('enters a directory, puts its path in the box, and still picks by the button', async () => {
    const onPick = vi.fn()
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    await press(browse(host))
    await press(entries(host)[0])
    expect(host.querySelector('.where')?.textContent).toBe('/media')
    expect(box(host).value).toBe('/media')
    expect(entries(host).map((b) => b.textContent)).toEqual(['Comics', 'Manga'])

    await press(entries(host)[1])
    expect(api.listDirs).toHaveBeenLastCalledWith('/media/Manga')
    expect(api.listDirs).toHaveBeenCalledTimes(3)
    expect(box(host).value).toBe('/media/Manga')
    expect(onPick).not.toHaveBeenCalled()

    await press(pick(host))
    expect(onPick).toHaveBeenCalledWith('/media/Manga')
  })

  it('tells onPath each text the box holds: typed, browsed to, and emptied by a pick, and nothing on mount', async () => {
    const onPath = vi.fn()
    const host = mount(
      <DirPicker action="Add directory" onPick={vi.fn()} onPath={onPath} />,
    )
    expect(onPath).not.toHaveBeenCalled()

    await typeInto(box(host), '/srv/typed')
    expect(onPath.mock.calls).toEqual([['/srv/typed']])

    await press(browse(host))
    await press(entries(host)[0])
    expect(onPath.mock.calls).toEqual([['/srv/typed'], ['/media']])

    await press(pick(host))
    expect(onPath.mock.calls).toEqual([['/srv/typed'], ['/media'], ['']])
  })

  it('tells onPath nothing when a pick is taken after the box was typed in again', async () => {
    const held = deferred<void>()
    const onPath = vi.fn()
    const host = mount(
      <DirPicker
        action="Add directory"
        onPick={vi.fn().mockReturnValue(held.promise)}
        onPath={onPath}
      />,
    )
    await typeInto(box(host), '/srv/first')
    await press(pick(host))
    await typeInto(box(host), '/srv/next')

    await flush(() => held.resolve())
    expect(box(host).value).toBe('/srv/next')
    expect(onPath.mock.calls).toEqual([['/srv/first'], ['/srv/next']])
  })

  it('goes up to the parent, and from a prefix to the prefixes', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    await press(entries(host)[1])

    await press(up(host))
    expect(api.listDirs).toHaveBeenLastCalledWith('/media')
    expect(box(host).value).toBe('/media')

    await press(up(host))
    expect(api.listDirs).toHaveBeenLastCalledWith(undefined)
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(box(host).value).toBe('/media')
  })

  /**
   * No testing-library here, so the name is computed as it comes out for this
   * markup: the button's text, less every `aria-hidden` descendant, with no
   * `aria-label` standing in for the text. Not trimmed: Chromium's tree keeps
   * a space left outside the arrow at the front of the name.
   */
  const nameOf = (button: HTMLElement) => {
    expect(button.hasAttribute('aria-label')).toBe(false)
    expect(button.hasAttribute('aria-labelledby')).toBe(false)
    const named = button.cloneNode(true) as HTMLElement
    for (const hidden of named.querySelectorAll('[aria-hidden="true"]')) {
      hidden.remove()
    }
    return named.textContent
  }

  it.each([
    ['/srv/manga/extra/Trial-a', '/srv/manga/extra', 'Up to extra'],
    ['/srv/manga/extra/Trial-a', '/srv/manga/extra/', 'Up to extra'],
    ['/srv', '/', 'Up to /'],
  ])('at %s names Up for the last part of %s', async (path, parent, name) => {
    vi.mocked(api.listDirs).mockResolvedValue({ path, parent, dirs: [] })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(up(host).textContent).toBe(`↑ ${name}`)
    expect(nameOf(up(host))).toBe(name)
  })

  it('names Up from a top directory for the list of browsable directories', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    expect(host.querySelector('.where')?.textContent).toBe('/media')
    expect(up(host).textContent).toBe('↑ Back to all browsable directories')
    expect(nameOf(up(host))).toBe('Back to all browsable directories')
  })

  it('says to type the path when the server may browse nothing', async () => {
    vi.mocked(api.listDirs).mockResolvedValue({
      path: null,
      parent: null,
      dirs: [],
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    // The server answers this for two different reasons, and the sentence
    // names both.
    expect(host.querySelector('.dir-browser .empty')?.textContent).toBe(
      'No directories to browse: either the server was not told which it may ' +
        'browse (--dirs, or TAIYAKU_DIRS), or none of the directories it ' +
        'was told about exist. Type the path instead.',
    )
  })

  it('says a directory holds none, without that sentence', async () => {
    vi.mocked(api.listDirs).mockResolvedValue({
      path: '/media/Empty',
      parent: '/media',
      dirs: [],
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(host.querySelector('.dir-browser .empty')?.textContent).toBe(
      'No directories in here.',
    )
  })

  it("draws a refusal's sentence and keeps the list it had", async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    vi.mocked(api.listDirs).mockRejectedValue(
      new Error(
        '/api/library/dirs?path=%2Fmedia: 403 {"error":"/elsewhere is not inside a directory this server may browse"}',
      ),
    )
    await press(entries(host)[0])
    expect(host.querySelector('.dir-browser [role="alert"]')?.textContent).toBe(
      '/elsewhere is not inside a directory this server may browse',
    )
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(box(host).value).toBe('')
  })

  it('draws the directory asked for last when two answers cross', async () => {
    const slow = deferred<api.DirListing>()
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    vi.mocked(api.listDirs).mockReturnValueOnce(slow.promise)
    await press(entries(host)[0])
    await press(up(host))
    expect(host.querySelector('.where')?.textContent).toBe(
      'Directories this server may browse',
    )

    await flush(() => slow.resolve(MANGA))
    expect(host.querySelector('.where')?.textContent).toBe(
      'Directories this server may browse',
    )
    expect(box(host).value).toBe('/media')
  })

  it('draws no refusal for a listing that was asked for before the last', async () => {
    const slow = deferred<api.DirListing>()
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    vi.mocked(api.listDirs).mockReturnValueOnce(slow.promise)
    await press(entries(host)[0])
    await press(up(host))

    await flush(() => slow.reject(new Error('/x: 403 {"error":"too late"}')))
    expect(host.querySelector('.dir-browser [role="alert"]')).toBeNull()
  })

  it('stops drawing a refusal once a later listing is read', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    vi.mocked(api.listDirs).mockRejectedValueOnce(
      new Error('/x: 403 {"error":"refused"}'),
    )
    await press(entries(host)[0])
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('refused')

    await press(entries(host)[0])
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('.where')?.textContent).toBe('/media')
  })

  it('keeps a refusal drawn above the listing until the next answer lands, so the listing holds still', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    vi.mocked(api.listDirs).mockRejectedValueOnce(
      new Error('/x: 403 {"error":"refused"}'),
    )
    await press(entries(host)[0])
    const browser = host.querySelector('.dir-browser') as HTMLElement
    expect(browser.firstElementChild?.getAttribute('role')).toBe('alert')

    const next = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(next.promise)
    await press(entries(host)[0])
    expect(browser.firstElementChild?.textContent).toBe('refused')
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])

    await flush(() => next.resolve(MEDIA))
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(browser.firstElementChild).toBe(host.querySelector('.where'))
  })

  it('draws a newer refusal in the place of the one it replaces', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    vi.mocked(api.listDirs).mockRejectedValueOnce(
      new Error('/x: 403 {"error":"refused first"}'),
    )
    await press(entries(host)[0])
    const next = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(next.promise)
    await press(entries(host)[0])

    await flush(() =>
      next.reject(new Error('/x: 403 {"error":"refused again"}')),
    )
    const alerts = [...host.querySelectorAll('[role="alert"]')]
    expect(alerts.map((a) => a.textContent)).toEqual(['refused again'])
  })

  it('keeps where it was when closed and opened again, asking nothing more', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    expect(api.listDirs).toHaveBeenCalledTimes(2)

    await press(browse(host))
    expect(host.querySelector('.dir-browser')).toBeNull()
    expect(browse(host).getAttribute('aria-expanded')).toBe('false')

    await press(browse(host))
    expect(host.querySelector('.where')?.textContent).toBe('/media')
    expect(api.listDirs).toHaveBeenCalledTimes(2)
  })

  it('asks for nothing more when closed while the first listing is still loading', async () => {
    const first = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValue(first.promise)
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    await press(browse(host))
    expect(host.querySelector('.dir-browser')).toBeNull()
    expect(api.listDirs).toHaveBeenCalledTimes(1)

    await flush(() => first.resolve(PREFIXES))
    await press(browse(host))
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(api.listDirs).toHaveBeenCalledTimes(1)
  })

  it('asks again when opened after a refused first listing, and draws it', async () => {
    vi.mocked(api.listDirs).mockRejectedValueOnce(
      new Error('/api/library/dirs: 500 {"error":"unreadable"}'),
    )
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('unreadable')

    await press(browse(host))
    await press(browse(host))
    expect(api.listDirs).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
  })

  it('keeps a pick that is pending when a directory is entered', async () => {
    const held = deferred<void>()
    const onPick = vi.fn().mockReturnValue(held.promise)
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    await typeInto(box(host), '/media/a')
    await press(browse(host))
    await press(pick(host))
    await press(entries(host)[0])
    expect(box(host).value).toBe('/media')
    // The browse answer has landed and the pick has not: the button is still
    // held, though the box now holds a path.
    expect(pick(host).disabled).toBe(true)

    await flush(() => held.resolve())
    expect(onPick).toHaveBeenCalledTimes(1)
    expect(onPick).toHaveBeenCalledWith('/media/a')
    expect(box(host).value).toBe('/media')
    expect(pick(host).disabled).toBe(false)
  })

  it('draws a failure of the first listing, and not the placeholder with it', async () => {
    vi.mocked(api.listDirs).mockRejectedValue(
      new Error('/api/library/dirs: 500 {"error":"unreadable"}'),
    )
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(host.querySelector('.dir-browser [role="alert"]')?.textContent).toBe(
      'unreadable',
    )
    expect(host.querySelector('.dir-browser')?.textContent).not.toContain(
      'Reading directories',
    )
  })

  /** Moves the clock on, and lets React draw what its timers set. */
  const wait = (ms: number) => act(async () => vi.advanceTimersByTime(ms))
  const group = (host: HTMLElement) =>
    host.querySelector('.dir-browser') as HTMLElement

  it('keeps the listing it had drawn, and nothing above it, while the next is read', async () => {
    vi.useFakeTimers()
    onTestFinished(() => {
      vi.useRealTimers()
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(group(host).getAttribute('aria-busy')).toBe('false')
    const slow = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(slow.promise)
    await press(entries(host)[0])

    expect(group(host).getAttribute('aria-busy')).toBe('true')
    expect(group(host).firstElementChild).toBe(host.querySelector('.where'))
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(group(host).textContent).not.toContain('Reading directories')

    await wait(api.READING_DELAY_MS - 1)
    expect(group(host).textContent).not.toContain('Reading directories')

    // Slow now: said, after the listing, which stays where it was.
    await wait(1)
    expect(group(host).lastElementChild?.textContent).toBe(
      'Reading directories…',
    )
    expect(group(host).firstElementChild).toBe(host.querySelector('.where'))
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])

    await flush(() => slow.resolve(MEDIA))
    expect(group(host).textContent).not.toContain('Reading directories')
    expect(group(host).getAttribute('aria-busy')).toBe('false')
    expect(entries(host).map((b) => b.textContent)).toEqual(['Comics', 'Manga'])
  })

  it('never says it is reading for a listing answered before the delay', async () => {
    vi.useFakeTimers()
    onTestFinished(() => {
      vi.useRealTimers()
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    const quick = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(quick.promise)
    await press(entries(host)[0])
    await wait(api.READING_DELAY_MS - 1)
    await flush(() => quick.resolve(MEDIA))

    await wait(api.READING_DELAY_MS)
    expect(group(host).textContent).not.toContain('Reading directories')
    expect(group(host).getAttribute('aria-busy')).toBe('false')
  })

  it('never says it is reading once the newest read is answered, on an older read’s clock', async () => {
    vi.useFakeTimers()
    onTestFinished(() => {
      vi.useRealTimers()
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    const older = deferred<api.DirListing>()
    const newer = deferred<api.DirListing>()
    vi.mocked(api.listDirs)
      .mockReturnValueOnce(older.promise)
      .mockReturnValueOnce(newer.promise)
    await press(entries(host)[0])
    await wait(api.READING_DELAY_MS / 2)
    await press(entries(host)[0])
    await flush(() => newer.resolve(MEDIA))

    await wait(api.READING_DELAY_MS)
    expect(group(host).textContent).not.toContain('Reading directories')
  })

  it('stops saying it is reading when a slow listing is refused', async () => {
    vi.useFakeTimers()
    onTestFinished(() => {
      vi.useRealTimers()
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    const refused = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(refused.promise)
    await press(entries(host)[0])
    await wait(api.READING_DELAY_MS)
    expect(group(host).textContent).toContain('Reading directories…')

    await flush(() =>
      refused.reject(new Error('/x: 403 {"error":"not browsable"}')),
    )
    expect(group(host).textContent).not.toContain('Reading directories')
    expect(group(host).getAttribute('aria-busy')).toBe('false')
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'not browsable',
    )
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
  })

  it('leaves no timer running once it is gone in the middle of a read', async () => {
    vi.useFakeTimers()
    onTestFinished(() => {
      vi.useRealTimers()
    })
    vi.mocked(api.listDirs).mockReturnValueOnce(
      deferred<api.DirListing>().promise,
    )
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(vi.getTimerCount()).toBe(1)

    rerender(null)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('draws nothing in the browser while the first listing is read, until it is slow', async () => {
    vi.useFakeTimers()
    onTestFinished(() => {
      vi.useRealTimers()
    })
    const first = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(first.promise)
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(group(host).childElementCount).toBe(0)
    expect(group(host).getAttribute('aria-busy')).toBe('true')

    await wait(api.READING_DELAY_MS)
    expect(group(host).textContent).toBe('Reading directories…')

    await flush(() => first.resolve(PREFIXES))
    expect(group(host).textContent).not.toContain('Reading directories')
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
  })

  it('does not overwrite a path that was edited while the listing was being read', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    const slow = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(slow.promise)
    await press(entries(host)[0])
    await typeInto(box(host), '/media/Manga/Typed')
    await flush(() => slow.resolve(MEDIA))
    expect(box(host).value).toBe('/media/Manga/Typed')
    // The listing itself is still drawn.
    expect(host.querySelector('.where')?.textContent).toBe('/media')
  })

  it('writes a path in again when the box has not been touched since the request', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await typeInto(box(host), '/before')
    await press(browse(host))
    await press(entries(host)[0])
    expect(box(host).value).toBe('/media')
  })

  it('moves focus to the line that says where it is, after entering a directory and after Up, and not on opening while focus is in the path box', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    host.querySelector<HTMLElement>('input[name="path"]')?.focus()
    await press(browse(host))
    expect(document.activeElement).not.toBe(host.querySelector('.where'))

    // Focus is on the button that was pressed, as a click or a key leaves it.
    entries(host)[0].focus()
    await press(entries(host)[0])
    expect(document.activeElement).toBe(host.querySelector('.where'))
    expect(host.querySelector('.where')?.textContent).toBe('/media')

    entries(host)[1].focus()
    await press(entries(host)[1])
    expect(document.activeElement).toBe(host.querySelector('.where'))
    expect(host.querySelector('.where')?.textContent).toBe('/media/Manga')

    up(host).focus()
    await press(up(host))
    expect(document.activeElement).toBe(host.querySelector('.where'))
    expect(host.querySelector('.where')?.textContent).toBe('/media')
  })

  it('tells its owner each time the path in the box changes, by hand or by browsing', async () => {
    const onEdit = vi.fn()
    const host = mount(
      <DirPicker action="Add directory" onPick={vi.fn()} onEdit={onEdit} />,
    )
    // The prefixes name no directory, so the box is not written.
    await press(browse(host))
    expect(onEdit).not.toHaveBeenCalled()

    await press(entries(host)[0])
    expect(box(host).value).toBe('/media')
    expect(onEdit).toHaveBeenCalledTimes(1)

    await typeInto(box(host), '/media/x')
    expect(onEdit).toHaveBeenCalledTimes(2)

    await press(entries(host)[1])
    expect(onEdit).toHaveBeenCalledTimes(3)
    await press(up(host))
    expect(onEdit).toHaveBeenCalledTimes(4)
  })

  it('leaves focus in the path box when a listing arrives while a person is typing there', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    const slow = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(slow.promise)
    entries(host)[0].focus()
    await press(entries(host)[0])
    box(host).focus()
    await typeInto(box(host), '/media/Manga/Ty')
    await flush(() => slow.resolve(MEDIA))
    expect(host.querySelector('.where')?.textContent).toBe('/media')
    expect(document.activeElement).toBe(box(host))
    expect(box(host).value).toBe('/media/Manga/Ty')
  })

  it('leaves focus alone when it is on something outside the browser as a listing arrives', async () => {
    const host = mount(
      <div>
        <button type="button" name="elsewhere">
          Elsewhere
        </button>
        <DirPicker action="Add directory" onPick={vi.fn()} />
      </div>,
    )
    await press(browse(host))
    const slow = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(slow.promise)
    await press(entries(host)[0])
    const elsewhere = host.querySelector<HTMLElement>(
      'button[name="elsewhere"]',
    )
    elsewhere?.focus()
    await flush(() => slow.resolve(MEDIA))
    expect(document.activeElement).toBe(elsewhere)
  })

  it('leaves focus where it is when a listing is refused', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    vi.mocked(api.listDirs).mockRejectedValueOnce(
      new Error('/x: 403 {"error":"refused"}'),
    )
    const first = entries(host)[0]
    first.focus()
    await press(first)
    expect(document.activeElement).toBe(first)
  })
  it('takes focus to the line that says where it is when Browse… opens it', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    // Focus is on the button that was pressed, as a click or a key leaves it.
    browse(host).focus()
    await press(browse(host))
    expect(document.activeElement).toBe(host.querySelector('.where'))
  })

  it('takes focus to the line that says where it is when Browse… opens it with focus on the page', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    // A click in Safari focuses no button, so focus is on the page's body
    // as the listing lands; `press` moves no focus either.
    ;(document.activeElement as HTMLElement | null)?.blur()
    expect(document.activeElement).toBe(document.body)
    await press(browse(host))
    expect(document.activeElement).toBe(host.querySelector('.where'))
  })

  it('takes focus to the line that says where it is when Browse… opens it again over the listing it holds', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    browse(host).focus()
    await press(browse(host))
    await press(browse(host))
    expect(host.querySelector('.where')).toBeNull()

    browse(host).focus()
    await press(browse(host))
    // Drawn from what it held: the second opening asked for nothing.
    expect(api.listDirs).toHaveBeenCalledTimes(1)
    expect(document.activeElement).toBe(host.querySelector('.where'))
  })

  it('leaves focus in a field it was moved to while the first listing is read', async () => {
    const first = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(first.promise)
    const host = mount(
      <div>
        <input name="name" aria-label="Name" />
        <DirPicker action="Add directory" onPick={vi.fn()} />
      </div>,
    )
    browse(host).focus()
    await press(browse(host))
    const name = host.querySelector('input[name="name"]') as HTMLInputElement
    name.focus()

    await flush(() => first.resolve(PREFIXES))
    expect(host.querySelector('.where')?.textContent).toBe(
      'Directories this server may browse',
    )
    expect(document.activeElement).toBe(name)
  })
})

/**
 * The callers' shape: a host drawn after the form, handed over once mounted.
 * Made in a function because `useComponentExportOnlyModules` refuses an
 * unexported component at module scope, and `noExportsInTest` refuses the
 * export.
 */
function hosted() {
  function Hosted() {
    const [place, setPlace] = useState<HTMLDivElement | null>(null)
    return (
      <>
        <DirPicker
          action="Add directory"
          onPick={vi.fn()}
          browserHost={place}
        />
        <div className="dir-browser-host" ref={setPlace} />
      </>
    )
  }
  return <Hosted />
}

describe('DirPicker, where its browser is drawn (O316)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(api.listDirs).mockResolvedValue(PREFIXES)
  })

  const listed = (place: ParentNode) =>
    [...place.querySelectorAll('.dir-list button')].map((b) => b.textContent)

  it('draws its browser in the picker, beneath the box, when handed no host', async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    const browser = host.querySelector('.dir-browser') as HTMLElement
    expect(browser.parentElement).toBe(host.querySelector('.dir-picker'))
    expect(
      box(host).compareDocumentPosition(browser) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0)
  })

  it('points Browse… at the browser it opens, in the host it is handed, while it is open', async () => {
    const host = mount(hosted())
    expect(browse(host).hasAttribute('aria-controls')).toBe(false)
    await press(browse(host))
    const browser = host.querySelector('.dir-browser-host > .dir-browser')
    expect(browser).not.toBeNull()
    expect(
      document.getElementById(browse(host).getAttribute('aria-controls') ?? ''),
    ).toBe(browser)
    await press(browse(host))
    expect(browse(host).hasAttribute('aria-controls')).toBe(false)
  })

  it('draws its browser in the host it is handed, and not in the picker', async () => {
    const host = mount(hosted())
    await press(browse(host))
    const place = host.querySelector('.dir-browser-host') as HTMLElement
    expect(listed(place)).toEqual(['/media'])
    expect(host.querySelector('.dir-picker .dir-browser')).toBeNull()
  })

  it('draws nothing while its host is not yet there, and the browser once it is', async () => {
    const place = document.createElement('div')
    document.body.append(place)
    onTestFinished(() => {
      place.remove()
    })
    const host = mount(
      <DirPicker action="Add directory" onPick={vi.fn()} browserHost={null} />,
    )
    await press(browse(host))
    expect(document.querySelector('.dir-browser')).toBeNull()

    rerender(
      <DirPicker action="Add directory" onPick={vi.fn()} browserHost={place} />,
    )
    expect(listed(place)).toEqual(['/media'])
  })
})

const ROOTS: api.DirListing = {
  path: null,
  parent: null,
  dirs: [
    { name: '/srv/comics', path: '/srv/comics' },
    { name: '/srv/manga', path: '/srv/manga' },
  ],
}
/**
 * Names that differ in case (Bocchi, BOCCHI the Rock) and by an accent alone
 * (Café, Cafe), and two that "af" matches inside a word.
 */
const SHELF: api.DirListing = {
  path: '/srv/manga',
  parent: null,
  dirs: [
    { name: 'Bocchi', path: '/srv/manga/Bocchi' },
    { name: 'BOCCHI the Rock', path: '/srv/manga/BOCCHI the Rock' },
    { name: 'Café', path: '/srv/manga/Café' },
    { name: 'Cafe', path: '/srv/manga/Cafe' },
  ],
}
/** Names no filter typed on the shelf matches, so one kept hides them. */
const VOLUMES: api.DirListing = {
  path: '/srv/manga/Bocchi',
  parent: '/srv/manga',
  dirs: [
    { name: 'Vol. 01', path: '/srv/manga/Bocchi/Vol. 01' },
    { name: 'Vol. 02', path: '/srv/manga/Bocchi/Vol. 02' },
  ],
}

describe('DirPicker, the filter (O317)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(api.listDirs).mockImplementation((path) =>
      Promise.resolve(
        path === undefined ? ROOTS : path === '/srv/manga' ? SHELF : VOLUMES,
      ),
    )
  })

  const filterBox = (host: HTMLElement) =>
    host.querySelector('input[name="filter"]') as HTMLInputElement
  const names = (host: HTMLElement) => entries(host).map((b) => b.textContent)
  const entry = (host: HTMLElement, name: string) => {
    const found = entries(host).find((b) => b.textContent === name)
    if (!found) throw new Error(`no entry named ${name}`)
    return found
  }
  /** The filter's text and the entries drawn under it, read together. */
  const narrowed = (host: HTMLElement) => ({
    filter: filterBox(host).value,
    entries: names(host),
  })
  /** Opens the browser and enters the shelf the filter is tried on. */
  const openShelf = async (host: HTMLElement) => {
    await press(browse(host))
    await press(entry(host, '/srv/manga'))
  }
  const mountShelf = async () => {
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await openShelf(host)
    return host
  }

  it('draws the filter under the where line, above Up and the entries', async () => {
    const host = await mountShelf()
    const parts = [...(host.querySelector('.dir-browser')?.children ?? [])]
    expect(parts.map((el) => el.getAttribute('name') ?? el.className)).toEqual([
      'where',
      'search-box',
      'up',
      'dir-list',
      'visually-hidden',
    ])
  })

  it('names the filter "Filter" alone, unseen, and says what it narrows in its placeholder, as a search box', async () => {
    const host = await mountShelf()
    const box = filterBox(host)
    expect([...(box.labels ?? [])].map((l) => l.textContent)).toEqual([
      'Filter',
    ])
    // Either would outrank the label as the name.
    expect(box.hasAttribute('aria-label')).toBe(false)
    expect(box.hasAttribute('aria-labelledby')).toBe(false)
    expect(box.placeholder).toBe('Filter these directories')
    expect(box.type).toBe('search')
    expect(
      box.labels?.[0]?.querySelector('.visually-hidden')?.textContent,
    ).toBe('Filter')
  })

  it('asks the browser for no remembered entries, which would be drawn over the listing', async () => {
    const host = await mountShelf()
    expect(filterBox(host).getAttribute('autocomplete')).toBe('off')
  })

  it('says to a screen reader that no directory matches, in a status line drawn empty until then', async () => {
    const host = await mountShelf()
    const status = () =>
      host.querySelector('.dir-browser [role="status"]')?.textContent
    expect(status()).toBe('')
    await typeInto(filterBox(host), 'zz')
    expect(status()).toBe('No directories here match this filter.')
    await typeInto(filterBox(host), 'af')
    expect(status()).toBe('')
  })

  /**
   * The elements saying `words` alone that a screen reader reads: none
   * `aria-hidden`, nor inside one that is. No testing-library here, so the
   * tree is read as it comes out for this markup.
   */
  const readSaying = (host: HTMLElement, words: string) =>
    [...host.querySelectorAll('*')].filter(
      (el) =>
        el.childElementCount === 0 &&
        el.textContent === words &&
        el.closest('[aria-hidden="true"]') === null,
    )

  it('has a screen reader read that no directory matches once, from the status line, while the line drawn says it too', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'zz')
    const words = 'No directories here match this filter.'
    expect(host.querySelector('.dir-browser .empty')?.textContent).toBe(words)
    expect(readSaying(host, words)).toEqual([
      host.querySelector('.dir-browser [role="status"]'),
    ])
  })

  it('leaves an empty listing’s line to a screen reader, drawn where a no-match line was', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'zz')
    const line = host.querySelector('.dir-browser .empty')
    vi.mocked(api.listDirs).mockResolvedValueOnce({
      path: null,
      parent: null,
      dirs: [],
    })
    await press(up(host))
    const empty = host.querySelector('.dir-browser .empty') as HTMLElement
    // The same element, saying other words.
    expect(empty).toBe(line)
    expect(empty.textContent).toMatch(/^No directories to browse/)
    expect(readSaying(host, empty.textContent ?? '')).toEqual([empty])
  })

  it('matches inside a name, not only at its start', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'af')
    expect(names(host)).toEqual(['Café', 'Cafe'])
  })

  it('sets case aside, in the name and in the filter', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'bocchi')
    expect(names(host)).toEqual(['Bocchi', 'BOCCHI the Rock'])
    await typeInto(filterBox(host), 'BOCCHI')
    expect(names(host)).toEqual(['Bocchi', 'BOCCHI the Rock'])
  })

  it('keeps accents: e matches no é, and é no e', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'Cafe')
    expect(names(host)).toEqual(['Cafe'])
    await typeInto(filterBox(host), 'Café')
    expect(names(host)).toEqual(['Café'])
  })

  it('sets aside spaces around the filter', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), '  af  ')
    expect(names(host)).toEqual(['Café', 'Cafe'])
  })

  it('never hides Up, which is the way out of a listing it has emptied', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'af')
    expect(up(host)?.textContent).toBe('↑ Back to all browsable directories')
  })

  it('says no directory matches, and draws no list, when none does', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'zz')
    expect(host.querySelector('.dir-browser .empty')?.textContent).toBe(
      'No directories here match this filter.',
    )
    expect(host.querySelector('.dir-list')).toBeNull()
  })

  it('is drawn over a directory that holds none', async () => {
    vi.mocked(api.listDirs).mockResolvedValue({
      path: '/srv/manga/Empty',
      parent: '/srv/manga',
      dirs: [],
    })
    const host = mount(<DirPicker action="Add directory" onPick={vi.fn()} />)
    await press(browse(host))
    expect(filterBox(host)).not.toBeNull()
  })

  it('keeps the filter, and what it narrows, while the next listing is read', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'bocchi')
    vi.mocked(api.listDirs).mockReturnValueOnce(
      deferred<api.DirListing>().promise,
    )
    await press(entry(host, 'Bocchi'))
    expect(narrowed(host)).toEqual({
      filter: 'bocchi',
      entries: ['Bocchi', 'BOCCHI the Rock'],
    })
  })

  it('is emptied when the directory entered is listed', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'bocchi')
    await press(entry(host, 'Bocchi'))
    expect(narrowed(host)).toEqual({
      filter: '',
      entries: ['Vol. 01', 'Vol. 02'],
    })
  })

  it('is emptied when the directory Up goes to is listed', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'af')
    await press(up(host))
    expect(narrowed(host)).toEqual({
      filter: '',
      entries: ['/srv/comics', '/srv/manga'],
    })
  })

  it('is emptied when the browser is closed and opened again', async () => {
    const host = await mountShelf()
    await typeInto(filterBox(host), 'af')
    await press(browse(host))
    await press(browse(host))
    expect(narrowed(host)).toEqual({
      filter: '',
      entries: ['Bocchi', 'BOCCHI the Rock', 'Café', 'Cafe'],
    })
  })

  // jsdom lays nothing out: a list's scroll offset here is only the number
  // set on it, which is enough to tell which list a listing is drawn in and
  // what that list carries over.
  it('draws each listing that lands in a list of its own, at its top, and keeps the list it had, and its offset, while the next is read', async () => {
    const host = await mountShelf()
    const list = () => host.querySelector('.dir-list') as HTMLElement
    const shelf = list()
    shelf.scrollTop = 40
    const next = deferred<api.DirListing>()
    vi.mocked(api.listDirs).mockReturnValueOnce(next.promise)
    await press(entry(host, 'Bocchi'))
    expect({ kept: list() === shelf, scrollTop: list().scrollTop }).toEqual({
      kept: true,
      scrollTop: 40,
    })
    await flush(() => next.resolve(VOLUMES))
    expect({
      entries: names(host),
      kept: list() === shelf,
      scrollTop: list().scrollTop,
    }).toEqual({ entries: ['Vol. 01', 'Vol. 02'], kept: false, scrollTop: 0 })
    const volumes = list()
    volumes.scrollTop = 40
    await press(up(host))
    expect({
      entries: names(host),
      kept: list() === volumes,
      scrollTop: list().scrollTop,
    }).toEqual({
      entries: ['Bocchi', 'BOCCHI the Rock', 'Café', 'Cafe'],
      kept: false,
      scrollTop: 0,
    })
  })

  it('takes the list back to its top as the filter’s words change, and not for a space after them, which matches nothing new', async () => {
    const host = await mountShelf()
    const list = () => host.querySelector('.dir-list') as HTMLElement
    list().scrollTop = 40
    await typeInto(filterBox(host), 'b')
    expect({ entries: names(host), scrollTop: list().scrollTop }).toEqual({
      entries: ['Bocchi', 'BOCCHI the Rock'],
      scrollTop: 0,
    })
    list().scrollTop = 40
    await typeInto(filterBox(host), 'b ')
    expect({ entries: names(host), scrollTop: list().scrollTop }).toEqual({
      entries: ['Bocchi', 'BOCCHI the Rock'],
      scrollTop: 40,
    })
  })

  it('keeps Enter from the form around it, and picks nothing', async () => {
    const onPick = vi.fn()
    const host = mount(<DirPicker action="Add directory" onPick={onPick} />)
    // Entering the shelf wrote its path in the box, so a pick would be taken.
    await openShelf(host)
    await typeInto(filterBox(host), 'af')
    // jsdom submits no form for a key event, so what is asserted is the
    // event's own default being prevented, which is what stops a browser.
    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    })
    await flush(() => {
      filterBox(host).dispatchEvent(enter)
    })
    expect(enter.defaultPrevented).toBe(true)
    expect(onPick).not.toHaveBeenCalled()
  })
})
