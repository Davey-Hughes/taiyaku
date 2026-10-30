import { act, Profiler } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { Filmstrip, type FilmstripProps } from '../src/Filmstrip'
import type { Group } from '../src/pairing'
import type { Position } from '../src/resolver'
import { mount, rerender } from './support/mount'
import { shownShare } from '../src/tiles'

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

// jsdom carries no `ResizeObserver`, having no layout to observe, and every
// mount below builds one on the rail. This one keeps what it was handed, so a
// test can say the rail's box changed by calling `changed` itself.
class StubResizeObserver {
  static made: StubResizeObserver[] = []
  readonly changed: () => void
  readonly observed: Element[] = []
  disconnected = false
  constructor(changed: () => void) {
    this.changed = changed
    StubResizeObserver.made.push(this)
  }
  observe(target: Element) {
    this.observed.push(target)
  }
  unobserve() {}
  disconnect() {
    this.disconnected = true
  }
}
globalThis.ResizeObserver =
  StubResizeObserver as unknown as typeof ResizeObserver
beforeEach(() => {
  StubResizeObserver.made = []
})

// Four slots: two ordinary pages, and one joined spread's two halves —
// addressed as their own singleton groups, the shape `pairing.ts`'s
// `correctionTiles` produces while correcting (see `FilmstripProps.groups`'
// own doc). That shape is what exercises the half badge and the half-aware
// alt text without needing `pairing.ts` itself in this file at all.
//
// Load-bearing, since O140: this, `Chrome.test.tsx`'s 3 groups and
// `App.test.tsx`'s 6 pages all sit inside the rail's 33-tile window
// (`windowMargin` either side of the focus), so `lo` is always 0, no leading
// spacer renders, and `buttons(host)[i]` still addresses tile `i` directly.
// Lengthening any of those three fixtures past the window would silently
// shift index-based lookups like that one onto the wrong tile — `groups`
// gaining a leading spacer changes what `buttons(host)[0]` points at without
// changing this file's assertions in any way that would flag it.
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
    expect(img0?.getAttribute('alt')).toBe('page 11')
    expect(img0?.getAttribute('loading')).toBe('lazy')

    // jsdom has no `matchMedia`, so `stripTileEdge` takes its wide branch at
    // `devicePixelRatio` 1 and every tile asks for 160 — today's image at an
    // explicit size. The rung that varies is the browser project's to assert
    // (`Filmstrip.browser.test.tsx`); what this pins is that the URL carries
    // one at all, on every tile, and that it is the same one.
    const images = [...host.querySelectorAll('img')]
    expect(images).toHaveLength(groups.length)
    for (const [i, img] of images.entries()) {
      expect(img.getAttribute('src')).toBe(
        `/api/works/gotoubun-v01/en-mono/pages/${slots[i].page}/thumb?w=160`,
      )
    }
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
    expect(btns[2].querySelector('.reader-half-badge')?.textContent).toBe('R')

    const imgL = btns[3].querySelector('img')
    expect(imgL?.getAttribute('alt')).toBe('page 3 L')
    expect(btns[3].textContent).toBe('L')
    expect(btns[3].querySelector('.reader-half-badge')?.textContent).toBe('L')

    expect(btns[0].textContent).toBe('')
    expect(btns[0].querySelector('img')?.getAttribute('alt')).toBe('page 11')
    expect(btns[0].querySelector('.reader-half-badge')).toBeNull()
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

  // jsdom has no layout engine, so these assert the style the component
  // *writes*, not a measured box. The measured box is the browser test's
  // job (`Filmstrip.browser.test.tsx`).
  it('gives every tile a floor width in nominal page-widths', () => {
    const host = mount(<Filmstrip {...props()} />)
    for (const btn of buttons(host)) {
      expect(btn.style.minWidth).toBe(
        'calc(var(--strip-unit) * 1 + var(--strip-tile-border) * 2)',
      )
    }
    // A floor, not a fixed size — a loaded image may draw wider.
    expect(buttons(host)[0].style.width).toBe('')
  })

  it('gives a joined spread twice the floor of a single page', () => {
    // One spread group, two slots sharing page 2, beside two ordinary pages.
    const spreadGroups: Group[] = [
      { slots: [0] },
      { slots: [2, 3], spread: true },
    ]
    const host = mount(
      <Filmstrip {...props({ groups: spreadGroups, currentIndex: 0 })} />,
    )
    const [single, spread] = buttons(host)
    expect(single.style.minWidth).toBe(
      'calc(var(--strip-unit) * 1 + var(--strip-tile-border) * 2)',
    )
    expect(spread.style.minWidth).toBe(
      'calc(var(--strip-unit) * 2 + var(--strip-tile-border) * 2)',
    )
  })
})

describe('current tile', () => {
  it('marks the current group aria-current with a solid accent border', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    const btns = buttons(host)
    expect(btns[1].getAttribute('aria-current')).toBe('true')
    expect(btns[1].style.borderColor).toBe('var(--accent)')
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
    expect(btns[2].style.borderColor).toBe('var(--warning)')
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

describe('the rail', () => {
  // O138. The class is what `styles/reader.css` hides the rail's scrollbar
  // by: `styles/reader.css.test.ts` holds the declarations, and
  // `Filmstrip.browser.test.tsx` that the two meet.
  it('wears the class the stylesheet hides its scrollbar by', () => {
    const host = mount(<Filmstrip {...props()} />)
    expect((host.firstElementChild as HTMLElement).className).toBe(
      'reader-strip-rail',
    )
  })
})

describe('recentring on a thumbnail load', () => {
  // This used to pin the opposite: a rail-level `load` listener that
  // re-centred on every descendant thumbnail's load, added back when a tile
  // had no width of its own and a lazy image decoding could collapse the
  // layout. A `minWidth` floor (Task 2a, 2026-09-21) closed that gap, and
  // windowing turned the listener from a safeguard into the regression this
  // pins against: scrolling the rail renders new tiles, whose images then
  // load, so the listener fired continuously while the reader was mid-flick
  // and dragged the rail back to the current tile — reported from a real
  // iPhone as a flick that swipes and then returns within about a second,
  // which made the rail impossible to make progress on. The fix removed the
  // listener rather than narrowing it, so a thumbnail's `load` must now be a
  // no-op for the rail.
  it('does not recentre when a descendant thumbnail fires a load event', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    expect(scrolledTo).toHaveLength(1)
    const img = buttons(host)[1].querySelector('img')
    act(() => {
      img?.dispatchEvent(new Event('load'))
    })
    expect(scrolledTo).toHaveLength(1)
  })
})

// jsdom has no layout engine: `getBoundingClientRect` reports all-zero and
// `<img>.naturalWidth` never becomes non-zero on their own, so both are
// stubbed by hand below to stand in for a tile that has actually drawn.
// `--strip-tile-border` and `--strip-unit` are likewise set directly on the
// rail's own inline style rather than left to a stylesheet: this file never
// imports `reader.css` (only `Filmstrip.browser.test.tsx` does, against a
// real layout engine — see that file's own comment on the import), and
// jsdom's `getComputedStyle` only ever reflects an element's own inline
// style, never one cascaded from a class selector.
describe('measuring --strip-unit at runtime', () => {
  // The runtime measurement needs at least 5 loaded one-unit tiles before it
  // trusts a median (`medianTileWidth`'s own minimum sample, `tiles.ts`), so
  // these tests need more tiles than the shared top-level `groups` fixture
  // (4) has — an 8-tile, one-group-per-slot fixture of its own, small
  // enough to stay inside the rail's window (`lo` stays 0), so
  // `buttons(host)[i]` still addresses tile `i` directly.
  const unitSlots: Position[] = Array.from({ length: 8 }, (_, i) => ({
    page: i,
  }))
  const unitGroups: Group[] = unitSlots.map((_, i) => ({ slots: [i] }))
  const unitProps = (overrides: Partial<FilmstripProps> = {}) =>
    props({
      slots: unitSlots,
      groups: unitGroups,
      currentIndex: 0,
      ...overrides,
    })

  function primeRail(host: HTMLElement, { border = 2, currentUnit = 54 } = {}) {
    const rail = host.firstElementChild as HTMLElement
    rail.style.setProperty('--strip-tile-border', `${border}px`)
    rail.style.setProperty('--strip-unit', `${currentUnit}px`)
    return rail
  }

  // Stubs a loaded tile's measured box and its image's `naturalWidth` — the
  // two jsdom leaves at their untouched defaults (an all-zero rect and a
  // `naturalWidth` of 0) that the real effect reads to decide whether a tile
  // has actually drawn. Returns the image, so a caller can dispatch `load`
  // on whichever one it likes once every tile it needs is stubbed — the
  // effect rescans the whole rail on each `load`, not just the tile that
  // fired it, so which one dispatches does not matter.
  function stubLoaded(host: HTMLElement, index: number, width: number) {
    const btn = buttons(host)[index]
    btn.getBoundingClientRect = () => ({ width }) as DOMRect
    const img = btn.querySelector('img') as HTMLImageElement
    Object.defineProperty(img, 'naturalWidth', {
      value: width,
      configurable: true,
    })
    return img
  }

  // O140's measured phone case (390×844, dpr 3, mobile emulation): a
  // loaded one-unit tile's outer width is 68.19 px. Border 2 px either side
  // gives a measured unit of 64.19, ten-odd pixels past the nominal 54 the
  // rail is primed with — well outside the 0.5 px tolerance, so the
  // corrected value is written onto the rail once `medianTileWidth`'s
  // minimum sample (3, fix-round review finding A) agrees on it.
  it('writes the median measured unit once at least 3 one-unit tiles have loaded, past tolerance', () => {
    const host = mount(<Filmstrip {...unitProps()} />)
    const rail = primeRail(host)
    let lastImg: HTMLImageElement | undefined
    for (const i of [0, 1, 2]) {
      lastImg = stubLoaded(host, i, 68.19)
    }
    act(() => {
      lastImg?.dispatchEvent(new Event('load'))
    })
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('64.19px')
  })

  it('does not write the property when no tile has loaded', () => {
    const host = mount(<Filmstrip {...unitProps()} />)
    const rail = primeRail(host)
    const setProperty = vi.spyOn(rail.style, 'setProperty')
    for (const btn of buttons(host)) {
      // Every tile draws wide enough to matter if it were read — the point
      // of this test is that none of them qualifies, because no `img` here
      // has a non-zero `naturalWidth` (jsdom's untouched default).
      btn.getBoundingClientRect = () => ({ width: 68.19 }) as DOMRect
    }
    act(() => {
      buttons(host)[1].querySelector('img')?.dispatchEvent(new Event('load'))
    })
    expect(setProperty).not.toHaveBeenCalledWith(
      '--strip-unit',
      expect.anything(),
    )
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('54px')
  })

  // C2 (whole-branch review): the minimum-sample guard on its own terms — 2
  // loaded tiles is one short of `medianTileWidth`'s default floor of 3
  // (lowered from 5 by fix-round review finding A, now that every loaded
  // tile contributes rather than only one-unit ones), so even a unanimous,
  // well-past-tolerance width from both of them must not write anything
  // yet. Without this floor, an early render with a single loaded tile (or
  // a small handful) could set the property off of a sample too small to
  // trust — see `medianTileWidth`'s own doc for why the same floor is also
  // what stops one unmarked double-page scan from deciding the measurement
  // on its own.
  it('does not write the property when fewer than the minimum sample have loaded', () => {
    const host = mount(<Filmstrip {...unitProps()} />)
    const rail = primeRail(host)
    const setProperty = vi.spyOn(rail.style, 'setProperty')
    let lastImg: HTMLImageElement | undefined
    for (const i of [0, 1]) {
      lastImg = stubLoaded(host, i, 68.19)
    }
    act(() => {
      lastImg?.dispatchEvent(new Event('load'))
    })
    expect(setProperty).not.toHaveBeenCalledWith(
      '--strip-unit',
      expect.anything(),
    )
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('54px')
  })

  // Finding A, part 2 (fix-round review): the ref flag above only fires once
  // `medianTileWidth` reaches its minimum sample — a rail with fewer tiles
  // than that can never reach it, so the `load` listener needs a second way
  // to detach or it stays attached for the component's whole life, forcing
  // a layout read on every future load. `unitGroups` sliced to 2 tiles is
  // one below the minimum sample (3): the property never gets measured, but
  // the listener still has to let go once both of the only tiles it could
  // ever read have loaded.
  it('detaches the load listener once every rendered tile has loaded, even below the minimum sample', () => {
    const host = mount(
      <Filmstrip {...unitProps({ groups: unitGroups.slice(0, 2) })} />,
    )
    const rail = primeRail(host)
    const removeEventListener = vi.spyOn(rail, 'removeEventListener')

    const img0 = stubLoaded(host, 0, 68.19)
    act(() => {
      img0.dispatchEvent(new Event('load'))
    })
    expect(removeEventListener).not.toHaveBeenCalledWith(
      'load',
      expect.anything(),
      true,
    )

    const img1 = stubLoaded(host, 1, 68.19)
    act(() => {
      img1.dispatchEvent(new Event('load'))
    })
    expect(removeEventListener).toHaveBeenCalledWith(
      'load',
      expect.anything(),
      true,
    )
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('54px')
  })

  // A loaded tile 58.16 px wide against a 58 px nominal (54 unit + 2×2
  // border) is a 0.16 px difference, inside tolerance, which must not cause
  // a write.
  it('does not write the property when the measured median is within tolerance', () => {
    const host = mount(<Filmstrip {...unitProps()} />)
    const rail = primeRail(host)
    const setProperty = vi.spyOn(rail.style, 'setProperty')
    let lastImg: HTMLImageElement | undefined
    for (const i of [0, 1, 2, 3, 4]) {
      lastImg = stubLoaded(host, i, 58.16)
    }
    act(() => {
      lastImg?.dispatchEvent(new Event('load'))
    })
    expect(setProperty).not.toHaveBeenCalledWith(
      '--strip-unit',
      expect.anything(),
    )
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('54px')
  })

  // C2 (whole-branch review): the failure this branch fixes. A double-page
  // scan the sidecar never marked `spread: true` is a one-slot group, so
  // `tileUnits` correctly calls it one unit and it carries the same
  // one-unit `minWidth` floor as an ordinary tile — while drawing at
  // roughly twice the nominal width (tile 0 here, at 132 px against an
  // ordinary tile's 68.19). Taking the first qualifying tile, the rule this
  // replaces, would have accepted 132 outright whenever it happened to load
  // first and corrupted `--strip-unit` to about double the true value —
  // reproduced with exactly this stubbed width during the whole-branch
  // review that found it. The median of `[132, 68.19, 68.19, 68.19, 68.19]`
  // is 68.19: the outlier is a minority of the sample and cannot move it.
  it('derives the ordinary unit even when one loaded one-unit tile is an unmarked double-page scan', () => {
    const host = mount(<Filmstrip {...unitProps()} />)
    const rail = primeRail(host)
    stubLoaded(host, 0, 132)
    let lastImg: HTMLImageElement | undefined
    for (const i of [1, 2, 3, 4]) {
      lastImg = stubLoaded(host, i, 68.19)
    }
    act(() => {
      lastImg?.dispatchEvent(new Event('load'))
    })
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('64.19px')
  })

  // The convergence guard: this measures at most once per mount (a ref
  // flag, not the tolerance, is what stops it), so tiles loading later — at
  // widths that would themselves justify a very different median — must not
  // move the property again.
  //
  // Finding B (fix-round review): the previous version of this test
  // re-stubbed only a sixth, outlying tile (999 among five 68.19s) before
  // the second load. An odd sample's outlier sorts to one end and never
  // reaches the middle, so a second measurement — if the ref did not block
  // one — would recompute the *same* 68.19 median and write the *same*
  // 64.19px regardless: deleting `unitMeasuredRef.current = true` in
  // `Filmstrip.tsx` still passed this test, which is what the comment above
  // claims but the test itself never checked. Re-stubbing the five tiles
  // that *form* the median — not a sixth tile outside it — is what makes a
  // second measurement, if one ran, want to write something else: median
  // 300, unit 296, nowhere near 64.19px.
  it('measures at most once per mount, whatever loads afterwards', () => {
    const host = mount(<Filmstrip {...unitProps()} />)
    const rail = primeRail(host)
    let lastImg: HTMLImageElement | undefined
    for (const i of [0, 1, 2, 3, 4]) {
      lastImg = stubLoaded(host, i, 68.19)
    }
    act(() => {
      lastImg?.dispatchEvent(new Event('load'))
    })
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('64.19px')

    for (const i of [0, 1, 2, 3, 4]) {
      lastImg = stubLoaded(host, i, 300)
    }
    act(() => {
      lastImg?.dispatchEvent(new Event('load'))
    })
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('64.19px')
  })

  // fix-round review, finding A: a two-unit tile no longer sits out of the
  // sample the way it used to (see this describe block's own tests above,
  // now updated) — `normalizedTileWidth` (`tiles.ts`) rescales its own outer
  // width back to what a one-unit tile of the same per-unit width would have
  // measured, so a genuine `spread: true` tile (`tileMinWidth(2)`) joins the
  // same sample the two ordinary one-unit tiles beside it do, rather than
  // being excluded from it. 132.38 is chosen so its normalised figure agrees
  // with theirs exactly — `(132.38 - 2*2) / 2 + 2*2` is 68.19, the same
  // outer width the ordinary tiles measure here — so all three agreeing
  // reaches the new minimum sample of 3 and writes.
  it('normalises a two-unit tile into the same sample a one-unit tile contributes to', () => {
    const spreadGroups: Group[] = [
      { slots: [0] },
      { slots: [1] },
      { slots: [2, 3], spread: true },
    ]
    const host = mount(
      <Filmstrip {...props({ groups: spreadGroups, currentIndex: 0 })} />,
    )
    const rail = primeRail(host)
    const [one, two, spread] = buttons(host)
    for (const btn of [one, two]) {
      const img = btn.querySelector('img') as HTMLImageElement
      btn.getBoundingClientRect = () => ({ width: 68.19 }) as DOMRect
      Object.defineProperty(img, 'naturalWidth', {
        value: 68.19,
        configurable: true,
      })
    }
    const spreadImg = spread.querySelector('img') as HTMLImageElement
    spread.getBoundingClientRect = () => ({ width: 132.38 }) as DOMRect
    Object.defineProperty(spreadImg, 'naturalWidth', {
      value: 132.38,
      configurable: true,
    })
    act(() => {
      spreadImg.dispatchEvent(new Event('load'))
    })
    expect(rail.style.getPropertyValue('--strip-unit')).toBe('64.19px')
  })
})

describe('windowing', () => {
  // 120 ordinary pages — long enough that a 16-tile margin leaves most of
  // the rail unrendered.
  const longSlots: Position[] = Array.from({ length: 120 }, (_, i) => ({
    page: i,
  }))
  const longGroups: Group[] = longSlots.map((_, i) => ({ slots: [i] }))
  const longProps = (overrides: Partial<FilmstripProps> = {}) =>
    props({ slots: longSlots, groups: longGroups, ...overrides })

  const spacers = (host: HTMLElement) =>
    Array.from(host.querySelectorAll('[data-testid="strip-spacer"]'))

  // jsdom has no layout engine, so every element's `getBoundingClientRect`
  // reports all-zero until stubbed — and the scroll handler now reads it (on
  // the rail and on the rendered tiles) to decide whether to rebuild the
  // window. A left/width pair is all any caller here needs, so this fills in
  // the rest of `DOMRect`'s shape rather than asking every call site to.
  function rect(left: number, width: number) {
    return {
      x: left,
      y: 0,
      left,
      top: 0,
      right: left + width,
      bottom: 0,
      width,
      height: 0,
      toJSON: () => ({}),
    } as DOMRect
  }

  it('renders a window of tiles rather than one per page', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    expect(buttons(host).length).toBeLessThan(40)
    expect(buttons(host).length).toBeGreaterThan(0)
  })

  it('always renders the current tile, and marks it', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    const current = host.querySelector('[aria-current="true"]')
    expect(current).not.toBeNull()
    expect(current?.querySelector('img')?.getAttribute('alt')).toBe('page 61')
  })

  it('stands a spacer either side of a window in the middle of the rail', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    expect(spacers(host)).toHaveLength(2)
  })

  it('has no leading spacer when the window starts at the first tile', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    expect(spacers(host)).toHaveLength(1)
  })

  it('has no trailing spacer when the window ends at the last tile', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 119 })} />)
    expect(spacers(host)).toHaveLength(1)
  })

  // The window follows whichever tile the centring chain would centre on —
  // the same `currentRef ?? pairedRef ?? anchorRef` order the component
  // already documents. A window built only around `currentIndex` would
  // leave the tile a correction is about unrendered, and `scrollIntoView`
  // with nothing to scroll to.
  it('is built around the resolved tile while correcting, not the current one', () => {
    const host = mount(
      <Filmstrip
        {...longProps({
          currentIndex: -1,
          correcting: { to: 'ed2', pairedIndex: 90 },
          onPickCounterpart: vi.fn(),
        })}
      />,
    )
    const dashed = Array.from(host.querySelectorAll('button')).find(
      (b) => b.style.borderStyle === 'dashed',
    )
    expect(dashed).toBeDefined()
    expect(dashed?.querySelector('img')?.getAttribute('alt')).toBe('page 91')
  })

  it('is built around the anchor where this edition has no slot at all', () => {
    const host = mount(
      <Filmstrip {...longProps({ currentIndex: -1, anchor: 75 })} />,
    )
    const alts = Array.from(host.querySelectorAll('img')).map((i) =>
      i.getAttribute('alt'),
    )
    expect(alts).toContain('page 76')
  })

  // The scroll effect throttles its work to one animation frame, but the
  // test below dispatches `scroll` synchronously and asserts straight after
  // — without this, the frame it scheduled would run after the assertion.
  beforeEach(() => {
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      cb(0)
      return 0
    })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  // jsdom reports every box as zero, so a real scroll cannot be simulated.
  // What can be: that the rail listens, and that a scroll to the far end
  // moves the window there. `scrollWidth`/`clientWidth` are defined on the
  // element directly for this one test, and so is the rail's own bounding
  // box (`rect(0, 400)`, an arbitrary positive width starting at x 0) — the
  // scroll handler reads it to size its guard, and left unstubbed it would
  // be zero too, which makes the guard zero and `nearAGap` false, and the
  // window would never rebuild. The rendered spacer's own box is
  // deliberately left at jsdom's zero default rather than stubbed away from
  // the rail: a zero-width spacer at (0, 0) sits inside the guarded box this
  // rail's stub produces (-100..500), which is exactly "a spacer near the
  // strip" — this test is about the window moving on a scroll, not about the
  // guard's exact boundary (that is
  // `describe('re-anchoring on the rendered window running out')`, below,
  // where the spacer boxes are stubbed on purpose to land on either side of
  // it).
  //
  // The target page is the fixture's very last one, not an arbitrary one
  // partway along: `longSlots` is 120 tiles and `windowMargin` is 16, so a
  // scroll to the true far end (fraction 1) centres the window on index 119
  // and renders only 104..120 — a page much short of the end (e.g. 100)
  // would still be outside that window and the assertion would be wrong
  // about what "the far end" reaches.
  it('extends the window to where the reader scrolls', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    const rail = host.firstElementChild as HTMLElement
    expect(
      Array.from(host.querySelectorAll('img')).map((i) => i.alt),
    ).not.toContain('page 120')

    Object.defineProperty(rail, 'scrollWidth', {
      value: 1200,
      configurable: true,
    })
    Object.defineProperty(rail, 'clientWidth', {
      value: 200,
      configurable: true,
    })
    rail.getBoundingClientRect = () => rect(0, 400)
    rail.scrollLeft = 1000 // the far end
    act(() => {
      rail.dispatchEvent(new Event('scroll'))
    })

    expect(
      Array.from(host.querySelectorAll('img')).map((i) => i.alt),
    ).toContain('page 120')
  })

  // The regression this guards against (reported from a real iPhone):
  // scrolling the rail renders new tiles the way the test above shows, and
  // those tiles' images then load — before the `load` listener was removed
  // from `Filmstrip.tsx`'s centring effect, that firing while the reader was
  // still mid-flick called `scrollIntoView` on the current tile and dragged
  // the rail straight back to it, so a flick "does swipe but then returns"
  // within about a second and the reader could not make progress. The
  // sequence here is the same scroll as the test above, then a `load` on a
  // tile the scroll newly rendered — one that was not on screen before the
  // window moved, matching what a real flick produces.
  it('leaves the rail alone when a thumbnail loads after a scroll moved the window', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    const rail = host.firstElementChild as HTMLElement

    Object.defineProperty(rail, 'scrollWidth', {
      value: 1200,
      configurable: true,
    })
    Object.defineProperty(rail, 'clientWidth', {
      value: 200,
      configurable: true,
    })
    // See the previous test's comment for why the rail's box is stubbed and
    // the spacer's is not.
    rail.getBoundingClientRect = () => rect(0, 400)
    rail.scrollLeft = 1000 // the far end
    act(() => {
      rail.dispatchEvent(new Event('scroll'))
    })
    expect(rail.scrollLeft).toBe(1000)
    const callsAfterScroll = scrolledTo.length

    const img = rail.querySelector('img[alt="page 120"]')
    expect(img).not.toBeNull()
    act(() => {
      img?.dispatchEvent(new Event('load'))
    })

    expect(rail.scrollLeft).toBe(1000)
    expect(scrolledTo).toHaveLength(callsAfterScroll)
  })

  // Finding C (fix-round review): the test above takes its baseline *after*
  // the scroll that moves the window, so a re-centre the scroll itself
  // caused would already be absorbed into `callsAfterScroll` and go
  // unnoticed — it only pins a re-centre caused by a *subsequent* load. The
  // C1 regression this guards against (a `pendingRecentre` ref and a
  // no-dependency retry effect that runs after every render, including the
  // one `setScrolledTo` below triggers) could just as easily be reintroduced
  // by the scroll-driven re-render itself, which is exactly the shape this
  // pins: the baseline is taken *before* the `act()` that dispatches the
  // scroll.
  //
  // `currentIndex: 110`, not 0: the far-end scroll's new window is
  // `stripWindow(119, 120, 16)` = tiles 103..119, and unless the current
  // tile is inside *that* window there is nothing for a wrongly-set
  // `pendingRecentre` to find — `recenter()` would fail either way and the
  // assertion below would pass whether or not the guard actually held. 110
  // sits inside both the initial window (94..119) and the post-scroll one
  // (103..119), so `currentRef` stays attached throughout and a stray
  // re-centre has something to act on.
  it('does not recentre the rail on the re-render a window-moving scroll itself causes', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 110 })} />)
    const rail = host.firstElementChild as HTMLElement
    const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)

    Object.defineProperty(rail, 'scrollWidth', {
      value: 1200,
      configurable: true,
    })
    Object.defineProperty(rail, 'clientWidth', {
      value: 200,
      configurable: true,
    })
    rail.getBoundingClientRect = () => rect(0, 400)
    rail.scrollLeft = 1000 // the far end — moves the window, as above

    const callsBeforeScroll = scrolledTo.length
    act(() => {
      rail.dispatchEvent(new Event('scroll'))
    })

    // Confirms the scroll really did move the window — otherwise a passing
    // assertion below would prove nothing, since no re-render would have
    // happened at all.
    const after = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
    expect(after).not.toEqual(before)
    expect(scrolledTo).toHaveLength(callsBeforeScroll)
  })

  // O140's fix: the scroll handler re-anchors once a *spacer* comes within a
  // quarter of the rail's width of the visible rail, read via
  // `getBoundingClientRect` on the rail and on each rendered spacer — not on
  // a count of tiles the focus has drifted (a drift-based rule was replaced: it
  // assumed the viewport straddles the focus tile, when the focus is the rail's
  // leading edge and the viewport extends forward from it, which cost a 321 px
  // gap at 1440 px), and not on the first/last *rendered tile's* box either —
  // an earlier version of this fix tried that, checking both ends of the DOM
  // order against both ends of the rail on the reasoning that checking both
  // made it agnostic to reading direction. It did not: the rail is
  // `row-reverse` under `rtl`, so the last tile in the DOM sits at the left of
  // the screen, not the right, and checking both ends of the DOM order makes
  // one of the two checks true on nearly every frame instead. Measured driving
  // a 1.2 s flick at 390×844 under 4× CPU throttle: 71 re-anchors across 182
  // frames, worse than the 65 the drift-based rule produced and worse than the
  // 24 the spacer rule below achieves. A spacer reaching the visible strip *is*
  // the blank gap this guard exists to prevent, which is symmetric under either
  // flex direction with no reading-direction arithmetic needed.
  //
  // jsdom has no layout engine — every element's `getBoundingClientRect`
  // reports all-zero by default — so the rail's and the spacers' boxes are
  // stubbed explicitly below. That means these tests pin the *decision
  // rule* (rebuild once a spacer's box overlaps a quarter-rail-width guard
  // around the visible rail), not real geometry;
  // `Filmstrip.browser.test.tsx` is what exercises the rule against an
  // actual layout.
  //
  // `units` are one per tile here (`longSlots` is 120 ordinary pages), and
  // `scrollWidth`/`clientWidth` are set so `scrollLeft` reads back as the
  // fraction `focusAtFraction` needs to land on an exact tile index — see
  // `focusAtFraction`'s own tests in `tiles.test.ts` for the same trick.
  describe('re-anchoring on the rendered window running out', () => {
    // Rail 400 px wide, so the guard (a quarter of its width) is 100 px:
    // the check fires once a spacer's box overlaps the rail's own box
    // widened by 100 px on each side. `currentIndex: 60` in this 120-tile,
    // 16-margin fixture renders both a leading and a trailing spacer
    // (stripWindow(60, 120, 16) = { lo: 44, hi: 77 }, and 44 > 0 while
    // 77 < 120), so both are available to stub.
    function primed(currentIndex: number) {
      const host = mount(<Filmstrip {...longProps({ currentIndex })} />)
      const rail = host.firstElementChild as HTMLElement
      Object.defineProperty(rail, 'scrollWidth', {
        value: 120,
        configurable: true,
      })
      Object.defineProperty(rail, 'clientWidth', {
        value: 0,
        configurable: true,
      })
      rail.getBoundingClientRect = () => rect(0, 400)
      const [leading, trailing] = spacers(host)
      return { host, rail, leading, trailing }
    }

    it('does not rebuild the window while both spacers sit well clear of the rail', () => {
      const { host, rail, leading, trailing } = primed(60)
      // Both spacers sit far outside the guard (rail: 0..400, guard 100):
      // no gap is near the strip.
      leading.getBoundingClientRect = () => rect(-500, 50)
      trailing.getBoundingClientRect = () => rect(600, 50)

      const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
      rail.scrollLeft = 90
      act(() => rail.dispatchEvent(new Event('scroll')))
      const after = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
      expect(after).toEqual(before)
    })

    it('rebuilds the window once a spacer comes within the guard of the rail', () => {
      const { host, rail, leading, trailing } = primed(60)
      leading.getBoundingClientRect = () => rect(-500, 50)
      // The trailing spacer's left edge (370) is inside the guard (rail's
      // right edge 400 plus guard 100 = 500), so it overlaps the guarded box.
      trailing.getBoundingClientRect = () => rect(370, 50)

      const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
      rail.scrollLeft = 90
      act(() => rail.dispatchEvent(new Event('scroll')))
      const after = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
      expect(after).not.toEqual(before)
    })

    // The floor on the guard (200 px minimum) exists because a narrow screen's
    // proportional guard (390/4 = 97.5 px) can be too small: the guard must
    // cover the distance scrolled while the new window renders, which depends
    // on scroll speed and render time, not viewport width. A 97.5 px guard on
    // a narrow screen leaves a spacer briefly visible, whereas the floor ensures
    // enough headroom even when the proportional guard is small.
    it('rebuilds the window when a spacer sits between the proportional guard and the floor', () => {
      const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
      const rail = host.firstElementChild as HTMLElement
      Object.defineProperty(rail, 'scrollWidth', {
        value: 120,
        configurable: true,
      })
      Object.defineProperty(rail, 'clientWidth', {
        value: 0,
        configurable: true,
      })
      // Narrow rail: 390 px wide, so proportional guard is 390/4 = 97.5 px,
      // well below the 200 px floor. Rail spans 0..390.
      rail.getBoundingClientRect = () => rect(0, 390)
      const [leading, trailing] = spacers(host)

      leading.getBoundingClientRect = () => rect(-500, 50)
      // The trailing spacer's left edge (488) sits just outside the
      // proportional guard's reach (487.5) but within the floor's reach (590):
      // without the floor, 488 >= 487.5 and the spacer sits outside the guard,
      // so no re-anchor; with the floor, 488 < 590 and the re-anchor fires.
      trailing.getBoundingClientRect = () => rect(488, 50)

      const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
      rail.scrollLeft = 90
      act(() => rail.dispatchEvent(new Event('scroll')))
      const after = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
      expect(after).not.toEqual(before)
    })
  })

  // The target page is the exact boundary the mechanism produces, not an
  // arbitrary later one: the initial window (focus 0, margin 16) renders 17
  // tiles, so the last one taking focus is index 16, and `lo + at` (0 + 16)
  // extends the window to stripWindow(16, 120, 16) = { lo: 0, hi: 33 } —
  // page 33 is the new edge and a page further out (e.g. 40) would still be
  // unreached, which would make the assertion wrong about what one `Tab`
  // to the edge extends the window to.
  it('extends the window to a tile that takes focus, for a reader using the keyboard', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 0 })} />)
    const rail = host.firstElementChild as HTMLElement
    const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
    expect(before).not.toContain('page 33')

    // The last tile the window built takes focus, as it would under `Tab`.
    const tiles = [...rail.querySelectorAll('button')]
    const last = tiles[tiles.length - 1]
    act(() => {
      last.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })

    expect(
      Array.from(host.querySelectorAll('img')).map((i) => i.alt),
    ).toContain('page 33')
  })

  // The test above starts at `currentIndex: 0`, where `lo` is 0 and `lo +
  // at` is indistinguishable from `at` alone — it cannot tell a fix from a
  // regression that drops the `lo` offset entirely. This one starts at
  // `currentIndex: 60`, where `lo` is 44: stripWindow(60, 120, 16) renders
  // indices 44..76 (pages 45..77), 33 tiles wide, so the last one takes
  // focus at slice-relative `at` = 32. The correct absolute index is
  // `lo + at` = 76, extending the window to stripWindow(76, 120, 16) —
  // indices 60..92, i.e. pages 61..93, with page 93 the new far edge. Drop
  // the `lo` offset and the window would extend around raw `at` = 32
  // instead — stripWindow(32, 120, 16), indices 16..48, i.e. pages 17..49 —
  // so page 93 present and page 17 absent is what tells the two apart.
  it('extends the window to a tile that takes focus, from a window that has already moved off the start', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 60 })} />)
    const rail = host.firstElementChild as HTMLElement
    const before = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
    expect(before).not.toContain('page 93')
    expect(before).not.toContain('page 17')

    // The last tile the window built takes focus, as it would under `Tab`.
    const tiles = [...rail.querySelectorAll('button')]
    const last = tiles[tiles.length - 1]
    act(() => {
      last.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })

    const after = Array.from(host.querySelectorAll('img')).map((i) => i.alt)
    expect(after).toContain('page 93')
    expect(after).not.toContain('page 17')
  })

  // C1 (whole-branch review): a page turn while the reader had flicked the
  // rail away from the current tile used to leave the strip blank. The
  // centring effect's own, synchronous `recenter()` call ran against the
  // *stale* window — still built from the scroll position, since
  // `setScrolledTo(undefined)` in that same effect only schedules the reset
  // and does not apply it before `recenter` runs — found no ref to scroll
  // to (the new current tile was not in that stale window), and was never
  // retried once the reset actually rebuilt the window around the new
  // current tile, because the effect's own dependencies had not changed. A
  // real repro at 390×844 showed the rendered window correctly follow the
  // new `currentIndex` while `scrollLeft` stayed exactly where the flick
  // left it and zero tiles were visible.
  it('recentres on the new current tile after a page turn that lands outside the window the reader had flicked to', () => {
    const host = mount(<Filmstrip {...longProps({ currentIndex: 10 })} />)
    const rail = host.firstElementChild as HTMLElement

    // The flick: scroll far enough that the window moves to surround tile
    // 100, well clear of both `currentIndex: 10` above and `currentIndex:
    // 15` below — same stubbing as "extends the window to where the reader
    // scrolls" above. fraction = 1500 / (2000 - 200) = 0.8333, and
    // `focusAtFraction` over 120 one-unit tiles puts that at index 100;
    // `stripWindow(100, 120, 16)` is `{ lo: 84, hi: 117 }`, i.e. pages
    // 85..117.
    Object.defineProperty(rail, 'scrollWidth', {
      value: 2000,
      configurable: true,
    })
    Object.defineProperty(rail, 'clientWidth', {
      value: 200,
      configurable: true,
    })
    rail.getBoundingClientRect = () => rect(0, 400)
    rail.scrollLeft = 1500
    act(() => {
      rail.dispatchEvent(new Event('scroll'))
    })
    const afterFlick = Array.from(host.querySelectorAll('img')).map(
      (i) => i.alt,
    )
    expect(afterFlick).toContain('page 100')
    expect(afterFlick).not.toContain('page 16')

    // The page turn: `currentIndex` moves to 15, nowhere near the window
    // the flick just built.
    act(() => {
      rerender(<Filmstrip {...longProps({ currentIndex: 15 })} />)
    })

    const current = host.querySelector('[aria-current="true"]')
    expect(current?.querySelector('img')?.getAttribute('alt')).toBe('page 16')
    expect(scrolledTo.at(-1)).toBe(current)
  })
})

describe('the strip as one stop (spec 2026-09-26 §3.2)', () => {
  const stops = (host: HTMLElement) => buttons(host).map((b) => b.tabIndex)

  it('makes the current tile the only stop', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    expect(stops(host)).toEqual([-1, 0, -1, -1])
  })

  it('makes the anchor tile the stop where nothing is current', () => {
    const host = mount(
      <Filmstrip {...props({ currentIndex: -1, anchor: 2 })} />,
    )
    expect(stops(host)).toEqual([-1, -1, 0, -1])
  })

  /** Picking a tile is what a correction is for, and nothing is current then. */
  it('keeps every tile a stop while correcting', () => {
    const host = mount(
      <Filmstrip
        {...props({ currentIndex: -1, correcting: { to: 'raw-mono' } })}
      />,
    )
    expect(stops(host)).toEqual([0, 0, 0, 0])
  })

  it('moves focus to the new current tile when a turn lands with focus on the strip', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    act(() => buttons(host)[1].focus())
    rerender(<Filmstrip {...props({ currentIndex: 2 })} />)
    expect(document.activeElement).toBe(buttons(host)[2])
  })

  it('leaves focus alone when it was not on the strip', () => {
    const host = mount(<Filmstrip {...props({ currentIndex: 1 })} />)
    const outside = document.createElement('button')
    document.body.append(outside)
    act(() => outside.focus())
    rerender(<Filmstrip {...props({ currentIndex: 2 })} />)
    expect(document.activeElement).toBe(outside)
    expect(buttons(host)[2]).not.toBe(document.activeElement)
    outside.remove()
  })

  // The follow is a ref plus a *second*, dependency-free effect rather than a
  // `focus()` inside the centring effect precisely because the new current
  // tile may not be rendered on the render a turn lands on: the window is
  // still built around `scrolledTo ?? primaryFocus`, and `scrolledTo` still
  // holds the *old* focused tile's index on that render — the rail's
  // `focusin` listener is what put it there, and the centring effect only
  // schedules clearing it, it does not rebuild the window inline. The window
  // is rebuilt around the new current tile only on a later render, once that
  // `setScrolledTo(undefined)` lands. 100 single-slot groups, not the shared
  // 4-group fixture above (whose own comment says why it must not grow past
  // one window): tile 80 has to sit outside the window `currentIndex: 10`
  // renders, so a turn to it is exactly the case the retry exists for.
  it('retries the follow once the rebuilt window renders the new current tile', () => {
    const longSlots: Position[] = Array.from({ length: 100 }, (_, i) => ({
      page: i,
    }))
    const longGroups: Group[] = longSlots.map((_, i) => ({ slots: [i] }))
    const longProps = (overrides: Partial<FilmstripProps> = {}) =>
      props({ slots: longSlots, groups: longGroups, ...overrides })
    const tileByAlt = (host: HTMLElement, alt: string) =>
      buttons(host).find(
        (b) => b.querySelector('img')?.getAttribute('alt') === alt,
      )

    const host = mount(<Filmstrip {...longProps({ currentIndex: 10 })} />)
    act(() => tileByAlt(host, 'page 11')?.focus())
    // Sanity check on the fixture itself: the window `currentIndex: 10`
    // builds does not reach tile 80 — otherwise this test would not be
    // exercising the retry at all.
    expect(tileByAlt(host, 'page 81')).toBeUndefined()

    rerender(<Filmstrip {...longProps({ currentIndex: 80 })} />)

    const target = tileByAlt(host, 'page 81')
    expect(document.activeElement).toBe(target)
    expect(target?.getAttribute('aria-current')).toBe('true')
    expect(target?.tabIndex).toBe(0)
  })
})

// O138. jsdom has no layout, so `scrollLeft` here is a number the handler
// wrote and not a place the rail went: which way the rail moves on screen is
// `Filmstrip.browser.test.tsx`'s to say. What is held here is that the rail
// listens, applies `stripWheel`'s answer the way the rail runs, cancels what
// it takes and nothing else, and leaves one listener behind a change of
// direction rather than two.
describe('the wheel on the rail', () => {
  const railOf = (host: HTMLElement) => host.firstElementChild as HTMLElement
  /** `false` once a listener has cancelled the event: `dispatchEvent`'s own answer. */
  const send = (rail: HTMLElement, init: WheelEventInit) =>
    rail.dispatchEvent(
      new WheelEvent('wheel', { bubbles: true, cancelable: true, ...init }),
    )

  it('moves the rail right for a notch down where the pages run left to right', () => {
    const rail = railOf(mount(<Filmstrip {...props({ reading: 'ltr' })} />))
    expect(send(rail, { deltaY: 100 })).toBe(false)
    expect(rail.scrollLeft).toBe(100)
  })

  it('moves it left where they run right to left', () => {
    const rail = railOf(mount(<Filmstrip {...props({ reading: 'rtl' })} />))
    expect(send(rail, { deltaY: 100 })).toBe(false)
    expect(rail.scrollLeft).toBe(-100)
  })

  it('leaves a wheel it does not take uncancelled, and the rail where it was', () => {
    const rail = railOf(mount(<Filmstrip {...props()} />))
    expect(send(rail, { deltaY: 100, ctrlKey: true })).toBe(true)
    expect(rail.scrollLeft).toBe(0)
  })

  // Two listeners would move the rail right and then left, and leave it at 0.
  it('listens once after the direction is flipped', () => {
    const host = mount(<Filmstrip {...props({ reading: 'ltr' })} />)
    rerender(<Filmstrip {...props({ reading: 'rtl' })} />)
    const rail = railOf(host)
    send(rail, { deltaY: 100 })
    expect(rail.scrollLeft).toBe(-100)
  })

  // The listener is on the rail itself: every case above dispatches at the
  // rail, which a listener on any ancestor would also see. The target here is
  // the rail's parent, the nearest element outside it.
  it('leaves a wheel outside the rail alone, and takes the same wheel on it', () => {
    const rail = railOf(mount(<Filmstrip {...props()} />))
    const outside = rail.parentElement as HTMLElement
    expect(send(outside, { deltaY: 100 })).toBe(true)
    expect(rail.scrollLeft).toBe(0)
    // THE CONTROL: the same wheel, at the rail, is taken.
    expect(send(rail, { deltaY: 100 })).toBe(false)
    expect(rail.scrollLeft).toBe(100)
  })

  // The cases above each send a wheel at the rail itself, from a rail at
  // rest. A wheel in use is over a tile, the event bubbles up to the rail, and
  // the rail is wherever the last wheel left it. So: from a rail that has
  // already moved, at a tile, twice. Held here: the write adds to `scrollLeft`
  // and does not assign it, a wheel over a tile is the rail's, and the
  // listener outlives its first event.
  it('moves a rail that has already moved, by a wheel on a tile, each time', () => {
    const rail = railOf(mount(<Filmstrip {...props({ reading: 'ltr' })} />))
    rail.scrollLeft = 250
    const tile = rail.querySelector('button') as HTMLElement
    expect(send(tile, { deltaY: 100 })).toBe(false)
    expect(rail.scrollLeft).toBe(350)
    expect(send(tile, { deltaY: 100 })).toBe(false)
    expect(rail.scrollLeft).toBe(450)
  })

  // The bar is hidden by the stylesheet and the wheel is handled here, which
  // makes `overflow-x: hidden` look like a tidy-up that shows nothing on
  // screen. It would also take away the touch flick, the sideways swipe and
  // Shift with the wheel, all of which are the browser's scrolling of this box.
  it('is still a box the browser scrolls', () => {
    const rail = railOf(mount(<Filmstrip {...props()} />))
    expect(rail.style.overflowX).toBe('auto')
  })
})

// O216: what `Chrome` marks on the progress line. jsdom has no layout, so the
// rail's three numbers are given by hand, as the wheel's cases above give
// `scrollLeft`. What is held here is when the strip tells and what it tells
// from those numbers; that a browser's own numbers put the mark where the
// strip stands is `Chrome.browser.test.tsx`'s to say.
//
// The shared four-tile fixture, on purpose: it fits inside one window, so no
// spacer is drawn and a scroll rebuilds nothing. A scroll that set state
// would be followed by a render, and the telling after a render would hide
// whether the scroll's own frame told anything.
describe('telling what the rail shows', () => {
  const railOf = (host: HTMLElement) => host.firstElementChild as HTMLElement

  // Frames are kept and not run, so a test says when one lands and can count
  // how many were asked for.
  let pending: Map<number, FrameRequestCallback>
  let nextFrame: number
  beforeEach(() => {
    pending = new Map()
    nextFrame = 0
    vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
      nextFrame += 1
      pending.set(nextFrame, cb)
      return nextFrame
    })
    vi.stubGlobal('cancelAnimationFrame', (id: number) => {
      pending.delete(id)
    })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })
  const runFrames = () =>
    act(() => {
      const callbacks = [...pending.values()]
      pending.clear()
      for (const cb of callbacks) cb(0)
    })

  /** The numbers a layout would give the rail: `scrollWidth` long, showing `clientWidth` of it from `scrollLeft`. */
  function lay(
    rail: HTMLElement,
    at: { scrollLeft: number; clientWidth: number; scrollWidth: number },
  ) {
    Object.defineProperty(rail, 'scrollWidth', {
      value: at.scrollWidth,
      configurable: true,
    })
    Object.defineProperty(rail, 'clientWidth', {
      value: at.clientWidth,
      configurable: true,
    })
    rail.scrollLeft = at.scrollLeft
  }

  /** A reversed row's `scrollLeft` runs negative from the start of the volume. */
  const along = (reading: 'ltr' | 'rtl', px: number) =>
    reading === 'rtl' ? -px : px

  /** A rail four times as long as it is wide, scrolled `px` from the start of the volume, and the frame its scroll takes. */
  function scrollTo(rail: HTMLElement, reading: 'ltr' | 'rtl', px: number) {
    lay(rail, {
      scrollLeft: along(reading, px),
      clientWidth: 400,
      scrollWidth: 1600,
    })
    rail.dispatchEvent(new Event('scroll'))
    runFrames()
  }

  it('tells the whole rail on mount, where nothing has been laid out to overflow', () => {
    const onShown = vi.fn()
    mount(<Filmstrip {...props({ onShown })} />)
    expect(onShown).toHaveBeenCalledExactlyOnceWith({ start: 0, end: 1 })
  })

  for (const reading of ['ltr', 'rtl'] as const) {
    it(`tells the stretch the rail stands at after a scroll, in the frame the scroll takes, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      onShown.mockClear()

      lay(rail, {
        scrollLeft: along(reading, 600),
        clientWidth: 400,
        scrollWidth: 1600,
      })
      rail.dispatchEvent(new Event('scroll'))
      // Not in the event, which comes far more often than a frame does: one
      // frame is asked for, and nothing is told until it lands.
      expect(onShown).not.toHaveBeenCalled()
      expect(pending.size).toBe(1)
      runFrames()

      expect(onShown).toHaveBeenCalledExactlyOnceWith({
        start: 0.375,
        end: 0.625,
      })
      expect(onShown).toHaveBeenCalledWith(shownShare(rail))
    })

    it(`does not tell again for a scroll that left the rail where it stood, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      scrollTo(rail, reading, 600)
      onShown.mockClear()

      scrollTo(rail, reading, 600)

      expect(onShown).not.toHaveBeenCalled()
      // THE CONTROL: the same listener does tell a scroll that moved the rail.
      scrollTo(rail, reading, 1200)
      expect(onShown).toHaveBeenCalledExactlyOnceWith({ start: 0.75, end: 1 })
    })

    // A volume a little longer than the strip is wide: the rail's travel is a
    // small part of its length, so one pixel is next to nothing against the
    // length and a real part of the travel, which is what the mark is placed
    // by. Every pixel is told.
    it(`tells a movement however small it is against the rail's length, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      const stand = (px: number) => {
        lay(rail, {
          scrollLeft: along(reading, px),
          clientWidth: 2400,
          scrollWidth: 2467,
        })
        rail.dispatchEvent(new Event('scroll'))
        runFrames()
      }
      stand(66)
      onShown.mockClear()

      stand(67)

      expect(onShown).toHaveBeenCalledExactlyOnceWith({
        start: 67 / 2467,
        end: 1,
      })
    })

    it(`does not tell again for a render that moved nothing, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      scrollTo(rail, reading, 600)
      onShown.mockClear()

      rerender(<Filmstrip {...props({ reading, onShown })} />)

      expect(onShown).not.toHaveBeenCalled()
    })

    // A render can change the rail's length under a `scrollLeft` that stays
    // where it was. Here the length is changed by hand and the render is of
    // the same props: no scroll is dispatched, no thumbnail loads and the
    // observer is not called, so the render is all that can have told.
    it(`tells after a render that changed what the rail shows, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      scrollTo(rail, reading, 600)
      onShown.mockClear()

      lay(rail, {
        scrollLeft: along(reading, 600),
        clientWidth: 400,
        scrollWidth: 3200,
      })
      rerender(<Filmstrip {...props({ reading, onShown })} />)

      expect(onShown).toHaveBeenCalledExactlyOnceWith({
        start: 0.1875,
        end: 0.3125,
      })
      // And no frame was asked for on the way.
      expect(pending.size).toBe(0)
    })

    // A thumbnail that loads gives its tile its real width: the rail grows
    // under a `scrollLeft` that does not move, with no scroll and no render.
    // `load` does not bubble, so it is dispatched at the image and has to be
    // heard on the way down.
    it(`tells after a thumbnail's load changed the rail's length, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      scrollTo(rail, reading, 600)
      onShown.mockClear()

      lay(rail, {
        scrollLeft: along(reading, 600),
        clientWidth: 400,
        scrollWidth: 3200,
      })
      rail.querySelector('img')?.dispatchEvent(new Event('load'))
      runFrames()

      expect(onShown).toHaveBeenCalledExactlyOnceWith({
        start: 0.1875,
        end: 0.3125,
      })
    })

    // The rail's own box, as when the window is resized. The stub's observer
    // is called by hand: jsdom has no layout to call it.
    it(`tells when the rail's own width changes, ${reading}`, () => {
      const onShown = vi.fn()
      const rail = railOf(mount(<Filmstrip {...props({ reading, onShown })} />))
      scrollTo(rail, reading, 600)
      onShown.mockClear()
      expect(StubResizeObserver.made).toHaveLength(1)
      const [observer] = StubResizeObserver.made
      expect(observer.observed).toEqual([rail])

      lay(rail, {
        scrollLeft: along(reading, 600),
        clientWidth: 800,
        scrollWidth: 1600,
      })
      act(() => observer.changed())

      expect(onShown).toHaveBeenCalledExactlyOnceWith({
        start: 0.375,
        end: 0.875,
      })
      // And it goes on watching: a window is resized more than once.
      expect(observer.disconnected).toBe(false)
    })
  }

  // The listener that refines `--strip-unit` hears the same loads and lets go
  // once it has measured. This one must not go with it: the tiles here are
  // all loaded at the width the rail is primed with, which finishes that
  // measuring, and a thumbnail that loads afterwards is still told.
  it('goes on telling loads after the strip has finished measuring its unit', () => {
    const onShown = vi.fn()
    const rail = railOf(mount(<Filmstrip {...props({ onShown })} />))
    rail.style.setProperty('--strip-tile-border', '2px')
    rail.style.setProperty('--strip-unit', '54px')
    const removed = vi.spyOn(rail, 'removeEventListener')
    for (const tile of buttons(rail)) {
      tile.getBoundingClientRect = () => ({ width: 58 }) as DOMRect
      Object.defineProperty(tile.querySelector('img'), 'naturalWidth', {
        value: 58,
        configurable: true,
      })
    }
    scrollTo(rail, 'ltr', 600)
    rail.querySelector('img')?.dispatchEvent(new Event('load'))
    runFrames()
    // THE STATE THIS TEST NAMES: the measuring is over, and its listener gone.
    expect(removed).toHaveBeenCalledWith('load', expect.anything(), true)
    onShown.mockClear()

    lay(rail, { scrollLeft: 600, clientWidth: 400, scrollWidth: 3200 })
    rail.querySelector('img')?.dispatchEvent(new Event('load'))
    runFrames()

    expect(onShown).toHaveBeenCalledExactlyOnceWith({
      start: 0.1875,
      end: 0.3125,
    })
  })

  // A window's worth of thumbnails arrives together, and the strip reads the
  // rail once for all of them, in a frame, and not once for each.
  it('takes one frame for however many thumbnails load before it lands', () => {
    const onShown = vi.fn()
    const rail = railOf(mount(<Filmstrip {...props({ onShown })} />))
    scrollTo(rail, 'ltr', 600)
    onShown.mockClear()

    lay(rail, { scrollLeft: 600, clientWidth: 400, scrollWidth: 3200 })
    for (const img of rail.querySelectorAll('img')) {
      img.dispatchEvent(new Event('load'))
    }
    // Nothing is told until the frame, and one frame is all that was asked for.
    expect(onShown).not.toHaveBeenCalled()
    expect(pending.size).toBe(1)
    runFrames()
    expect(onShown).toHaveBeenCalledOnce()

    // The frame that landed leaves the next load free to ask for another.
    lay(rail, { scrollLeft: 600, clientWidth: 400, scrollWidth: 6400 })
    rail.querySelector('img')?.dispatchEvent(new Event('load'))
    expect(pending.size).toBe(1)
    runFrames()
    expect(onShown).toHaveBeenCalledTimes(2)
  })

  it('tells nothing for a load or a change of the box that moved nothing', () => {
    const onShown = vi.fn()
    const rail = railOf(mount(<Filmstrip {...props({ onShown })} />))
    scrollTo(rail, 'ltr', 600)
    onShown.mockClear()

    rail.querySelector('img')?.dispatchEvent(new Event('load'))
    runFrames()
    act(() => StubResizeObserver.made[0].changed())

    expect(onShown).not.toHaveBeenCalled()
  })

  // `Chrome` hands a new function on every render, and the strip tells
  // through the one it was handed last.
  it('tells the `onShown` of the latest render', () => {
    const first = vi.fn()
    const second = vi.fn()
    const rail = railOf(mount(<Filmstrip {...props({ onShown: first })} />))
    rerender(<Filmstrip {...props({ onShown: second })} />)
    first.mockClear()

    scrollTo(rail, 'ltr', 600)

    expect(second).toHaveBeenCalledExactlyOnceWith({ start: 0.375, end: 0.625 })
    expect(first).not.toHaveBeenCalled()
  })

  it('stops watching the rail and listening on it when the strip is taken away', () => {
    const rail = railOf(mount(<Filmstrip {...props({ onShown: vi.fn() })} />))
    const [observer] = StubResizeObserver.made
    const img = rail.querySelector('img') as HTMLImageElement
    // THE CONTROL: while the strip is there a load asks for a frame. The
    // frame is run, so none is waiting when the strip goes: a listener left
    // behind would be free to ask for another, and would not be hidden
    // behind the one it was already waiting on.
    img.dispatchEvent(new Event('load'))
    expect(pending.size).toBe(1)
    runFrames()
    expect(observer.disconnected).toBe(false)

    rerender(null)

    expect(observer.disconnected).toBe(true)
    // `img` is still the old rail's child, so a listener left on that rail
    // would hear this.
    expect(rail.contains(img)).toBe(true)
    img.dispatchEvent(new Event('load'))
    expect(pending.size).toBe(0)
  })

  it('cancels the frame a load was waiting on when the strip is taken away', () => {
    const rail = railOf(mount(<Filmstrip {...props({ onShown: vi.fn() })} />))
    rail.querySelector('img')?.dispatchEvent(new Event('load'))
    expect(pending.size).toBe(1)

    rerender(null)

    expect(pending.size).toBe(0)
  })
})

// O216: a mouse drags the rail. jsdom has no layout, so a `scrollLeft` here is
// a number the listener wrote and a `clientX` one the test chose: that the
// rail's content goes where the pointer went, and that a pointer outside the
// strip is still followed, are `Filmstrip.browser.test.tsx`'s to say, with a
// real mouse. What is held here is what the listeners do with each event.
//
// The events are made, and sent at a tile, since that is where a real press
// lands and the rail hears it on the way up. Once the pointer is captured a
// real one is sent to the rail itself, so the cases send those there. A
// release is listened for on the window, and a case sends it where it says
// the pointer was let go: on the rail, or on something that is not the rail's.
describe('a mouse dragging the rail', () => {
  const railOf = (host: HTMLElement) => host.firstElementChild as HTMLElement

  // jsdom implements no part of the Pointer Capture API, and the rail captures
  // the pointer when a press becomes a drag. Stood in for here and not guarded
  // in the component, for the reason `Labeller.test.tsx` gives. These keep
  // what they were asked, so a case can say which element holds the pointer.
  //
  // The swallowing of a click ends on a timer. Faked, so a case says when the
  // timer lands and none waits on a clock.
  let capturing: Map<number, Element>
  beforeEach(() => {
    capturing = new Map()
    Element.prototype.setPointerCapture = function (this: Element, id) {
      capturing.set(id, this)
    }
    Element.prototype.hasPointerCapture = function (this: Element, id) {
      return capturing.get(id) === this
    }
    Element.prototype.releasePointerCapture = function (this: Element, id) {
      if (capturing.get(id) === this) capturing.delete(id)
    }
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  })
  afterEach(() => {
    Reflect.deleteProperty(Element.prototype, 'setPointerCapture')
    Reflect.deleteProperty(Element.prototype, 'hasPointerCapture')
    Reflect.deleteProperty(Element.prototype, 'releasePointerCapture')
    vi.useRealTimers()
  })
  /** The timers that are due now, and no later one. */
  const timersLand = () => vi.advanceTimersByTime(0)

  /** What a case says of a pointer event: its fields, and when it was sent where the case is about time. */
  type Sent = PointerEventInit & { timeStamp?: number }

  /**
   * A pointer event at `x` along the rail, as a mouse with its primary button
   * held sends it unless `init` says otherwise. `false` once a listener has
   * cancelled it.
   *
   * jsdom stamps an event with the clock's time as it is made and takes no
   * other, so the stamp is written over. A case about the pointer's speed
   * gives each of its events a `timeStamp`. Every other event is sent at one
   * time, 0: a pointer that covers its ground in no time has no speed to be
   * read, so no release in a case that is not about speed carries the rail
   * on, however long the case took to run.
   */
  const pointer = (
    target: Element,
    type: string,
    x: number,
    { timeStamp = 0, ...init }: Sent = {},
  ) => {
    const event = new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      pointerId: 1,
      pointerType: 'mouse',
      button: 0,
      buttons: 1,
      clientX: x,
      ...init,
    })
    Object.defineProperty(event, 'timeStamp', { value: timeStamp })
    return target.dispatchEvent(event)
  }
  const press = (target: Element, x: number, init?: Sent) =>
    pointer(target, 'pointerdown', x, init)
  /** A move changes no button, which is what `button: -1` says. */
  const moveTo = (target: Element, x: number, init?: Sent) =>
    pointer(target, 'pointermove', x, { button: -1, ...init })
  const release = (target: Element, x: number, init?: Sent) =>
    pointer(target, 'pointerup', x, { buttons: 0, ...init })
  const cancel = (target: Element, init?: Sent) =>
    pointer(target, 'pointercancel', 0, { buttons: 0, ...init })
  /** `false` once a listener has cancelled the click: `dispatchEvent`'s own answer. */
  const click = (target: Element) =>
    target.dispatchEvent(
      new MouseEvent('click', { bubbles: true, cancelable: true }),
    )
  /** jsdom has no `DragEvent`. The rail's listener reads nothing off the event. */
  const dragstart = (target: Element) =>
    target.dispatchEvent(
      new Event('dragstart', { bubbles: true, cancelable: true }),
    )

  /**
   * A pointer leaving the rail's box. It does not bubble, and is sent at the
   * rail, which is the element it is about.
   */
  const leave = (rail: Element, init?: PointerEventInit) =>
    pointer(rail, 'pointerleave', 0, { bubbles: false, button: -1, ...init })

  /**
   * The strip, with its rail standing 300 px from the start of the volume, and
   * the tile every case presses: the third, which is slot 2 and not the
   * current one.
   */
  function strip(overrides: Partial<FilmstripProps> = {}) {
    const onNavigate = vi.fn()
    const host = mount(<Filmstrip {...props({ onNavigate, ...overrides })} />)
    const rail = railOf(host)
    // A reversed row's `scrollLeft` runs negative from the start of the volume.
    const stood = overrides.reading === 'rtl' ? -300 : 300
    rail.scrollLeft = stood
    // What hears a click the rail let through: React's own listener is on the
    // host too.
    const above = vi.fn()
    host.addEventListener('click', above)
    return { host, rail, tile: buttons(host)[2], stood, onNavigate, above }
  }

  /**
   * Something outside the strip: a place for a release to land, and something
   * that can hold focus, as a button of the bars does.
   */
  function elsewhere(): HTMLButtonElement {
    const button = document.createElement('button')
    document.body.append(button)
    onTestFinished(() => button.remove())
    return button
  }

  /**
   * What a mouse does between the press and the first move: the browser moves
   * focus to the tile that was pressed. jsdom makes no such move for a made
   * event. In an `act`, since the rail's `focusin` listener sets state.
   */
  const mouseFocuses = (tile: HTMLElement) => act(() => tile.focus())

  /**
   * The strip of a volume long enough that the rail builds a window of it and
   * not all of it: 120 single pages, the reader on the sixty-first, and focus
   * on that page's tile, where a reader who came to the strip by `Tab` has it.
   */
  function longStrip(reading: 'ltr' | 'rtl') {
    const pages: Position[] = Array.from({ length: 120 }, (_, page) => ({
      page,
    }))
    const long = props({
      reading,
      slots: pages,
      groups: pages.map((_, slot) => ({ slots: [slot] })),
      currentIndex: 60,
    })
    const host = mount(<Filmstrip {...long} />)
    const rail = railOf(host)
    const tileOf = (group: number) => {
      const tile = rail.querySelector<HTMLElement>(`[data-group="${group}"]`)
      if (!tile) throw new Error(`tile ${group} is not built`)
      return tile
    }
    act(() => tileOf(60).focus())
    /** The same strip with the reader on another page: a page turn. */
    const turnTo = (currentIndex: number) =>
      rerender(<Filmstrip {...long} currentIndex={currentIndex} />)
    return { rail, tileOf, turnTo }
  }
  /** The first and the last of the tiles the rail has built, as `groups` indices. */
  const windowOf = (rail: HTMLElement) => {
    const built = [...rail.querySelectorAll('button')].map((tile) =>
      Number(tile.dataset.group),
    )
    return [built[0], built[built.length - 1]]
  }

  // Both reading directions, every case: a reversed row's `scrollLeft` starts
  // from the other end of its range, and each case here writes it or holds
  // that it was not written.
  for (const reading of ['ltr', 'rtl'] as const) {
    // In figures, and not in `STRIP_DRAG_SLOP`'s name: 5 px is what was chosen,
    // and a case written in the constant's own terms would pass whatever the
    // constant was changed to.
    it(`becomes a drag at 5 px of travel and follows the pointer from the press, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 504)
      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)

      moveTo(tile, 505)
      expect(rail.scrollLeft).toBe(stood - 5)
      expect(rail.style.cursor).toBe('grabbing')
      expect(capturing.get(1)).toBe(rail)

      moveTo(rail, 620)
      expect(rail.scrollLeft).toBe(stood - 120)
      // Back to within the slop of the press: it is still a drag.
      moveTo(rail, 502)
      expect(rail.scrollLeft).toBe(stood - 2)
      // And on past the press, the other way.
      moveTo(rail, 430)
      expect(rail.scrollLeft).toBe(stood + 70)

      release(rail, 430)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)
      // The rail stays where it was let go, and a move after that moves nothing.
      expect(rail.scrollLeft).toBe(stood + 70)
      moveTo(rail, 300, { buttons: 0 })
      expect(rail.scrollLeft).toBe(stood + 70)
    })

    it(`becomes a drag at 5 px the other way too, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 496)
      expect(rail.scrollLeft).toBe(stood)

      moveTo(tile, 495)
      expect(rail.scrollLeft).toBe(stood + 5)
      expect(rail.style.cursor).toBe('grabbing')
    })

    // The 5 px are along the rail. A hand that presses a tile and slips down
    // it has travelled a long way and is still clicking that tile.
    it(`measures the travel along the rail and not across it, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })

      press(tile, 500, { clientY: 300 })
      moveTo(tile, 503, { clientY: 420 })
      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)
      release(tile, 503, { clientY: 420 })
      expect(click(tile)).toBe(true)
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)

      // THE CONTROL: the same slip with 5 px along the rail is a drag.
      press(tile, 500, { clientY: 300 })
      moveTo(tile, 505, { clientY: 420 })
      expect(rail.scrollLeft).toBe(stood - 5)
    })

    // A press between two tiles, or in the rail's own padding, lands on the
    // rail and on no tile. It drags the rail as a press on a tile does.
    it(`drags from a press on the rail itself, where no tile is, ${reading}`, () => {
      const { rail, stood, above } = strip({ reading })

      press(rail, 500)
      moveTo(rail, 560)
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(rail.style.cursor).toBe('grabbing')
      expect(capturing.get(1)).toBe(rail)

      release(rail, 560)
      expect(click(rail)).toBe(false)
      expect(above).not.toHaveBeenCalled()
    })

    it(`swallows the click after a drag where it lands on the tile, ${reading}`, () => {
      const { rail, tile, onNavigate, above } = strip({ reading })
      // The tile's own listener: the click is stopped on its way down, so it
      // does not reach the tile at all.
      const reached = vi.fn()
      tile.addEventListener('click', reached)

      press(tile, 500)
      moveTo(tile, 560)
      release(rail, 560)

      expect(click(tile)).toBe(false)
      expect(onNavigate).not.toHaveBeenCalled()
      expect(reached).not.toHaveBeenCalled()
      expect(above).not.toHaveBeenCalled()

      // THE CONTROL, and the next click is the reader's own: the same click
      // again turns the page.
      expect(click(tile)).toBe(true)
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
      expect(reached).toHaveBeenCalledOnce()
      expect(above).toHaveBeenCalledOnce()
    })

    it(`takes a press that travels less than 5 px as the click it was, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 504)
      moveTo(tile, 497)
      release(tile, 497)

      expect(rail.scrollLeft).toBe(stood)
      expect(capturing.size).toBe(0)
      expect(click(tile)).toBe(true)
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
    })

    // A captured pointer's click is the capturing element's in the browser the
    // other file drives, so this is the one a real drag is followed by there.
    it(`swallows the click after a drag where it lands on the rail itself, ${reading}`, () => {
      const { rail, tile, above } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 560)
      release(rail, 560)

      expect(click(rail)).toBe(false)
      expect(above).not.toHaveBeenCalled()
      // THE CONTROL: the same click again is let through.
      expect(click(rail)).toBe(true)
      expect(above).toHaveBeenCalledOnce()
    })

    // Not every end of a drag is followed by a click: a `pointercancel` is not.
    // The swallowing has to end by itself then, or the reader's next click
    // would be the one it takes.
    it(`swallows nothing once the release is past, where no click followed it, ${reading}`, () => {
      const { rail, tile, onNavigate } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 560)
      release(rail, 560)
      timersLand()

      expect(click(tile)).toBe(true)
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
    })

    for (const [whose, down, held] of [
      ['a touch', { pointerType: 'touch' }, { pointerType: 'touch' }],
      ['a pen', { pointerType: 'pen' }, { pointerType: 'pen' }],
      ["a mouse's middle button", { button: 1, buttons: 4 }, { buttons: 4 }],
      ["a mouse's secondary button", { button: 2, buttons: 2 }, { buttons: 2 }],
    ] as const) {
      it(`leaves ${whose} alone, ${reading}`, () => {
        const { rail, tile, stood, onNavigate } = strip({ reading })

        press(tile, 500, down)
        moveTo(tile, 560, held)
        expect(rail.scrollLeft).toBe(stood)
        expect(rail.style.cursor).toBe('')
        expect(capturing.size).toBe(0)
        release(tile, 560, down)
        // Nothing was dragged, so the click after it is not swallowed.
        expect(click(tile)).toBe(true)
        expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)

        // THE CONTROL: the same press and move with a mouse's primary button.
        press(tile, 500)
        moveTo(tile, 560)
        expect(rail.scrollLeft).toBe(stood - 60)
      })
    }

    // A finger on the screen while the mouse is held is a second pointer, with
    // an id of its own. Its moves are not the mouse's travel and its lifting is
    // not the mouse's release, on the rail or off it.
    it(`takes another pointer's moves and release as none of the press's, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })
      const finger = { pointerId: 2, pointerType: 'touch' }

      press(tile, 500)
      moveTo(tile, 700, finger)
      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
      release(tile, 700, finger)
      release(elsewhere(), 700, finger)

      // THE CONTROL: the mouse's press still stands, and its travel is measured
      // from where it was pressed.
      moveTo(tile, 560)
      expect(rail.scrollLeft).toBe(stood - 60)

      // The same of a drag under way.
      moveTo(tile, 900, finger)
      expect(rail.scrollLeft).toBe(stood - 60)
      cancel(tile, finger)
      expect(rail.style.cursor).toBe('grabbing')
      moveTo(rail, 620)
      expect(rail.scrollLeft).toBe(stood - 120)
    })

    // A second button pressed while one is held is sent as a move and not as a
    // press. So the primary button joining a held secondary one reaches the rail
    // as moves that carry it, after a press that was the other button's.
    it(`leaves alone a press of another button that the primary button then joins, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })

      press(tile, 500, { button: 2, buttons: 2 })
      moveTo(tile, 500, { button: 0, buttons: 3 })
      moveTo(tile, 560, { buttons: 3 })

      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
    })

    // Before a drag the pointer is not captured, so it can leave the rail with
    // the button down and be let go out there, on something that is not the
    // rail's. The press ends there all the same. Left standing, it would take
    // another press, made out there and carried back in over the rail, for
    // its own moves: moves with the button down and no press on the rail
    // before them.
    it(`ends a press that is let go off the rail, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 503)
      release(elsewhere(), 503)
      moveTo(tile, 560)
      moveTo(tile, 620)
      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)

      // THE CONTROL: a press on the rail after that does drag, from where it is.
      press(tile, 620)
      moveTo(tile, 680)
      expect(rail.scrollLeft).toBe(stood - 60)
      release(rail, 680)
      click(rail)
      expect(onNavigate).not.toHaveBeenCalled()
    })

    // The release is heard before anything nearer to where it lands has had
    // it, so nothing there can keep it from the rail by stopping it.
    it(`ends a press at a release that is stopped where it lands, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })
      const stops = elsewhere()
      const stopped = vi.fn((event: Event) => event.stopPropagation())
      stops.addEventListener('pointerup', stopped)

      press(tile, 500)
      release(stops, 500)
      moveTo(tile, 560)

      // THE CONTROL: the release did land there, and was stopped there.
      expect(stopped).toHaveBeenCalledOnce()
      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
    })

    // A drag let go off the rail ends there too, as a release on the rail
    // ends it. A real drag's pointer is captured and its release comes to the
    // rail; this is the release that does not.
    it(`ends a drag that is let go off the rail, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 560)
      release(elsewhere(), 560)

      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)
      moveTo(rail, 620)
      expect(rail.scrollLeft).toBe(stood - 60)
    })

    // A hand that slips off the strip's edge and back is still pressing. The
    // pointer's leaving the rail ends nothing: not a press that is yet to be
    // a drag, and not a drag.
    it(`keeps a press, and a drag, whose pointer leaves the rail, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 503)
      leave(rail)
      moveTo(tile, 560)
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(rail.style.cursor).toBe('grabbing')

      leave(rail)
      moveTo(rail, 620)
      expect(rail.scrollLeft).toBe(stood - 120)
      expect(rail.style.cursor).toBe('grabbing')
    })

    // Where a release is heard nowhere, the next the rail hears of the press
    // is a move with the button up.
    it(`forgets a press it did not hear let go, at a move with the button up, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 560, { buttons: 0 })
      expect(rail.scrollLeft).toBe(stood)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)

      // Forgotten, and not only passed over: a move that carries the button
      // again, with no press on the rail before it, is no drag either.
      moveTo(tile, 620)
      expect(rail.scrollLeft).toBe(stood)

      // THE CONTROL: a press on the rail after that does drag, from where it is.
      press(tile, 620)
      moveTo(tile, 680)
      expect(rail.scrollLeft).toBe(stood - 60)
      release(rail, 680)
      click(rail)
      expect(onNavigate).not.toHaveBeenCalled()
    })

    // The primary button let go while another is held is sent as a move too,
    // and the release itself only when the last button comes up. The drag ends
    // at that move as it does at a release, the click after it swallowed with it.
    it(`ends a drag at a move with the primary button up, as at a release, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 560)
      moveTo(rail, 560, { button: 2, buttons: 3 })
      expect(rail.style.cursor).toBe('grabbing')
      expect(capturing.get(1)).toBe(rail)

      moveTo(rail, 600, { button: 0, buttons: 2 })
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)
      expect(click(tile)).toBe(false)
      expect(onNavigate).not.toHaveBeenCalled()

      // The drag is over: neither a later move nor the last button's release is
      // anything to it, and the next click is the reader's.
      moveTo(rail, 700, { buttons: 2 })
      release(rail, 700, { button: 2 })
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(click(tile)).toBe(true)
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
    })

    it(`ends a drag at a \`pointercancel\` as at a release, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })

      press(tile, 500)
      moveTo(tile, 560)
      expect(rail.style.cursor).toBe('grabbing')
      expect(capturing.get(1)).toBe(rail)

      cancel(rail)
      expect(rail.style.cursor).toBe('')
      expect(capturing.size).toBe(0)
      // The drag is over: a move that still carries the button moves nothing.
      moveTo(rail, 700)
      expect(rail.scrollLeft).toBe(stood - 60)

      // No click follows a cancel, and the next one is the reader's.
      timersLand()
      expect(click(tile)).toBe(true)
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
    })

    // A render between the press and the move, as the reading-time readout
    // ticking makes one. `App` builds `groups` and `slots` as it renders, so
    // a render of it can hand the strip new arrays that hold what the old
    // ones held, and each render here does. The listeners are put on the rail
    // and the window once, and the press they hold is still held after it.
    it(`carries a press on through a render of the strip, ${reading}`, () => {
      const onNavigate = vi.fn()
      /** The same strip rendered again, its `groups` and its `slots` new arrays. */
      const again = () => (
        <Filmstrip
          {...props({
            reading,
            onNavigate,
            groups: groups.map((group) => ({ ...group })),
            slots: slots.map((slot) => ({ ...slot })),
          })}
        />
      )
      const host = mount(again())
      const rail = railOf(host)
      const tile = buttons(host)[2]

      press(tile, 500)
      rerender(again())
      expect(railOf(host)).toBe(rail)
      moveTo(tile, 560)
      expect(rail.scrollLeft).toBe(-60)
      expect(rail.style.cursor).toBe('grabbing')

      // And a drag under way is carried through one too.
      rerender(again())
      moveTo(rail, 620)
      expect(rail.scrollLeft).toBe(-120)
      release(rail, 620)
      expect(click(rail)).toBe(false)
    })

    // The drag is written straight onto the rail. State for it would render the
    // strip on every move of the mouse.
    it(`renders nothing for the moves and the release of a drag, ${reading}`, () => {
      let commits = 0
      const host = mount(
        <Profiler
          id="strip"
          onRender={() => {
            commits += 1
          }}
        >
          <Filmstrip {...props({ reading })} />
        </Profiler>,
      )
      const rail = railOf(host)
      const tile = buttons(host)[2]
      const mounted = commits
      expect(mounted).toBeGreaterThan(0)

      act(() => {
        press(tile, 500)
        moveTo(tile, 560)
        moveTo(rail, 620)
        release(rail, 620)
        click(rail)
        timersLand()
      })

      // THE CONTROL: the drag happened.
      expect(rail.scrollLeft).toBe(-120)
      expect(commits).toBe(mounted)
    })

    // The rail leaves the document with the strip, and `tile` is still its
    // child, so a listener left on that rail would hear a move or a
    // `dragstart` sent at it. The release and the cancel are listened for on
    // the window, which is still there: they are sent at the document's body,
    // and a listener left on the window would hear them. A drag is under way
    // when the strip goes: that is the state in which the listeners for a
    // move, a release and a cancel each have something to do.
    it(`leaves nothing listening on the rail or on the window when the strip is taken away, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })
      press(tile, 500)
      moveTo(tile, 560)
      // THE CONTROL: they were listening.
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(rail.style.cursor).toBe('grabbing')

      rerender(null)
      expect(rail.contains(tile)).toBe(true)

      moveTo(tile, 700)
      expect(rail.scrollLeft).toBe(stood - 60)
      // Nothing puts the cursor back, since nothing heard the drag end.
      release(document.body, 700)
      expect(rail.style.cursor).toBe('grabbing')
      cancel(document.body)
      expect(rail.style.cursor).toBe('grabbing')
      expect(dragstart(tile)).toBe(true)
    })

    // The same, for the listener that swallows a click: the strip goes between
    // a drag's release and the click after it.
    it(`leaves nothing swallowing a click when the strip is taken away, ${reading}`, () => {
      const { rail, tile } = strip({ reading })
      press(tile, 500)
      moveTo(tile, 560)
      release(rail, 560)

      rerender(null)
      expect(rail.contains(tile)).toBe(true)

      expect(click(tile)).toBe(true)
    })

    // A mouse focuses the tile it presses, and Space and Enter press a focused
    // tile, which turns the page to it. After a click that is the page the
    // reader chose. After a drag it would be a page they only took hold of.
    it(`hands focus back, when a press becomes a drag, to what had it before the press, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })
      const before = elsewhere()
      before.focus()
      const handed = vi.spyOn(before, 'focus')

      press(tile, 500)
      mouseFocuses(tile)
      moveTo(tile, 504)
      // Still a click in the making, and the tile's to keep.
      expect(document.activeElement).toBe(tile)

      moveTo(tile, 505)
      expect(document.activeElement).toBe(before)
      // Without scrolling to it: what is handed focus may be out of sight.
      expect(handed).toHaveBeenCalledExactlyOnceWith({ preventScroll: true })
      // THE CONTROL: that move was the one that made the press a drag.
      expect(rail.scrollLeft).toBe(stood - 5)

      moveTo(rail, 620)
      release(rail, 620)
      expect(document.activeElement).toBe(before)
    })

    // Handed back when the press becomes a drag, and at no later move of it.
    // Focus that comes to a tile of the rail while the drag is under way is
    // not the drag's to take.
    it(`leaves focus alone for the rest of a drag, ${reading}`, () => {
      const { host, rail, tile, stood } = strip({ reading })
      const before = elsewhere()
      before.focus()
      const current = buttons(host)[1]

      press(tile, 500)
      mouseFocuses(tile)
      moveTo(tile, 560)
      expect(document.activeElement).toBe(before)

      act(() => current.focus())
      moveTo(rail, 620)

      // THE CONTROL: the drag went on.
      expect(rail.scrollLeft).toBe(stood - 120)
      expect(document.activeElement).toBe(current)
    })

    // The press landed on the tile that had focus already. Nothing has moved,
    // so nothing is handed anywhere: the tile is neither focused again nor
    // left without focus.
    it(`leaves focus on a tile that had it before it was pressed, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })
      act(() => tile.focus())
      const focused = vi.spyOn(tile, 'focus')
      const blurred = vi.spyOn(tile, 'blur')

      press(tile, 500)
      moveTo(tile, 560)

      // THE CONTROL: the press did become a drag.
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(document.activeElement).toBe(tile)
      expect(focused).not.toHaveBeenCalled()
      expect(blurred).not.toHaveBeenCalled()
    })

    // Nothing had focus before the press: the document's body is what stands
    // for that, and it takes none. The tile is not left holding it.
    it(`leaves the tile without focus where nothing had it before the press, ${reading}`, () => {
      const { tile } = strip({ reading })
      expect(document.activeElement).toBe(document.body)

      press(tile, 500)
      mouseFocuses(tile)
      expect(document.activeElement).toBe(tile)
      moveTo(tile, 560)

      expect(document.activeElement).toBe(document.body)
    })

    // The rail's own stop had focus, as it has for a reader who came to the
    // strip by `Tab`. It gets it back, in an `act` since a tile taking focus
    // moves the strip's window to it.
    it(`hands focus back to another tile of the rail that had it, ${reading}`, () => {
      const { host, tile } = strip({ reading })
      const current = buttons(host)[1]
      act(() => current.focus())

      press(tile, 500)
      mouseFocuses(tile)
      act(() => {
        moveTo(tile, 560)
      })

      expect(document.activeElement).toBe(current)
    })

    it(`leaves focus where a click put it, ${reading}`, () => {
      const { rail, tile, stood, onNavigate } = strip({ reading })
      const before = elsewhere()
      before.focus()

      press(tile, 500)
      mouseFocuses(tile)
      moveTo(tile, 503)
      release(tile, 503)
      expect(click(tile)).toBe(true)

      // THE CONTROL: it was a click, and no drag.
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
      expect(rail.scrollLeft).toBe(stood)
      expect(document.activeElement).toBe(tile)
    })

    // Only focus that stands on a tile of this rail is handed back. Focus that
    // has gone somewhere else by the time the press is a drag is not the
    // rail's to move.
    it(`leaves focus alone where it is not on a tile of the rail, ${reading}`, () => {
      const { rail, tile, stood } = strip({ reading })
      const before = elsewhere()
      const other = elsewhere()
      before.focus()
      const handed = vi.spyOn(before, 'focus')

      press(tile, 500)
      other.focus()
      moveTo(tile, 560)

      // THE CONTROL: the press did become a drag.
      expect(rail.scrollLeft).toBe(stood - 60)
      expect(document.activeElement).toBe(other)
      expect(handed).not.toHaveBeenCalled()
    })

    // A tile that takes focus has the strip's window moved to it, which is
    // for a reader who reaches a tile by `Tab`. Focus handed back is no such
    // reaching: the tile it goes back to is wherever the strip was before the
    // rail was taken on from it, and a window moved there can end short of
    // what the rail shows.
    it(`leaves the strip's window where it is when it hands focus back to a tile, ${reading}`, () => {
      const { rail, tileOf } = longStrip(reading)
      const pressed = tileOf(72)

      press(pressed, 500)
      mouseFocuses(pressed)
      // THE STATE THIS TEST NAMES: the window has moved off the tile that had
      // focus, to the tile that was pressed, and still holds the first.
      expect(windowOf(rail)).toEqual([56, 88])

      act(() => {
        moveTo(pressed, 560)
      })
      // THE CONTROL: focus was handed back to that tile.
      expect(document.activeElement).toBe(tileOf(60))
      expect(windowOf(rail)).toEqual([56, 88])
    })

    // It is that one focus the window does not follow. A tile that takes
    // focus afterwards, as one reached by `Tab` does, has the window moved to
    // it as it always had.
    it(`moves the strip's window to a tile that takes focus after it has handed focus back, ${reading}`, () => {
      const { rail, tileOf } = longStrip(reading)
      const pressed = tileOf(72)
      press(pressed, 500)
      mouseFocuses(pressed)
      act(() => {
        moveTo(pressed, 560)
      })
      release(rail, 560)
      // THE STATE THIS TEST NAMES: focus has been handed back to a tile, and
      // the window did not follow it.
      expect(document.activeElement).toBe(tileOf(60))
      expect(windowOf(rail)).toEqual([56, 88])

      act(() => tileOf(80).focus())

      expect(windowOf(rail)).toEqual([64, 96])
    })
  }

  it("prevents the browser's own drag of anything on the rail", () => {
    const { host, tile } = strip()

    expect(dragstart(tile)).toBe(false)
    // THE CONTROL: the same event outside the rail is nobody's to prevent.
    expect(dragstart(host)).toBe(true)
  })

  // O216, the glide: a drag let go while the pointer is moving carries the
  // rail on. Left to jsdom an event carries the wall clock's time and a
  // frame comes on a timer. So each case gives its events their times, says
  // what `performance.now()` reads, and runs each frame itself with the
  // time the frame is stamped with. Nothing in one waits, and none passes
  // or fails by how long it took.
  //
  // In figures, and not in the constants' names, for the reason the case
  // about 5 px of travel gives.
  describe('let go while the pointer is moving', () => {
    let pending: Map<number, FrameRequestCallback>
    let nextFrame: number
    let clock: number
    beforeEach(() => {
      pending = new Map()
      nextFrame = 0
      clock = 0
      vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
        nextFrame += 1
        pending.set(nextFrame, cb)
        return nextFrame
      })
      vi.stubGlobal('cancelAnimationFrame', (id: number) => {
        pending.delete(id)
      })
      const now = vi.spyOn(performance, 'now').mockImplementation(() => clock)
      onTestFinished(() => now.mockRestore())
    })
    afterEach(() => {
      vi.unstubAllGlobals()
    })

    /**
     * Runs the frames that have been asked for, each stamped `at`, which is
     * what the clock reads by then. In an `act`, so that a render a frame
     * asked for would be made and counted.
     */
    const frameAt = (at: number) =>
      act(() => {
        clock = at
        const callbacks = [...pending.values()]
        pending.clear()
        for (const callback of callbacks) callback(at)
      })

    /** Runs a frame every 16 ms from `from` until none is asked for, and answers how long that took. */
    function framesUntilNoneIsAsked(from: number): number {
      let elapsed = 0
      while (pending.size > 0 && elapsed < 10_000) {
        elapsed += 16
        frameAt(from + elapsed)
      }
      return elapsed
    }

    /**
     * A flick of `tile`, `by` px along the rail from x 500: pressed at
     * 1000 ms, ten moves 10 ms apart, and ended `rest` ms after the last of
     * them, by a release unless `how` says otherwise. A mouse's, unless
     * `whose` says whose. Answers when it ended.
     *
     * The speed is read from the press on, which is 100 ms before the last
     * move: `by` in 100 ms, so `by` / 100 px a millisecond.
     */
    function flick(
      tile: Element,
      rail: Element,
      by: number,
      {
        rest = 0,
        how = 'release',
        whose = {},
      }: {
        rest?: number
        how?: 'release' | 'cancel' | 'button up'
        whose?: PointerEventInit
      } = {},
    ): number {
      press(tile, 500, { ...whose, timeStamp: 1000 })
      for (let move = 1; move <= 10; move++) {
        moveTo(move === 1 ? tile : rail, 500 + (by * move) / 10, {
          ...whose,
          timeStamp: 1000 + 10 * move,
        })
      }
      clock = 1100 + rest
      const ended = { ...whose, timeStamp: clock }
      if (how === 'release') release(rail, 500 + by, ended)
      else if (how === 'cancel') cancel(rail, ended)
      else moveTo(rail, 500 + by, { ...ended, buttons: 0 })
      return clock
    }

    /** How far a glide begun at `speed` px a millisecond has carried the rail `elapsed` ms in. */
    const carried = (speed: number, elapsed: number) =>
      speed * 325 * (1 - Math.exp(-elapsed / 325))

    /**
     * Gives the rail ends, which jsdom's has none of: its `scrollLeft` takes
     * any number. A rail 2,000 px longer than it is wide, whose `scrollLeft`
     * stays within its travel as a browser's does.
     */
    function bounded(rail: HTMLElement, reading: 'ltr' | 'rtl') {
      let left = rail.scrollLeft
      const [least, most] = reading === 'rtl' ? [-2000, 0] : [0, 2000]
      Object.defineProperty(rail, 'scrollLeft', {
        configurable: true,
        get: () => left,
        set: (to: number) => {
          left = Math.min(Math.max(to, least), most)
        },
      })
    }

    for (const reading of ['ltr', 'rtl'] as const) {
      for (const by of [100, -100]) {
        it(`carries the rail on in later frames, the other way to a pointer going ${by > 0 ? 'right' : 'left'}, ${reading}`, () => {
          const { rail, tile, stood, onNavigate } = strip({ reading })
          const letGo = flick(tile, rail, by)
          const at = stood - by
          // Nothing is written at the release itself: a frame is asked for.
          expect(rail.scrollLeft).toBe(at)
          expect(pending.size).toBe(1)

          const speed = by / 100
          frameAt(letGo + 16)
          expect(rail.scrollLeft).toBeCloseTo(at - carried(speed, 16), 6)
          frameAt(letGo + 325)
          expect(rail.scrollLeft).toBeCloseTo(at - carried(speed, 325), 6)
          // The way the drag took it, which is the other way to the pointer.
          expect(Math.sign(rail.scrollLeft - at)).toBe(-Math.sign(by))
          expect(pending.size).toBe(1)

          // The click the release makes is a drag's, glide or no glide.
          expect(click(rail)).toBe(false)
          expect(onNavigate).not.toHaveBeenCalled()
        })
      }

      // A drag whose release begins a glide has ended as any drag ends: the
      // cursor is put back and the pointer let go of, the focus the press
      // moved is where the drag handed it back to, and the click the release
      // makes is swallowed.
      it(`ends the drag as a drag where its release begins a glide, ${reading}`, () => {
        const { rail, tile, stood, onNavigate, above } = strip({ reading })
        const before = elsewhere()
        before.focus()

        press(tile, 500, { timeStamp: 1000 })
        mouseFocuses(tile)
        for (let move = 1; move <= 10; move++) {
          moveTo(move === 1 ? tile : rail, 500 + 10 * move, {
            timeStamp: 1000 + 10 * move,
          })
        }
        expect(rail.style.cursor).toBe('grabbing')
        expect(capturing.get(1)).toBe(rail)
        clock = 1100
        release(rail, 600, { timeStamp: 1100 })

        // THE STATE THIS TEST NAMES: the release began a glide.
        expect(pending.size).toBe(1)
        expect(rail.scrollLeft).toBe(stood - 100)
        expect(rail.style.cursor).toBe('')
        expect(capturing.size).toBe(0)
        expect(document.activeElement).toBe(before)
        expect(click(rail)).toBe(false)
        expect(onNavigate).not.toHaveBeenCalled()
        expect(above).not.toHaveBeenCalled()
      })

      // A press that creeps to the 5 px that make a drag and is let go at
      // once: 6 px in all, a pixel every 40 ms, and its last two moves 2 ms
      // apart, or 8. Its speed is read from the press on, and not from the
      // move that made it a drag: 3 px in the 82 ms, or the 88, before its
      // last move, which is under 0.1 px a millisecond. Read from the move
      // that made it a drag it is a pixel in those 2 ms, or 8.
      for (const lastAt of [202, 208]) {
        it(`carries nothing on from a press that creeps past 5 px and is let go, its last two moves ${lastAt - 200} ms apart, ${reading}`, () => {
          const { rail, tile, stood } = strip({ reading })

          press(tile, 500, { timeStamp: 0 })
          for (const [at, px] of [
            [40, 1],
            [80, 2],
            [120, 3],
            [160, 4],
            [200, 5],
            [lastAt, 6],
          ]) {
            moveTo(px < 5 ? tile : rail, 500 + px, { timeStamp: at })
          }
          clock = lastAt
          release(rail, 506, { timeStamp: lastAt })

          // THE CONTROL: it did become a drag, and the rail went with it.
          expect(rail.scrollLeft).toBe(stood - 6)
          expect(pending.size).toBe(0)
          frameAt(lastAt + 16)
          expect(rail.scrollLeft).toBe(stood - 6)
        })
      }

      // A flick shorter than the 100 ms the speed is read over is read
      // whole, from its press: 45 px in 30 ms.
      it(`carries the rail on from a quick flick let go 30 ms after its press, at the speed of the whole of it, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })

        press(tile, 500, { timeStamp: 1000 })
        moveTo(tile, 510, { timeStamp: 1010 })
        moveTo(rail, 525, { timeStamp: 1020 })
        moveTo(rail, 545, { timeStamp: 1030 })
        clock = 1030
        release(rail, 545, { timeStamp: 1030 })

        expect(pending.size).toBe(1)
        frameAt(1046)
        expect(rail.scrollLeft).toBeCloseTo(stood - 45 - carried(1.5, 16), 6)
      })

      // Each press begins the reading anew. A second drag pressed 10 ms after
      // the first was let go, and going the other way, is read from its own
      // press: 30 px left in 30 ms. The first drag's last moves are within
      // 100 ms of it still, and read with them it would be going right.
      it(`reads a drag's speed from its own press, where the drag before it ended a moment ago, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })
        flick(tile, rail, 100)

        clock = 1110
        press(tile, 600, { timeStamp: 1110 })
        moveTo(tile, 590, { timeStamp: 1120 })
        moveTo(rail, 580, { timeStamp: 1130 })
        moveTo(rail, 570, { timeStamp: 1140 })
        clock = 1140
        release(rail, 570, { timeStamp: 1140 })

        // THE STATE THIS TEST NAMES: back 30 px from where the first drag
        // left the rail, and a glide begun.
        expect(rail.scrollLeft).toBe(stood - 70)
        expect(pending.size).toBe(1)
        frameAt(1156)
        expect(rail.scrollLeft).toBeCloseTo(stood - 70 - carried(-1, 16), 6)
      })

      // A mouse only, as the drag is. A finger's travel along the rail and a
      // pen's are the browser's to scroll the rail by: nothing here drags
      // for them, and so nothing is there to be carried on.
      for (const [whose, pointer] of [
        ['a finger', { pointerId: 2, pointerType: 'touch' }],
        ['a pen', { pointerId: 3, pointerType: 'pen' }],
      ] as const) {
        it(`carries nothing on for ${whose}, ${reading}`, () => {
          const { rail, tile, stood } = strip({ reading })

          const letGo = flick(tile, rail, 100, { whose: pointer })
          expect(rail.scrollLeft).toBe(stood)
          expect(pending.size).toBe(0)
          frameAt(letGo + 16)
          expect(rail.scrollLeft).toBe(stood)

          // THE CONTROL: the same flick with a mouse.
          flick(tile, rail, 100)
          expect(rail.scrollLeft).toBe(stood - 100)
          expect(pending.size).toBe(1)
        })
      }

      // A drag that is not steady: 100 px at once, a wait, and then 5 px and
      // 45 px in its last 100 ms. Read over those 100 ms it is going at
      // 0.5 px a millisecond. Read from its first move it would be going at
      // a sixth of one, and from its last 50 ms alone at 0.9.
      it(`reads the pointer's speed from all of the drag's last 100 ms and from none of it before, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })
        press(tile, 500, { timeStamp: 700 })
        moveTo(tile, 600, { timeStamp: 800 })
        moveTo(rail, 600, { timeStamp: 1000 })
        moveTo(rail, 605, { timeStamp: 1050 })
        moveTo(rail, 650, { timeStamp: 1100 })
        clock = 1100
        release(rail, 650, { timeStamp: 1100 })

        expect(pending.size).toBe(1)
        frameAt(1116)
        expect(rail.scrollLeft).toBeCloseTo(stood - 150 - carried(0.5, 16), 6)
      })

      // A frame is stamped with the time it began, and a glide is counted
      // from when its release was heard. Should the frame's time be the
      // earlier, the rail does not step back for it.
      it(`stands where it was let go for a frame stamped before the release, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })
        const letGo = flick(tile, rail, 100)

        frameAt(letGo - 8)
        expect(rail.scrollLeft).toBe(stood - 100)
        // And goes on from there.
        expect(pending.size).toBe(1)
        frameAt(letGo + 16)
        expect(rail.scrollLeft).toBeCloseTo(stood - 100 - carried(1, 16), 6)
      })

      it(`carries nothing on from a pointer that had rested 60 ms when it was let go, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })

        const letGo = flick(tile, rail, 100, { rest: 60 })
        expect(pending.size).toBe(0)
        frameAt(letGo + 16)
        expect(rail.scrollLeft).toBe(stood - 100)

        // THE CONTROL: the same flick, let go a millisecond sooner.
        flick(tile, rail, 100, { rest: 59 })
        expect(rail.scrollLeft).toBe(stood - 200)
        expect(pending.size).toBe(1)
      })

      it(`carries nothing on from a pointer going at under 0.1 px a millisecond, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })

        const letGo = flick(tile, rail, 9)
        expect(pending.size).toBe(0)
        frameAt(letGo + 16)
        expect(rail.scrollLeft).toBe(stood - 9)

        // THE CONTROL: at 0.1 px a millisecond it is carried on, by that.
        const again = flick(tile, rail, 10)
        expect(pending.size).toBe(1)
        frameAt(again + 16)
        expect(rail.scrollLeft).toBeCloseTo(stood - 19 - carried(0.1, 16), 6)
      })

      // A `pointercancel` ends the drag and is no release: the browser took
      // the pointer away, and nobody let the strip go.
      it(`carries nothing on after a \`pointercancel\`, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })

        const ended = flick(tile, rail, 100, { how: 'cancel' })
        expect(pending.size).toBe(0)
        frameAt(ended + 16)
        expect(rail.scrollLeft).toBe(stood - 100)

        // THE CONTROL: the same flick, let go.
        flick(tile, rail, 100)
        expect(pending.size).toBe(1)
      })

      // Nor is a move with the button up, which is how a press ends whose
      // release was heard nowhere: it can come long after the button did.
      it(`carries nothing on from a drag that ends at a move with the button up, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })

        const ended = flick(tile, rail, 100, { how: 'button up' })
        expect(pending.size).toBe(0)
        frameAt(ended + 16)
        expect(rail.scrollLeft).toBe(stood - 100)
        // THE CONTROL: that move did end the drag.
        expect(rail.style.cursor).toBe('')
      })

      // 325 px in all at 1 px a millisecond. Half a pixel short of that is
      // 2,105 ms in, and the first frame after it, at one every 16 ms, is
      // 2,112 ms in.
      it(`asks for no more frames once it has come to rest, ${reading}`, () => {
        const { rail, tile, stood } = strip({ reading })
        const letGo = flick(tile, rail, 100)

        const took = framesUntilNoneIsAsked(letGo)

        expect(took).toBe(2112)
        const rest = stood - 100 - 325
        expect(Math.abs(rail.scrollLeft - rest)).toBeLessThan(0.5)
        // And it has not gone past where it comes to rest.
        expect(rail.scrollLeft).toBeGreaterThan(rest)
      })

      // Toward each end of the rail in turn, from 300 px short of it: the end
      // is 200 px off once the drag has taken 100. The glide would carry the
      // rail 325. It has carried it 201, a pixel past the end, 313 ms in, and
      // the first frame after that is 320 ms in.
      //
      // The start of the volume is where `scrollLeft` is 0 in either
      // direction. Its end is where it is 2,000 one way round and -2,000 the
      // other: an end that is not 0.
      for (const [which, end] of [
        ['the start of the volume', 0],
        ['the end of the volume', 2000],
      ] as const) {
        it(`asks for no more frames once the rail will go no further, at ${which}, ${reading}`, () => {
          const { rail, tile } = strip({ reading })
          bounded(rail, reading)
          const sign = reading === 'rtl' ? -1 : 1
          rail.scrollLeft = sign * Math.abs(end - 300)
          // A pointer going right takes `scrollLeft` down, which is toward
          // the start one way round and toward the end the other.
          const towardStart = sign * 100
          const letGo = flick(
            tile,
            rail,
            end === 0 ? towardStart : -towardStart,
          )
          // THE STATE THIS TEST NAMES: short of the rail's end, and nearer
          // it than the glide would go.
          expect(Math.abs(Math.abs(rail.scrollLeft) - end)).toBe(200)

          const took = framesUntilNoneIsAsked(letGo)

          expect(took).toBe(320)
          expect(Math.abs(rail.scrollLeft)).toBe(end)
        })
      }

      for (const [whose, init] of [
        ["a mouse's primary button", {}],
        ["a mouse's secondary button", { button: 2, buttons: 2 }],
        ['a finger', { pointerId: 2, pointerType: 'touch' }],
        ['a pen', { pointerId: 3, pointerType: 'pen' }],
      ] as const) {
        it(`stops at a press of ${whose} on the rail, ${reading}`, () => {
          const { rail, tile, stood } = strip({ reading })
          const letGo = flick(tile, rail, 100)
          frameAt(letGo + 16)
          const carriedTo = rail.scrollLeft
          // THE STATE THIS TEST NAMES: a glide under way.
          expect(carriedTo).not.toBe(stood - 100)
          expect(pending.size).toBe(1)

          press(tile, 600, init)

          // The frame it had asked for is given back, and none is run.
          expect(pending.size).toBe(0)
          frameAt(letGo + 200)
          expect(rail.scrollLeft).toBe(carriedTo)
        })
      }

      // Any wheel over the rail: one the rail takes and moves by, and one it
      // leaves to the browser, which scrolls the rail with it.
      it(`stops at a wheel over the rail, ${reading}`, () => {
        const { rail, tile } = strip({ reading })
        const wheel = (init: WheelEventInit) =>
          tile.dispatchEvent(
            new WheelEvent('wheel', {
              bubbles: true,
              cancelable: true,
              ...init,
            }),
          )

        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)
        // Sideways: not the rail's to take, so nothing here moves the rail.
        expect(wheel({ deltaX: 100 })).toBe(true)
        expect(pending.size).toBe(0)
        frameAt(letGo + 200)
        expect(rail.scrollLeft).toBe(carriedTo)

        const again = flick(tile, rail, 100)
        frameAt(again + 16)
        expect(pending.size).toBe(1)
        expect(wheel({ deltaY: 100 })).toBe(false)
        const wheeledTo = rail.scrollLeft
        expect(pending.size).toBe(0)
        frameAt(again + 200)
        expect(rail.scrollLeft).toBe(wheeledTo)
      })

      it(`stops when the strip scrolls to the page that was turned to, ${reading}`, () => {
        const { host, rail, tile, onNavigate } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)
        scrolledTo.length = 0

        rerender(
          <Filmstrip {...props({ reading, onNavigate, currentIndex: 3 })} />,
        )

        // THE CONTROL: the strip did scroll to the new page's tile.
        expect(scrolledTo).toEqual([buttons(host)[3]])
        expect(pending.size).toBe(0)
        frameAt(letGo + 200)
        expect(rail.scrollLeft).toBe(carriedTo)
      })

      // The rail turned round measures `scrollLeft` from its other end, and
      // a glide's next frame would write a place reckoned from the first.
      it(`stops when the reading direction changes, ${reading}`, () => {
        const { host, rail, tile, onNavigate } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)

        const turned = reading === 'rtl' ? 'ltr' : 'rtl'
        rerender(<Filmstrip {...props({ reading: turned, onNavigate })} />)

        // THE CONTROL: the same strip, and its rail does run the other way.
        expect(railOf(host)).toBe(rail)
        expect(rail.style.flexDirection).toBe(
          turned === 'rtl' ? 'row-reverse' : 'row',
        )
        expect(pending.size).toBe(0)
        frameAt(letGo + 200)
        expect(rail.scrollLeft).toBe(carriedTo)
      })

      it(`stops when the strip is taken away, ${reading}`, () => {
        const { rail, tile } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)

        rerender(null)

        expect(pending.size).toBe(0)
        frameAt(letGo + 200)
        expect(rail.scrollLeft).toBe(carriedTo)
      })

      // A page turned to that is outside the window the strip has built. The
      // strip's first try at scrolling to the page's tile finds no tile, and
      // it is the try after the window is built round the page that scrolls.
      // That one has to stop the glide as the first would have.
      it(`stops when the strip scrolls to a page turned to outside its window, at its second try, ${reading}`, () => {
        const { rail, tileOf, turnTo } = longStrip(reading)
        // The rail a tenth of the way along its travel, which is the
        // thirteenth of its 120 tiles: the window follows the scroll there,
        // in the frame the scroll takes.
        Object.defineProperty(rail, 'scrollWidth', {
          value: 12000,
          configurable: true,
        })
        Object.defineProperty(rail, 'clientWidth', {
          value: 1000,
          configurable: true,
        })
        rail.scrollLeft = reading === 'rtl' ? -1100 : 1100
        rail.dispatchEvent(new Event('scroll'))
        frameAt(900)
        // THE STATE THIS TEST NAMES: a window that holds no tile for the page
        // about to be turned to.
        expect(windowOf(rail)).toEqual([0, 28])

        const letGo = flick(tileOf(10), rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)
        scrolledTo.length = 0

        turnTo(61)

        // THE CONTROL: the window was built round the new page, and the strip
        // scrolled to its tile, once.
        expect(windowOf(rail)).toEqual([45, 77])
        expect(scrolledTo).toEqual([tileOf(61)])
        expect(pending.size).toBe(0)
        frameAt(letGo + 200)
        expect(rail.scrollLeft).toBe(carriedTo)
      })

      // A reader who reaches a tile by `Tab` while a glide is under way has
      // the browser scroll that tile into view, and the glide's next frame
      // would write the rail back. Focus that goes anywhere else is nothing
      // to the glide.
      it(`stops when a tile of the rail takes focus, ${reading}`, () => {
        const { host, rail, tile } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        elsewhere().focus()
        expect(pending.size).toBe(1)

        act(() => buttons(host)[1].focus())

        expect(pending.size).toBe(0)
        frameAt(letGo + 200)
        expect(rail.scrollLeft).toBe(carriedTo)
      })

      // A render of the strip is no reason for a glide to stop. `App` hands
      // the strip new `groups` and `slots` arrays at every render of its own,
      // and the reading-time readout's tick is one.
      it(`carries on through a render of the strip, ${reading}`, () => {
        const { host, rail, tile, stood, onNavigate } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)

        rerender(
          <Filmstrip
            {...props({
              reading,
              onNavigate,
              groups: groups.map((group) => ({ ...group })),
              slots: slots.map((slot) => ({ ...slot })),
            })}
          />,
        )

        expect(railOf(host)).toBe(rail)
        expect(pending.size).toBe(1)
        frameAt(letGo + 32)
        expect(rail.scrollLeft).toBeCloseTo(stood - 100 - carried(1, 32), 6)
      })

      // The rail does no more with a finger or a pen than stop the glide. Its
      // tap is not swallowed as a mouse's press is, and the focus it moves is
      // not handed back: it is a click on the tile it lands on.
      for (const [whose, pointer] of [
        ['a finger', { pointerId: 2, pointerType: 'touch' }],
        ['a pen', { pointerId: 3, pointerType: 'pen' }],
      ] as const) {
        it(`leaves a tap of ${whose} that stops a glide the click it is, ${reading}`, () => {
          const { rail, tile, onNavigate } = strip({ reading })
          const before = elsewhere()
          before.focus()
          const letGo = flick(tile, rail, 100)
          // The click the flick's own release makes, swallowed as a drag's is.
          expect(click(rail)).toBe(false)
          timersLand()
          frameAt(letGo + 16)
          expect(pending.size).toBe(1)

          press(tile, 600, pointer)
          // Where a tap moves focus to the tile, as a mouse's press does.
          act(() => tile.focus())
          release(tile, 600, pointer)

          // THE CONTROL: the tap did stop the glide.
          expect(pending.size).toBe(0)
          expect(click(tile)).toBe(true)
          expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
          expect(document.activeElement).toBe(tile)
        })
      }

      // The tile under a press that stops a glide still carrying the rail is
      // one the glide carried there: the reader was stopping the strip, and
      // chose no page. 16 ms into a glide from 1 px a millisecond the rail is
      // going at 0.95.
      it(`takes a press that stops a glide still carrying the rail, let go where it landed, as no click, and hands back the focus it moved, ${reading}`, () => {
        const { rail, tile, onNavigate, above } = strip({ reading })
        const before = elsewhere()
        before.focus()
        const letGo = flick(tile, rail, 100)
        // The click the flick's own release makes, swallowed as a drag's is.
        expect(click(rail)).toBe(false)
        timersLand()
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft

        press(tile, 600)
        mouseFocuses(tile)
        expect(document.activeElement).toBe(tile)
        release(tile, 600)

        expect(document.activeElement).toBe(before)
        expect(click(tile)).toBe(false)
        expect(onNavigate).not.toHaveBeenCalled()
        expect(above).not.toHaveBeenCalled()
        expect(rail.scrollLeft).toBe(carriedTo)
        expect(pending.size).toBe(0)

        // THE CONTROL: the same press and release with no glide to stop is a
        // click, and its focus is the tile's to keep.
        press(tile, 600)
        mouseFocuses(tile)
        release(tile, 600)
        expect(click(tile)).toBe(true)
        expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
        expect(document.activeElement).toBe(tile)
      })

      it(`drags the rail from where the glide was stopped, for a press that stops one and goes on past 5 px, ${reading}`, () => {
        const { rail, tile } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft

        press(tile, 600)
        moveTo(tile, 604)
        expect(rail.scrollLeft).toBe(carriedTo)
        moveTo(tile, 605)
        expect(rail.scrollLeft).toBe(carriedTo - 5)
        expect(rail.style.cursor).toBe('grabbing')
        expect(capturing.get(1)).toBe(rail)
        moveTo(rail, 540)
        expect(rail.scrollLeft).toBe(carriedTo + 60)

        // Nothing of the glide is left to run under the drag.
        expect(pending.size).toBe(0)
        frameAt(letGo + 500)
        expect(rail.scrollLeft).toBe(carriedTo + 60)
      })

      // The pointer's place is kept from the press on, for a press that is to
      // become a drag. One that stops a glide and is let go short of a drag
      // has moved all the same, 4 px in 2 ms here, and that is no flick.
      it(`carries nothing on from a press that stops a glide and is let go short of a drag, however fast it went, ${reading}`, () => {
        const { rail, tile } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        expect(click(rail)).toBe(false)
        timersLand()
        frameAt(letGo + 16)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)

        press(tile, 600, { timeStamp: 2000 })
        moveTo(tile, 604, { timeStamp: 2002 })
        release(tile, 604, { timeStamp: 2002 })

        expect(pending.size).toBe(0)
        expect(rail.scrollLeft).toBe(carriedTo)
        expect(rail.style.cursor).toBe('')
      })

      // A glide runs on until it is half a pixel from rest, and for most of
      // that time the strip has stopped to the eye. A press then stops the
      // glide, as any press does, and is otherwise the press it would be with
      // no glide: a click on the tile, whose focus is the tile's to keep.
      //
      // The line is 0.1 px a millisecond. A glide from 1 px a millisecond is
      // under it from 749 ms in: 325 × ln 10 is 748.3.
      it(`takes a press that stops a glide gone under 0.1 px a millisecond as the click it is, and leaves its focus on the tile, ${reading}`, () => {
        const { rail, tile, onNavigate, above } = strip({ reading })
        const before = elsewhere()
        before.focus()
        const letGo = flick(tile, rail, 100)
        expect(click(rail)).toBe(false)
        timersLand()
        frameAt(letGo + 749)
        const carriedTo = rail.scrollLeft
        // THE STATE THIS TEST NAMES: a glide still under way, frames to come.
        expect(pending.size).toBe(1)

        press(tile, 600)
        // It stopped the glide, at the press and before the tile took focus,
        // which stops one too.
        expect(pending.size).toBe(0)
        mouseFocuses(tile)
        release(tile, 600)

        frameAt(letGo + 900)
        expect(rail.scrollLeft).toBe(carriedTo)
        // And it is a click.
        expect(click(tile)).toBe(true)
        expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
        expect(above).toHaveBeenCalledOnce()
        expect(document.activeElement).toBe(tile)
      })

      // Either side of the line, a millisecond apart, and with the pointer
      // going left: the rail's speed is under 0 then, and it is how fast and
      // not which way that is asked.
      it(`takes a press 748 ms into a glide from 1 px a millisecond as no click, and one 749 ms in as a click, ${reading}`, () => {
        const { rail, tile, onNavigate } = strip({ reading })
        /** Whether a press `ms` into a glide, let go where it landed, is a click. */
        const clicksAt = (ms: number) => {
          const letGo = flick(tile, rail, -100)
          expect(click(rail)).toBe(false)
          timersLand()
          frameAt(letGo + ms)
          expect(pending.size).toBe(1)
          press(tile, 400)
          release(tile, 400)
          return click(tile)
        }

        expect(clicksAt(748)).toBe(false)
        expect(onNavigate).not.toHaveBeenCalled()
        expect(clicksAt(749)).toBe(true)
        expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
      })

      // And on the line: a glide going at exactly 0.1 px a millisecond is
      // still carrying the rail. The slowest glide there is begins at that,
      // so a press at the instant of its release is no click, and one a
      // millisecond later is.
      it(`takes a glide going at exactly 0.1 px a millisecond as still carrying the rail, ${reading}`, () => {
        const { rail, tile, onNavigate } = strip({ reading })

        flick(tile, rail, 10)
        // The flick's own click first, so that what swallows the next one is
        // the press and not the drag before it.
        expect(click(rail)).toBe(false)
        timersLand()
        expect(pending.size).toBe(1)
        press(tile, 510)
        release(tile, 510)
        expect(click(tile)).toBe(false)
        expect(onNavigate).not.toHaveBeenCalled()

        const again = flick(tile, rail, 10)
        expect(click(rail)).toBe(false)
        timersLand()
        frameAt(again + 1)
        expect(pending.size).toBe(1)
        press(tile, 510)
        release(tile, 510)
        expect(click(tile)).toBe(true)
        expect(onNavigate).toHaveBeenCalledExactlyOnceWith(2)
      })

      it(`drags from a press that stops a glide gone under 0.1 px a millisecond and goes on past 5 px, ${reading}`, () => {
        const { rail, tile, onNavigate } = strip({ reading })
        const letGo = flick(tile, rail, 100)
        expect(click(rail)).toBe(false)
        timersLand()
        frameAt(letGo + 749)
        const carriedTo = rail.scrollLeft
        expect(pending.size).toBe(1)

        press(tile, 600)
        expect(pending.size).toBe(0)
        moveTo(tile, 604)
        expect(rail.scrollLeft).toBe(carriedTo)
        moveTo(tile, 605)
        expect(rail.scrollLeft).toBe(carriedTo - 5)
        expect(rail.style.cursor).toBe('grabbing')
        expect(capturing.get(1)).toBe(rail)

        // It is a drag, and the click its release makes is a drag's.
        release(rail, 605)
        expect(click(rail)).toBe(false)
        expect(onNavigate).not.toHaveBeenCalled()
      })

      // As the case of the drag's own handing back, above: here the tile that
      // had focus is the one the glide has carried the rail away from.
      it(`leaves the strip's window where it is when a press that stopped a glide hands focus back to a tile, ${reading}`, () => {
        const { rail, tileOf } = longStrip(reading)
        const pressed = tileOf(72)
        const letGo = flick(pressed, rail, 100)
        expect(click(rail)).toBe(false)
        timersLand()
        frameAt(letGo + 16)
        // THE STATE THIS TEST NAMES: a glide under way, and focus where it
        // was before the flick.
        expect(pending.size).toBe(1)
        expect(document.activeElement).toBe(tileOf(60))

        press(pressed, 600)
        mouseFocuses(pressed)
        expect(windowOf(rail)).toEqual([56, 88])
        act(() => {
          release(pressed, 600)
        })

        // THE CONTROL: focus was handed back to that tile.
        expect(document.activeElement).toBe(tileOf(60))
        expect(windowOf(rail)).toEqual([56, 88])

        // And a tile that takes focus after that has the window moved to it.
        act(() => tileOf(80).focus())
        expect(windowOf(rail)).toEqual([64, 96])
      })

      it(`carries nothing on where \`glides\` is false, and does once it is true, ${reading}`, () => {
        const { rail, tile, stood, onNavigate } = strip({
          reading,
          glides: false,
        })

        const letGo = flick(tile, rail, 100)
        expect(pending.size).toBe(0)
        frameAt(letGo + 16)
        expect(rail.scrollLeft).toBe(stood - 100)

        // THE CONTROL, and the setting is the one of the latest render: the
        // same flick once it is turned on, and again once it is turned off.
        rerender(
          <Filmstrip {...props({ reading, onNavigate, glides: true })} />,
        )
        const again = flick(tile, rail, 100)
        expect(pending.size).toBe(1)
        frameAt(again + 16)
        expect(rail.scrollLeft).toBeCloseTo(stood - 200 - carried(1, 16), 6)
        press(tile, 500)
        release(tile, 500)

        rerender(
          <Filmstrip {...props({ reading, onNavigate, glides: false })} />,
        )
        flick(tile, rail, 100)
        expect(pending.size).toBe(0)
      })

      it(`carries nothing on where the system asks for reduced motion, ${reading}`, () => {
        let reduce = true
        const asked: string[] = []
        vi.stubGlobal('matchMedia', (query: string) => {
          asked.push(query)
          return {
            matches: reduce && query === '(prefers-reduced-motion: reduce)',
          }
        })
        const { rail, tile, stood } = strip({ reading })

        const letGo = flick(tile, rail, 100)
        expect(pending.size).toBe(0)
        frameAt(letGo + 16)
        expect(rail.scrollLeft).toBe(stood - 100)
        expect(asked).toContain('(prefers-reduced-motion: reduce)')

        // THE CONTROL: the same flick where the system does not ask.
        reduce = false
        flick(tile, rail, 100)
        expect(pending.size).toBe(1)
      })

      // A glide is written straight onto the rail, a frame at a time. State
      // for it would render the strip on every one of them.
      it(`renders nothing for a glide, from its release to its rest, ${reading}`, () => {
        let commits = 0
        const host = mount(
          <Profiler
            id="strip"
            onRender={() => {
              commits += 1
            }}
          >
            <Filmstrip {...props({ reading })} />
          </Profiler>,
        )
        const rail = railOf(host)
        const tile = buttons(host)[2]
        const mounted = commits
        expect(mounted).toBeGreaterThan(0)

        let letGo = 0
        act(() => {
          letGo = flick(tile, rail, 100)
        })
        const took = framesUntilNoneIsAsked(letGo)

        // THE CONTROL: the glide happened, all of it.
        expect(took).toBe(2112)
        expect(rail.scrollLeft).toBeLessThan(-100 - 324)
        expect(commits).toBe(mounted)
      })
    }
  })
})
