/**
 * A page's reads, and whether its body may be drawn (spec §4). The page
 * names every read its body needs; the hook reads them from the store as one
 * set and says when the body may be drawn. A set whose requests it started
 * itself lands in one step, so the page moves once, unless a newer request
 * overtakes one of them; a read that joins a request another set started
 * lands with whichever of the two sets lands first, which may be sooner.
 *
 * **What counts as settled.** A read is settled for the page when its entry
 * is there and not stale, and either holds an answer or holds a failure this
 * mount's own read has settled since. So a revisit with every answer kept
 * and fresh draws at once, a stale answer is never drawn on arrival, and a
 * failure kept from an earlier visit waits for this visit's read. A resolved
 * set is never taken to mean an entry is there: the store keeps only the
 * newest request's answer, so a member of the set may have been overtaken.
 *
 * **A failure is handed over once this mount has read its address.** A
 * failure kept from an earlier visit is not drawn on arrival, though an
 * answer kept beside it is; this visit's read hands over whatever failure it
 * leaves, and so does every read after it, this page's or another's.
 *
 * **Once ready, ready while the addresses are the same.** The store marks an
 * entry stale without telling anyone, so a page drawn from it reads the mark
 * whenever it is drawn again for a reason of its own; it keeps drawing the
 * answer until the re-read lands (§4.3).
 *
 * **Keyed by address, not by `Read`.** A read whose address is worked out
 * from another's answer (`libraryScan(id)`) is a new `Read` on every render,
 * and must not be read again for that.
 */
import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react'
import { useDrawnWait } from './hooks'
import { type Entry, entryOf, type Read, readSet, subscribe } from './reads'

export interface Page<T extends readonly unknown[]> {
  /**
   * The body may be drawn: on arrival every read answered and none stale,
   * or every read has settled since. Once true it stays true for this mount
   * while the addresses are unchanged (§4.3).
   */
  readonly ready: boolean
  readonly answers: { readonly [K in keyof T]: T[K] | undefined }
  /**
   * Each read's latest failure, once a read of its address by this mount
   * has settled; `undefined` before that, whatever the store keeps.
   */
  readonly failures: { readonly [K in keyof T]: unknown }
  /** `useDrawnWait(!ready)`. */
  readonly loadingDrawn: boolean
}

type Reads = readonly (Read<unknown> | undefined)[]
type Entries = readonly (Entry<unknown> | undefined)[]

/** The list's addresses as one string, `null` for a read not named yet. */
function keyOf(reads: Reads): string {
  return JSON.stringify(reads.map((r) => r?.address ?? null))
}

function addressesIn(key: string): Set<string> {
  const named = JSON.parse(key) as (string | null)[]
  return new Set(named.filter((address) => address !== null))
}

/**
 * An `undefined` entry is an address not known yet (§4.5): the page is not
 * ready. An empty list is ready, and hands back no answer and no failure,
 * whatever `T` names: a page that reads nothing for now, such as a closed
 * panel, passes `[]`.
 */
export function usePage<T extends readonly unknown[]>(
  reads: { readonly [K in keyof T]: Read<T[K]> | undefined } | readonly [],
): Page<T> {
  const list = reads as Reads
  const key = keyOf(list)

  const subscribeAll = useCallback(
    (changed: () => void) => {
      const offs = [...addressesIn(key)].map((a) => subscribe(a, changed))
      return () => {
        for (const off of offs) off()
      }
    },
    [key],
  )
  // The same array while every entry is the same object, as
  // `useSyncExternalStore` needs of a snapshot.
  const snapshot = useRef<Entries>([])
  const entries = useSyncExternalStore(subscribeAll, () => {
    const now = list.map((r) => (r === undefined ? undefined : entryOf(r)))
    const kept = snapshot.current
    if (now.length !== kept.length || now.some((e, i) => e !== kept[i])) {
      snapshot.current = now
    }
    return snapshot.current
  })

  /** Addresses whose read by this mount has settled. */
  const settled = useRef(new Set<string>())
  /** The set that last read each address for this mount. */
  const readBy = useRef(new Map<string, object>())
  /** The list's addresses as the effect below last saw them. */
  const listed = useRef(new Set<string>())
  /** Counts the sets of this mount that have settled with a failure. */
  const [settles, drawAgain] = useReducer((n: number) => n + 1, 0)

  // A passive effect, as the subscription `useSyncExternalStore` makes is,
  // and React runs one component's passive effects back to back: another
  // component's mount effect runs before both or after both, so an
  // invalidation it makes never falls between the read and the
  // subscription, where nothing would draw the address and the read's
  // request would be dropped. Which of the two is declared first changes
  // nothing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` stands for `list`, whose `Read`s may be new objects on every render for the same addresses; reading again for those would send a request per render.
  useEffect(() => {
    const now = addressesIn(key)
    for (const address of listed.current) {
      if (!now.has(address)) {
        settled.current.delete(address)
        readBy.current.delete(address)
      }
    }
    // The addresses new to the list, so that one named once another's
    // answer is known is waited for alone (§4.5); and any kept one with no
    // fresh entry. Between the cleanup of the subscription to the old list
    // and the new subscription, a child's mount effect may have invalidated
    // a kept address while nothing drew it, which drops its request; one
    // still out is joined, not sent again.
    const fresh = list.filter((r): r is Read<unknown> => {
      if (r === undefined) return false
      if (!listed.current.has(r.address)) return true
      const entry = entryOf(r)
      return entry === undefined || entry.stale
    })
    listed.current = now
    if (fresh.length === 0) return
    const set = {}
    for (const r of fresh) readBy.current.set(r.address, set)
    void readSet(fresh).then(() => {
      let failed = false
      for (const r of fresh) {
        if (readBy.current.get(r.address) !== set) continue
        settled.current.add(r.address)
        const entry = entryOf(r)
        failed ||=
          entry !== undefined && !entry.stale && entry.failure !== undefined
      }
      // The store told the page of the set as it was applied, before this
      // ran; a failure counts only from here, both as a failure handed over
      // and, with no answer beside it, as settled, so the page is drawn
      // again to see it. An answer counted already.
      if (failed) drawAgain()
    })
  }, [key])

  const settledNow = list.every((r, i) => {
    const entry = entries[i]
    return (
      r !== undefined &&
      entry !== undefined &&
      !entry.stale &&
      (entry.answer !== undefined || settled.current.has(r.address))
    )
  })
  const [held, setHeld] = useState({ key, ready: settledNow })
  if (held.key !== key || (settledNow && !held.ready)) {
    setHeld({ key, ready: settledNow })
  }
  const ready = settledNow || (held.key === key && held.ready)
  const loadingDrawn = useDrawnWait(!ready)

  const answers = useMemo(() => entries.map((e) => e?.answer), [entries])
  // biome-ignore lint/correctness/useExhaustiveDependencies: `settles` is a trigger: a settle adds to `settled`, a ref, and draws the page again to hand over the failure it left.
  const failures = useMemo(() => {
    const named = JSON.parse(key) as (string | null)[]
    return entries.map((e, i) => {
      const address = named[i]
      return address !== null && settled.current.has(address)
        ? e?.failure
        : undefined
    })
  }, [entries, key, settles])
  return {
    ready,
    answers: answers as unknown as Page<T>['answers'],
    failures: failures as unknown as Page<T>['failures'],
    loadingDrawn,
  }
}
