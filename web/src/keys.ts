import type { ReaderModule } from './module'

/** What a keypress asked for. */
export type Intent =
  | { kind: 'toggle' }
  | { kind: 'view'; edition: string }
  | { kind: 'step'; by: 1 | -1 }
  | { kind: 'chrome' }
  /** Open correction mode: the filmstrip switches to the counterpart edition. */
  | { kind: 'correct' }
  /** A module's own declared key fired. `intent` is the string it registered — see `ReaderModule.keys`. */
  | { kind: 'module'; intent: string }

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
 *
 * `modules` is the registered set's own list (`Registry.all()`), not a fixed
 * import — this file names the `ReaderModule` type and nothing else, so a
 * seventh module needs no edit here. The built-ins above are checked first:
 * they are the reader's own reserved keys, and a module claiming one of them
 * would never fire regardless, so the fixed checks staying first is not a
 * priority choice so much as those keys not being a module's to claim.
 */
export function intentOf(
  event: KeyboardEvent,
  editions: string[],
  modules: ReaderModule<unknown>[] = [],
): Intent | undefined {
  if (event.ctrlKey || event.metaKey || event.altKey) return undefined
  if (event.key === ' ') return { kind: 'toggle' }
  if (event.key === 'ArrowRight') return { kind: 'step', by: 1 }
  if (event.key === 'ArrowLeft') return { kind: 'step', by: -1 }
  if (event.key === 'Tab') return { kind: 'chrome' }
  if (event.key === 'c') return { kind: 'correct' }
  if (/^[1-9]$/.test(event.key)) {
    const edition = editions[Number(event.key) - 1]
    return edition === undefined ? undefined : { kind: 'view', edition }
  }
  for (const mod of modules) {
    const binding = mod.keys?.find((k) => k.key === event.key)
    if (binding) return { kind: 'module', intent: binding.intent }
  }
  return undefined
}

/**
 * Whether a keydown's target is a control rather than the page at large — a
 * `button`, an `a`, an `input`, a `textarea`, anything `contenteditable`, or
 * anything else carrying `tabindex`.
 *
 * **`textarea` was missing until M5c, and the reader had no textarea to
 * notice it with.** The note editor is the first surface here that takes
 * typed text, and without this every module key fired mid-word: typing `s`
 * opened the settings panel, `n` started another note, Space swapped edition,
 * a digit jumped to one. Reported from a real browser. The tests that
 * "covered" the editor had set its value with Playwright's `fill()`, which
 * dispatches an input event rather than keystrokes, so not one of them ever
 * sent a key through this function — which is why a defect this loud survived
 * a browser pass.
 *
 * `contenteditable` is here for the same reason ahead of a caller rather than
 * behind one: the rule is "is this target taking text", not "is it one of
 * four tag names", and the next surface that takes text should not have to
 * rediscover this.
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
  const el = target as {
    tagName?: unknown
    isContentEditable?: unknown
    hasAttribute?: (name: string) => boolean
  } | null
  if (!el || typeof el.tagName !== 'string') return false
  if (
    el.tagName === 'BUTTON' ||
    el.tagName === 'A' ||
    el.tagName === 'INPUT' ||
    el.tagName === 'TEXTAREA'
  ) {
    return true
  }
  if (el.isContentEditable === true) return true
  return typeof el.hasAttribute === 'function' && el.hasAttribute('tabindex')
}

/**
 * Whether a *pointer* press landed on a control, counting ancestors.
 *
 * `isInteractiveTarget` above answers the same question for a keydown, where
 * the target is the focused element itself and asking about ancestors would
 * be asking about nothing. A pointer press is different in kind: its target is
 * whatever node is under the pixel, which for a button holding a run of text
 * is a `span` or a text node's parent — not the button. So the keydown rule
 * applied to a press would return `false` for a drag begun on the words inside
 * a note-list entry, and the caller would treat it as a press on the page.
 *
 * `closest` rather than a hand-written walk, and the same set of controls
 * spelled once here rather than twice: a selector is what `closest` takes, and
 * keeping the two functions' notions of "a control" from drifting apart is
 * worth more than sharing an implementation between them.
 *
 * Deliberately **not** used for the keydown case. Ancestors matter to a press
 * and not to a keystroke, and collapsing the two would make a key pressed
 * while focus sits anywhere inside a `tabindex` container stop being a reader
 * intent — a behaviour nobody asked for, in the name of one fewer function.
 */
export function isInteractivePress(target: EventTarget | null): boolean {
  const el = target as { closest?: (selector: string) => unknown } | null
  if (!el || typeof el.closest !== 'function') return false
  return (
    el.closest(
      'button, a, input, textarea, select, [tabindex], [contenteditable=""], [contenteditable="true"]',
    ) !== null
  )
}
