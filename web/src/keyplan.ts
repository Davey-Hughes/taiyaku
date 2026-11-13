/**
 * Given a key, an intent and the flags in `Guards`: what should happen, and
 * should the browser's own default be prevented.
 *
 * This is a module of its own because `App.tsx` value-imports `resolver.ts`,
 * which keeps a decision made there out of a test's reach.
 *
 * **This is the decision, not the handler.** Every *action* lives in the
 * module that owns it — the note draft in `notes.ts`, the rect in
 * `modules/zoom.ts`, the write in `correction.ts`, the preference slices in
 * `module.ts` — so what is left to say is only which of them a keypress means.
 * What stays in `App.tsx` is the `window` subscription, the refs the flags are
 * read through, and one `switch` of one-line calls.
 */
import type { ControlKind, Intent } from './keys'
import { REVIEW_INTENT } from './modules/annotations'
import { PANEL_INTENT } from './modules/credits'
import { TOGGLE_INTENT as HELP_TOGGLE_INTENT } from './modules/help'
import {
  IN_INTENT as ZOOM_IN_INTENT,
  OUT_INTENT as ZOOM_OUT_INTENT,
  WHOLE_INTENT as ZOOM_WHOLE_INTENT,
} from './modules/zoom'
import type { PanelMode } from './notes'
import type { ReadingIntent } from './session'

/**
 * What the handler must know that the event does not carry.
 *
 * **Four of them are gates rather than state** — `zoomable`, `creditsPanel`,
 * `helpEnabled`, `mayCorrect` — and are decided here rather than left inline
 * in the module switch, where they would make the residual dispatch guarded
 * cases rather than one-line calls. Each is a decision about whether a key
 * does anything, which is the one thing this module is for; a guard left in
 * `App.tsx` is a branch nothing can reach.
 */
export interface Guards {
  /** The help overlay is up. It closes first, ahead of everything else. */
  helpOpen: boolean
  /** Which surface the note panel is showing, or none. */
  panel: PanelMode | undefined
  /** A correction is being asserted — `pending` is set. */
  correcting: boolean
  /** A correction is on the wire. */
  submitting: boolean
  /** A note write is on the wire. */
  notesBusy: boolean
  /** `keys.ts`'s `controlKind(event.target)` — which keys the focused element keeps. */
  control: ControlKind
  /** Whether the account panel is open over the reader.
  ///
  /// **`Escape` belongs to whatever is on top.** Without this it would fall
  /// through to `cancel-correction`, so pressing it to dismiss the account
  /// panel would silently abandon a pending correction the reader had not
  /// finished — the panel would stay open and the correction be gone, with
  /// nothing saying either. The note editor already has exactly this guard
  /// (`panel === 'edit'`). */
  accountOpen: boolean
  /** `Chrome`'s bars are up. `Escape` takes them away when nothing above them is open (spec 2026-09-26 §3.1). */
  barsUp: boolean
  /** The settings panel is open. */
  settingsOpen: boolean
  /** The credits panel is open. */
  creditsOpen: boolean
  /** `keys.ts`'s `inPanel(event.target)` — focus is inside an open panel, whose `Tab` it is (spec 2026-09-26 §3.3). */
  inPanel: boolean
  /**
   * `modules/zoom.ts`'s `zoomable(zoomPrefs, drawn !== undefined)` — one flag
   * for the three zoom keys, and the same predicate `notes.ts`'s
   * `pressTakesPan` is gated on, so a keyboard zoom and a pan press cannot
   * disagree about whether there is anything to move.
   */
  zoomable: boolean
  /** `creditsPrefs.panel` — what makes `C` genuinely inert, rather than opening a panel a reader switched off. */
  creditsPanel: boolean
  /** `helpPrefs.enabled` — the same, for `?` and `h`. */
  helpEnabled: boolean
  /** The account is an administrator, the only one whose correction the server takes (O58). */
  mayCorrect: boolean
}

export type KeyAction =
  | { kind: 'close-help' }
  /** Flush and close. `Escape` means "I am finished here", not "abandon" — undo is what abandons. */
  | { kind: 'close-editor' }
  | { kind: 'cancel-correction' }
  /** `Tab`: one stop round the reader's ring (spec 2026-09-26 §3.1); `App.tsx` reads `shiftKey` for the direction. */
  | { kind: 'ring' }
  | { kind: 'open-account' }
  | { kind: 'close-account' }
  | { kind: 'close-settings' }
  | { kind: 'close-credits' }
  /** Hide the bars and put focus on the page — spec 2026-09-26 §3.1. */
  | { kind: 'leave-bars' }
  | { kind: 'begin-correction' }
  | { kind: 'module'; intent: string }
  /** Back to the library grid — spec C1 §6. */
  | { kind: 'leave-reader' }
  /**
   * `ReadingIntent` is `session.ts`'s own `Exclude`, reused rather than
   * restated, so a fourth `Intent` kind cannot be admitted here silently.
   */
  | { kind: 'reading'; intent: ReadingIntent }
  /**
   * The reader claims this key and prevents its default, and nothing happens.
   *
   * **Not a placeholder — it is what the ordering means.** `App.tsx` calls
   * `event.preventDefault()` *before* the module switch, so a `C` pressed with
   * the credits panel switched off, or `=` in scroll mode, or `a` while a note
   * write is on the wire, swallows the browser's own default. Folding those
   * into `undefined` would quietly hand those keys back to the browser, a
   * change in what they do. Distinguishing "inert" from "not ours" keeps the
   * ordering and keeps every one of these gates inside a tested module.
   */
  | { kind: 'inert' }

export interface Planned {
  action: KeyAction
  /**
   * Every key this reader claims prevents the browser's default, `Tab`
   * included since the accessibility pass: the reader moves focus round its
   * ring itself (spec 2026-09-26 §3.1). Without `preventDefault` on `Tab`,
   * toggling the bars would layer on the browser's own focus move, which walks
   * focus out of the page into the browser's frame (O135).
   */
  preventDefault: boolean
}

const CLAIMED: Planned = { action: { kind: 'inert' }, preventDefault: true }

/**
 * Whether the focused element keeps this key for itself — spec 2026-09-26
 * §3.2. Text entry keeps every key; a button keeps the two that press it; the
 * page and a panel keep none.
 */
function controlKeeps(control: ControlKind, key: string): boolean {
  if (control === 'text') return true
  return control === 'button' && (key === ' ' || key === 'Enter')
}

/**
 * Whether a module's key does anything right now.
 *
 * The module intents gated on a flag rather than on the session are decided
 * here. The rest are gated on nothing, or on something only the component can
 * read — `sessionRef.current` and `display()` against a live resolver, which
 * is why `n`, `d`, `b` and `c` keep a guard apiece in the residual dispatch.
 */
function moduleKeyLive(intent: string, guards: Guards): boolean {
  switch (intent) {
    case ZOOM_IN_INTENT:
    case ZOOM_OUT_INTENT:
    case ZOOM_WHOLE_INTENT:
      // Inert exactly where the `Viewport` wrapper is not mounted or has
      // nothing under it: scroll mode and a gap. Without this, `=` in scroll
      // mode silently zooms a rect nobody is drawing and the reader finds it
      // applied on their next switch back to single.
      return guards.zoomable
    case PANEL_INTENT:
      return guards.creditsPanel
    case HELP_TOGGLE_INTENT:
      return guards.helpEnabled
    case REVIEW_INTENT:
      // Not while a note write is on the wire, for the reason `Escape`'s note
      // branch stands down: swapping the panel would tell the reader the note
      // was abandoned while it is still going to land.
      return !guards.notesBusy
    default:
      return true
  }
}

/**
 * What a keypress means.
 *
 * `key` is checked before `intent`, because `Escape` is not one of `intentOf`'s
 * intents and deliberately does *not* stand down for a focused control — the
 * tile a reader just clicked is itself a button, and gating `Escape` on the
 * focused control would make it fire everywhere except the one moment it
 * matters most.
 *
 * `undefined` means the keypress belongs to something else and the browser
 * should have it: to a control that has focus, to the note editor (nothing
 * reaches the page while it is open), or to a write that is still on the wire.
 * A key this reader claims but cannot act on right now is `inert` instead —
 * see that variant's own doc for why the two are not the same answer.
 */
export function keyPlan(
  key: string,
  intent: Intent | undefined,
  guards: Guards,
): Planned | undefined {
  if (key === 'Escape') {
    // The help overlay closes first, ahead of everything else, because
    // closing it discards nothing while every branch under it either abandons
    // a correction or flushes a note. If both are open, spending the keypress
    // on the one that costs nothing and can be reopened with `?` is the
    // cheaper mistake.
    //
    // Not gated on `helpEnabled`, unlike the toggle: a reader who switches the
    // module off while the overlay is up would otherwise be left with a panel
    // whose own keys have just gone inert.
    if (guards.helpOpen) {
      return { action: { kind: 'close-help' }, preventDefault: true }
    }
    // **Beside `helpOpen`, and above the correction.** `Escape` belongs to
    // whatever is on top, and the account panel is a screen over the reader.
    // Without this it would fall through to `cancel-correction`, so a reader
    // dismissing the panel would silently abandon a pending correction —
    // the panel would stay open and the correction be gone, with nothing
    // saying either.
    if (guards.accountOpen) {
      return { action: { kind: 'close-account' }, preventDefault: true }
    }
    // **Settings and Credits close next, above the note panel and the
    // correction**, for the reason Help closes first: closing either discards
    // nothing (spec 2026-09-26 §3.3, O135, O161).
    if (guards.settingsOpen) {
      return { action: { kind: 'close-settings' }, preventDefault: true }
    }
    if (guards.creditsOpen) {
      return { action: { kind: 'close-credits' }, preventDefault: true }
    }
    // The note panel closes on `Escape` too, but only where there is no
    // correction to abandon: a reader mid-correction pressing `Escape` means
    // the correction, which is the irreversible one.
    if (!guards.correcting && guards.panel) {
      // Not while a note write is on the wire — taking the panel away would
      // tell the reader the note was abandoned while it is still going to
      // land. `undefined` rather than `inert`: nothing here claims the key,
      // and the browser's own `Escape` (stopping a load, closing a native
      // dialog) is not this reader's to swallow while it is busy.
      if (guards.notesBusy) return undefined
      return { action: { kind: 'close-editor' }, preventDefault: true }
    }
    if (guards.correcting) {
      // Not while the write is on the wire: taking the banner away would tell
      // the reader the correction was abandoned when it is still going to
      // land. The bars stay too — the correction holds them up.
      if (guards.submitting) return undefined
      return { action: { kind: 'cancel-correction' }, preventDefault: true }
    }
    // Last: the bars themselves, from anywhere in them or from the page while
    // a tap has them up (spec 2026-09-26 §3.1).
    if (guards.barsUp) {
      return { action: { kind: 'leave-bars' }, preventDefault: true }
    }
    return undefined
  }
  // One dispatch path: every key, built-in or module-declared, has already
  // passed through `intentOf`, which applies the modifier guard once.
  if (!intent) return undefined
  // **Nothing reaches the page while the note editor is open.** It is a dialog
  // that takes text, so a keystroke means the note — whatever the browser
  // currently thinks is focused. The control guard below only stands a key
  // down while focus is *in* a control, and a refused save or a stray click is
  // enough to break that: with a server rejecting a write, the editor stays
  // open holding the reader's text while focus sits on `<body>`, so typing
  // would open the settings panel instead of writing the note. `Escape` is
  // handled above and still closes the editor; everything else belongs to the
  // note until it does.
  if (guards.panel === 'edit') return undefined
  // **The account panel stands every other key down too**, exactly as the note
  // editor above does. Consulting `accountOpen` only inside the `Escape` branch
  // would leave the reader keys live behind a `position: fixed; inset: 0`
  // overlay with no focus trap: clicking Create or Revoke drops focus to
  // `<body>` — both `disabled={busy}` and the refresh re-render do it — so the
  // control guard stands nothing down, and `→ → →` would turn the hidden reader
  // three pages forward with the debounced `putProgress` writing that position
  // later, while `c` would arm a correction nobody could see. It is the failure
  // the editor guard's own comment describes.
  if (guards.accountOpen) return undefined
  // Ahead of the control check, unlike everything below it: `Tab` moving
  // focus onto a control is not the same event as activating it, so there is
  // nothing for it to collide with.
  if (intent.kind === 'chrome') {
    // O161: the first `Tab` in an open panel would show the bars behind it.
    if (guards.inPanel) return undefined
    return { action: { kind: 'ring' }, preventDefault: true }
  }
  // A key the focused element uses is the element's. Without this, tabbing
  // onto a mode button and pressing Space to click it would also fire the
  // edition toggle — not because two listeners raced, but because a
  // window-level listener has no way to know a focused button is about to
  // handle the very same keypress itself. Every other key is the reader's,
  // wherever focus is: `→` turns the page from ⚙ (spec 2026-09-26 §3.2).
  if (controlKeeps(guards.control, key)) return undefined
  // **Below the guard, unlike `chrome` above it, and the difference is that
  // `Tab` is not a character and `A` is.** `intentOf` bails only on
  // ctrl/meta/alt, not Shift, so this fires for a shifted `a` typed anywhere —
  // and above the guard, with `preventDefault`, a reader whose password
  // contains a capital A could not type it into the login field, the account
  // panel's own password fields, or the key-name box. The character would
  // never reach the input and the panel would open instead.
  if (intent.kind === 'account') {
    return { action: { kind: 'open-account' }, preventDefault: true }
  }
  if (intent.kind === 'library') {
    return { action: { kind: 'leave-reader' }, preventDefault: true }
  }
  if (intent.kind === 'correct') {
    // O58: for anyone else `c` is not the reader's key, so the browser keeps it.
    if (!guards.mayCorrect) return undefined
    // Not while a correction is already on the wire. Without this, a `c` press
    // mid-write would silently replace the pending correction the completion
    // handler is about to resolve. Nothing else stops it: a button keeps only
    // the keys that press it (spec 2026-09-26 §3.2), and
    // `disabled={submitting}` blurs the just-clicked Confirm the instant the
    // write starts anyway.
    if (guards.submitting) return undefined
    return { action: { kind: 'begin-correction' }, preventDefault: true }
  }
  if (intent.kind === 'module') {
    return moduleKeyLive(intent.intent, guards)
      ? {
          action: { kind: 'module', intent: intent.intent },
          preventDefault: true,
        }
      : CLAIMED
  }
  return { action: { kind: 'reading', intent }, preventDefault: true }
}
