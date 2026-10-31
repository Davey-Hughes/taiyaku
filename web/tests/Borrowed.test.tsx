import { act } from 'react'
import { describe, expect, it } from 'vitest'
import { Borrowed } from '../src/Borrowed'
import { fitModule } from '../src/modules/fit'
import { mount } from './support/mount'

const lender = { edition: 'raw-mono', page: 47 }
const props = {
  lender,
  src: '/api/works/w/raw-mono/pages/46',
  fit: fitModule.defaults,
  offers: <p>the offers</p>,
  viewing: 'en-mono',
  reading: 'rtl' as const,
  before: { slot: 97, page: 89 },
  after: { slot: 104, page: 91 },
  onTake: () => {},
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
    // observes the pre-click DOM. `act` forces the update to flush before the
    // assertion below reads the DOM, as `Labeller.browser.test.tsx` does for
    // its own clicks.
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

  it('says which edition has no page here, without the mark being pressed', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.textContent).toContain('No en-mono page here')
  })

  it('offers both escapes beside that line, the two the arrows also take', () => {
    const host = mount(<Borrowed {...props} />)
    const labels = [...host.querySelectorAll('button')].map(
      (b) => b.textContent,
    )
    expect(labels.some((l) => l?.includes('en-mono page 89'))).toBe(true)
    expect(labels.some((l) => l?.includes('en-mono page 91'))).toBe(true)
  })

  it('hands the pressed offer slot to onTake, not its display page', () => {
    // `before` is `{ slot: 97, page: 89 }` — asserting the slot rather than
    // the page rules out a swap that returned the wrong one, which would
    // look identical here since the button is labelled with the page, not
    // the slot. Its own spy, not the shared `props.onTake`, so the other
    // cases above stay unaffected.
    const taken: number[] = []
    const host = mount(
      <Borrowed {...props} onTake={(slot) => taken.push(slot)} />,
    )
    const before = [...host.querySelectorAll('button')].find((b) =>
      b.textContent?.includes('en-mono page 89'),
    )
    expect(before).toBeDefined()
    act(() => {
      before?.click()
    })
    expect(taken).toEqual([97])
  })

  it('marks the borrowed page in the warning colour, its look in the stylesheet', () => {
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button') as HTMLButtonElement
    expect(mark.className).toBe('borrowed-mark')
    expect(mark.style.background).toBe('')
    expect((host.firstElementChild as HTMLElement).style.boxShadow).toBe(
      'inset 4px 0 0 0 var(--warning)',
    )
  })

  it('docks the mark below the top bar’s line, clear of a phone’s notch, whether or not the chrome is shown', () => {
    // The top bar covers `top: 0`, so a mark docked there would sit under it,
    // with the bar taking the pointer from the mark: the pointer's only way
    // into the offers, and back out. Docked here unconditionally, the same
    // rule spec §8.1 gives the panels: always below the top bar's line, and
    // above the bottom bar's, whether or not the chrome is shown.
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button') as HTMLButtonElement
    expect(mark.style.top).toBe('calc(var(--safe-top) + var(--reader-top-bar))')
    expect(mark.style.left).toBe('var(--safe-left)')
  })

  it('docks the caption 32px below the mark’s own top, at the same left edge', () => {
    const host = mount(<Borrowed {...props} />)
    const caption = host.querySelector('.borrowed-caption') as HTMLElement
    expect(caption.style.top).toBe(
      'calc(var(--safe-top) + var(--reader-top-bar) + 32px)',
    )
    expect(caption.style.left).toBe('var(--safe-left)')
  })
})
