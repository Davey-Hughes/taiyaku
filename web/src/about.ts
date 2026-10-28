/**
 * `GET /api/about`: the build's commit and whether its tracked files
 * differed from it, the repository it was built from, the OPDS catalog's
 * path and the version of each format taiyaku writes.
 *
 * **Each call sends a request; nothing is held here.** The pages read it
 * through the read store as `ABOUT` (`pageReads.ts`), which shares one
 * request among the sidebar's label, About and `/account` when they ask
 * together, and keeps the answer for this tab until sign-out. A held answer
 * here would be handed back to the store's re-reads as if it were new.
 */
import { getJson } from './api'

/** What the route answers. Every field is always present. */
export type About = {
  /** The full 40-character hash, or `null` when the build carries none. */
  commit: string | null
  /** True only with a commit: tracked files differed from it. */
  dirty: boolean
  repository: string
  /** A path on this server; the catalog's address is `location.origin` before it. */
  catalog: string
  /**
   * In the order the route lists them. `version` is `null` for a format with
   * none; `description` is the sentence About draws under the name.
   */
  formats: { name: string; version: string | null; description: string }[]
}

export function about(): Promise<About> {
  return getJson<About>('/api/about')
}

/** The seven characters the label shows, and CI tags the image with as `sha-<7>`. */
export const shortCommit = (commit: string): string => commit.slice(0, 7)
