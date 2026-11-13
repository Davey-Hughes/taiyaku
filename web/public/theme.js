/*
 * The stored theme, applied before first paint (spec 2026-09-17 §3): without
 * it, a reader who picked Ink on a light-mode OS sees paper while the modules
 * load. theme.test.ts runs it against theme.ts's loadTheme and applyTheme.
 *
 * A file and not inline, because the server's policy runs no inline script
 * (O277). A classic script in <head> still runs before the body is parsed.
 *
 * Wrapped in a function called at once, so `choice` is a local and not a
 * global on `window`.
 */
(function () {
  try {
    var choice = JSON.parse(localStorage.getItem('taiyaku.prefs.theme')).choice
    if (choice === 'paper' || choice === 'ink') {
      document.documentElement.setAttribute('data-theme', choice)
    }
  } catch (_) {}
})()
