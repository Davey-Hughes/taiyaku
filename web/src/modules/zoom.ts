import type { ReaderModule } from '../module'

export const IN_INTENT = 'zoom:in'
export const OUT_INTENT = 'zoom:out'
export const WHOLE_INTENT = 'zoom:whole'

/**
 * Zoom and pan, and what a swap does with them.
 *
 * **Declared ahead of its own experiment, and the experiment has since run.**
 * `onSwap: 'content'` means "map through whichever derived transform wins" —
 * `viewport.ts`'s `mapRect` — and this module landed before the corpus
 * measurement scheduled as Task 8 of
 * `docs/superpowers/plans/2026-08-07-l1-geometry.md` had said which that was.
 * `'content'` was the right default even then, on the design's own promotion
 * rule (`docs/superpowers/specs/2026-08-07-l1-geometry-design.md` §4):
 * `'reset'` is what ships if no candidate beats throwing the view away, not a
 * default to assume ahead of a result.
 *
 * The result, `docs/notes.md`'s "Which derived transform maps a rect between
 * editions", left this line alone and changed the one it points at: neither
 * candidate the design named won, `mapRect`'s `'pillarbox'` did, and every one
 * of the three beat throwing the view away. So `'content'` is now a
 * measurement rather than a bet, and `mapRect`'s doc carries the figures. A
 * reader who disagrees with the default can still say so.
 *
 * **`maxScale` and `step` are guesses, not measurements.** 8x on a
 * 1200px-tall page puts a single line of dialogue across the viewport, which
 * is past what anyone has asked for and short of a scale where the source
 * pixels stop meaning anything; 1.15 is about five wheel notches to double
 * (1.15⁵ ≈ 2.011). Both are worth revisiting against a reader rather than
 * against a number.
 *
 * `0` is safe to claim and cannot become an edition jump later: `keys.ts`'s
 * `intentOf` tests digits with `/^[1-9]$/`, so the tenth edition would need a
 * different key regardless of what this module does.
 */
export interface ZoomPrefs {
  enabled: boolean
  maxScale: number
  step: number
  onSwap: 'content' | 'reset'
}

export const zoomModule: ReaderModule<ZoomPrefs> = {
  id: 'zoom',
  label: 'Zoom',
  defaults: {
    enabled: true,
    maxScale: 8,
    step: 1.15,
    onSwap: 'content',
  },
  options: { onSwap: ['content', 'reset'] },
  keys: [
    { key: '=', intent: IN_INTENT },
    { key: '-', intent: OUT_INTENT },
    { key: '0', intent: WHOLE_INTENT },
  ],
}
