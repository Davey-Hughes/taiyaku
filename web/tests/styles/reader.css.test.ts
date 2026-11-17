import { describe, expect, it } from 'vitest'
import indexHtml from '../../index.html?raw'
import { PHONE_LAYOUT } from '../../src/phoneLayout'
import { STRIP_TILE_PX } from '../../src/tileSize'
import {
  atRulesOf,
  rules,
  selectors,
  valuesOf,
  withoutComments,
} from '../support/css'
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

  it('stands the strip’s tiles at 160 px and 96 in the phone layout, with a matching starting --strip-unit', () => {
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
    // Anchored to the phone layout's own `.reader` block, not a search of
    // the whole file — but not exclusive: `--strip-unit` lives in the same
    // block (O140), and this must hold with it there.
    expect(withoutComments(readerSource)).toMatch(
      new RegExp(
        `@media ${PHONE_LAYOUT.replace(/[()]/g, '\\$&')}\\s*\\{\\s*\\.reader\\s*\\{[^}]*--strip-tile:\\s*${STRIP_TILE_PX.narrow}px;`,
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
    // O151. `base.css` carries this floor for `:where(.library, .login)`
    // because a control under 16px makes iOS Safari zoom the document on focus
    // and the zoom outlives the field. **The reader is neither**, so the
    // panel — `--text-ui`, 14px, passed to every control by its own
    // `font: inherit` — would do exactly what that floor exists to stop, to
    // the `type="number"` boxes `settingsFields.tsx` puts in Settings.
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
    // The floor names `.reader-field` so that it wins on specificity, not on
    // source order. A floor at (0,1,0), the same as `.reader-field`'s own
    // `font: inherit`, would win that tie only by sitting after it, and the
    // panel's text boxes would render at 14px under a coarse pointer the
    // moment it did not.
    //
    // A floor that depends on being last is a floor the next rule removes
    // silently, so what is pinned is that the floor *names* the class at a
    // specificity that does not care where either rule sits.
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

    // The floor also sits *after* it, which is belt and braces rather than
    // the mechanism: Biome's `noDescendingSpecificity` wants the
    // higher-specificity rule later, and taking the suppression instead would
    // leave the file one edit away from relying on source order.
    expect(
      readerSource.indexOf('.reader-dialog-panel .reader-field'),
    ).toBeGreaterThan(inherit)
  })
})

describe('the docked panels’ controls clear the iOS zoom floor too', () => {
  it('gives the docked panels’ controls a coarse-pointer font-size of at least 16px (O171)', () => {
    // The note editor's text box is in a docked panel, not a dialog, so the
    // dialog floor above never reaches it. Its own rule, rather than a wider
    // dialog-floor selector, because the case above pins that prelude
    // exactly. `rules()` drops the `@media`, so whether this sits inside
    // the coarse block is held by `App.browser.test.tsx`'s fine-pointer case.
    expect(
      flat(
        readerSource,
        '.docked-panel :where(input, select, textarea), .docked-panel .reader-field',
        'font-size',
      ),
    ).toEqual(['max(16px,var(--text-ui))'])
  })
})

describe('the panel’s fields clear the 44px floor too', () => {
  it('puts .reader-field in the rule .reader-button has had since ③', () => {
    // §7.1's 44px row. The note body wears `.reader-field`, and so do the
    // account panel's boxes. One rule rather than two, so the next edit to
    // the floor cannot reach one and miss the other.
    //
    // **What this cannot see**, stated rather than left to be discovered:
    // `rules()` discards which `@media` a rule came from, so a floor lifted
    // out of the coarse block — giving a mouse the same 44px — passes here.
    // This file's several coarse blocks are why an `indexOf` on the at-rule
    // is not the fix.
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
   * **The one declaration on this stylesheet that CI cannot otherwise see.**
   * `.reader-section-head` carries no margin — its only other user draws one
   * row per section — and the account panel stacks it, so the separation
   * lives on a second class beside it. The browser project measures the
   * rendered separation, but `scripts/check-all.sh` runs that project only
   * when a local Chromium is present and says in its own comment that it is
   * not a CI job. What CI does run asserts the class is *applied*, not that
   * it *declares* anything, so a declaration silently absent while every
   * class name is right would pass.
   */
  it('gives a stacked panel row its separation', () => {
    expect(
      declared(readerSource, '.reader-section-head-stacked', 'margin-top'),
    ).toEqual(['var(--space-2)'])
  })
})

describe('the progress line and its mark (O216, O286)', () => {
  /**
   * Every rule for the line itself, at the top level or under an at-rule:
   * `.reader-progress` and its `:where()` forms, and not its fill's, its
   * mark's or its band's. Asserts that the line's rule at the top level and
   * the one that thickens it are among them, so an empty answer read from
   * these is theirs.
   */
  function theLinesRules() {
    const preludes = new Set(
      rules(readerSource)
        .map(({ prelude }) => prelude)
        .filter((prelude) => /^\.reader-progress(?![\w-])/.test(prelude)),
    )
    const found = [...preludes].flatMap((prelude) =>
      atRulesOf(readerSource, prelude).map((rule) => ({ prelude, ...rule })),
    )
    // THE CONTROL.
    const lines = found.filter(({ prelude }) => prelude === '.reader-progress')
    expect(lines.map(({ at }) => at)).toContain('')
    expect(lines.map(({ at }) => at)).toContain('@media (any-pointer: fine)')
    return found
  }

  /**
   * O286. The band that takes a mouse's press and the line's wake are there
   * together wherever the device has a fine pointer, a touch laptop's
   * trackpad too (spec goal 6). A second condition on either, such as
   * `(hover: hover)`, would part them on some device: a band taking presses
   * on a line that never wakes. `Chrome.test.tsx` holds that the wake asks
   * the same condition of `matchMedia`.
   */
  it('asks one pointer condition of the progress line, `(any-pointer: fine)`', () => {
    const FINE = '@media (any-pointer: fine)'
    // Each rule the condition gates, and a declaration only its gated form
    // makes.
    const gated: Array<[string, RegExp]> = [
      ['.reader-progress-grip', /(?:^|;)\s*display:\s*block/],
      ['.reader-progress', /(?:^|;)\s*box-shadow:/],
      ['.reader-progress:where([data-awake], [data-held])', /--grow:\s*1/],
      ['.reader-progress:where([data-still])', /transition:\s*none/],
      ['.reader-progress-fill', /(?:^|;)\s*top:[^;]*var\(--grow\)/],
      ['.reader-progress-shown', /(?:^|;)\s*top:[^;]*var\(--grow\)/],
    ]
    for (const [prelude, declaration] of gated) {
      const found = atRulesOf(readerSource, prelude)
      const asked = found
        .map(({ at }) => at)
        .filter((at) => /pointer|hover/.test(at))
      expect(new Set(asked), prelude).toEqual(new Set([FINE]))
      // THE CONTROL: the gated rule itself is the one found under it.
      expect(
        found.some(({ at, body }) => at === FINE && declaration.test(body)),
        prelude,
      ).toBe(true)
    }
  })

  /**
   * O286, spec goal 6. Where no pointer is fine there is no band: its top
   * 4 px lie over the rail's padding, where a phone's finger starts a swipe
   * of the strip. Hidden first and drawn after, so the fine block, of the
   * same weight, wins where it applies. The browser project finds no band
   * on a phone, and CI does not run it.
   */
  it('draws the press band only where some pointer is fine', () => {
    const FINE = '@media (any-pointer: fine)'
    const grip = atRulesOf(readerSource, '.reader-progress-grip')
    const displays = grip.map(({ at, body }) => ({
      at,
      display: valuesOf(body, 'display'),
    }))
    expect(displays).toEqual([
      { at: '', display: ['none'] },
      { at: FINE, display: ['block'] },
    ])
  })

  /**
   * O216. The mark is the line's child and stands proud of it above and
   * below, further while the line is awake (O286). A line that clipped what
   * overflows it would cut both rows off and leave a mark no taller than the
   * line. The browser project reads the rows as they are drawn, and CI does
   * not run it; what is held here is that no rule for the line declares any
   * of the properties below, whether at the top level or under an at-rule.
   */
  it('no rule for the progress line, under any at-rule, declares overflow, clip-path or contain', () => {
    const found = theLinesRules()
    for (const { prelude, at, body } of found) {
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
          valuesOf(body, property),
          `${property} in ${prelude} ${at || 'at the top level'}`,
        ).toEqual([])
      }
    }
  })

  /**
   * O286, spec goal 8. Only the fill's leading end is round; the grey line
   * stays square, awake or asleep. A radius on the line would round it and
   * both the shadows that thicken it. The browser project reads only the
   * fill's corners, and CI does not run it, so this is what holds the
   * line's.
   */
  it('draws the grey line square: no rule for it, under any at-rule, declares a radius', () => {
    const found = theLinesRules()
    for (const { prelude, at, body } of found) {
      for (const property of [
        'border-radius',
        'border-top-left-radius',
        'border-top-right-radius',
        'border-bottom-right-radius',
        'border-bottom-left-radius',
        'border-start-start-radius',
        'border-start-end-radius',
        'border-end-start-radius',
        'border-end-end-radius',
      ]) {
        expect(
          valuesOf(body, property),
          `${property} in ${prelude} ${at || 'at the top level'}`,
        ).toEqual([])
      }
    }
  })

  /**
   * O286. One registered number eases, and the line's shadows, the fill's
   * and the mark's insets and their corners are worked out from it, so the
   * three cannot grow out of step. A size given by a rule of its own for the
   * awake line would jump while the number eased. The browser project reads
   * the sizes it gives partway through a wake
   * (`Chrome.look.browser.test.tsx`).
   */
  it('registers one number, `--grow`, that the line, its fill and the mark are drawn from', () => {
    const registered = atRulesOf(readerSource, '@property --grow')
    expect(
      registered.map(({ at, body }) => ({
        at,
        syntax: valuesOf(body, 'syntax'),
        inherits: valuesOf(body, 'inherits'),
        initial: valuesOf(body, 'initial-value'),
      })),
    ).toEqual([
      { at: '', syntax: ['"<number>"'], inherits: ['true'], initial: ['0'] },
    ])

    const FINE = '@media (any-pointer: fine)'
    const drawnFrom: Array<[string, string]> = [
      ['.reader-progress', 'box-shadow'],
      ['.reader-progress-fill', 'top'],
      ['.reader-progress-fill', 'bottom'],
      ['.reader-progress-fill', 'border-radius'],
      ['.reader-progress-shown', 'top'],
      ['.reader-progress-shown', 'bottom'],
      ['.reader-progress-shown', 'border-radius'],
    ]
    for (const [prelude, property] of drawnFrom) {
      const values = atRulesOf(readerSource, prelude)
        .filter(({ at }) => at === FINE)
        .flatMap(({ body }) => valuesOf(body, property))
      expect(values, `${prelude} ${property}`).toHaveLength(1)
      expect(values[0], `${prelude} ${property}`).toContain('var(--grow)')
    }
  })

  /**
   * O286, spec goal 7. Quick to wake and slower to sleep, as an overlay
   * scrollbar is. The rules that stop the easing are of the awake rule's
   * weight (`:where()`), so the still one has to come after it in the same
   * block to win, and the reduced-motion block after that block. The
   * reduced-motion block asks nothing of the pointer, so no device escapes
   * it.
   */
  it('eases a wake over 150 ms out and a sleep over 300 ms in and out, and none when still or reduced', () => {
    const AWAKE = '.reader-progress:where([data-awake], [data-held])'
    const STILL = '.reader-progress:where([data-still])'
    const blocks = atRulesOf(readerSource, '@media (any-pointer: fine)').map(
      ({ body }) => rules(body),
    )
    const block = blocks.find((list) =>
      list.some(({ prelude }) => prelude === AWAKE),
    )
    // THE CONTROL: the block that wakes the line is found.
    expect(block, 'the block holding the awake rule').toBeDefined()
    const eased = (block ?? []).flatMap(({ prelude, body }) =>
      valuesOf(body, 'transition').map((value) => ({
        prelude,
        transition: value.replace(/\s+/g, ''),
      })),
    )
    // In the block's order, so the still rule is seen to come last.
    expect(eased).toEqual([
      { prelude: '.reader-progress', transition: '--grow300msease-in-out' },
      { prelude: AWAKE, transition: '--grow150msease-out' },
      { prelude: STILL, transition: 'none' },
    ])

    const reduced = atRulesOf(readerSource, '.reader-progress').filter(
      ({ at }) => /prefers-reduced-motion/.test(at),
    )
    expect(
      reduced.map(({ at, body }) => ({
        at,
        transition: valuesOf(body, 'transition'),
      })),
    ).toEqual([
      { at: '@media (prefers-reduced-motion: reduce)', transition: ['none'] },
    ])
    // In the stylesheet's order: after the line's easing rule, and so, in a
    // block of its own (above), after the awake rule's whole block too.
    expect(
      atRulesOf(readerSource, '.reader-progress')
        .filter(({ body }) => valuesOf(body, 'transition').length > 0)
        .map(({ at }) => at),
    ).toEqual([
      '@media (any-pointer: fine)',
      '@media (prefers-reduced-motion: reduce)',
    ])
  })

  // O286: the insets are the stylesheet's and not `Chrome.tsx`'s, so the
  // rule that thickens the line can grow them; this is the pixel at rest.
  it('draws the mark one pixel proud of the line at rest', () => {
    const atTheTop = atRulesOf(readerSource, '.reader-progress-shown').filter(
      ({ at }) => at === '',
    )
    expect(atTheTop).toHaveLength(1)
    const [{ body }] = atTheTop
    expect([valuesOf(body, 'top'), valuesOf(body, 'bottom')]).toEqual([
      ['-1px'],
      ['-1px'],
    ])
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
   * at-rule, so either rule moved inside the phone layout's `@media` would
   * still be found, and the scrollbar would come back beside the sidebar. A
   * rule at the top level starts a line.
   */
  it('stands at the top level of the stylesheet, not inside an at-rule', () => {
    const source = withoutComments(readerSource)
    expect(source).toMatch(/^\.reader-strip-rail \{/m)
    expect(source).toMatch(/^\.reader-strip-rail::-webkit-scrollbar \{/m)
  })
})
