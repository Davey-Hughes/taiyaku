/**
 * One root: what it is, what it defaults to, and what its last scan did —
 * spec C4 §2 and §3.
 *
 * **The card owns its own scan poll**, the way `ProposalRow` owns its own
 * evidence fetch: a scan belongs to the root being scanned rather than to the
 * page, and hoisting the poll to the list would make a page with six roots
 * poll six times whether or not any of them is scanning.
 *
 * **It asks the status route once on mount, and that is not optional.** The
 * server keeps `ServeState.scans` precisely so a client can poll a scan across
 * requests; a card that only ever polled after pressing its own button showed
 * nothing at all to an admin who navigated away during a scan and came back —
 * no progress, no eventual report, and an enabled "Scan now" whose only answer
 * was a 409 about the scan they had started themselves. Whole-branch review,
 * 2026-09-04. The cost is one request per root per mount, against an in-memory
 * registry lookup plus one indexed row — far below the dry-run counts, which
 * stay lazy because each of those is a real derivation.
 *
 * **A field is patched on blur, and only the field the admin actually
 * edited** — an absent key means "leave this column alone" to
 * `patch_nullable_text`, so sending the whole row on every edit would rewrite
 * five columns to answer a question about one. **An emptied box clears the
 * column to `null`, not to `''`**: the schema's nullable columns mean "nothing
 * set", and an empty string is a value nothing in the store ever means.
 *
 * **"Edited" is an `input` event, not a value comparison, and getting there
 * took three rounds of one review.** The obvious rule — write when the box
 * differs from what the row holds — cannot work for `exclude_globs`, whose
 * value the row never reports, so `from` answered `''` forever and an admin
 * who set a pattern then emptied the box hit `'' === ''` and wrote nothing
 * (pass 1). Comparing against the last value *this card* had sent fixed that
 * only within a single mount, since that state starts empty on every load
 * (pass 3). Treating the column's value as *unknown* instead removed the
 * guard altogether, so a bare focus-and-tab-away sent `{exclude_globs: null}`
 * and silently deleted a stored pattern — config loss nothing would have
 * noticed, since O92 means nothing reads the column yet (pass 4). Whether
 * somebody typed in the box is the fact all three were reaching for, and it
 * is the one the DOM already reports. `from` returning `undefined` for a
 * column the row cannot describe is the other half: it makes "no opinion"
 * expressible instead of faked with `''`.
 *
 * **`exclude_globs` is edited here and does nothing.** O92: the column ships
 * in migration 1, `scan::update_root` patches it, and `scan::scan_root` never
 * reads it, so nothing is ever excluded from a scan. It is drawn because C4
 * §2 lists it among a root's fields and because leaving it out would hide the
 * gap rather than record it; the label says so in as many words. It is also
 * **write-only**: `GET /api/library/roots` does not send the column, so the box
 * starts empty whatever the row holds.
 *
 * **Live progress is elapsed seconds and nothing else.** `ScanProgress`
 * carries a start `Instant` and a done flag and no per-file counter; drawing a
 * bar over a number nobody computes would be an invented figure on the screen
 * whose own discipline is that a report names what it did not do.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  patchRoot,
  type Probe,
  regimeLine,
  type Root,
  type RootPatch,
  type Derived,
  SCAN_POLL_MS,
  type ScanOutcome,
  scanStatus,
  startScan,
  statusOf,
} from './config'
import { RemoveRoot } from './RemoveRoot'
import { DeriveSection, ScanReport } from './ScanReport'

export interface RootRowProps {
  root: Root
  /** Only a root added in this session has one. See `config.ts`'s `Probe`. */
  probe?: Probe
  /** The list reloads on a save, a removal, or a finished scan. */
  onChanged: () => void
}

/**
 * The four text columns this card edits.
 *
 * **Each carries its own `patch` builder rather than a `keyof RootPatch` and a
 * computed key.** `save({ [field.name]: value })` types as
 * `{ [x: string]: string | null }`, which is not a `RootPatch` — TypeScript
 * widens a computed key even when the key's type is a union of literals — so
 * the alternative was a cast, and a cast here would silently accept a field
 * name `RootPatch` does not have. Four three-line builders cost less than one
 * `as` that turns a typo into a 400.
 */
interface TextField {
  name: string
  label: string
  /**
   * What the `root` row says this column holds, or `undefined` where the row
   * cannot say.
   *
   * `exclude_globs` is the `undefined` case, and it is not a quirk: the list
   * route does not send that column, so the row reports nothing about it and
   * an emptied box there is a genuine instruction rather than a no-op. Every
   * other column answers a string, and an edit back to that string writes
   * nothing.
   */
  from: (root: Root) => string | undefined
  patch: (value: string | null) => RootPatch
}

const TEXT_FIELDS: TextField[] = [
  {
    name: 'edition_key',
    label: 'Edition key',
    from: (r) => r.edition_key ?? '',
    patch: (edition_key) => ({ edition_key }),
  },
  {
    name: 'default_kind',
    label: 'Default kind',
    from: (r) => r.default_kind ?? '',
    patch: (default_kind) => ({ default_kind }),
  },
  {
    name: 'default_language',
    label: 'Default language',
    from: (r) => r.default_language ?? '',
    patch: (default_language) => ({ default_language }),
  },
  {
    name: 'exclude_globs',
    label: 'Exclude globs (recorded, not yet applied — O92)',
    // The list route does not send this column, so the row can never say what
    // it holds — `undefined`, not `''`. See `from`'s own doc comment.
    from: () => undefined,
    patch: (exclude_globs) => ({ exclude_globs }),
  },
]

/** How many consecutive failed polls a running scan survives. See `misses`. */
const POLL_MISSES_ALLOWED = 3

/** The one sentence a bad scan interval gets, in both of its shapes. */
const BAD_INTERVAL =
  'A scan interval has to be a whole number of seconds, at least one. ' +
  'Leave it empty to set no interval at all.'

export function RootRow({ root, probe, onChanged }: RootRowProps) {
  /**
   * Why the last write or scan *request* failed. Cleared by the next one.
   *
   * **Separate from `scanNotice`, for the reason `Roots.tsx` keeps `failure`
   * and `addFailure` apart** — and this is the same defect one level down,
   * left standing when that one was fixed. A single slot meant a routine
   * `PATCH` succeeding wiped "this server has no record of that scan", which
   * is the one message saying the library may be half-derived. Pass 3 of the
   * whole-branch review.
   */
  const [failure, setFailure] = useState<string | undefined>()
  /** What the scan poll has to say. Cleared only by starting another scan. */
  const [scanNotice, setScanNotice] = useState<string | undefined>()
  const [confirming, setConfirming] = useState(false)
  const [running, setRunning] = useState(false)
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | undefined>()
  const [report, setReport] = useState<ScanOutcome | null>(null)
  /**
   * What the real removal's own re-derive found, kept after the dialog closes.
   *
   * **Its own state rather than folded into `report`.** A removal produces a
   * `DeriveReport` and no `ScanReport` — there was no walk — so pushing it
   * through `ScanOutcome` would need a cast asserting a `scan` and a `note`
   * that do not exist. See `RemoveRoot`'s `onRemoved` for why it must reach
   * somebody at all. Pass 4 of the whole-branch review.
   */
  const [removalDerive, setRemovalDerive] = useState<Derived | null>(null)
  /**
   * Which text boxes the admin has actually typed in since focusing them.
   *
   * **A write happens because somebody edited a box, never because its value
   * differs from something.** Two rounds of this branch tried to decide it by
   * comparison and both were wrong, in opposite directions: comparing against
   * the row meant `exclude_globs` — whose stored value the row cannot report —
   * could never be cleared, and treating its value as *unknown* instead meant
   * a bare focus-and-tab-away sent `{exclude_globs: null}` and silently
   * deleted a stored pattern. Editing is the fact both were reaching for, and
   * an `input` event is what says it happened. Whole-branch review passes 1,
   * 3 and 4.
   */
  const edited = useRef<Set<string>>(new Set())
  /**
   * The checkbox's own value, because it has to be able to go back.
   *
   * **Controlled, where the four text boxes are not.** An uncontrolled
   * checkbox cannot be reset by a reloaded prop, so a refused `PATCH` left it
   * asserting the opposite of what migration 012 actually holds for the root —
   * and clicking it again sent the value the server already had, so the only
   * way back was two more clicks. A text box at least keeps the admin's typed
   * text as a retry target; a tick reads as settled. Review pass 2.
   */
  const [keepsDocuments, setKeepsDocuments] = useState(root.keeps_documents)

  // **Follow the row when it changes underneath.** The state is seeded once,
  // so a value changed from anywhere but this card — another admin, another
  // tab — left the tick asserting the opposite of what migration 012 holds,
  // and the next click then sent the value the server already had, taking two
  // clicks to get back. That is the same shape pass 2 fixed for the refused-
  // PATCH path. Pass 4, 2026-09-04.
  useEffect(() => {
    setKeepsDocuments(root.keeps_documents)
  }, [root.keeps_documents])

  // `onChanged` is called from inside the poll's interval callback, so it is
  // held in a ref rather than named as a dependency: a parent handing over a
  // fresh closure each render would otherwise tear down and restart the
  // interval on every one of its own renders, which is a poll that never
  // fires at its own cadence.
  const changed = useRef(onChanged)
  changed.current = onChanged

  /**
   * Whether a `POST` is already on its way.
   *
   * **A ref, not the `starting` state below, and that distinction is the whole
   * fix.** React batches state updates, so two clicks dispatched before the
   * next render both read the *old* `disabled` — the button is still enabled
   * on the second, and both POSTs go. A ref is written and read synchronously,
   * so the second click sees the first. `starting` still exists because it is
   * what *draws* the disabled button; this is what enforces it.
   */
  const inFlight = useRef(false)

  /**
   * Consecutive failed polls. Reset by any answer.
   *
   * **A single failure must not abandon a live scan.** The `.catch` used to
   * `setRunning(false)`, which tore the interval down for good — so one blip
   * (a proxy 502, a `SQLITE_BUSY` while the scan's own writes hold the file)
   * produced no progress, no eventual report, and a re-enabled "Scan now"
   * whose only answer is a 409 about the scan still running: exactly the state
   * the mount read above exists to prevent, reached by another door. Review
   * pass 2, 2026-09-04. Three is chosen, not measured — long enough to ride
   * out a blip at one second apiece, short enough that a genuinely broken
   * route says so rather than polling for ever.
   */
  const misses = useRef(0)

  const save = (field: TextField, next: string) => {
    patchRoot(root.id, field.patch(next === '' ? null : next))
      .then(() => {
        setFailure(undefined)
        onChanged()
      })
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  const savePatch = (patch: RootPatch) => {
    patchRoot(root.id, patch)
      .then(() => {
        setFailure(undefined)
        onChanged()
      })
      .catch((err: unknown) => {
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  /**
   * Fold one status answer into this card's state.
   *
   * Shared by the mount read and the poll so the two cannot disagree about
   * what a given answer means.
   */
  const absorb = useCallback(
    (status: {
      running: boolean
      elapsed_secs?: number
      report: ScanOutcome | null
    }) => {
      setElapsed(status.elapsed_secs)
      if (status.running) {
        setRunning(true)
        return
      }
      setRunning(false)
      setReport(status.report)
    },
    [],
  )

  // **The mount read.** See this file's own doc comment. It runs once per
  // mount and seeds `running`, so a scan already in flight is drawn and then
  // polled by the effect below exactly as one this card started would be.
  useEffect(() => {
    let cancelled = false
    scanStatus(root.id)
      .then((status) => {
        if (cancelled) return
        // Only a *running* scan is adopted on mount. A finished one belongs to
        // whatever session started it, and drawing its report unasked would
        // put a stale wall of counts under every card on every page load.
        if (status.running) absorb(status)
      })
      .catch(() => {
        // Deliberately silent. A status read this card did not ask for must
        // not put a red alert over a screen the admin has only just opened;
        // pressing "Scan now" is what surfaces a real fault.
      })
    return () => {
      cancelled = true
    }
  }, [root.id, absorb])

  const begin = useCallback(() => {
    // Guarded synchronously, not by `disabled` alone: that left two rapid
    // presses both sending, and the second's 409 — about the admin's own scan
    // — landed in the alert slot while that scan ran fine.
    if (inFlight.current) return
    inFlight.current = true
    setStarting(true)
    setFailure(undefined)
    setScanNotice(undefined)
    setReport(null)
    setRemovalDerive(null)
    setElapsed(undefined)
    misses.current = 0
    startScan(root.id)
      .then(() => {
        inFlight.current = false
        setStarting(false)
        setRunning(true)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setStarting(false)
        const why = err instanceof Error ? err.message : String(err)
        setFailure(why)
        // **A 409 is positive proof a scan is in flight**, so it starts the
        // poll rather than only complaining. Without this the card stayed
        // blind after any route back to a re-enabled button — three failed
        // polls, say — showing no progress, no report and no reload when the
        // scan finished, and answering every retry with the same 409: exactly
        // the state `misses` above exists to prevent. Pass 3.
        if (statusOf(why) === 409) {
          misses.current = 0
          setRunning(true)
        }
      })
  }, [root.id])

  useEffect(() => {
    if (!running) return
    let cancelled = false
    const timer = setInterval(() => {
      scanStatus(root.id)
        .then((status) => {
          if (cancelled) return
          misses.current = 0
          absorb(status)
          if (status.running) return
          // **A `null` report on a stopped scan is not a clean finish.** The
          // registry is in memory, so a `serve` restarted mid-scan answers
          // exactly this — and writing it through silently cleared "Scanning…"
          // and drew nothing, which reads as success. Whole-branch review.
          if (status.report === null) {
            setScanNotice(
              'This server has no record of that scan. It was most likely ' +
                'interrupted by a restart; nothing here says whether it ' +
                'finished. Scan again to be sure.',
            )
          }
          changed.current()
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses.current += 1
          if (misses.current < POLL_MISSES_ALLOWED) return
          setRunning(false)
          setScanNotice(err instanceof Error ? err.message : String(err))
        })
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [running, root.id, absorb])

  return (
    <li className="root-row">
      <h3>
        <code>{root.path}</code>
      </h3>
      <p className="regime">{regimeLine(root, probe)}</p>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {scanNotice !== undefined && (
        <p role="alert" className="scan-notice">
          {scanNotice}
        </p>
      )}

      <div className="fields">
        {TEXT_FIELDS.map((field) => (
          <label key={field.name}>
            <span>{field.label}</span>
            <input
              type="text"
              name={field.name}
              defaultValue={field.from(root) ?? ''}
              onFocus={() => {
                edited.current.delete(field.name)
              }}
              onChange={() => {
                edited.current.add(field.name)
              }}
              onBlur={(e) => {
                const next = e.target.value
                // Nothing typed since this box was focused: nothing to write,
                // whatever the box happens to show.
                if (!edited.current.has(field.name)) return
                edited.current.delete(field.name)
                // A column the row *can* report, edited back to what it
                // already holds, is a no-op. One it cannot report always
                // sends — which is the only way to clear such a column.
                const stored = field.from(root)
                if (stored !== undefined && next === stored) return
                save(field, next)
              }}
            />
          </label>
        ))}
        <label>
          <span>Scan interval (seconds)</span>
          <input
            type="number"
            name="scan_interval_secs"
            min="1"
            step="1"
            defaultValue={root.scan_interval_secs ?? ''}
            onBlur={(e) => {
              const box = e.target
              // **An unparseable entry reads as `''`**, not as the text typed:
              // that is what `<input type=number>` reports. So `12e` blurred
              // looked identical to an emptied box and cleared a column nobody
              // meant to touch. `badInput` is the only thing that tells them
              // apart. Whole-branch review, 2026-09-04.
              if (box.validity.badInput) {
                setFailure(BAD_INTERVAL)
                return
              }
              const raw = box.value
              const next = raw === '' ? null : Number(raw)
              if (next === root.scan_interval_secs) return
              if (next !== null && (!Number.isInteger(next) || next < 1)) {
                setFailure(BAD_INTERVAL)
                return
              }
              setFailure(undefined)
              savePatch({ scan_interval_secs: next })
            }}
          />
        </label>
        <label>
          <span>Keeps documents under .taiyaku/</span>
          <input
            type="checkbox"
            name="keeps_documents"
            checked={keepsDocuments}
            onChange={(e) => {
              const next = e.target.checked
              setKeepsDocuments(next)
              patchRoot(root.id, { keeps_documents: next })
                .then(() => {
                  setFailure(undefined)
                  onChanged()
                })
                .catch((err: unknown) => {
                  // Back to what the server still holds. See the state's own
                  // doc comment for why this box is controlled and the text
                  // ones are not.
                  setKeepsDocuments(!next)
                  setFailure(err instanceof Error ? err.message : String(err))
                })
            }}
          />
        </label>
      </div>

      <div className="actions">
        {/*
          `confirming` disables this too, and that is not tidiness. The
          removal dialog fetches its dry run once, on `[root.id]`, and never
          refetches — so a scan started here while it is open imports works
          underneath a sentence still quoting the old counts, and the confirm
          below it then sends a real DELETE into a running derive, which is
          the interleaving `DeriveReport::root_removed_mid_derive` counts.
          Review pass 2, 2026-09-04.
        */}
        <button
          type="button"
          onClick={begin}
          disabled={running || starting || confirming}
        >
          Scan now
        </button>
        <button
          type="button"
          onClick={() => setConfirming(true)}
          disabled={running || starting || confirming}
        >
          Remove…
        </button>
      </div>

      {running && (
        <p className="scanning">
          Scanning…{elapsed === undefined ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}
      {report !== null && <ScanReport outcome={report} />}
      {removalDerive !== null && (
        <div className="removal-report">
          <p>What re-deriving after that removal found:</p>
          <DeriveSection derive={removalDerive} />
        </div>
      )}
      {confirming && (
        <RemoveRoot
          root={root}
          onCancel={() => setConfirming(false)}
          onRemoved={(answer) => {
            setConfirming(false)
            setRemovalDerive(answer.derive)
            onChanged()
          }}
        />
      )}
    </li>
  )
}
