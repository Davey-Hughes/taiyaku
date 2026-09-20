import { describe, expect, it } from 'vitest'
import indexHtml from '../../index.html?raw'
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

  it('stands the strip’s tiles at 160 px, and 96 below the breakpoint', () => {
    expect(declared(readerSource, '.reader', '--strip-tile')).toEqual([
      '160px',
      '96px',
    ])
    expect(withoutComments(readerSource)).toMatch(
      /@media \(width < 900px\)\s*\{\s*\.reader\s*\{\s*--strip-tile:\s*96px;\s*\}\s*\}/,
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
