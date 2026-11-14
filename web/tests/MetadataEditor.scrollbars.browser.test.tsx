import { act } from 'react'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import { MetadataEditor } from '../src/MetadataEditor'
import { mount } from './support/mount'

/**
 * The open Title box (O274) beside a scrollbar that takes its width, as a
 * classic one does. Run only in the launch that shows scrollbars,
 * `vite.config.ts`'s second `browser` instance.
 */
describe('the open Title box, where a scrollbar takes width (O274)', () => {
  /** A line longer at a scrollbar's width less than the open box's. */
  const TITLE =
    'The Complete Digital Edition of Kagami no Kuni no Harisugawa, Volume 01'

  it('is as tall as its title’s lines at its open width', async () => {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(1280, 800)
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          <div className="sidebar-column" />
          <main>
            <ul className="volumes">
              <li>
                <MetadataEditor slug="w" title={TITLE} onPatch={vi.fn()} />
              </li>
              <li />
            </ul>
          </main>
        </div>,
      )
    })
    await document.fonts.ready
    const label = host?.querySelector('.metadata-editor .field.title')
    const box = host?.querySelector<HTMLTextAreaElement>("[name='title']")
    if (label == null || box == null) throw new Error('no title box mounted')

    /** How tall `TITLE` is set `width` px wide as the box sets it. */
    const linesAt = (width: number) => {
      const probe = document.createElement('div')
      probe.style.cssText = `position: absolute; visibility: hidden; width: ${width}px; white-space: pre-wrap; overflow-wrap: break-word`
      probe.textContent = TITLE
      label.append(probe)
      const height = probe.getBoundingClientRect().height
      probe.remove()
      return height
    }
    const scroller = document.createElement('div')
    scroller.style.cssText =
      'position: absolute; overflow-y: scroll; width: 100px; height: 10px'
    document.body.append(scroller)
    const scrollbar = scroller.offsetWidth - scroller.clientWidth
    scroller.remove()

    await act(async () => {
      box.focus()
      await userEvent.click(box)
    })
    const style = getComputedStyle(box)
    const content =
      box.clientWidth -
      parseFloat(style.paddingLeft) -
      parseFloat(style.paddingRight)

    // The fixture tells: a scrollbar here takes width, and that width less,
    // the title takes a line more.
    expect(scrollbar).toBeGreaterThan(0)
    expect(linesAt(content - scrollbar)).toBeGreaterThan(linesAt(content))

    expect(
      box.clientHeight -
        parseFloat(style.paddingTop) -
        parseFloat(style.paddingBottom),
      'the open box’s lines against its title’s',
    ).toBe(linesAt(content))
  })
})
