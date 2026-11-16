import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
import '../src/index.css'
import type { ReaderModule } from '../src/module'
import { Settings } from '../src/Settings'
import { tokenColour } from './support/colour'
import { mount } from './support/mount'
import { coarseRulesOn } from './support/pointer'
import { MS_SCALE } from './support/sliderScale'

/**
 * A declared slider is the kind's, not the View panel's (O285 item 13): the
 * reader's Settings draws the same markup, so the rules that draw it must
 * reach it there too. Inside `.reader`, as `App` draws the panel.
 */
describe('a declared slider in the reader’s Settings panel', () => {
  it('marks the middle of a slider in the reader’s Settings panel', async () => {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(1280, 844)
    const fx: ReaderModule<{ delay: number }> = {
      id: 'fx',
      label: 'Fixture',
      defaults: { delay: 100 },
      sliders: { delay: MS_SCALE },
    }
    const host = mount(
      <div className="reader">
        <Settings
          visible
          modules={[fx]}
          prefs={{}}
          onChange={vi.fn()}
          onReset={vi.fn()}
          onClose={vi.fn()}
          onAccount={vi.fn()}
        />
      </div>,
    )
    const track = host.querySelector('.field-slider-track')
    const range = track?.querySelector('input[type="range"]')
    if (!track || !range) throw new Error('no slider drawn')
    const mark = getComputedStyle(track, '::after')
    expect([mark.width, mark.height]).toEqual(['1px', '6px'])
    expect(mark.backgroundColor).toBe(tokenColour(track, '--line-strong'))
    const box = range.getBoundingClientRect()
    const centre =
      track.getBoundingClientRect().left + Number.parseFloat(mark.left) + 0.5
    expect(Math.abs(centre - (box.left + box.width / 2))).toBeLessThanOrEqual(
      0.5,
    )
    const top = () =>
      track.getBoundingClientRect().top + Number.parseFloat(mark.top)
    const below = () => {
      const { top: from, height } = range.getBoundingClientRect()
      return top() - (from + height / 2)
    }
    // This Chromium's fine box is the thumb's own height: below it is below
    // the thumb, and a coarse pointer must keep the mark where it was.
    expect(top()).toBeGreaterThanOrEqual(box.bottom)
    const fine = below()
    onTestFinished(coarseRulesOn())
    expect(range.getBoundingClientRect().height).toBe(44)
    expect(Math.abs(below() - fine)).toBeLessThanOrEqual(1)
  })
})
