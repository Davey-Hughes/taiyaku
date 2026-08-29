import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { MetadataEditor } from './MetadataEditor'
import { mount } from './testing/mount'

/**
 * Set a controlled field's value the way a keystroke does — through the
 * prototype's own setter, not the element's. `AcceptForm.test.tsx` carries
 * the same helper for the same reason: React tracks the value it last set on
 * the instance, and a plain `el.value = x` assignment races that tracker, so
 * the `input` event dispatched right after can fire with React believing
 * nothing changed.
 */
async function typeInto(el: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set?.call(el, value)
    el.dispatchEvent(new Event('input', { bubbles: true }))
    await Promise.resolve()
  })
}

const click = async (el: Element) => {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

const submitButton = (host: HTMLElement) =>
  host.querySelector('button[type="submit"]') as HTMLButtonElement

const titleField = (host: HTMLElement) =>
  host.querySelector<HTMLInputElement>("input[name='title']")

describe('MetadataEditor', () => {
  it('saves a corrected title and shows the new one', async () => {
    const patch = vi
      .fn()
      .mockResolvedValue({ title: 'Volume 1', persisted: true })
    const host = mount(
      <MetadataEditor
        slug="w"
        title="Series - v01 (2021).cbz"
        onPatch={patch}
      />,
    )

    const field = titleField(host)
    expect(field?.value).toBe('Series - v01 (2021).cbz')

    await typeInto(field!, 'Volume 1')
    await click(submitButton(host))

    expect(patch).toHaveBeenCalledWith('w', { title: 'Volume 1' })
    expect(host.textContent).toContain('Volume 1')
  })

  /**
   * **I6's third shape.** The test above cannot distinguish
   * `setCurrent(result.title ?? value)` from the regressed `setCurrent(value)`
   * — its mock resolves with the exact string just typed, so the two
   * expressions agree by coincidence. This mocks a re-projected title that
   * differs from what was typed (the server's own word for what it actually
   * holds now, per this file's own doc comment), so only the correct
   * expression can make the displayed title match it.
   */
  it('shows the server-reprojected title, not the value just typed, when they differ', async () => {
    const patch = vi.fn().mockResolvedValue({
      title: 'Volume 1: The Reprojected Title',
      persisted: true,
    })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await typeInto(titleField(host)!, 'Volume 1')
    await click(submitButton(host))

    expect(host.querySelector('p.current')?.textContent).toBe(
      'Volume 1: The Reprojected Title',
    )
  })

  it('says so when the correction could not be written beside the library', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: false })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.textContent).toMatch(/could not be saved beside your library/i)
  })

  it('says nothing about being unwritten when the correction landed cleanly', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.textContent).not.toMatch(/could not be saved/i)
  })

  it('names every reason a fanned-out edit could not all reach disk', async () => {
    const patch = vi.fn().mockResolvedValue({
      title: 'old',
      persisted: false,
      why: ['root is read-only', 'no on-disk address'],
    })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.textContent).toContain('root is read-only')
    expect(host.textContent).toContain('no on-disk address')
  })

  it('surfaces a hard failure as an alert, distinct from an unwritten sidecar', async () => {
    const patch = vi
      .fn()
      .mockRejectedValue(
        new Error('/api/library/works/w: 409 a name this route refused'),
      )
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      '409 a name this route refused',
    )
    expect(host.textContent).not.toMatch(/could not be saved beside/i)
  })

  it('stringifies a rejection that is not an Error, rather than losing it', async () => {
    const patch = vi.fn().mockRejectedValue('offline')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })
})
