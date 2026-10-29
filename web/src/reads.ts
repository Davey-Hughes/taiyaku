/**
 * The pages' reads, kept in this tab's memory: one entry per `GET` address,
 * holding the last good answer, the latest failure, and whether a write has
 * made it stale (spec §3).
 *
 * **A set of reads lands together.** A page names every read it needs as one
 * set; the set's answers are applied when the last of them settles, and each
 * listener is told once, so a page drawn from several reads moves in one step
 * rather than one per answer. A request two sets share is applied by
 * whichever lands first, and one a newer request overtook is not applied.
 *
 * **Only the newest request for an address is kept.** `invalidate` never
 * joins a request that is out, because the one out may predate the write
 * that made the address stale; an answer to any request but the newest is
 * dropped, so an older answer landing late cannot undo a newer one.
 *
 * **A mark can tell no one.** `markStale` marks an entry for a write that no
 * page on screen is to take up: a page drawing it keeps drawing it, and one
 * arriving later waits for a read of its own, as after `invalidate`.
 *
 * **What nothing draws is kept up to a count.** The Library view's addresses
 * carry a library, a series or a query, so a long session would keep one
 * entry per series and search visited. Past `IDLE_KEPT` entries nothing
 * draws, the one drawn least recently goes; its next arrival is a first
 * visit. An entry something draws, or one whose request is out, is never
 * dropped: the first is on screen, and the second's answer would be lost.
 *
 * No React here: `usePage` subscribes through `useSyncExternalStore` on
 * `subscribe` and `entryOf`, which is why an entry is the same object until
 * something in it changes, and why an answer with the same text as the kept
 * one keeps the kept answer, and the entry too unless it was stale or held
 * a failure.
 */

/** One GET the store keeps: its address, and the client call that reads it. */
export interface Read<T> {
  readonly address: string
  readonly fetch: () => Promise<T>
}

export function read<T>(address: string, fetch: () => Promise<T>): Read<T> {
  return { address, fetch }
}

/** What the store holds for one address. The same object until it changes. */
export interface Entry<T> {
  /** The last good answer; `undefined` until one lands. */
  readonly answer: T | undefined
  /** The latest attempt's failure, cleared by the next good answer. */
  readonly failure: unknown
  /**
   * Marked by `invalidate` or `markStale`, and as the last page drawing the
   * address goes when a request out at a `markStale` settled it
   * (`Slot.settledBeforeMark`); cleared when a re-read settles.
   */
  readonly stale: boolean
}

type Outcome =
  | { readonly answered: true; readonly answer: unknown }
  | { readonly answered: false; readonly failure: unknown }

/** One call of an address's client function. */
interface Request {
  readonly address: string
  /** The call's outcome; never rejects, so a set waits on all its members. */
  readonly outcome: Promise<Outcome>
  /**
   * Set once its outcome is in the entry. A request two sets share is
   * applied by whichever lands first, and the other must change nothing: a
   * failure applied twice would be a new entry, and a redraw, for nothing.
   */
  applied: boolean
  /**
   * Set by `markStale` on a request out for an address something drew then:
   * its answer is from before the mark. It is applied if the address is
   * drawn when it lands, which answers the reader that sent it, since any
   * reader arriving since has sent a request of its own that overtakes it;
   * it is dropped otherwise. No later read joins it, and the entry it
   * settles is marked stale once nothing draws the address.
   */
  beforeMark: boolean
}

interface Slot {
  /**
   * The `Read` the latest request here was started from: how
   * `invalidatePrefix` reads again an address it knows only as a key of
   * `slots`.
   */
  read: Read<unknown>
  entry: Entry<unknown> | undefined
  /** `JSON.stringify(entry.answer)`: a new answer's text is compared to it. */
  text: string | undefined
  /**
   * The newest request started for this address, until `invalidate` or
   * `markStale` drops it for an address nothing draws. Only its outcome is
   * kept, and until it is applied a `readSet` joins it rather than sending
   * another, unless it was out at a `markStale`.
   */
  newest: Request | undefined
  /** The entry the listeners were last told of. */
  told: Entry<unknown> | undefined
  /**
   * The entry was last settled by a request out at a `markStale`, for the
   * page that drew the address then: once nothing draws it, the entry is
   * marked stale, so a page arriving after waits for a read of its own.
   */
  settledBeforeMark: boolean
}

/** Entries kept that nothing draws, past which the least recently drawn goes. */
export const IDLE_KEPT = 50

/**
 * In the order last drawn, oldest first: a slot is put at the end as its
 * last listener goes, and one never drawn stays where it was first read.
 */
const slots = new Map<string, Slot>()
const listeners = new Map<string, Set<() => void>>()

function slotOf(r: Read<unknown>): Slot {
  let slot = slots.get(r.address)
  if (slot === undefined) {
    slot = {
      read: r,
      entry: undefined,
      text: undefined,
      newest: undefined,
      told: undefined,
      settledBeforeMark: false,
    }
    slots.set(r.address, slot)
  }
  return slot
}

function drawn(address: string): boolean {
  return (listeners.get(address)?.size ?? 0) > 0
}

/**
 * Drops the slots drawn least recently until at most `IDLE_KEPT` that nothing
 * draws remain. A drawn slot is neither dropped nor counted; one whose newest
 * request is not yet applied is counted but kept, since dropping it would
 * drop the answer its reader waits for.
 */
function trim(): void {
  let idle = 0
  for (const address of slots.keys()) if (!drawn(address)) idle += 1
  for (const [address, slot] of slots) {
    if (idle <= IDLE_KEPT) return
    const out = slot.newest !== undefined && !slot.newest.applied
    if (drawn(address) || out) continue
    slots.delete(address)
    idle -= 1
  }
}

let trimDue = false

/**
 * `trim` once the work running now is done, for the last unsubscribe. React
 * runs a commit's effect cleanups before its mount effects, so a page leaving
 * lets go of its addresses before the page arriving, drawn already from what
 * is kept, subscribes to its own: trimming at once could drop the entry the
 * arriving page has just drawn.
 */
function trimSoon(): void {
  if (trimDue) return
  trimDue = true
  queueMicrotask(() => {
    trimDue = false
    trim()
  })
}

/**
 * Marks the slot's entry stale and tells no one. A new object only where it
 * was not stale already, so a second mark draws nothing again.
 */
function markEntry(slot: Slot | undefined): void {
  const entry = slot?.entry
  if (slot === undefined || entry === undefined || entry.stale) return
  slot.entry = { ...entry, stale: true }
}

/** Calls the client function now, as the newest request for its address. */
function start(r: Read<unknown>): Request {
  // The executor runs before this returns, so the call is sent at once, and
  // a client that throws rather than rejecting fails like any other.
  const call = new Promise<unknown>((resolve) => resolve(r.fetch()))
  const request: Request = {
    address: r.address,
    outcome: call.then(
      (answer): Outcome => ({ answered: true, answer }),
      (failure: unknown): Outcome => ({ answered: false, failure }),
    ),
    applied: false,
    beforeMark: false,
  }
  const slot = slotOf(r)
  slot.read = r
  slot.newest = request
  return request
}

function joinOrStart(r: Read<unknown>): Request {
  const out = slots.get(r.address)?.newest
  return out !== undefined && !out.applied && !out.beforeMark ? out : start(r)
}

/** Puts a settled request's outcome in its entry, if it is still the newest. */
function apply(request: Request, outcome: Outcome): void {
  const slot = slots.get(request.address)
  // No slot: emptied since the request started. Another newest: overtaken.
  if (slot?.newest !== request || request.applied) return
  if (request.beforeMark && !drawn(request.address)) {
    slot.newest = undefined
    return
  }
  request.applied = true
  slot.settledBeforeMark = request.beforeMark
  const kept = slot.entry
  if (!outcome.answered) {
    slot.entry = {
      answer: kept?.answer,
      failure: outcome.failure,
      stale: false,
    }
    return
  }
  const text = JSON.stringify(outcome.answer)
  if (kept !== undefined && slot.text !== undefined && text === slot.text) {
    if (kept.failure !== undefined || kept.stale) {
      slot.entry = { answer: kept.answer, failure: undefined, stale: false }
    }
    return
  }
  slot.entry = { answer: outcome.answer, failure: undefined, stale: false }
  slot.text = text
}

/** Calls each listener of `addresses` once, however many of them it is on. */
function notify(addresses: Iterable<string>): void {
  const due = new Set<() => void>()
  for (const address of addresses) {
    for (const listener of listeners.get(address) ?? []) due.add(listener)
  }
  for (const listener of due) listener()
}

/**
 * Tells the listeners of each address whose entry has changed since they
 * were last told of it.
 */
function tell(addresses: Iterable<string>): void {
  const changed: string[] = []
  for (const address of addresses) {
    const slot = slots.get(address)
    if (slot === undefined || slot.entry === slot.told) continue
    slot.told = slot.entry
    changed.push(address)
  }
  notify(changed)
}

/** Waits for every request, then applies them all and tells once (§3.8). */
async function land(
  requests: readonly Request[],
  addresses: Iterable<string>,
): Promise<void> {
  const outcomes = await Promise.all(requests.map((q) => q.outcome))
  requests.forEach((q, i) => {
    apply(q, outcomes[i])
  })
  trim()
  tell(addresses)
}

export function entryOf<T>(r: Read<T>): Entry<T> | undefined {
  return slots.get(r.address)?.entry as Entry<T> | undefined
}

/**
 * Per address; the count of listeners is what "drawn" means to
 * `invalidate`, `markStale` and the bound. The last to go makes its entry the
 * one drawn most recently, and marks it stale if a request out at a
 * `markStale` settled it (`Slot.settledBeforeMark`).
 */
export function subscribe(address: string, listener: () => void): () => void {
  let set = listeners.get(address)
  if (set === undefined) {
    set = new Set()
    listeners.set(address, set)
  }
  set.add(listener)
  return () => {
    set.delete(listener)
    // An unsubscribe called again after its set was let go finds a later
    // listener's set, or none, and must not take that one away.
    if (set.size > 0 || listeners.get(address) !== set) return
    listeners.delete(address)
    const slot = slots.get(address)
    if (slot === undefined) return
    slots.delete(address)
    slots.set(address, slot)
    if (slot.entry !== undefined && slot.settledBeforeMark) {
      slot.settledBeforeMark = false
      markEntry(slot)
    }
    trimSoon()
  }
}

/** How many addresses have a slot, and a listener: exported for its tests. */
export function heldCounts(): { slots: number; listened: number } {
  return { slots: slots.size, listened: listeners.size }
}

/**
 * Reads `reads` as one set (§3.8): joins the newest request for an address
 * while it is out, starts one otherwise; resolves once the set is applied.
 */
export function readSet(reads: readonly Read<unknown>[]): Promise<void> {
  const requests = reads.map(joinOrStart)
  return land(
    requests,
    requests.map((q) => q.address),
  )
}

/**
 * Marks each stale and reads the drawn ones now as one set, always with new
 * requests (§3.2, §3.6). Resolves once applied.
 */
export function invalidate(...reads: readonly Read<unknown>[]): Promise<void> {
  const named = new Map(reads.map((r) => [r.address, r]))
  const requests: Request[] = []
  for (const [address, r] of named) {
    const slot = slots.get(address)
    // Marked without telling anyone: a page already drawing the answer keeps
    // drawing it until the re-read lands (§4.3), and hears of both at once
    // unless a newer request overtakes the re-read.
    markEntry(slot)
    if (drawn(address)) {
      requests.push(start(r))
    } else if (slot !== undefined) {
      // Not read until something asks for it. A request out for it may
      // predate the write, so it is overtaken: neither joined nor kept.
      slot.newest = undefined
    }
  }
  return land(requests, named.keys())
}

/** Whether `address` is `prefix`, or continues it with a query or a path. */
function under(address: string, prefix: string): boolean {
  return (
    address === prefix ||
    address.startsWith(`${prefix}?`) ||
    address.startsWith(`${prefix}/`)
  )
}

/**
 * `invalidate` for every kept address equal to a prefix or continuing it with
 * `?` or `/`: for a write that changes addresses carrying ids and queries it
 * cannot name one by one. `/api/library/series` covers `?library=2` and `/17`,
 * not `/api/library/seriesX`. An address never read, or dropped by the bound,
 * is not read: its next arrival reads it anyway.
 */
export function invalidatePrefix(...prefixes: readonly string[]): void {
  const matching = [...slots]
    .filter(([address]) => prefixes.some((p) => under(address, p)))
    .map(([, slot]) => slot.read)
  void invalidate(...matching)
}

/**
 * Marks each stale and tells no one, for a write whose change no reader on
 * screen is to take up: a page drawing the answer keeps drawing it, and no
 * request is sent, while a page that arrives later waits for its own read
 * rather than draw what was kept from before the write (§9's call 9). An
 * address with nothing kept has no entry to mark, as a first visit reads it
 * anyway.
 *
 * A request out for an address nothing draws is dropped, as `invalidate`
 * drops it, so its answer from before the mark never lands. One out for an
 * address drawn is the drawing page's read, which may be the one its body
 * waits on, so it still answers that page, not stale, since a waiting page
 * draws no stale entry. No later read joins it, it is dropped if the page
 * has gone when it lands, and the entry it settles is marked stale again
 * once nothing draws the address, so a page arriving after the drawing one
 * has gone waits for a read of its own. A page arriving while it still draws
 * the address would draw that answer at once.
 */
export function markStale(...reads: readonly Read<unknown>[]): void {
  for (const r of reads) {
    const slot = slots.get(r.address)
    if (slot === undefined) continue
    markEntry(slot)
    const out = slot.newest
    if (out === undefined || out.applied) continue
    if (drawn(r.address)) out.beforeMark = true
    else slot.newest = undefined
  }
}

/**
 * A poll's read: a new request applied alone; resolves with its answer,
 * rejects with its failure.
 */
export async function readNow<T>(r: Read<T>): Promise<T> {
  const request = start(r)
  const outcome = await request.outcome
  apply(request, outcome)
  trim()
  tell([r.address])
  if (!outcome.answered) throw outcome.failure
  return outcome.answer as T
}

/**
 * Signing in or out, and every test's end: drops every entry, and every
 * answer still in flight.
 */
export function emptyReads(): void {
  const held = [...slots]
    .filter(([, slot]) => slot.entry !== undefined)
    .map(([address]) => address)
  // Dropping the slots drops their newest requests too, so an answer to one
  // started before this is overtaken when it lands, and nothing joins it.
  slots.clear()
  notify(held)
}
