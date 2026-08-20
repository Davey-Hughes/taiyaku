/**
 * The keydown decision, which was thirteen early returns in `App.tsx` and so
 * was reachable by nothing.
 *
 * Several of those returns exist because of a defect somebody watched happen —
 * a refused save dropping focus to `<body>`, `disabled={submitting}` blurring
 * the just-clicked Confirm — and until this file there was no way to write the
 * state down, let alone assert on it. Every test below is a state and an
 * answer; nothing here mounts anything.
 */
import { describe, expect, it } from 'vitest'
import { intentOf, type Intent } from './keys'
import { keyPlan, type Guards, type KeyAction } from './keyplan'
import {
  NEW_INTENT,
  REDO_INTENT,
  REVEAL_INTENT,
  REVIEW_INTENT,
  UNDO_INTENT,
} from './modules/annotations'
import { PANEL_INTENT } from './modules/credits'
import { TOGGLE_INTENT as DIRECTION_INTENT } from './modules/direction'
import { TOGGLE_INTENT as HELP_TOGGLE_INTENT } from './modules/help'
import { registry } from './modules/registry'
import { TOGGLE_INTENT as REGROUP_INTENT } from './modules/regroup'
import { TOGGLE_INTENT as SETTINGS_INTENT } from './modules/settings'
import {
  IN_INTENT as ZOOM_IN_INTENT,
  OUT_INTENT as ZOOM_OUT_INTENT,
  WHOLE_INTENT as ZOOM_WHOLE_INTENT,
} from './modules/zoom'

/** Nothing open, nothing on the wire, focus on the page, everything switched on. */
const CALM: Guards = {
  helpOpen: false,
  panel: undefined,
  correcting: false,
  submitting: false,
  notesBusy: false,
  onControl: false,
  accountOpen: false,
  zoomable: true,
  creditsPanel: true,
  helpEnabled: true,
}

const guards = (over: Partial<Guards> = {}): Guards => ({ ...CALM, ...over })

const mod = (intent: string): Intent => ({ kind: 'module', intent })
const step: Intent = { kind: 'step', by: 1 }

/** What the plan says, or `undefined` — the shape most assertions below want. */
const act = (
  key: string,
  intent: Intent | undefined,
  over: Partial<Guards> = {},
): KeyAction | undefined => keyPlan(key, intent, guards(over))?.action

describe('keyPlan, Escape', () => {
  /**
   * **Checked before `intent`, and deliberately not gated on `onControl`.**
   * `Escape` is not one of `intentOf`'s intents, and the tile a reader just
   * clicked to pick a counterpart is itself a button — gating this on a
   * focused control would make it fire everywhere except the one moment it
   * matters most.
   */
  it('cancels a correction even with focus in a control', () => {
    expect(
      act('Escape', undefined, { correcting: true, onControl: true }),
    ).toEqual({
      kind: 'cancel-correction',
    })
  })

  /** A modifier makes `intentOf` answer nothing, and `Escape` still means `Escape`. */
  it('does not need an intent at all', () => {
    expect(act('Escape', undefined, { correcting: true })).toEqual({
      kind: 'cancel-correction',
    })
  })

  it('does nothing where there is nothing open to back out of', () => {
    expect(keyPlan('Escape', undefined, guards())).toBeUndefined()
  })

  /**
   * The help overlay closes first because closing it discards nothing, while
   * every branch under it either abandons a correction or flushes a note. If
   * both are open, spending the keypress on the one that can be reopened with
   * `?` is the cheaper mistake.
   */
  it('closes the help overlay ahead of a correction and a note', () => {
    expect(
      act('Escape', undefined, {
        helpOpen: true,
        correcting: true,
        panel: 'edit',
      }),
    ).toEqual({ kind: 'close-help' })
  })

  /**
   * Not gated on `helpEnabled`, unlike the toggle: a reader who switched the
   * module off while the overlay was up would otherwise be left with a panel
   * whose own keys have just gone inert.
   */
  it('closes the help overlay even with the module switched off', () => {
    expect(
      act('Escape', undefined, { helpOpen: true, helpEnabled: false }),
    ).toEqual({ kind: 'close-help' })
  })

  it('closes the note editor', () => {
    expect(act('Escape', undefined, { panel: 'edit' })).toEqual({
      kind: 'close-editor',
    })
    expect(act('Escape', undefined, { panel: 'review' })).toEqual({
      kind: 'close-editor',
    })
  })

  /** A reader mid-correction pressing `Escape` means the correction, which is the irreversible one. */
  it('prefers the correction over the note panel when both are open', () => {
    expect(
      act('Escape', undefined, { correcting: true, panel: 'edit' }),
    ).toEqual({ kind: 'cancel-correction' })
  })

  /**
   * Both of these would tell the reader something was abandoned while it is
   * still going to land — the one thing `submitting` and `notesBusy` exist to
   * prevent.
   */
  it('does nothing while either write is on the wire', () => {
    expect(
      keyPlan('Escape', undefined, guards({ panel: 'edit', notesBusy: true })),
    ).toBeUndefined()
    expect(
      keyPlan(
        'Escape',
        undefined,
        guards({ correcting: true, submitting: true }),
      ),
    ).toBeUndefined()
  })

  it('prevents the default on every answer it gives', () => {
    for (const over of [
      { helpOpen: true },
      { panel: 'edit' as const },
      { correcting: true },
    ]) {
      expect(keyPlan('Escape', undefined, guards(over))?.preventDefault).toBe(
        true,
      )
    }
  })
})

describe('keyPlan, the note editor', () => {
  /**
   * **Nothing reaches the page while the editor is open.** It is a dialog that
   * takes text, so a keystroke means the note — whatever the browser thinks is
   * focused. `onControl` alone is not enough: measured, with a server refusing
   * a write, the editor stayed open holding the reader's text while focus sat
   * on `<body>`, so typing opened the settings panel instead of writing the
   * note.
   */
  it('refuses every key but Escape while the editor is open, focus or no focus', () => {
    for (const intent of [
      step,
      { kind: 'chrome' } as Intent,
      { kind: 'correct' } as Intent,
      mod(SETTINGS_INTENT),
      mod(NEW_INTENT),
    ]) {
      expect(keyPlan('x', intent, guards({ panel: 'edit' }))).toBeUndefined()
    }
  })

  /** The review list is not a textarea, so the page keys still work under it. */
  it('lets keys through while the review list is open', () => {
    expect(act('x', step, { panel: 'review' })).toEqual({
      kind: 'reading',
      intent: step,
    })
  })
})

describe('keyPlan, Tab', () => {
  /**
   * **The one answer with `preventDefault: false`.** Toggling the chrome is
   * layered on top of the browser's own focus movement rather than replacing
   * it, and preventing it is what made every focusable control in the reader —
   * the mode buttons, all of the filmstrip's tiles, `Gap`'s offered moves —
   * mouse-only.
   */
  it('toggles the chrome without preventing the browser’s own focus move', () => {
    const plan = keyPlan('Tab', { kind: 'chrome' }, guards())
    expect(plan).toEqual({
      action: { kind: 'toggle-chrome' },
      preventDefault: false,
    })
  })

  /**
   * Ahead of the control check, unlike everything below it: moving focus onto
   * a control is not the same event as activating it, so `Tab` has nothing to
   * collide with.
   */
  it('toggles the chrome even with focus in a control', () => {
    expect(act('Tab', { kind: 'chrome' }, { onControl: true })).toEqual({
      kind: 'toggle-chrome',
    })
  })
})

describe('keyPlan, a focused control', () => {
  /**
   * A reader key belongs to the page, not to whatever a previous `Tab`
   * happened to focus: tabbing onto a mode button and pressing Space to click
   * it would otherwise also fire the edition toggle, because a window-level
   * listener has no way to know the button is about to handle the same press.
   */
  it('stands down for a reader key, a correction and a module key alike', () => {
    for (const intent of [
      step,
      { kind: 'toggle' } as Intent,
      { kind: 'correct' } as Intent,
      mod(SETTINGS_INTENT),
      mod(ZOOM_IN_INTENT),
    ]) {
      expect(keyPlan('x', intent, guards({ onControl: true }))).toBeUndefined()
    }
  })
})

describe('keyPlan, correction', () => {
  it('opens correction mode', () => {
    expect(act('c', { kind: 'correct' })).toEqual({ kind: 'begin-correction' })
  })

  /**
   * **Without this, `c` mid-write silently replaces the correction the
   * completion handler is about to resolve.** `disabled={submitting}` blurs
   * the just-clicked Confirm the instant the write starts, so `onControl` no
   * longer stands the handler down — which is exactly why this guard is not
   * redundant with the one above it.
   */
  it('refuses while a correction is on the wire', () => {
    expect(
      keyPlan('c', { kind: 'correct' }, guards({ submitting: true })),
    ).toBeUndefined()
  })

  /** A second `c` while merely asserting is fine — it is only the wire that is guarded. */
  it('allows a second c while a correction is being asserted', () => {
    expect(act('c', { kind: 'correct' }, { correcting: true })).toEqual({
      kind: 'begin-correction',
    })
  })
})

describe('keyPlan, module keys', () => {
  const ungated = [
    DIRECTION_INTENT,
    REGROUP_INTENT,
    NEW_INTENT,
    REVEAL_INTENT,
    UNDO_INTENT,
    REDO_INTENT,
    SETTINGS_INTENT,
  ]

  it('routes an ungated module key straight through', () => {
    for (const intent of ungated) {
      expect(act('x', mod(intent))).toEqual({ kind: 'module', intent })
    }
  })

  it('prevents the default on a module key', () => {
    expect(keyPlan('x', mod(REVEAL_INTENT), guards())?.preventDefault).toBe(
      true,
    )
  })

  /**
   * The three zoom keys share one guard — `modules/zoom.ts`'s `zoomable`, the
   * same predicate the pan press is gated on. Inert exactly where the
   * `Viewport` wrapper is not mounted or has nothing under it: scroll mode and
   * a gap. Without it, `=` in scroll mode silently zooms a rect nobody is
   * drawing and the reader finds it applied on switching back to single.
   */
  it('makes the three zoom keys inert where nothing is drawn to zoom', () => {
    for (const intent of [ZOOM_IN_INTENT, ZOOM_OUT_INTENT, ZOOM_WHOLE_INTENT]) {
      expect(act('=', mod(intent), { zoomable: true })).toEqual({
        kind: 'module',
        intent,
      })
      expect(act('=', mod(intent), { zoomable: false })).toEqual({
        kind: 'inert',
      })
    }
  })

  it('makes C inert where the credits panel is switched off', () => {
    expect(act('C', mod(PANEL_INTENT))).toEqual({
      kind: 'module',
      intent: PANEL_INTENT,
    })
    expect(act('C', mod(PANEL_INTENT), { creditsPanel: false })).toEqual({
      kind: 'inert',
    })
  })

  it('makes ? and h inert where the help module is switched off', () => {
    expect(act('?', mod(HELP_TOGGLE_INTENT), { helpEnabled: false })).toEqual({
      kind: 'inert',
    })
  })

  /** `a` swapping the panel mid-write would say the note was abandoned while it is still going to land. */
  it('makes a inert while a note write is on the wire', () => {
    expect(act('a', mod(REVIEW_INTENT), { notesBusy: true })).toEqual({
      kind: 'inert',
    })
    expect(act('a', mod(REVIEW_INTENT))).toEqual({
      kind: 'module',
      intent: REVIEW_INTENT,
    })
  })

  /**
   * **`inert` still prevents the default, and that is the point of it being a
   * separate answer from `undefined`.** The handler this replaced called
   * `preventDefault()` *before* the module switch, so every one of the gates
   * above already swallowed the browser's own default; folding them into
   * "not ours" would have quietly handed those keys back.
   */
  it('claims the key it makes inert, rather than handing it back', () => {
    const plan = keyPlan('=', mod(ZOOM_IN_INTENT), guards({ zoomable: false }))
    expect(plan).toEqual({ action: { kind: 'inert' }, preventDefault: true })
  })

  /** A module registered later gets no gate here by default — it does something, and its own case says what. */
  it('routes an intent this module has never heard of', () => {
    expect(act('x', mod('future:thing'))).toEqual({
      kind: 'module',
      intent: 'future:thing',
    })
  })
})

describe('keyPlan, reader keys', () => {
  it('routes a step, a toggle and a digit as reading intents', () => {
    const view: Intent = { kind: 'view', edition: 'en' }
    expect(act('ArrowRight', step)).toEqual({ kind: 'reading', intent: step })
    expect(act(' ', { kind: 'toggle' })).toEqual({
      kind: 'reading',
      intent: { kind: 'toggle' },
    })
    expect(act('2', view)).toEqual({ kind: 'reading', intent: view })
  })

  it('prevents the default on a reader key', () => {
    expect(keyPlan('ArrowRight', step, guards())?.preventDefault).toBe(true)
  })

  it('does nothing for a key no binding claims', () => {
    expect(keyPlan('q', undefined, guards())).toBeUndefined()
  })

  /** Neither `submitting` nor `notesBusy` gates the page keys — a correction on the wire does not stop reading. */
  it('keeps reading while a correction is on the wire', () => {
    expect(
      act('ArrowRight', step, { submitting: true, correcting: true }),
    ).toEqual({ kind: 'reading', intent: step })
  })
})

/**
 * The plan against the real key table, rather than against hand-built intents
 * — so a key whose binding moves cannot leave these tests passing about a
 * keypress that no longer produces that intent.
 */
describe('keyPlan, against intentOf', () => {
  const press = (key: string, over: Partial<Guards> = {}) => {
    const event = {
      key,
      ctrlKey: false,
      metaKey: false,
      altKey: false,
    } as KeyboardEvent
    return keyPlan(
      key,
      intentOf(event, ['a', 'b'], registry.all()),
      guards(over),
    )
  }

  it('answers the reader’s own keys', () => {
    expect(press('ArrowRight')?.action).toEqual({
      kind: 'reading',
      intent: { kind: 'step', by: 1 },
    })
    expect(press(' ')?.action).toEqual({
      kind: 'reading',
      intent: { kind: 'toggle' },
    })
    expect(press('1')?.action).toEqual({
      kind: 'reading',
      intent: { kind: 'view', edition: 'a' },
    })
    expect(press('Tab')?.action).toEqual({ kind: 'toggle-chrome' })
    expect(press('c')?.action).toEqual({ kind: 'begin-correction' })
  })

  /** A digit past the work's editions is silent rather than offered on to a module. */
  it('is silent for a digit the work has no edition for', () => {
    expect(press('9')).toBeUndefined()
  })

  it('answers each registered module’s declared key', () => {
    expect(press('d')?.action).toEqual({
      kind: 'module',
      intent: DIRECTION_INTENT,
    })
    expect(press('r')?.action).toEqual({
      kind: 'module',
      intent: REVEAL_INTENT,
    })
    expect(press('z')?.action).toEqual({ kind: 'module', intent: UNDO_INTENT })
    expect(press('Z')?.action).toEqual({ kind: 'module', intent: REDO_INTENT })
  })

  /**
   * **`z`/`Z`, not `u`/`U`.** `intentOf` yields any key carrying a modifier so
   * a reader's own `ctrl+z` always wins, which is why undo is an unmodified
   * key at all — and pinned here because a plan that routed the wrong pair
   * would look right in every test written against the intent directly.
   */
  it('leaves a modified key entirely to the browser', () => {
    const ctrlZ = {
      key: 'z',
      ctrlKey: true,
      metaKey: false,
      altKey: false,
    } as KeyboardEvent
    expect(
      keyPlan('z', intentOf(ctrlZ, ['a', 'b'], registry.all()), guards()),
    ).toBeUndefined()
  })

  it('makes 0 zoom to whole rather than selecting a tenth edition', () => {
    expect(press('0')?.action).toEqual({
      kind: 'module',
      intent: ZOOM_WHOLE_INTENT,
    })
  })

  /**
   * **`A` is a character, and `Tab` is not — which is why `chrome` sits above
   * the `onControl` guard and `account` must not.**
   *
   * `intentOf` bails only on ctrl/meta/alt, not Shift, so `READER_KEYS`'s `A`
   * fires for a shifted `a` typed anywhere. With the account branch above the
   * guard and `preventDefault: true`, a reader whose password contains a
   * capital A could not type it into the login field, the account panel's
   * current or new password fields, or the key-name box: the character never
   * reached the input and the panel opened instead. Found by the whole-branch
   * review, 2026-08-18.
   */
  it('leaves A alone when focus is in a control, unlike Tab', () => {
    expect(act('A', { kind: 'account' }, { onControl: true })).toBeUndefined()
    // And still opens the panel when nothing is focused, which is the half that
    // would make this a regression rather than a fix if it broke.
    expect(act('A', { kind: 'account' })).toEqual({ kind: 'open-account' })
  })

  /**
   * **`Escape` belongs to whatever is on top.** Without the account panel in
   * `Guards` it fell through to `cancel-correction`, so a reader dismissing the
   * panel silently abandoned a pending correction — the panel stayed open and
   * the correction was gone, with nothing saying either. The note editor
   * already had exactly this guard.
   *
   * Found by the second whole-branch review.
   */
  it('closes the account panel on Escape rather than cancelling a correction', () => {
    expect(
      act('Escape', undefined, { accountOpen: true, correcting: true }),
    ).toEqual({ kind: 'close-account' })
    // And with no panel open, a correction is still what Escape abandons.
    expect(act('Escape', undefined, { correcting: true })).toEqual({
      kind: 'cancel-correction',
    })
  })

  /**
   * The account panel stands the reader keys down, exactly as the note editor
   * does. Consulting it only for `Escape` left arrows turning the hidden reader
   * behind the modal — and the debounced write recording that position.
   *
   * Found by the third whole-branch review.
   */
  it('stands the reader keys down while the account panel is open', () => {
    for (const key of ['ArrowRight', 'ArrowLeft', ' ', 'c', '1']) {
      expect(
        act(key, { kind: 'step', by: 1 }, { accountOpen: true }),
        `${key} reached the page behind the panel`,
      ).toBeUndefined()
    }
    // Escape still closes it, which is the one key it does answer.
    expect(act('Escape', undefined, { accountOpen: true })).toEqual({
      kind: 'close-account',
    })
  })
})
