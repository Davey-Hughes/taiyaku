import { type SliderScale, withFieldValue } from './settingsPanel'

/** A key a module claims, and the intent it produces. */
export interface KeyBinding {
  key: string
  intent: string
  /**
   * What this key does, in words a reader can read.
   *
   * Optional, and the help screen falls back to `intent` without it, so no
   * existing declaration breaks. **Widening this contract is meant to be a
   * decision rather than a habit.** The reason here was the help screen:
   * `intent` strings were written as internal identifiers and two of them,
   * `annotations:reveal` and `annotations:review`, differ by one letter while
   * naming different things. Rendering those to a reader is not
   * documentation, it is a leak.
   *
   * The alternative was stripping the `namespace:` prefix in the overlay,
   * which fixes half of them and relies on a naming convention nothing
   * enforces — `zoom:whole` would still read as jargon and `annotations:review`
   * would still be a near-homograph of its neighbour.
   */
  label?: string
}

/**
 * A reader behaviour that can be switched off and tuned.
 *
 * Declarative rather than self-registering, and that is the whole design. A
 * module that registers itself on load cannot be enumerated while it is
 * disabled, so the settings panel would need hand-written knowledge of every
 * module — which is precisely the coupling "modular" exists to remove.
 * Declaring also turns a keybinding collision into a startup error instead of
 * two modules silently fighting over one key.
 *
 * A module uses the extension points it needs and no others.
 */
export interface ReaderModule<P> {
  id: string
  label: string
  /** The preference slice and its defaults. The settings panel is generated from this. */
  defaults: P
  /** Keys this module claims while enabled. Checked for collisions at startup. */
  keys?: KeyBinding[]
  /**
   * The values a `string` preference is allowed to take, keyed by preference
   * name — which turns its control from a free-text box into a choice.
   *
   * `annotations` has to let a reader place the note list beside the page or
   * below it, and a text box for that is a control that invites a typo and
   * answers with nothing.
   */
  options?: Record<string, string[]>
  /**
   * The words a choice's values are shown by, keyed by preference name and
   * then by value; a value with none is shown by its own spelling
   * (`settingsPanel.ts`'s `humanize`). Only what is shown: the stored value
   * is still the option itself, so a saved setting loads whatever the words.
   *
   * A value can be an id rather than a word: `layout`'s `two` is two pages
   * side by side, and `scroll` names a different layout in each reader. The
   * panel is generated from these declarations and holds no knowledge of
   * any one module (`ReaderModule`), so the module declares its own words.
   */
  optionWords?: Record<string, Record<string, string>>
  /**
   * The scale a `number` preference is drawn on, keyed by preference name —
   * which turns its control from a number box into a slider, for a value
   * judged by eye as it changes. Its default must sit at the scale's middle
   * (`sliderFaults`).
   */
  sliders?: Record<string, SliderScale>
}

export interface Collision {
  key: string
  modules: string[]
}

/**
 * Keys claimed by more than one module.
 *
 * A module claiming the same key twice is not a collision — that is one
 * module's own arrangement and nothing else can be surprised by it.
 */
export function collisions(modules: ReaderModule<unknown>[]): Collision[] {
  const claims = new Map<string, string[]>()
  for (const m of modules) {
    for (const key of new Set((m.keys ?? []).map((k) => k.key))) {
      claims.set(key, [...(claims.get(key) ?? []), m.id])
    }
  }
  return [...claims.entries()]
    .filter(([, ids]) => ids.length > 1)
    .map(([key, modules]) => ({ key, modules }))
}

/**
 * Module ids that appear more than once.
 *
 * Kept separate from `collisions` because it answers a different question:
 * `collisions` is about two modules fighting over a key at read time, this is
 * about two modules fighting over an entry in the registry itself. Without
 * this check, a copy-paste registration bug would let the later entry
 * silently shadow the earlier one in `get()`, while `ids` and `all()` would
 * both still report both — an inconsistent registry with no startup error.
 */
export function duplicateIds(modules: ReaderModule<unknown>[]): string[] {
  const seen = new Set<string>()
  const dupes = new Set<string>()
  for (const m of modules) {
    if (seen.has(m.id)) dupes.add(m.id)
    seen.add(m.id)
  }
  return [...dupes]
}

/**
 * `<id>.<key>` for each declared slider whose default is not a finite number
 * at its scale's middle position, a key `defaults` lacks among them. The
 * panel marks the default at the track's middle, the one place it can be
 * drawn without the thumb's width, which differs between browsers.
 */
export function sliderFaults(modules: ReaderModule<unknown>[]): string[] {
  const faults: string[] = []
  for (const m of modules) {
    const defaults = m.defaults as Record<string, unknown>
    for (const [key, scale] of Object.entries(m.sliders ?? {})) {
      const value = defaults[key]
      const middle =
        typeof value === 'number' &&
        Number.isFinite(value) &&
        scale.positionOf(value) * 2 === scale.last
      if (!middle) faults.push(`${m.id}.${key}`)
    }
  }
  return faults
}

export interface Registry {
  ids: string[]
  get(id: string): ReaderModule<unknown> | undefined
  all(): ReaderModule<unknown>[]
}

/**
 * One module's preference slice out of a map keyed by module id — every
 * caller that needs a single module's prefs back with its own `P` restored,
 * rather than the widened `object` a map holding every module's slice at
 * once is typed as. Falls back to `mod.defaults` for a module with no entry
 * yet, the same fallback `Settings.tsx`'s own `modulePrefs` lookup already
 * applies at the panel boundary — this is that rule at every other read site.
 *
 * This, together with the map itself, is what lets `App.tsx` hold every
 * module's preferences in one generic slice keyed by `ReaderModule.id`
 * instead of one hand-declared `useState` per module: a new module reading
 * and writing its own slice needs this function and its own `id`, nothing
 * that names the module by hand.
 */
export function sliceFor<P extends object>(
  map: Record<string, object>,
  mod: ReaderModule<P>,
): P {
  return (map[mod.id] as P | undefined) ?? mod.defaults
}

/**
 * Build the registry, refusing at startup if two modules claim one key or
 * share one id, or a slider's default is off its middle.
 */
export function buildRegistry(modules: ReaderModule<unknown>[]): Registry {
  const dupes = duplicateIds(modules)
  if (dupes.length > 0) {
    throw new Error(
      `two modules share one id: ${dupes.map((id) => `\`${id}\``).join(', ')}`,
    )
  }
  const clashes = collisions(modules)
  if (clashes.length > 0) {
    const detail = clashes.map(
      (c) => `\`${c.key}\` claimed by ${c.modules.join(' and ')}`,
    )
    throw new Error(`two modules claim one key: ${detail.join('; ')}`)
  }
  const offMiddle = sliderFaults(modules)
  if (offMiddle.length > 0) {
    throw new Error(
      `a slider's default is not at its middle: ${offMiddle.map((name) => `\`${name}\``).join(', ')}`,
    )
  }
  const byId = new Map(modules.map((m) => [m.id, m]))
  return {
    ids: modules.map((m) => m.id),
    get: (id) => byId.get(id),
    all: () => modules,
  }
}

/**
 * One module's slice replaced.
 *
 * Spread rather than assigned, and that is the whole of it: `map[mod.id] =
 * next` on the state React handed the updater would mutate it, and an updater
 * must stay pure because React may invoke it more than once.
 */
export function withSlice<P extends object>(
  map: Record<string, object>,
  mod: ReaderModule<P>,
  next: P,
): Record<string, object> {
  return { ...map, [mod.id]: next }
}

/**
 * One module's slice changed, given its current value.
 *
 * The shape every preference-writing key press has: read this module's slice
 * out of the map, apply one rule to it, put it back. In `direction.ts`, a
 * hand-written copy that forgot to spread would wipe every other edition's
 * saved override, which `setOverride` exists to prevent.
 */
export function changeSlice<P extends object>(
  map: Record<string, object>,
  mod: ReaderModule<P>,
  change: (prefs: P) => P,
): Record<string, object> {
  return withSlice(map, mod, change(sliceFor(map, mod)))
}

/** Every registered module's slice, loaded. `load` is `prefs.ts`'s `loadPrefs`. */
export function initialPrefs(
  modules: ReaderModule<unknown>[],
  load: (id: string, defaults: object) => object,
): Record<string, object> {
  return Object.fromEntries(
    modules.map((mod) => [mod.id, load(mod.id, mod.defaults as object)]),
  )
}

/**
 * Every registered module's slice, saved. The whole map, always — never a
 * diff for the one id that moved.
 *
 * `savePrefs` is a cheap `JSON.stringify` plus `setItem` over a handful of
 * small objects, so "persist everything" costs nothing measurable and cannot
 * drift from which id actually changed the way a hand-maintained diff could.
 * A module with no entry yet saves its own defaults rather than `undefined`.
 */
export function persistPrefs(
  modules: ReaderModule<unknown>[],
  map: Record<string, object>,
  save: (id: string, prefs: object) => void,
): void {
  for (const mod of modules) {
    save(mod.id, map[mod.id] ?? (mod.defaults as object))
  }
}

/**
 * The settings panel's change, generic over every registered module.
 *
 * Unlike the key dispatch — a small explicit switch, because each key does
 * something behaviourally different — a panel change is the same shape for
 * every module always: look the id up, apply the change to that id's slice.
 * `withFieldValue` is what actually applies it, coercing the raw control value
 * and refusing an unknown key, an unparseable number or an object-typed field,
 * so nothing here needs per-module knowledge of any of that. A module
 * registered with nothing but a `defaults` shape is fully wired the moment it
 * is registered.
 *
 * The map back unchanged for an id the registry does not know, which is what
 * keeps a stale panel from writing a slice no module will ever read.
 */
export function fieldChanged(
  map: Record<string, object>,
  registry: Registry,
  id: string,
  key: string,
  raw: unknown,
): Record<string, object> {
  const mod = registry.get(id)
  if (!mod) return map
  return {
    ...map,
    [id]: withFieldValue(map[id] ?? (mod.defaults as object), key, raw),
  }
}

/**
 * The settings panel's reset, in memory — the caller still calls `resetPrefs`
 * so a reload does not resurrect the old value.
 *
 * Both halves are needed and neither is enough. `resetPrefs` alone satisfies
 * "reload, confirm it persisted" and silently fails "change a setting, confirm
 * it takes effect" — a reset the reader could watch do nothing until they
 * reloaded.
 *
 * A fresh copy of `defaults`, not the object itself: handing the module's own
 * literal to state makes every later `withFieldValue` on it spread from a
 * shared object, and one that a test or a second reader could then be looking
 * at through a different map.
 */
export function sliceReset(
  map: Record<string, object>,
  registry: Registry,
  id: string,
): Record<string, object> {
  const mod = registry.get(id)
  if (!mod) return map
  return { ...map, [id]: { ...(mod.defaults as object) } }
}
