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
 * Every reading behaviour this build ships, declared once. A module that
 * claims no keys (`motionModule`, `prefetchModule`, `readoutsModule`,
 * `fitModule` and `borrowModule` among them: tuned in settings, not pressed
 * mid-read) never appears in `intentOf`'s dispatch, but it still goes through
 * `buildRegistry` like every other module — see `duplicateIds`'s check, which
 * a keyless module is just as capable of tripping as a keyed one.
 * `settingsModule` claims one key (`s`) for the panel and nothing else, so
 * `Settings.tsx`'s generated panel renders no section for it — see that
 * module's own doc. `creditsModule` claims `C` for the panel that lists a
 * scanlation group's own pages — one key, the same shape as `settingsModule`'s
 * single claim. `annotationsModule` claims more than one key (`n`, `r`, `a`,
 * `z`, `Z`); `buildRegistry`'s collision check treats them as separate
 * claims, so a later module taking any one of them fails at import rather
 * than fighting for it at runtime.
 *
 * Kept in this file rather than in `App.tsx`, and built at module scope
 * rather than inside a component, for two reasons at once. First:
 * `buildRegistry`'s startup check runs the moment this file is imported —
 * before any component mounts, not on a first render that could be swallowed
 * by an error boundary. A collision or a duplicate id is a contract violation
 * that must fail loudly, per `module.ts`'s own doc comment on why declaring
 * instead of self-registering is the point. Second: `App.tsx` value-imports
 * `resolver.ts`, so a test reaches it only by mocking that module (see that
 * file's own comment), but nothing about the registry itself needs that —
 * this file imports only module contracts, so a test can build a registry,
 * including a throwaway extra module, and check it against
 * `settingsPanel.ts`'s pure inference without ever touching `App.tsx`.
 *
 * This is also the one place a module is registered at all — adding one to the
 * array below is the only hand-edit its *existence* requires. Making a
 * module's preferences load, save, and reach the settings panel needs no edit
 * here or in `App.tsx`: `App.tsx`'s preference state is a single map built from
 * `registry.all()`, keyed by id, so a module with a `defaults` shape and
 * nothing else is fully wired the moment it appears in this array. Only a
 * module that also wants a keybinding (like `direction`'s `d`) or bespoke
 * behaviour (like `prefetch`'s decode-queue rebuild) needs anything
 * hand-written in `App.tsx`, and that is behaviour, not tunability.
 *
 * A module registered ahead of its behaviour still gets its settings section
 * from registration alone, so for a while that section can govern nothing. The
 * entry goes in the array anyway, because the property this comment is about —
 * one array, one hand-edit — is worth more than hiding an inert section, and
 * the module's own doc says plainly what the section does meanwhile.
 *
 * `helpModule` claims two keys (`?` and `h`, one intent between them), and its
 * generated settings section is one `enabled` toggle. This entry is also what
 * puts every other module's keys into its overlay, because `App.tsx` lists
 * from `registry.all()` and not from a written list, so a module registered
 * below is documented to the reader by that registration alone. A module that
 * claims a key needs no edit to `Help.tsx` to appear in it — only a `label` on
 * the binding if it would rather not be described by its intent string.
 *
 * `layoutModule` holds the reading mode, so the reader has exactly one way of
 * remembering anything about a person: registering it is what makes the mode
 * load, save and reach the settings panel. A `loadPrefs('mode', …)` in
 * `App.tsx` would be shorter but would be the first stored value that was not
 * a module's.
 *
 * `borrowModule` claims no keys, and its generated section is the one
 * `enabled` toggle; `App.tsx` wraps `display` with its `withBorrow`. Its
 * behaviour is a *pure function over `Display`* rather than a keypress or a
 * style, which is why registration plus one wrapped binding is the whole of it.
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
