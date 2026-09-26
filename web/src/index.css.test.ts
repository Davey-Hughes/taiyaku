import { describe, expect, it } from 'vitest'
import adminSource from './styles/admin.css?raw'
import baseSource from './styles/base.css?raw'
import librarySource from './styles/library.css?raw'
import shellSource from './styles/shell.css?raw'
import tokensSource from './styles/tokens.css?raw'
import { rules, selectors, subject, withoutComments } from './testing/css'

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

describe('the token model', () => {
  it('reproduces four ratios the spec computed outside this file', () => {
    expect(contrast(token('text', 'paper'), token('bg', 'paper'))).toBeCloseTo(
      14.3,
      2,
    )
    expect(
      contrast(token('muted', 'paper'), token('surface', 'paper')),
    ).toBeCloseTo(4.71, 2)
    expect(
      contrast(token('line-strong', 'ink'), token('bg', 'ink')),
    ).toBeCloseTo(3.58, 2)
    expect(contrast(token('accent', 'ink'), token('bg', 'ink'))).toBeCloseTo(
      6.13,
      2,
    )
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

const BLACK: Rgb = [0, 0, 0]
const WHITE: Rgb = [255, 255, 255]

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

  it('reproduce two ratios the spec computed outside this file', () => {
    const { rgb, alpha } = readerToken('reader-panel')
    expect(
      contrast(token('text', 'ink'), composite(rgb, BLACK, alpha)),
    ).toBeCloseTo(14.86, 2)
    expect(
      contrast(token('muted', 'ink'), composite(rgb, WHITE, alpha)),
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
        (text) => [text, contrast(token(text, 'ink'), panel)] as const,
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
 * The sidebar's section headings — Roots, Library, Theme. They were faded with
 * `opacity: 0.65`, which has no floor anyone checks; spec 2026-09-17 §7.1
 * gives them `--muted` instead, on the page's own ground.
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
