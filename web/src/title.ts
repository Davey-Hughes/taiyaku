/**
 * What a browser tab, its history and a screen reader's page announcement
 * call a screen (O173). Until this, every screen was `index.html`'s one
 * title.
 *
 * **Every routed screen calls `useDocumentTitle`, so none restores the title
 * it replaced when it unmounts**: the screen that mounts next sets its own.
 * A screen that stopped calling it would keep the last screen's name, which
 * each screen's own test catches by setting a stale title before it mounts.
 * The reader's panels — Account among them — set none: the reader's title
 * stays while one is open.
 */
import { useEffect } from 'react'

/** The app's name, which ends every title. `index.html` carries it alone. */
export const APP_NAME = 'taiyaku'

/** `name` and then the app's name, or the app's name alone before `name` is known. */
export function documentTitle(name: string | undefined): string {
  return name === undefined || name === '' ? APP_NAME : `${name} — ${APP_NAME}`
}

/** Sets `document.title` to `documentTitle(name)`, and again whenever `name` changes. */
export function useDocumentTitle(name: string | undefined): void {
  useEffect(() => {
    document.title = documentTitle(name)
  }, [name])
}
