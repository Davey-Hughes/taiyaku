import { describe, expect, it } from 'vitest'
// `?raw` rather than `node:fs`: this app's tsconfig only carries
// `vite/client`'s types (no `"node"` in `types`), and every other test in
// this tree reads source the same way Vite serves it rather than reaching
// past the bundler for a Node API nothing else here uses.
import loginSource from './Login.tsx?raw'

/**
 * A narrow regression guard for the actual defect: `Login.tsx` hardcoded a
 * dark palette (`#111`, `#ffb4a8`, ...) that could not follow the reader's
 * colour scheme, discovered because `Login` used to appear only over the
 * reader's own fixed-dark chrome and now fronts the library instead, which
 * follows `color-scheme: light dark`. See the comment above `overlayStyle`
 * in `Login.tsx`.
 *
 * **What this proves:** no hex colour literal sits in `Login.tsx`'s code
 * outside the one deliberate `light-dark(...)` pair used for the error text
 * — chosen, and its contrast against both canvases computed, in the comment
 * above `errorStyle`. That is the exact shape the original defect took:
 * hex/rgba literals standing in for what should have been a system colour or
 * a scheme-aware pair.
 *
 * **What this does not prove:** that the screen renders correctly, or even
 * legibly, in either scheme. jsdom does not resolve CSS system colours or
 * `light-dark()` to real pixels — a test asserting "the background is white
 * in light mode" here would be asserting a fiction, so this does not attempt
 * one. It also would not catch a *non-hex* scheme-blind literal reintroduced
 * as a background or border (an `rgba(...)` grey, or a named colour like
 * `black`) — only a hardcoded hex, which is what broke here and is the one
 * shape worth mechanically ruling out.
 */
describe('Login colours', () => {
  it('hardcodes no colour outside the one deliberate light/dark pair', () => {
    const withoutComments = loginSource
      .replace(/\/\*[\s\S]*?\*\//g, '')
      .replace(/\/\/.*$/gm, '')
    const withoutLightDark = withoutComments.replace(/light-dark\([^)]*\)/g, '')
    expect(withoutLightDark).not.toMatch(/#[0-9a-fA-F]{3,8}\b/)
  })
})
