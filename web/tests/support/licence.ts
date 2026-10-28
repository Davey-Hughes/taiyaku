import { act } from 'react'

/**
 * Lets a mounted licence page's `import()` of its texts land, inside `act`.
 *
 * Called after `mount`, whose own `act` has run the page's effect, so the
 * page's import is under way before the one here, which waits on the same
 * module; the macrotask after it lets the page's `.then` draw. A macrotask
 * alone can end before Vite has served the module to the test run. A
 * refused import is the page's to say, so it is not thrown here.
 */
export async function licenceTextsLanded(): Promise<void> {
  await act(async () => {
    await import('../../src/licenceTexts').catch(() => undefined)
    await new Promise((done) => {
      setTimeout(done, 0)
    })
  })
}
