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
const key = (id: string) => `taiyaku.prefs.${id}`

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
    // reload, but the loss is no longer invisible to whoever investigates.
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
