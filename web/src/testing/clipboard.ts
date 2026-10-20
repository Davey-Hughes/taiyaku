import { afterEach } from 'vitest'

/**
 * Where `CatalogAddress` may draw its Copy address button: `isSecureContext`
 * set to `secure`, and `clipboard` as `navigator.clipboard`, left out for
 * none.
 *
 * jsdom has neither half, so a jsdom test that wants the button states both,
 * and each is stubbed on its own so a test can take either half away. A
 * browser test has both, and states them to put a clipboard of its own in
 * place of the browser's. One helper for every test that stubs either, so
 * no two stub it two ways.
 *
 * Undone after each test by this module's own hook, as `mount.ts` tears its
 * root down: each property goes back to what it was, and nothing else a test
 * stubbed is touched.
 */
export function clipboardContext(
  secure: boolean,
  clipboard?: Partial<Clipboard>,
): void {
  hold(globalThis, 'isSecureContext', secure)
  hold(navigator, 'clipboard', clipboard)
}

const held: { on: object; name: string; was?: PropertyDescriptor }[] = []

function hold(on: object, name: string, value: unknown) {
  if (!held.some((entry) => entry.on === on && entry.name === name)) {
    held.push({ on, name, was: Object.getOwnPropertyDescriptor(on, name) })
  }
  Object.defineProperty(on, name, { value, configurable: true })
}

afterEach(() => {
  for (const { on, name, was } of held.splice(0)) {
    if (was === undefined) Reflect.deleteProperty(on, name)
    else Object.defineProperty(on, name, was)
  }
})
