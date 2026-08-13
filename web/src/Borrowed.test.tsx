import { act } from 'react'
import { describe, expect, it } from 'vitest'
import { Borrowed } from './Borrowed'
import { fitModule } from './modules/fit'
import { mount } from './testing/mount'

const lender = { edition: 'raw-mono', page: 47 }
const props = {
  lender,
  src: '/api/works/w/raw-mono/pages/46',
  fit: fitModule.defaults,
  offers: <p>the offers</p>,
}

describe('Borrowed', () => {
  it('names the lender and its page on the mark', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.textContent).toContain('raw-mono')
    expect(host.textContent).toContain('47')
  })

  it('says so in the alt text, which is not a style choice', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.querySelector('img')?.alt).toBe(
      'page 47, borrowed from raw-mono',
    )
  })

  it('draws the page, not the offers, until the mark is pressed', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.textContent).not.toContain('the offers')
    expect(host.querySelector('img')).not.toBeNull()
  })

  it('reveals the offers when the mark is pressed', () => {
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button')
    expect(mark).not.toBeNull()
    // `mount.ts` deliberately does not use `@testing-library/react`, so a
    // plain `.click()` here is not auto-wrapped the way `fireEvent`/`userEvent`
    // would wrap it. Under `createRoot`, React defers a click-triggered state
    // update to its own scheduler rather than committing it inline with the
    // dispatch, so reading `textContent` right after an unwrapped `.click()`
    // observes the pre-click DOM — confirmed by a failing run with the raw
    // call, in both the jsdom and browser projects alike, which is what rules
    // out "PageView misbehaves under jsdom" as the cause: PageView itself
    // renders correctly under jsdom on its own. `act` forces the update to
    // flush before the assertion below reads the DOM, the same fix
    // `Labeller.browser.test.tsx` already applies to its own clicks.
    act(() => {
      mark?.click()
    })
    expect(host.textContent).toContain('the offers')
  })

  it('comes back from the offers, since nothing else can bring it back', () => {
    const host = mount(<Borrowed {...props} />)
    act(() => {
      host.querySelector('button')?.click()
    })
    expect(host.textContent).toContain('the offers')
    // The revealed side's own control. Found by label rather than by index
    // because it is the *first* button there — `Gap` renders its offers after
    // it — and an index would pass just as well if the two swapped places.
    const back = [...host.querySelectorAll('button')].find((b) =>
      b.textContent?.includes('back to the borrowed page'),
    )
    expect(back).toBeDefined()
    act(() => {
      back?.click()
    })
    // Without this the reveal is one-way: `Gap` takes no dismiss handler and
    // `session.ts`'s `step` refuses a borrowed state, so a reader who pressed
    // the mark to see what they were missing could only get the page back by
    // swapping to another edition and back again.
    expect(host.textContent).not.toContain('the offers')
    expect(host.querySelector('img')).not.toBeNull()
    expect(host.textContent).toContain('borrowed from raw-mono page 47')
  })

  it('names which half of a joined spread was lent, in the mark and the alt', () => {
    // A joined spread is two slots sharing one `page`, so the number alone
    // names the whole spread. A screen reader would otherwise hear "page 47"
    // for a page that is half of 47 — and the `alt` is the one part of this
    // marking that is not a style choice.
    const host = mount(
      <Borrowed {...props} lender={{ ...lender, half: 'R' }} />,
    )
    expect(host.querySelector('img')?.alt).toBe(
      'page 47 R, borrowed from raw-mono',
    )
    // `page 47 R`, exactly `App.tsx`'s `altAt` format — a divergence here is
    // the same page described two ways in one reader.
    expect(host.textContent).toContain('borrowed from raw-mono page 47 R')
  })

  it('puts the mark in the tab order, since no key claims it', () => {
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button')
    expect(mark?.tagName).toBe('BUTTON')
    expect(mark?.getAttribute('type')).toBe('button')
  })
})
