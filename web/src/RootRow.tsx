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
 * differs from what the row holds — could not work for `exclude_globs`, whose
 * value the row did not report, so `from` answered `''` forever and an admin
 * who set a pattern then emptied the box hit `'' === ''` and wrote nothing
 * (pass 1). Comparing against the last value *this card* had sent fixed that
 * only within a single mount, since that state starts empty on every load
 * (pass 3). Treating the column's value as *unknown* instead removed the
 * guard altogether, so a bare focus-and-tab-away sent `{exclude_globs: null}`
 * and silently deleted a stored pattern (pass 4). Whether somebody typed in
 * the box is the fact all three were reaching for, and it is the one the DOM
 * already reports. **That rule is unchanged now that the row does report the
 * column** — an edit is still what sends, and the equality guard is still the
 * second check rather than the first. What did go is `from`'s `undefined`
 * arm, which existed only to say "this row cannot describe this column".
 *
 * **`exclude_globs` is read, patched, and applied — O92 is closed.** The
 * column ships in migration 1, `scan::update_root` patches it, `GET
 * /api/library/roots` sends it back, and `scan::scan_root` reads it and
 * excludes what it matches. It is drawn as a `<textarea>` rather than the
 * other three columns' `<input>` because the column holds one pattern per
 * line, and a newline is the only separator a real path cannot itself
 * contain — a comma and a space both occur in real directory names. The
 * label says what the syntax is and does not claim the pattern is checked:
 * the server rejects a dangling `\` and an unclosed `{` and accepts
 * everything else, which is a guard against two typos rather than a
 * verification.
 *
 * **Live progress is elapsed seconds and nothing else.** `ScanProgress`
 * carries a start `Instant` and a done flag and no per-file counter; drawing a
 * bar over a number nobody computes would be an invented figure on the screen
 * whose own discipline is that a report names what it did not do.
 */
import {
  type FocusEvent,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import {
  patchRoot,
  type Probe,
  regimeFacts,
  type Root,
  type RootPatch,
  type RemovalAnswer,
  SCAN_POLL_MS,
  type ScanOutcome,
  scanStatus,
  startScan,
  statusOf,
} from './config'
import { RemoveRoot } from './RemoveRoot'
import { ScanReport } from './ScanReport'
import { messageOf, sentenceOf } from './message'

export interface RootRowProps {
  root: Root
  /** Only a root added in this session has one. See `config.ts`'s `Probe`. */
  probe?: Probe
  /** Bumped by the library's card when it starts a scan of the library. */
  adopt?: number
  /** The library's name, where this is its only folder. */
  lastOf?: string
  /**
   * Something else in the library is going on: a scan of the library, a
   * removal of it, or another folder's scan or removal dialog. Scan now and
   * Remove… do nothing then and are drawn disabled, and an open removal
   * dialog is closed.
   */
  held?: boolean
  /** Says whether this row is scanning, starting a scan or confirming a removal. */
  onBusy?: (busy: boolean) => void
  /**
   * The folder is gone, with what its re-derive found. The row is not drawn
   * once the list reloads, so the screen keeps the report.
   */
  onRemoved?: (answer: RemovalAnswer) => void
  /**
   * The list reloads on a save, a removal, or a finished scan. `quiet` is a
   * scan that ended by itself: no press started that read.
   */
  onChanged: (quiet?: boolean) => void
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
   * What the `root` row says this column holds.
   *
   * **Every column answers, including `exclude_globs`.** It used to be the
   * exception — the list route did not send it, so `from` could only answer
   * `undefined` and the blur guard had to let an unreadable column always
   * send. O92's readable half closed that: `root_json` carries the column,
   * so an edit back to the stored string writes nothing here exactly as it
   * does for the other three.
   */
  from: (root: Root) => string
  patch: (value: string | null) => RootPatch
  /**
   * Drawn as a `<textarea>` rather than an `<input>`.
   *
   * `exclude_globs` alone: it holds one pattern per line, and a newline is
   * the only separator a path cannot itself contain — a comma and a space
   * both occur in real directory names, so either would make some real
   * pattern unwritable.
   */
  multiline?: boolean
  /**
   * A line under the box, drawn only when set.
   *
   * **`exclude_globs` alone, because it alone hides a trap the matcher's own
   * correctness makes invisible.** Measured against the `ignore` crate,
   * 2026-09-09: `**\/extras/**` does not exclude the `extras` directory it
   * names, only files under it — so a scan still descends into every
   * `extras`, still records one as a scan entry, and reports "Directories
   * excluded: 0", which reads to an admin as "my pattern did nothing" when
   * gitignore did exactly what was asked. The label already says what the
   * syntax is; this says what one shape of it does that has no equivalent
   * trap on the other three fields, and it is silent about validation on
   * purpose — the server rejects a dangling `\` and an unclosed `{` and
   * accepts everything else, and this line must not imply otherwise.
   */
  hint?: string
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
    // O92 is closed: the scanner applies this now. The label says what the
    // syntax is, because gitignore's is not what "globs" alone implies —
    // `!` negates, a leading `/` anchors, a trailing `/` means the
    // directory. It does *not* claim the pattern is checked: the server
    // rejects a dangling `\` and an unclosed `{` and accepts everything
    // else, which is a guard against two typos rather than a verification.
    label: 'Exclude globs (one gitignore pattern per line)',
    multiline: true,
    from: (r) => r.exclude_globs ?? '',
    patch: (exclude_globs) => ({ exclude_globs }),
    hint:
      'A trailing slash or bare name excludes the directory and everything ' +
      "under it; a '/**' suffix excludes only what's inside it.",
  },
]

/** How many consecutive failed polls a running scan survives. See `misses`. */
const POLL_MISSES_ALLOWED = 3

/** The one sentence a bad scan interval gets, in both of its shapes. */
const BAD_INTERVAL =
  'A scan interval has to be a whole number of seconds, at least one. ' +
  'Leave it empty to set no interval at all.'

export function RootRow({
  root,
  probe,
  adopt,
  lastOf,
  held = false,
  onBusy,
  onRemoved,
  onChanged,
}: RootRowProps) {
  /**
   * O165: each text box is named by its label's own words and described by
   * its hint, rather than through the `<label>` around both — which named
   * Exclude globs with the hint's whole sentence. The hint is reached by an
   * id, per instance, because the libraries screen draws a card per folder.
   *
   * O175: the name is an `aria-label` holding those words, where it was an
   * `aria-labelledby` naming the label's `<span>`. `.field` draws the span
   * in capitals, and Chromium's accessibility tree named the box in those
   * capitals; `aria-labelledby` outranks `aria-label`, so it had to go
   * rather than stay beside it.
   */
  const fieldIds = useId()
  /**
   * Why the last write or scan *request* failed. Cleared by the next one.
   *
   * **Separate from `scanNotice`**, so that a routine `PATCH` succeeding does
   * not wipe "this server has no record of that scan", which is the one
   * message saying the library may be half-derived.
   */
  const [failure, setFailure] = useState<string | undefined>()
  /** What the scan poll has to say. Cleared only by starting another scan. */
  const [scanNotice, setScanNotice] = useState<string | undefined>()
  /** The sentence of a 409, true only until the scan it names ends. */
  const [conflict, setConflict] = useState<string | undefined>()
  const [confirming, setConfirming] = useState(false)
  const [running, setRunning] = useState(false)
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | undefined>()
  const [report, setReport] = useState<ScanOutcome | null>(null)
  /**
   * Which text boxes the admin has actually typed in since focusing them.
   *
   * **A write happens because somebody edited a box, never because its value
   * differs from something.** Two rounds of C4 PR A tried to decide it by
   * comparison and both were wrong, in opposite directions: comparing against
   * the row meant `exclude_globs` — whose stored value the row could not then
   * report — could never be cleared, and treating its value as *unknown*
   * instead meant a bare focus-and-tab-away sent `{exclude_globs: null}` and
   * silently deleted a stored pattern. Editing is the fact both were reaching
   * for, and an `input` event is what says it happened. Whole-branch review
   * passes 1, 3 and 4. **The row does report the column now** (O92's readable
   * half) and the rule is unchanged: this guard still runs first, and the
   * equality check against the row is still the second.
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

  // The card holds this row's sibling rows, and its own Scan now and Remove
  // library…, while this row is doing something, so it is told. Held in a ref
  // for the reason above, and told "not busy" when the row goes.
  const busyNow = running || starting || confirming
  const told = useRef(onBusy)
  told.current = onBusy
  useEffect(() => {
    told.current?.(busyNow)
    return () => told.current?.(false)
  }, [busyNow])

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
        setFailure(sentenceOf(err))
      })
  }

  const savePatch = (patch: RootPatch) => {
    patchRoot(root.id, patch)
      .then(() => {
        setFailure(undefined)
        onChanged()
      })
      .catch((err: unknown) => {
        setFailure(sentenceOf(err))
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
        // A removal being confirmed quotes counts from before this scan.
        setConfirming(false)
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

  // **A scan this row did not start.** The library's own Scan now holds this
  // folder's registry slot, and its card bumps `adopt` when one starts. The
  // row then polls as it does after its own button. The value it was mounted
  // with is not a start: a card mounted mid-scan is adopted by the mount read
  // above.
  const adopted = useRef(adopt)
  useEffect(() => {
    if (adopt === adopted.current) return
    adopted.current = adopt
    setFailure(undefined)
    setConflict(undefined)
    setScanNotice(undefined)
    setConfirming(false)
    setReport(null)
    setElapsed(undefined)
    misses.current = 0
    setRunning(true)
  }, [adopt])

  const begin = useCallback(() => {
    // Guarded synchronously, not by `disabled` alone: that left two rapid
    // presses both sending, and the second's 409 — about the admin's own scan
    // — landed in the alert slot while that scan ran fine.
    if (inFlight.current) return
    inFlight.current = true
    setStarting(true)
    setFailure(undefined)
    setConflict(undefined)
    setScanNotice(undefined)
    setReport(null)
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
        const why = messageOf(err)
        if (statusOf(why) === 409) setConflict(sentenceOf(err))
        else setFailure(sentenceOf(err))
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
          setConflict(undefined)
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
          changed.current(true)
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses.current += 1
          if (misses.current < POLL_MISSES_ALLOWED) return
          setRunning(false)
          // The 409's sentence is true while the scan it named is watched,
          // and nothing watches it now.
          setConflict(undefined)
          setScanNotice(sentenceOf(err))
        })
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [running, root.id, absorb])

  return (
    <li className="root-row">
      <h4>
        <code>{root.path}</code>
      </h4>
      <dl className="regime">
        {regimeFacts(root, probe).map(({ label, value }) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {conflict !== undefined && <p role="alert">{conflict}</p>}
      {scanNotice !== undefined && (
        <p role="alert" className="scan-notice">
          {scanNotice}
        </p>
      )}

      <div className="fields">
        {TEXT_FIELDS.map((field) => {
          const boxProps = {
            name: field.name,
            defaultValue: field.from(root),
            onFocus: () => {
              edited.current.delete(field.name)
            },
            onChange: () => {
              edited.current.add(field.name)
            },
            onBlur: (e: FocusEvent<HTMLInputElement | HTMLTextAreaElement>) => {
              const next = e.target.value
              // Nothing typed since this box was focused: nothing to write,
              // whatever the box happens to show.
              if (!edited.current.has(field.name)) return
              edited.current.delete(field.name)
              // Edited back to what the row already holds: a no-op. Every
              // column can be compared this way since O92's readable half
              // landed; `exclude_globs` was the one that could not, and the
              // `undefined` arm that carried it is gone with it.
              if (next === field.from(root)) return
              save(field, next)
            },
            'aria-label': field.label,
            'aria-describedby':
              field.hint === undefined
                ? undefined
                : `${fieldIds}-${field.name}-hint`,
          }
          return field.multiline ? (
            <label key={field.name} className="field">
              <span>{field.label}</span>
              <textarea rows={3} {...boxProps} />
              {field.hint !== undefined && (
                <small id={`${fieldIds}-${field.name}-hint`}>
                  {field.hint}
                </small>
              )}
            </label>
          ) : (
            <label key={field.name} className="field">
              <span>{field.label}</span>
              <input type="text" {...boxProps} />
              {field.hint !== undefined && (
                <small id={`${fieldIds}-${field.name}-hint`}>
                  {field.hint}
                </small>
              )}
            </label>
          )
        })}
        <label className="field">
          <span>Scan interval (seconds)</span>
          <input
            type="number"
            name="scan_interval_secs"
            aria-label="Scan interval (seconds)"
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
      </div>

      <div className="checks">
        <label>
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
                  setFailure(sentenceOf(err))
                })
            }}
          />
          <span>Keeps documents under .taiyaku/</span>
        </label>
      </div>

      <div className="actions">
        {/*
          `confirming` disables this too, and that is not tidiness. The
          removal dialog asks its dry run once, when it opens, so a scan
          started here while it is open imports works underneath a sentence
          still quoting the old counts, and the confirm below it then sends a
          real DELETE into a running derive, which is the interleaving
          `DeriveReport::root_removed_mid_derive` counts.
        */}
        <button
          type="button"
          onClick={begin}
          disabled={running || starting || confirming || held}
        >
          Scan now
        </button>
        <button
          type="button"
          onClick={() => setConfirming(true)}
          disabled={running || starting || confirming || held}
        >
          Remove…
        </button>
      </div>

      {running && (
        <p className="scanning">
          Scanning…{elapsed === undefined ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}
      {report !== null && <ScanReport outcome={report} level={5} />}
      {confirming && (
        <RemoveRoot
          root={root}
          lastOf={lastOf}
          held={held}
          onCancel={() => setConfirming(false)}
          onRemoved={(answer) => {
            setConfirming(false)
            onRemoved?.(answer)
            onChanged()
          }}
        />
      )}
    </li>
  )
}
