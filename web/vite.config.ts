import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
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
