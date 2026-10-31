import { fileURLToPath } from 'node:url'
import type { Rolldown } from 'vite'
import { build } from 'vite'
import { beforeAll, describe, expect, it } from 'vitest'
import tokensSource from '../src/styles/tokens.css?raw'
import { rules, selectors } from './support/css'

/**
 * **The stylesheet as the build ships it, not as it is written (O218).**
 * The source is what the dev server and the test projects load, and it is
 * not what ships: the build passes it through Lightning CSS first, and for a
 * browser target older than `light-dark()` that rewrites each paired token to
 * `var(--lightningcss-light, <paper>) var(--lightningcss-dark, <ink>)` —
 * resolved on `:root`, where the tokens are declared, so `.reader`'s
 * `color-scheme: dark` would reach nothing and a reader in a light theme
 * would draw its bars from the Paper half. `vite.config.ts`'s
 * `build.cssTarget` is what keeps the pairs as written.
 *
 * Built here from `index.css` alone, with the project's own config, so the
 * build loads the stylesheets and none of the app's scripts: CI's `web` job
 * has no wasm package. The stylesheet this emits is the one the app's
 * build emits.
 */

const WEB = fileURLToPath(new URL('..', import.meta.url))

let built = ''

beforeAll(async () => {
  const result = await build({
    root: WEB,
    configFile: `${WEB}vite.config.ts`,
    logLevel: 'silent',
    build: {
      write: false,
      rolldownOptions: { input: `${WEB}src/index.css` },
    },
  })
  // An array per environment, or one output; a watcher only under `watch`.
  const outputs = ([result].flat() as Rolldown.RolldownOutput[]).flatMap(
    (o) => o.output,
  )
  const sheets = outputs.filter(
    (o): o is Rolldown.OutputAsset =>
      o.type === 'asset' && o.fileName.endsWith('.css'),
  )
  expect(sheets.map((s) => s.fileName)).toHaveLength(1)
  built = String(sheets[0]?.source)
}, 60_000)

/**
 * Every rule `tokens.css` writes `light-dark()` pairs in, by its selector
 * list, with the tokens it pairs: `:root`, which declares the palette, and
 * the reader's roots, which pin the tokens the Ink lift changed back to the
 * pairs the reader was designed with (spec 2026-10-18 §3).
 */
const PAIRING_RULES = new Map(
  rules(tokensSource)
    .map(({ prelude, body }): [string, string[]] => [
      selectors(prelude).join(', '),
      [...body.matchAll(/(?:^|;)\s*(--[\w-]+):\s*light-dark\(/g)].map(
        (m) => m[1] ?? '',
      ),
    ])
    .filter(([, tokens]) => tokens.length > 0),
)

const ROOT = ':root'
const READER = '.reader-route, .reader'

/** Every token `:root` writes as a `light-dark()` pair. */
const PAIRED = PAIRING_RULES.get(ROOT) ?? []

/**
 * Every declaration of `token` in a rule of the built stylesheet that
 * `rules()` reads, as the selector list of the rule holding it and the value
 * it is given; the case below counts the rest. `:` after the name, so
 * `--line` does not match `--line-strong`.
 */
function declarations(token: string): Array<{ where: string; value: string }> {
  return rules(built).flatMap(({ prelude, body }) =>
    [...body.matchAll(new RegExp(`(?:^|;)\\s*${token}:([^;]*)`, 'g'))].map(
      (m) => ({ where: selectors(prelude).join(', '), value: m[1] ?? '' }),
    ),
  )
}

describe('the built stylesheet', () => {
  it('reads the paired tokens from tokens.css, by the rule that pairs them', () => {
    // A guard on the lists the next test walks: a pattern that matched
    // nothing would leave it passing over no tokens at all.
    expect([...PAIRING_RULES.keys()]).toEqual([ROOT, READER])
    expect(PAIRED).toHaveLength(13)
    expect(PAIRED).toContain('--line')
    expect(PAIRED).toContain('--text')
    const pinned = PAIRING_RULES.get(READER) ?? []
    expect(pinned).toHaveLength(7)
    expect(pinned.filter((token) => !PAIRED.includes(token))).toEqual([])
  })

  it.each(PAIRED)(
    'declares %s as the light-dark() pair it is written as, once in each rule that writes it',
    (token) => {
      // Where the build put each declaration, against where the source
      // wrote one: a second declaration in one rule, or one in a rule the
      // source does not pair it in, changes this list. The reader's pin is
      // the one second rule allowed, and only for the tokens it writes.
      const found = declarations(token)
      // `rules()` reads only a block with no block inside it, so a
      // declaration in a rule that nests another is not in `found`. Every
      // `<token>:` in the whole text is, wherever the build put it.
      const everywhere = built.match(new RegExp(`(?:^|[{;\\s])${token}:`, 'g'))
      expect(found).toHaveLength(everywhere?.length ?? 0)
      const written = [...PAIRING_RULES]
        .filter(([, tokens]) => tokens.includes(token))
        .map(([where]) => where)
      expect(found.map(({ where }) => where)).toEqual(written)
      for (const { value } of found) expect(value).toMatch(/^light-dark\(/)
    },
  )

  it('gives no rule the switch properties a compiled pair reads', () => {
    // The matches, not the stylesheet: a failure prints what was found.
    expect(built.match(/--lightningcss-(?:light|dark)/g) ?? []).toEqual([])
  })
})
