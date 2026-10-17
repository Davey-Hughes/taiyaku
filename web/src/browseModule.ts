/**
 * The browse screens' preferences — spec C1 §4 and §11 step 8.
 *
 * **A second registry, not an entry in `modules/registry.ts`.** That file is
 * "every reading behaviour this build ships"; a browse screen is not one, and
 * its keys live in a different collision domain — `/` for search here has
 * nothing to argue with a reader key about, and one registry would forbid that
 * for no benefit. `buildRegistry` is reused unchanged, so a second module
 * added here gets the same import-time duplicate-id and key-collision check
 * the reader's list has.
 *
 * **Nothing here is a settings screen.** `settingsPanel.ts`'s `fieldKind`
 * derives a control from a preference's runtime type — boolean to checkbox,
 * a string whose module declares `options` to a select — so the values below
 * are the whole declaration and `BrowseSettings.tsx` renders whatever they
 * turn out to be.
 *
 * **The honest cost**, recorded in §4 and not softened here: the reader's
 * module system works because each module is a small *behaviour*, and this is
 * the first one that owns whole screens. Two presentations of one dataset are
 * genuinely two components to keep in step.
 */
import { createContext, useContext } from 'react'
import { buildRegistry, type ReaderModule } from './module'

export interface BrowsePrefs {
  /** §3's two presentations. */
  layout: 'grid' | 'table'
  /** §3's edition-as-a-mode switch. */
  editionSelector: boolean
  /**
   * §3's alignment panel. `'button'` rather than `'on-select'` is the owner's
   * correction to mockup C, and it is right for this library: with about 90%
   * of series single-edition, a panel that opens on every selection is mostly
   * empty, while a small `i` on the cover costs nothing until it is wanted.
   */
  panel: 'off' | 'button' | 'on-select'
  /** How tightly the grids pack. */
  density: 'compact' | 'comfortable'
  /** Whether the library screen opens on what you were reading. */
  continueStrip: boolean
}

export const browseModule: ReaderModule<BrowsePrefs> = {
  id: 'browse',
  label: 'Library',
  defaults: {
    layout: 'grid',
    editionSelector: true,
    panel: 'button',
    density: 'comfortable',
    continueStrip: true,
  },
  options: {
    layout: ['grid', 'table'],
    panel: ['off', 'button', 'on-select'],
    density: ['compact', 'comfortable'],
  },
}

/**
 * Built at module scope, exactly as `modules/registry.ts` is and for the same
 * reason: `buildRegistry`'s checks run on import, before any component mounts,
 * rather than on a first render an error boundary could swallow.
 */
export const browseRegistry = buildRegistry([browseModule])

/**
 * `browseModule.defaults` where no shell sits above the reader of this
 * context — a test mounting one screen on its own. Never `undefined`: a screen
 * that cannot get preferences should draw preset A, not nothing.
 */
export const BrowsePrefsContext = createContext<BrowsePrefs>(
  browseModule.defaults,
)

export function useBrowsePrefs(): BrowsePrefs {
  return useContext(BrowsePrefsContext)
}
