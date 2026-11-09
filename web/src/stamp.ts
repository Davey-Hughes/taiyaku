/**
 * The `/api/library/works/{slug}/stamp-comicinfo` client — spec C3 §6.
 *
 * **Its own module rather than `metadata.ts`'s**, mirroring the split
 * `taiyaku-serve/src/stamp.rs` made one layer down and for the reason C3 §6
 * gives: this is not a save path — *"putting it on the same Save button would
 * make the irreversible thing a side effect of the reversible one"* — and
 * sharing a module with the four PATCH routes is that mistake repeated in the
 * browser.
 *
 * **The plan is echoed, never rebuilt.** `stamp::apply` compares the POST's
 * body to a freshly computed plan field for field and refuses a mismatch with
 * a 409, so `Archive` is the plan's identity rather than a summary of it and
 * nothing here reduces one to a count. A screen that sent `{ member_ids: [7] }`
 * would be refused every time.
 */
import { getJson, sendJson } from './api'

/** One archive a stamp would rewrite, as both directions carry it. */
export interface Archive {
  member_id: number
  edition_key: string
  path: string
  bytes: number
}

/**
 * What a stamp would do. **An empty `archives` is a 200**, not a 404 —
 * "nothing to do" is an answer, which is either a work with no zip member or
 * one whose every zip already holds the document taiyaku would write.
 */
export interface StampPlan {
  archives: Archive[]
  total_bytes: number
  /**
   * The formats this work holds that a stamp will not write — `rar`, `7z`,
   * `unreadable` for a member nothing could open, or `copied` for one held in
   * more than one directory. **Distinct from the plan being short:** "4
   * archives" reads as "all of them" unless the screen can also say the rar
   * was left alone.
   */
  skipped_formats: string[]
}

/** What actually moved. */
export interface Stamped {
  rewritten: number
  /**
   * The work's new identity, or **`null` when it did not move**.
   *
   * `stamp::apply` sets this from `moved_key`, which is `None` whenever the
   * freshly derived key equals the old one, and `taiyaku-serve`'s handler
   * serialises the `Option` straight through — so `null` is on the wire.
   *
   * **Not a rare answer.** `identity::content_key` hashes the *hub* edition's
   * member digests alone, so stamping a spoke edition's zips while the hub is
   * a rar, a directory or an untouched edition rewrites files and leaves the
   * work's identity exactly where it was.
   *
   * Typed `string | null` rather than `string` because nothing else would
   * catch the lie: `getJson`/`sendJson` cast, so the screen would render "this
   * work's content_key is now ." for every one of those works.
   */
  content_key: string | null
  /**
   * What `hand::rename_from` answered for this stamp — **not** "there was no
   * document".
   *
   * It is hard-coded `false` when `content_key` is `null`, without anything
   * asking whether a document exists; and when the key *did* move it is,
   * for a stamp, `false` for five separate causes, two of which mean the
   * document is now stranded at the old key. That is open item O81. A screen
   * that reads this as "no document on disk" states the benign cause as
   * certain.
   */
  document_moved: boolean
  /**
   * Whether the work's `taiyaku.json` was re-exported alongside the archives.
   *
   * `false` for a *scanned* work, which has no sidecar path — see `apply`'s
   * own doc. Rendered rather than dropped: this is a second file on disk that
   * this press rewrote, and a screen that lists what it changed and silently
   * omits one of the files is the same defect as the two above in a smaller
   * size.
   */
  sidecar_rewritten: boolean
}

const route = (slug: string) =>
  `/api/library/works/${encodeURIComponent(slug)}/stamp-comicinfo`

/**
 * What a stamp of `slug` would rewrite. Writes nothing.
 *
 * **No empty-204 guard, unlike `performStamp` below**, and the asymmetry is
 * the answer rather than an omission. Every guard of that shape in this tree
 * sits on `sendJson`, which answers `T | undefined` because a 204 is a real
 * answer on a write route and a caller handed `undefined` would render it.
 * `getJson` answers `T`, no GET client here carries one, and this route has a
 * single success shape: "nothing to do" is a 200 whose `archives` is empty,
 * which is what `StampPlan` above documents and what `stamp.rs` answers. A 204
 * here would be a server contradicting its own contract, and it surfaces as
 * `getJson`'s `SyntaxError` rather than a sentence of ours.
 *
 * The alternative — a bespoke `fetch` in this module, or a 204 branch inside
 * the shared `getJson` — buys a better message for a case no route produces,
 * at the price of either a second place for the 401 rule to be forgotten
 * (which is the failure `whenSignedOut` exists to have fixed once) or a new
 * `undefined` in the return type of every GET in the app.
 */
export const stampPlan = (slug: string): Promise<StampPlan> =>
  getJson<StampPlan>(route(slug))

/**
 * Do it.
 *
 * A refusal arrives as a thrown `Error` carrying the server's own sentence: a
 * 409 is either the plan no longer matching the library or a library job
 * already running (the serve crate's `LibraryJob` names them), and both are
 * worth printing verbatim rather than translating.
 */
export async function performStamp(
  slug: string,
  archives: Archive[],
): Promise<Stamped> {
  const body = await sendJson<Stamped>(route(slug), 'POST', { archives })
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}
