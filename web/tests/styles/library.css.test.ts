import { describe, expect, it } from 'vitest'
import { coverSizes } from '../../src/library'
import { atRulesOf, valuesOf } from '../support/css'
import librarySource from '../../src/styles/library.css?raw'
import shellSource from '../../src/styles/shell.css?raw'

/**
 * A volume's panel opened on a Series page's last row ends the page with as
 * much room below it as the last row has with no panel (O283). Below 900
 * px, where the document scrolls rather than `main` (`shell.css`), the room
 * under the last row holds `.library`'s bottom safe-area inset as well as
 * `main`'s padding, so the panel's room adds the inset there and only there:
 * at 900 px and wider the inset lies outside the scroller. A headless
 * browser reports no inset, so only the stylesheet shows this; the browser
 * project's `Series.scrollbars.browser.test.tsx` measures the room itself.
 */
describe('library.css gives a last-row panel the room under it that the last row has', () => {
  it('sizes the room under a panel by the property both of main’s rules take their block-end padding from, adding the bottom safe-area inset only below 900 px', () => {
    const mains = atRulesOf(shellSource, '.library > main').map(
      ({ at, body }) => ({ at, end: valuesOf(body, 'padding-block-end') }),
    )
    // The fixture tells: `main`'s own rule takes its block-end padding from
    // a custom property.
    const room = mains[0]?.end[0] ?? ''
    expect(room).toMatch(/^var\(--[\w-]+\)$/)
    // Below 900 px too: that rule's `padding` shorthand would otherwise set
    // the block-end side apart from the property the panel reads.
    expect(mains, 'each of main’s rules’ block-end padding').toEqual([
      { at: '', end: [room] },
      { at: '@media (width < 900px)', end: [room] },
    ])

    expect(
      atRulesOf(librarySource, '.tile-panel::after').map(({ at, body }) => ({
        at,
        height: valuesOf(body, 'height'),
      })),
    ).toEqual([
      { at: '', height: [room] },
      {
        at: '@media (width < 900px)',
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
 * `sizes` cannot read CSS: the breakpoint below which a cover fills its
 * column, and the gap's 1rem cap, which its 32 px allowance holds. Either
 * moved here alone would have a cover between the two values ask for an
 * image smaller than it is drawn.
 */
describe('library.css and coverSizes agree', () => {
  it('fills the columns below the breakpoint coverSizes names, at a gap of at most 1rem', () => {
    const set = (property: string) =>
      atRulesOf(librarySource, '.library')
        .map(({ at, body }) => ({ at, values: valuesOf(body, property) }))
        .filter(({ values }) => values.length > 0)
    expect(set('--tile-fill')).toEqual([
      { at: '', values: ['0'] },
      { at: '@media (width < 900px)', values: ['1'] },
    ])
    const fill = (set('--tile-fill')[1]?.at ?? '').replace(/^@media /, '')
    for (const size of [96, 144, 240])
      expect(coverSizes(size).slice(0, fill.length + 1)).toBe(`${fill} `)

    const gaps = set('--tile-gap')
    expect(gaps.map(({ at }) => at)).toEqual([''])
    const gap = gaps[0]?.values[0] ?? ''
    expect(gap).toMatch(/^clamp\(/)
    expect(/,\s*([^,()]+?)\s*\)$/.exec(gap)?.[1]).toBe('1rem')
  })
})
