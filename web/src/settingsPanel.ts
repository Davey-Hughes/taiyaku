/**
 * The pure half of the generated settings panel: given a module's `defaults`
 * (`ReaderModule.defaults` — see `module.ts`), infer what control each key
 * wants, and how a control's raw input value writes back into that slice.
 *
 * Kept out of `Settings.tsx` deliberately. This is the "what control does
 * this value's type want" logic the design calls out by name as pure and
 * testable without a DOM — the same reasoning `decode.ts` and
 * `modules/motion.ts`'s `scheduleFade` already apply to their own pieces of
 * `App.tsx`/`Chrome.tsx` wiring. Putting the inference here is what lets it
 * be pinned by a fast, DOM-free test.
 */

/** A number preference drawn as a slider over positions 0 to `last`. */
export interface SliderScale {
  /** The last position; the input steps by 1, and the default sits at `last / 2`. */
  last: number
  /** The value at a position. */
  valueAt: (position: number) => number
  /** The nearest position to a value, held to 0 to `last`. */
  positionOf: (value: number) => number
  /** The value's unit as written beside the name: `144 px`. */
  unit: string
  /** The unit as read aloud, for `aria-valuetext`: `144 pixels`. */
  spokenUnit: string
}

/** What kind of control a preference key's current value wants. */
export type FieldKind =
  | 'boolean'
  | 'number'
  | 'string'
  | 'select'
  | 'slider'
  | 'object'

/** One row the generated panel renders, before it is turned into a control. */
export interface Field {
  key: string
  kind: FieldKind
  /** The values a `'select'` field may take, in the order the module declared them. */
  options?: string[]
  /** A `'slider'`'s scale, as its module declared it. */
  scale?: SliderScale
  /** A `'slider'`'s default: under the mark, and what a double-click restores. */
  initial?: number
}

/**
 * Classify a preference value by its runtime type, the way `Settings.tsx`
 * turns a module's `defaults` into controls: `boolean` → checkbox, `number`
 * → numeric input, `string` → text. A select or a slider is its module's
 * declaration on a string or a number, which `fieldsOf` reads, not this.
 *
 * Everything else — a nested `Record` such as `direction`'s `override` or
 * `regroup`'s `breaks`, but also `null`, `undefined` and an array, none of
 * which any module actually stores — falls to `'object'` rather than being
 * guessed at as one of the three primitive kinds. Rendering `{}` into a
 * checkbox or a number input would be a broken control, not a degraded one,
 * and `'object'` is what tells `Settings.tsx` to render the reset-only
 * summary instead (see `nestedSummary`).
 */
export function fieldKind(value: unknown): FieldKind {
  if (typeof value === 'boolean') return 'boolean'
  if (typeof value === 'number') return 'number'
  if (typeof value === 'string') return 'string'
  return 'object'
}

/**
 * One `Field` per key of `defaults`, in the object's own declaration order —
 * the order every module already writes its `defaults` literal in, so a
 * module's fields appear in the panel in the same order its own source
 * reads, with nothing here free to reorder them.
 *
 * Empty for a module whose `defaults` is `{}` (`modules/settings.ts`'s own
 * `settingsModule`), which is what lets `Settings.tsx` skip a section with
 * nothing to show instead of rendering an empty one.
 */
export function fieldsOf<P extends object>(
  defaults: P,
  options: Record<string, string[]> = {},
  sliders: Record<string, SliderScale> = {},
): Field[] {
  return Object.entries(defaults).map(([key, value]) => {
    const kind = fieldKind(value)
    // A string with declared values is a choice, not free text. Only strings:
    // options against a boolean or a number would be a module contradicting
    // its own default's type, and inferring from the *value* is what keeps
    // this panel from having to know which module it is looking at.
    if (kind === 'string' && options[key]?.length)
      return { key, kind: 'select', options: options[key] }
    // A number with a declared scale is a slider, by the same rule. Own keys
    // only: an inherited `toString` is no scale.
    if (kind === 'number' && Object.hasOwn(sliders, key))
      return { key, kind: 'slider', scale: sliders[key], initial: value }
    return { key, kind }
  })
}

/**
 * Apply one control's raw change to a preference slice, coercing `raw`
 * against the *current* value's own type rather than trusting the caller —
 * a `<input type="number">` element hands back a string on every change
 * event, so a number field must parse it, and a checkbox hands back a
 * boolean already, so a boolean field only needs to accept it as-is.
 *
 * Refuses three ways rather than corrupting the slice:
 *
 * - `key` not present in `prefs` at all: returned unchanged. A settings
 *   panel generated from a *different* build of a module than the one that
 *   saved `prefs` (an older stored slice, or a stale closure) must not grow
 *   a field nothing declared, the same reasoning `prefs.ts`'s `loadPrefs`
 *   merge already applies at the storage boundary — this is the same
 *   discipline one layer up, at the edit boundary.
 * - The current value is object-shaped (`fieldKind` reports `'object'`):
 *   returned unchanged, unconditionally. `Settings.tsx` never renders an
 *   editable control for one of these — see `fieldKind`'s own doc — so
 *   reaching this branch at all would mean a caller bypassed the generated
 *   panel; refusing here is what keeps a per-edition record like
 *   `direction`'s `override` from ever being overwritten with a plain
 *   string by a code path that forgot that rule.
 * - A number field's `raw` does not parse to a finite number (`'abc'`, or an
 *   input mid-edit at `''`): returned unchanged rather than writing `NaN`
 *   into storage, where `prefs.ts`'s `loadPrefs` would hand it straight back
 *   out on the next load with nothing downstream expecting it.
 */
export function withFieldValue<P extends object>(
  prefs: P,
  key: string,
  raw: unknown,
): P {
  const current = (prefs as Record<string, unknown>)[key]
  if (!(key in prefs)) return prefs
  if (typeof current === 'boolean') return { ...prefs, [key]: Boolean(raw) }
  if (typeof current === 'number') {
    const parsed = typeof raw === 'number' ? raw : Number(raw)
    return Number.isFinite(parsed) ? { ...prefs, [key]: parsed } : prefs
  }
  // A select hands back one of its own declared values, and a text box hands
  // back whatever was typed; both are strings, and neither is checked against
  // the module's options here. `noteColour`'s reasoning applies — a value this
  // build does not recognise falls back at the point of *use*, where the
  // consequence is a default rather than a preference that refuses to save.
  if (typeof current === 'string') return { ...prefs, [key]: String(raw) }
  return prefs
}

/**
 * How many entries a nested-record field (`direction`'s `override`,
 * `regroup`'s `breaks`) currently holds — the count `Settings.tsx` shows in
 * place of an editable control. `0` for anything that is not itself a plain
 * object, rather than throwing: this is display-only, called on a value
 * `fieldKind` already reported as `'object'`, and a defensive fallback here
 * costs nothing against a value shaped differently than expected.
 */
export function nestedCount(value: unknown): number {
  if (typeof value !== 'object' || value === null) return 0
  return Object.keys(value).length
}

/**
 * `nestedCount`'s number, worded for the panel: `'none set'` at zero rather
 * than `'0 set'`, so an edition with no per-edition override reads as a
 * plain fact rather than a number that looks like it might be a bug.
 */
export function nestedSummary(count: number): string {
  return count === 0 ? 'none set' : `${count} set`
}

/**
 * A preference key as a label: `maxConcurrent` → `max concurrent`. A plain
 * string transform on the key's own spelling, not a per-field name lookup —
 * a lookup table would be exactly the kind of per-module knowledge the
 * generated panel exists to avoid, one entry added by hand for every key
 * every module ever declares. A single-word key such as `ms` or `animate`
 * passes through with nothing to split.
 */
export function humanize(key: string): string {
  return key.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase()
}
