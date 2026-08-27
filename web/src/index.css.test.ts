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

/**
 * **The disabled Accept — the one place on the proposals panel where the
 * "WCAG exempts an inactive component" exemption was the wrong call.**
 *
 * `.proposal-detail .accept` is disabled for the whole of every edit whose
 * answer is a refusal, which is the state the form sits in while a pattern is
 * being widened — not a momentary one like the dismiss button beside it. It
 * shipped at `color: GrayText` over the `ButtonFace` its enabled rule paints,
 * which the model above puts at **1.35:1 in dark** and 3.43:1 in light: the
 * word was gone and only the border said a control was there.
 *
 * Those two figures are what this block's own model produces for the unfixed
 * declarations, which is what shows it is the real one — the same check the
 * D2/D3 comment above makes of itself. `GrayText` is `rgb(128 128 128)` in
 * both schemes, measured off a live Chromium the way the three colours above
 * were, and reconfirmed by `AcceptForm.browser.test.tsx`, which reads the
 * shipped control's own computed colours rather than modelling them.
 *
 * The fix keeps the word at full contrast and says "unavailable" with the
 * ground and the border instead, so what this guards is both halves: legible
 * in either scheme, and still not drawn as the live control.
 */
describe('the disabled Accept is legible in both schemes', () => {
  const GRAY_TEXT = [128, 128, 128] as const

  const PALETTE = {
    light: {
      Canvas: LIGHT.canvas,
      CanvasText: LIGHT.canvasText,
      ButtonFace: LIGHT.buttonFace,
      GrayText: GRAY_TEXT,
    },
    dark: {
      Canvas: DARK.canvas,
      CanvasText: DARK.canvasText,
      ButtonFace: DARK.buttonFace,
      GrayText: GRAY_TEXT,
    },
  } as const

  type Scheme = keyof typeof PALETTE
  type ColourName = keyof (typeof PALETTE)['light']

  function declared(body: string, property: string): string | undefined {
    const m = body.match(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`))
    return m?.[1].trim()
  }

  const enabled = ruleBody('.proposal-detail .accept')
  const disabled = ruleBody('.proposal-detail .accept:disabled')

  /** What the disabled control actually paints: its own rule, or the base. */
  function effective(property: string): string {
    const value = declared(disabled, property) ?? declared(enabled, property)
    if (value === undefined) {
      throw new Error(`neither .accept nor .accept:disabled sets ${property}`)
    }
    return value
  }

  function rgb(
    name: string,
    scheme: Scheme,
  ): readonly [number, number, number] {
    const found = PALETTE[scheme][name as ColourName]
    if (!found) throw new Error(`not a system colour this model knows: ${name}`)
    return found
  }

  it('reproduces the shipped defect from the same model, so the model is the real one', () => {
    // `GrayText` on the enabled rule's `ButtonFace`, which is what the file
    // held before this block existed.
    expect(
      contrast(rgb('GrayText', 'dark'), rgb('ButtonFace', 'dark')),
    ).toBeCloseTo(1.35, 2)
    expect(
      contrast(rgb('GrayText', 'light'), rgb('ButtonFace', 'light')),
    ).toBeCloseTo(3.43, 2)
  })

  it('clears 4.5:1 in dark — the scheme the shipped `GrayText` failed at 1.35:1', () => {
    expect(
      contrast(
        rgb(effective('color'), 'dark'),
        rgb(effective('background'), 'dark'),
      ),
    ).toBeGreaterThanOrEqual(4.5)
  })

  it('clears 4.5:1 in light too', () => {
    expect(
      contrast(
        rgb(effective('color'), 'light'),
        rgb(effective('background'), 'light'),
      ),
    ).toBeGreaterThanOrEqual(4.5)
  })

  it('is still not drawn as the live control', () => {
    // The other half, and the reason legibility alone is not the whole fix: a
    // readable button that looks exactly like the enabled one says the form
    // is ready when it is refusing. The ground and the border carry that
    // here, since the word no longer can.
    expect(declared(disabled, 'background')).toBeDefined()
    expect(declared(disabled, 'background')).not.toBe(
      declared(enabled, 'background'),
    )
    expect(declared(disabled, 'border-style')).toBe('dashed')
    expect(declared(enabled, 'border')).toContain('solid')
  })
})
