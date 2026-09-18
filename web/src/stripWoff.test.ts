import { preprocessCSS, resolveConfig } from 'vite'
import { describe, expect, it } from 'vitest'
import { stripWoff } from './stripWoff'

/**
 * The plugin run as the build runs it: through Vite's own CSS pipeline, with
 * nothing but it in `css.postcss`, over a literal stylesheet. `@fontsource`'s
 * own files are not read here — the `node` project does not process CSS
 * outside `src/` — so the face below copies their shape; the build is what
 * shows the real ones lose their `.woff` (`docs/notes.md`, the style pass).
 */
async function built(css: string): Promise<string> {
  const config = await resolveConfig(
    {
      configFile: false,
      logLevel: 'silent',
      css: { postcss: { plugins: [stripWoff()] } },
    },
    'build',
  )
  return (await preprocessCSS(css, 'probe.css', config)).code
}

const FACE = `@font-face {
  font-family: 'Zen Kaku Gothic New';
  font-weight: 400;
  src: url(./files/zen-kaku-gothic-new-latin-400-normal.woff2) format('woff2'), url(./files/zen-kaku-gothic-new-latin-400-normal.woff) format('woff');
  unicode-range: U+0000-00FF;
}`

describe('stripWoff', () => {
  it('drops a face’s .woff source and keeps its .woff2 one', async () => {
    expect(await built(FACE)).toBe(`@font-face {
  font-family: 'Zen Kaku Gothic New';
  font-weight: 400;
  src: url(./files/zen-kaku-gothic-new-latin-400-normal.woff2) format('woff2');
  unicode-range: U+0000-00FF;
}`)
  })

  it('drops a double-quoted .woff source even when it is listed first', async () => {
    // Two ways a narrower rule would still pass the test above: matching only
    // a single-quoted `format('woff')`, since `FACE` never uses double quotes;
    // and dropping only a *trailing* `.woff` source, since `FACE` never lists
    // one first. This face is double-quoted and lists its `.woff` source
    // before its `.woff2`, so either shortcut leaves it behind.
    const face = `@font-face {
  font-family: 'taiyaku figures';
  src: url(./d.woff) format("woff"), url(./d.woff2) format('woff2');
}`
    expect(await built(face)).toBe(`@font-face {
  font-family: 'taiyaku figures';
  src: url(./d.woff2) format('woff2');
}`)
  })

  it('leaves a rule that is not a font face alone, even one naming a .woff', async () => {
    const rule = `.probe {
  src: url(./a.woff2) format('woff2'), url(./a.woff) format('woff');
  background: url(./b.woff);
}`
    expect(await built(rule)).toBe(rule)
  })

  it('leaves a face with no .woff source, or nothing else, as it was', async () => {
    const faces = `@font-face {
  font-family: "taiyaku figures";
  src: local("Helvetica Neue"), local("Arial");
}
@font-face {
  font-family: "only woff";
  src: url(./c.woff) format("woff");
}`
    expect(await built(faces)).toBe(faces)
  })
})
