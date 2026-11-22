/**
 * The keydown decision.
 *
 * Several of its branches exist because of a defect — a refused save dropping
 * focus to `<body>`, `disabled={submitting}` blurring the just-clicked Confirm.
 * Every test below is a state and an answer; nothing here mounts anything.
 */
import { describe, expect, it } from 'vitest'
import { intentOf, type Intent } from '../src/keys'
import { keyPlan, type Guards, type KeyAction } from '../src/keyplan'
import {
  NEW_INTENT,
  REDO_INTENT,
  REVEAL_INTENT,
  REVIEW_INTENT,
  UNDO_INTENT,
} from '../src/modules/annotations'
import { PANEL_INTENT } from '../src/modules/credits'
import { TOGGLE_INTENT as DIRECTION_INTENT } from '../src/modules/direction'
import { TOGGLE_INTENT as HELP_TOGGLE_INTENT } from '../src/modules/help'
import { registry } from '../src/modules/registry'
import { TOGGLE_INTENT as REGROUP_INTENT } from '../src/modules/regroup'
import { TOGGLE_INTENT as SETTINGS_INTENT } from '../src/modules/settings'
import {
  IN_INTENT as ZOOM_IN_INTENT,
  OUT_INTENT as ZOOM_OUT_INTENT,
  WHOLE_INTENT as ZOOM_WHOLE_INTENT,
} from '../src/modules/zoom'

/** Nothing open, nothing on the wire, focus on the page, everything switched on. */
const CALM: Guards = {
  helpOpen: false,
  panel: undefined,
  correcting: false,
  submitting: false,
  notesBusy: false,
  control: 'none',
  accountOpen: false,
  keyboardHoldsBars: false,
  settingsOpen: false,
  creditsOpen: false,
  inPanel: false,
  zoomable: true,
  creditsPanel: true,
  helpEnabled: true,
  mayCorrect: true,
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
   * **Checked before `intent`, and deliberately not gated on the focused
   * control.** `Escape` is not one of `intentOf`'s intents, and the tile a
   * reader just clicked to pick a counterpart is itself a button — gating this
   * on a focused control would make it fire everywhere except the one moment
   * it matters most.
   */
  it('cancels a correction even with focus in a control', () => {
    expect(
      act('Escape', undefined, { correcting: true, control: 'text' }),
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

  /**
   * With nothing open and the keyboard not holding the bars, `Escape` leaves
   * the reader by its way back. Bars a tap or the mouse brought up are not
   * the keyboard's until a key moves focus into them, so they do not take it.
   */
  it('leaves the reader where there is nothing open to back out of', () => {
    expect(keyPlan('Escape', undefined, guards())).toEqual({
      action: { kind: 'leave-reader' },
      preventDefault: true,
    })
  })

  /** A text field keeps the key with nothing open: `Escape` never leaves from one. */
  it('leaves a text field its own Escape with nothing open', () => {
    expect(
      keyPlan('Escape', undefined, guards({ control: 'text' })),
    ).toBeUndefined()
  })

  /** What is open still closes from a text field, the panel the field is in. */
  it('closes what is open from a text field', () => {
    expect(
      act('Escape', undefined, { panel: 'edit', control: 'text' }),
    ).toEqual({ kind: 'close-editor' })
    expect(
      act('Escape', undefined, { settingsOpen: true, control: 'text' }),
    ).toEqual({ kind: 'close-settings' })
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
      { keyboardHoldsBars: true },
      {},
    ]) {
      expect(keyPlan('Escape', undefined, guards(over))?.preventDefault).toBe(
        true,
      )
    }
  })
})

describe('keyPlan, Escape closes every panel (spec 2026-09-26 §3.3)', () => {
  it('closes Settings', () => {
    expect(act('Escape', undefined, { settingsOpen: true })).toEqual({
      kind: 'close-settings',
    })
  })

  it('closes Credits', () => {
    expect(act('Escape', undefined, { creditsOpen: true })).toEqual({
      kind: 'close-credits',
    })
  })

  it('leaves Help and Account first, as before', () => {
    expect(
      act('Escape', undefined, { helpOpen: true, settingsOpen: true }),
    ).toEqual({ kind: 'close-help' })
    expect(
      act('Escape', undefined, { accountOpen: true, settingsOpen: true }),
    ).toEqual({ kind: 'close-account' })
  })

  /**
   * Above the note panel and the correction, for the reason Help already
   * closes first: closing them discards nothing, where the note panel flushes
   * a note and the correction abandons one.
   */
  it('closes Settings and Credits before the note panel and the correction', () => {
    expect(
      act('Escape', undefined, { settingsOpen: true, panel: 'review' }),
    ).toEqual({ kind: 'close-settings' })
    expect(
      act('Escape', undefined, { creditsOpen: true, correcting: true }),
    ).toEqual({ kind: 'close-credits' })
    expect(
      act('Escape', undefined, { settingsOpen: true, creditsOpen: true }),
    ).toEqual({ kind: 'close-settings' })
  })
})

describe('keyPlan, Escape takes the bars away while the keyboard holds them (spec 2026-09-26 §3.1)', () => {
  it('hides the bars while the keyboard holds them and nothing above them is open', () => {
    expect(act('Escape', undefined, { keyboardHoldsBars: true })).toEqual({
      kind: 'leave-bars',
    })
  })

  /** The ring's way out, not the reader's: a focused bar stop is a button. */
  it('hides the bars from a focused stop', () => {
    expect(
      act('Escape', undefined, { keyboardHoldsBars: true, control: 'button' }),
    ).toEqual({ kind: 'leave-bars' })
  })

  it('spends itself on a correction first', () => {
    expect(
      act('Escape', undefined, { keyboardHoldsBars: true, correcting: true }),
    ).toEqual({ kind: 'cancel-correction' })
  })

  it('spends itself on Help, Settings and the note panel first', () => {
    expect(
      act('Escape', undefined, { keyboardHoldsBars: true, helpOpen: true }),
    ).toEqual({ kind: 'close-help' })
    expect(
      act('Escape', undefined, { keyboardHoldsBars: true, settingsOpen: true }),
    ).toEqual({ kind: 'close-settings' })
    expect(
      act('Escape', undefined, { keyboardHoldsBars: true, panel: 'review' }),
    ).toEqual({ kind: 'close-editor' })
  })

  /** A correction on the wire holds the key, and the bars with it. */
  it('does nothing while a correction is on the wire', () => {
    expect(
      keyPlan(
        'Escape',
        undefined,
        guards({ keyboardHoldsBars: true, correcting: true, submitting: true }),
      ),
    ).toBeUndefined()
  })
})

describe('keyPlan, the note editor', () => {
  /**
   * **Nothing reaches the page while the editor is open.** It is a dialog that
   * takes text, so a keystroke means the note — whatever the browser thinks is
   * focused. The control guard alone is not enough: with a server refusing a
   * write, the editor stays open holding the reader's text while focus sits
   * on `<body>`, so typing would open the settings panel instead of writing
   * the note.
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

describe('keyPlan, Tab (spec 2026-09-26 §3.1)', () => {
  /**
   * The reader moves focus round its ring itself. Leaving the move to the
   * browser would walk focus out of the page into the browser's own
   * frame (O135).
   */
  it('steps round the ring and prevents the browser’s own focus move', () => {
    expect(keyPlan('Tab', { kind: 'chrome' }, guards())).toEqual({
      action: { kind: 'ring' },
      preventDefault: true,
    })
  })

  it('steps round the ring from a focused button too', () => {
    expect(act('Tab', { kind: 'chrome' }, { control: 'button' })).toEqual({
      kind: 'ring',
    })
  })

  /** Focus in a text field outside any panel still steps round the ring. */
  it('steps round the ring from a focused text field too', () => {
    expect(act('Tab', { kind: 'chrome' }, { control: 'text' })).toEqual({
      kind: 'ring',
    })
  })

  /**
   * O161: without this guard the first `Tab` in an open panel would show the
   * bars behind it.
   */
  it('leaves Tab inside an open panel to the panel', () => {
    expect(
      keyPlan('Tab', { kind: 'chrome' }, guards({ inPanel: true })),
    ).toBeUndefined()
  })
})

describe('keyPlan, a focused control (spec 2026-09-26 §3.2)', () => {
  /**
   * Text entry keeps every key. Typing in the note editor, a Settings field or
   * a password box is not pressing reader keys.
   */
  it('text entry keeps a reader key, a correction and a module key alike', () => {
    const cases: [string, Intent][] = [
      ['ArrowRight', step],
      [' ', { kind: 'toggle' }],
      ['c', { kind: 'correct' }],
      ['s', mod(SETTINGS_INTENT)],
      ['=', mod(ZOOM_IN_INTENT)],
    ]
    for (const [key, intent] of cases) {
      expect(
        keyPlan(key, intent, guards({ control: 'text' })),
        key,
      ).toBeUndefined()
    }
  })

  /**
   * A button keeps the two keys that press it. Tabbing onto a mode button and
   * pressing Space would otherwise also swap the edition, because a
   * window-level listener cannot know the button is about to take the press.
   */
  it('a button keeps Space and Enter', () => {
    expect(
      keyPlan(' ', { kind: 'toggle' }, guards({ control: 'button' })),
    ).toBeUndefined()
    expect(
      keyPlan('Enter', mod(SETTINGS_INTENT), guards({ control: 'button' })),
    ).toBeUndefined()
  })

  /** Every other key goes to the reader: `→` turns the page from ⚙. */
  it('a button hands every other key to the reader', () => {
    expect(act('ArrowRight', step, { control: 'button' })).toEqual({
      kind: 'reading',
      intent: step,
    })
    expect(act('s', mod(SETTINGS_INTENT), { control: 'button' })).toEqual({
      kind: 'module',
      intent: SETTINGS_INTENT,
    })
    expect(act('c', { kind: 'correct' }, { control: 'button' })).toEqual({
      kind: 'begin-correction',
    })
  })

  it('the page itself keeps nothing, Space included', () => {
    expect(act(' ', { kind: 'toggle' }, { control: 'none' })).toEqual({
      kind: 'reading',
      intent: { kind: 'toggle' },
    })
  })
})

describe('keyPlan, correction', () => {
  it('opens correction mode', () => {
    expect(act('c', { kind: 'correct' })).toEqual({ kind: 'begin-correction' })
  })

  /** O58: the server refuses anyone else's write, so the key is not offered and the browser keeps it. */
  it('leaves c to the browser for an account that may not correct', () => {
    expect(
      keyPlan('c', { kind: 'correct' }, guards({ mayCorrect: false })),
    ).toBeUndefined()
    expect(act('c', { kind: 'correct' }, { mayCorrect: true })).toEqual({
      kind: 'begin-correction',
    })
  })

  /**
   * **Without this, `c` mid-write silently replaces the correction the
   * completion handler is about to resolve.** Nothing else stops it: a
   * focused button keeps only `Space` and `Enter` (spec 2026-09-26 §3.2), and
   * `disabled={submitting}` blurs the just-clicked Confirm the instant the
   * write starts anyway.
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
   * separate answer from `undefined`.** Every one of the gates above swallows
   * the browser's own default; folding them into "not ours" would quietly hand
   * those keys back.
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

describe('keyPlan, the library key', () => {
  it('L leaves the reader', () => {
    expect(act('L', { kind: 'library' })).toEqual({ kind: 'leave-reader' })
  })

  it('L does nothing while the account screen is open', () => {
    expect(act('L', { kind: 'library' }, { accountOpen: true })).toBeUndefined()
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
    expect(press('Tab')?.action).toEqual({ kind: 'ring' })
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
   * the control guard and `account` must not.**
   *
   * `intentOf` bails only on ctrl/meta/alt, not Shift, so `READER_KEYS`'s `A`
   * fires for a shifted `a` typed anywhere. With the account branch above the
   * guard and `preventDefault: true`, a reader whose password contains a
   * capital A could not type it into the login field, the account panel's
   * current or new password fields, or the key-name box: the character would
   * never reach the input and the panel would open instead.
   */
  it('leaves A alone when focus is in a control, unlike Tab', () => {
    expect(act('A', { kind: 'account' }, { control: 'text' })).toBeUndefined()
    // Unlike A: Tab moving focus onto a control is not activating it, so text
    // entry does not keep it — the ring still claims the key.
    expect(act('Tab', { kind: 'chrome' }, { control: 'text' })).toEqual({
      kind: 'ring',
    })
    // And it opens the panel when nothing is focused, which would be a
    // regression if it broke.
    expect(act('A', { kind: 'account' })).toEqual({ kind: 'open-account' })
  })

  /**
   * **`Escape` belongs to whatever is on top.** Without the account panel in
   * `Guards` it would fall through to `cancel-correction`, so a reader
   * dismissing the panel would silently abandon a pending correction —
   * the panel would stay open and the correction be gone, with nothing
   * saying either.
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
   * The account panel stands the reader keys down, exactly as the note
   * editor does. Consulting it only for `Escape` would leave arrows turning
   * the hidden reader behind the modal — and the debounced write recording
   * that position.
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
