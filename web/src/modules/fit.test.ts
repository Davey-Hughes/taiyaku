import { describe, expect, it } from 'vitest'
import { cellOf } from '../pairing'
import { fitModule, hugForCell, pageStyle, seamStyle } from './fit'
import { registry } from './registry'

describe('fitModule', () => {
  it('is registered', () => {
    expect(registry.ids).toContain('fit')
  })

  it('claims no keys — it is tuned in settings, not pressed mid-read', () => {
    expect(fitModule.keys ?? []).toEqual([])
  })

  it('upscales by default, which reverses the M5 design deliberately', () => {
    expect(fitModule.defaults.upscaleToFit).toBe(true)
  })

  it('offers the filter as a choice, because manga is line art', () => {
    expect(fitModule.options?.upscaleFilter).toEqual(['smooth', 'crisp'])
    expect(fitModule.defaults.upscaleFilter).toBe('smooth')
  })

  it('closes the seam by default', () => {
    expect(fitModule.defaults.gap).toBe(0)
  })
})

describe('hugForCell', () => {
  // The one thing in this translation it is possible to get backwards, and the
  // reason `ArtworkHug` is named for the edge rather than for the cell: the
  // *first* cell is the one on the left, so the seam — and therefore its hug —
  // is on its right.
  it('pulls the first cell right and the second left', () => {
    expect(hugForCell('first')).toBe('right-edge')
    expect(hugForCell('second')).toBe('left-edge')
  })

  // A joined spread fills its group. There is no seam inside one image.
  it('centres the only cell', () => {
    expect(hugForCell('only')).toBe('centre')
  })

  // The pair this and `cellOf` make has to come out the same way round under
  // both reading directions: `cellsFor` has already applied the work's
  // direction by the time a `Cell` exists, so applying it a second time here
  // would hug both pages to the same side. Asserted through `cellOf` rather
  // than restated, since that is exactly how the caller asks.
  it('hugs by screen position, not by reading direction', () => {
    const pair = { slots: [4, 5] }
    expect(hugForCell(cellOf(pair, 'rtl', 5))).toBe('right-edge')
    expect(hugForCell(cellOf(pair, 'rtl', 4))).toBe('left-edge')
    expect(hugForCell(cellOf(pair, 'ltr', 4))).toBe('right-edge')
    expect(hugForCell(cellOf(pair, 'ltr', 5))).toBe('left-edge')
  })

  // An unpartnered page keeps its own half-width cell (`cellsFor`'s `alone`
  // without `wide`), so it is a `'first'` and hugs the seam like any other
  // first cell; a real spread is `'only'` and does not.
  it('hugs a solo slot but not a spread', () => {
    expect(hugForCell(cellOf({ slots: [7] }, 'rtl', 7))).toBe('right-edge')
    expect(hugForCell(cellOf({ slots: [2, 3], spread: true }, 'rtl', 2))).toBe(
      'centre',
    )
  })
})

describe('pageStyle', () => {
  const prefs = { ...fitModule.defaults }

  it('lets a small page grow when upscaling, by sizing to the box', () => {
    const s = pageStyle({ ...prefs, upscaleToFit: true })
    expect(s.width).toBe('100%')
    expect(s.height).toBe('100%')
    expect(s.objectFit).toBe('contain')
  })

  // The exact declarations today's reader draws. Off must be byte-for-byte the
  // old behaviour, not merely similar, or turning the setting off is a new
  // third rendering nobody asked for. The `objectPosition` this now also emits
  // does not break that: unset, `object-position` *is* `50% 50%`, so the
  // default hug restates the value rather than changing it.
  it('is the old fit-whole exactly when upscaling is off', () => {
    const s = pageStyle({ ...prefs, upscaleToFit: false })
    expect(s.width).toBe('auto')
    expect(s.height).toBe('auto')
    expect(s.maxWidth).toBe('100%')
    expect(s.maxHeight).toBe('100%')
    expect(s.objectFit).toBe('contain')
  })

  it('keeps lines crisp on request and leaves the browser alone otherwise', () => {
    expect(pageStyle({ ...prefs, upscaleFilter: 'crisp' }).imageRendering).toBe(
      'pixelated',
    )
    expect(
      pageStyle({ ...prefs, upscaleFilter: 'smooth' }).imageRendering,
    ).toBeUndefined()
  })

  // A filter is only meaningful while something is being enlarged.
  it('does not ask for pixelation while upscaling is off', () => {
    expect(
      pageStyle({ ...prefs, upscaleToFit: false, upscaleFilter: 'crisp' })
        .imageRendering,
    ).toBeUndefined()
  })

  // The seam is closed here and nowhere else: with `upscaleToFit` on the
  // element already fills its cell, so only `object-position` can move the art
  // inside it. Named for the edge the art hugs, never for the cell — the left
  // page of a pair asks for `right-edge`.
  it('pulls the artwork to the edge it is asked for', () => {
    expect(pageStyle(prefs, 'right-edge').objectPosition).toBe('100% 50%')
    expect(pageStyle(prefs, 'left-edge').objectPosition).toBe('0% 50%')
  })

  // Single mode, scroll mode and every standalone `PageView` reach this
  // branch, and it has to be the centring they already drew.
  it('centres when nobody asks, and says so rather than omitting it', () => {
    expect(pageStyle(prefs).objectPosition).toBe('50% 50%')
    expect(pageStyle(prefs, 'centre')).toEqual(pageStyle(prefs))
  })

  // The hug is not a second upscale switch: it rides alongside whatever the
  // sizing rule decided, so a cell of a pair keeps hugging with the setting
  // off. Whether it *moves* anything then is a different question — with
  // `auto` sizing the element is its own content and there is no free space
  // to distribute — and that belongs to the browser, not to this output.
  it('hugs independently of the upscale switch', () => {
    expect(
      pageStyle({ ...prefs, upscaleToFit: false }, 'right-edge').objectPosition,
    ).toBe('100% 50%')
  })
})

describe('seamStyle', () => {
  const prefs = { ...fitModule.defaults }

  // This used to assert `justifyContent: 'flex-end'` and `'flex-start'` on the
  // two cells, which was the whole of what it checked and was inert in
  // `PageView`'s markup — a cell whose only child is `width: 100%` has no free
  // space to distribute, and neither `justify-content` on a non-flex box nor
  // one that does nothing is observable from a style object. So the assertion
  // passed for as long as the declaration existed and would have passed had it
  // done the opposite. What actually pushes the pages together is
  // `pageStyle`'s hug, asserted above; this pins that the cell has stopped
  // claiming to.
  it('leaves positioning the artwork to pageStyle', () => {
    for (const cell of ['first', 'second'] as const) {
      expect(seamStyle(prefs, cell).justifyContent).toBeUndefined()
      expect(seamStyle(prefs, cell).display).toBeUndefined()
    }
    expect(pageStyle(prefs, 'right-edge').objectPosition).toBe('100% 50%')
    expect(pageStyle(prefs, 'left-edge').objectPosition).toBe('0% 50%')
  })

  it('splits the gap between the two cells so the pair stays centred', () => {
    const g = { ...prefs, gap: 24 }
    expect(seamStyle(g, 'first').paddingRight).toBe('12px')
    expect(seamStyle(g, 'second').paddingLeft).toBe('12px')
  })

  it('adds no padding at all when the gap is zero', () => {
    expect(seamStyle(prefs, 'first').paddingRight).toBe('0px')
    expect(seamStyle(prefs, 'second').paddingLeft).toBe('0px')
  })

  it('refuses a negative gap rather than overlapping the pages', () => {
    expect(seamStyle({ ...prefs, gap: -10 }, 'first').paddingRight).toBe('0px')
  })
})
