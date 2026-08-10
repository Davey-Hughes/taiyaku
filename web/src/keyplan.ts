/**
 * Given a key, an intent and nine flags: what should happen, and should the
 * browser's own default be prevented.
 *
 * The keydown handler was 413 lines in `App.tsx` and almost all of it was
 * guards — thirteen early returns, several of which exist because of a
 * measured defect: a refused save dropping focus to `<body>` so that typing a
 * note opened the settings panel instead; `disabled={submitting}` blurring the
 * just-clicked Confirm so that `c` mid-write silently replaced the correction
 * the completion handler was about to resolve. None of it was reachable from a
 * test, because `App.tsx` value-imports `resolver.ts`.
 *
 * **This is the decision, not the handler.** By the time it exists every
 * *action* has already moved into the module that owns it — the note draft to
 * `notes.ts`, the rect to `modules/zoom.ts`, the write to `correction.ts`, the
 * preference slices to `module.ts` — so what is left to say is only which of
 * them a keypress means. What stays in `App.tsx` is the `window`
 * subscription, the refs the flags are read through, and one `switch` of
 * one-line calls.
 */
import type { Intent } from './keys'
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
 * **Nine, not six.** Three of them — `zoomable`, `creditsPanel`,
 * `helpEnabled` — are gates a first reading of the file left inline in the
 * module switch, where they would have made the residual dispatch three
 * guarded cases rather than one-line calls. Each is a decision about whether a
 * key does anything, which is the one thing this module is for; a guard left
 * in `App.tsx` is a branch nothing can reach. What they cost is three lines
 * computed at the call site, all three reading refs that are read there
 * already.
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
  /** `isInteractiveTarget(event.target)` — focus is in a control. */
  onControl: boolean
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
}

export type KeyAction =
  | { kind: 'close-help' }
  /** Flush and close. `Escape` means "I am finished here", not "abandon" — undo is what abandons. */
  | { kind: 'close-editor' }
  | { kind: 'cancel-correction' }
  | { kind: 'toggle-chrome' }
  | { kind: 'begin-correction' }
  | { kind: 'module'; intent: string }
  /**
   * `ReadingIntent` is `session.ts`'s own `Exclude`, reused rather than
   * restated, so a fourth `Intent` kind cannot be admitted here silently.
   */
  | { kind: 'reading'; intent: ReadingIntent }
  /**
   * The reader claims this key and prevents its default, and nothing happens.
   *
   * **Not a placeholder — it is what today's ordering means.** `App.tsx`
   * called `event.preventDefault()` *before* the module switch, so a `C`
   * pressed with the credits panel switched off, or `=` in scroll mode, or `a`
   * while a note write is on the wire, already swallowed the browser's own
   * default. Folding those into `undefined` would have quietly handed those
   * keys back to the browser, which is a behaviour change rather than an
   * extraction. Distinguishing "inert" from "not ours" keeps the ordering and
   * keeps every one of these gates inside a tested module.
   */
  | { kind: 'inert' }

export interface Planned {
  action: KeyAction
  /**
   * **`Tab` alone is `false`.** Toggling the chrome is layered on top of the
   * browser's own focus movement rather than replacing it, and preventing it
   * is what made every focusable control in the reader — the mode buttons, all
   * of the filmstrip's tiles, `Gap`'s offered moves — mouse-only.
   */
  preventDefault: boolean
}

const CLAIMED: Planned = { action: { kind: 'inert' }, preventDefault: true }

/**
 * Whether a module's key does anything right now.
 *
 * Four of the thirteen module intents are gated on a flag rather than on the
 * session, and those four are here. The rest are gated on nothing, or on
 * something only the component can read — `sessionRef.current` and
 * `display()` against a live resolver, which is why `n`, `d`, `b` and `c` keep
 * a guard apiece in the residual dispatch.
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
 * tile a reader just clicked is itself a button, and gating `Escape` on
 * `onControl` would make it fire everywhere except the one moment it matters
 * most.
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
    if (!guards.correcting) return undefined
    // Not while the write is on the wire: taking the banner away would tell
    // the reader the correction was abandoned when it is still going to land.
    if (guards.submitting) return undefined
    return { action: { kind: 'cancel-correction' }, preventDefault: true }
  }
  // One dispatch path: every key, built-in or module-declared, has already
  // passed through `intentOf`, which applies the modifier guard once.
  if (!intent) return undefined
  // **Nothing reaches the page while the note editor is open.** It is a dialog
  // that takes text, so a keystroke means the note — whatever the browser
  // currently thinks is focused. `onControl` below only stands down while
  // focus is *in* a control, and a refused save or a stray click is enough to
  // break that: measured, with a server rejecting a write, the editor stayed
  // open holding the reader's text while focus sat on `<body>`, so typing
  // opened the settings panel instead of writing the note. `Escape` is handled
  // above and still closes the editor; everything else belongs to the note
  // until it does.
  if (guards.panel === 'edit') return undefined
  // Ahead of the `onControl` check, unlike everything below it: `Tab` moving
  // focus onto a control is not the same event as activating it, so there is
  // nothing for it to collide with.
  if (intent.kind === 'chrome') {
    return { action: { kind: 'toggle-chrome' }, preventDefault: false }
  }
  // A reader key belongs to the page, not to whatever a previous `Tab`
  // happened to focus. Without this, tabbing onto a mode button and pressing
  // Space to click it would also fire the edition toggle — not because two
  // listeners raced, but because a window-level listener has no way to know a
  // focused button is about to handle the very same keypress itself.
  if (guards.onControl) return undefined
  if (intent.kind === 'correct') {
    // Not while a correction is already on the wire. Without this,
    // `disabled={submitting}` blurs the just-clicked Confirm the instant the
    // write starts, so `onControl` no longer stands this down, and a `c` press
    // mid-write would silently replace the pending correction the completion
    // handler is about to resolve.
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
