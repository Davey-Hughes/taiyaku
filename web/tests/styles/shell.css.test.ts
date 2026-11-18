import { describe, expect, it } from 'vitest'
import { PHONE_LAYOUT, PHONE_LAYOUT_PARTS } from '../../src/phoneLayout'
import { atRulesOf, rules, selectors, subject, valuesOf } from '../support/css'
import shellSource from '../../src/styles/shell.css?raw'

const SHEETS = import.meta.glob<string>(['./index.css', './styles/*.css'], {
  base: '../../src',
  query: '?raw',
  import: 'default',
  eager: true,
})

/** `value` split at its top-level spaces, so `env(…, 0px)` stays one part. */
function parts(value: string): string[] {
  const found: string[] = []
  let depth = 0
  let start = 0
  for (let i = 0; i < value.length; i++) {
    const char = value[i]
    if (char === '(') depth++
    else if (char === ')') depth--
    else if (char === ' ' && depth === 0) {
      found.push(value.slice(start, i))
      start = i + 1
    }
  }
  found.push(value.slice(start))
  return found
}

/**
 * The open drawer is the sidebar column's width (O307): 14rem of content,
 * with the notch's safe area added to the width and padded on the left, so
 * a phone held sideways keeps the same 14rem beside its notch. A headless
 * browser reports no inset, so only the stylesheet shows the inset's part;
 * `Shell.browser.test.tsx`'s "the drawer on a short screen" measures the
 * rest.
 */
describe('shell.css draws the drawer and its foot', () => {
  it('draws the drawer 14rem wide plus the left safe area it pads', () => {
    const drawer = atRulesOf(shellSource, '.sidebar-column').filter(
      ({ at }) => at === `@media ${PHONE_LAYOUT}`,
    )
    expect(drawer).toHaveLength(1)
    const body = drawer[0]?.body ?? ''
    expect(valuesOf(body, 'width')).toEqual([
      'calc(14rem + env(safe-area-inset-left, 0px))',
    ])
    // The inset is inside the width only where the width holds the padding.
    expect(valuesOf(body, 'box-sizing')).toEqual(['border-box'])
    const padding = valuesOf(body, 'padding').map(parts)
    // The fixture tells: one shorthand, each side written out.
    expect(padding.map((sides) => sides.length)).toEqual([4])
    expect(padding[0]?.[3]).toBe('env(safe-area-inset-left, 0px)')
  })

  // O309: on a short screen the foot follows the navigation, which then has
  // the whole drawer. Every sheet is read, so a second rule turning the foot
  // static under another condition is found wherever it is written.
  it('sets the foot static under (height < 500px) and nowhere else', () => {
    const static_ = Object.entries(SHEETS).flatMap(([file, css]) =>
      [
        ...new Set(
          rules(css)
            .map(({ prelude }) => prelude)
            .filter((prelude) =>
              selectors(prelude).some((one) =>
                subject(one).includes('.sidebar-foot'),
              ),
            ),
        ),
      ].flatMap((prelude) =>
        atRulesOf(css, prelude)
          .filter(({ body }) => valuesOf(body, 'position').includes('static'))
          .map(({ at }) => `${file}: ${prelude} in ${at}`),
      ),
    )
    expect(static_).toEqual([
      `./styles/shell.css: .sidebar-foot in @media ${PHONE_LAYOUT_PARTS[1]}`,
    ])
  })
})
