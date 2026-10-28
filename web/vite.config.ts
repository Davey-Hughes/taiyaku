import { fileURLToPath } from 'node:url'
import { playwright } from '@vitest/browser-playwright'
// `vitest/config`'s `defineConfig`, not `vite`'s: the `test` key below is
// Vitest's extension to the Vite config, and `vite`'s own type does not know
// it — `tsc -b` typechecks this file, so the wrong import is a build failure
// rather than a silently ignored block.
import { defaultExclude, defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'
import { stripWoff } from './src/stripWoff.ts'

// taiyaku-wasm is a `link:` dependency, linked into node_modules from
// crates/taiyaku-wasm/pkg — a directory outside `web/`. Vite's dev-server
// filesystem allowlist otherwise refuses to serve the wasm asset from there,
// so the allowlist is widened to exactly that package directory rather than
// disabled. Resolved relative to this file, not hard-coded, since the repo is
// checked out at different paths on different machines.
const wasmPkgDir = fileURLToPath(new URL('../crates/taiyaku-wasm/pkg', import.meta.url))

// The repository's own licence, which `src/licenceTexts.ts` bundles for the
// licence page with `?raw`, is outside `web/` too, and is allowed as the one
// file so the allowlist reaches nothing else at the repository's root. **It
// is named twice, with and without the query**: Vite's check on a `?raw` id
// passes only when both the file and the id with its query are allowed, and
// an entry matches only its exact spelling, where a directory's matches
// everything beneath it, query and all. Without the second name the `jsdom`
// project refuses the import ("Denied ID").
const licenceFile = fileURLToPath(new URL('../LICENSE', import.meta.url))
const licenceAllow = [licenceFile, `${licenceFile}?raw`]

// The browser tests that need a scrollbar to take its width: see the
// `browser` project's `instances`.
const SHOWN_SCROLLBARS = 'src/**/*.scrollbars.browser.test.tsx'

// The reader is served from Vite in development and talks to `taiyaku serve`
// on another port. Proxying rather than enabling CORS on the server: CORS
// would be a production-facing change made for a development convenience, and
// `serve` has no browser-specific assumptions today — a constraint the design
// keeps deliberately, so a native shell can use the same API.
export default defineConfig({
  plugins: [react()],
  // `@fontsource`'s `.woff` fallbacks, removed before Vite emits them: every
  // browser that can draw the tokens loads the `.woff2` beside each one. See
  // `src/stripWoff.ts`.
  css: { postcss: { plugins: [stripWoff()] } },
  // **Three test projects, because three different questions need three
  // different environments.**
  //
  // `node` is every existing test: pure TypeScript with no DOM, which is why
  // `decode.ts` and friends were written to be importable without one. Fast,
  // and the overwhelming majority.
  //
  // `jsdom` is for components: a DOM without a layout engine, cheap enough
  // for CI to run on every push. See that project's own comment below for why
  // it is not simply folded into `browser`.
  //
  // `browser` exists for what neither of the other two can answer at all. The
  // defects this reader has actually shipped were visual — cell widths flapping
  // under still artwork, a pair tearing, a page drawn at the wrong size — and
  // `jsdom` has no layout engine, so `getBoundingClientRect` there returns
  // zeroes and none of it is observable. It also has no real
  // `HTMLImageElement.decode()`, which is the seam the whole page-turn
  // mechanism is built on. A real browser has both.
  //
  // Kept deliberately small: browser tests are slow and need a downloaded
  // Chromium, so they cover the properties nothing else can see rather than
  // duplicating what the other two projects already pin.
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'node',
          include: ['src/**/*.test.ts'],
          exclude: ['src/**/*.browser.test.*'],
          // Vitest's default `css: false` stubs every CSS import — `?raw`
          // included — to an empty string. The tests that read stylesheet
          // text rather than render it (jsdom resolves neither `light-dark()`
          // nor a composited `opacity`) need the real text, so CSS is
          // processed for every `.css` file under `src/` and nothing else —
          // not `css: true`. **Every** file there, not a list of the ones
          // read today: a stylesheet left off the list reaches
          // `colourLiterals.test.ts` as `''` and is counted as holding no
          // colour at all, which is why that test also refuses an empty
          // one.
          css: { include: [/\/src\/.+\.css(?:\?|$)/] },
        },
      },
      {
        extends: true,
        test: {
          // **Components, in jsdom rather than in the browser project.** The
          // browser project needs a downloaded Chromium that CI deliberately
          // does not carry, so anything measured there is invisible to the
          // gate and a developer's local number would exceed CI's. A gate has
          // to measure what CI can reproduce. What jsdom structurally cannot
          // see — real layout, real HTMLImageElement.decode() — stays in the
          // browser project, which is why both exist.
          name: 'jsdom',
          environment: 'jsdom',
          // The same flag the `browser` project sets. Without it React says
          // nothing about a state update made outside `act`, so a test making
          // one passes in silence — see `browser-setup.ts`'s own doc.
          setupFiles: ['./src/browser-setup.ts'],
          include: ['src/**/*.test.tsx'],
          exclude: ['src/**/*.browser.test.tsx'],
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
            // **Two launches of one Chromium.** Playwright starts headless
            // Chromium with `--hide-scrollbars`, so a scrollbar there takes
            // no width, and a layout that a scrollbar's width decides cannot
            // be seen. The files matching `SHOWN_SCROLLBARS` run only in the
            // second launch, which leaves that flag out; every other file
            // runs only in the first. `--project browser` runs both, so
            // `test:browser` and `scripts/check-all.sh` do. An instance's
            // `exclude` replaces the project's rather than adding to it, so
            // Vitest's own defaults are named again.
            instances: [
              {
                browser: 'chromium',
                exclude: [...defaultExclude, SHOWN_SCROLLBARS],
              },
              {
                browser: 'chromium',
                name: 'browser (chromium, scrollbars shown)',
                include: [SHOWN_SCROLLBARS],
                provider: playwright({
                  launchOptions: { ignoreDefaultArgs: ['--hide-scrollbars'] },
                }),
              },
            ],
          },
        },
      },
    ],
    // **`include` is set explicitly and that is the whole point.** v8 coverage
    // instruments what the run loaded, so a file no test imports is absent
    // from the report rather than sitting at 0%. Measured on this tree: the
    // default reports 98.43% over 15 files, while 50 are in the gate and the
    // real figure was 48.13% when this block landed. A gate built on the
    // default would enforce nothing while reading beautifully.
    //
    // Exclusions are named individually with a reason, never a wildcard: a
    // wildcard silently swallows the next file that matches it, which is the
    // exact failure this gate exists to prevent.
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts', 'src/**/*.tsx'],
      exclude: [
        'src/**/*.test.ts',
        'src/**/*.test.tsx',
        'src/**/*.d.ts',
        // Entry point: ten lines that call createRoot. Nothing to assert.
        'src/main.tsx',
        // The labeller's own entry point — the same shape as `main.tsx`
        // above, for the same reason: a handful of lines binding `Labeller`
        // to the real ports and calling `createRoot`, nothing to assert.
        'src/label.tsx',
        // The setup the `browser` and `jsdom` projects load, which sets the
        // act environment and empties the read store after each test: not
        // application code.
        'src/browser-setup.ts',
        // The shared mount/rerender helper: test harness, the same category
        // `browser-setup.ts` is excluded for. Counting it would put test
        // infrastructure inside an application-coverage number, and its
        // `rerender` half is reachable only from the `browser` project, which
        // this gate does not run — so it would sit permanently short of 100%
        // for a structural reason rather than an untested one.
        'src/testing/**',
      ],
      reporter: ['text', 'json-summary'],
      // **Set at the end, at what was achieved, minus two points.** Not chosen
      // up front: a threshold picked before the work is either unreachable or
      // vacuous. The measured figure sits beside each one so a later reader can
      // see the headroom without re-running anything, and the margin is small
      // enough that losing one covered suite still trips it — proven, not
      // assumed. See `docs/ci.md`, "The coverage gates".
      //
      // Two points, rather than one or five: v8 counts what the run loaded, so
      // an unrelated change to which files a test imports moves these by
      // fractions. Five would let a whole module regress in silence.
      //
      // Read these from `coverage/coverage-summary.json`, not from the text
      // table — the table omits every file at 100% on all four metrics, and
      // showed 18 rows against the JSON's 57 files when these were set. The
      // report holds 115 files as of 2026-09-19 (counted from the JSON's own
      // keys, excluding `total`; was 109 on 2026-09-12).
      //
      // **The margins narrow on their own, and `branches` proved it.** Nothing
      // regressed: the 66 files that existed on 2026-08-14 still measured
      // 93.96% a month later, while the 42 that arrived in between measured
      // 90.91% — and a tree diluted three points below its own rate had taken
      // `branches` from 2.38 of margin to **0.80** without a single gate run
      // going red. O103 put it back, and the sentence this comment has always
      // carried is why it was put back rather than legislated away: **the
      // number to move when this fires is the coverage, not the bar.** What
      // moved the coverage was mostly *removing* branches — forty copies of
      // `err instanceof Error ? err.message : String(err)` became one
      // `messageOf` in `src/message.ts` — plus nine tests for the stale-answer
      // guards no test had ever reached. `docs/ci.md` has the arithmetic.
      thresholds: {
        statements: 96, // measured 97.72, was 98.03 when set
        branches: 92, // measured 94.05, was 94.38 when set
        functions: 97, // measured 99.16, was 99.29 when set
        lines: 97, // measured 99.31, was 99.53 when set
        // **These are aggregate, and an aggregate cannot see one module go
        // dark.** Measured: skipping the whole of `pairing.test.ts` drops
        // `pairing.ts` from 100% to 83.58% and the total only 98.03% → 97.49%,
        // because `App.test.tsx` exercises it transitively through a mounted
        // reader. Both thresholds still passed. `scripts/coverage-floor.mjs` is
        // the other half of this gate and catches exactly that; vitest cannot
        // express both here — a glob entry is aggregated over its matches, and
        // `perFile` inside one is ignored, both tried and measured.
      },
    },
  },
  build: {
    // **The stylesheet is built for the browsers the UI supports, so
    // `light-dark()` ships as written (O218).** Left alone, `cssTarget`
    // follows `build.target`, whose default in Vite 8 is Chrome and Edge 111,
    // Firefox 114 and Safari 16.4 — all older than `light-dark()`. For those
    // Lightning CSS compiles each paired token to
    // `var(--lightningcss-light, <paper>) var(--lightningcss-dark, <ink>)`,
    // and a `var()` is substituted where the token is declared, on `:root`.
    // `.reader` then inherits tokens already resolved for the page's theme,
    // whatever its own `color-scheme` says, and a reader in a light theme
    // drew its bars from the Paper half.
    //
    // These five are the first versions Lightning CSS leaves the function
    // alone for, read one version either side on 2026-10-12. The UI needs
    // them already, since the themed colour tokens are `light-dark()` pairs:
    // spec 2026-09-17 §3 gives the Safari figure. `build.target`, which the
    // scripts are built for, is not set.
    // `src/builtStylesheet.test.ts` builds the stylesheet and holds it.
    cssTarget: ['chrome123', 'edge123', 'firefox120', 'safari17.5', 'ios17.5'],
    rollupOptions: {
      // Two entries. Without this Vite takes index.html alone and the
      // labeller silently does not ship -- there is no error, just a missing
      // page.
      input: {
        main: fileURLToPath(new URL('index.html', import.meta.url)),
        label: fileURLToPath(new URL('label.html', import.meta.url)),
      },
    },
  },
  // **Device passes only.** `vite preview` serves the production build, and
  // `allowedHosts` is what lets it answer a request whose `Host` is this
  // machine's tailnet name rather than localhost — which is how an iPhone
  // reaches it, and iOS wants TLS, which `tailscale serve` terminates in
  // front of this. Named exactly rather than `true`: the check exists to stop
  // DNS rebinding, and one hostname keeps that protection everywhere else.
  // Nothing in production uses `vite preview`.
  preview: {
    allowedHosts: ['davey-endeavor.walrus-census.ts.net'],
  },
  server: {
    fs: {
      allow: ['.', wasmPkgDir, ...licenceAllow],
    },
    // **`changeOrigin: false`, and that is a decision measured rather than a
    // default left alone.** With it `true`, the proxy rewrites `Host` to the
    // target while the browser's `Origin` stays `localhost:5173` — so spec
    // A2's origin check compares `localhost:5173` against `127.0.0.1:8080`,
    // and every login through `pnpm dev` answered **403**. Measured
    // 2026-08-18; that spec's §14 recorded this as unverified and said which
    // way to fix it if it misbehaved:
    //
    // > If it misbehaves, the fix is in the dev proxy and not in the cookie
    // > policy — a production cookie must not be weakened for a development
    // > convenience.
    //
    // So the proxy forwards the `Host` the browser sent, which is the one its
    // `Origin` names. Nothing here needs the rewrite: the target is this
    // project's own `serve`, which routes on the path and reads `Host` only to
    // compare it against `Origin`.
    proxy: {
      '/api': { target: process.env.TAIYAKU_SERVE ?? 'http://127.0.0.1:8080', changeOrigin: false },
      '/opds': { target: process.env.TAIYAKU_SERVE ?? 'http://127.0.0.1:8080', changeOrigin: false },
    },
  },
})
