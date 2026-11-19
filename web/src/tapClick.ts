/**
 * How long a tap's `click` is waited for. A page with `width=device-width`
 * gets it straight after the touch ends, and the next press ends the wait
 * sooner, so this bounds only a wait whose click never comes.
 */
export const CLICK_WAIT_MS = 1000

/**
 * Takes the `click` a touch or pen tap sends after acting at its `pointerup`:
 * it is hit-tested only then, so it would press whatever the tap drew under
 * the finger. A mouse's click, and the keyboard's (`detail` 0), are left alone,
 * and the next press ends the wait. `keepFocus` prevents the tap's `mousedown`
 * too, which would move focus from what the tap opened to what is under the
 * finger. `mouseToo` takes a mouse's click as well, for a press already given
 * its meaning wherever it landed.
 */
export function takeTapClick(
  event: { pointerType: string },
  {
    keepFocus = false,
    mouseToo = false,
  }: { keepFocus?: boolean; mouseToo?: boolean } = {},
): void {
  if (event.pointerType === 'mouse' && !mouseToo) return
  const take = (click: MouseEvent) => {
    if (click.detail === 0) return
    click.stopPropagation()
    click.preventDefault()
    done()
  }
  const hold = (down: MouseEvent) => down.preventDefault()
  const done = () => {
    window.removeEventListener('click', take, true)
    window.removeEventListener('mousedown', hold, true)
    window.removeEventListener('pointerdown', done, true)
    window.clearTimeout(expiry)
  }
  window.addEventListener('click', take, true)
  if (keepFocus) window.addEventListener('mousedown', hold, true)
  window.addEventListener('pointerdown', done, true)
  const expiry = window.setTimeout(done, CLICK_WAIT_MS)
}
