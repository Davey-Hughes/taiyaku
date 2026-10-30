import { READER_KEYS, type ReaderBinding } from '../keys'
import { stepFor, type Direction } from './direction'
import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'help:toggle'

/**
 * The keybind list.
 *
 * Two keys, one intent: `?` is what a reader tries and `h` is what a reader
 * who has used a pager tries, and making them two intents would mean two
 * cases in `App.tsx` doing the same thing.
 *
 * On by default, unlike most modules' more cautious defaults, because a help
 * screen that has to be found in settings before it can be found by keypress
 * has the problem backwards.
 */
export interface HelpPrefs {
  enabled: boolean
}

export const helpModule: ReaderModule<HelpPrefs> = {
  id: 'help',
  label: 'Keyboard help',
  defaults: { enabled: true },
  keys: [
    { key: '?', intent: TOGGLE_INTENT, label: 'show this list of keys' },
    { key: 'h', intent: TOGGLE_INTENT, label: 'show this list of keys' },
  ],
}

/**
 * The four arrows, as arrows.
 *
 * All four rather than the two `READER_KEYS` declares, because they are one
 * family and a table holding `ArrowLeft` and `ArrowRight` while leaving
 * `ArrowUp` and `ArrowDown` to render as identifiers would read as an
 * oversight rather than a decision. Nothing declares the vertical pair today —
 * that half is written ahead of a caller, which is the one thing in this file
 * that is.
 */
const ARROWS: Record<string, string> = {
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
}

/**
 * A `KeyboardEvent.key` value as something to put in front of a reader.
 *
 * **Here rather than as a `display` field on `ReaderBinding`**, which was the
 * other candidate. Three things decided it. `ReaderBinding.keys` is a
 * `string[]` and the digit row holds nine of them, so one `display` string on
 * the row cannot name each key it answers. `module.ts`'s `KeyBinding` would
 * need the identical field to cover the module tables, so the field is two
 * contract widenings rather than one. And `settingsPanel.ts`'s `humanize` is
 * the precedent for the shape: a plain transform on the name's own spelling,
 * not a per-declaration lookup that grows by hand for every key any module
 * ever claims.
 *
 * What that costs is that the mapping sits away from the declarations, so a
 * module claiming a key with a DOM name nobody has thought about gets that
 * raw name until someone edits this function. The fallback is why that is
 * survivable rather than merely accepted: every printable single character is
 * already its own label, and the multi-character names in this vocabulary
 * (`Tab`, `Escape`, `Enter`, `Home`) are words a reader can read. Of the keys
 * the two tables declare, only `' '`, `ArrowLeft` and `ArrowRight` do not
 * render as themselves. `Tab` is the only other name of more than one
 * character, and it needs nothing.
 */
export function keyLabel(key: string): string {
  if (key === ' ') return 'Space'
  return ARROWS[key] ?? key
}

/**
 * Whether these keys are a consecutive run of single characters, and so can
 * be shown as a range.
 *
 * Three, not two: `a–b` is no shorter than `a / b` and reads as a range where
 * there is none. Nine is the case this exists for — `READER_KEYS`'s digit row
 * is one rule with nine keys, and `keys.ts` says where it is declared that
 * nine help lines saying the same thing would be worse than one saying it
 * once. Rendering the row as nine cells would have been that same defect
 * moved one file over.
 */
function isRun(keys: string[]): boolean {
  if (keys.length < 3) return false
  return keys.every(
    (key, i) =>
      key.length === 1 &&
      (i === 0 || key.charCodeAt(0) === keys[i - 1].charCodeAt(0) + 1),
  )
}

/**
 * Every key one row answers, as one cell.
 *
 * The dash is an en dash and not a hyphen because `-` is itself a key in the
 * list this renders into — `modules/zoom.ts` claims it for zoom-out — so
 * `1-9` would sit three rows from a row whose entire key is `-`.
 */
export function keysLabel(keys: string[]): string {
  if (isRun(keys)) {
    return `${keyLabel(keys[0])}–${keyLabel(keys[keys.length - 1])}`
  }
  return keys.map(keyLabel).join(' / ')
}

/** One line of the overlay: what to press, and what it does. */
export interface HelpRow {
  keys: string
  description: string
}

/** A heading and the rows under it. */
export interface HelpGroup {
  heading: string
  rows: HelpRow[]
}

/**
 * The overlay's contents, derived from the two tables the reader dispatches
 * on and from nothing else.
 *
 * **Group order is answer order.** `intentOf` walks `READER_KEYS` first and
 * then the modules in the order it is handed them, so listing in the same
 * order means the overlay reads down in the order a keypress is resolved.
 * That is not a presentation choice made here — it is the one `keys.ts`
 * already made and recorded, and this follows it rather than sorting
 * alphabetically and inventing a second order.
 *
 * **A module's rows are keyed by intent, not by key.** Two bindings with one
 * intent are one thing a reader can do, and one row is what says so;
 * `helpModule` above is the case — `?` and `h` are a single line reading
 * `? / h`, not two lines saying `help:toggle` twice.
 *
 * **A row says the binding's `label`, and falls back to its `intent`.**
 * `module.ts` has that optional field for this screen and says why: the
 * intent strings were written to be dispatched on, and `annotations:reveal`
 * and `annotations:review` are one letter apart while naming different
 * things. Every binding the registry declares carries one, so the fallback is
 * reached by no module at all; it stays because a new binding that forgets a
 * label should be a jargon row and not a build error, which is the property
 * `module.ts` chose when it made the field optional. Where two bindings share
 * an intent the first label wins, since one row can only say one thing;
 * declaring two different labels for one intent would be declaring two
 * descriptions of a single action, and the one that loses is the later one.
 *
 * **`reader` is a parameter with a default** rather than a straight import of
 * `READER_KEYS`, so the tests can feed a table of their own and check the
 * derivation instead of only the current data. Every caller uses the default.
 *
 * A module declaring no keys is skipped rather than given an empty heading —
 * `motion`, `prefetch`, `readouts` and `fit`, among others, are tuned in
 * settings and pressed never, and an empty section would be an invitation to
 * look for a key that does not exist.
 */
export function helpGroups(
  modules: ReaderModule<unknown>[],
  reading: Direction = 'ltr',
  reader: ReaderBinding[] = READER_KEYS,
): HelpGroup[] {
  // The two arrow rows trade descriptions in a right-to-left work, because the
  // physical key that advances is the other one. Derived from `stepFor` rather
  // than restated: that function is the rule the arrows and the filmstrip both
  // follow, and a help screen with its own opinion about which way is forward
  // would be the copy of a rule this whole module exists to avoid.
  //
  // Only the pairing flips. `READER_KEYS` still owns both strings, so nothing
  // here has to know what "next page" says.
  // Paired by what a row *does*, not by where it sits. Filtering the rows whose
  // key starts with `Arrow` and reversing that list is a swap for exactly two
  // and nonsense for three — and `ARROWS` above already names
  // `ArrowUp`/`ArrowDown` ahead of a caller, so the breaking case is one this
  // file anticipates. Asking `resolve` what a row turns, and `stepFor` what the
  // physical key does under this reading, pairs them by meaning and does not
  // care how many there are.
  const stepOf = (b: ReaderBinding): 1 | -1 | undefined => {
    const intent = b.resolve(b.keys[0] ?? '', [])
    return intent?.kind === 'step' ? intent.by : undefined
  }
  const describe = (binding: ReaderBinding): string => {
    const declared = stepOf(binding)
    if (declared === undefined) return binding.label
    const effective = stepFor(declared, reading)
    return reader.find((o) => stepOf(o) === effective)?.label ?? binding.label
  }
  const groups: HelpGroup[] = [
    {
      heading: 'Reading',
      rows: reader.map((binding) => ({
        keys: keysLabel(binding.keys),
        description: describe(binding),
      })),
    },
  ]
  for (const mod of modules) {
    const byIntent = new Map<string, { keys: string[]; label?: string }>()
    for (const binding of mod.keys ?? []) {
      const row = byIntent.get(binding.intent) ?? { keys: [] }
      row.keys.push(binding.key)
      row.label ??= binding.label
      byIntent.set(binding.intent, row)
    }
    if (byIntent.size === 0) continue
    groups.push({
      heading: mod.label,
      rows: [...byIntent].map(([intent, row]) => ({
        keys: keysLabel(row.keys),
        description: row.label ?? intent,
      })),
    })
  }
  return groups
}

/**
 * The modules whose keys are live, out of the ones the build ships.
 *
 * `helpGroups` lists exactly the modules it is handed, and `App.tsx` holds
 * every module's preferences in one map keyed by id, so this is the join
 * between them: it is what stops the overlay telling a reader that `=` zooms
 * when they have switched zoom off. Handing it `registry.all()` unfiltered
 * was the other option and it is the one a caller reaches for by default,
 * which is why this exists and why `Help`'s own doc says the filtering is the
 * caller's job.
 *
 * **`enabled` is a convention, not a field of the contract, and a module
 * without one is treated as always on.** That is the honest reading rather
 * than a safe-looking one: most registered modules have no such preference, so
 * the alternative — treating an absent flag as off — would empty the overlay
 * of `d`, `b`, `s`, `C` and all five annotation keys, every one of which
 * works. The other alternative was widening `ReaderModule` with a required
 * `enabled`, which `module.ts` asks to be a decision rather than a habit and
 * which would mean `settings` gaining an off switch that hides `s` — a reader
 * can then no longer reach the panel that would turn it back on.
 *
 * What the convention costs, stated because it is a real gap and not a
 * theoretical one: `credits` gates `C` on a preference called `panel`, not
 * `enabled` (see `App.tsx`'s `PANEL_INTENT` case, which returns early when it
 * is off). Turning that off therefore leaves `C` listed here while the key
 * does nothing. It is the only module in today's registry with an off switch
 * under another name, and the fix if it grows is a contract field rather than
 * a list of per-module rules here — a list like that is the copy of a rule
 * this whole file exists to avoid.
 */
export function enabledModules(
  modules: ReaderModule<unknown>[],
  prefs: Record<string, object>,
): ReaderModule<unknown>[] {
  return modules.filter((mod) => {
    // The same fallback `sliceFor` applies, for the same reason: a module with
    // no entry in the map yet is running on its own defaults.
    const slice = (prefs[mod.id] ?? mod.defaults) as { enabled?: unknown }
    return slice.enabled !== false
  })
}
