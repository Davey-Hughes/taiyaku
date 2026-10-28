# web

A Vite + React + TypeScript app: a library grid over spec C1's browse API, and
a reader that swaps between two editions of the same manga volume, page by
page.

## Building and testing

From this directory:

```sh
pnpm install --frozen-lockfile
pnpm run build      # tsc -b && vite build
pnpm run typecheck  # tsc -b, no bundling — what CI runs
pnpm run test       # vitest run --project node: the node project alone
```

From the repository root, the same commands take `-C web` — which is the form
`scripts/check-all.sh` and the CI workflow use, since they run from there.

The tests are in `tests/`, which mirrors `src/`: `src/reads.ts` is tested in
`tests/reads.test.ts`, and the helpers only tests use are in `tests/support/`.
A file's suffix picks the vitest project that runs it (`vite.config.ts`), and
`tests/srcHoldsNoTests.test.ts` fails if a test file is left in `src/`, where
none of them would find it.

`pnpm run dev` proxies `/api` and `/opds` to `taiyaku serve` (default
`http://127.0.0.1:8080`, override with `TAIYAKU_SERVE`) — see `vite.config.ts`.

## Routes

`Routes.tsx`'s `AppRoutes`, over `react-router` — the first runtime dependency
beyond React since the project began, spec C1 §1:

| Route | Renders |
| ----- | ------- |
| `/` | the library grid — `Library.tsx`: a continue strip, three filter chips and an `added` sort, all client-side over `library.ts`'s `/api/library/*` client, under a search bar. `?library=N` narrows the grid and the search to one library. The search bar is `SearchBar.tsx` over `searchHits.ts`: `?q=` in the URL, debounced through `debounce.ts` at the reader's own 250 ms, and while it holds a query its hits replace the grid and the chips, the sort and the continue strip go. Migration 8 tokenises both FTS5 indexes `trigram`, answering any query of three characters or more in either script; `browse::search`'s `LIKE` path answers the two `trigram` cannot. The grid and the hits are both `SeriesGrid.tsx` |
| `/series/:id` | one series and its volumes — `Series.tsx`: a grid or a table layout, an edition selector that dims a volume rather than removing it, and an optional `i` panel naming a verdict and a paired count from `/api/library/works/{slug}` |
| `/search` | a redirect — `Search.tsx`: to `/` with its `q` and `library`, so an old search link lands on the search bar with its hits |
| `/read/:work` | the reader (`App.tsx`), keyed by the slug in the URL so a swap between works remounts rather than trying to stay one component |

`Sidebar.tsx` draws the Libraries heading and list; `Shell.tsx` puts the build's label (`VersionLabel.tsx`), More (`AccountMenu.tsx`) and the theme switch at its foot, in that order, wraps every screen but the reader and the `/search` redirect, and owns the one status fetch they need. The browse preferences are behind the View button on the Library view and on a series page (`ViewMenu.tsx`), generated from `browseModule.ts` exactly as `Settings.tsx` is generated from the reader's module registry.

`/proposals`, `/config/*` and `/account` are the proposals inbox, the configuration screens and the account screen (`Proposals.tsx`, `Config.tsx`, `Account.tsx`), each in the shell. Any other path redirects to `/`.

**Every route sits under one sign-in gate**, `Gate.tsx`, which `main.tsx` wraps
around `AppRoutes`. It asks `/api/me` once per page load, draws nothing until
the answer arrives, and renders `Login.tsx` instead of the routes when the
answer is a 401 — spec C1 §5. It is also the single registration of `api.ts`'s
`whenSignedOut`, which is a module-level slot, so a 401 arriving from *any*
later call — including the writes whose own `.catch` swallows the rejection —
lands on the same screen. Signing out unmounts everything below it rather than
resetting state by hand.

That gate is above the router rather than inside a screen, and the reason is a
regression it was written to close: sign-in lived inside `App` until 2026-08-21,
so once `/` became the library, a signed-out visitor reached a grid whose loads
401'd into an alert banner reading "not signed in", with no login screen
anywhere they could get to.

`gate.ts` holds the context and `useGate()`, split out from the component
because `useComponentExportOnlyModules` refuses a file that exports both.

## Signing in against a development server

Every route needs a credential since spec A2, so `serve` needs an account before
the reader can load anything:

```sh
taiyaku library user promote davey --db /tmp/dev.db   # prompts twice
taiyaku serve some/taiyaku.json --addr 127.0.0.1:8080 --db /tmp/dev.db
pnpm run dev                                          # then sign in at :5173
```

**The proxy sets `changeOrigin: false`, and that is load-bearing rather than a
default nobody touched.** With it `true` the proxy rewrites `Host` to the target
while the browser's `Origin` stays `localhost:5173`, so the server's CSRF origin
check compares two different authorities and **every login answers 403**.
Measured on 2026-08-18; the fix is in the proxy rather than in the cookie policy,
because a production cookie must not be weakened for a development convenience.
See `docs/notes.md`, "Spec A2's measurements".

The session cookie carries no `Secure` here, correctly: nothing configures
`--trusted-proxy`, so it has to survive a plain-HTTP hop to `localhost`.

## The WASM dependency

`taiyaku-wasm` is a `link:` dependency on `../crates/taiyaku-wasm/pkg`, which
is generated by `./scripts/build-wasm.sh` and gitignored. Unlike npm's `file:`,
which this project used before, `link:` tolerates `pkg/` not existing yet:
`pnpm install --frozen-lockfile` still succeeds against a fresh clone,
creating a dangling symlink rather than failing or copying stale content. See
`docs/ci.md`, "Running the compiled module in CI", for why `link:` replaced
`file:` and what the latter did instead.

This is also why CI does not need a Rust or wasm toolchain: `resolver.ts` is
the only file that imports `taiyaku-wasm` as a *value*, and no test loads the
binding. `resolver.test.ts` mocks `taiyaku-wasm` before it imports
`resolver.ts`, and the tests that would reach it through `App.tsx` mock
`resolver.ts` or `App.tsx` itself. `pnpm run typecheck` does still open
`resolver.ts` — other modules import its *types* — but its one `taiyaku-wasm`
import is marked `@ts-ignore` for exactly the case where `pkg/` doesn't exist
yet, so a missing package degrades the typecheck rather than failing it. See
`docs/ci.md` for how the install and `pnpm run typecheck` behave
against a clone where `pkg/` does not exist.
