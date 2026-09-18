/**
 * The reader's theme: follow the OS, or pin paper or ink (spec 2026-09-17 §3).
 *
 * Stored per device through `prefs.ts`, like every preference about the person
 * rather than the work, and applied as `data-theme` on `<html>`, which
 * `styles/tokens.css` turns into `color-scheme` — and every colour token is a
 * `light-dark()` pair that `color-scheme` picks a half of.
 *
 * **`index.html` reads the same key before first paint**, in an inline script
 * that cannot import this module. `theme.test.ts` runs that script against
 * `loadTheme` and `applyTheme` for every stored shape, so the two cannot drift
 * apart in silence.
 */
import { loadPrefs, savePrefs } from './prefs'

export type ThemeChoice = 'system' | 'paper' | 'ink'

/** In the order the control offers them. */
export const THEME_CHOICES: readonly ThemeChoice[] = ['system', 'paper', 'ink']

/** The `prefs.ts` id, and so the `localStorage` key `taiyaku.prefs.theme`. */
export const THEME_PREF_ID = 'theme'

function isThemeChoice(value: unknown): value is ThemeChoice {
  return THEME_CHOICES.includes(value as ThemeChoice)
}

export function loadTheme(): ThemeChoice {
  const { choice } = loadPrefs<{ choice: unknown }>(THEME_PREF_ID, {
    choice: 'system',
  })
  return isThemeChoice(choice) ? choice : 'system'
}

export function saveTheme(choice: ThemeChoice): void {
  savePrefs(THEME_PREF_ID, { choice })
}

/** `system` removes the attribute rather than writing one, so the OS decides. */
export function applyTheme(
  choice: ThemeChoice,
  root: Pick<Element, 'setAttribute' | 'removeAttribute'>,
): void {
  if (choice === 'system') {
    root.removeAttribute('data-theme')
  } else {
    root.setAttribute('data-theme', choice)
  }
}
