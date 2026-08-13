import { buildRegistry } from '../module'
import { annotationsModule } from './annotations'
import { borrowModule } from './borrow'
import { creditsModule } from './credits'
import { directionModule } from './direction'
import { fitModule } from './fit'
import { helpModule } from './help'
import { layoutModule } from './layout'
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
 *
 * `fitModule` is the tenth, and claims no keys — the `prefetchModule`/
 * `readoutsModule` shape, tuned in settings rather than pressed mid-read.
 * Registration alone generates its "Fit" settings section, `upscaleToFit`
 * and `gap` as ordinary controls and `upscaleFilter` as a choice, the same
 * `defaults`-plus-`options` inference every module here gets for free. What
 * registration does not reach is again what it never reached for `credits`
 * while `credits` was inert: nothing yet calls `pageStyle`/`seamStyle`, so
 * the settings this section offers govern nothing until then. The difference
 * from `credits` is how long that lasts — Tasks 2 and 3 of the same plan
 * (`docs/superpowers/plans/2026-08-08-fit-seam-and-help.md`) wire `PageView`
 * and the seam layout through them in this same milestone, not a later one.
 *
 * `helpModule` is the eleventh, and it is the case `creditsModule`'s note
 * above asks for rather than a repeat of it: its two keys (`?` and `h`, one
 * intent between them) get their `case` in `App.tsx` and their overlay in the
 * same commit as this entry, so nothing here is inert for a while. Its
 * generated settings section is one `enabled` toggle. What this array does
 * *not* do for it is the reason the module is worth naming here at all: this
 * entry is also what puts every other module's keys into the overlay, because
 * `App.tsx` lists from `registry.all()` and not from a written list, so a
 * module registered below is documented to the reader by that registration
 * alone. A twelfth that claims a key needs no edit to `Help.tsx` to appear in
 * it — only a `label` on the binding if it would rather not be described by
 * its intent string.
 *
 * `layoutModule` is the twelfth and exists for one reason: the reading mode
 * was a plain `useState` and so was forgotten on every reload. Registering it
 * is the entire fix — this array is what makes a preference load, save and
 * reach the settings panel — and it is also why the reader still has exactly
 * one way of remembering anything about a person. A `loadPrefs('mode', …)` in
 * `App.tsx` would have been shorter and would have been the first stored value
 * that was not a module's.
 *
 * `borrowModule` is the thirteenth, and follows `helpModule`'s shape rather
 * than `credits`' earlier one: it claims no keys, its generated section is the
 * one `enabled` toggle, and `App.tsx` wraps `display` with its `withBorrow` in
 * the same commit as this entry, so nothing here is inert for a while. It is
 * also the first module whose behaviour is a *pure function over `Display`*
 * rather than a keypress or a style — which is why registration plus one
 * wrapped binding is the whole of it.
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
  fitModule,
  helpModule,
  layoutModule,
  borrowModule,
]

export const registry = buildRegistry(modules)
