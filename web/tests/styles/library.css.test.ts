import { describe, expect, it } from 'vitest'
import { atRulesOf } from '../support/css'
import librarySource from '../../src/styles/library.css?raw'
import shellSource from '../../src/styles/shell.css?raw'

/** Each value `body` declares for `property`, in order, spaces collapsed. */
const valuesOf = (body: string, property: string) =>
  [
    ...body.matchAll(
      new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]*?)\\s*(?=;|$)`, 'g'),
    ),
  ].map((m) => m[1].replace(/\s+/g, ' '))

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
