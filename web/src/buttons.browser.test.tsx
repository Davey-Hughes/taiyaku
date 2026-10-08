import { describe, expect, it } from 'vitest'
// **Required**: the rules under test reach the app through `index.css`.
import './index.css'
import { mount } from './testing/mount'

/**
 * O201: a held button is drawn unlike a live one, by `base.css`'s own rule,
 * and so is one a class rule colours: `.confirm` sets a border colour and a
 * text colour, and neither is what says "held". The word keeps its colour.
 */
describe('a held button', () => {
  it('is drawn on the page’s ground with a dashed border, its word unchanged', () => {
    const host = mount(
      <>
        <div className="library">
          <button type="button" name="live">
            Live
          </button>
          <button type="button" name="held" disabled>
            Held
          </button>
          <div className="remove-library">
            <button type="button" className="confirm" name="live-confirm">
              Remove
            </button>
            <button
              type="button"
              className="confirm"
              name="held-confirm"
              disabled
            >
              Remove
            </button>
          </div>
        </div>
        <div className="login">
          <button type="button" name="login-live">
            Live
          </button>
          <button type="button" name="login-held" disabled>
            Held
          </button>
        </div>
      </>,
    )
    const style = (name: string) =>
      getComputedStyle(host.querySelector(`button[name="${name}"]`) as Element)
    const page = getComputedStyle(document.body).backgroundColor

    for (const [live, held] of [
      ['live', 'held'],
      ['live-confirm', 'held-confirm'],
      ['login-live', 'login-held'],
    ]) {
      expect(style(live).borderTopStyle).toBe('solid')
      expect(style(held).borderTopStyle).toBe('dashed')
      expect(style(held).backgroundColor).toBe(page)
      expect(style(held).backgroundColor).not.toBe(style(live).backgroundColor)
      expect(style(held).color).toBe(style(live).color)
    }
  })
})
