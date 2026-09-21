import { useRef, type CSSProperties } from 'react'
import type { ReaderModule } from './module'
import { fieldsOf } from './settingsPanel'
import { FieldControl } from './settingsFields'

// A field's row, handed to `FieldControl`, which takes its layout as an
// object because the library's own settings pass theirs (`settingsFields.tsx`).
// Colour and type are `.reader-dialog-panel`'s, in `styles/reader.css`, like
// the rest of this panel's look (spec 2026-09-17 §8.5).
const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 12,
  padding: '6px 0',
}

export interface SettingsProps {
  /** Whether the panel is on screen at all — see `modules/settings.ts`'s `s` key. */
  visible: boolean
  /** Every registered module, in registry order (`module.ts`'s `Registry.all()`) — one section per entry, generated rather than named here. */
  modules: ReaderModule<unknown>[]
  /**
   * Each module's current preference slice, keyed by `ReaderModule.id`. A
   * module with no entry here (nothing loaded yet) renders its `defaults`.
   *
   * Typed `object` rather than `Record<string, unknown>`: every module's own
   * prefs type (`DirectionPrefs`, `MotionPrefs`, …) is a plain interface with
   * no index signature, so it structurally satisfies `object` but not
   * `Record<string, unknown>` — TypeScript requires an explicit index
   * signature for the latter, which no module declares and none should have
   * to just to be handed to this panel. `FieldControl`'s own read narrows
   * with a cast at the one place a key is actually looked up by name.
   */
  prefs: Record<string, object>
  /**
   * One sentence about a module's own situation on this work, keyed by
   * `ReaderModule.id`, shown below that module's fields as secondary text.
   *
   * A caller-computed map rather than something this file derives, the same
   * way `App.tsx`'s `<CreditsPanel>` a few lines below its `<Settings>` call
   * is handed `work`/`edition`/`pages` rather than working them out itself:
   * this panel is generated from every module's `defaults` and must stay
   * ignorant of what any one module's fields *mean*, so a module whose toggle
   * can be a no-op for reasons specific to the work being read (`modules/borrow.ts`'s
   * `borrowNote`, today) hands the finished sentence in rather than this file
   * learning that module's id. A module with no entry here renders exactly as
   * it always has — this is additive, not a replacement for any field.
   */
  notes?: Record<string, string>
  /** One field changed for one module — `raw` is whatever the control's own change event handed back; `settingsPanel.ts`'s `withFieldValue` is what a caller coerces it with. */
  onChange: (id: string, key: string, raw: unknown) => void
  /** Reset one module's whole preference slice back to its defaults. */
  onReset: (id: string) => void
  /**
   * Close the panel. Its own button because ⚙ opens it by pointer (spec
   * 2026-09-17 §8.3), and `s` — the only other way out — is a key.
   */
  onClose: () => void
}

/**
 * The settings panel: one section per registered module, one control per
 * key of that module's `defaults`, both generated from `settingsPanel.ts`'s
 * pure inference rather than named per module here. A seventh module needs
 * no edit to this file — only a `defaults` shape for `fieldsOf` to read.
 *
 * A module whose `defaults` has no fields (`modules/settings.ts`'s own
 * `settingsModule`, today) renders no section at all: `fieldsOf` reports an
 * empty list, and a heading over zero controls would be a blank shelf
 * asking to be filled by something nobody built. That's a deliberate
 * consequence of generation, not a special case written for this one
 * module's id.
 */
export function Settings({
  visible,
  modules,
  prefs,
  notes,
  onChange,
  onReset,
  onClose,
}: SettingsProps) {
  // Declared before the `if (!visible) return null` guard below — a hook
  // after an early return does not run on every render.
  const fromScrim = useRef(false)
  const upOnScrim = useRef(false)
  if (!visible) return null
  return (
    // The scrim dismisses on a press that goes down *and* comes up on its
    // own scrim (spec §9.2). Measured on 2026-09-20: this element's box is
    // the whole viewport, so the tap ④ gives every other panel — a press on
    // the page area, which `gestures.ts` answers with `close-panels` —
    // cannot reach the page at all while this is open. Without this,
    // Settings is the one panel that tap cannot reach. Its Close button below
    // is already a pointer exit and has been since style pass ③; what it
    // lacked was the tap-outside ④ gives every other panel, on the panel
    // O135 records as closing on neither `Escape` nor `s` once ⚙ has
    // opened it.
    //
    // **Both ends are recorded from `pointerdown`/`pointerup` rather than
    // read off `click`.** *Fixed 2026-09-20, I4 of the whole-branch review.*
    // A DOM `click` fires on the nearest common ancestor of the down and up
    // targets, which for *either* mixed direction — down on a control and up
    // on the scrim, or down on the scrim and up on a control — is this
    // element either way, so `click`'s own target cannot tell the two
    // apart. A press that begins on a control and releases on the scrim is
    // correctly refused by `fromScrim` alone, but nothing was guarding the
    // reverse: a press down on the scrim and up on a control closed the
    // dialog and lost the panel under it. `fromScrim` and `upOnScrim` are
    // each set from the real `pointerdown`/`pointerup` target — which does
    // not go through `click`'s ancestor collapse — and `onClick` requires
    // both before it dismisses.
    //
    // **`upOnScrim` cannot be read off `pointerup`'s target, and the obvious
    // form above — `event.target === event.currentTarget`, exactly like
    // `fromScrim` — is wrong on a touch screen.** *Fixed 2026-09-20, I1 of
    // the second whole-branch review.* A touch pointer gets implicit pointer
    // capture on `pointerdown`, so the browser dispatches every later event
    // for that pointer — `pointerup` included — at the *capture* element
    // rather than at whatever is really under the finger. For a press down
    // on the scrim and up over the panel, `pointerup`'s target is still the
    // scrim on a touch screen, so the obvious form reads `upOnScrim` as true
    // and I4's press — down on the dim background, sweep onto the panel,
    // lift — still closes the dialog and loses the panel under it. A mouse
    // does not retarget, which is why a mouse-shaped test for this passed and
    // could not see it. `upOnScrim` is instead read by a hit test against
    // where the pointer actually is on release, which capture does not
    // move — measured in Chromium with touch emulation to return the panel
    // even while `pointerup`'s own `target` is the scrim.
    //
    // biome-ignore lint/a11y/useKeyWithClickEvents: the keyboard equivalent is the panel's own Close button, a few lines down — Escape and s dismissing it too is O135's, the accessibility pass's, not this task's.
    <div
      className="reader-dialog"
      role="dialog"
      aria-label="Settings"
      onPointerDown={(event) => {
        fromScrim.current = event.target === event.currentTarget
      }}
      onPointerUp={(event) => {
        upOnScrim.current =
          document.elementFromPoint(event.clientX, event.clientY) ===
          event.currentTarget
      }}
      onClick={(event) => {
        if (
          fromScrim.current &&
          upOnScrim.current &&
          event.target === event.currentTarget
        )
          onClose()
      }}
    >
      <div className="reader-dialog-panel">
        <div className="reader-panel-head">
          <h2 className="reader-panel-title">Settings</h2>
          <button type="button" className="reader-button" onClick={onClose}>
            Close
          </button>
        </div>
        {modules.map((mod) => {
          const fields = fieldsOf(mod.defaults as object, mod.options)
          if (fields.length === 0) return null
          const modulePrefs = (prefs[mod.id] ?? mod.defaults) as Record<
            string,
            unknown
          >
          const note = notes?.[mod.id]
          return (
            <section key={mod.id} className="reader-section">
              <div className="reader-section-head">
                <h3 className="reader-section-title">{mod.label}</h3>
                <button
                  type="button"
                  className="reader-button"
                  onClick={() => onReset(mod.id)}
                >
                  Reset
                </button>
              </div>
              {fields.map((field) => (
                <FieldControl
                  key={field.key}
                  field={field}
                  value={modulePrefs[field.key]}
                  onChange={(raw) => onChange(mod.id, field.key, raw)}
                  rowStyle={rowStyle}
                />
              ))}
              {note && <p className="reader-meta reader-note">{note}</p>}
            </section>
          )
        })}
      </div>
    </div>
  )
}
