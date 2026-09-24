import { describe, expect, it } from 'vitest'
import indexHtml from '../../index.html?raw'
import { STRIP_BREAKPOINT_PX, STRIP_TILE_PX } from '../tileSize'
import { rules, selectors, withoutComments } from '../testing/css'
import readerSource from './reader.css?raw'
import shellSource from './shell.css?raw'

/**
 * The reader's frame, read from its stylesheet (spec 2026-09-17 §8.2). jsdom
 * renders none of this, and the browser project measures what it draws
 * (`App.browser.test.tsx`); what is pinned here is what the rules declare.
 */

/** The bodies of every rule whose prelude is exactly `prelude`. */
const bodies = (css: string, prelude: string) =>
  rules(css)
    .filter((rule) => rule.prelude === prelude)
    .map((rule) => rule.body)

/**
 * Every value `property` is given in those rules, with all whitespace taken
 * out, so the check does not depend on how Biome wraps a long `calc()`.
 */
function declared(css: string, prelude: string, property: string): string[] {
  return bodies(css, prelude).flatMap((body) =>
    [
      ...body.matchAll(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`, 'g')),
    ].map((m) => m[1].replace(/\s+/g, '')),
  )
}

describe('the reader root', () => {
  it('is dark in both themes, on the black ground', () => {
    expect(declared(readerSource, '.reader', 'color-scheme')).toEqual(['dark'])
    expect(declared(readerSource, '.reader', 'background')).toEqual([
      'var(--reader-ground)',
    ])
    expect(declared(readerSource, '.reader', 'color')).toEqual(['var(--text)'])
  })

  it('blacks out the document behind it, from the route box as well as the root', () => {
    const ground = rules(readerSource).find(
      (rule) =>
        rule.body.includes('var(--reader-ground)') &&
        rule.prelude !== '.reader',
    )
    expect(ground && selectors(ground.prelude)).toEqual([
      ':root:has(.reader-route, .reader)',
      ':root:has(.reader-route, .reader) body',
    ])
  })

  it('stands the strip’s tiles at 160 px and 96 below the breakpoint, with a matching starting --strip-unit', () => {
    // Built from `tileSize.ts`'s constants rather than written out again:
    // the filmstrip asks the server for a rung derived from those two numbers
    // during its first render, before there is an element to measure, so this
    // assertion is what keeps the restatement honest — O134.
    expect(declared(readerSource, '.reader', '--strip-tile')).toEqual([
      `${STRIP_TILE_PX.wide}px`,
      `${STRIP_TILE_PX.narrow}px`,
    ])
    expect(declared(readerSource, '.reader', '--strip-unit')).toEqual([
      '97px',
      '54px',
    ])
    // Anchored to the phone breakpoint's own `.reader` block, not a search of
    // the whole file — but not exclusive: `--strip-unit` lives in the same
    // block (O140), and this must not regress the moment it does.
    expect(withoutComments(readerSource)).toMatch(
      new RegExp(
        `@media \\(width < ${STRIP_BREAKPOINT_PX}px\\)\\s*\\{\\s*\\.reader\\s*\\{[^}]*--strip-tile:\\s*${STRIP_TILE_PX.narrow}px;`,
      ),
    )
  })

  it('docks panels between the bars from the safe areas and the bars’ heights', () => {
    expect(declared(readerSource, '.reader', '--panel-top')).toEqual([
      'calc(var(--safe-top)+var(--reader-top-bar)+var(--space-4))',
    ])
    expect(declared(readerSource, '.reader', '--panel-bottom')).toEqual([
      'calc(var(--safe-bottom)+var(--reader-bottom-bar)+var(--space-4))',
    ])
    for (const edge of ['top', 'right', 'bottom', 'left']) {
      expect(declared(readerSource, '.reader', `--safe-${edge}`)).toEqual([
        `env(safe-area-inset-${edge},0px)`,
      ])
    }
  })
})

describe('the dialog panel’s controls clear the iOS zoom floor', () => {
  it('gives them a coarse-pointer font-size of at least 16px', () => {
    // O151, confirmed on the reporter's iPhone 2026-09-23. `base.css` carries
    // this floor for `:where(.library, .login)` because a control under 16px
    // makes iOS Safari zoom the document on focus and the zoom outlives the
    // field. **The reader is neither**, so the panel — `--text-ui`, 14px,
    // passed to every control by its own `font: inherit` — kept doing exactly
    // what that floor exists to stop, to the `type="number"` boxes
    // `settingsFields.tsx` puts in Settings.
    const floor = declared(
      readerSource,
      '.reader-dialog-panel :where(input, select, textarea)',
      'font-size',
    )
    expect(floor).toEqual(['max(16px,var(--text-ui))'])
  })

  it('writes that floor after the rule it corrects, so the tie goes to it', () => {
    // Both are `.reader-dialog-panel :where(…)` — (0,1,0) each — so nothing
    // but source order decides which wins, and a companion pasted above the
    // `font: inherit` it overrides would lose on every pointer.
    //
    // **Located by the rule, not by the first `@media (pointer: coarse)`.**
    // This file has three of those blocks and the first is 200 lines above
    // the one that matters; an `indexOf` on the at-rule passed against a
    // correct stylesheet and failed against this one, which is how the first
    // version of this assertion was written and why it is not.
    const inherits = readerSource.indexOf(
      '.reader-dialog-panel :where(input, select) {',
    )
    const floored = readerSource.indexOf(
      '.reader-dialog-panel :where(input, select, textarea) {',
    )
    expect(inherits).toBeGreaterThan(-1)
    expect(floored).toBeGreaterThan(inherits)
  })
})

describe('the safe areas', () => {
  it('are there to pad: index.html lets the page run under the notch', () => {
    expect(indexHtml).toMatch(
      /<meta name="viewport" content="width=device-width, initial-scale=1\.0, viewport-fit=cover" \/>/,
    )
  })

  it('pad the library’s frame, its phone bar and its drawer', () => {
    /** Every rule for `prelude`, joined, with whitespace taken out. */
    const padding = (prelude: string) =>
      bodies(shellSource, prelude).join(';').replace(/\s+/g, '')
    expect(padding('.library')).toContain('env(safe-area-inset-left,0px)')
    expect(padding('.library')).toContain('env(safe-area-inset-right,0px)')
    expect(padding('.library')).toContain('env(safe-area-inset-bottom,0px)')
    expect(padding('.phone-bar')).toContain('env(safe-area-inset-top,0px)')
    expect(padding('.sidebar-column')).toContain(
      'env(safe-area-inset-left,0px)',
    )
    expect(padding('.menu-close')).toContain('env(safe-area-inset-top,0px)')
  })
})

describe('the panels (spec §8.5)', () => {
  it('dock between the bars, whether or not the bars are shown', () => {
    expect(declared(readerSource, '.docked-panel', 'top')).toEqual([
      'var(--panel-top)',
    ])
    expect(declared(readerSource, '.docked-panel', 'max-height')).toEqual([
      'max(8rem,calc(100%-var(--panel-top)-var(--panel-bottom)))',
    ])
  })

  for (const panel of ['.docked-panel', '.reader-dialog-panel']) {
    it(`draw ${panel} on the reader's panel, with a hairline, square and unshadowed`, () => {
      expect(declared(readerSource, panel, 'background')).toEqual([
        'var(--reader-panel)',
      ])
      expect(declared(readerSource, panel, 'border')).toEqual([
        '1pxsolidvar(--reader-hair)',
      ])
      expect(declared(readerSource, panel, 'border-radius')).toEqual([])
      expect(declared(readerSource, panel, 'box-shadow')).toEqual([])
    })
  }

  it('ring the chosen note colour in the panel, then the text', () => {
    expect(
      declared(
        readerSource,
        '.reader-swatch[aria-pressed="true"]',
        'box-shadow',
      ),
    ).toEqual(['0002pxvar(--bg),0004pxvar(--text)'])
  })
})

describe('the bars’ and the borrowed mark’s colours', () => {
  it('pin each to the token it declares', () => {
    expect(declared(readerSource, '.reader-top-bar', 'background')).toEqual([
      'var(--reader-panel)',
    ])
    expect(declared(readerSource, '.reader-top-bar', 'color')).toEqual([
      'var(--text)',
    ])
    expect(declared(readerSource, '.reader-bottom-bar', 'background')).toEqual([
      'var(--reader-panel)',
    ])
    expect(declared(readerSource, '.reader-bottom-bar', 'color')).toEqual([
      'var(--text)',
    ])
    expect(declared(readerSource, '.reader-progress', 'background')).toEqual([
      'var(--line)',
    ])
    expect(
      declared(readerSource, '.reader-progress-fill', 'background'),
    ).toEqual(['var(--accent)'])
    expect(declared(readerSource, '.reader-readout', 'color')).toEqual([
      'var(--muted)',
    ])
    expect(declared(readerSource, '.borrowed-mark', 'background')).toEqual([
      'var(--warning)',
    ])
    expect(declared(readerSource, '.borrowed-mark', 'color')).toEqual([
      'var(--bg)',
    ])
  })
})
