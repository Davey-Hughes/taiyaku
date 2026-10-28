import { describe, expect, it } from 'vitest'
import indexHtml from '../../index.html?raw'
import { STRIP_BREAKPOINT_PX, STRIP_TILE_PX } from '../../src/tileSize'
import { rules, selectors, withoutComments } from '../support/css'
import readerSource from '../../src/styles/reader.css?raw'
import shellSource from '../../src/styles/shell.css?raw'

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
      '107px',
      '64px',
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
  /** `declared`, but tolerant of how Biome wraps a two-prelude selector list. */
  const flat = (css: string, prelude: string, property: string) =>
    rules(css)
      .filter((rule) => rule.prelude.replace(/\s+/g, ' ').trim() === prelude)
      .flatMap((rule) =>
        [
          ...rule.body.matchAll(
            new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`, 'g'),
          ),
        ].map((m) => m[1].replace(/\s+/g, '')),
      )

  it('gives them a coarse-pointer font-size of at least 16px', () => {
    // O151, confirmed on the reporter's iPhone 2026-09-23. `base.css` carries
    // this floor for `:where(.library, .login)` because a control under 16px
    // makes iOS Safari zoom the document on focus and the zoom outlives the
    // field. **The reader is neither**, so the panel — `--text-ui`, 14px,
    // passed to every control by its own `font: inherit` — kept doing exactly
    // what that floor exists to stop, to the `type="number"` boxes
    // `settingsFields.tsx` puts in Settings.
    // Both preludes, because the floor has two and they do different jobs:
    // the `:where()` one is the catch-all at (0,1,0), and the
    // `.reader-field` one is (0,2,0) so it beats that class's own
    // `font: inherit`.
    expect(
      flat(
        readerSource,
        '.reader-dialog-panel :where(input, select, textarea), .reader-dialog-panel .reader-field',
        'font-size',
      ),
    ).toEqual(['max(16px,var(--text-ui))'])
  })

  it('names .reader-field in that floor, so it wins on specificity not order', () => {
    // **This assertion replaces one that asserted the opposite mechanism**,
    // and the replacement is the whole finding. The floor used to be a
    // single (0,1,0) prelude that won its tie by sitting after the
    // `font: inherit` it corrects — and the old case here pinned exactly
    // that source order. ⑤c then put the Account panel's controls on
    // `.reader-field`, whose own `font: inherit` is also (0,1,0) and sits
    // ~100 lines *later*, so it took the tie and the panel's three text
    // boxes rendered at 14px under a coarse pointer — measured in Chromium
    // at 390x844 with `hasTouch`, against the built stylesheet.
    //
    // A floor that depends on being last is a floor the next rule removes
    // silently, so what is pinned now is that the floor *names* the class
    // at a specificity that does not care where either rule sits.
    const floored = rules(readerSource).find((rule) =>
      rule.prelude
        .replace(/\s+/g, ' ')
        .includes('.reader-dialog-panel .reader-field'),
    )
    expect(floored?.body).toMatch(/font-size:\s*max\(16px/)

    // And the rule it has to beat is still there and still (0,1,0), so the
    // specificity above is doing real work rather than describing a pair
    // that could not collide. Anchored to the line start: `.reader-field {`
    // is also the tail of the floor's own second prelude, which an
    // unanchored `indexOf` finds first.
    const inherit = readerSource.search(/^\.reader-field \{/m)
    expect(inherit).toBeGreaterThan(-1)
    expect(readerSource.slice(inherit, inherit + 400)).toMatch(
      /font:\s*inherit/,
    )

    // The floor also sits *after* it now, which is belt and braces rather
    // than the mechanism: Biome's `noDescendingSpecificity` wants the
    // higher-specificity rule later, and taking the suppression instead
    // would have left the file one edit away from the original defect.
    expect(
      readerSource.indexOf('.reader-dialog-panel .reader-field'),
    ).toBeGreaterThan(inherit)
  })
})

describe('the panel’s fields clear the 44px floor too', () => {
  it('puts .reader-field in the rule .reader-button has had since ③', () => {
    // §7.1's 44px row. Not a new need: the note body has worn
    // `.reader-field` since ③ and ④'s touch pass did not pick it up, so it
    // sat at its desktop height under a finger for two style passes; ⑤c's
    // account panel adds three more boxes to it. One rule rather than two,
    // so the next edit to the floor cannot reach one and miss the other.
    //
    // **What this cannot see**, stated rather than left to be discovered:
    // `rules()` discards which `@media` a rule came from, so a floor lifted
    // out of the coarse block — giving a mouse the same 44px — passes here.
    // The three-block shape of this file is why an `indexOf` on the at-rule
    // is not the fix; the describe above records what that cost once.
    expect(
      declared(readerSource, '.reader-button, .reader-field', 'min-height'),
    ).toEqual(['44px'])
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
    expect(
      declared(readerSource, '.reader-progress-shown', 'background'),
    ).toEqual(['var(--reader-mark)'])
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

  /**
   * O216. The mark on the progress line is the line's child and stands a
   * pixel proud of it above and below (`Chrome.tsx`'s `markStyle`). A line
   * that clipped what overflows it would cut both rows off and leave a mark
   * no taller than the line. The browser project reads the rows as they are
   * drawn (`Chrome.browser.test.tsx`), and CI does not run it. What is held
   * here is narrower: that the line's rule declares none of the properties
   * listed below.
   */
  it('lets the mark stand proud of the progress line, whose rule declares no overflow, clip-path or contain', () => {
    // THE CONTROL: the rule is found, so an empty answer below is its own.
    expect(bodies(readerSource, '.reader-progress')).toHaveLength(1)
    for (const property of [
      'overflow',
      'overflow-x',
      'overflow-y',
      'overflow-block',
      'overflow-inline',
      'clip-path',
      'contain',
    ]) {
      expect(
        declared(readerSource, '.reader-progress', property),
        property,
      ).toEqual([])
    }
  })

  /**
   * **The one declaration on this stylesheet that CI cannot otherwise see.**
   * `.reader-section-head` carries no margin — its only ③ user draws one row
   * per section — and ⑤c's account panel stacks it, so the separation lives
   * on a second class beside it. The browser project measures the rendered
   * 8 px, but `scripts/check-all.sh` runs that project only when a local
   * Chromium is present and says in its own comment that it is not a CI job.
   * What CI does run asserts the class is *applied*, not that it *declares*
   * anything — which is exactly the shape of the defect this rule fixes: a
   * declaration silently absent while every class name was right.
   */
  it('gives a stacked panel row its separation', () => {
    expect(
      declared(readerSource, '.reader-section-head-stacked', 'margin-top'),
    ).toEqual(['var(--space-2)'])
  })
})

describe('the strip’s rail', () => {
  /**
   * O138. `Filmstrip.test.tsx` holds that the rail wears the class and
   * `Filmstrip.browser.test.tsx` that the first rule reaches it. The second
   * rule is held here and nowhere else.
   */
  it('draws no scrollbar, by the standard property and by WebKit’s own', () => {
    expect(
      declared(readerSource, '.reader-strip-rail', 'scrollbar-width'),
    ).toEqual(['none'])
    expect(
      declared(
        readerSource,
        '.reader-strip-rail::-webkit-scrollbar',
        'display',
      ),
    ).toEqual(['none'])
  })

  /**
   * `declared` finds a rule by its prelude and cannot see an enclosing
   * at-rule, so either rule moved inside `@media (width < 900px)` would still
   * be found, and the scrollbar would come back above that width. A rule at
   * the top level starts a line.
   */
  it('stands at the top level of the stylesheet, not inside an at-rule', () => {
    const source = withoutComments(readerSource)
    expect(source).toMatch(/^\.reader-strip-rail \{/m)
    expect(source).toMatch(/^\.reader-strip-rail::-webkit-scrollbar \{/m)
  })
})
