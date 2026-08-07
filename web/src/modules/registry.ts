import { buildRegistry } from '../module'
import { annotationsModule } from './annotations'
import { creditsModule } from './credits'
import { directionModule } from './direction'
import { motionModule } from './motion'
import { prefetchModule } from './prefetch'
import { readoutsModule } from './readouts'
import { regroupModule } from './regroup'
import { settingsModule } from './settings'

/**
 * Every reading behaviour this build ships, declared once. `motionModule`
 * joins `directionModule` here and nothing else changes: it claims no keys,
 * so it never appears in `intentOf`'s dispatch, but it still goes through
 * `buildRegistry` like every other module — see `duplicateIds`'s check,
 * which a keyless module is just as capable of tripping as a keyed one.
 * `prefetchModule` and `readoutsModule` join the same way, for the same
 * reason: both claim no keys — tuned in settings, not pressed mid-read.
 * `settingsModule` claims one key (`s`) for the panel and nothing else, so
 * `Settings.tsx`'s generated panel renders no section for it — see that
 * module's own doc. `annotationsModule` is the first to claim more than one
 * key (`n`, `r`, `a`), which the contract already allowed and nothing here
 * had exercised; `buildRegistry`'s collision check treats them as three
 * separate claims, so a later module taking any one of them fails at import
 * rather than fighting for it at runtime.
 *
 * Kept in this file rather than in `App.tsx`, and built at module scope
 * rather than inside a component, for two reasons at once. First, the
 * original one: `buildRegistry`'s startup check runs the moment this file is
 * imported — before any component mounts, not on a first render that could
 * be swallowed by an error boundary. A collision or a duplicate id is a
 * contract violation that must fail loudly, per `module.ts`'s own doc
 * comment on why declaring instead of self-registering is the point.
 * Second: `App.tsx` value-imports `resolver.ts` and is therefore
 * permanently Vitest-unreachable (see that file's own comment), but nothing
 * about the registry itself needs to be — this file imports only module
 * contracts, so a test can build a registry, including a throwaway extra
 * module, and check it against `settingsPanel.ts`'s pure inference without
 * ever touching `App.tsx`.
 *
 * This is also the one place a seventh module is registered at all — adding
 * one to the array below is the only hand-edit its *existence* requires.
 * `creditsModule` is the eighth, claiming `C` for the panel that will list a
 * scanlation group's own pages — one key, the same shape as `settingsModule`'s
 * single claim, and nothing more here than that entry and the import above.
 * Making its preferences load, save, and reach the settings panel needs no
 * edit here or in `App.tsx`: `App.tsx`'s preference state is a single map
 * built from `registry.all()`, keyed by id, so a module with a `defaults`
 * shape and nothing else is fully wired the moment it appears in this array.
 * Only a module that also wants a keybinding (like `direction`'s `d`) or
 * bespoke behaviour (like `prefetch`'s decode-queue rebuild) needs anything
 * hand-written in `App.tsx`, and that is behaviour, not tunability.
 *
 * **`creditsModule` is exactly that case, and its behaviour is not written
 * yet — the panel is not mounted.** Registration wires its preferences and
 * nothing else: `App.tsx` renders no credits panel and its module switch has
 * no case for `PANEL_INTENT`, so `C` is swallowed by that branch's own
 * `preventDefault()` and the generated "Credit pages" settings section holds
 * two toggles that persist and govern nothing. This is deliberate and blocked
 * on O25 (`docs/status.md`) — the whole chrome and every panel below the
 * `shown.kind === 'gap'` branch go unrendered while a gap is showing, and a
 * gap is the state a reader most wants this list from. The entry stays in the
 * array regardless, because the property this comment is about — one array,
 * one hand-edit — is worth more than hiding an inert section, and
 * `modules/credits.ts`'s own doc says plainly what the section does today.
 */
const modules = [
  directionModule,
  motionModule,
  regroupModule,
  readoutsModule,
  prefetchModule,
  annotationsModule,
  settingsModule,
  creditsModule,
]

export const registry = buildRegistry(modules)
