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
 * requests; a row that only ever polled after pressing its own button would
 * show nothing at all to an admin who navigated away during a scan and came
 * back — no progress, no eventual report, and an enabled "Scan directory"
 * whose only answer would be a 409 about the scan they had started themselves.
 * The cost is one request per root per mount, against an in-memory registry
 * lookup plus one indexed row — far below the dry-run counts, which stay lazy
 * because each of those is a real derivation.
 *
 * **A field is patched on blur, and only the field the admin actually
 * edited** — an absent key means "leave this column alone" to
 * `patch_nullable_text`, so sending the whole row on every edit would rewrite
 * five columns to answer a question about one. **An emptied box clears the
 * column to `null`, not to `''`**: the schema's nullable columns mean "nothing
 * set", and an empty string is a value nothing in the store ever means.
 *
 * **"Edited" is an `input` event, not a value comparison.** The obvious rule —
 * write when the box differs from what the row holds — would fail for a column
 * whose value the row does not report: `from` would answer `''` forever, and
 * an admin who set a pattern then emptied the box would hit `'' === ''` and
 * write nothing. Comparing against the last value *this row* had sent would
 * fix that only within a single mount, since that state starts empty on every
 * load. Treating the column's value as *unknown* instead would remove the
 * guard altogether, so a bare focus-and-tab-away would send
 * `{exclude_globs: null}` and silently delete a stored pattern. Whether
 * somebody typed in the box is the fact all three reach for, and it is the one
 * the DOM already reports. An edit is what sends, and the equality guard is
 * the second check rather than the first.
 *
 * **`exclude_globs` is read, patched, and applied (O92).** The
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
  Fragment,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import { Link } from 'react-router'
import { Callout } from './Callout'
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
import { dirPath } from './configAddress'
import { focusLeaves } from './focusLeaves'
import { JOB_WAITS, type OwnRequests } from './deriveHold'
import { HeldButton } from './HeldButton'
import { useDrawn } from './hooks'
import { JOINING_DOT } from './joiningDot'
import {
  type MoveAnswer,
  namesOf,
  type RemovalHold,
  type UnlinkAnswer,
} from './libraries'
import { MoveRoot, type NamedLibrary } from './MoveRoot'
import { invalidate } from './reads'
import { Refusal } from './Refusal'
import { RemoveRoot } from './RemoveRoot'
import { UnlinkRoot } from './UnlinkRoot'
import { ScanReport } from './ScanReport'
import { messageOf, sentenceOf } from './message'
import { useLineEnds } from './lineEnds'
import { invalidateLibraryViews, ROOTS } from './pageReads'
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
  /**
   * What the page says of the directories around and inside this one, under
   * its facts in the header, since it is as much what the directory is as
   * they are.
   */
  nesting?: ReactNode
  /** The page was opened from a link that is now hidden or gone: the path takes focus. */
  arrived?: boolean
  /** Only a root added in this session has one. See `config.ts`'s `Probe`. */
  probe?: Probe
  /**
   * The library whose page this is. Scan directory names it, so that the
   * scan is filed under it, and Move… and Remove from this library take the
   * directory out of it.
   */
  library?: NamedLibrary
  /**
   * Every library holding the directory, this one included
   * (`LibraryDir.libraries`). Where another holds it, the header says which,
   * and Removal offers Remove from this library beside Remove this directory
   * (decision 24): only while another library keeps it is there anything to
   * take it out of one alone.
   */
  libraries?: NamedLibrary[]
  /**
   * A scan of this directory is reserved, as its library's status says before
   * the directory's own does: the server refuses every action here, so Scan
   * directory, Move…, Remove from this library and Remove… do nothing then
   * and are drawn held, and an open removal, unlink or move dialog is closed
   * unless it draws a refusal, which the person reads and closes.
   */
  held?: boolean
  /**
   * Why the server refuses this one's removal: a scan of a directory around
   * or inside it is reserved, or a derive holds the lease (O51). Remove… is
   * drawn held, its help says which, and an open removal dialog is closed
   * unless it draws a refusal. What the scan does to a move is `moveHeld`'s.
   */
  removalHeld?: RemovalHold
  /**
   * The page's own requests that take the derive lease, which the removal's
   * dry run and DELETE are counted among, so that a read of the status the
   * page sends while either is out is not taken for another derive.
   */
  ownRequests?: OwnRequests
  /**
   * The server refuses every move of this directory: its job lock is held, or
   * a scan of another of the library's directories is reserved. Move… is
   * drawn held and an open move dialog is closed unless it draws a refusal.
   * The server refuses taking it out of this library alone for the same
   * reasons, so this holds Remove from this library and closes its dialog
   * too.
   */
  moveHeld?: boolean
  /**
   * Where this directory could be moved: every library not holding it. The
   * directory's page always gives it, with `library`; a row drawn without
   * both has no Move….
   */
  moveTo?: NamedLibrary[]
  /**
   * The directory is gone, with what its re-derive found. The row is not drawn
   * once the list reloads, so the screen keeps the report.
   */
  onRemoved?: (answer: RemovalAnswer) => void
  /**
   * The directory has left this library alone, with what that did. The row
   * is not drawn once the list reloads, so the screen keeps the report.
   */
  onUnlinked?: (answer: UnlinkAnswer) => void
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
  /**
   * Move…, Remove from this library or Remove… was refused for a hold (a
   * 409) that nothing drawn here said: the page asks its library's status
   * again, so that the hold is drawn, and Remove from this library's
   * refusal goes as the hold ends.
   */
  onHeldRefusal?: () => void
  /** Scan directory was pressed and a scan of this directory is being asked for. */
  onScan?: () => void
  /**
   * What the library's scan status said before now no longer holds: a scan
   * of this directory was asked for, and again as it started or was answered
   * 409, or the mount read took one up, or the poll found it over, or the
   * row went while it ran and so will not see its end.
   */
  onScanMoved?: () => void
  /**
   * A write sent from this row was refused after the row had gone, with the
   * sentence that says so and names the directory: nothing of the row is left
   * to draw it on. An edit sent as the row went is one.
   */
  onLateRefusal?: (sentence: string) => void
  /**
   * An Also in link was followed to `to`, another library's page of this
   * directory: the link goes with this page, so that page's heading takes
   * focus, as a page opened from a library's table does.
   */
  onOpen?: (to: string) => void
}

/**
 * The four text columns this row edits.
 *
 * **Each carries its own `patch` builder rather than a `keyof RootPatch` and a
 * computed key.** `save({ [field.name]: value })` types as
 * `{ [x: string]: string | null }`, which is not a `RootPatch` — TypeScript
 * widens a computed key even when the key's type is a union of literals — so
 * the alternative was a cast, and a cast here would silently accept a field
 * name `RootPatch` does not have. Four one-line builders cost less than one
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
   * **Every column answers, including `exclude_globs`**: `root_json` carries
   * it, so an edit back to the stored string writes nothing here exactly as it
   * does for the other three (O92).
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
   * correctness makes invisible.** With the `ignore` crate, `**\/extras/**`
   * does not exclude the `extras` directory it names, only files under it — so
   * a scan descends into every `extras`, records one as a scan entry, and
   * reports "Directories excluded: 0", which reads to an admin as "my pattern
   * did nothing" when gitignore did exactly what was asked. The label already
   * says what the syntax is; this says what one shape of it does that has no
   * equivalent trap on the other three fields, and it is silent about
   * validation on purpose — the server rejects a dangling `\` and an unclosed
   * `{` and accepts everything else, and this line must not imply otherwise.
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
    // The label says what the syntax is, because gitignore's is not what
    // "globs" alone implies — `!` negates, a leading `/` anchors, a trailing
    // `/` means the directory. It does *not* claim the pattern is checked: the
    // server rejects a dangling `\` and an unclosed `{` and accepts everything
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
 * The least Every may be, in seconds. The server refuses less with a 400
 * (`taiyaku_library::schedule::MIN_INTERVAL_SECS`), and a page holds back
 * only what the server refuses (overlapping-libraries decision 22), so the
 * two have to agree.
 */
const MIN_EVERY_SECS = 300

/**
 * What choosing Every saves where no interval is kept for it: a day, drawn
 * as 24 hours for the admin to change. Saved at once, as every choice is, so
 * that the radio never reads Every while the store holds another choice.
 */
const DEFAULT_EVERY_SECS = 86400

/** Seconds in each of Every's units. */
const UNIT_SECS = { minutes: 60, hours: 3600 } as const
type Unit = keyof typeof UNIT_SECS

/** The Scan schedule's choices, as its radios' values. */
type Choice = 'automatic' | 'every' | 'off'

/** The Scan schedule's label, which names its refusals once the row has gone. */
const SCHEDULE = 'Scan schedule'

/** Every's count box, by its name, which is its key in `edited`. */
const EVERY_BOX = 'scan_every'

/** The one sentence a count the page will not send gets. */
const BAD_EVERY = `Every has to be a whole number of minutes or hours, at least ${MIN_EVERY_SECS / 60} minutes.`

/**
 * The choice a stored interval is drawn as: null Automatic, 0 or less Off,
 * as the scheduler reads them (`schedule::every`), and any other value
 * Every, one under the minimum included, as it is stored.
 */
function choiceOf(stored: number | null): Choice {
  if (stored === null) return 'automatic'
  return stored <= 0 ? 'off' : 'every'
}

/**
 * The unit Every's count is drawn in: hours where the stored interval is a
 * whole number of them, and minutes otherwise, as for a schedule that is
 * not Every, whose count starts empty.
 */
function unitOf(stored: number | null): Unit {
  return stored !== null && stored > 0 && stored % UNIT_SECS.hours === 0
    ? 'hours'
    : 'minutes'
}

/** Every's count as drawn: the stored interval in its unit, rounded down. */
function countOf(stored: number | null): string {
  if (choiceOf(stored) !== 'every') return ''
  return String(Math.floor((stored as number) / UNIT_SECS[unitOf(stored)]))
}

/**
 * The seconds Every's count asks for in `unit`: nothing for an empty box, or
 * for one at `stored` where that is at least the minimum, and `BAD_EVERY` for
 * an entry that is not a whole number, that the box cannot read, or that is
 * under the minimum.
 */
function everySecs(
  stored: number | null,
  box: HTMLInputElement,
  unit: Unit,
): number | typeof BAD_EVERY | undefined {
  // `<input type=number>` reads an unparseable entry such as `12e` as `''`,
  // the same as an emptied box, which sends nothing. `badInput` is the only
  // thing that tells them apart.
  if (box.validity.badInput) return BAD_EVERY
  if (box.value === '') return undefined
  const count = Number(box.value)
  const secs = count * UNIT_SECS[unit]
  if (!Number.isInteger(count) || secs < MIN_EVERY_SECS) return BAD_EVERY
  if (secs === stored) return undefined
  return secs
}

/**
 * What a box left holding its entry asks of `row`, refusals sending nothing.
 * `unit` is Every's unit as last drawn, and `stored` the interval as the row
 * knows it (`knownInterval`), for its count.
 */
function leftPatch(
  row: Root,
  box: HTMLInputElement | HTMLTextAreaElement,
  unit: Unit,
  stored: number | null,
): RootPatch | undefined {
  const field = TEXT_FIELDS.find((one) => one.name === box.name)
  if (field !== undefined) return textPatch(field, row, box.value)
  const secs = everySecs(stored, box as HTMLInputElement, unit)
  return typeof secs === 'number' ? { scan_interval_secs: secs } : undefined
}

/** A Scan schedule write a row has sent, by the interval it sent. */
type SentSchedule = { secs: number | null }

/**
 * The interval as a row knows it: its own latest write that no read of the
 * row has brought back yet, or the row's.
 */
function knownInterval(
  sent: readonly SentSchedule[],
  row: Root,
): number | null {
  const last = sent.at(-1)
  return last === undefined ? row.scan_interval_secs : last.secs
}

/**
 * Write `patch` to directory `id`. A write of its scan interval waits for
 * each one before it in `writes` to be answered, and joins them.
 */
function writeRoot(
  writes: { current: Promise<void> | null },
  id: number,
  patch: RootPatch,
): Promise<void> {
  const write = () => patchRoot(id, patch)
  if (!('scan_interval_secs' in patch)) return write()
  // With none waiting it goes at once, as every other write does.
  const sent = writes.current === null ? write() : writes.current.then(write)
  const answered = sent.then(
    () => undefined,
    () => undefined,
  )
  writes.current = answered
  void answered.then(() => {
    if (writes.current === answered) writes.current = null
  })
  return sent
}

/**
 * The setting a box left holding its entry is named for in a refusal: its
 * own label, and the Scan schedule for Every's count, whose `aria-label`
 * names the count alone.
 */
const leftLabel = (box: HTMLInputElement | HTMLTextAreaElement) =>
  box.name === EVERY_BOX
    ? SCHEDULE
    : (box.getAttribute('aria-label') ?? box.name)

export function RootRow({
  root,
  back,
  notices,
  nesting,
  arrived = false,
  probe,
  library,
  libraries = [],
  held = false,
  removalHeld,
  ownRequests,
  moveHeld = false,
  moveTo,
  onRemoved,
  onUnlinked,
  onMoved,
  onChanged,
  onScan,
  onScanMoved,
  onHeldRefusal,
  onLateRefusal,
  onOpen,
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
  const [unlinking, setUnlinking] = useState(false)
  /**
   * Why taking the directory out of this library was refused: drawn over the
   * row of the button that asked, and cleared by the next press of any of the
   * page's actions, by focus leaving that button, or by the end of a hold of
   * it.
   */
  const [unlinkRefusal, setUnlinkRefusal] = useState<string | undefined>()
  const [moving, setMoving] = useState(false)
  const [running, setRunning] = useState(false)
  /** `running` as last drawn, for a refusal answered after that draw. */
  const watchesScan = useRef(running)
  watchesScan.current = running
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | undefined>()
  /** The scan watched waits for the job lock or another derive's lease (O51). */
  const [waiting, setWaiting] = useState(false)
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
  const unlinkButton = useRef<HTMLButtonElement>(null)
  /**
   * Whether focus is in Remove from this library or its dialog: only focus
   * there falls to the page when a list read takes them away, and only
   * that is handed on to Remove….
   */
  const focusInUnlink = useRef(false)
  const unlinkFocus = {
    onFocus: () => {
      focusInUnlink.current = true
    },
    onBlur: (e: FocusEvent<Element>) => {
      if (focusLeaves(e)) focusInUnlink.current = false
    },
  }
  /** `backToRemove`, for Remove from this library and its dialog. */
  const backToUnlink = useRef(false)
  useEffect(() => {
    if (unlinking || !backToUnlink.current) return
    backToUnlink.current = false
    const at = document.activeElement
    if (at === null || at === document.body) unlinkButton.current?.focus()
  }, [unlinking])
  /**
   * Each box the admin has typed in since focusing it, by its name, Every's
   * count among them.
   *
   * **A write happens because somebody edited a box, never because its value
   * differs from something.** Deciding it by comparison would go wrong in
   * opposite directions: comparing against the row would leave a column whose
   * stored value the row does not report unclearable, and treating the value
   * as *unknown* instead would let a bare focus-and-tab-away send
   * `{exclude_globs: null}` and silently delete a stored pattern. Editing is
   * the fact both reach for, and an `input` event is what says it happened.
   * This guard runs first; the equality check against the row is the second.
   */
  const edited = useRef<Map<string, HTMLInputElement | HTMLTextAreaElement>>(
    new Map(),
  )
  /** The row as last drawn, for an edit sent once the row has gone. */
  const latest = useRef(root)
  latest.current = root
  /**
   * The Scan schedule's writes not yet answered, each sent once the one
   * before it is (`writeRoot`).
   *
   * **In turn, so that the choice made last is the one stored.** A press on
   * Automatic or Off leaves Every's box first, which sends its count, and
   * the choice then goes as a second write of the same column; sent
   * together, the server may store them in either order.
   */
  const intervalWrites = useRef<Promise<void> | null>(null)
  /**
   * The checkbox's own value, because it has to be able to go back.
   *
   * **Controlled, where the four text boxes are not.** An uncontrolled
   * checkbox cannot be reset by a reloaded prop, so a refused `PATCH` would
   * leave it asserting the opposite of what migration 012 actually holds for
   * the root — and clicking it again would send the value the server already
   * has, so the only way back would be two more clicks. A text box at least
   * keeps the admin's typed text as a retry target; a tick reads as settled.
   */
  const [keepsDocuments, setKeepsDocuments] = useState(root.keeps_documents)
  /**
   * The Scan schedule's choice and Every's unit.
   *
   * **State, not the row's value read afresh**, because a choice is drawn
   * as it is sent, before the row holds it, and a count as it is typed.
   * They follow the row's changes from elsewhere (`sentSchedules`), and a
   * refused choice goes back to `knownInterval`. A refused count and unit
   * stay where the store holds Every: a typed entry kept for a retry, as a
   * text box's is, whose unit cannot go back alone without the two saying
   * another interval.
   */
  const [choice, setChoice] = useState(() => choiceOf(root.scan_interval_secs))
  const [unit, setUnit] = useState(() => unitOf(root.scan_interval_secs))
  /**
   * Every's count as its box holds it, which the unit's words agree with.
   * The box is not controlled by it: it starts the box, which is drawn
   * afresh, by `boxDraw`, whenever the schedule is drawn from the row.
   */
  const [count, setCount] = useState(() => countOf(root.scan_interval_secs))
  const [boxDraw, setBoxDraw] = useState(0)
  /** The choice and unit as last drawn, for an answer or a going that comes after. */
  const choiceNow = useRef(choice)
  choiceNow.current = choice
  const unitNow = useRef(unit)
  unitNow.current = unit
  /** Every's count box, which its unit's change sends. */
  const everyBox = useRef<HTMLInputElement>(null)
  /** The schedule as the row draws `stored`, Every's count in a box afresh. */
  const drawSchedule = useCallback((stored: number | null) => {
    setChoice(choiceOf(stored))
    setUnit(unitOf(stored))
    setCount(countOf(stored))
    setBoxDraw((n) => n + 1)
  }, [])
  /**
   * This row's Scan schedule writes that no read of the row has brought
   * back, oldest first; a refused one leaves at its refusal.
   *
   * **A read that brings one back is this row's own write coming home**,
   * not a change from elsewhere, so the schedule is not drawn from it: the
   * choice on the page is that write's, or one made since, such as Every
   * with a count typed and not yet sent, which drawing Off over it would
   * drop.
   */
  const sentSchedules = useRef<SentSchedule[]>([])
  /**
   * The last Every interval the row has read or this row has sent, kept
   * while another choice is drawn, so that choosing Every again saves it
   * again at once. Arrow keys save each radio they land on, so an Every
   * drawn empty and unsent would read Every while the store holds the
   * choice passed through. One under the minimum is kept as the minimum,
   * which the scheduler scans it at (`schedule::every`): the server
   * refuses less.
   */
  const everyKept = useRef<number | undefined>(undefined)
  /** The last Every interval the store took, read or answered, as kept. */
  const everyStored = useRef<number | undefined>(undefined)

  // **Follow the row when it changes underneath.** The state is seeded once,
  // so a value changed from anywhere but this row — another admin, another
  // tab — would leave the tick asserting the opposite of what migration 012
  // holds, and the next click would then send the value the server already
  // has, taking two clicks to get back. It is the same shape as the
  // refused-`PATCH` case in the state's doc comment above.
  useEffect(() => {
    setKeepsDocuments(root.keeps_documents)
  }, [root.keeps_documents])
  // The same for the Scan schedule, unless the read brings back this row's
  // own write (`sentSchedules`), which also settles each written before it.
  // Every's count is drawn afresh with it, so that the count and its unit
  // always say one interval. A count typed in the box this replaces or
  // takes away is no longer on the page, so it is not sent as the row goes.
  useEffect(() => {
    const stored = root.scan_interval_secs
    if (choiceOf(stored) === 'every') {
      everyStored.current = Math.max(stored as number, MIN_EVERY_SECS)
      everyKept.current = everyStored.current
    }
    const sent = sentSchedules.current
    const back = sent.findLastIndex((one) => one.secs === stored)
    if (back >= 0) {
      sent.splice(0, back + 1)
      return
    }
    sent.length = 0
    drawSchedule(stored)
    edited.current.delete(EVERY_BOX)
  }, [root.scan_interval_secs, drawSchedule])

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
  // Held as `changed` is, for the mount read, the poll and its cleanup, and
  // read by a start that answers once the row has gone.
  const moved = useRef(onScanMoved)
  moved.current = onScanMoved
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
   * **A single failure must not abandon a live scan.** A `.catch` that ran
   * `setRunning(false)` would tear the interval down for good — so one blip
   * (a proxy 502, a `SQLITE_BUSY` while the scan's own writes hold the file)
   * would produce no progress, no eventual report, and a re-enabled "Scan
   * directory" whose only answer is a 409 about the scan still running:
   * exactly the state the mount read above exists to prevent, reached by
   * another door. Three is chosen, not measured — long enough to ride out a
   * blip at one second apiece, short enough that a genuinely broken route says
   * so rather than polling for ever.
   */
  const misses = useRef(0)

  /**
   * `label` names the setting in a refusal that comes once the row has gone,
   * `refused` puts back a control that drew the refused value, and `saved`
   * hears that the store took it.
   */
  const savePatch = (
    patch: RootPatch,
    label: string,
    refused?: () => void,
    saved?: () => void,
  ) => {
    writeRoot(intervalWrites, root.id, patch)
      .then(() => {
        saved?.()
        setFailure(undefined)
        // The roots list carries a directory's settings, so the one Rules
        // keeps is out of date.
        void invalidate(ROOTS)
        onChanged()
      })
      .catch((err: unknown) => {
        refused?.()
        const sentence = sentenceOf(err)
        if (drawn.current) setFailure(sentence)
        else
          onLateRefusal?.(
            `${label} for ${root.path} was not saved (${sentence}).`,
          )
      })
  }

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
      last_scan_status: string | null
    }) => {
      setElapsed(status.elapsed_secs)
      // Only the server's own scan writes `waiting`, and a start rewrites one
      // a stop left, so a running scan's status reads it while the scan waits,
      // and past its wait only where the store refused the write that ended it.
      setWaiting(status.running && status.last_scan_status === 'waiting')
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
  // polled by the effect below exactly as one this row started would be.
  useEffect(() => {
    let cancelled = false
    scanStatus(root.id)
      .then((status) => {
        if (cancelled) return
        // Only a *running* scan is adopted on mount. A finished one belongs to
        // whatever session started it, and drawing its report unasked would
        // put a stale wall of counts on the page each time it is opened.
        if (status.running) {
          absorb(status)
          moved.current?.()
        }
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
        const patch = leftPatch(
          row,
          box,
          unitNow.current,
          knownInterval(sentSchedules.current, row),
        )
        if (patch === undefined) continue
        const label = leftLabel(box)
        writeRoot(intervalWrites, row.id, patch).then(
          () => {
            void invalidate(ROOTS)
            changed.current()
          },
          (err: unknown) => {
            late.current?.(
              `${label} for ${row.path} was not saved (${sentenceOf(err)}).`,
            )
          },
        )
      }
    }
  }, [])

  /**
   * What the watch of a scan starts from: no report or notice of an earlier
   * one, and no failed polls counted against it, so that one blip does not
   * end it. A press of Scan directory starts one, and so does a scan taken
   * up after a refusal.
   */
  const watchAfresh = useCallback(() => {
    setScanNotice(undefined)
    setReport(null)
    setElapsed(undefined)
    setWaiting(false)
    misses.current = 0
  }, [])

  const begin = useCallback(() => {
    // Guarded synchronously, not by the hold alone: that would let two rapid
    // presses both send, and the second's 409 — about the admin's own scan —
    // would land in the alert slot while that scan ran fine.
    if (inFlight.current) return
    inFlight.current = true
    onScan?.()
    setStarting(true)
    setFailure(undefined)
    setUnlinkRefusal(undefined)
    setConflict(undefined)
    watchAfresh()
    // At the press, so a return while the start is out waits for a read sent
    // after it; and again at the answer, for a read out across the start,
    // which the server may have answered before it registered the scan.
    moved.current?.()
    startScan(root.id, library?.id)
      .then(() => {
        inFlight.current = false
        setStarting(false)
        setRunning(true)
        moved.current?.()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setStarting(false)
        const why = messageOf(err)
        // A 409 is proof of a scan in flight, whether or not the row is
        // still drawn to watch it.
        if (statusOf(why) === 409) moved.current?.()
        if (!drawn.current) {
          onLateRefusal?.(
            `A scan of ${root.path} did not start (${sentenceOf(err)}).`,
          )
          return
        }
        if (statusOf(why) === 409) setConflict(sentenceOf(err))
        else setFailure(sentenceOf(err))
        // **A 409 is positive proof a scan is in flight**, so it starts the
        // poll rather than only complaining. Without this the row would stay
        // blind after any route back to a re-enabled button — three failed
        // polls, say — showing no progress, no report and no reload when the
        // scan finished, and answering every retry with the same 409: exactly
        // the state `misses` above exists to prevent.
        if (statusOf(why) === 409) {
          misses.current = 0
          setRunning(true)
        }
      })
  }, [
    root.id,
    root.path,
    library?.id,
    onScan,
    onLateRefusal,
    drawn,
    watchAfresh,
  ])

  useEffect(() => {
    if (!running) return
    let cancelled = false
    /** Cleared once the poll stops: the scan has ended, or the polls failed. */
    let watching = true
    const timer = setInterval(() => {
      scanStatus(root.id)
        .then((status) => {
          if (cancelled) return
          misses.current = 0
          absorb(status)
          if (status.running) return
          watching = false
          moved.current?.()
          setConflict(undefined)
          // **A `null` report on a stopped scan is not a clean finish.** The
          // registry is in memory, so a `serve` restarted mid-scan answers
          // exactly this — and writing it through would silently clear
          // "Scanning…" and draw nothing, which reads as success.
          if (status.report === null) {
            setScanNotice(
              'This server has no record of that scan. It was most likely ' +
                'interrupted by a restart; nothing here says whether it ' +
                'finished. Scan again to be sure.',
            )
          }
          changed.current(true)
          // What the scan found changes what the Library views draw, and a
          // scan the server has no record of may have written some of it
          // before the restart.
          invalidateLibraryViews()
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses.current += 1
          if (misses.current < POLL_MISSES_ALLOWED) return
          watching = false
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
      // The row goes, or turns to another directory, while the scan runs:
      // nothing here will see it end.
      if (watching) moved.current?.()
    }
  }, [running, root.id, absorb])

  /*
   * Scan directory, Move…, Remove from this library and Remove… are held
   * alike, for one set of reasons: by a scan of this directory, and by a
   * press of their own, either of which can come while the button has focus,
   * so it is held by `HeldButton` and keeps it. A press of one of the others
   * holds it by `disabled`: focus is on the button pressed then, and a button
   * that cannot take focus as its hold comes on stays out of the tab order.
   * An open dialog is given the same hold, and cancels by its own rule.
   */
  const scanning = running || held
  /** The other libraries holding the directory, where this page's is known. */
  const alsoIn =
    library === undefined
      ? []
      : libraries.filter((one) => one.id !== library.id)
  const shared = alsoIn.length > 0
  const wasShared = useRef(shared)
  /** Set as the directory stops being shared, for the effect after it. */
  const toRemove = useRef(false)
  // A list read that finds no other library holding the directory takes
  // Remove from this library away, and its dialog and refusal with it: an
  // open dialog left in state would hold the other actions with nothing
  // drawn to cancel it. Only as the directory stops being shared, so that
  // an unshared page's first draw takes no focus below.
  useEffect(() => {
    const was = wasShared.current
    wasShared.current = shared
    if (shared || !was) return
    toRemove.current = focusInUnlink.current
    focusInUnlink.current = false
    setUnlinking(false)
    setUnlinkRefusal(undefined)
  }, [shared])
  // Focus in what went falls to the page, and goes to Remove…, the removal
  // that is left, once the dialog's going has freed it: while the dialog is
  // open it is `disabled`, which takes no focus.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `shared` is a trigger, not a closure read: with no dialog open, `unlinking` does not change as the directory stops being shared, and the button that had focus goes all the same.
  useEffect(() => {
    if (unlinking || !toRemove.current) return
    toRemove.current = false
    const at = document.activeElement
    if (at === null || at === document.body) removeButton.current?.focus()
  }, [unlinking, shared])
  /** What holds Remove from this library, as it holds Move…, but its own press. */
  const unlinkHeld = scanning || moveHeld
  /*
   * A hold from the page says why it holds, beside what it holds (O250):
   * Move…'s in a line under the header, the removals' in their rows' help.
   * Not while this directory's own scan is drawn, whose "Scanning…" says it.
   */
  const moveWaits = moveHeld && !running
  const removalWaits = running ? undefined : removalHeld
  /** The Move… line's, which describes Move… while it is drawn. */
  const moveWaitsId = useId()
  const wasUnlinkHeld = useRef(unlinkHeld)
  // A refusal drawn over Remove from this library goes as the hold that
  // refused it ends: it would say what is no longer so, over a button that
  // can be pressed again, and take the press.
  useEffect(() => {
    if (wasUnlinkHeld.current && !unlinkHeld) setUnlinkRefusal(undefined)
    wasUnlinkHeld.current = unlinkHeld
  }, [unlinkHeld])
  /**
   * A dialog was refused for a hold (a 409), proof of something holding, as
   * Scan directory's 409 is. The page reads its library's status so that
   * the hold is drawn and its end seen. That status leaves a scan of this
   * directory alone to this row (`DirPage`'s `useOtherScan`), so the row
   * reads its own once and takes up a scan it finds running, as the mount
   * read does: "Scanning…", and every action held. A refusal for any other
   * hold finds none, and leaves the row as it is.
   */
  const heldRefusal = () => {
    moved.current?.()
    onHeldRefusal?.()
    // Not while the row watches its scan: the poll reads the same, and this
    // read answered after the poll saw the scan end would take it up again.
    if (watchesScan.current) return
    scanStatus(root.id)
      .then((status) => {
        if (!status.running) return
        watchAfresh()
        absorb(status)
      })
      .catch(() => {
        // Silent, as the mount read is: the refusal has said what failed.
      })
  }
  /** Each press of the page's actions clears the refusal of an earlier one. */
  const pressed = (then: () => void) => () => {
    setUnlinkRefusal(undefined)
    then()
  }

  /**
   * Choose a schedule, saved at once: Automatic, Off, and Every at the
   * interval kept for it, or `DEFAULT_EVERY_SECS` where none is kept.
   */
  const chooseSchedule = (next: Choice) => {
    const secs =
      next === 'every'
        ? (everyKept.current ?? DEFAULT_EVERY_SECS)
        : next === 'off'
          ? 0
          : null
    if (next === 'every') {
      // In its unit, so that the count the box starts with and the unit
      // beside it say the same interval.
      setUnit(unitOf(secs))
      setCount(countOf(secs))
    }
    // A count typed in and not left goes with Every's box: sent as the row
    // goes, it would undo this choice.
    edited.current.delete(EVERY_BOX)
    setChoice(next)
    saveSchedule(secs, () => {
      // A radio left on a refused choice says the store holds what it does
      // not, and a press on it sends nothing, since a checked radio fires no
      // change; so it goes back, as the checkbox does. Not over a choice
      // made since, which has its own answer coming. A count typed in the box
      // this takes away goes with it, as a read's does.
      if (choiceNow.current === next) {
        edited.current.delete(EVERY_BOX)
        drawSchedule(knownInterval(sentSchedules.current, latest.current))
      }
    })
  }

  /**
   * Save the Scan schedule as `secs`, as a write a read may bring back
   * (`sentSchedules`); `refused` runs once a refused one has left it.
   */
  const saveSchedule = (secs: number | null, refused?: () => void) => {
    const write = { secs }
    sentSchedules.current.push(write)
    // Kept as it is sent, for a choice of Every before it is answered; a
    // refused one was never stored, so the last the store took is kept, not
    // the one kept when this was sent, which may have been refused too.
    const every = choiceOf(secs) === 'every'
    if (every) everyKept.current = secs as number
    savePatch(
      { scan_interval_secs: secs },
      SCHEDULE,
      () => {
        const sent = sentSchedules.current
        const at = sent.indexOf(write)
        if (at >= 0) sent.splice(at, 1)
        if (everyKept.current === secs) everyKept.current = everyStored.current
        refused?.()
      },
      () => {
        if (every) everyStored.current = secs as number
      },
    )
  }

  /** Send Every's count in `inUnit`, or say why it is not sent. */
  const sendEvery = (box: HTMLInputElement, inUnit: Unit) => {
    const secs = everySecs(
      knownInterval(sentSchedules.current, root),
      box,
      inUnit,
    )
    if (typeof secs === 'string') {
      setFailure(secs)
      return
    }
    if (secs === undefined) return
    setFailure(undefined)
    saveSchedule(secs, () => {
      // Kept for a retry only where the store holds Every: otherwise the
      // radio would read Every against the choice the store holds.
      const known = knownInterval(sentSchedules.current, latest.current)
      if (choiceOf(known) !== 'every') {
        edited.current.delete(EVERY_BOX)
        drawSchedule(known)
      }
    })
  }

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
          <>
            <dl ref={factEnds} className="regime">
              {regimeFacts(root, probe).map(({ label, value }) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd>{value}</dd>
                </div>
              ))}
            </dl>
            {alsoIn.length > 0 && (
              <p className="also-in">
                Also in{' '}
                {alsoIn.map((one, i) => {
                  const to = dirPath(one.name, root.id)
                  return (
                    <Fragment key={one.id}>
                      {i === 0 ? '' : i === alsoIn.length - 1 ? ' and ' : ', '}
                      <Link to={to} onClick={() => onOpen?.(to)}>
                        {one.name}
                      </Link>
                    </Fragment>
                  )
                })}
                {`, which ${alsoIn.length === 1 ? 'shares' : 'share'} these settings and scans.`}
              </p>
            )}
            {nesting}
          </>
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
              // `unlinking` holds it as `moving` does: the server refuses an
              // unlink while a scan is reserved in this library, so a scan
              // started under its dialog would have that confirm refused.
              disabled={confirming || unlinking || moving}
            >
              Scan directory
            </HeldButton>
            {moveTo !== undefined && library !== undefined && (
              <HeldButton
                ref={moveButton}
                type="button"
                onClick={pressed(() => setMoving(true))}
                held={scanning || moveHeld || moving}
                disabled={starting || confirming || unlinking}
                aria-describedby={moveWaits ? moveWaitsId : undefined}
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
          {waiting && `${JOINING_DOT}waiting`}
        </p>
      )}
      {moveWaits && moveTo !== undefined && library !== undefined && (
        <p id={moveWaitsId} className="move-waits">
          A scan or another library job is running; move this directory once it
          has finished.
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
      {moving && moveTo !== undefined && library !== undefined && (
        <MoveRoot
          root={root}
          from={library}
          targets={moveTo}
          held={scanning || moveHeld}
          onCancel={() => {
            backToMove.current = true
            setMoving(false)
          }}
          onMoved={(answer) => {
            setMoving(false)
            onMoved?.(answer)
            onChanged()
          }}
          // The dialog draws the refusal itself.
          onRefused={(_, status) => {
            if (status === 409) heldRefusal()
          }}
          onLateRefusal={onLateRefusal}
        />
      )}

      <Section title="How it is scanned">
        <Row
          label={SCHEDULE}
          help={`Every is at least ${MIN_EVERY_SECS / 60} minutes. With Off, the server scans it only when Scan directory or Scan library is pressed.`}
          group="radiogroup"
        >
          {(ids) => (
            <div className="settings-choices">
              <label>
                <input
                  type="radio"
                  name={`scan_schedule-${root.id}`}
                  value="automatic"
                  checked={choice === 'automatic'}
                  onChange={() => chooseSchedule('automatic')}
                />
                <span>Automatic</span>
              </label>
              {/* The count and its unit follow Every's label and are not in
                  it: a control inside a label is that label's too. */}
              <div className="scan-every">
                <label>
                  <input
                    type="radio"
                    name={`scan_schedule-${root.id}`}
                    value="every"
                    checked={choice === 'every'}
                    onChange={() => chooseSchedule('every')}
                  />
                  <span>Every</span>
                </label>
                {choice === 'every' && (
                  <>
                    <input
                      key={boxDraw}
                      ref={everyBox}
                      type="number"
                      name={EVERY_BOX}
                      aria-label="Scan every"
                      aria-describedby={ids.help}
                      min="1"
                      step="1"
                      defaultValue={count}
                      onFocus={() => {
                        edited.current.delete(EVERY_BOX)
                      }}
                      onChange={(e) => {
                        edited.current.set(EVERY_BOX, e.target)
                        setCount(e.target.value)
                      }}
                      onBlur={(e) => {
                        // Nothing typed since this box was focused: nothing
                        // to send, though its count may be the stored
                        // interval rounded down and so not equal to it.
                        if (!edited.current.has(EVERY_BOX)) return
                        edited.current.delete(EVERY_BOX)
                        sendEvery(e.target, unit)
                      }}
                    />
                    <select
                      name="scan_every_unit"
                      aria-label="Scan every unit"
                      aria-describedby={ids.help}
                      value={unit}
                      onChange={(e) => {
                        const next = e.target.value as Unit
                        setUnit(next)
                        // Sent with its unit now, so not again as the row
                        // goes.
                        edited.current.delete(EVERY_BOX)
                        if (everyBox.current !== null) {
                          sendEvery(everyBox.current, next)
                        }
                      }}
                    >
                      <option value="minutes">
                        {Number(count) === 1 ? 'minute' : 'minutes'}
                      </option>
                      <option value="hours">
                        {Number(count) === 1 ? 'hour' : 'hours'}
                      </option>
                    </select>
                  </>
                )}
              </div>
              <label>
                <input
                  type="radio"
                  name={`scan_schedule-${root.id}`}
                  value="off"
                  checked={choice === 'off'}
                  onChange={() => chooseSchedule('off')}
                />
                <span>Off</span>
              </label>
            </div>
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
                    void invalidate(ROOTS)
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
        {/* The rows in a group, which draws nothing of its own, so that a
            refusal laid over them is as wide as they are: laid over the box
            itself it would reach into the box's padding. */}
        <div className="settings-group">
          {library !== undefined && alsoIn.length > 0 && (
            <>
              {/* Held exactly when Move… is: the server refuses both for the
                same reasons. */}
              <Row
                label={`Remove from ${library.name}`}
                help={
                  moveWaits
                    ? JOB_WAITS
                    : `${namesOf(alsoIn.map((one) => one.name))} ${alsoIn.length === 1 ? 'keeps' : 'keep'} it; nothing is deleted.`
                }
                action
              >
                {(ids) => (
                  <HeldButton
                    ref={unlinkButton}
                    type="button"
                    onClick={pressed(() => setUnlinking(true))}
                    // Focus leaving it takes its refusal away, as focus
                    // leaving a form takes the form's (`Account`): so that a
                    // press on the refusal, which takes the press, frees the
                    // button.
                    onFocus={unlinkFocus.onFocus}
                    onBlur={(e) => {
                      unlinkFocus.onBlur(e)
                      if (unlinkRefusal !== undefined && focusLeaves(e)) {
                        setUnlinkRefusal(undefined)
                      }
                    }}
                    held={unlinkHeld || unlinking}
                    disabled={starting || moving || confirming}
                    aria-describedby={ids.help}
                  >
                    Remove from {library.name}…
                  </HeldButton>
                )}
              </Row>
            </>
          )}
          <Row
            label="Remove this directory"
            help={
              removalWaits === 'around'
                ? 'Waits for a scan of a directory around or inside this one.'
                : removalWaits === 'derive'
                  ? JOB_WAITS
                  : alsoIn.length > 0
                    ? 'From every library that holds it, with what it would take.'
                    : 'Asks first, with what it would take.'
            }
            action
          >
            {(ids) => (
              <HeldButton
                ref={removeButton}
                type="button"
                onClick={pressed(() => setConfirming(true))}
                held={scanning || removalHeld !== undefined || confirming}
                disabled={starting || moving || unlinking}
                aria-describedby={ids.help}
              >
                Remove…
              </HeldButton>
            )}
          </Row>
          {/* After both rows, though it answers the first: with no row below
            it, a refusal lies over the rows above the last (`Callout`), so
            over Remove from this library's own row, and never over Remove
            this directory, which a refusal on the directory's last library
            names. Laid over the rows, its coming and going moves nothing.
            Over the button alone it would stand out of the box, its words
            wrapped to the button's width. */}
          {shared && unlinkRefusal !== undefined && (
            <Callout>
              {/* A press on it takes it away, wherever focus is: it lies
                  over the button it answers, and would take that press. */}
              <Refusal onPress={() => setUnlinkRefusal(undefined)}>
                {unlinkRefusal}
              </Refusal>
            </Callout>
          )}
        </div>
        {/* Each dialog after both rows, at the page's foot, so that opening
            either moves nothing on the page. */}
        {unlinking && library !== undefined && shared && (
          <UnlinkRoot
            root={root}
            from={library}
            keptBy={alsoIn.map((one) => one.name)}
            held={scanning || moveHeld}
            onCancel={() => {
              backToUnlink.current = true
              setUnlinking(false)
            }}
            onRefused={(sentence, status) => {
              backToUnlink.current = true
              setUnlinking(false)
              setUnlinkRefusal(sentence)
              // The hold's end takes the refusal away.
              if (status === 409) heldRefusal()
            }}
            onFocus={unlinkFocus.onFocus}
            onBlur={unlinkFocus.onBlur}
            onUnlinked={(answer) => {
              setUnlinking(false)
              onUnlinked?.(answer)
              onChanged()
            }}
            onLateRefusal={onLateRefusal}
          />
        )}
        {confirming && (
          <RemoveRoot
            root={root}
            holders={
              alsoIn.length > 0 ? libraries.map((one) => one.name) : undefined
            }
            held={scanning || removalHeld !== undefined}
            own={ownRequests}
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
            // The dialog draws the refusal itself.
            onRefused={(_, status) => {
              if (status === 409) heldRefusal()
            }}
            onLateRefusal={onLateRefusal}
          />
        )}
      </RemovalSection>
    </>
  )
}
