import type { ReaderModule } from './module'

/** What a keypress asked for. */
export type Intent =
  | { kind: 'toggle' }
  | { kind: 'view'; edition: string }
  | { kind: 'step'; by: 1 | -1 }
  | { kind: 'chrome' }
  /** Open the account screen: password, API keys, what is signed in. */
  | { kind: 'account' }
  /** Open correction mode: the filmstrip switches to the counterpart edition. */
  | { kind: 'correct' }
  /** A module's own declared key fired. `intent` is the string it registered — see `ReaderModule.keys`. */
  | { kind: 'module'; intent: string }
  /** Leave the reader for the library grid — spec C1 §6. */
  | { kind: 'library' }

/**
 * One of the reader's own keys: what it is called, what it asks for, and how
 * to say so to a person.
 *
 * `label` exists so the help screen can be *generated* rather than written.
 * A second list of what the keys do is a copy of a rule, and this project has
 * repeatedly found copies of rules drifting from the rules — a comment true
 * when written and false four commits later is the same failure at a smaller
 * scale. One table, dispatched from and listed from.
 *
 * `keys` is every key the row answers, so the digits are one row rather than
 * nine: `1`..`9` is a single rule — select the nth edition — and nine help
 * lines saying the same thing would be worse than one saying it once.
 *
 * `resolve` takes the pressed key because a row can answer several, and takes
 * `editions` because the digit row is the only rule here that depends on
 * anything outside the keyboard.
 */
export interface ReaderBinding {
  keys: string[]
  label: string
  resolve: (key: string, editions: string[]) => Intent | undefined
}

/**
 * The reader's own keys, **in the order they are answered**.
 *
 * The order is the rule that was previously carried by the sequence of `if`
 * statements in `intentOf`: these are checked before any module, because a
 * module claiming one of them would never fire regardless. Keeping them first
 * is therefore not a priority choice so much as those keys not being a
 * module's to claim.
 *
 * **Two readers, which is the whole point.** `intentOf` below dispatches from
 * it, and `Help.tsx` lists from it through `modules/help.ts` — so a row that
 * changes what a key does cannot keep an old description, because there is
 * one description. The alternative was to leave the `if` statements and give
 * the help its own list, which is the copy this exists to avoid.
 *
 * *This said "nothing lists from this yet" until the overlay landed later the
 * same day. A comment describing work in flight is a comment with an expiry
 * date on it.*
 *
 * `0` is absent on purpose, and `modules/zoom.ts` depends on that: it claims
 * `0` for zoom-to-whole-page, which is only safe while no row here answers it.
 * A tenth edition would therefore need a key other than `0`.
 */
export const READER_KEYS: ReaderBinding[] = [
  {
    keys: [' '],
    label: 'swap to the last edition and back',
    resolve: () => ({ kind: 'toggle' }),
  },
  // The clause after the dash is not decoration: out of a gap or a borrowed
  // page `session.ts`'s `step` case takes `nearest.ts`'s `nearestPaired`
  // offer instead of moving by one, and that offer can sit many slots away —
  // measured at 43 on `gotoubun-v01`. Leaving the label at "next page" would
  // have `modules/help.ts`'s `helpGroups` and `Help.tsx` repeat a claim that
  // is only true from an ordinary page, which is exactly what `stepFor`
  // having three consumers — `App.tsx`, `Filmstrip`, this table — exists to
  // prevent.
  {
    keys: ['ArrowRight'],
    label:
      'next page — or the nearest one forward, out of a gap or a borrowed page',
    resolve: () => ({ kind: 'step', by: 1 }),
  },
  {
    keys: ['ArrowLeft'],
    label:
      'previous page — or the nearest one back, out of a gap or a borrowed page',
    resolve: () => ({ kind: 'step', by: -1 }),
  },
  {
    keys: ['Tab'],
    label: 'show or hide the filmstrip and readouts',
    resolve: () => ({ kind: 'chrome' }),
  },
  {
    keys: ['c'],
    label: 'correct this pairing',
    resolve: () => ({ kind: 'correct' }),
  },
  {
    // **`A` rather than `a`, which `annotations` already claims** for the
    // review list. Uppercase for a panel is this reader's existing shape: `c`
    // corrects a pairing and `C` opens the credits panel, so the same letter in
    // two cases meaning "do the thing" and "show me the list" is a convention
    // rather than a collision dodged. The existing note-editor tests are what
    // caught the first attempt at `a`.
    keys: ['A'],
    label: 'account: password, API keys, and what is signed in',
    resolve: () => ({ kind: 'account' }),
  },
  {
    keys: ['L'],
    label: 'back to the library',
    resolve: () => ({ kind: 'library' }),
  },
  {
    keys: ['1', '2', '3', '4', '5', '6', '7', '8', '9'],
    label: 'jump to the nth edition',
    resolve: (key, editions) => {
      const edition = editions[Number(key) - 1]
      return edition === undefined ? undefined : { kind: 'view', edition }
    },
  },
]

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
 * new module needs no edit here. `READER_KEYS` above is walked first, in
 * the order it is written: those are the reader's own reserved keys, and a
 * module claiming one of them would never fire regardless, so the table going
 * first is not a priority choice so much as those keys not being a module's
 * to claim.
 *
 * A matched row **ends the walk even when it resolves to nothing**, which is
 * the digit rule: `4` on a three-edition work is silent rather than offered
 * on to the modules. `continue` there would let a module hold `4` on a work
 * with three editions and lose it on a work with four, so the same press
 * would mean different things depending on the material. The `if` statements
 * this replaced returned `undefined` rather than falling through, so the walk
 * is keeping that behaviour rather than choosing it afresh.
 */
export function intentOf(
  event: KeyboardEvent,
  editions: string[],
  modules: ReaderModule<unknown>[] = [],
): Intent | undefined {
  if (event.ctrlKey || event.metaKey || event.altKey) return undefined
  for (const binding of READER_KEYS) {
    if (binding.keys.includes(event.key))
      return binding.resolve(event.key, editions)
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
