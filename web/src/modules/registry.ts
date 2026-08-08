import { buildRegistry } from '../module'
import { annotationsModule } from './annotations'
import { creditsModule } from './credits'
import { directionModule } from './direction'
import { motionModule } from './motion'
import { prefetchModule } from './prefetch'
import { readoutsModule } from './readouts'
import { regroupModule } from './regroup'
import { settingsModule } from './settings'
import { zoomModule } from './zoom'

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
 * `creditsModule` is the eighth, claiming `C` for the panel that lists a
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
 * **`creditsModule` was exactly that case for a while, and no longer is.**
 * Registration wired its preferences and nothing else: `App.tsx` rendered no
 * credits panel and its module switch had no case for `PANEL_INTENT`, so `C`
 * was swallowed by that branch's own `preventDefault()` and the generated
 * "Credit pages" section held two toggles that persisted and governed nothing.
 * That was deliberate and blocked on O25 (`docs/status.md`), which closed on
 * 2026-08-07; the panel and the skip landed with it. The entry would have
 * stayed in the array either way, because the property this comment is about —
 * one array, one hand-edit — is worth more than hiding an inert section, and
 * `modules/credits.ts`'s own doc said plainly what the section did meanwhile.
 * That is the shape to repeat for the next module registered ahead of its
 * behaviour, not an argument that this one is still waiting.
 *
 * `zoomModule` is the ninth, and lands in `creditsModule`'s earlier situation
 * rather than its current one: three keys declared (`=`, `-`, `0`) and none of
 * them do anything yet. This array is still the whole story for *existence* —
 * a `Zoom` section appears in the generated settings panel the moment it is
 * in this array, `enabled`, `maxScale` and `step` as ordinary controls and
 * `onSwap` as a choice, the same `defaults`-plus-`options` generation
 * `creditsModule` already exercises. What registration does not reach is the
 * same thing it never reached for `credits` while `credits` was inert:
 * `intentOf` resolves `=` to `zoom:in` correctly, but `App.tsx`'s
 * module-intent switch has no `case` for it yet, so the key is swallowed by
 * that branch's own `preventDefault()` and does nothing. Those cases are
 * Task 7 of the same milestone
 * (`docs/superpowers/plans/2026-08-07-l1-geometry.md`), not a later one.
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
  zoomModule,
]

export const registry = buildRegistry(modules)
