// Shared by the browser tests that draw About and the licence page:
// `About.browser.test.tsx` and `Licence.browser.test.tsx`, which lay them
// out as `Shell` draws them, and `fieldNames.browser.test.tsx`, which reads
// their headings.
import { afterEach } from 'vitest'
import { type About, forgetAbout } from '../about'
import { ABOUT } from './about'

/**
 * `children` inside `.library`'s `<main>`, where `base.css` draws a button
 * and a link, as `Shell` puts them. The sidebar's column is there, empty,
 * because at a desktop width `.library` is a grid whose first column is the
 * sidebar's, and `<main>` alone would be drawn in it.
 */
export function inShell(children: React.ReactNode) {
  return (
    <div className="library">
      <div className="sidebar-column" />
      <main>{children}</main>
    </div>
  )
}

const realFetch = globalThis.fetch

/**
 * `fetch` answering `/api/about` with `answer`, `ABOUT` where it is left
 * out, and anything else with a 404, so About draws from the real
 * `about.ts`. Undone after each test by this module's own hook, which puts
 * `fetch` back and drops the answer `about.ts` holds, so the next test asks
 * again; nothing else a test stubbed is touched.
 */
export function answerAbout(answer: About = ABOUT): void {
  globalThis.fetch = ((input: RequestInfo | URL) =>
    Promise.resolve(
      String(input) === '/api/about'
        ? new Response(JSON.stringify(answer), {
            status: 200,
            headers: { 'content-type': 'application/json' },
          })
        : new Response(null, { status: 404 }),
    )) as typeof fetch
}

afterEach(() => {
  globalThis.fetch = realFetch
  forgetAbout()
})
