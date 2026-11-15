/**
 * A correction to one thing already on screen — spec C3 §5.
 *
 * **Contextual, not a metadata section.** There is no route to navigate to:
 * the same editor mounts wherever the thing being corrected already renders
 * — a volume's own title, on the series page — because "the correction is
 * always prompted by looking at something wrong" (§5's own words). This file
 * knows nothing about a route or which caller mounted it; `Series.tsx`
 * decides when one belongs on screen and which PATCH route `onPatch` calls.
 *
 * **Scope: the one field every one of §5's contextual entry points can edit
 * with no data it does not already have.** `metadata.ts`'s `PatchWork`,
 * `patchSeries`, `patchEdition` and `patchMember` also cover volume number,
 * series membership, edition label/kind/language/credits and a member's
 * chapter — none of those are wired to a screen yet. A title correction from
 * a volume's row needs only the slug `library.ts` already hands the page, but
 * the sibling entry point the spec also asks for — a series page correcting
 * its own title — would have to replace `LibraryDoc.series.titles` wholesale
 * (`librarydoc::set_series_titles` in `taiyaku-library`, which `patch_series`
 * calls, replaces a series' whole title set rather than merging into it).
 * `SeriesCard.title` is one display string, the primary title, never the
 * underlying `{ title, kind, primary }` entries, so wiring it up from that
 * would silently drop a series' native/alt titles the moment anyone
 * corrected the romaji one. **The read path exists (O83) and no screen uses
 * it:** `library.ts`'s `oneSeries` answers `SeriesDetail.titles` in the shape
 * `patchSeries` sends, but this component edits one string and hands
 * `onPatch` a `WorkEdit`, so a series page that edits its own titles is
 * still to be built.
 *
 * **`persisted: false` is stated, not hidden, and it is not an alarm.**
 * `metadata.ts`'s own module doc is explicit that this answer is not a
 * failure — the edit is already safe from the next scan, and only the
 * `<stem>.library.json` beside the library did not land (a failed write, a
 * work with no on-disk address, or a document a newer taiyaku wrote there).
 * So the note below renders as plain text, not `role="alert"`, which stays
 * reserved for the one path that really did lose the edit: a thrown
 * 409/404/403 from a hard failure.
 */
import {
  type FormEvent,
  type Ref,
  useEffect,
  useId,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { humanBytes } from './bytes'
import { statusOf } from './config'
import { JOB_WAITS, useDeriveHold, useOwnRequests } from './deriveHold'
import { HeldButton } from './HeldButton'
import { useDrawnWait } from './hooks'
import { type PatchResult, reasons, type WorkEdit } from './metadata'
import {
  type Archive,
  performStamp,
  type Stamped,
  type StampPlan,
  stampPlan,
} from './stamp'
import { messageOf } from './message'
import { PathWithBreaks } from './PathWithBreaks'

/**
 * How many archives, as one phrase. A helper rather than an inline ternary at
 * each place that prints this count, so a singular cannot be right in one of
 * them and wrong in the next.
 */
const archiveCount = (n: number) => `${n} ${n === 1 ? 'archive' : 'archives'}`

/**
 * What the ComicInfo section is doing, as one value.
 *
 * **This exists so that no two conditions in the section can disagree.** A
 * control or status line that stated its own condition over six independent
 * variables (`plan`, `asking`, `planFailure`, `stamped`, `writing`,
 * `stampFailure`) plus two refs would leave each fix moving one condition and
 * the others disagreeing with it. So the combination is computed once, in
 * `phaseOf` below, and everything renders from the answer.
 *
 * **A discriminated union rather than a bare string union.** Five of the eight
 * phases have something to draw — the plan they are about, or the sentence a
 * server sent — and a string would leave every reader reaching back into the
 * raw state for it, which is exactly what the phase avoids. The cost is that
 * the union has to be constructed rather than compared; that is one function.
 */
type StampPhase =
  /** Nothing has been asked. The read control is all there is. */
  | { kind: 'idle' }
  /** A plan read is out, with no answer yet to draw. */
  | { kind: 'reading' }
  /** The last plan read failed. Nothing is on the wire. */
  | { kind: 'read-failed'; message: string }
  /** A plan landed naming no archive to rewrite. */
  | { kind: 'empty'; plan: StampPlan }
  /** A plan landed with archives in it, waiting to be confirmed. */
  | { kind: 'ready'; plan: StampPlan }
  /**
   * A rewrite is on the wire.
   *
   * **`plan` is the plan being written, held apart from the plan slot.** A
   * re-read pressed in the same React batch as the confirm blanks that slot
   * while the POST is still out, and what is being rewritten right now is a
   * fact about the write rather than about the slot: with the slot blanked,
   * the only thing on screen would be "Working out which archives would
   * change…", naming the read.
   */
  | { kind: 'writing'; plan: StampPlan }
  /**
   * A rewrite was refused.
   *
   * `plan` is the plan slot, which is what a retry would send. It is
   * `undefined` only when a re-read in the same batch blanked it, in which
   * case there is nothing to retry and the read control is the way on.
   */
  | { kind: 'refused'; message: string; plan: StampPlan | undefined }
  /** A rewrite finished and nothing has been asked since. */
  | { kind: 'done' }

/**
 * The one derivation. Nothing below this line reads the raw state.
 *
 * **The order is total, over states that are mostly exclusive already**, and
 * the exclusions are mechanical rather than argued: `askPlan` blanks the plan
 * slot and both failures at the press, and `confirmStamp` clears the stamp
 * failure at the press and supersedes any plan read in flight, so a read
 * cannot outlive the stamp that follows it. That leaves `refused` beside a
 * plan slot as the only combination that really occurs, which is why that
 * phase carries one. The chain still names every state: a total order costs
 * a line each where an argument about which combinations occur costs a
 * review pass.
 *
 * **A write in flight is tested first** because a rewrite with no undo
 * outranks anything else this section could be saying.
 *
 * **`plan` is tested before `asking`, deliberately.** The two cannot both be
 * set — `askPlan` blanks the slot at the press — so in correct code the order
 * is unobservable, and the reason for it is what a *broken* version shows.
 * With `askPlan`'s `setPlan(undefined)` deleted, this order fails the test
 * that names the frame — `blanks the refused plan and its confirm control
 * while the re-read is out` — and the other order does not, because a stale
 * plan hides behind "Working out which archives would change…" until the
 * read settles. The deletion is caught either way. What this order buys is
 * the test that describes the frame still being the test that sees it.
 */
const phaseOf = (state: {
  plan: StampPlan | undefined
  asking: boolean
  planFailure: string | undefined
  writing: StampPlan | undefined
  stampFailure: string | undefined
  stamped: Stamped | undefined
}): StampPhase => {
  if (state.writing !== undefined)
    return { kind: 'writing', plan: state.writing }
  if (state.stampFailure !== undefined)
    return { kind: 'refused', message: state.stampFailure, plan: state.plan }
  if (state.planFailure !== undefined)
    return { kind: 'read-failed', message: state.planFailure }
  if (state.plan !== undefined)
    return state.plan.archives.length === 0
      ? { kind: 'empty', plan: state.plan }
      : { kind: 'ready', plan: state.plan }
  if (state.asking) return { kind: 'reading' }
  return state.stamped === undefined ? { kind: 'idle' } : { kind: 'done' }
}

/**
 * A phase this file has not been taught to draw, refused at compile time.
 *
 * **Compiler-enforced coverage is the whole argument for making the phase a
 * discriminated union**, and neither `switch` over it, `phaseBody`'s or
 * `confirmablePlan`'s, gets any without this call. A `switch` whose arms all
 * `return` and which names eight kinds of nine just infers `undefined` into
 * its return type — a legal thing for a renderer to return — and a
 * `default: return undefined` swallows an unnamed kind and answers with the
 * same value it gives five named ones. So a ninth phase would compile at
 * both sites, drawing nothing and dropping the confirm at the same moment:
 * an empty status region beside the read control alone. Naming the five
 * explicitly below is what lets a `default` mean "nothing reaches here"
 * rather than "everything else".
 *
 * **A `never` parameter rather than a return-type annotation**, which is the
 * alternative a reader reaches for first and which catches neither site:
 * `undefined` is assignable to `ReactNode`, and `confirmablePlan` is already
 * annotated `StampPlan | undefined`. Assigning the narrowed value to `never`
 * is the only spelling that makes the missing case an error.
 *
 * **It draws nothing rather than throwing.** A section absent altogether
 * cannot be pressed where a half-drawn confirmation for a rewrite with no
 * undo can, which is the argument for throwing. What that argument misses is
 * the blast radius: there is no error boundary anywhere in `web/src`, and
 * `main.tsx` renders through a plain `createRoot`, so a throw here unmounts
 * the whole tree under `#root` — the nav, the gate and every route — rather
 * than this section. Returning `undefined` costs nothing the compiler
 * gives, since the `never` parameter above is what makes a missing case an
 * error; it only changes which failure the already-impossible branch lands
 * on. The `console.error` is there so "impossible" stays observable instead
 * of silent.
 */
const unhandledPhase = (phase: never): undefined => {
  console.error('unhandled stamp phase', phase)
  return undefined
}

/**
 * The plan a confirm would send in this phase, or `undefined` where there is
 * none to send.
 *
 * **One function, so the control, its label and the body cannot disagree.**
 * A confirm gated apart from the body can leave a phase that renders a
 * total, an archive list, an irreversible warning and **no controls at all**.
 *
 * An `empty` plan is deliberately not confirmable: there is nothing to write.
 */
const confirmablePlan = (phase: StampPhase): StampPlan | undefined => {
  switch (phase.kind) {
    case 'ready':
    case 'writing':
    case 'refused':
      return phase.plan
    case 'idle':
    case 'reading':
    case 'read-failed':
    case 'empty':
    case 'done':
      return undefined
    default:
      return unhandledPhase(phase)
  }
}

/** A title is one line: each line break a paste brings becomes a space. */
const oneLine = (typed: string) => typed.replace(/\r\n|\r|\n/g, ' ')

/**
 * Sets the Title field's height to its title's lines at its own width, up
 * to five, past which it scrolls (O283). One line is read from the field at
 * `rows={1}` with its own height cleared.
 */
function fitTitle(box: HTMLTextAreaElement) {
  // Cleared, the field is one line tall and loses its place in the title.
  const scrolled = box.scrollTop
  box.style.height = ''
  const style = getComputedStyle(box)
  const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom)
  const line = box.clientHeight - padding
  const border = box.getBoundingClientRect().height - box.clientHeight
  // Read with no scrollbar, which a field one line tall would draw beside
  // its lines and which would narrow them.
  box.style.overflowY = 'hidden'
  const height = Math.min(box.scrollHeight, 5 * line + padding) + border
  box.style.overflowY = ''
  box.style.height = `${height}px`
  box.scrollTop = scrolled
}

/** What `Series.tsx` holds of the open editor, so that no close loses a title. */
export interface TitleFlush {
  /**
   * A blank title is put back first. Then `null` when nothing typed is
   * unsaved and no save is out; otherwise the save of the newest text, sent
   * now or after the one out: `true` once it is stored and written beside
   * the library, `false` once a save failed or was stored but not written
   * there, which is then shown under the field. A `false` holds the close
   * waiting on it, so no close hides either; after an unwritten save
   * nothing is left to send, and the next answer is `null`.
   */
  flush(): Promise<boolean> | null
  /**
   * Whether the field holds a title whose save failed. A leave, an Enter
   * or a close asks for it to be sent again; a pointer passing over tiles
   * does not, so `Series` moves no panel by hover while this holds.
   */
  failed(): boolean
}

export function MetadataEditor({
  slug,
  title,
  onPatch,
  canStamp = false,
  ref,
}: {
  slug: string
  title: string
  /**
   * Same shape as `patchWork` itself — the caller hands over whichever of
   * the four PATCH routes fits the thing being corrected. `slug` is a loose
   * name for "whatever identifier that route takes": a work's slug today,
   * but nothing here inspects it.
   */
  onPatch: (slug: string, fields: WorkEdit) => Promise<PatchResult>
  /**
   * Whether to offer the ComicInfo write-back at all — spec C3 §6.
   *
   * **The caller asserts two things by passing `true`**, which is why this is
   * a prop rather than a `useGate()` read here: that the account holds
   * `can_stamp`, and that `slug` is a *work* slug. The second matters because
   * this component's own doc is explicit that `slug` is a loose name for
   * whatever identifier `onPatch` takes, and the stamp routes take a work's.
   *
   * **Absent, not disabled**, per C3 §6 and `Series.tsx`'s `mayDownload`: a
   * disabled control invites a person to find out why and nothing on this
   * screen could tell them.
   */
  canStamp?: boolean
  /** Where `Series` is handed the editor's `flush`, which its closes wait on. */
  ref?: Ref<TitleFlush>
}) {
  const [value, setValue] = useState(title)
  /**
   * The field's text, as `value` holds it, for the readers no render
   * reaches: a save as it lands, which closes over the render that sent
   * it, and `flush`, which `Series` calls from its document listeners.
   */
  const latest = useRef(title)
  /** Sets the field's text and `latest` together, so neither lags. */
  const setText = (text: string) => {
    latest.current = text
    setValue(text)
  }
  const [editing, setEditing] = useState(false)
  const titleBox = useRef<HTMLTextAreaElement>(null)
  /**
   * The title stored: the one mounted, then the text of each save that
   * succeeded. Escape puts it back while no save is out, and a title equal
   * to it is not sent. Never the server's re-projected title, since the
   * field keeps what was typed (O283), and never taken at focus, where the
   * field may hold a title whose save failed.
   */
  const stored = useRef(title)
  /**
   * The text of the title save that is out, `null` when none. Escape and a
   * blank title put it back while it is out: it is the stored title once
   * it lands, and a field put back to the older one would send that over
   * it at the next leave.
   */
  const sending = useRef<string | null>(null)
  /**
   * The title save that is out, `null` when none: one at a time. It settles
   * only after the newest text asked for meanwhile is sent and settles too,
   * so whoever waits on it waits for every edit.
   */
  const out = useRef<Promise<boolean> | null>(null)
  /**
   * A leave, an Enter or a `flush` came while a save was out, so the newest
   * text goes once that save lands. Cleared as each save is sent, and by
   * Escape, which undoes every edit not yet sent, this ask among them.
   * Cleared when a save fails too: nothing is sent after a failure, which
   * stays shown until the next save, or until the field is back at the
   * stored title: put there by Escape, or found there by a leave, an Enter
   * or a `flush`.
   */
  const again = useRef(false)
  /**
   * `sending` as state, so the note can say a save is out: true from
   * `send` until the chain it starts ends. Never false between a save and
   * the newer title it sends as it lands, or the note would go blank while
   * that title is out. Drawn only once it has held `WAIT_DRAWN_AFTER_MS`,
   * so a quick save says only "Saved".
   */
  const [saving, setSaving] = useState(false)
  const slow = useDrawnWait(saving)
  /**
   * A save has just landed. Said for a moment, and gone at the next edit or
   * save: it speaks of the title that save stored, not the one in the field.
   */
  const [saved, setSaved] = useState(false)
  useEffect(() => {
    if (!saved) return
    const quiet = setTimeout(() => {
      setSaved(false)
    }, 3000)
    return () => {
      clearTimeout(quiet)
    }
  }, [saved])
  // Three states, not two. `undefined` means "nothing to say" — either
  // nothing has been saved yet, or the last save reached disk cleanly. `[]`
  // means the last save succeeded but did not reach `<stem>.library.json`
  // with no reason to name — a work imported from a sidecar, whose document
  // has no on-disk address at all (`hand::path_for` answers `None`, so
  // `hand::Saved::why` is `None` too, and `reasons()` defaults that to `[]`
  // rather than `undefined`). A non-empty array is the same outcome with a
  // reason attached — a failed write's own `io::Error`, or the newer-document
  // reason. The JSX below branches on all three: `unwritten !== undefined`
  // decides whether the note renders at all, and `unwritten.length > 0`
  // decides, independently, whether it renders a reason.
  const [unwritten, setUnwritten] = useState<string[]>()
  const [failure, setFailure] = useState<string>()

  /**
   * The six pieces of state the stamp section holds. **None of them is read
   * by the JSX**: `phaseOf` above folds them into one phase, and everything
   * below renders from that. A control gated on one of these directly is the
   * defect that fold removes.
   *
   * `plan` is the answer to the last read that landed and was not superseded;
   * `asking` tells "nothing read yet" from "a read is out", since both leave
   * the slot `undefined`.
   */
  const [plan, setPlan] = useState<StampPlan>()
  const [asking, setAsking] = useState(false)
  const [planFailure, setPlanFailure] = useState<string>()

  /**
   * The confirmation's own three.
   *
   * **`writing` holds the plan being written rather than a boolean.** A
   * boolean cannot carry the count, which would then be read off the plan
   * slot — which a re-read blanks — so the one place that said a rewrite was
   * running would be a button that two reachable states did not render. The
   * plan is what the POST was given; a boolean is that fact with the useful
   * half thrown away.
   */
  const [stamped, setStamped] = useState<Stamped>()
  const [writing, setWriting] = useState<StampPlan>()
  const [stampFailure, setStampFailure] = useState<string>()

  /**
   * Whether a plan read is already out — the same ref, for the same reason,
   * as `inFlight` below.
   *
   * React batches state, so two presses dispatched before the next render
   * both run this function with `asking` still `false` and the control still
   * in the DOM: without this ref, two `stampPlan`s would go out.
   */
  const planInFlight = useRef(false)
  /**
   * The newest plan read, the same counter as `AccountPage.askCost`'s and for
   * its reason: whichever read was asked for last is the one whose answer may
   * draw, so a slow earlier answer cannot land on top of a newer one.
   *
   * **It is load-bearing.** The read control is rendered in every phase
   * (disabled while a request is out), and `confirmStamp` bumps this counter
   * to supersede a plan read that a stamp has overtaken. Each leaves an
   * answer that must not draw, and this is what discards it.
   */
  const latestPlan = useRef(0)

  /**
   * Whether a stamp is already on its way.
   *
   * **A ref as well as a phase, for the reason every other guarded press in
   * this codebase carries one**: React batches state, so two presses
   * dispatched before the next render both run the handler and `disabled` has
   * not reached the DOM. On this control that is not a cosmetic double-send —
   * the second POST arrives against a library the first has already moved, so
   * it is answered 409 by the plan-mismatch guard and reads to the person as
   * a stamp that failed.
   *
   * Declared above `askPlan` rather than beside `confirmStamp`, because a
   * plan read has to see it too — see `askPlan`'s own guard.
   */
  const inFlight = useRef(false)

  /**
   * Apply a plan answer, if it is still the one being waited for.
   *
   * The ref is released here rather than at the call sites so a superseded
   * answer leaves it held by the read that superseded it. A read superseded
   * by a *stamp* is the exception — nothing else would ever release it — so
   * `confirmStamp` releases it there.
   */
  const settlePlan = (seq: number, draw: () => void) => {
    if (seq !== latestPlan.current) return
    planInFlight.current = false
    setAsking(false)
    draw()
  }

  // Fetched on press, never on mount — computing a plan costs a disk seek
  // per archive, unmeasured on the deployment target (a NAS over spinning
  // disks), and mounting this section once per opened panel would spend that
  // cost on every panel nobody stamps.
  const askPlan = () => {
    // **The ref, paired with the `disabled` the control carries.** The
    // attribute is what a person sees and what stops the second press of a
    // slow double-click; the ref is what stops two presses dispatched inside
    // one React batch, before that attribute reaches the DOM. This control
    // and the confirm below carry both.
    //
    // **A running POST refuses a plan read as firmly as a running GET does.**
    // The confirm and this control are on screen together in the `ready` and
    // `refused` phases and React batches state, so one press of each inside a
    // single batch reaches both handlers with the DOM unchanged; without this
    // the GET would go out while the stamp was on the wire and answer with a
    // library caught mid-rewrite, which is neither the plan that was
    // confirmed nor the one that follows it. `confirmStamp` carries no
    // matching `planInFlight` guard because its POST needs none: it sends the
    // plan the person is looking at, and `stamp::apply` compares that body to
    // a freshly computed plan field for field and answers 409 on any
    // difference, so a library that moved under it refuses the write rather
    // than mis-applying it.
    if (planInFlight.current || inFlight.current) return
    planInFlight.current = true
    const seq = ++latestPlan.current
    setAsking(true)
    setPlan(undefined)
    setPlanFailure(undefined)
    // The refusal that sent the person back here is about the plan they were
    // just shown, and this reads a new one — leaving it on screen would put a
    // stale alert over a fresh list.
    //
    // **`stamped` is deliberately not cleared here.** What an irreversible
    // rewrite did is not a property of the current read, so it is not a
    // property of the phase either: the completion below renders from
    // `stamped` at the section's own level, and a read cannot swallow it
    // whenever it happens. The record is cleared by the press that starts a
    // *different* rewrite, in `confirmStamp`, and by nothing else.
    setStampFailure(undefined)
    stampPlan(slug)
      .then((answer) => {
        settlePlan(seq, () => {
          setPlan(answer)
        })
      })
      .catch((err: unknown) => {
        settlePlan(seq, () => {
          setPlanFailure(messageOf(err))
        })
      })
  }

  /**
   * Rewrite the archives `sending` names.
   *
   * **The plan comes in as an argument rather than off the closure**, and it
   * is the one the phase drew: the control that calls this is rendered from
   * `confirmablePlan(phase)` and hands that same value back, so the body
   * posted is by construction the plan the person was looking at.
   * Reading `plan` here instead could send a body the block around it was no
   * longer describing.
   */
  const confirmStamp = (sending: StampPlan) => {
    if (inFlight.current) return
    inFlight.current = true
    setWriting(sending)
    setStampFailure(undefined)
    // **The previous rewrite's report goes with the press that starts a new
    // one.** Two completions cannot be told apart on screen: leaving it would
    // put "Rewrote 2 archives." above a refusal of a *different* write with
    // nothing to say which was which. The alternative — keeping it and
    // labelling the two — needs a timestamp this section does not have and
    // would still read as one report.
    setStamped(undefined)
    // **A stamp supersedes a plan read in flight.** That GET went out before
    // this POST and answers about a library this write is in the middle of
    // moving: it is neither the plan that was confirmed nor the one that
    // follows. Bumping the counter is what makes `settlePlan` discard it, and
    // since that discard returns before releasing the ref, the release
    // happens here instead.
    //
    // The alternative — drawing that answer — would draw a plan describing
    // archives this write is in the middle of rewriting, and the section
    // would report either "Nothing to write" or a fresh plan with every
    // control gated off.
    planInFlight.current = false
    latestPlan.current += 1
    setAsking(false)
    // **`sending.archives`, not a rebuild of them.** `stamp::apply` compares
    // the body to a freshly computed plan field for field; anything
    // reconstructed here is a 409 waiting to happen.
    own
      .track(performStamp(slug, sending.archives))
      .then((answer) => {
        inFlight.current = false
        setWriting(undefined)
        setStamped(answer)
        // **The plan this applied is spent.** Every archive it named now
        // holds the document taiyaku would write, so the server answers 409
        // for it and it describes nothing anybody can act on. Clearing it is
        // what makes "which plans on screen predate the rewrite" a question
        // with a mechanical answer — none of them do, because this line
        // clears the one that was applied and the supersede above discards
        // the one that raced it. A plan on screen after this is one somebody
        // pressed for afterwards, and confirming it is a real thing to do.
        setPlan(undefined)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setWriting(undefined)
        setStampFailure(messageOf(err))
        // A 409 may be a hold no read here has seen: read it, so it is drawn.
        if (statusOf(messageOf(err)) === 409) hold.askAgain()
      })
  }

  /**
   * Sends `text` as the title. Once it is stored, the field's newest text
   * follows it if one was asked for meanwhile and differs; the answer is
   * the last one's, as `TitleFlush.flush` gives it.
   */
  const send = (text: string): Promise<boolean> => {
    sending.current = text
    again.current = false
    setSaving(true)
    setSaved(false)
    setFailure(undefined)
    const save = onPatch(slug, { title: text }).then(
      (result) => {
        stored.current = text
        setUnwritten(result.persisted ? undefined : reasons(result))
        const newest = latest.current
        // A blank field is still being typed in, and its leave puts a
        // title back rather than sending one.
        if (again.current && newest !== text && newest.trim() !== '') {
          return send(newest)
        }
        sending.current = null
        out.current = null
        setSaving(false)
        setSaved(true)
        return result.persisted
      },
      (err: unknown) => {
        sending.current = null
        out.current = null
        again.current = false
        setSaving(false)
        setFailure(messageOf(err))
        // Focused, the field holds a title not stored, which its own Escape
        // puts back; unmarked, `Series` would take that Escape as a close
        // and send the failed title again.
        if (document.activeElement === titleBox.current) setEditing(true)
        return false
      },
    )
    out.current = save
    return save
  }

  /**
   * Saves the field's title, as it is left, on Enter, by the form's submit
   * or by `flush`, and answers as `TitleFlush.flush` says. While a save is
   * out it sends nothing yet, and answers that save, which sends the newest
   * text after it.
   */
  const saveTitle = (): Promise<boolean> | null => {
    // The server stores an empty title unchecked, and leaving the field
    // saves it: a stray leave after clearing the field must not write one.
    if (latest.current.trim() === '') {
      setText(sending.current ?? stored.current)
    }
    if (out.current !== null) {
      again.current = true
      return out.current
    }
    if (latest.current === stored.current) {
      // The failure spoke of a title no longer in the field. The unwritten
      // note stays: it speaks of the stored title, the one in the field.
      setFailure(undefined)
      return null
    }
    return send(latest.current)
  }

  useImperativeHandle(ref, () => ({
    flush: saveTitle,
    failed: () => failure !== undefined && latest.current !== stored.current,
  }))

  const submit = (event: FormEvent) => {
    event.preventDefault()
    saveTitle()
  }

  // Fitted again as the title changes and as the field's width does: a new
  // width is a new count of lines (O283).
  // biome-ignore lint/correctness/useExhaustiveDependencies: `value` is a trigger: the title's length sets the field's height.
  useLayoutEffect(() => {
    const box = titleBox.current
    if (box === null) return
    fitTitle(box)
    // And as a face lands: the UI face comes in subsets fetched as text
    // first needs them, so a title in kanji is fitted in a fallback's
    // widths until its subset arrives. jsdom has no `document.fonts`.
    const landed = () => {
      fitTitle(box)
    }
    document.fonts?.addEventListener('loadingdone', landed)
    const unheard = () => {
      document.fonts?.removeEventListener('loadingdone', landed)
    }
    // jsdom has no observer and lays nothing out to follow.
    if (typeof ResizeObserver === 'undefined') return unheard
    let width = box.clientWidth
    let again = 0
    const resized = new ResizeObserver(() => {
      // The first report as the field is watched carries the width it was
      // fitted at; only a new width moves a line.
      if (box.clientWidth === width) return
      width = box.clientWidth
      // Unwatched while it is fitted: a height set inside this callback is
      // a resize the observer cannot report in this frame, which it says
      // with an error event. Watched again, its first report carries the
      // width then, so a change in between is still caught.
      resized.unobserve(box)
      fitTitle(box)
      again = requestAnimationFrame(() => {
        resized.observe(box)
      })
    })
    resized.observe(box)
    return () => {
      unheard()
      if (again) cancelAnimationFrame(again)
      resized.disconnect()
    }
  }, [value])

  const titleKeys = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // An Enter or an Escape that ends an input method's composition is
    // that method's: Japanese titles are typed through one. Safari sends
    // it once the composition has ended, marked only by keyCode 229.
    const { isComposing, keyCode } = event.nativeEvent
    if (isComposing || keyCode === 229) return
    if (event.key === 'Enter') {
      // A title is one line: Enter saves it rather than breaking it.
      event.preventDefault()
      saveTitle()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      setText(sending.current ?? stored.current)
      again.current = false
      // The failure spoke of the title this discards. The unwritten note
      // stays: it speaks of the stored title, the one put back.
      setFailure(undefined)
      setEditing(false)
    }
  }

  /** A title typed or pasted after Escape is being edited again. */
  const edit = (line: string) => {
    setText(line)
    setEditing(true)
    setSaved(false)
  }

  const titlePaste = (event: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const pasted = event.clipboardData.getData('text')
    const line = oneLine(pasted)
    if (line === pasted) return
    // Put in here, where the caret can be left after it: a value changed
    // through the render puts the caret at the end.
    event.preventDefault()
    const box = event.currentTarget
    box.setRangeText(line, box.selectionStart, box.selectionEnd, 'end')
    edit(box.value)
  }

  const phase = phaseOf({
    plan,
    asking,
    planFailure,
    writing,
    stampFailure,
    stamped,
  })
  const confirmable = confirmablePlan(phase)
  /**
   * The two phases with a request out, which are exactly the two the refs
   * guard. Named once so the attribute and the ref cannot come apart.
   */
  const busy = phase.kind === 'reading' || phase.kind === 'writing'
  /**
   * **A derive or the job lock holds the confirm** (O51): the server refuses
   * a stamp while either holds. Read only while a plan can be confirmed,
   * as the plan itself is read only when asked for, and polled while it
   * holds (`useDeriveHold`). Not while the stamp is out: that is its own,
   * and so is the derive a read sent while it is out finds (`own`).
   */
  const own = useOwnRequests()
  const hold = useDeriveHold(confirmable !== undefined, own)
  const stampWaits = (hold.derive || hold.job) && phase.kind !== 'writing'
  const stampWaitsId = useId()

  /**
   * The formats this work holds that a stamp will not write.
   *
   * **One definition, drawn from whichever plan is being shown.** Rendered
   * inside the non-empty branch alone, it would leave a work whose every
   * archive is a `.cbr` told "nothing to write" and never told that the rar
   * is why — two situations the server has already told the screen apart,
   * collapsed into one sentence. It reads "this work holds" rather than
   * "also holds" for the same reason: in the empty branch there is no *also*.
   *
   * **What it says about those formats is the rule, not a claim about
   * reading them.** Saying they are formats "which taiyaku reads and never
   * writes" would be false for every token it can print but `dir`:
   * `Format::is_readable` is `Dir | Zip` alone, and `pages.rs`'s
   * `unsupported_container` refuses rar, 7z, tar and PDF outright. What is
   * true of all nine tokens is the write rule, so that is what this states,
   * and the two non-formats get their own sentence rather than being folded
   * into a list of formats.
   *
   * **The EPUB sentence is conditional, and it is there because the rule as
   * stated is unreconcilable without it.** An EPUB *is* a zip container, so
   * "writes into zip archives and nothing else" beside `epub-fixed` in the
   * list reads as a contradiction. What taiyaku actually does is filter on
   * what `sniff` identified: `stamp::plan` skips anything whose format is not
   * `Format::Zip`, and an EPUB is `EpubFixed`/`EpubReflow`. So the rule is
   * stated in terms of that identification, and the extra sentence draws only
   * when an EPUB is in the list — the alternative, saying it always, spends a
   * sentence on a distinction that does not arise for a work holding a rar.
   */
  const skippedNote = (shown: StampPlan) => {
    const skipped = shown.skipped_formats
    if (skipped.length === 0) return null
    /**
     * The tokens that name a format, as opposed to the three that do not.
     * `stamp.rs`'s `skipped_formats` prints `dir`, `rar`, `7z`, `pdf`, `tar`,
     * `epub-fixed` and `epub-reflow` from `Format::token`; `unknown` for a
     * file that matched no signature, `unreadable` for one that could not be
     * opened, and `copied` for one held in more than one directory. Those three
     * are not formats — "this work holds unreadable" is not a sentence.
     */
    const namedFormats = skipped.filter(
      (token) =>
        token !== 'unknown' && token !== 'unreadable' && token !== 'copied',
    )
    const containers = namedFormats.some((token) => token.startsWith('epub'))
    return (
      <div className="skipped">
        {namedFormats.length > 0 && (
          <p className="formats">
            Left alone: this work holds {namedFormats.join(', ')}. A stamp
            writes ComicInfo.xml into the archives taiyaku identifies as a plain
            zip, and nothing else, whether or not taiyaku can read the format.
          </p>
        )}
        {containers && (
          <p className="containers">
            An EPUB is a zip container as well, which is where that
            identification shows: taiyaku identifies one as an EPUB rather than
            as a plain zip, so a stamp leaves it alone with the rest.
          </p>
        )}
        {skipped.includes('unreadable') && (
          <p className="unopened">
            At least one file in this work could not be opened at all, so
            nothing here can say what it holds. A stamp leaves it where it is.
          </p>
        )}
        {skipped.includes('unknown') && (
          <p className="unrecognised">
            At least one file in this work matches no format taiyaku recognises.
            A stamp leaves it where it is.
          </p>
        )}
        {skipped.includes('copied') && (
          <p className="copied">
            At least one file in this work is held in more than one directory. A
            stamp leaves it alone, so its copies stay identical.
          </p>
        )}
      </div>
    )
  }

  /** What a confirm would rewrite, and everything that follows from it. */
  const planBlock = (shown: StampPlan) => (
    <>
      <p className="total">
        This rewrites {archiveCount(shown.archives.length)},{' '}
        {humanBytes(shown.total_bytes)} in all. A zip cannot be edited in place,
        so each one is copied whole and renamed over the original.
      </p>
      <ul className="archives">
        {shown.archives.map((archive: Archive) => (
          <li className="archive" key={archive.member_id}>
            <span className="path">
              <PathWithBreaks path={archive.path} />
            </span>{' '}
            <span className="size">{humanBytes(archive.bytes)}</span>
          </li>
        ))}
      </ul>
      <div className="consequences">
        <p>
          Every archive above gets a new sha256, and three things follow from
          that:
        </p>
        <ul>
          <li>
            any alignment or labelling recorded against these members is treated
            as stale until the next scan, because the staleness check compares
            exactly this digest;
          </li>
          <li>
            every thumbnail already generated for these members is regenerated
            the next time it is asked for;
          </li>
          {/*
           * **A forecast weakened to what the outcome copy can report.** It
           * does not promise that `<content_key>.library.json` moves with the
           * key, the one thing `key-moved-alone` below has to take back: for
           * a stamp `hand::rename_from` answers `false` for five causes, and two
           * of them — a file already at the new address, and a root that
           * refuses the rename — leave the document stranded at the old key.
           * Both paragraphs are on screen together after a stamp, so such a
           * promise would be contradicted in place; and it would be made
           * before the person agreed to something with no undo, which is the
           * half that matters.
           */}
          <li>
            this work's content_key changes if any archive above belongs to its
            hub edition, and taiyaku then tries to move its
            &lt;content_key&gt;.library.json to the new address. That move is
            attempted, not promised: a file already sitting at the new address,
            or a root that refuses the rename, leaves the document stranded at
            the old key.
          </li>
        </ul>
        <p className="irreversible">
          There is no undo: the original files are replaced, not kept. If this
          is interrupted partway, the archives already rewritten stay rewritten
          and their rows stay stale until the next scan.
        </p>
      </div>
      {skippedNote(shown)}
    </>
  )

  /**
   * What the section is saying, one branch per phase and no branch reading
   * anything but the phase.
   *
   * **Every phase here either draws a control below or is one press from
   * one.** `idle` and `done` draw nothing at all: `idle` because nothing has
   * been asked, `done` because the completion above it is the whole answer.
   * Neither is a dead end — the read control renders in all eight phases —
   * and there is no terminal phase in this section, which is why none of them
   * says so.
   */
  const phaseBody = () => {
    switch (phase.kind) {
      case 'idle':
      case 'done':
        return null
      case 'reading':
        return (
          <p className="asking">Working out which archives would change…</p>
        )
      case 'read-failed':
        return (
          <p className="plan-failure" role="alert">
            {phase.message}
          </p>
        )
      case 'empty':
        return (
          <>
            {/*
             * **The sentence carries no disjunct for a work with no zip
             * archive at all.** ", or this work has no zip archive at all"
             * would be false above "Rewrote 2 archives." and above a 409
             * saying the work's archives had changed — both states this
             * section reaches with an empty plan on screen. It is also
             * unnecessary: "every zip archive already holds it" is true of a
             * work with none, so the universal covers that case, and the
             * `.skipped` note under it says what the work holds instead.
             */}
            <p className="nothing">
              Nothing to write: every zip archive in this work already holds the
              ComicInfo.xml taiyaku would write.
            </p>
            {skippedNote(phase.plan)}
          </>
        )
      case 'ready':
        return planBlock(phase.plan)
      case 'writing':
        return (
          <>
            {/*
             * **At the section's level, not inside the confirm control.** A
             * "rewrite is running" that reached the DOM only as that button's
             * label, gated on the plan slot, would let a re-read blank the
             * slot and the section say "Working out which archives would
             * change…" while archives were being irreversibly rewritten. The
             * phase carries the plan being written, so this sentence can be
             * drawn whatever the slot holds.
             */}
            <p className="writing">
              Rewriting {archiveCount(phase.plan.archives.length)} — the
              originals are being replaced now, and nothing here can stop it.
            </p>
            {planBlock(phase.plan)}
          </>
        )
      case 'refused':
        return (
          <>
            <p className="stamp-failure" role="alert">
              {phase.message}
            </p>
            {phase.plan !== undefined && planBlock(phase.plan)}
          </>
        )
      default:
        return unhandledPhase(phase)
    }
  }

  return (
    <>
      <form className="metadata-editor" onSubmit={submit}>
        <label className="field title">
          <span>Title</span>
          {/* A textarea, so the whole title wraps, at rest as while it is
              edited; `fitTitle` sets its height (O283). */}
          <textarea
            ref={titleBox}
            name="title"
            rows={1}
            // Firefox spellchecks a textarea, as it does not an input, and
            // would underline a romaji title. Enter saves, and says so.
            spellCheck={false}
            enterKeyHint="done"
            value={value}
            data-editing={editing ? '' : undefined}
            onFocus={() => {
              setEditing(true)
            }}
            onBlur={() => {
              setEditing(false)
              saveTitle()
            }}
            onKeyDown={titleKeys}
            onPaste={titlePaste}
            onChange={(event) => {
              // A line break that comes another way, such as a drop.
              edit(oneLine(event.target.value))
            }}
          />
        </label>
        {/* There from the first render, since a live region must exist
            before what it announces. Outside the label, so it is no part
            of the field's name; drawn on the label's line (`library.css`). */}
        <p className="saved-note" role="status">
          {slow ? 'Saving…' : saved ? 'Saved' : ''}
        </p>
        {unwritten !== undefined && (
          <div className="unwritten">
            <p>
              Saved — but this correction could not be saved beside your library
              {unwritten.length > 0 ? ':' : '.'}
            </p>
            {unwritten.length > 0 && (
              <ul>
                {unwritten.map((why) => (
                  <li key={why}>{why}</li>
                ))}
              </ul>
            )}
          </div>
        )}
        {failure !== undefined && (
          <p className="failure" role="alert">
            {failure}
          </p>
        )}
      </form>
      {/*
       * Outside the `<form>` above, not inside it — a `<button>` inside a
       * form with no `type` attribute submits that form, which would make an
       * irreversible archive rewrite a side effect of a title save on a
       * reversible title edit. `type="button"` below is belt and braces on
       * top of that structural separation.
       */}
      {canStamp && (
        <section className="stamp-comicinfo" aria-label="Write ComicInfo.xml">
          <h3>Write ComicInfo.xml into this work's archives</h3>
          {/*
           * One element for all eight phases and for the completion above
           * them, present from the first render: a live region has to exist
           * before the thing it announces changes, which is `AcceptForm`'s own
           * reasoning for the identical wrapper around its four. `aria-live`
           * rather than `role="status"`, so the two failure paragraphs inside
           * keep the assertive `role="alert"` they need without a second live
           * role wrapping them.
           *
           * `MetadataEditor.test.tsx` pins it the way `AcceptForm.test.tsx`
           * pins its own.
           */}
          <div className="stamp-state" aria-live="polite">
            {/*
             * **On `stamped` alone, above the phase and inside the live
             * region.** What a rewrite with no undo actually did is a fact
             * about a cycle that has finished, and the phase is what this
             * section is doing now — so a plan read afterwards can put the
             * section back into `ready` without touching this. It sits first
             * because it reads chronologically: what happened, then what is
             * being considered next.
             *
             * **Inside the region because success is otherwise announced
             * nowhere.** A failure carries `role="alert"`; this does not.
             * Outside every live region, a screen reader would hear
             * "Rewriting 2 archives — the originals are being replaced now",
             * then that paragraph would be removed (silent under the default
             * `aria-relevant`), and the outcome of the irreversible act
             * inserted in silence. The alternative — `role="status"` on
             * `.stamped` itself — announces nothing, for `AcceptForm`'s own
             * reason: a region declared on the message is created at the moment
             * its content appears, and a region has to exist before what it
             * announces is put into it. The wrapper this sits inside is
             * rendered in all eight phases, from the first render.
             *
             * Inside `plan.archives.length > 0`, a fresh empty plan — the
             * likely answer, since the stamp has just written every archive —
             * would replace "Rewrote 2 archives" and the whole O81
             * stranded-document paragraph with "Nothing to write".
             */}
            {stamped !== undefined && (
              <div className="stamped">
                <p className="rewrote">
                  Rewrote {archiveCount(stamped.rewritten)}.
                </p>
                {/*
                 * Three outcomes, not two. `content_key` is `null` whenever
                 * the work's identity did not move — `identity::content_key`
                 * hashes the hub edition's member digests alone — and a
                 * paragraph that printed that `null` into the sentence and
                 * explained the absent key with "it had no document on disk
                 * to move" would claim what `stamp::apply` never does: it
                 * hard-codes `document_moved` false when the key did not
                 * move, without asking whether a document exists.
                 */}
                {stamped.content_key === null ? (
                  <p className="key-held">
                    This work's content_key did not change, so nothing had to
                    move with it: the key is derived from the hub edition's
                    members alone, and no archive rewritten here belongs to that
                    edition.
                  </p>
                ) : stamped.document_moved ? (
                  <p className="key-moved">
                    This work's content_key is now {stamped.content_key}, and
                    its &lt;content_key&gt;.library.json moved with it.
                  </p>
                ) : (
                  /*
                   * O81, stated as the uncertainty it is.
                   * For a stamp, `hand::rename_from` answers `false` for
                   * five causes: no on-disk address, no parent, nothing at the
                   * old key, something already at the new one, and a rename
                   * the root refused. The last two leave the document
                   * stranded at the old key; the first three are benign.
                   * Nothing in the answer says which, so neither does this.
                   */
                  <p className="key-moved-alone">
                    This work's content_key is now {stamped.content_key}, and
                    its &lt;content_key&gt;.library.json did not move with it.
                    Either there was no document to move, or one is now stranded
                    at the old key — nothing here can tell those apart. The next
                    scan reports a document left behind as an orphan, with the
                    title it holds.
                  </p>
                )}
                {stamped.document_rewritten && (
                  <p className="document">
                    This work's &lt;content_key&gt;.library.json was rewritten
                    too, so it records the new digests.
                  </p>
                )}
                {stamped.sidecar_rewritten && (
                  <p className="sidecar">
                    This work's taiyaku.json was rewritten too, so it records
                    the new digests.
                  </p>
                )}
              </div>
            )}
            {phaseBody()}
          </div>
          {confirmable !== undefined && (
            <HeldButton
              type="button"
              name="confirm-stamp"
              onClick={() => {
                confirmStamp(confirmable)
              }}
              held={stampWaits}
              disabled={phase.kind === 'writing'}
              aria-describedby={stampWaits ? stampWaitsId : undefined}
            >
              {phase.kind === 'writing'
                ? 'Rewriting…'
                : `Rewrite ${archiveCount(confirmable.archives.length)}`}
            </HeldButton>
          )}
          {/*
           * **One read control, one label, in every phase.** Two — "What
           * would this change?" gated on there being no plan, and "What would
           * this change now?" gated on a refusal — with the same handler would
           * both draw at once after a failed read beside a refused stamp.
           * Rendering it always, disabled while a request is out, also leaves
           * no phase without controls: whatever this section is showing, there
           * is a way to ask again.
           */}
          <button
            type="button"
            name="plan-stamp"
            onClick={askPlan}
            disabled={busy}
          >
            What would this change?
          </button>
          {stampWaits && (
            <p id={stampWaitsId} className="stamp-waits">
              {JOB_WAITS}
            </p>
          )}
          {hold.failure !== undefined && <p role="alert">{hold.failure}</p>}
        </section>
      )}
    </>
  )
}
