import { existsSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import { rules, withoutComments } from '../testing/css'
import tokensSource from './tokens.css?raw'

/**
 * O179's `"taiyaku marks"` faces, read from `tokens.css`'s text.
 * `../fonts.browser.test.tsx` measures what they draw, but CI does not run
 * the browser project. What is pinned here is what the stylesheet declares — that `--font-ui` asks for the faces
 * first, and that each weight Zen Kaku is imported at has a face of its own,
 * pointing at its own file, for exactly the seven characters.
 */

const CSS = withoutComments(tokensSource)

const MARKS = 'U+0060, U+00A8, U+00B4, U+00B8, U+02C6, U+02DA, U+02DC'

/** Each `@font-face` for `family`, as its declarations by property. */
function faces(family: string): Array<Record<string, string>> {
  return rules(tokensSource)
    .filter((rule) => rule.prelude === '@font-face')
    .map((rule) =>
      Object.fromEntries(
        rule.body
          .split(';')
          .map((declaration) => declaration.split(/:(.*)/s))
          .filter(([, value]) => value !== undefined)
          .map(([property, value]) => [
            property.trim(),
            value.replace(/\s+/g, ' ').trim(),
          ]),
      ),
    )
    .filter((face) => face['font-family'] === `"${family}"`)
}

describe("tokens.css's marks faces", () => {
  it('puts them first in --font-ui, ahead of Zen Kaku', () => {
    const [, stack] = /--font-ui:\s*([^;]+);/.exec(CSS) ?? []
    expect(stack?.replace(/\s+/g, ' ').trim()).toBe(
      '"taiyaku marks", "Zen Kaku Gothic New", system-ui, sans-serif',
    )
  })

  it("gives each weight Zen Kaku is imported at a face, from that weight's file", () => {
    const imported = [
      ...CSS.matchAll(
        /@import "@fontsource\/zen-kaku-gothic-new\/(\d+)\.css"/g,
      ),
    ].map((m) => m[1])
    expect(imported).toEqual(['400', '500', '700'])
    expect(faces('taiyaku marks')).toEqual(
      imported.map((weight) => ({
        'font-family': '"taiyaku marks"',
        'font-weight': weight,
        src: `url("./fonts/marks-${weight}.woff2") format("woff2")`,
        'font-display': 'swap',
        'unicode-range': MARKS,
      })),
    )
    for (const weight of imported) {
      expect(
        existsSync(new URL(`./fonts/marks-${weight}.woff2`, import.meta.url)),
      ).toBe(true)
    }
  })
})
