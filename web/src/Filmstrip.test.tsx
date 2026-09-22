import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
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
  // CSS constant claims — well outside the 0.5 px tolerance, so the
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

  // O140's other measured case: under desktop emulation at dpr 1 on the same
  // machine, where the rail subtracts 15 px of its own height, a loaded tile
  // is 58.16 px against the same 58 px nominal (54 unit + 2×2 border) — a
  // 0.16 px difference, inside tolerance, which must not cause a write.
  // *What* performs that subtraction is not established — see `tiles.ts`'s
  // `refinedStripUnit`, which records why O138 cannot be cited for it — and
  // this test does not depend on the answer: it pins the tolerance, not a
  // cause.
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
  // a count of tiles the focus has drifted (see the handler's own doc in
  // `Filmstrip.tsx` for why a drift-based rule was replaced: it assumed the
  // viewport straddles the focus tile, when the focus is the rail's leading
  // edge and the viewport extends forward from it, which cost a 321 px gap
  // at 1440 px), and not on the first/last *rendered tile's* box either — an
  // earlier version of this fix tried that, checking both ends of the DOM
  // order against both ends of the rail on the reasoning that checking both
  // made it agnostic to reading direction. It did not: the rail is
  // `row-reverse` under `rtl`, so the last tile in the DOM sits at the left
  // of the screen, not the right, and checking both ends of the DOM order
  // makes one of the two checks true on nearly every frame instead. Measured
  // driving a 1.2 s flick at 390×844 under 4× CPU throttle: 71 re-anchors
  // across 182 frames, worse than the 65 the drift-based rule produced and
  // worse than the 24 the spacer rule below achieves. A spacer reaching the
  // visible strip *is* the blank gap this guard exists to prevent, which is
  // symmetric under either flex direction with no reading-direction
  // arithmetic needed.
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
