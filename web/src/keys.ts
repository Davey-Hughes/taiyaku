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
  /** Leave the reader by its way back: `back.ts`. */
  | { kind: 'library' }

/**
 * One of the reader's own keys: what it is called, what it asks for, and how
 * to say so to a person.
 *
 * `label` exists so the help screen can be *generated* rather than written.
 * A second list of what the keys do is a copy of a rule, and copies of rules
 * drift from the rules. One table, dispatched from and listed from.
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
 * The order is the rule: these are checked before any module, because a
 * module claiming one of them would never fire regardless. Keeping them first
 * is therefore not a priority choice so much as those keys not being a
 * module's to claim.
 *
 * **Two readers, which is the whole point.** `intentOf` below dispatches from
 * it, and `Help.tsx` lists from it through `modules/help.ts` — so a row that
 * changes what a key does cannot keep an old description, because there is
 * one description. Giving the help its own list would be the copy this exists
 * to avoid.
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
  // offer instead of moving by one, and that offer can sit many slots away.
  // Leaving the label at "next page" would have `modules/help.ts`'s
  // `helpGroups` and `Help.tsx` repeat a claim that is only true from an
  // ordinary page, which is exactly what sharing `stepFor` exists to prevent.
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
    label: 'move into the bars, and on round to the page',
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
    // rather than a collision dodged.
    keys: ['A'],
    label: 'account: password, API keys, and what is signed in',
    resolve: () => ({ kind: 'account' }),
  },
  {
    keys: ['L'],
    label: 'back where you came from, else the library',
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
 * The reader's keys without the one that corrects a pairing: the help of an
 * account whose correction the server refuses (O58). Found by what the row
 * asks for, not by its letter or label, so a rebinding cannot leave it listed.
 */
export function withoutCorrecting(bindings: ReaderBinding[]): ReaderBinding[] {
  return bindings.filter(
    (b) => b.resolve(b.keys[0] ?? '', [])?.kind !== 'correct',
  )
}

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
 * would mean different things depending on the material.
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
 * Which keys a keydown's target keeps for itself — spec 2026-09-26 §3.2.
 *
 * - `'text'` keeps every key: an `input` other than a checkbox or a button, a
 *   `textarea`, a `select`, anything `contentEditable`.
 * - `'button'` keeps `Space` and `Enter`, the keys that press it: a `button`,
 *   a link, a checkbox, an `input` button.
 * - `'none'` keeps nothing: the page area, a panel itself, `<body>`.
 *
 * **The answer is three-valued, not one yes-or-no for all of them.** A single
 * yes-or-no would stand every reader key down on a focused button, so `→` on
 * ⚙ would do nothing, and on anything carrying `tabindex`, which the page
 * area and the panels carry (§3.1 and §3.3). A `select` is `'text'`: without
 * that, a key pressed on one of Settings' selects would reach the reader
 * instead — `s` would close the panel under it and `→` turn the page
 * (O166, `docs/notes.md`).
 *
 * **`textarea` is in the `'text'` rule because the note editor takes typed
 * text.** Without it every module key would fire mid-word. `contentEditable`
 * is there for the same reason ahead of a caller: the rule is "is this target
 * taking text", not "is it one of four tag names".
 *
 * Duck-typed rather than `instanceof`, because `keys.test.ts` runs in the
 * `node` project with no DOM.
 */
export type ControlKind = 'text' | 'button' | 'none'

/** The `input` types that are pressed rather than typed into. */
const PRESSED_INPUTS = new Set(['checkbox', 'button', 'submit', 'reset'])

export function controlKind(target: EventTarget | null): ControlKind {
  const el = target as {
    tagName?: unknown
    isContentEditable?: unknown
    type?: unknown
  } | null
  if (!el || typeof el.tagName !== 'string') return 'none'
  if (el.isContentEditable === true) return 'text'
  switch (el.tagName) {
    case 'TEXTAREA':
    case 'SELECT':
      return 'text'
    case 'INPUT':
      return PRESSED_INPUTS.has(String(el.type)) ? 'button' : 'text'
    case 'BUTTON':
    case 'A':
      return 'button'
    default:
      return 'none'
  }
}

/**
 * Whether a keydown's target is inside one of the reader's panels, every one
 * of which is a `role="dialog"`. `Tab` there is the panel's, not the ring's
 * (spec 2026-09-26 §3.3).
 */
export function inPanel(target: EventTarget | null): boolean {
  const el = target as { closest?: (selector: string) => unknown } | null
  return (
    typeof el?.closest === 'function' && el.closest('[role="dialog"]') !== null
  )
}

/**
 * Whether a *pointer* press landed on a control, counting ancestors.
 *
 * `controlKind` above answers a narrower question for a keydown — which
 * keys the focused element keeps — where the target is the focused element
 * itself and asking about ancestors would be asking about nothing. A pointer
 * press is different in kind: its target is whatever node is under the pixel,
 * which for a button holding a run of text is a `span` or a text node's
 * parent — not the button. So the keydown rule applied to a press would
 * answer `'none'` for a drag begun on the words inside a note-list entry, and
 * the caller would treat it as a press on the page.
 *
 * `closest` rather than a hand-written walk: a selector is what it takes.
 *
 * **The page area is left out of `[tabindex]`.** It carries `tabIndex={-1}`
 * as the ring's page stop (spec 2026-09-26 §3.1), and counted it would make
 * every press on the page a press on a control, refusing every tap and swipe
 * `gestures.ts` reads.
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
      'button, a, input, textarea, select, [tabindex]:not([data-reader-page]), [contenteditable=""], [contenteditable="true"]',
    ) !== null
  )
}
