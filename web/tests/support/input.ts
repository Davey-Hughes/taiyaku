import { act } from 'react'
import { flush } from './async'

/**
 * Type `value` into a controlled box.
 *
 * Through the prototype's setter, because React tracks a controlled input's
 * value on the element and ignores an assignment that goes through its own
 * override.
 */
export async function typeInto(
  box: HTMLInputElement | HTMLTextAreaElement,
  value: string,
) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      Object.getPrototypeOf(box),
      'value',
    )?.set?.call(box, value)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

/** Click, and let what the click started settle. */
export async function press(el: Element) {
  await flush(() => {
    ;(el as HTMLElement).click()
  })
}

/**
 * Enter pressed in `box`, as a browser takes it, and what it started let
 * settle: the key goes to the box, and unless a handler prevented its
 * default, the form the box is in is submitted implicitly, by a click on its
 * default button (the first submit button it owns) where that is not held.
 * jsdom does only the first part, so without the second an Enter that would
 * send a form in a browser sends nothing here, and a test that it sends
 * nothing passes whatever the page does with the key.
 */
export async function pressEnter(box: HTMLInputElement) {
  await flush(() => {
    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    })
    box.dispatchEvent(enter)
    if (enter.defaultPrevented || box.form === null) return
    const byDefault = [...box.form.elements].find(
      (el) =>
        (el instanceof HTMLButtonElement || el instanceof HTMLInputElement) &&
        el.type === 'submit',
    ) as HTMLButtonElement | HTMLInputElement | undefined
    // A form with no submit button is submitted by Enter only where at most
    // one of its fields blocks that, which no caller's form needs yet.
    if (byDefault === undefined) {
      throw new Error('pressEnter models a form with a submit button only')
    }
    if (!byDefault.disabled) byDefault.click()
  })
}

/** Submit a form, and let what the submit started settle. */
export async function submit(form: Element) {
  await flush(() => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  })
}
