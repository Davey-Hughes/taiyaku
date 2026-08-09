import { fileURLToPath } from 'node:url'
import { playwright } from '@vitest/browser-playwright'
// `vitest/config`'s `defineConfig`, not `vite`'s: the `test` key below is
// Vitest's extension to the Vite config, and `vite`'s own type does not know
// it — `tsc -b` typechecks this file, so the wrong import is a build failure
// rather than a silently ignored block.
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

// taiyaku-wasm is a `file:` dependency, linked into node_modules from
// crates/taiyaku-wasm/pkg — a directory outside `web/`. Vite's dev-server
// filesystem allowlist otherwise refuses to serve the wasm asset from there,
// so the allowlist is widened to exactly that package directory rather than
// disabled. Resolved relative to this file, not hard-coded, since the repo is
// checked out at different paths on different machines.
const wasmPkgDir = fileURLToPath(new URL('../crates/taiyaku-wasm/pkg', import.meta.url))

// The reader is served from Vite in development and talks to `taiyaku serve`
// on another port. Proxying rather than enabling CORS on the server: CORS
// would be a production-facing change made for a development convenience, and
// `serve` has no browser-specific assumptions today — a constraint the design
// keeps deliberately, so a native shell can use the same API.
export default defineConfig({
  plugins: [react()],
  // **Two test projects, because two different questions need two different
  // environments.**
  //
  // `node` is every existing test: pure TypeScript with no DOM, which is why
  // `decode.ts` and friends were written to be importable without one. Fast,
  // and the overwhelming majority.
  //
  // `browser` exists for what a Node environment cannot answer at all. The
  // defects this reader has actually shipped were visual — cell widths flapping
  // under still artwork, a pair tearing, a page drawn at the wrong size — and
  // `jsdom` has no layout engine, so `getBoundingClientRect` there returns
  // zeroes and none of it is observable. It also has no real
  // `HTMLImageElement.decode()`, which is the seam the whole page-turn
  // mechanism is built on. A real browser has both.
  //
  // Kept deliberately small: browser tests are slow and need a downloaded
  // Chromium, so they cover the properties nothing else can see rather than
  // duplicating what the `node` project already pins.
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'node',
          include: ['src/**/*.test.ts'],
          exclude: ['src/**/*.browser.test.*'],
        },
      },
      {
        extends: true,
        test: {
          name: 'browser',
          include: ['src/**/*.browser.test.tsx'],
          setupFiles: ['./src/browser-setup.ts'],
          browser: {
            enabled: true,
            // A factory, not a string: `browser.provider` changed shape in
            // Vitest 4.1 and the string form is refused outright.
            provider: playwright(),
            headless: true,
            instances: [{ browser: 'chromium' }],
          },
        },
      },
    ],
  },
  server: {
    fs: {
      allow: ['.', wasmPkgDir],
    },
    proxy: {
      '/api': { target: process.env.TAIYAKU_SERVE ?? 'http://127.0.0.1:8080', changeOrigin: true },
      '/opds': { target: process.env.TAIYAKU_SERVE ?? 'http://127.0.0.1:8080', changeOrigin: true },
    },
  },
})
