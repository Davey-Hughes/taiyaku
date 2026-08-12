/**
 * The labeller's keyboard: one table, **dispatched from and listed from**.
 *
 * A file of its own rather than a block at the top of `Labeller.tsx`, for the
 * reason `keys.ts` sits apart from the reader's components: none of this is
 * DOM, so it runs in the fast `node` project, and a module that exports a
 * component may export nothing else without costing Fast Refresh.
 *
 * `keys.ts` states the rule this follows, and it is worth restating because it
 * is the whole point of the shape: *a second list of what the keys do is a
 * copy of a rule, and this project has repeatedly found copies of rules
 * drifting from the rules.* The legend at the foot of the labeller is
 * generated from this table — so a binding that changes what it does cannot
 * keep an old description, because there is only one description.
 */
import {
  type BoxAxis,
  type Kind,
  type LabelBox,
  removeBox,
  setAxis,
  setKind,
  toggleIgnore,
} from './labelling'

/** Index order is the `1`-`6` key order, and it matches `taiyaku-core`'s `Kind`. */
export const KINDS: readonly Kind[] = [
  'dialogue',
  'narration',
  'sfx',
  'logo',
  'credits',
  'other',
]

/**
 * The axis keys, and how to say each to a person.
 *
 * `null` is a real axis value, set by `n` — a radial sound effect has no
 * reading direction. "Not an axis key at all" is this lookup finding no row at
 * all. The two must stay apart: collapsing them would make `v`/`h`/`n`
 * indistinguishable from every other keystroke.
 */
export const AXES: readonly { key: string; axis: BoxAxis; says: string }[] = [
  { key: 'v', axis: 'v', says: 'vertical' },
  { key: 'h', axis: 'h', says: 'horizontal' },
  { key: 'n', axis: null, says: 'none' },
]

/** One row of the labeller's keyboard: what it answers, what it does, and how to say so. */
export interface LabelBinding {
  /** Every key the row answers, so `1`-`6` is one row rather than six. */
  keys: string[]
  /** How to say it to a person. The on-screen legend is generated from this. */
  label: string
  /**
   * What the key does to the selected box. Absent for a row that moves the
   * *selection* rather than editing a box — `Tab` — which the handler answers
   * ahead of this table and with no box selected.
   */
  apply?: (boxes: LabelBox[], id: string, key: string) => LabelBox[]
  /**
   * Whether the edit leaves nothing selected.
   *
   * State hygiene rather than a visible behaviour: with the box gone, every
   * readout already falls back to empty and no overlay can carry
   * `data-selected`, so nothing on screen distinguishes a cleared selection
   * from one still pointing at a deleted id. It is set because holding a
   * selection to a box that no longer exists is wrong, and the next thing to
   * trust `selected` would inherit that. No test pins it, and this note is
   * here so that absence is a stated choice rather than an oversight.
   */
  clearsSelection?: boolean
}

/**
 * The labeller's keys, in the order the handler checks them.
 *
 * The kind row is derived from `KINDS` rather than spelled out beside it, and
 * the axis row from `AXES`, for the same reason one level down: the digits and
 * the names they set are one fact, and a seventh kind must not be able to
 * appear in the handler while the legend still lists six.
 */
export const LABEL_KEYS: readonly LabelBinding[] = [
  { keys: ['Tab'], label: 'cycle the boxes under the pointer' },
  {
    keys: KINDS.map((_, i) => String(i + 1)),
    label: KINDS.join(' · '),
    apply: (boxes, id, key) => setKind(boxes, id, KINDS[Number(key) - 1]),
  },
  {
    keys: AXES.map((a) => a.key),
    label: `reading axis — ${AXES.map((a) => `${a.key} ${a.says}`).join(', ')}`,
    apply: (boxes, id, key) =>
      // `find` cannot miss: this row answers exactly the keys it was built from.
      setAxis(boxes, id, AXES.find((a) => a.key === key)?.axis ?? null),
  },
  {
    keys: ['i'],
    label: 'ignore — decline a box rather than guess at it',
    apply: (boxes, id) => toggleIgnore(boxes, id),
  },
  {
    keys: ['Delete'],
    label: 'remove the box',
    apply: (boxes, id) => removeBox(boxes, id),
    clearsSelection: true,
  },
]

/**
 * How a row's keys are written on screen: `1-6` rather than `1 2 3 4 5 6`.
 *
 * Only a **contiguous** run collapses. Length alone is not the test, and
 * treating it as one printed the axis row as `v-n` — which reads as a range
 * from `v` to `n`, a span that is backwards, four keys wide and includes
 * neither `h` nor anything else anybody could press. `v h n` is three
 * unrelated keys and is written as three.
 */
export function keysOf(binding: LabelBinding): string {
  const contiguous = binding.keys.every(
    (key, i) =>
      key.length === 1 &&
      (i === 0 || key.charCodeAt(0) === binding.keys[i - 1].charCodeAt(0) + 1),
  )
  return contiguous && binding.keys.length > 2
    ? `${binding.keys[0]}-${binding.keys[binding.keys.length - 1]}`
    : binding.keys.join(' ')
}
