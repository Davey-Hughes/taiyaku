/**
 * `showModal()` for jsdom, whose `HTMLDialogElement` has the `open` attribute
 * and none of the methods.
 *
 * **It opens the dialog and does nothing else.** The page behind is not made
 * inert, Escape is not a close request and focus does not move, so a jsdom
 * test can hold what a dialog says and which handler each event reaches, and
 * nothing about how it behaves as a modal: that is the browser project's
 * (`Libraries.shell.browser.test.tsx`, Move… in its dialog). Installed only
 * where the method is missing, so a real browser keeps its own.
 */
if (
  typeof HTMLDialogElement !== 'undefined' &&
  !('showModal' in HTMLDialogElement.prototype)
) {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', {
    configurable: true,
    value(this: HTMLDialogElement) {
      this.open = true
    },
  })
}

export {}
