import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { Filmstrip, type FilmstripProps } from './Filmstrip'
import type { Group } from './pairing'
import type { Position } from './resolver'
import { mount, rerender } from './testing/mount'

// jsdom has no layout engine and no real `scrollIntoView` at all — it throws
// "Not implemented" the instant anything calls it — and every mount below
// runs the effect that calls it, so every test needs a stub. Recording
// `this` on each call, rather than only counting calls, is what lets a test
// name *which* tile the rail centred on: the whole point of the `currentRef
// ?? pairedRef ?? anchorRef` chain the component builds.
let scrolledTo: Element[]
beforeEach(() => {
  scrolledTo = []
  Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
    scrolledTo.push(this)
  })
})

// Four slots: two ordinary pages, and one joined spread's two halves —
// addressed as their own singleton groups, the shape `pairing.ts`'s
// `correctionTiles` produces while correcting (see `FilmstripProps.groups`'
// own doc). That shape is what exercises the half badge and the half-aware
// alt text without needing `pairing.ts` itself in this file at all.
const slots: Position[] = [
  { page: 10 },
  { page: 11 },
  { page: 2, half: 'R' },
  { page: 2, half: 'L' },
]

const groups: Group[] = [
  { slots: [0] },
  { slots: [1] },
  { slots: [2] },
  { slots: [3] },
]

function props(overrides: Partial<FilmstripProps> = {}): FilmstripProps {
  return {
    reading: 'ltr',
    work: 'gotoubun-v01',
    edition: 'en-mono',
    slots,
    groups,
    currentIndex: 1,
    onNavigate: vi.fn(),
    ...overrides,
  }
}

const buttons = (host: HTMLElement) =>
  Array.from(host.querySelectorAll('button'))

describe('tiles', () => {
  it('draws one thumbnail per group at the page thumbUrl, alt 1-based', () => {
    const host = mount(<Filmstrip {...props()} />)
    const btns = buttons(host)
    expect(btns).toHaveLength(4)
    const img0 = btns[0].querySelector('img')
    expect(img0?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/en-mono/pages/10/thumb',
    )
    expect(img0?.getAttribute('alt')).toBe('page 11')
    expect(img0?.getAttribute('loading')).toBe('lazy')
  })

  // The half badge and the half-aware alt text both exist so a joined
  // spread's two correction tiles — same thumbnail, same base alt text —
  // can be told apart. A slot with no half gets neither.
  it('names the half in alt text and shows a badge only for a half slot', () => {
    const host = mount(<Filmstrip {...props()} />)
    const btns = buttons(host)
    const imgR = btns[2].querySelector('img')
    expect(imgR?.getAttribute('alt')).toBe('page 3 R')
    expect(btns[2].textContent).toBe('R')

    const imgL = btns[3].querySelector('img')
    expect(imgL?.getAttribute('alt')).toBe('page 3 L')
    expect(btns[3].textContent).toBe('L')

    expect(btns[0].textContent).toBe('')
    expect(btns[0].querySelector('img')?.getAttribute('alt')).toBe('page 11')
  })

  // A pair (`two` mode's non-spread grouping): both slots are separate
  // pages, so `tilePages` keeps two `<img>`s under the one button and
  // neither carries a half.
  it('draws both slots of a pair under one tile, neither with a half', () => {
    const host = mount(
      <Filmstrip {...props({ groups: [{ slots: [0, 1] }] })} />,
    )
    const imgs = host.querySelectorAll('img')
    expect(imgs).toHaveLength(2)
    expect(imgs[0].getAttribute('alt')).toBe('page 11')
    expect(imgs[1].getAttribute('alt')).toBe('page 12')
  })

  // A joined spread (`spread: true`): one wide image for two slots, at the
  // first slot's own page — `tilePages`' other branch, reached only this
  // way (never through a plain multi-slot group).
  it('draws a joined spread as a single tile at its first slot’s page', () => {
    const host = mount(
      <Filmstrip {...props({ groups: [{ slots: [2, 3], spread: true }] })} />,
    )
    const imgs = host.querySelectorAll('img')
    expect(imgs).toHaveLength(1)
    expect(imgs[0].getAttribute('alt')).toBe('page 3')
  })
})

describe('current tile', () => {
  it('marks the current group aria-current with a solid white border', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    const btns = buttons(host)
    expect(btns[1].getAttribute('aria-current')).toBe('true')
    expect(btns[1].style.borderColor).toBe('rgb(255, 255, 255)')
    expect(btns[1].style.borderStyle).toBe('solid')
  })

  it('leaves every other tile plain, with no aria-current and a transparent border', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    const btns = buttons(host)
    for (const i of [0, 2, 3]) {
      expect(btns[i].getAttribute('aria-current')).toBeNull()
      expect(btns[i].style.borderColor).toBe('transparent')
    }
  })

  it('centres the rail on the current tile on mount', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 2 })} />)
    expect(scrolledTo).toEqual([buttons(host)[2]])
  })

  it('recentres when the same instance’s currentIndex changes', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 0 })} />)
    expect(scrolledTo).toEqual([buttons(host)[0]])
    act(() => {
      rerender(<Filmstrip {...props({ currentIndex: 3 })} />)
    })
    expect(scrolledTo.at(-1)).toBe(buttons(host)[3])
  })
})

describe('correcting: the resolved (dashed) tile', () => {
  it('marks the group holding pairedIndex with a dashed amber border', () => {
    const host = mount(
      <Filmstrip
        {...props({
          currentIndex: -1,
          correcting: { to: 'ja-raw', pairedIndex: 2 },
        })}
      />,
    )
    const btns = buttons(host)
    expect(btns[2].style.borderColor).toBe('rgb(242, 177, 52)')
    expect(btns[2].style.borderStyle).toBe('dashed')
    // Resolved is not current: no aria-current on it either.
    expect(btns[2].getAttribute('aria-current')).toBeNull()
  })

  it('leaves every tile plain when pairedIndex has not resolved yet', () => {
    const host = mount(
      <Filmstrip
        {...props({
          currentIndex: -1,
          correcting: { to: 'ja-raw' },
        })}
      />,
    )
    for (const btn of buttons(host)) {
      expect(btn.style.borderColor).toBe('transparent')
    }
  })

  it('centres the rail on the resolved tile', () => {
    const host = mount(
      <Filmstrip
        {...props({
          currentIndex: -1,
          correcting: { to: 'ja-raw', pairedIndex: 3 },
        })}
      />,
    )
    expect(scrolledTo).toEqual([buttons(host)[3]])
  })
})

describe('anchor', () => {
  it('centres on the anchor tile, unmarked, when nothing is current or resolving', () => {
    const host = mount(
      <Filmstrip {...props({ currentIndex: -1, anchor: 3 })} />,
    )
    const btns = buttons(host)
    expect(scrolledTo).toEqual([btns[3]])
    // "Marking it either way would put a claim on screen" — no border color
    // at all, current or resolved, for the anchor tile.
    expect(btns[3].style.borderColor).toBe('transparent')
    expect(btns[3].getAttribute('aria-current')).toBeNull()
  })

  // The contract `isAnchor` re-tests rather than trusting the caller: an
  // anchor set alongside a real `currentIndex` must lose to the current
  // tile, not attach a second ref. Observed through which button actually
  // gets centred — the anchor ref is either attached or it isn't, and
  // `scrollIntoView`'s target is the only externally visible trace of that.
  it('loses to a real currentIndex set alongside it', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1, anchor: 3 })} />)
    const btns = buttons(host)
    expect(scrolledTo).toEqual([btns[1]])
    expect(btns[3].getAttribute('aria-current')).toBeNull()
  })

  it('loses to an active correction set alongside it', () => {
    const host = mount(
      <Filmstrip
        {...props({
          currentIndex: -1,
          anchor: 3,
          correcting: { to: 'ja-raw', pairedIndex: 2 },
        })}
      />,
    )
    const btns = buttons(host)
    expect(scrolledTo).toEqual([btns[2]])
  })
})

describe('clicking a tile', () => {
  it('navigates to the group’s first slot while not correcting', () => {
    const onNavigate = vi.fn()
    const host = mount(<Filmstrip {...props({ onNavigate })} />)
    act(() => {
      buttons(host)[2].click()
    })
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
  })

  it('asserts the counterpart instead, while correcting with a handler', () => {
    const onNavigate = vi.fn()
    const onPickCounterpart = vi.fn()
    const host = mount(
      <Filmstrip
        {...props({
          onNavigate,
          onPickCounterpart,
          correcting: { to: 'ja-raw' },
        })}
      />,
    )
    act(() => {
      buttons(host)[3].click()
    })
    expect(onPickCounterpart).toHaveBeenCalledExactlyOnceWith(3)
    expect(onNavigate).not.toHaveBeenCalled()
  })

  it('falls back to navigation while correcting with no onPickCounterpart', () => {
    const onNavigate = vi.fn()
    const host = mount(
      <Filmstrip {...props({ onNavigate, correcting: { to: 'ja-raw' } })} />,
    )
    act(() => {
      buttons(host)[0].click()
    })
    expect(onNavigate).toHaveBeenCalledExactlyOnceWith(0)
  })
})

describe('preview while correcting', () => {
  it('reports the hovered tile’s page/half on enter and clears it on leave', () => {
    const onPreview = vi.fn()
    const host = mount(
      <Filmstrip {...props({ onPreview, correcting: { to: 'ja-raw' } })} />,
    )
    const target = buttons(host)[2]
    act(() => {
      target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    })
    expect(onPreview).toHaveBeenLastCalledWith({ page: 2, half: 'R' })
    act(() => {
      target.dispatchEvent(new MouseEvent('mouseout', { bubbles: true }))
    })
    expect(onPreview).toHaveBeenLastCalledWith(undefined)
  })

  it('reports the same preview on keyboard focus and clears it on blur', () => {
    const onPreview = vi.fn()
    const host = mount(
      <Filmstrip {...props({ onPreview, correcting: { to: 'ja-raw' } })} />,
    )
    const target = buttons(host)[0]
    act(() => {
      target.focus()
    })
    expect(onPreview).toHaveBeenLastCalledWith({ page: 10, half: undefined })
    act(() => {
      target.blur()
    })
    expect(onPreview).toHaveBeenLastCalledWith(undefined)
  })

  // "Both are no-ops while reading" — a preview of the page already on
  // screen would be noise, so hovering must not call `onPreview` at all.
  it('does nothing on hover or focus while not correcting', () => {
    const onPreview = vi.fn()
    const host = mount(<Filmstrip {...props({ onPreview })} />)
    const target = buttons(host)[0]
    act(() => {
      target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
      target.focus()
    })
    expect(onPreview).not.toHaveBeenCalled()
  })
})

describe('reading direction', () => {
  it('lays the rail out row for ltr and row-reverse for rtl', () => {
    const ltr = mount(<Filmstrip {...props({ reading: 'ltr' })} />)
    expect(ltr.firstElementChild).toHaveProperty('style.flexDirection', 'row')
    const rtl = mount(<Filmstrip {...props({ reading: 'rtl' })} />)
    expect(rtl.firstElementChild).toHaveProperty(
      'style.flexDirection',
      'row-reverse',
    )
  })
})

describe('recentring on a thumbnail load', () => {
  // A tile has no explicit size, so a not-yet-decoded `<img loading="lazy">`
  // lays out near zero width — the rail listens for `load` in the capture
  // phase (it does not bubble) precisely so a thumbnail that finishes
  // loading after the initial centring call re-triggers it. Dispatched here
  // without `bubbles`, since the capture phase reaches every descendant
  // regardless — a listener that (wrongly) relied on bubbling would still
  // pass a `bubbles: true` version of this test, which is why this one
  // deliberately omits it.
  it('recentres when a descendant thumbnail fires a non-bubbling load event', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    expect(scrolledTo).toHaveLength(1)
    const img = buttons(host)[1].querySelector('img')
    act(() => {
      img?.dispatchEvent(new Event('load'))
    })
    expect(scrolledTo).toHaveLength(2)
    expect(scrolledTo[1]).toBe(buttons(host)[1])
  })
})
