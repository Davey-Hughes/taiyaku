import { describe, expect, it } from 'vitest'
import { noCohort } from './cohort'
import { unboundedDecodeQueue } from './decode'
import { GroupView, type GroupViewProps } from './GroupView'
import { fitModule } from './modules/fit'
import type { Group } from './pairing'
import { mount, rerender } from './testing/mount'

/**
 * `GroupView` in `jsdom`, and the first test in it is the detector for an
 * invariant that had none.
 *
 * `PageView.browser.test.tsx` was believed to guard the reconciliation rule
 * this component holds. It does not and never did: it imports neither `App`
 * nor this component, and every one of its six tests mounts a hand-written
 * replica built from bare `PageView`s — so all six pass identically whether
 * the group's two-cell shape is right, wrong, or absent. What actually needs
 * guarding is **node identity across a wide → paired change**, and asking
 * about that needs no layout engine, no real decode and no
 * `getBoundingClientRect`. So it runs here, which means it runs in CI.
 *
 * Every test below is unbuffered, which is what lets them assert on the drawn
 * `<img>` with no decode to arrange: `PageView` puts `src` straight on the
 * element in that mode and reports it through `onShown` all the same. The
 * decode gate itself is `PageView.test.tsx`'s subject, not this file's.
 */

const paired: Group = { slots: [0, 1] }
/** A joined spread: `cellsFor` gives both cells one URL and collapses the second. */
const spread: Group = { slots: [2, 3], spread: true }
/** An unpartnered page — `alone` without `wide`, which keeps its own half-width cell. */
const solo: Group = { slots: [7] }

function props(over: Partial<GroupViewProps> = {}): GroupViewProps {
  return {
    group: paired,
    mode: 'two',
    reading: 'rtl',
    src: (slot) => `/p/${slot}.png`,
    alt: (slot) => `page ${slot}`,
    hug: () => 'centre',
    buffered: false,
    // Neither is reached while unbuffered — `PageView` starts no decode there
    // — but both are required props and a real one says so honestly.
    decodeQueue: unboundedDecodeQueue,
    cohort: noCohort,
    fit: fitModule.defaults,
    drawn: {},
    onShown: () => undefined,
    ...over,
  }
}

const imagesIn = (host: HTMLElement) => [...host.querySelectorAll('img')]
/** The two-cell layout's second cell — the `<div>` whose style says whether it shows. */
const secondCell = (host: HTMLElement) =>
  host.firstElementChild?.children[1] as HTMLElement

describe('GroupView, reconciliation', () => {
  // **The detector.** A remount destroys node identity and a prop update
  // preserves it, so this asserts on the element objects themselves rather
  // than on a selector that would match an equivalent-looking replacement.
  //
  // Watched failing before it was trusted: with the two-page branch returning
  // a bare `<PageView>` for a wide group — the shape this component replaced —
  // both assertions on the wide → paired step fail, because React sees a
  // different element type at that position and mounts a fresh subtree with
  // nothing decoded in it.
  it('keeps the same two <img> nodes across a wide group and back', () => {
    const host = mount(<GroupView {...props({ group: paired })} />)
    const [firstImg, secondImg] = imagesIn(host)
    expect(firstImg).toBeDefined()
    expect(secondImg).toBeDefined()

    rerender(<GroupView {...props({ group: spread })} />)
    expect(imagesIn(host)[0]).toBe(firstImg)
    expect(imagesIn(host)[1]).toBe(secondImg)

    rerender(<GroupView {...props({ group: paired })} />)
    expect(imagesIn(host)[0]).toBe(firstImg)
    expect(imagesIn(host)[1]).toBe(secondImg)
  })

  // The other half of the same claim: the nodes survived *and* the change was
  // real. Without this, a rerender that silently did nothing would satisfy
  // every assertion above.
  it('changes the sources on those same nodes, rather than nothing happening', () => {
    const host = mount(<GroupView {...props({ group: paired })} />)
    const [firstImg] = imagesIn(host)
    expect(firstImg?.getAttribute('src')).toBe('/p/1.png')

    rerender(<GroupView {...props({ group: spread })} />)
    // A spread's own two slots share one page; `cellsFor` puts its first slot
    // in both cells, and neither is reordered by reading direction.
    expect(imagesIn(host).map((i) => i.getAttribute('src'))).toEqual([
      '/p/2.png',
      '/p/3.png',
    ])
    expect(imagesIn(host)[0]).toBe(firstImg)
  })

  // `rtl` puts the higher slot on the left. The mirrored second cell of a
  // `wide` group is not reordered, which `cellsFor` states and this pins from
  // the outside.
  it('orders an ordinary pair by reading direction', () => {
    const host = mount(<GroupView {...props({ reading: 'ltr' })} />)
    expect(imagesIn(host).map((i) => i.getAttribute('src'))).toEqual([
      '/p/0.png',
      '/p/1.png',
    ])
  })
})

describe('GroupView, whether the second cell shows', () => {
  // With nothing reported yet there is nothing on screen to read, so the
  // target's own answer stands — which is what a group's first frame is.
  it('follows the target while the cells have reported nothing', () => {
    const pair = mount(<GroupView {...props({ group: paired, drawn: {} })} />)
    expect(secondCell(pair).style.visibility).toBe('')
    expect(secondCell(pair).getAttribute('aria-hidden')).toBeNull()

    const one = mount(<GroupView {...props({ group: solo, drawn: {} })} />)
    expect(secondCell(one).style.visibility).toBe('hidden')
    expect(secondCell(one).getAttribute('aria-hidden')).toBe('true')
  })

  // **The flicker fix's own assertion.** State moves on the keypress and the
  // pictures wait for their decode, so the target says "a pair" while both
  // cells are still drawing one page. Keyed on the target, the layout flapped
  // — three changes in 114 ms during a flick, with the artwork never changing.
  it('hides the second cell while both cells draw one page, whatever the target says', () => {
    const host = mount(
      <GroupView
        {...props({
          group: paired,
          drawn: { first: 'a.png', second: 'a.png' },
        })}
      />,
    )
    expect(secondCell(host).style.visibility).toBe('hidden')
    expect(secondCell(host).getAttribute('aria-hidden')).toBe('true')
  })

  it('shows the second cell while the cells draw two pages, whatever the target says', () => {
    const host = mount(
      <GroupView
        {...props({ group: solo, drawn: { first: 'a.png', second: 'b.png' } })}
      />,
    )
    expect(secondCell(host).style.visibility).toBe('')
    expect(secondCell(host).getAttribute('aria-hidden')).toBeNull()
  })

  // **This is the test that pins `drawn` being a prop.** It is only passable
  // if the answer comes from outside, so it fails the moment somebody moves
  // that state inside this component, where a fresh mount has nothing — which
  // is exactly the state a `two` → `scroll` → `two` excursion produces.
  it('draws what a populated drawn says on the very first mount', () => {
    const host = mount(
      <GroupView
        {...props({ group: solo, drawn: { first: 'a.png', second: 'b.png' } })}
      />,
    )
    expect(secondCell(host).style.visibility).toBe('')
  })

  // `wide` is the one question that does *not* follow the pixels: a spread is
  // one double-width image whichever generation is showing, so the cell is
  // collapsed from the target and reading it off the drawn sources would let a
  // mid-flick coincidence resize the artwork.
  it('collapses the second cell for a spread, from the target and not the pixels', () => {
    const host = mount(
      <GroupView
        {...props({
          group: spread,
          drawn: { first: 'a.png', second: 'b.png' },
        })}
      />,
    )
    expect(secondCell(host).style.width).toBe('0px')
    // Not merely hidden: an unpartnered page keeps its width and only goes
    // empty, which is the distinction that stopped the low-speed flicker.
    expect(secondCell(host).style.visibility).toBe('')
  })

  // The seam is padding on the cells, and a `wide` or `alone` group has no
  // seam to open — padding the first cell then narrows the box its one page
  // centres in and shifts it off centre.
  it('opens the seam only for a real pair', () => {
    const gap = { ...fitModule.defaults, gap: 40 }
    const pair = mount(<GroupView {...props({ fit: gap })} />)
    const firstCell = pair.firstElementChild?.firstElementChild as HTMLElement
    expect(firstCell.style.paddingRight).toBe('20px')
    expect(secondCell(pair).style.paddingLeft).toBe('20px')

    const one = mount(<GroupView {...props({ group: solo, fit: gap })} />)
    const onlyCell = one.firstElementChild?.firstElementChild as HTMLElement
    expect(onlyCell.style.paddingRight).toBe('')
  })
})

describe('GroupView, the other two modes', () => {
  it('draws one page and no cells at all in single mode', () => {
    const host = mount(<GroupView {...props({ mode: 'single' })} />)
    const images = imagesIn(host)
    expect(images).toHaveLength(1)
    expect(images[0]?.getAttribute('src')).toBe('/p/0.png')
    // Unbuffered, which is what `loading="lazy"` rides on.
    expect(images[0]?.getAttribute('loading')).toBe('lazy')
  })

  it('draws the first slot of a spread as one wide page in scroll mode', () => {
    const host = mount(
      <GroupView {...props({ mode: 'scroll', group: spread })} />,
    )
    expect(imagesIn(host).map((i) => i.getAttribute('src'))).toEqual([
      '/p/2.png',
    ])
  })
})

describe('GroupView, what it asks of its caller', () => {
  // The hug is asked by slot rather than written out here, so the page this
  // draws and the geometry that measures it cannot disagree.
  it('threads the caller hug through to each cell', () => {
    const host = mount(
      <GroupView
        {...props({
          hug: (_group, slot) => (slot === 1 ? 'right-edge' : 'left-edge'),
        })}
      />,
    )
    // `rtl`, so slot 1 is the first cell.
    expect(imagesIn(host)[0]?.style.objectPosition).toBe('100% 50%')
    expect(imagesIn(host)[1]?.style.objectPosition).toBe('0% 50%')
  })

  it('reports which cell painted what, so the caller can hold the record', () => {
    const seen: [string, string][] = []
    mount(
      <GroupView
        {...props({ onShown: (cell, src) => seen.push([cell, src]) })}
      />,
    )
    expect(seen).toEqual([
      ['first', '/p/1.png'],
      ['second', '/p/0.png'],
    ])
  })

  it('reports again when a turn changes what a cell draws', () => {
    const seen: [string, string][] = []
    const onShown = (cell: 'first' | 'second', src: string) =>
      seen.push([cell, src])
    mount(<GroupView {...props({ onShown })} />)
    seen.length = 0
    rerender(<GroupView {...props({ group: spread, onShown })} />)
    expect(seen).toEqual([
      ['first', '/p/2.png'],
      ['second', '/p/3.png'],
    ])
  })
})
