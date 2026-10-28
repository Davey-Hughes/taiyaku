import { describe, expect, it } from 'vitest'
import adminSource from '../src/styles/admin.css?raw'
import baseSource from '../src/styles/base.css?raw'
import librarySource from '../src/styles/library.css?raw'
import shellSource from '../src/styles/shell.css?raw'
import tokensSource from '../src/styles/tokens.css?raw'
import { rules, selectors, subject, withoutComments } from './support/css'

/**
 * WCAG AA contrast, computed from the stylesheets' own text.
 *
 * **jsdom cannot answer this by rendering** — it resolves neither
 * `light-dark()` nor a composited `opacity` into pixels — so this reads the
 * `light-dark(<paper>, <ink>)` pairs in `styles/tokens.css` and the rules in
 * `styles/`, and recomputes the relative-luminance arithmetic a browser
 * applies.
 *
 * **Why the model is trusted.** Until 2026-09-17 these rules used CSS system
 * colours, and this file modelled the values a live Chromium reported for
 * them; that model reproduced the browser pass's own figures (D2's 3.81:1 in
 * dark, the disabled Accept's 1.35:1), which is what showed it was the real
 * one. No rule names a system colour now, so the token model earns the same
 * trust by reproducing four ratios the style pass's spec computed outside this
 * file (`docs/superpowers/specs/2026-09-17-style-pass-design.md`, §2).
 *
 * Three regression checks predate the tokens and are kept, recomputed against
 * them: D2 and D3 from the 2026-08-21 browser pass, and the disabled Accept.
 * The fixes are described above `.alignment-panel .paired` and
 * `.volumes li.without .cover-box` in `styles/library.css`, and above
 * `.proposal-detail .accept:disabled` in `styles/admin.css`.
 */

type Rgb = readonly [number, number, number]
const SCHEMES = ['paper', 'ink'] as const
type Scheme = (typeof SCHEMES)[number]

const cssSource = [shellSource, librarySource, adminSource].join('\n')

/**
 * Every stylesheet under `src/`, by path — `index.css` and every partial in
 * `styles/`, the ones `ruleBody` below reads and the ones it does not — for
 * D3's check, which has to hold wherever a rule is written.
 */
const STYLESHEETS = import.meta.glob<string>('./**/*.css', {
  base: '../src',
  query: '?raw',
  import: 'default',
  eager: true,
})

function hex(value: string): Rgb {
  const n = Number.parseInt(value.slice(1), 16)
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255]
}

/** Every `--name: light-dark(#paper, #ink)` in `tokens.css`. */
const TOKENS = new Map<string, Record<Scheme, Rgb>>(
  [
    ...tokensSource.matchAll(
      /--([a-z-]+):\s*light-dark\(\s*(#[0-9a-fA-F]{6})\s*,\s*(#[0-9a-fA-F]{6})\s*\)/g,
    ),
  ].map((m): [string, Record<Scheme, Rgb>] => [
    m[1],
    { paper: hex(m[2]), ink: hex(m[3]) },
  ]),
)

function token(name: string, scheme: Scheme): Rgb {
  const pair = TOKENS.get(name)
  if (!pair)
    throw new Error(`tokens.css has no light-dark() pair for --${name}`)
  return pair[scheme]
}

/** A declared `var(--name)`, as the colour that token paints in `scheme`. */
function resolve(value: string, scheme: Scheme): Rgb {
  const m = value.match(/^var\(--([a-z-]+)\)$/)
  if (!m) throw new Error(`not a token reference: ${value}`)
  return token(m[1], scheme)
}

function channelLum(c: number): number {
  const s = c / 255
  return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
}

function relLum([r, g, b]: Rgb): number {
  return (
    0.2126 * channelLum(r) + 0.7152 * channelLum(g) + 0.0722 * channelLum(b)
  )
}

/** WCAG contrast ratio between two sRGB colours. */
function contrast(a: Rgb, b: Rgb): number {
  const la = relLum(a)
  const lb = relLum(b)
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05)
}

/**
 * `opacity` composites the *whole element group* against what is behind it —
 * not the text colour alone blended toward the background it sits on. A group
 * rendered at `alpha` shows, wherever a layer painted,
 * `alpha * layer + (1 - alpha) * backdrop`; `alpha = 1` is the unchanged
 * colour, which is what lets one helper cover the dimmed and undimmed cases.
 */
function composite(fg: Rgb, backdrop: Rgb, alpha: number): Rgb {
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
  if (!m) throw new Error(`no rule found for ${selector} in styles/`)
  return m[1]
}

function declared(body: string, property: string): string | undefined {
  const m = body.match(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`))
  return m?.[1].trim()
}

function required(body: string, property: string): string {
  const value = declared(body, property)
  if (value === undefined) throw new Error(`rule sets no ${property}: ${body}`)
  return value
}

function opacityOf(body: string): number {
  const m = body.match(/opacity:\s*([\d.]+)/)
  if (!m) throw new Error(`rule sets no opacity: ${body}`)
  return Number(m[1])
}

/**
 * Every paired token as it was before spec 2026-10-18 §3 lifted Ink's: the
 * palette the reader was designed with, which it still draws (its own block
 * below), the hairline §3 holds the lifted `--line` to, and the values the
 * style pass's spec computed its ratios from. Written out rather than read
 * from `tokens.css`, so a change to either half there fails against it.
 */
const BEFORE_LIFT: Record<string, Record<Scheme, string>> = {
  bg: { paper: '#f3eee3', ink: '#15130f' },
  surface: { paper: '#e9e2d2', ink: '#211e18' },
  field: { paper: '#fbf8f2', ink: '#1b1914' },
  line: { paper: '#d6ccb9', ink: '#35312a' },
  'line-strong': { paper: '#897d68', ink: '#746c60' },
  text: { paper: '#231e17', ink: '#ece4d4' },
  muted: { paper: '#6b6151', ink: '#a69b87' },
  accent: { paper: '#b0361d', ink: '#e8715a' },
  'on-accent': { paper: '#fbf8f2', ink: '#15130f' },
  danger: { paper: '#9e1b32', ink: '#f28b9a' },
  success: { paper: '#3f6b2f', ink: '#95c283' },
  warning: { paper: '#7d5500', ink: '#e2b55c' },
  scrim: { paper: '#231e1759', ink: '#0000008c' },
}

/** An opaque colour from `BEFORE_LIFT`, for a contrast ratio. */
function beforeLift(name: string, scheme: Scheme): Rgb {
  const value = BEFORE_LIFT[name]?.[scheme]
  if (!value || !/^#[0-9a-fA-F]{6}$/.test(value)) {
    throw new Error(`no opaque pair before the lift for --${name}`)
  }
  return hex(value)
}

describe('the token model', () => {
  it('reproduces four ratios the spec computed outside this file', () => {
    expect(contrast(token('text', 'paper'), token('bg', 'paper'))).toBeCloseTo(
      14.3,
      2,
    )
    expect(
      contrast(token('muted', 'paper'), token('surface', 'paper')),
    ).toBeCloseTo(4.71, 2)
    // Ink's two from the values the style pass's spec measured, which the
    // lift has since moved on `:root`; `:root`'s own are held to spec
    // 2026-10-18 §3's figures below.
    expect(
      contrast(beforeLift('line-strong', 'ink'), beforeLift('bg', 'ink')),
    ).toBeCloseTo(3.58, 2)
    expect(
      contrast(beforeLift('accent', 'ink'), beforeLift('bg', 'ink')),
    ).toBeCloseTo(6.13, 2)
  })
})

const GROUNDS = ['bg', 'surface', 'field'] as const
const TEXT_TOKENS = [
  'text',
  'muted',
  'accent',
  'danger',
  'success',
  'warning',
] as const

describe("spec §2's floors hold in both themes", () => {
  for (const scheme of SCHEMES) {
    it(`every text colour clears 4.5:1 and every control border 3:1 in ${scheme}`, () => {
      const failures: string[] = []
      for (const ground of GROUNDS) {
        for (const text of TEXT_TOKENS) {
          const ratio = contrast(token(text, scheme), token(ground, scheme))
          if (ratio < 4.5) {
            failures.push(`--${text} on --${ground}: ${ratio.toFixed(2)}`)
          }
        }
        const border = contrast(
          token('line-strong', scheme),
          token(ground, scheme),
        )
        if (border < 3) {
          failures.push(`--line-strong on --${ground}: ${border.toFixed(2)}`)
        }
      }
      const onAccent = contrast(
        token('on-accent', scheme),
        token('accent', scheme),
      )
      if (onAccent < 4.5) {
        failures.push(`--on-accent on --accent: ${onAccent.toFixed(2)}`)
      }
      expect(failures).toEqual([])
    })
  }
})

/**
 * APCA-W3 0.0.98G's lightness contrast, Lc, of `text` drawn on `ground`:
 * positive for dark text on a light ground, negative for light on dark. Spec
 * 2026-10-18 §3 measures with it beside WCAG 2 because WCAG 2 passed Ink's
 * muted and accent at 4.5:1 and more while the user could not read them; its
 * luminance is a plain 2.4 power, with no linear toe, and near-black is
 * lifted by a soft clamp before the two are compared.
 */
function apcaLc(text: Rgb, ground: Rgb): number {
  const luminance = ([r, g, b]: Rgb) => {
    const y =
      0.2126729 * (r / 255) ** 2.4 +
      0.7151522 * (g / 255) ** 2.4 +
      0.072175 * (b / 255) ** 2.4
    // biome-ignore lint/suspicious/noApproximativeNumericConstant: APCA's soft-clamp exponent is 1.414 as published, not √2
    return y < 0.022 ? y + (0.022 - y) ** 1.414 : y
  }
  const yt = luminance(text)
  const yb = luminance(ground)
  if (Math.abs(yb - yt) < 0.0005) return 0
  if (yb > yt) {
    const s = (yb ** 0.56 - yt ** 0.57) * 1.14
    return s < 0.1 ? 0 : (s - 0.027) * 100
  }
  const s = (yb ** 0.65 - yt ** 0.62) * 1.14
  return s > -0.1 ? 0 : (s + 0.027) * 100
}

/**
 * Paper's Lc for each text token on each ground, recorded rather than held
 * to a floor: spec §3 leaves Paper's colours as they are, and this is what
 * they score. Literal figures, so a Paper colour changed here moves one and
 * fails; a table computed from `tokens.css` could not.
 */
const PAPER_LC: Record<
  (typeof TEXT_TOKENS)[number],
  Record<(typeof GROUNDS)[number], number>
> = {
  text: { bg: 93.6, surface: 86.7, field: 99.4 },
  muted: { bg: 70.5, surface: 63.5, field: 76.3 },
  accent: { bg: 69.8, surface: 62.9, field: 75.7 },
  danger: { bg: 75.9, surface: 69.0, field: 81.8 },
  success: { bg: 71.1, surface: 64.2, field: 77.0 },
  warning: { bg: 72.6, surface: 65.6, field: 78.4 },
}

describe('APCA beside WCAG (spec 2026-10-18 §3)', () => {
  it('reproduces two Lc figures the spec computed outside this file', () => {
    // Ink's muted before the lift on Ink's page, and Paper's muted on Paper's:
    // the asymmetry §3 opens with. The sign is the polarity, light on dark.
    expect(Math.round(apcaLc(hex('#a69b87'), hex('#15130f')))).toBe(-48)
    expect(Math.round(apcaLc(hex('#6b6151'), hex('#f3eee3')))).toBe(70)
  })

  it('holds every Ink text colour to |Lc| 60 on bg, surface and field', () => {
    const failures: string[] = []
    for (const ground of GROUNDS) {
      for (const text of TEXT_TOKENS) {
        const lc = apcaLc(token(text, 'ink'), token(ground, 'ink'))
        if (Math.abs(lc) < 60) {
          failures.push(`--${text} on --${ground}: ${lc.toFixed(1)}`)
        }
      }
    }
    expect(failures).toEqual([])
  })

  it("records Paper's Lc for every text colour on bg, surface and field", () => {
    const moved: string[] = []
    for (const ground of GROUNDS) {
      for (const text of TEXT_TOKENS) {
        const lc = apcaLc(token(text, 'paper'), token(ground, 'paper'))
        const recorded = PAPER_LC[text][ground]
        if (Math.abs(lc - recorded) >= 0.05) {
          moved.push(
            `--${text} on --${ground}: ${lc.toFixed(2)}, recorded ${recorded}`,
          )
        }
      }
    }
    expect(moved).toEqual([])
  })
})

/**
 * Spec 2026-10-18 §3's After column: the seven Ink halves the lift set,
 * as the spec gives them. Literal, so a pair written the wrong way round,
 * whose Paper half then stands in for Ink's, fails here where a floor it
 * still clears would not.
 */
const LIFTED_INK: Record<string, string> = {
  surface: '#2b2821',
  muted: '#c1b6a2',
  accent: '#ff9d88',
  danger: '#ff9aa8',
  success: '#97c485',
  line: '#3e3932',
  'line-strong': '#787064',
}

describe("spec 2026-10-18 §3's other rules for Ink", () => {
  it("writes Ink's seven lifted halves at the spec's values", () => {
    const moved = Object.entries(LIFTED_INK)
      .filter(
        ([name, value]) => token(name, 'ink').join() !== hex(value).join(),
      )
      .map(([name, value]) => `--${name}: ${token(name, 'ink')} for ${value}`)
    expect(moved).toEqual([])
  })

  it('steps the surface away from the page, to 1.25:1 or more', () => {
    expect(
      contrast(token('surface', 'ink'), token('bg', 'ink')),
    ).toBeGreaterThanOrEqual(1.25)
  })

  it('keeps a hairline on the surface at least as visible as before the lift', () => {
    expect(
      contrast(token('line', 'ink'), token('surface', 'ink')),
    ).toBeGreaterThanOrEqual(
      contrast(beforeLift('line', 'ink'), beforeLift('surface', 'ink')),
    )
  })

  // Ink's control border and its text on the accent: §2's floors, above.
})

/** `--name: rgb(r g b / a)` in `tokens.css`, as its channels and its alpha. */
function readerToken(name: string): { rgb: Rgb; alpha: number } {
  const m = tokensSource.match(
    new RegExp(`--${name}:\\s*rgb\\((\\d+) (\\d+) (\\d+) / ([\\d.]+)\\)`),
  )
  if (!m) throw new Error(`tokens.css has no rgb() value for --${name}`)
  return {
    rgb: [Number(m[1]), Number(m[2]), Number(m[3])],
    alpha: Number(m[4]),
  }
}

/** The rule under which `tokens.css` pins the reader's own pairs. */
const READER_PIN_BODY =
  rules(tokensSource).find(
    ({ prelude }) => prelude === '.reader-route, .reader',
  )?.body ?? ''

/**
 * The `light-dark(rgb(…), rgb(…))` pairs in that rule, by name: the tokens
 * the Ink lift changed, held for the reader at the pairs it was designed
 * with. `TOKENS` above reads only hex pairs, so it never takes these for
 * the theme's own.
 */
const READER_PIN = new Map<string, Record<Scheme, Rgb>>(
  [
    ...READER_PIN_BODY.matchAll(
      /--([a-z-]+):\s*light-dark\(\s*rgb\((\d+) (\d+) (\d+)\)\s*,\s*rgb\((\d+) (\d+) (\d+)\)\s*\)/g,
    ),
  ].map((m): [string, Record<Scheme, Rgb>] => [
    m[1],
    {
      paper: [Number(m[2]), Number(m[3]), Number(m[4])],
      ink: [Number(m[5]), Number(m[6]), Number(m[7])],
    },
  ]),
)

/**
 * The colour `--name` paints inside the reader in `scheme`: the reader's own
 * pair where it pins one, `:root`'s where it does not.
 */
function inReader(name: string, scheme: Scheme): Rgb {
  return READER_PIN.get(name)?.[scheme] ?? token(name, scheme)
}

/** The `:root` rules' bodies, where `tokens.css` declares the palette. */
const ROOT_BODY = rules(tokensSource)
  .filter(({ prelude }) => prelude === ':root')
  .map(({ body }) => body)
  .join(';')

/**
 * Every token `:root` writes as a `light-dark()` pair, whatever the halves
 * are written as: the list a check of the whole palette walks, so a token
 * added to `tokens.css` is walked too.
 */
const PAIRED = [
  ...ROOT_BODY.matchAll(/(?:^|;)\s*--([\w-]+):\s*light-dark\(/g),
].map((m) => m[1] ?? '')

const COLOUR = String.raw`#[0-9a-fA-F]{8}|#[0-9a-fA-F]{6}|rgb\(\d+ \d+ \d+\)`

/** Each `--name: light-dark(<paper>, <ink>)` in `body`, halves as written. */
function pairsIn(body: string): Map<string, Record<Scheme, string>> {
  const pair = new RegExp(
    String.raw`--([\w-]+):\s*light-dark\(\s*(${COLOUR})\s*,\s*(${COLOUR})\s*\)`,
    'g',
  )
  return new Map(
    [...body.matchAll(pair)].map((m): [string, Record<Scheme, string>] => [
      m[1] ?? '',
      { paper: m[2] ?? '', ink: m[3] ?? '' },
    ]),
  )
}

/** A colour as written, as its red, green, blue and alpha bytes. */
function channels(value: string): string {
  const rgb = value.match(/^rgb\((\d+) (\d+) (\d+)\)$/)
  if (rgb) return [rgb[1], rgb[2], rgb[3], 255].join()
  const digits = value.slice(1)
  const bytes = (digits.length === 6 ? `${digits}ff` : digits).match(/../g)
  if (!value.startsWith('#') || !bytes || bytes.length !== 4) {
    throw new Error(`not a colour this reads: ${value}`)
  }
  return bytes.map((byte) => Number.parseInt(byte, 16)).join()
}

describe("the reader's own palette (spec 2026-10-18 §3)", () => {
  // The reader is dark in both themes and the user called it "quite good",
  // so the lift was made for the library's grounds and not for it.
  it("draws every paired token at the pair it was designed with, and leaves Paper's halves on :root as they were", () => {
    // The table is held to the source's list both ways: a token missing
    // from it fails below, and one the source no longer pairs fails here.
    expect(Object.keys(BEFORE_LIFT).filter((n) => !PAIRED.includes(n))).toEqual(
      [],
    )
    const root = pairsIn(ROOT_BODY)
    const pinned = pairsIn(READER_PIN_BODY)
    const moved: string[] = []
    for (const name of PAIRED) {
      const designed = BEFORE_LIFT[name]
      const drawn = pinned.get(name) ?? root.get(name)
      const own = root.get(name)
      if (!designed || !drawn || !own) {
        moved.push(
          `--${name}: ${designed ? 'no pair read' : 'not in the table'}`,
        )
        continue
      }
      for (const scheme of SCHEMES) {
        if (channels(drawn[scheme]) !== channels(designed[scheme])) {
          moved.push(
            `${scheme} --${name}: ${drawn[scheme]} for ${designed[scheme]}`,
          )
        }
      }
      // The lift was Ink's alone, so Paper's colours are unchanged on
      // `:root` too, where the reader's pin would otherwise hide a change.
      if (channels(own.paper) !== channels(designed.paper)) {
        moved.push(`:root paper --${name}: ${own.paper} for ${designed.paper}`)
      }
    }
    expect(moved).toEqual([])
  })

  it('keeps its metadata at 12px', () => {
    expect(declared(READER_PIN_BODY, '--text-meta')).toBe('0.75rem')
  })
})

const BLACK: Rgb = [0, 0, 0]
const WHITE: Rgb = [255, 255, 255]

/**
 * Computed from what the reader draws (`inReader`), not from `:root`'s
 * lifted pairs, which the reader never paints.
 */
describe("the reader's tokens (spec §8.2)", () => {
  it("are Ink's --bg and --text at §2's opacities, over a black ground", () => {
    expect(tokensSource).toMatch(/--reader-ground:\s*#000;/)
    expect(readerToken('reader-panel')).toEqual({
      rgb: token('bg', 'ink'),
      alpha: 0.92,
    })
    expect(readerToken('reader-hair')).toEqual({
      rgb: token('text', 'ink'),
      alpha: 0.14,
    })
  })

  // O216. The mark on the progress line is drawn over the line and its fill,
  // and half of each has to come through it.
  it("hold the progress line's mark to Ink's --text, half let through", () => {
    expect(readerToken('reader-mark')).toEqual({
      rgb: token('text', 'ink'),
      alpha: 0.5,
    })
  })

  it('reproduce two ratios the spec computed outside this file', () => {
    const { rgb, alpha } = readerToken('reader-panel')
    expect(
      contrast(inReader('text', 'ink'), composite(rgb, BLACK, alpha)),
    ).toBeCloseTo(14.86, 2)
    expect(
      contrast(inReader('muted', 'ink'), composite(rgb, WHITE, alpha)),
    ).toBeCloseTo(5.52, 2)
  })

  for (const [under, ground] of [
    ['black', BLACK],
    ['white artwork', WHITE],
  ] as const) {
    it(`keep every Ink text colour above 4.5:1 on a panel over ${under}`, () => {
      const { rgb, alpha } = readerToken('reader-panel')
      const panel = composite(rgb, ground, alpha)
      const failures = TEXT_TOKENS.map(
        (text) => [text, contrast(inReader(text, 'ink'), panel)] as const,
      )
        .filter(([, ratio]) => ratio < 4.5)
        .map(([text, ratio]) => `--${text}: ${ratio.toFixed(2)}`)
      expect(failures).toEqual([])
    })
  }
})

describe('D2: `.alignment-panel .paired` clears WCAG AA in both themes', () => {
  // `.paired` is `178 of 180`, the number the panel exists to show, faded by
  // `opacity` over the panel's own ground. One opacity lands at different
  // ratios on different grounds, so it has to clear 4.5:1 in both themes.
  const panel = ruleBody('.alignment-panel')
  const opacity = opacityOf(ruleBody('.alignment-panel .paired'))

  for (const scheme of SCHEMES) {
    it(`clears 4.5:1 in ${scheme}`, () => {
      const ground = resolve(required(panel, 'background'), scheme)
      const text = resolve(required(panel, 'color'), scheme)
      expect(
        contrast(composite(text, ground, opacity), ground),
      ).toBeGreaterThanOrEqual(4.5)
    })
  }
})

/** The cover's own parts — the only things `.without` may legitimately dim. */
const COVER_PARTS = ['.cover-box', 'img', '.cover-missing']

/**
 * The opacity a rule's body dims with, below 1 — as a plain `opacity`
 * property or as `filter: opacity(…)` — checked declaration by declaration,
 * or `undefined` if neither is present below 1.
 */
function dimmingOpacity(body: string): number | undefined {
  for (const declaration of body.split(';')) {
    const plain = declaration.match(/^\s*opacity:\s*([\d.]+)(%?)/)
    if (plain) {
      const value = Number(plain[1]) / (plain[2] === '%' ? 100 : 1)
      if (value < 1) return value
    }
    const filtered = declaration.match(
      /^\s*filter:.*?\bopacity\(\s*([\d.]+)(%?)/,
    )
    if (filtered) {
      const value = Number(filtered[1]) / (filtered[2] === '%' ? 100 : 1)
      if (value < 1) return value
    }
  }
  return undefined
}

/**
 * Every rule, in every stylesheet given, that dims a `.without` tile itself
 * — `opacity` or `filter: opacity()` below 1 on a selector that contains
 * `.without` anywhere — rather than one of the cover's own parts inside it,
 * as `.volumes li.without .cover-box` does. An allowlist on what the
 * selector's last compound targets (`.cover-box`, `img`, `.cover-missing`),
 * not a spelling match on `.without`'s own compound: a combinator, a
 * wildcard or a different class after it all still count as dimming the
 * tile. `where` names the file and the one selector, so a failure says
 * which.
 *
 * **What it cannot see**, measured by the review of the fix that made it an
 * allowlist: `.without` spelled as an attribute selector,
 * `[class~="without"]`, is not recognised; and `dimmingOpacity` takes the
 * first dimming declaration in source order rather than the cascade's last,
 * so `opacity: 0.5; opacity: 1` is flagged although it does not dim. The
 * first fails open, the second closed; neither shape exists in `styles/`.
 */
function dimsTheTile(
  stylesheets: Record<string, string>,
): Array<{ where: string; opacity: number }> {
  const found: Array<{ where: string; opacity: number }> = []
  for (const [path, source] of Object.entries(stylesheets)) {
    for (const { prelude, body } of rules(source)) {
      const opacity = dimmingOpacity(body)
      if (opacity === undefined) continue
      for (const selector of selectors(prelude)) {
        if (!selector.includes('.without')) continue
        if (COVER_PARTS.includes(subject(selector))) continue
        found.push({ where: `${path}: ${selector}`, opacity })
      }
    }
  }
  return found
}

/** `dimsTheTile`'s findings as `file: selector → opacity`, for a diff. */
const described = (stylesheets: Record<string, string>): string[] =>
  dimsTheTile(stylesheets).map(({ where, opacity }) => `${where} → ${opacity}`)

describe('D3: a dimmed volume tile fades its cover, not its label or its button', () => {
  // The defect: `.volumes li.without { opacity: 0.5 }` dimmed the whole `<li>`
  // — cover, title and the `i` button together — though the tile is not
  // disabled. `groupOpacity` is what `dimsTheTile` finds: any `opacity` or
  // `filter: opacity()` below 1 on a selector that contains `.without`,
  // unless what it actually dims — the selector's last compound — is the
  // cover itself (`.cover-box`, `img` or `.cover-missing`). With none left
  // (the fix) there is nothing to dim the title or the button and it is 1.
  function groupOpacity(): number {
    return Math.min(1, ...dimsTheTile(STYLESHEETS).map((r) => r.opacity))
  }

  const library = ruleBody('.library')
  const button = ruleBody('.volumes .panel-open')

  it('reads index.css and every partial in styles/', () => {
    expect(Object.keys(STYLESHEETS)).toEqual(
      expect.arrayContaining([
        './index.css',
        './styles/admin.css',
        './styles/annotations.css',
        './styles/base.css',
        './styles/library.css',
        './styles/shell.css',
        './styles/tokens.css',
      ]),
    )
    expect(STYLESHEETS['./styles/library.css']).toContain(
      '.volumes li.without .cover-box',
    )
  })

  it('no longer dims the whole tile as one group, in any stylesheet', () => {
    expect(described(STYLESHEETS)).toEqual([])
  })

  it('flags the tile itself however it is dimmed, but not the cover inside it', () => {
    expect(
      described({
        'probe.css': `
          .volumes li.without, .volumes li.unused { opacity: 0.5 }
          .volumes .without { opacity: 50% }
          .volumes li.without:is(.a, .b) { color: red; opacity: .4 }
          .volumes li.without > * { opacity: 0.5 }
          .volumes li.without * { opacity: 0.5 }
          .volumes li.without a { opacity: 0.5 }
          .volumes li.without .title { opacity: 0.5 }
          .volumes li.without .panel-open { opacity: 0.5 }
          .volumes li.without { filter: opacity(0.5) }
          .volumes li.without .cover-box { opacity: 0.5 }
          .volumes li.without img, .volumes li.without .cover-missing { opacity: 0.5 }
          .volumes li.without { opacity: 1 }
        `,
      }),
    ).toEqual([
      'probe.css: .volumes li.without → 0.5',
      'probe.css: .volumes .without → 0.5',
      'probe.css: .volumes li.without:is(.a, .b) → 0.4',
      'probe.css: .volumes li.without > * → 0.5',
      'probe.css: .volumes li.without * → 0.5',
      'probe.css: .volumes li.without a → 0.5',
      'probe.css: .volumes li.without .title → 0.5',
      'probe.css: .volumes li.without .panel-open → 0.5',
      'probe.css: .volumes li.without → 0.5',
    ])
  })

  it('dims the cover box instead, and still dims it (not merely relocated to a no-op)', () => {
    const opacity = opacityOf(ruleBody('.volumes li.without .cover-box'))
    expect(opacity).toBeLessThan(1)
  })

  for (const scheme of SCHEMES) {
    it(`leaves the title at 4.5:1 or better in ${scheme}`, () => {
      const alpha = groupOpacity()
      const ground = resolve(required(library, 'background'), scheme)
      const text = resolve(required(library, 'color'), scheme)
      expect(
        contrast(composite(text, ground, alpha), ground),
      ).toBeGreaterThanOrEqual(4.5)
    })

    it(`leaves the \`i\` button at 4.5:1 or better in ${scheme}`, () => {
      const alpha = groupOpacity()
      const ground = resolve(required(library, 'background'), scheme)
      const text = composite(
        resolve(required(button, 'color'), scheme),
        ground,
        alpha,
      )
      const face = composite(
        resolve(required(button, 'background'), scheme),
        ground,
        alpha,
      )
      expect(contrast(text, face)).toBeGreaterThanOrEqual(4.5)
    })
  }
})

/**
 * **The disabled Accept** — the one place on the proposals panel where "WCAG
 * exempts an inactive component" was the wrong call, because the form sits
 * disabled for the whole of every edit whose answer is a refusal. It shipped at
 * `GrayText` over `ButtonFace`, 1.35:1 in dark Chromium. The fix keeps the word
 * at full contrast and says "unavailable" with the ground and the border
 * instead, so this guards both halves.
 */
describe('the disabled Accept is legible in both themes', () => {
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

  for (const scheme of SCHEMES) {
    it(`clears 4.5:1 in ${scheme}`, () => {
      expect(
        contrast(
          resolve(effective('color'), scheme),
          resolve(effective('background'), scheme),
        ),
      ).toBeGreaterThanOrEqual(4.5)
    })
  }

  it('is still not drawn as the live control', () => {
    expect(declared(disabled, 'background')).toBeDefined()
    expect(declared(disabled, 'background')).not.toBe(
      declared(enabled, 'background'),
    )
    expect(declared(disabled, 'border-style')).toBe('dashed')
    expect(declared(enabled, 'border')).toContain('solid')
  })
})

/**
 * The sidebar's section headings — Libraries, Library, Theme. They were faded
 * with `opacity: 0.65`, which has no floor anyone checks; spec 2026-09-17
 * §7.1 gives them `--muted` instead, on the page's own ground.
 */
describe('the sidebar headings are legible in both themes', () => {
  const heading = ruleBody('.sidebar-heading')

  it('are not faded with opacity', () => {
    expect(declared(heading, 'opacity')).toBeUndefined()
  })

  for (const scheme of SCHEMES) {
    it(`clear 4.5:1 on the page's ground in ${scheme}`, () => {
      const ground = resolve(
        required(ruleBody('.library'), 'background'),
        scheme,
      )
      expect(
        contrast(resolve(required(heading, 'color'), scheme), ground),
      ).toBeGreaterThanOrEqual(4.5)
    })
  }
})

/**
 * The series table's missing cell — spec 2026-09-17 §5 ② and §7.2. It was
 * `opacity: 0.55` over the text colour, 3.60:1 on Paper's ground; it is now
 * `--muted`, with its dashed border back at full `--line-strong`.
 */
describe('the series table’s missing cell is legible in both themes', () => {
  const missing = ruleBody('.editions td.missing')

  it('is not faded with opacity', () => {
    expect(declared(missing, 'opacity')).toBeUndefined()
  })

  for (const scheme of SCHEMES) {
    it(`draws its dash at 4.5:1 and its border at 3:1 in ${scheme}`, () => {
      const ground = resolve(
        required(ruleBody('.library'), 'background'),
        scheme,
      )
      const border = required(missing, 'border').match(/var\(--[a-z-]+\)/)
      if (!border) throw new Error('the missing cell’s border names no token')
      expect(
        contrast(resolve(required(missing, 'color'), scheme), ground),
      ).toBeGreaterThanOrEqual(4.5)
      expect(
        contrast(resolve(border[0], scheme), ground),
      ).toBeGreaterThanOrEqual(3)
    })
  }
})

/**
 * The figures face on the page counts only — spec 2026-09-17 §5 ②. `base.css`
 * sets it on every table inside `.library`, which drew the digits inside a
 * volume's file name heavier than the title around them; the series table
 * takes the UI face and gives the figures face back to its page-count cells.
 */
describe('the figures face reaches the series table’s page counts only', () => {
  it('sets the UI face on the table and the figures face on its page counts', () => {
    expect(declared(ruleBody('.editions'), 'font-family')).toBe(
      'var(--font-ui)',
    )
    expect(declared(ruleBody('.editions td.pages'), 'font-family')).toBe(
      'var(--font-figures)',
    )
  })

  it('is named by no other rule in any stylesheet the screens load', () => {
    // `base.css` is read here too since ⑤a. It carries the blanket rule
    // that puts the figures face on every table under `.library`, which is
    // exactly what this case exists to notice — and could not, while it
    // read only the three screen stylesheets.
    const naming = rules([cssSource, baseSource].join('\n'))
      .filter((rule) => rule.body.includes('--font-figures'))
      .map((rule) => rule.prelude)
    expect(naming).toEqual([
      '.editions td.pages',
      ':where(.library, .login) :where(table)',
    ])
  })

  it('does not reach the rule preview’s tables', () => {
    // §7.2 deferred this to ⑤. Neither table has a numeric column: every
    // digit in Matched files is inside a file name, and Provenance's only
    // digits are rule ordinals reading `rule 1`. So both opt out the way
    // `.editions` does — and unlike `.editions` they add no cell exception
    // back, because there is no page-count column to make one for.
    expect(declared(ruleBody('.preview table'), 'font-family')).toBe(
      'var(--font-ui)',
    )
    const exceptions = rules(adminSource)
      .filter(
        (rule) =>
          rule.prelude.startsWith('.preview') &&
          rule.body.includes('--font-figures'),
      )
      .map((rule) => rule.prelude)
    expect(exceptions).toEqual([])
  })
})

/**
 * **Two rows of one-of-N buttons, drawn two ways, and a third control that is
 * neither.** The library's filter row and the inbox's kind row take the
 * underlined tab look; the inbox's state row keeps pills, because it is the
 * coarser filter and two identical strips stacked give no hierarchy (spec
 * §12.3). What this pins is that no single selector reaches both kinds — the
 * regression ② shipped, when `.chips` drew "All" and "Hide weak proposals"
 * with the same accent underline side by side.
 */
describe('a tab row and a pill row stay two different rows', () => {
  it('draws the library’s row and the inbox’s kind row as tabs', () => {
    for (const sel of ['.library-chips button', '.proposal-tabs button']) {
      const body = ruleBody(sel)
      expect(declared(body, 'background')).toBe('none')
      expect(declared(body, 'border-radius')).toBe('0')
    }
  })

  it('draws the inbox’s state row as pills', () => {
    const body = ruleBody('.proposal-states .state')
    expect(declared(body, 'border-radius')).toBe('999px')
    expect(declared(body, 'background')).toBe('var(--surface)')
  })

  it('has no `.chips` left anywhere', () => {
    // `.chips` was the selector that reached both kinds. Its absence is the
    // fact, not its replacement: a `.chips` written again would draw the
    // inbox's three controls alike whatever the rules below say. Comments
    // are stripped first: `admin.css` and `library.css` both explain in
    // prose what `.chips` was, and should keep being able to.
    expect(withoutComments(cssSource)).not.toContain('.chips')
  })
})
