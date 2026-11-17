import { describe, expect, it } from 'vitest'
import { PHONE_LAYOUT } from '../../src/phoneLayout'
import { atRulesOf, rules, selectors } from '../support/css'
import adminSource from '../../src/styles/admin.css?raw'
import settingsSource from '../../src/styles/settings.css?raw'
import shellSource from '../../src/styles/shell.css?raw'

/**
 * `useFitsPair` measures the split's width, and a scrollbar that came with
 * the pair and went with the page alone would change that width with the
 * layout it chose, flipping the two every frame within a scrollbar's width
 * of the breakpoint. A gutter kept whether or not the scroller scrolls holds
 * the width still, and every Configuration screen keeps the same gutter, so
 * that going between them moves nothing. That the layout then holds is a
 * real browser's to show; this holds the rules there.
 */
describe('settings.css holds a split still near its breakpoint', () => {
  it('keeps the gutter of each scroller a split or a Configuration screen is drawn in', () => {
    const gutters = rules(settingsSource)
      .filter(({ body }) =>
        /(?:^|;)\s*scrollbar-gutter:\s*stable\s*(?:;|$)/.test(body),
      )
      .map(({ prelude }) => prelude)
    expect(gutters).toEqual([
      '.library > main:has(.config, .split)',
      ':root:has(.config, .split)',
    ])
  })

  it('keeps the document’s gutter in shell.css’s phone layout, where the document scrolls and main does not', () => {
    expect(
      atRulesOf(settingsSource, ':root:has(.config, .split)').map(
        ({ at }) => at,
      ),
    ).toEqual([`@media ${PHONE_LAYOUT}`])
    // Where shell.css stops `main` scrolling, so the document scrolls instead.
    expect(
      atRulesOf(shellSource, '.library > main')
        .filter(({ body }) =>
          /(?:^|;)\s*overflow-y:\s*visible\s*(?:;|$)/.test(body),
        )
        .map(({ at }) => at),
    ).toEqual([`@media ${PHONE_LAYOUT}`])
  })
})

/**
 * The rules that keep a library page's lines inside it at a phone's width,
 * and everything around its title where it was while the title is edited:
 * without them a report's path with nowhere to break runs past its box and
 * scrolls the page sideways, and the box that edits the name, the buttons
 * that take Rename…'s place, or a refusal of the new name, would move the
 * header's lines and buttons and everything beneath it. This reads only
 * the stylesheet, which is what CI runs. It cannot see whether a selector
 * still matches the page's markup, nor what the rule then draws: the
 * browser project's `Libraries.shell.browser.test.tsx` measures each.
 */
describe('settings.css declares the rules for a library page’s long path and its title edited in place', () => {
  /** The declarations of every rule whose selector list holds `prelude`. */
  const bodyOf = (prelude: string) =>
    rules(settingsSource)
      .filter((rule) => selectors(rule.prelude).includes(prelude))
      .map(({ body }) => body)
      .join(';')
  const declares = (prelude: string, declaration: string) =>
    new RegExp(`(?:^|;)\\s*${declaration}\\s*(?:;|$)`).test(bodyOf(prelude))

  it('declares overflow-wrap: anywhere on the heading over a directory’s report', () => {
    expect(bodyOf('.settings-box > .dir-report > h4')).toMatch(
      /(?:^|;)\s*overflow-wrap:\s*anywhere\s*(?:;|$)/,
    )
  })

  it('lays an editor out of the flow over the title’s first line, the title unseen beneath it in its place', () => {
    const missing = (prelude: string, declarations: string[]) =>
      declarations
        .filter((declaration) => !declares(prelude, declaration))
        .map((declaration) => `${prelude} { ${declaration} }`)
    expect([
      ...missing('.page-header-text:has(> .page-title-editor)', [
        'position:\\s*relative',
      ]),
      ...missing('.page-header .page-title[inert]', ['visibility:\\s*hidden']),
      ...missing('.page-header .page-title-editor', [
        'position:\\s*absolute',
        'inset:\\s*0 0 auto',
        'display:\\s*flex',
        'align-items:\\s*center',
        'height:\\s*calc\\(1\\.25 \\* var\\(--text-h2\\)\\)',
      ]),
    ]).toEqual([])
  })

  it('stacks a header’s two sets of actions in one cell, the set not in use unseen', () => {
    expect(
      [
        ['.page-actions.page-actions-swap', 'display:\\s*grid'],
        ['.page-actions-swap > .page-actions-set', 'grid-area:\\s*1 / 1'],
        ['.page-actions-swap > [inert]', 'visibility:\\s*hidden'],
      ].filter(([prelude, declaration]) => !declares(prelude, declaration)),
    ).toEqual([])
  })

  it('draws the box that edits a title at the title’s face, size, weight and line height, inside the title’s column, its words where the title’s are, marked by a line under them that is its focus mark and a transparent ring inside it that forced colours draw, clear of the phone bar as the title is', () => {
    const box = '.page-header .page-title-box'
    expect(
      [
        [
          '.page-header .page-title',
          'scroll-margin-top:\\s*calc\\(var\\(--sticky-top, 0px\\) \\+ 0\\.5rem\\)',
        ],
        [box, 'box-sizing:\\s*border-box'],
        [box, 'flex:\\s*1 1 auto'],
        [box, 'min-width:\\s*0'],
        [box, '--underline:\\s*2px'],
        [box, 'margin-bottom:\\s*calc\\(-1 \\* var\\(--underline\\)\\)'],
        [box, 'padding:\\s*0'],
        [box, 'border:\\s*0'],
        [
          box,
          'border-bottom:\\s*var\\(--underline\\) solid var\\(--line-strong\\)',
        ],
        [box, 'border-radius:\\s*0'],
        [box, 'background:\\s*none'],
        [box, 'font-family:\\s*var\\(--font-title\\)'],
        [box, 'font-size:\\s*var\\(--text-h2\\)'],
        [box, 'font-weight:\\s*600'],
        [box, 'line-height:\\s*1\\.25'],
        [
          box,
          'scroll-margin-top:\\s*calc\\(var\\(--sticky-top, 0px\\) \\+ 0\\.5rem\\)',
        ],
        [`${box}:focus`, 'border-bottom-color:\\s*var\\(--accent\\)'],
        [`${box}:focus`, 'outline:\\s*2px solid transparent'],
        [`${box}:focus`, 'outline-offset:\\s*-2px'],
      ].filter(([prelude, declaration]) => !declares(prelude, declaration)),
    ).toEqual([])
  })

  it('lays an edit’s refusal out of the flow at the header’s foot, above what it lies over, on the raised surface inside an edge, with no margin of its own, clear of the window’s foot', () => {
    const callout = '.page-header-answer'
    const alert = '.page-header-answer > [role="alert"]'
    expect(
      [
        ['.page-header:has(> .page-header-answer)', 'position:\\s*relative'],
        [callout, 'position:\\s*absolute'],
        [callout, 'top:\\s*100%'],
        [callout, 'left:\\s*0'],
        [callout, 'z-index:\\s*1'],
        [callout, 'max-width:\\s*100%'],
        [alert, 'margin:\\s*0'],
        [alert, 'border:\\s*1px solid var\\(--line-strong\\)'],
        [alert, 'background:\\s*var\\(--surface\\)'],
        [alert, 'overflow-wrap:\\s*anywhere'],
        [alert, 'scroll-margin-bottom:\\s*0\\.5rem'],
      ].filter(([prelude, declaration]) => !declares(prelude, declaration)),
    ).toEqual([])
  })
})

/**
 * A dot drawn between items is the look's, not the words': without empty
 * alternative text a screen reader says each one, and since a dot that ends
 * a line is hidden (`lineEnds.ts`), what it said would change with the
 * window's width.
 */
describe('the Configuration stylesheets draw each separator dot with no alternative text', () => {
  it('gives every "·" that settings.css and admin.css generate the alternative text ""', () => {
    const dots = [
      ['settings.css', settingsSource],
      ['admin.css', adminSource],
    ].flatMap(([sheet, source]) =>
      rules(source)
        .filter(({ body }) => /(?:^|;)\s*content:[^;]*·/.test(body))
        .map(({ prelude, body }) => ({ at: `${sheet}: ${prelude}`, body })),
    )
    expect(dots.length).toBeGreaterThan(0)
    expect(
      dots
        .filter(
          ({ body }) =>
            !/(?:^|;)\s*content:\s*"·"\s*\/\s*""\s*(?:;|$)/.test(body),
        )
        .map(({ at }) => at),
    ).toEqual([])
  })
})
