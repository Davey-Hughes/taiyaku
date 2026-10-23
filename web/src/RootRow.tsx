/**
 * One root: what it is, what it defaults to, and what its last scan did —
 * spec C4 §2 and §3. It is the body of the directory's own page.
 *
 * **The row owns its own scan poll**, the way `ProposalRow` owns its own
 * evidence fetch: a scan belongs to the root being scanned rather than to the
 * page that draws it.
 *
 * **It asks the status route once on mount, and that is not optional.** The
 * server keeps `ServeState.scans` precisely so a client can poll a scan across
 * requests; a row that only ever polled after pressing its own button showed
 * nothing at all to an admin who navigated away during a scan and came back —
 * no progress, no eventual report, and an enabled "Scan directory" whose only
 * answer was a 409 about the scan they had started themselves. Whole-branch
 * review, 2026-09-04. The cost is one request per root per mount, against an
 * in-memory registry lookup plus one indexed row — far below the dry-run
 * counts, which stay lazy because each of those is a real derivation.
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
 * (pass 1). Comparing against the last value *this row* had sent fixed that
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
  type ChangeEvent,
  type FocusEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'
import {
  patchRoot,
  POLL_MISSES_ALLOWED,
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
import { HeldButton } from './HeldButton'
import { useDrawn } from './hooks'
import type { MoveAnswer } from './libraries'
import { MoveRoot, type NamedLibrary } from './MoveRoot'
import { RemoveRoot } from './RemoveRoot'
import { ScanReport } from './ScanReport'
import { messageOf, sentenceOf } from './message'
import { useLineEnds } from './lineEnds'
import { PathWithBreaks } from './PathWithBreaks'
import {
  type BackTo,
  PageHeader,
  RemovalSection,
  Row,
  Section,
} from './settingsLayout'

export interface RootRowProps {
  root: Root
  /** The way up to the directory's library, drawn in the page's header. */
  back?: BackTo
  /** What the page says under its header, before this directory's own alerts. */
  notices?: ReactNode
  /** The page was opened from a link that is now hidden or gone: the path takes focus. */
  arrived?: boolean
  /** Only a root added in this session has one. See `config.ts`'s `Probe`. */
  probe?: Probe
  /** The library's name, where this is its only directory. */
  lastOf?: string
  /**
   * Something else in the library is going on: another directory's scan.
   * Scan directory, Move… and Remove… do nothing then and are drawn held,
   * and an open removal or move dialog is closed.
   */
  held?: boolean
  /**
   * Where this directory could be moved. The directory's page always gives it;
   * a row drawn without it has no Move….
   */
  moveTo?: { from: NamedLibrary; targets: NamedLibrary[] }
  /**
   * The directory is gone, with what its re-derive found. The row is not drawn
   * once the list reloads, so the screen keeps the report.
   */
  onRemoved?: (answer: RemovalAnswer) => void
  /**
   * The directory has moved, with what the move did and what deriving after it
   * found. The row is not drawn once the list reloads, so the screen keeps
   * the report.
   */
  onMoved?: (answer: MoveAnswer) => void
  /**
   * The list reloads on a save, a removal, or a finished scan. `quiet` is a
   * scan that ended by itself: no press started that read.
   */
  onChanged: (quiet?: boolean) => void
  /** Scan directory was pressed and a scan of this directory is being asked for. */
  onScan?: () => void
  /**
   * A write sent from this row was refused after the row had gone, with the
   * sentence that says so and names the directory: nothing of the row is left
   * to draw it on. An edit sent as the row went is one.
   */
  onLateRefusal?: (sentence: string) => void
}

/**
 * The four text columns this row edits.
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
  /**
   * The setting's whole name: the box's `aria-label`, and the words a
   * refusal names it by once the page has gone.
   */
  label: string
  /** The row's label: `label`'s first words, which the name must contain. */
  title: string
  /** The section the row is drawn in. */
  group: 'scan' | 'read'
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
   * The row's help, beside the label, drawn only when set.
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
  help?: string
}

const TEXT_FIELDS: TextField[] = [
  {
    name: 'edition_key',
    label: 'Edition key',
    title: 'Edition key',
    group: 'read',
    from: (r) => r.edition_key ?? '',
    patch: (edition_key) => ({ edition_key }),
  },
  {
    name: 'default_kind',
    label: 'Default kind',
    title: 'Default kind',
    group: 'read',
    from: (r) => r.default_kind ?? '',
    patch: (default_kind) => ({ default_kind }),
  },
  {
    name: 'default_language',
    label: 'Default language',
    title: 'Default language',
    group: 'read',
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
    title: 'Exclude globs',
    group: 'scan',
    multiline: true,
    from: (r) => r.exclude_globs ?? '',
    patch: (exclude_globs) => ({ exclude_globs }),
    help:
      'One gitignore pattern per line. A trailing slash or a bare name ' +
      "excludes the directory and all under it; a '/**' suffix excludes " +
      'only what is inside it.',
  },
]

/** The one sentence a bad scan interval gets, in both of its shapes. */
const BAD_INTERVAL =
  'A scan interval has to be a whole number of seconds, at least one. ' +
  'Leave it empty to set no interval at all.'

/**
 * What a text box holding `value` asks of `row`: nothing where the row
 * already holds it, and `null` for an emptied box.
 */
function textPatch(
  field: TextField,
  row: Root,
  value: string,
): RootPatch | undefined {
  if (value === field.from(row)) return undefined
  return field.patch(value === '' ? null : value)
}

/**
 * What the interval box asks of `row`: nothing where the row already holds
 * it, `null` for an emptied box, and `BAD_INTERVAL` for an entry that is not
 * a whole number of seconds, at least one.
 */
function intervalPatch(
  row: Root,
  box: HTMLInputElement,
): RootPatch | typeof BAD_INTERVAL | undefined {
  // `<input type=number>` reads an unparseable entry such as `12e` as `''`,
  // the same as an emptied box, and an emptied box clears the column.
  // `badInput` is the only thing that tells them apart.
  if (box.validity.badInput) return BAD_INTERVAL
  const next = box.value === '' ? null : Number(box.value)
  if (next === row.scan_interval_secs) return undefined
  if (next !== null && (!Number.isInteger(next) || next < 1)) {
    return BAD_INTERVAL
  }
  return { scan_interval_secs: next }
}

/** What a box left holding its entry asks of `row`, refusals sending nothing. */
function leftPatch(
  row: Root,
  box: HTMLInputElement | HTMLTextAreaElement,
): RootPatch | undefined {
  const field = TEXT_FIELDS.find((one) => one.name === box.name)
  if (field !== undefined) return textPatch(field, row, box.value)
  const patch = intervalPatch(row, box as HTMLInputElement)
  return typeof patch === 'string' ? undefined : patch
}

export function RootRow({
  root,
  back,
  notices,
  arrived = false,
  probe,
  lastOf,
  held = false,
  moveTo,
  onRemoved,
  onMoved,
  onChanged,
  onScan,
  onLateRefusal,
}: RootRowProps) {
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
  const [moving, setMoving] = useState(false)
  const [running, setRunning] = useState(false)
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | undefined>()
  const [report, setReport] = useState<ScanOutcome | null>(null)
  const heading = useRef<HTMLHeadingElement>(null)
  /** Which of the facts under the title end a line. */
  const factEnds = useLineEnds<HTMLDListElement>('.regime > div')
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  const moveButton = useRef<HTMLButtonElement>(null)
  /**
   * Set when the move dialog is cancelled, by Cancel or Escape: focus goes
   * back to the Move… that opened it, as a closing dialog's should, and not
   * to the document, which is where the removed dialog leaves it. A hold
   * cancels it too, and focus goes back to Move… all the same, held as it
   * then is (`HeldButton`); a finished move leaves it to what the screen
   * draws next.
   */
  const backToMove = useRef(false)
  useEffect(() => {
    if (moving || !backToMove.current) return
    backToMove.current = false
    moveButton.current?.focus()
  }, [moving])
  const removeButton = useRef<HTMLButtonElement>(null)
  /**
   * Set when the removal's confirm is cancelled, for the reason `backToMove`
   * is: focus goes back to the Remove… that opened it, not to the document,
   * where the confirm's Cancel leaves it as it goes, and to a Remove… that a
   * hold has taken too (`HeldButton`). Only from the document: the confirm
   * is not modal, and a hold that cancels it while focus is in a box
   * elsewhere on the page leaves focus in that box.
   */
  const backToRemove = useRef(false)
  useEffect(() => {
    if (confirming || !backToRemove.current) return
    backToRemove.current = false
    const at = document.activeElement
    if (at === null || at === document.body) removeButton.current?.focus()
  }, [confirming])
  /**
   * Each box the admin has typed in since focusing it, by its name. The
   * interval box is in it too, for the row's going; its blur compares the
   * entry with the row instead.
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
  const edited = useRef<Map<string, HTMLInputElement | HTMLTextAreaElement>>(
    new Map(),
  )
  /** The row as last drawn, for an edit sent once the row has gone. */
  const latest = useRef(root)
  latest.current = root
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
  // so a value changed from anywhere but this row — another admin, another
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
  // Read by the edits sent as the row goes, which answer after it has.
  const late = useRef(onLateRefusal)
  late.current = onLateRefusal
  const drawn = useDrawn()

  /**
   * Whether a `POST` is already on its way.
   *
   * **A ref, not the `starting` state below.** React batches state updates,
   * so two clicks dispatched before the next render both read the *old*
   * hold: the button is still free on the second, and both POSTs go. A ref is
   * written and read synchronously, so the second click sees the first.
   * `starting` still exists because it is what *draws* the held button; this
   * is what enforces it.
   */
  const inFlight = useRef(false)

  /**
   * Consecutive failed polls. Reset by any answer.
   *
   * **A single failure must not abandon a live scan.** The `.catch` used to
   * `setRunning(false)`, which tore the interval down for good — so one blip
   * (a proxy 502, a `SQLITE_BUSY` while the scan's own writes hold the file)
   * produced no progress, no eventual report, and a re-enabled "Scan directory"
   * whose only answer is a 409 about the scan still running: exactly the state
   * the mount read above exists to prevent, reached by another door. Review
   * pass 2, 2026-09-04. Three is chosen, not measured — long enough to ride
   * out a blip at one second apiece, short enough that a genuinely broken
   * route says so rather than polling for ever.
   */
  const misses = useRef(0)

  /** `label` names the setting in a refusal that comes once the row has gone. */
  const savePatch = (patch: RootPatch, label: string) => {
    patchRoot(root.id, patch)
      .then(() => {
        setFailure(undefined)
        onChanged()
      })
      .catch((err: unknown) => {
        const sentence = sentenceOf(err)
        if (drawn.current) setFailure(sentence)
        else
          onLateRefusal?.(
            `${label} for ${root.path} was not saved (${sentence}).`,
          )
      })
  }

  /** Which dialog is open, for `absorb`, which is made once. */
  const open = useRef({ confirming, moving })
  open.current = { confirming, moving }
  /**
   * Fold one status answer into this row's state.
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
        // A removal or a move being confirmed quotes counts from before this
        // scan. The scan closes it as a hold from elsewhere does, and focus
        // goes back to the button that opened it as it does then.
        if (open.current.confirming) backToRemove.current = true
        if (open.current.moving) backToMove.current = true
        setConfirming(false)
        setMoving(false)
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
  // polled by the effect below exactly as one this row started would be.
  useEffect(() => {
    let cancelled = false
    scanStatus(root.id)
      .then((status) => {
        if (cancelled) return
        // Only a *running* scan is adopted on mount. A finished one belongs to
        // whatever session started it, and drawing its report unasked would
        // put a stale wall of counts on the page each time it is opened.
        if (status.running) absorb(status)
      })
      .catch(() => {
        // Deliberately silent. A status read this row did not ask for must
        // not put a red alert over a screen the admin has only just opened;
        // pressing "Scan directory" is what surfaces a real fault.
      })
    return () => {
      cancelled = true
    }
  }, [root.id, absorb])

  // A box still being typed in when the row goes holds an edit nothing has
  // sent, since the browser's Back, among other routes, leaves it no blur.
  // It is sent as its blur would have sent it, against the row as last
  // drawn. Only a box typed in is in `edited`, so a mount and unmount with
  // nothing typed, as StrictMode stages in development, sends nothing. A
  // refusal is handed up, named for the box and the directory, since the page
  // it would be drawn on is gone.
  useEffect(() => {
    const typed = edited.current
    return () => {
      const row = latest.current
      for (const box of typed.values()) {
        const patch = leftPatch(row, box)
        if (patch === undefined) continue
        const label = box.getAttribute('aria-label') ?? box.name
        patchRoot(row.id, patch).then(
          () => changed.current(),
          (err: unknown) => {
            late.current?.(
              `${label} for ${row.path} was not saved (${sentenceOf(err)}).`,
            )
          },
        )
      }
    }
  }, [])

  const begin = useCallback(() => {
    // Guarded synchronously, not by the hold alone: that left two rapid
    // presses both sending, and the second's 409 — about the admin's own scan
    // — landed in the alert slot while that scan ran fine.
    if (inFlight.current) return
    inFlight.current = true
    onScan?.()
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
        if (!drawn.current) {
          onLateRefusal?.(
            `A scan of ${root.path} did not start (${sentenceOf(err)}).`,
          )
          return
        }
        const why = messageOf(err)
        if (statusOf(why) === 409) setConflict(sentenceOf(err))
        else setFailure(sentenceOf(err))
        // **A 409 is positive proof a scan is in flight**, so it starts the
        // poll rather than only complaining. Without this the row stayed
        // blind after any route back to a re-enabled button — three failed
        // polls, say — showing no progress, no report and no reload when the
        // scan finished, and answering every retry with the same 409: exactly
        // the state `misses` above exists to prevent. Pass 3.
        if (statusOf(why) === 409) {
          misses.current = 0
          setRunning(true)
        }
      })
  }, [root.id, root.path, onScan, onLateRefusal, drawn])

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

  /*
   * Scan directory, Move… and Remove… are held alike, for one set of reasons:
   * by a scan, of this directory or another, and by a press of their own,
   * either of which can come while the button has focus, so it is held by
   * `HeldButton` and keeps it. A press of one of the other two holds it by
   * `disabled`: focus is on the button pressed then, and a button that cannot
   * take focus as its hold comes on stays out of the tab order.
   */
  const scanning = running || held

  const textRow = (field: TextField) => (
    <Row key={field.name} label={field.title} help={field.help}>
      {(ids) => {
        const boxProps = {
          id: ids.control,
          name: field.name,
          defaultValue: field.from(root),
          onFocus: () => {
            edited.current.delete(field.name)
          },
          onChange: (
            e: ChangeEvent<HTMLInputElement | HTMLTextAreaElement>,
          ) => {
            edited.current.set(field.name, e.target)
          },
          onBlur: (e: FocusEvent<HTMLInputElement | HTMLTextAreaElement>) => {
            // Nothing typed since this box was focused: nothing to write,
            // whatever the box happens to show.
            if (!edited.current.has(field.name)) return
            edited.current.delete(field.name)
            // Edited back to what the row already holds: a no-op.
            const patch = textPatch(field, root, e.target.value)
            if (patch !== undefined) savePatch(patch, field.label)
          },
          'aria-label': field.label,
          'aria-describedby': ids.help,
        }
        return field.multiline ? (
          <textarea rows={3} {...boxProps} />
        ) : (
          <input type="text" {...boxProps} />
        )
      }}
    </Row>
  )

  return (
    <>
      <PageHeader
        back={back}
        title={
          <code>
            <PathWithBreaks path={root.path} />
          </code>
        }
        titleRef={heading}
        description={
          <dl ref={factEnds} className="regime">
            {regimeFacts(root, probe).map(({ label, value }) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>{value}</dd>
              </div>
            ))}
          </dl>
        }
        actions={
          <>
            {/*
              `confirming` holds this too, and that is not tidiness. The
              removal dialog asks its dry run once, when it opens, so a scan
              started here while it is open imports works underneath a
              sentence still quoting the old counts, and the confirm below it
              then sends a real DELETE into a running derive, which is the
              interleaving `DeriveReport::root_removed_mid_derive` counts.
            */}
            <HeldButton
              type="button"
              onClick={begin}
              held={scanning || starting}
              disabled={confirming || moving}
            >
              Scan directory
            </HeldButton>
            {moveTo !== undefined && (
              <HeldButton
                ref={moveButton}
                type="button"
                onClick={() => setMoving(true)}
                held={scanning || moving}
                disabled={starting || confirming}
              >
                Move…
              </HeldButton>
            )}
          </>
        }
      />
      {running && (
        <p className="scanning">
          Scanning…{elapsed === undefined ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}
      {notices}
      {failure !== undefined && <p role="alert">{failure}</p>}
      {conflict !== undefined && <p role="alert">{conflict}</p>}
      {scanNotice !== undefined && (
        <p role="alert" className="scan-notice">
          {scanNotice}
        </p>
      )}
      {/* A modal, drawn over the page and so taking no place in it. Inside
          the page all the same, which is what gives it the screen's own
          rules and measure. */}
      {moving && moveTo !== undefined && (
        <MoveRoot
          root={root}
          from={moveTo.from}
          targets={moveTo.targets}
          held={held}
          onCancel={() => {
            backToMove.current = true
            setMoving(false)
          }}
          onMoved={(answer) => {
            setMoving(false)
            onMoved?.(answer)
            onChanged()
          }}
          onLateRefusal={onLateRefusal}
        />
      )}

      <Section title="How it is scanned">
        <Row
          label="Scan interval"
          help="Whole seconds, at least one. Empty sets no interval."
        >
          {(ids) => (
            <input
              type="number"
              id={ids.control}
              name="scan_interval_secs"
              aria-label="Scan interval (seconds)"
              aria-describedby={ids.help}
              min="1"
              step="1"
              defaultValue={root.scan_interval_secs ?? ''}
              onChange={(e) => {
                edited.current.set('scan_interval_secs', e.target)
              }}
              onBlur={(e) => {
                edited.current.delete('scan_interval_secs')
                const patch = intervalPatch(root, e.target)
                if (typeof patch === 'string') {
                  setFailure(patch)
                  return
                }
                if (patch === undefined) return
                setFailure(undefined)
                savePatch(patch, 'Scan interval (seconds)')
              }}
            />
          )}
        </Row>
        {TEXT_FIELDS.filter((field) => field.group === 'scan').map(textRow)}
        <Row label="Keeps documents under .taiyaku/" check>
          {(ids) => (
            <input
              type="checkbox"
              id={ids.control}
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
                    // Back to what the server still holds. See the state's
                    // own doc comment for why this box is controlled and the
                    // text ones are not.
                    setKeepsDocuments(!next)
                    const sentence = sentenceOf(err)
                    if (drawn.current) setFailure(sentence)
                    else
                      onLateRefusal?.(
                        `Keeps documents under .taiyaku/ for ${root.path} was not saved (${sentence}).`,
                      )
                  })
              }}
            />
          )}
        </Row>
      </Section>

      <Section title="How it is read">
        {TEXT_FIELDS.filter((field) => field.group === 'read').map(textRow)}
      </Section>

      {report !== null && (
        <Section title="Scan report">
          <ScanReport outcome={report} level={4} />
        </Section>
      )}

      <RemovalSection title="Removal">
        <Row
          label="Remove this directory"
          help="Asks first, with what it would take."
        >
          <HeldButton
            ref={removeButton}
            type="button"
            onClick={() => setConfirming(true)}
            held={scanning || confirming}
            disabled={starting || moving}
          >
            Remove…
          </HeldButton>
        </Row>
        {confirming && (
          <RemoveRoot
            root={root}
            lastOf={lastOf}
            held={held}
            onCancel={() => {
              backToRemove.current = true
              setConfirming(false)
            }}
            onRemoved={(answer) => {
              setConfirming(false)
              // The directory is gone, so an edit still in its box has nowhere
              // to go when the row does. A move keeps its edits: the directory
              // is still there, in another library.
              edited.current.clear()
              onRemoved?.(answer)
              onChanged()
            }}
            onLateRefusal={onLateRefusal}
          />
        )}
      </RemovalSection>
    </>
  )
}
