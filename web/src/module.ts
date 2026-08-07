/** A key a module claims, and the intent it produces. */
export interface KeyBinding {
  key: string
  intent: string
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
   * `settingsPanel.ts` has carried a note since M5b that this is "what the
   * design calls for once a module declares one with options; none does yet,
   * and the contract carries no field to declare options against, so that path
   * is not built ahead of a case that exists only in the design". The case now
   * exists: `annotations` has to let a reader place the note list beside the
   * page or below it, and a text box for that is a control that invites a typo
   * and answers with nothing.
   */
  options?: Record<string, string[]>
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

/** Build the registry, refusing at startup if two modules claim one key or share one id. */
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
  const byId = new Map(modules.map((m) => [m.id, m]))
  return {
    ids: modules.map((m) => m.id),
    get: (id) => byId.get(id),
    all: () => modules,
  }
}
