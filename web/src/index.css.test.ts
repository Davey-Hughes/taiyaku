import { describe, expect, it } from 'vitest'
import cssSource from './index.css?raw'

/**
 * WCAG AA contrast checks for the 2026-08-21 browser pass's D2 and D3 —
 * see the comments above `.alignment-panel .paired` and
 * `.volumes li.without .cover-box` in `index.css` for the fixes themselves.
 *
 * **jsdom cannot answer this by rendering.** `Login.theme.test.ts` already
 * notes jsdom does not resolve CSS system colours or composite `opacity`
 * into real pixels, so a test that mounts the page and reads
 * `getComputedStyle` would be asserting a fiction. Instead this reads
 * `index.css`'s own text and recomputes the same WCAG relative-luminance
 * math a real browser applies, against the actual `ButtonFace` / `CanvasText`
 * / `Canvas` RGB values a real Chromium reports for each colour scheme —
 * measured once via `getComputedStyle` on a live page (not invented):
 *
 * - light: `Canvas` `rgb(255 255 255)`, `CanvasText` `rgb(0 0 0)`,
 *   `ButtonFace` `rgb(239 239 239)`
 * - dark: `Canvas` `rgb(18 18 18)`, `CanvasText` `rgb(255 255 255)`,
 *   `ButtonFace` `rgb(107 107 107)`
 *
 * Plugging the unfixed values into the formulas below reproduces the
 * browser pass's own figures exactly — `.paired` at `opacity: 0.75` comes
 * out to 3.81:1 dark / 9.63:1 light, and the old whole-`<li>` `opacity: 0.5`
 * comes out to 3.98:1 (title, light) and 3.71:1 / 3.01:1 (button, light /
 * dark) — which is what shows this model is the real one and not a guess.
 */

const LIGHT = {
  canvas: [255, 255, 255] as const,
  canvasText: [0, 0, 0] as const,
  buttonFace: [239, 239, 239] as const,
}
const DARK = {
  canvas: [18, 18, 18] as const,
  canvasText: [255, 255, 255] as const,
  buttonFace: [107, 107, 107] as const,
}

function channelLum(c: number): number {
  const s = c / 255
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}

function relLum([r, g, b]: readonly [number, number, number]): number {
  return (
    0.2126 * channelLum(r) + 0.7152 * channelLum(g) + 0.0722 * channelLum(b)
  )
}

/** WCAG contrast ratio between two sRGB colours. */
function contrast(
  a: readonly [number, number, number],
  b: readonly [number, number, number],
): number {
  const la = relLum(a)
  const lb = relLum(b)
  const hi = Math.max(la, lb)
  const lo = Math.min(la, lb)
  return (hi + 0.05) / (lo + 0.05)
}

/**
 * `opacity` composites the *whole element group* against what is behind it
 * — not the text colour alone blended toward the background it sits on. A
 * group of `n` opaque layers rendered at `alpha` shows, wherever a layer
 * painted, `alpha * layer + (1 - alpha) * backdrop`; `alpha = 1` is the
 * unchanged colour, which is what lets one helper cover both the dimmed and
 * the undimmed case below.
 */
function composite(
  fg: readonly [number, number, number],
  backdrop: readonly [number, number, number],
  alpha: number,
): [number, number, number] {
  return [
    alpha * fg[0] + (1 - alpha) * backdrop[0],
    alpha * fg[1] + (1 - alpha) * backdrop[1],
    alpha * fg[2] + (1 - alpha) * backdrop[2],
  ]
}

function ruleBody(selector: string): string {
  const re = new RegExp(
    `${selector.replace(/[.[\]]/g, '\\$&')}\\s*\\{([^}]*)\\}`,
  )
  const m = cssSource.match(re)
  if (!m) throw new Error(`no rule found for ${selector} in index.css`)
  return m[1]
}

function opacityOf(body: string): number {
  const m = body.match(/opacity:\s*([\d.]+)/)
  if (!m) throw new Error(`rule sets no opacity: ${body}`)
  return Number(m[1])
}

describe('D2: `.alignment-panel .paired` clears WCAG AA in both schemes', () => {
  // The rule this is guarding: `.paired` is `178 of 180`, the number the
  // panel exists to show, faded by `opacity`. `ButtonFace` is a mid-grey in
  // dark and near-white in light, so one opacity lands at two very
  // different ratios — the fix must clear 4.5:1 in both, not just the one
  // the original author happened to check.
  const opacity = opacityOf(ruleBody('.alignment-panel .paired'))

  it('clears 4.5:1 in dark — the scheme the unfixed 0.75 failed at 3.81:1', () => {
    const ratio = contrast(
      composite(DARK.canvasText, DARK.buttonFace, opacity),
      DARK.buttonFace,
    )
    expect(ratio).toBeGreaterThanOrEqual(4.5)
  })

  it('clears 4.5:1 in light too', () => {
    const ratio = contrast(
      composite(LIGHT.canvasText, LIGHT.buttonFace, opacity),
      LIGHT.buttonFace,
    )
    expect(ratio).toBeGreaterThanOrEqual(4.5)
  })
})

describe('D3: a dimmed volume tile fades its cover, not its label or its button', () => {
  // The defect: `.volumes li.without { opacity: 0.5 }` dimmed the whole
  // `<li>` as one group — cover, title and the `i` button together — even
  // though the tile is not disabled: its link still navigates and its
  // button still opens the panel. `groupOpacity` below is what that bare
  // rule contributes to the title's and the button's own contrast; if the
  // rule is gone (the fix), there is nothing left to dim them and this is 1.
  function groupOpacity(): number {
    const m = cssSource.match(/\.volumes li\.without\s*\{([^}]*)\}/)
    return m ? opacityOf(m[1]) : 1
  }

  it('no longer dims the whole tile as one group', () => {
    expect(cssSource).not.toMatch(/\.volumes li\.without\s*\{/)
  })

  it('dims the cover box instead, and still dims it (not merely relocated to a no-op)', () => {
    const opacity = opacityOf(ruleBody('.volumes li.without .cover-box'))
    expect(opacity).toBeLessThan(1)
  })

  it('leaves the title at 4.5:1 or better in both schemes', () => {
    const alpha = groupOpacity()
    const lightRatio = contrast(
      composite(LIGHT.canvasText, LIGHT.canvas, alpha),
      LIGHT.canvas,
    )
    const darkRatio = contrast(
      composite(DARK.canvasText, DARK.canvas, alpha),
      DARK.canvas,
    )
    expect(lightRatio).toBeGreaterThanOrEqual(4.5)
    expect(darkRatio).toBeGreaterThanOrEqual(4.5)
  })

  it('leaves the `i` button at 4.5:1 or better in both schemes', () => {
    const alpha = groupOpacity()
    const lightText = composite(LIGHT.canvasText, LIGHT.canvas, alpha)
    const lightFace = composite(LIGHT.buttonFace, LIGHT.canvas, alpha)
    const darkText = composite(DARK.canvasText, DARK.canvas, alpha)
    const darkFace = composite(DARK.buttonFace, DARK.canvas, alpha)
    expect(contrast(lightText, lightFace)).toBeGreaterThanOrEqual(4.5)
    expect(contrast(darkText, darkFace)).toBeGreaterThanOrEqual(4.5)
  })
})
