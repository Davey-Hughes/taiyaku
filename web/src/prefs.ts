/**
 * Module preferences, stored client-side.
 *
 * The design's rule: position and content are server-side because they must
 * survive a different machine; layout and preference are client-side, which is
 * the precedent parity breaks set. A module toggle, an animation duration and a
 * reading-direction override are all preferences about the person, not facts
 * about the work.
 *
 * A stored slice is merged over the defaults rather than replacing them, so a
 * module gaining a setting does not strand every reader who saved before it
 * existed. Storage that is unparseable, or that parses to something other than
 * a plain object, falls back to the defaults: a reader whose preferences are
 * corrupt should get a working reader, not a blank page — and not defaults
 * polluted by spreading a stored string's character indices or an array's own
 * keys.
 */
/**
 * The prefix every key this reader writes to `localStorage` begins with.
 *
 * **One namespace, one writer.** This module is the only thing in `web/src`
 * that touches `localStorage` at all, and that is worth keeping true: a value
 * stored anywhere else would be the first one whose key nobody thought about,
 * on an origin this reader may share with whatever else is served from it. One
 * *reader* sits outside `web/src`: `index.html`'s pre-paint script reads
 * `taiyaku.prefs.theme`, which `theme.ts` writes through this module and
 * `theme.test.ts` holds the script to.
 *
 * That is also why the reading mode is a `ReaderModule` rather than a direct
 * `loadPrefs('mode', …)` call — see `modules/layout.ts`. Anything the reader
 * remembers about a person is a module's slice, so it is namespaced by
 * construction and nobody has to remember to do it.
 *
 * `prefs` is a second segment rather than part of one prefix so that a later
 * kind of stored value — a cache, a draft — gets its own sibling instead of
 * being mixed in with preferences.
 */
export const NAMESPACE = 'taiyaku.prefs'

const key = (id: string) => `${NAMESPACE}.${id}`

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}

export function loadPrefs<P extends object>(id: string, defaults: P): P {
  try {
    const raw = localStorage.getItem(key(id))
    if (raw === null) return { ...defaults }
    const parsed: unknown = JSON.parse(raw)
    if (!isPlainObject(parsed)) return { ...defaults }
    return { ...defaults, ...(parsed as Partial<P>) }
  } catch {
    return { ...defaults }
  }
}

export function savePrefs<P extends object>(id: string, prefs: P): void {
  try {
    localStorage.setItem(key(id), JSON.stringify(prefs))
  } catch (err) {
    // A full or unavailable store must not break reading, so we swallow the
    // error rather than let it unmount the reader mid-page. Swallowing it
    // silently would leave zero trace for a reader who reports "my settings
    // keep resetting", so warn instead — the preference is still lost on
    // reload, but the loss is visible to whoever investigates.
    console.warn(`savePrefs: could not persist preference "${id}"`, err)
  }
}

export function resetPrefs(id: string): void {
  try {
    localStorage.removeItem(key(id))
  } catch (err) {
    // Same reasoning as `savePrefs`.
    console.warn(`resetPrefs: could not clear preference "${id}"`, err)
  }
}
