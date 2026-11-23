/**
 * The browse screens' preferences — spec C1 §4 and §11 step 8.
 *
 * **A second registry, not an entry in `modules/registry.ts`.** That file is
 * "every reading behaviour this build ships"; a browse screen is not one, and
 * its keys live in a different collision domain — `/` for search here has
 * nothing to argue with a reader key about, and one registry would forbid that
 * for no benefit. `buildRegistry` is reused, so a second module added here
 * gets the same import-time checks the reader's list has.
 *
 * **Nothing here is a settings screen.** `settingsPanel.ts`'s `fieldsOf`
 * derives a control from a preference's runtime type — boolean to checkbox,
 * a string whose module declares `options` to a select, a number with a
 * declared scale to a slider — so the values below are the whole declaration
 * and `ViewMenu.tsx` renders whatever they turn out to be.
 *
 * **The honest cost**, recorded in §4 and not softened here: the reader's
 * module system works because each module is a small *behaviour*, and this is
 * the first one that owns whole screens. Two presentations of one dataset are
 * genuinely two components to keep in step.
 */
import { createContext, useContext } from 'react'
import { buildRegistry, type ReaderModule } from './module'
import { loadPrefs } from './prefs'
import type { SliderScale } from './settingsPanel'

/**
 * The cover-size slider's scale (O285): positions 0 to `COVER_LAST`, the
 * default at `COVER_MIDDLE`, the middle of the track. The left half runs 96
 * to 144 px in 1 px steps and the right half 144 to 240 px in 2 px steps:
 * those two steps are what put 144 at the middle with even steps, and they
 * land 104 and 144, the sizes a stored `density` carries over to, exactly on
 * a position.
 */
export const COVER_MIN = 96
export const COVER_DEFAULT = 144
export const COVER_MAX = 240
export const COVER_MIDDLE = 48
export const COVER_LAST = 96

/** The size in px at slider position `p`. */
export function coverSizeAt(p: number): number {
  return p <= COVER_MIDDLE
    ? COVER_MIN + p
    : COVER_DEFAULT + 2 * (p - COVER_MIDDLE)
}

/** The slider position nearest a size in px, held to the track. */
export function coverPositionOf(size: number): number {
  const p =
    size <= COVER_DEFAULT
      ? size - COVER_MIN
      : COVER_MIDDLE + (size - COVER_DEFAULT) / 2
  return Math.min(COVER_LAST, Math.max(0, Math.round(p)))
}

/** `coverSize`'s slider, as `browseModule.sliders` declares it. */
export const coverSizeScale: SliderScale = {
  last: COVER_LAST,
  valueAt: coverSizeAt,
  positionOf: coverPositionOf,
  unit: 'px',
  spokenUnit: 'pixels',
}

/**
 * `coverSize` on the slider's scale: a size off the scale is drawn at the
 * nearest position's, and a value that is not a finite number at the
 * default. `loadBrowsePrefs` lets any number through (JSON's `1e400` is
 * `Infinity`), and a `BrowsePrefsContext` provider other than the shell
 * hands in whatever it built.
 */
export function coverSizeOf(prefs: BrowsePrefs): number {
  const size = prefs.coverSize
  if (!Number.isFinite(size)) return browseModule.defaults.coverSize
  return coverSizeAt(coverPositionOf(size))
}

/**
 * Whether a reader puts its series page under itself (O431): for anything
 * but `'whereYouCameFrom'`, so a stored value that is neither choice reads as
 * the default, as a cover size off the scale does. `loadBrowsePrefs` lets
 * any stored value through.
 */
export function putsSeriesUnder(prefs: BrowsePrefs): boolean {
  return prefs.backFromReading !== 'whereYouCameFrom'
}

export interface BrowsePrefs {
  /** §3's two presentations. */
  layout: 'grid' | 'table'
  /** §3's edition-as-a-mode switch. */
  editionSelector: boolean
  /**
   * §3's alignment panel, on a series page's volumes and on the Library
   * home's Continue row alike (O417). `'button'` rather than `'on-select'`
   * is right for this library: with about 90% of series single-edition, a
   * panel that opens on every selection is mostly empty, while a small `i`
   * on the cover costs nothing until it is wanted.
   */
  panel: 'off' | 'button' | 'on-select'
  /**
   * The covers' width in px, on the library grid, the continue strip and a
   * series page's volumes alike (O285).
   */
  coverSize: number
  /** Whether the library screen opens on what you were reading. */
  continueStrip: boolean
  /**
   * Whether ⇄ stays shown on every volume's cover in the Series grid where
   * every pointer can hover, rather than waiting for the volumes to be
   * hovered or to hold keyboard focus (`library.css`).
   */
  alwaysShowAligned: boolean
  /**
   * Where a reader's way out goes. `'seriesPage'` puts the work's series page
   * under the reader, so the way out and the browser's Back land there
   * (O431); `'whereYouCameFrom'` keeps O422's rule for a reader not opened
   * from its own series page: back where you came from, else the last Library
   * view. Each reader reads it once, as it opens.
   */
  backFromReading: 'seriesPage' | 'whereYouCameFrom'
}

export const browseModule: ReaderModule<BrowsePrefs> = {
  id: 'browse',
  label: 'Library',
  defaults: {
    layout: 'grid',
    editionSelector: true,
    panel: 'button',
    coverSize: COVER_DEFAULT,
    continueStrip: true,
    alwaysShowAligned: false,
    backFromReading: 'seriesPage',
  },
  options: {
    layout: ['grid', 'table'],
    panel: ['off', 'button', 'on-select'],
    backFromReading: ['seriesPage', 'whereYouCameFrom'],
  },
  // A slider rather than a number box: a few px is judged by eye, and the
  // covers move as it is dragged.
  sliders: { coverSize: coverSizeScale },
}

/**
 * Built at module scope, exactly as `modules/registry.ts` is and for the same
 * reason: `buildRegistry`'s checks run on import, before any component mounts,
 * rather than on a first render an error boundary could swallow.
 */
export const browseRegistry = buildRegistry([browseModule])

/**
 * `prefs.ts`'s `loadPrefs` for the browse slice, which also carries a
 * `density` stored before `coverSize` existed over to it: compact was a
 * 6.5rem tile, so it becomes 104 px, and comfortable is the default.
 * `density` itself is dropped, so the next save stores `coverSize` and the
 * old value is read only once.
 */
export function loadBrowsePrefs(id: string, defaults: object): object {
  const stored = loadPrefs<Record<string, unknown>>(id, {})
  if (id !== browseModule.id) return { ...defaults, ...stored }
  const { density, ...rest } = stored
  const coverSize =
    typeof rest.coverSize === 'number'
      ? rest.coverSize
      : density === 'compact'
        ? 104
        : browseModule.defaults.coverSize
  return { ...defaults, ...rest, coverSize }
}

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

/**
 * How a screen changes the browse preferences: `Shell.tsx` owns the state and
 * its persistence, and a page's View panel draws the controls. `undefined`
 * where no shell sits above — a test mounting one screen — and a View button
 * draws nothing then.
 */
export interface BrowseControls {
  /**
   * One preference changed, **uncoerced** — `raw` is whatever the control's
   * own change event handed back, which for a number input is a string.
   * `module.ts`'s `fieldChanged` is what coerces it, through
   * `settingsPanel.ts`'s `withFieldValue`. This is `Settings.tsx`'s contract
   * verbatim; see its own `onChange` doc and the test named "reports a number
   * field change as the raw string, uncoerced".
   */
  change: (key: string, raw: unknown) => void
  /** Restores the whole slice, every key and not only the ones a panel shows. */
  reset: () => void
}

export const BrowseControlsContext = createContext<BrowseControls | undefined>(
  undefined,
)
