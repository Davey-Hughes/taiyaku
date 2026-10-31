/**
 * How a control is held: by `aria-disabled`, which keeps focus on it, by
 * `disabled`, which Chromium takes focus off, or not at all. A control
 * `HeldButton` holds, or one held as it is, reads `'held'`; a test that it is
 * held reads that rather than `disabled`, so that one held by `disabled`
 * fails it.
 */
export function holdOf(el: Element): 'held' | 'disabled' | 'free' {
  if ((el as HTMLButtonElement).disabled) return 'disabled'
  return el.getAttribute('aria-disabled') === 'true' ? 'held' : 'free'
}
