import { describe, expect, it } from 'vitest'
import { COVER_TALL } from '../../src/continueRow'
import { coverSizes } from '../../src/library'
import { PHONE_LAYOUT, PHONE_LAYOUT_PARTS } from '../../src/phoneLayout'
import { atRulesOf, rules, selectors, valuesOf } from '../support/css'
import librarySource from '../../src/styles/library.css?raw'
import shellSource from '../../src/styles/shell.css?raw'

/**
 * A volume's panel opened on a Series page's last row ends the page with as
 * much room below it as the last row has with no panel (O283). In the
 * phone layout, where the document scrolls rather than `main` (`shell.css`),
 * the room under the last row holds `.library`'s bottom safe-area inset as
 * well as `main`'s padding, so the panel's room adds the inset there and only
 * there: beside the sidebar the inset lies outside the scroller. A headless
 * browser reports no inset, so only the stylesheet shows this; the browser
 * project's `Series.scrollbars.browser.test.tsx` measures the room itself.
 */
describe('library.css gives a last-row panel the room under it that the last row has', () => {
  it('sizes the room under a panel by the property both of main’s rules take their block-end padding from, adding the bottom safe-area inset only in the phone layout', () => {
    const mains = atRulesOf(shellSource, '.library > main').map(
      ({ at, body }) => ({ at, end: valuesOf(body, 'padding-block-end') }),
    )
    // The fixture tells: `main`'s own rule takes its block-end padding from
    // a custom property.
    const room = mains[0]?.end[0] ?? ''
    expect(room).toMatch(/^var\(--[\w-]+\)$/)
    // In the phone layout too: that rule's `padding` shorthand would otherwise set
    // the block-end side apart from the property the panel reads.
    expect(mains, 'each of main’s rules’ block-end padding').toEqual([
      { at: '', end: [room] },
      { at: `@media ${PHONE_LAYOUT}`, end: [room] },
    ])

    expect(
      atRulesOf(librarySource, '.tile-panel::after').map(({ at, body }) => ({
        at,
        height: valuesOf(body, 'height'),
      })),
    ).toEqual([
      { at: '', height: [room] },
      {
        at: `@media ${PHONE_LAYOUT}`,
        height: [`calc(${room} + env(safe-area-inset-bottom, 0px))`],
      },
    ])
  })
})

// O286, spec goal 9: as the reader's and the account's failures are drawn.
// `MetadataEditor.browser.test.tsx` reads the colour the failure is given.
describe('library.css colours the Title form’s failure', () => {
  it('draws the Title form’s failure in --danger', () => {
    expect(
      atRulesOf(librarySource, '.metadata-editor .failure').map(
        ({ at, body }) => ({ at, color: valuesOf(body, 'color') }),
      ),
    ).toEqual([{ at: '', color: ['var(--danger)'] }])
  })
})

/**
 * `library.ts`'s `coverSizes` repeats two facts of this file, as an image's
 * `sizes` cannot read CSS: the layout in which a cover fills its column, and
 * the gap's 1rem cap, which its 32 px allowance holds. Either moved here
 * alone would have a cover between the two values ask for an image smaller
 * than it is drawn. `sizes` takes the condition as one entry per part, each
 * with the same width, since `or` there could not be checked in every
 * browser the app runs in.
 */
describe('library.css and coverSizes agree', () => {
  it('fills the columns in the phone layout, which coverSizes names part by part, at a gap of at most 1rem', () => {
    const set = (property: string) =>
      atRulesOf(librarySource, '.library')
        .map(({ at, body }) => ({ at, values: valuesOf(body, property) }))
        .filter(({ values }) => values.length > 0)
    expect(set('--tile-fill')).toEqual([
      { at: '', values: ['0'] },
      { at: `@media ${PHONE_LAYOUT}`, values: ['1'] },
    ])
    for (const size of [96, 144, 240]) {
      const entries = coverSizes(size).split(', ')
      expect(entries.pop(), `${size}: the bare size, last`).toBe(`${size}px`)
      const named = entries.map((entry) => ({
        condition: entry.slice(0, entry.lastIndexOf(' ')),
        width: entry.slice(entry.lastIndexOf(' ') + 1),
      }))
      expect(named.map(({ condition }) => condition)).toEqual([
        ...PHONE_LAYOUT_PARTS,
      ])
      const filled = named[0]?.width ?? ''
      expect(filled).toMatch(/^\d+px$/)
      for (const { width } of named) expect(width).toBe(filled)
    }

    const gaps = set('--tile-gap')
    expect(gaps.map(({ at }) => at)).toEqual([''])
    const gap = gaps[0]?.values[0] ?? ''
    expect(gap).toMatch(/^clamp\(/)
    expect(/,\s*([^,()]+?)\s*\)$/.exec(gap)?.[1]).toBe('1rem')
  })
})

/**
 * `continueRow.ts`'s `COVER_TALL` repeats the Continue card's cover ratio,
 * as the row finds a cover's foot from its column's width with no cover
 * read. Were the ratio moved here alone, the row would judge a slide by a
 * foot the cover does not have.
 */
describe('library.css and COVER_TALL agree', () => {
  it('draws a Continue card’s cover, and the placeholder in its place, COVER_TALL times as tall as wide', () => {
    for (const cover of ['.continue img', '.continue .cover-missing']) {
      const preludes = new Set(
        rules(librarySource)
          .map(({ prelude }) => prelude)
          .filter((prelude) => selectors(prelude).includes(cover)),
      )
      const ratios = [...preludes]
        .flatMap((prelude) => atRulesOf(librarySource, prelude))
        .flatMap(({ at, body }) =>
          valuesOf(body, 'aspect-ratio').map((ratio) => ({ at, ratio })),
        )
      expect(ratios, `${cover}: one ratio, in every layout`).toEqual([
        { at: '', ratio: expect.stringMatching(/^\d+ \/ \d+$/) },
      ])
      const [wide, tall] = (ratios[0]?.ratio ?? '').split('/').map(Number)
      expect(COVER_TALL, cover).toBe(tall / wide)
    }
  })
})
