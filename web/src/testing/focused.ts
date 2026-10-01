// Shared by `App.test.tsx` and `App.browser.test.tsx`'s ring tests, which
// walk focus with `Tab` in both `jsdom` and a real browser and need the same
// reading of where it landed either way.

/** What has focus, by the name a person would read. */
export function focused(): string {
  const el = document.activeElement as HTMLElement | null
  if (!el || el === document.body) return 'body'
  if (el.matches('[data-reader-page]')) return 'page'
  return (
    el.textContent || el.querySelector('img')?.getAttribute('alt') || el.tagName
  )
}
