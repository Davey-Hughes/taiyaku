/**
 * Drops the `.woff` fallback from every `@font-face` the build emits.
 *
 * `@fontsource`'s CSS gives each face two sources, `url(….woff2)
 * format('woff2'), url(….woff) format('woff')`. A browser takes the first
 * source whose format it supports, and every browser that parses
 * `light-dark()` — which every colour token needs, so Safari 17.5 and later —
 * supports woff2, so the second is never fetched. Vite emits whatever a `url()`
 * names all the same: before this, 574 `.woff` files, 18,148,448 bytes of the
 * built `dist`, that no browser the UI supports would ask for. The user chose
 * to strip them on 2026-09-18.
 *
 * Wired in through `vite.config.ts`'s `css.postcss`. Vite runs its own
 * `@import` inlining before the plugins listed there and its `url()` rewriting
 * after them, so this sees `@fontsource`'s faces already inlined into
 * `tokens.css`, and a source it removes is never emitted. `stripWoff.test.ts`
 * runs it through Vite's own CSS pipeline.
 *
 * The types come through Vite's, because `postcss` is Vite's dependency and
 * not this package's.
 */
import type { CSSOptions } from 'vite'

type PostcssOptions = Exclude<NonNullable<CSSOptions['postcss']>, string>
type PostcssPlugin = NonNullable<PostcssOptions['plugins']>[number]

/** A source whose format hint is `woff` — not `woff2`. */
const WOFF = /\bformat\(\s*(['"]?)woff\1\s*\)/

/**
 * `src` without its `woff` sources. Split on the commas between sources, not
 * the ones inside a `url()` or a `local()`. A face whose every source is
 * `woff` keeps them, since a face with no source at all would never load.
 */
export function withoutWoff(src: string): string {
  const sources = src.split(/,(?![^(]*\))/).map((source) => source.trim())
  const kept = sources.filter((source) => !WOFF.test(source))
  return kept.length > 0 ? kept.join(', ') : src
}

export function stripWoff(): PostcssPlugin {
  return {
    postcssPlugin: 'taiyaku-strip-woff',
    AtRule: {
      'font-face': (face) => {
        face.walkDecls('src', (decl) => {
          decl.value = withoutWoff(decl.value)
        })
      },
    },
  }
}
