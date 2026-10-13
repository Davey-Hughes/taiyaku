import { fileURLToPath } from 'node:url'
import type { Rolldown } from 'vite'
import { build } from 'vite'
import { beforeAll, describe, expect, it } from 'vitest'
import tokensSource from './styles/tokens.css?raw'
import { withoutComments } from './testing/css'

/**
 * **The stylesheet as the build ships it, not as it is written (O218).**
 * The source is what the dev server and the test projects load, and it is
 * not what ships: the build passes it through Lightning CSS first, and for a
 * browser target older than `light-dark()` that rewrites each paired token to
 * `var(--lightningcss-light, <paper>) var(--lightningcss-dark, <ink>)` —
 * resolved on `:root`, where the tokens are declared, so `.reader`'s
 * `color-scheme: dark` reached nothing and a reader in a light theme drew
 * its bars from the Paper half. `vite.config.ts`'s `build.cssTarget` is what
 * keeps the pairs as written.
 *
 * Built here from `index.css` alone, with the project's own config, so the
 * build loads the stylesheets and none of the app's scripts: CI's `web` job
 * has no wasm package. The stylesheet this emits is the one the app's build
 * emits — the same content hash, read on 2026-10-12.
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

/** Every token `tokens.css` writes as a `light-dark()` pair. */
const PAIRED = [
  ...withoutComments(tokensSource).matchAll(/(--[\w-]+):\s*light-dark\(/g),
].map((m) => m[1] ?? '')

describe('the built stylesheet', () => {
  it('reads the thirteen paired tokens from tokens.css', () => {
    // A guard on the list the next test walks: a pattern that matched
    // nothing would leave it passing over no tokens at all.
    expect(PAIRED).toHaveLength(13)
    expect(PAIRED).toContain('--line')
    expect(PAIRED).toContain('--text')
  })

  it.each(PAIRED)(
    'declares %s as the light-dark() pair it is written as',
    (token) => {
      // Every declaration of the token, wherever the build put it. `:` after
      // the name, so `--line` does not match `--line-strong`.
      const values = [
        ...built.matchAll(new RegExp(`(?:^|[{;])${token}:([^;}]*)`, 'g')),
      ].map((m) => m[1])
      expect(values).toHaveLength(1)
      expect(values[0]).toMatch(/^light-dark\(/)
    },
  )

  it('gives no rule the switch properties a compiled pair reads', () => {
    // The matches, not the stylesheet: a failure prints what was found
    // rather than 600 kB of CSS.
    expect(built.match(/--lightningcss-(?:light|dark)/g) ?? []).toEqual([])
  })
})
