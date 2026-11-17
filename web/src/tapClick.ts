/**
 * How long a tap's `click` is waited for. A page with `width=device-width`
 * gets it straight after the touch ends, and the next press ends the wait
 * sooner, so this bounds only a wait whose click never comes.
 */
export const CLICK_WAIT_MS = 1000

/**
 * Takes the `click` that a touch or pen tap acted on at its `pointerup`
 * sends afterwards. The browser hit-tests that click only then, so once the
 * tap has drawn something under the finger (Back after a jump, the bars, a
 * dialog) the click lands on it and acts a second time. A mouse's click goes
 * where its press and release were, so it is left alone, as is one from the
 * keyboard (`detail` 0). The next press ends the wait, so the click of a
 * later press is never taken.
 */
export function takeTapClick(event: { pointerType: string }): void {
  if (event.pointerType === 'mouse') return
  const take = (click: MouseEvent) => {
    if (click.detail === 0) return
    click.stopPropagation()
    click.preventDefault()
    done()
  }
  const done = () => {
    window.removeEventListener('click', take, true)
    window.removeEventListener('pointerdown', done, true)
    window.clearTimeout(expiry)
  }
  window.addEventListener('click', take, true)
  window.addEventListener('pointerdown', done, true)
  const expiry = window.setTimeout(done, CLICK_WAIT_MS)
}
