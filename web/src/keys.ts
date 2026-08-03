/** What a keypress asked for. */
export type Intent =
  | { kind: 'toggle' }
  | { kind: 'view'; edition: string }
  | { kind: 'step'; by: 1 | -1 }
  | { kind: 'chrome' }

/**
 * A keypress, as an intent, or nothing.
 *
 * The alt-tab model: `Space` flips between the current edition and the last
 * one, so the common back-and-forth stays one key and the gesture does not
 * change as editions are added; `1`..`9` jump directly. Cycling was rejected —
 * with five editions it takes up to four presses and you lose track.
 *
 * A modifier yields the key entirely. A reader that swallowed ⌘1 would break
 * tab switching to save itself a branch.
 */
export function intentOf(event: KeyboardEvent, editions: string[]): Intent | undefined {
  if (event.ctrlKey || event.metaKey || event.altKey) return undefined
  if (event.key === ' ') return { kind: 'toggle' }
  if (event.key === 'ArrowRight') return { kind: 'step', by: 1 }
  if (event.key === 'ArrowLeft') return { kind: 'step', by: -1 }
  if (event.key === 'Tab') return { kind: 'chrome' }
  if (/^[1-9]$/.test(event.key)) {
    const edition = editions[Number(event.key) - 1]
    return edition === undefined ? undefined : { kind: 'view', edition }
  }
  return undefined
}

/**
 * Whether a keydown's target is a focusable control rather than the page at
 * large — a `button`, an `a`, an `input`, or anything else carrying
 * `tabindex`.
 *
 * `App.tsx` used to `preventDefault()` every recognised key, `Tab` included,
 * reasoning that a later `Space` would
 * otherwise double-fire on a button `Tab` had just focused — the mode
 * buttons, a filmstrip tile, one of `Gap`'s offered moves. That reasoning was
 * real but the fix was wrong: it removed `Tab`'s own focus traversal
 * entirely, forever, rather than resolving the one collision it was worried
 * about. `App.tsx` now leaves `Tab` alone and instead skips a reader
 * intent — Space, an arrow, a digit — whenever this returns true for
 * `event.target`, so a focused button's own keyboard activation is the only
 * thing that runs. `Tab` itself is never checked against this: moving focus
 * onto a control is not the same event as activating it, so there is nothing
 * for `Tab` to collide with.
 */
export function isInteractiveTarget(target: EventTarget | null): boolean {
  const el = target as { tagName?: unknown; hasAttribute?: (name: string) => boolean } | null
  if (!el || typeof el.tagName !== 'string') return false
  if (el.tagName === 'BUTTON' || el.tagName === 'A' || el.tagName === 'INPUT') return true
  return typeof el.hasAttribute === 'function' && el.hasAttribute('tabindex')
}
