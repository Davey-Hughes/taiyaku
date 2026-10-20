/**
 * `GET /api/about`: the build's commit, the repository it was built from,
 * the OPDS catalog's path and the version of each format taiyaku writes.
 *
 * **Asked once per page load, and the answer shared.** The first answer
 * comes from the server that served this page's code, so it names that
 * code's build; asked again after the server is redeployed under an open
 * page, it would name a build the page is not running. The sidebar's
 * label and the About page both ask on mount, and on `/about` they mount
 * together, so the promise itself is held rather than its value, and the
 * second caller waits on the first one's request instead of sending its own.
 *
 * **A failure is not held**, so the next caller asks again. About asks on
 * every visit. The label asks once per mount of the shell, and
 * `Shell` is kept rather than remounted across the routes that draw it
 * (react-router keys none of them), so after a failure the label asks again
 * only after a reload or a trip through a route without the shell,
 * `/read/:work` or `/search`. A held rejection would keep both failing until
 * a reload, however soon the server came back.
 */
import { getJson } from './api'

/** What the route answers. Every field is always present. */
export type About = {
  /** The full 40-character hash, or `null` when the build carries none. */
  commit: string | null
  repository: string
  /** A path on this server; the catalog's address is `location.origin` before it. */
  catalog: string
  /**
   * In the order the route lists them. `version` is `null` for a format with
   * none; `description` is the sentence About draws under the name.
   */
  formats: { name: string; version: string | null; description: string }[]
}

let asked: Promise<About> | undefined

export function about(): Promise<About> {
  if (asked === undefined) {
    asked = getJson<About>('/api/about')
    // Only its own failure empties the slot, or `forgetAbout`, which tests
    // call only between settled requests; so the request that fails is the
    // one held.
    asked.catch(() => {
      asked = undefined
    })
  }
  return asked
}

/** Drops the held answer, so a test can start from a page that has asked nothing. */
export function forgetAbout(): void {
  asked = undefined
}

/** The seven characters the label shows, and CI tags the image with as `sha-<7>`. */
export const shortCommit = (commit: string): string => commit.slice(0, 7)
