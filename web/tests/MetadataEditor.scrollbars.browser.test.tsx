import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import { MetadataEditor } from '../src/MetadataEditor'
import { lineCount, lineOf } from './support/lines'
import { mount, rerender } from './support/mount'

/**
 * The Title field (O283) beside a scrollbar that takes its width, as a
 * classic one does. Run only in the launch that shows scrollbars,
 * `vite.config.ts`'s second `browser` instance.
 */
describe('the Title field, where a scrollbar takes width (O283)', () => {
  /** A line longer at a scrollbar's width less than the field's. */
  const TITLE =
    'The Complete Digital Edition of Kagami no Kuni no Harisugawa, Volume 01'

  /** The holder's width: there, a scrollbar's width less takes a line more. */
  const WIDTH = '18rem'

  it('is as tall as its title’s lines at its width, beside a scrollbar’s width', async () => {
    const host = mount(
      <div className="library">
        <div style={{ width: WIDTH }} />
      </div>,
    )
    const holder = host.querySelector('.library > div')
    if (holder === null) throw new Error('no holder mounted')
    // Loaded before the field is fitted as it mounts. The form sets no font
    // of its own, so it is drawn in the holder's.
    const face = getComputedStyle(holder)
    await document.fonts.load(
      `${face.fontStyle} ${face.fontWeight} ${face.fontSize} ${face.fontFamily}`,
      TITLE,
    )
    await act(async () => {
      rerender(
        <div className="library">
          <div style={{ width: WIDTH }}>
            <MetadataEditor slug="w" title={TITLE} onPatch={vi.fn()} />
          </div>
        </div>,
      )
    })
    const field = holder.querySelector<HTMLTextAreaElement>("[name='title']")
    if (field === null) throw new Error('no title field mounted')

    const scroller = document.createElement('div')
    scroller.style.cssText =
      'position: absolute; overflow-y: scroll; width: 100px; height: 10px'
    document.body.append(scroller)
    const scrollbar = scroller.offsetWidth - scroller.clientWidth
    scroller.remove()

    const style = getComputedStyle(field)
    const content = lineOf(field)

    // The fixture tells: a scrollbar here takes width, and that width less,
    // the title takes a line more.
    expect(scrollbar).toBeGreaterThan(0)
    expect(lineCount(field, TITLE, content - scrollbar)).toBeGreaterThan(
      lineCount(field, TITLE),
    )

    expect(
      field.clientHeight -
        parseFloat(style.paddingTop) -
        parseFloat(style.paddingBottom),
      'the field’s lines against its title’s',
    ).toBe(lineCount(field, TITLE) * parseFloat(style.lineHeight))
  })
})
