import { act } from 'react'
import { flush } from './async'

/**
 * Type `value` into a controlled box.
 *
 * Through the prototype's setter, because React tracks a controlled input's
 * value on the element and ignores an assignment that goes through its own
 * override.
 */
export async function typeInto(box: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
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

/** Submit a form, and let what the submit started settle. */
export async function submit(form: Element) {
  await flush(() => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  })
}
