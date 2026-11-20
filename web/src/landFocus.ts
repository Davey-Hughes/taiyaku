/**
 * Puts focus on `el`, a place focus lands rather than a control, such as
 * the page's `main` or a volume's panel: focusable only while it holds that
 * focus, since a standing `tabIndex` would take focus on every click on its
 * plain text, and the next `Tab` would start from its top.
 */
export function landFocus(el: HTMLElement): void {
  el.tabIndex = -1
  el.addEventListener('blur', () => el.removeAttribute('tabindex'), {
    once: true,
  })
  el.focus({ preventScroll: true })
}
