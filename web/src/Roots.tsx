/**
 * The roots screen — spec C4 §2 and §3.
 *
 * **The list owns which roots there are; each card owns what it is doing.**
 * `Proposals`/`ProposalRow`'s split and its stated reason: the evidence a row
 * fetches belongs to the thing being judged, and here that is the dry-run
 * counts and the scan poll.
 *
 * **The probe from an add is kept, keyed by the root's path.** `watch::probe`
 * runs on `POST` and nowhere else, so a root added in this session has a watch
 * state and one loaded from the list does not — see `config.ts`'s `Probe`.
 * Holding the answer here rather than dropping it is the difference between
 * showing the regime C4 §2 asks for "before the first scan" and showing it
 * never.
 *
 * **By path and not by id, and that is a correctness fix rather than taste.**
 * `root.id` is `INTEGER PRIMARY KEY` with no `AUTOINCREMENT`, so SQLite
 * reissues a freed id to the next `add_root` — `delete_root`'s own MINOR 6
 * comment says exactly this, and the server clears its `state.scans` slot for
 * the same reason. Keyed by id, removing a root and adding another handed the
 * new one the removed root's watch state, about a directory it has nothing to
 * do with. A path cannot collide that way: two roots may not overlap, so no
 * two rows ever share one. Whole-branch review, 2026-09-04.
 *
 * **The add banner is derived, not remembered — but not *only* derived.** It
 * is drawn while the root it names is still in the list, so removing that root
 * takes the banner with it and nothing has to remember to clear it. It used to
 * persist unconditionally, which put a stale success notice directly above a
 * later add's refusal.
 *
 * **It is also drawn when the list has not yet confirmed the root is gone.**
 * Deriving it from the list alone meant a `reload()` that rejected — the add
 * having succeeded — left `roots` at its pre-add value, the predicate false,
 * and the probe answer discarded. Nothing re-probes: `watch::probe` runs on
 * `POST` and nowhere else, so that answer is the only chance to know the
 * regime "before the first scan", which is exactly what C4 §2 asks the screen
 * to show (pass 3). But keying that on "the last read failed" made the
 * predicate true again after *any* later failure, so the banner returned for a
 * root that had since been removed (pass 4). What settles it is a fact rather
 * than a state: a **successful** list that does not hold the root is the only
 * thing that retires the banner, and it retires it for good.
 *
 * **No client-side path validation.** All three of C4 §2's checks run in the
 * server process, and the second one — is this path readable *by the server*
 * — is a question a browser cannot ask at all. The refusal's own sentence
 * names which check failed; this screen draws it.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  type Added,
  addRoot,
  humanInterval,
  listRoots,
  type Probe,
  type Root,
} from './config'
import { RootRow } from './RootRow'

export function Roots() {
  const [roots, setRoots] = useState<Root[] | undefined>()
  /**
   * Why the list could not be read. Separate from `addFailure` below.
   *
   * **One slot served both, and a background reload wiped the other's
   * message.** `reload` is handed to every `RootRow` as `onChanged`, and its
   * `.then` clears the failure — so a scan finishing in another row, a field
   * save, or a removal silently erased the add form's refusal, which is the
   * one message on this screen that names *which* of the three validations
   * said no. Review pass 2, 2026-09-04.
   */
  const [failure, setFailure] = useState<string | undefined>()
  /** Why the last add was refused. Cleared only by the next add. */
  const [addFailure, setAddFailure] = useState<string | undefined>()
  const [path, setPath] = useState('')
  const [adding, setAdding] = useState(false)
  const [probes, setProbes] = useState<Record<string, Probe>>({})
  const [justAdded, setJustAdded] = useState<Added | undefined>()
  /**
   * Whether an add is already on its way.
   *
   * **A ref, for the reason `RootRow`'s own `inFlight` is one**: React batches
   * state, so two submits dispatched before the next render both read
   * `adding === false` and both send. Reachable by holding Enter in the path
   * box as well as by a double-click, and the second POST passes all three
   * validations — the first has not committed on its own connection yet — then
   * trips `path TEXT NOT NULL UNIQUE`, so it answers 500 and renders raw
   * SQLite text above the success banner for the root just added. The first
   * fix round closed this in `RootRow` and left it open here and in
   * `RemoveRoot`; review pass 2 found both.
   */
  const inFlight = useRef(false)
  /** The newest `listRoots` call. See `reload`. */
  const latest = useRef(0)

  const reload = useCallback(() => {
    // **A sequence number, because responses do not have to arrive in
    // order.** `reload` is every row's `onChanged` and fires from field
    // saves, removals and finished scan polls, so several `listRoots` calls
    // are routinely in flight; without this an earlier response landing later
    // wrote the stale array and a just-removed root reappeared until
    // something else triggered a reload. `Search.tsx` carries the same
    // counter for the same reason. Pass 4.
    const seq = ++latest.current
    listRoots()
      .then((body) => {
        if (seq !== latest.current) return
        setRoots(body.roots)
        setFailure(undefined)
        // A successful read is the only thing that can retire the add banner
        // — see this file's own doc comment.
        setJustAdded((held) =>
          held !== undefined &&
          !body.roots.some((r) => r.path === held.root.path)
            ? undefined
            : held,
        )
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (path === '') return
    if (inFlight.current) return
    inFlight.current = true
    setAdding(true)
    setJustAdded(undefined)
    setAddFailure(undefined)
    addRoot(path)
      .then((added) => {
        inFlight.current = false
        setAdding(false)
        setPath('')
        setAddFailure(undefined)
        setJustAdded(added)
        setProbes((held) => ({
          ...held,
          [added.root.path]: {
            watching: added.watching,
            watch_reason: added.watch_reason,
          },
        }))
        reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setAdding(false)
        setAddFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <section className="roots">
      <h2>Roots</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}

      {justAdded !== undefined && (
        <p className="probe">
          {justAdded.root.path}:{' '}
          {justAdded.watching
            ? 'the watcher is running'
            : `the watch probe failed${
                justAdded.watch_reason === null
                  ? ''
                  : `: ${justAdded.watch_reason}`
              }`}
          , so it will be scanned every {humanInterval(justAdded.interval_secs)}
          .
        </p>
      )}

      {roots !== undefined &&
        (roots.length === 0 ? (
          <p className="empty">
            No roots yet. Add the directory your library lives in below; the
            server checks that it can read it and that it does not overlap one
            you already have.
          </p>
        ) : (
          <ul className="root-list">
            {roots.map((root) => (
              <RootRow
                // **Keyed by path, not id, for the reason `probes` is.**
                // SQLite reissues a freed `root.id`, and React matches on
                // `key` — so a recycled id makes it *reuse this RootRow
                // instance*: its report, scan notice, running flag and four
                // uncontrolled inputs all survive into a different root, and
                // the mount read (deps `[root.id]`) does not re-run, so a scan
                // actually running on the new root is never adopted. Two roots
                // may not overlap, so a path cannot collide. Pass 4.
                key={root.path}
                root={root}
                probe={probes[root.path]}
                onChanged={reload}
              />
            ))}
          </ul>
        ))}

      {addFailure !== undefined && <p role="alert">{addFailure}</p>}
      <form onSubmit={submit} className="add-root">
        <label>
          <span>Add a root</span>
          <input
            type="text"
            name="path"
            value={path}
            placeholder="/srv/manga"
            onChange={(e) => setPath(e.target.value)}
          />
        </label>
        <button type="submit" disabled={adding || path === ''}>
          {adding ? 'Checking…' : 'Add'}
        </button>
      </form>
    </section>
  )
}
