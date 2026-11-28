import type { ReaderModule } from '../module'
import { type Layout, PAGE_LAYOUT_WORDS } from '../layouts'

/**
 * How the page area is laid out: one page, a two-page spread, a continuous
 * scroll, or one page beside another edition's.
 *
 * **It is a module so that it persists**: registering it here is all it takes,
 * since `App.tsx` builds its preference state from `registry.all()` and writes
 * every slice through `prefs.ts`, so a module's `defaults` are loaded, saved
 * and given a settings control with no code written for any of the three.
 *
 * **Deliberately not a new storage key.** The alternative was calling
 * `loadPrefs('mode', …)` directly from `App.tsx`, which is two lines shorter
 * and starts a second way of persisting things — the first thing in this
 * reader to be stored without being a module, and so the first thing whose
 * key nobody would think to namespace. Everything the reader remembers about
 * a person goes through one path; see `prefs.ts`.
 *
 * The mode also has buttons of its own in the top bar, which `Tab` shows, so
 * the settings entry is a second way to reach the same preference rather
 * than the only one. That is the same shape as `annotations`' `reveal`,
 * which has both a key and a toggle, and it costs nothing: both write the
 * one slice.
 */
export interface LayoutPrefs {
  mode: Layout
}

export const layoutModule: ReaderModule<LayoutPrefs> = {
  id: 'layout',
  label: 'Layout',
  defaults: { mode: 'single' },
  options: { mode: ['single', 'two', 'scroll', 'compare'] },
  optionWords: { mode: PAGE_LAYOUT_WORDS },
}
