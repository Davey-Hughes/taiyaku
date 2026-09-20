import type { ReaderModule } from '../module'
import type { Mode } from '../pairing'

/**
 * How the page area is laid out: one page, a two-page spread, or a continuous
 * scroll.
 *
 * **It is a module so that it persists**, and it persists because a reader who
 * chose two-page mode had chosen it again on every reload — `mode` was a plain
 * `useState` in `App.tsx`, reset to `'single'` on every load since the reader
 * shipped. Registering it here is the whole fix: `App.tsx` builds its
 * preference state from `registry.all()` and writes every slice through
 * `prefs.ts`, so a module's `defaults` are loaded, saved and given a settings
 * control with no code written for any of the three.
 *
 * **Deliberately not a new storage key.** The alternative was calling
 * `loadPrefs('mode', …)` directly from `App.tsx`, which is two lines shorter
 * and starts a second way of persisting things — the first thing in this
 * reader to be stored without being a module, and so the first thing whose
 * key nobody would think to namespace. Everything the reader remembers about
 * a person now goes through one path; see `prefs.ts`.
 *
 * The mode also has buttons of its own in the top bar, which `Tab` shows, so
 * the settings entry is a second way to reach the same preference rather
 * than the only one. That is the same shape as `annotations`' `reveal`,
 * which has both a key and a toggle, and it costs nothing: both write the
 * one slice.
 */
export interface LayoutPrefs {
  mode: Mode
}

export const layoutModule: ReaderModule<LayoutPrefs> = {
  id: 'layout',
  label: 'Layout',
  defaults: { mode: 'single' },
  options: { mode: ['single', 'two', 'scroll'] },
}
