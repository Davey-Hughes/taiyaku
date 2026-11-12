/**
 * The rule editor — spec C4 §1, and the build design's §5. The one screen
 * Komga structurally cannot have.
 *
 * **It previews on commit and never while typing.** A preview is expensive on a
 * real library, and the derivation is the entire cost. So a preview runs on: a
 * field blur or Enter, an enable toggle, a drag-reorder, an add, a delete, and
 * selecting a different rule. The last is the one that is not obvious: `rows`
 * is scoped to the selected rule, so changing which rule is open is the only
 * way to populate the Matched files and Provenance tabs for it — and it is one
 * gesture, not a stream. A person clicking down a five-rule list pays five
 * derivations; a person typing a regex pays none. The alternatives — an
 * explicit Preview button, a long typing debounce — are weighed and refused in
 * the build design's §5.3.
 *
 * **Nothing is written until Save**, per C4 §1, and the button states what it
 * will do from the live preview's `works_rehomed`. Because the preview is
 * never stale, that number is never a lie.
 *
 * **The save's own impact is compared against the preview it was made from.**
 * Nothing should differ on a single-writer library — but the screen's entire
 * promise is that the preview is true, and the cheapest place to notice it is
 * false is the save that just disproved it. So a difference is drawn rather
 * than swallowed. The compare is field-by-field over `Object.keys`, matching
 * `RuleForm`'s own dirty-check comment rather than the `JSON.stringify` it
 * warns against — `Impact` is server-controlled, but the risk the comment
 * names (a field reorder silently comparing unequal objects as equal) is a
 * property of `JSON.stringify` itself, not of who controls the type.
 *
 * **Save is held while a preview is in flight, not only while saving.**
 * The comparison above means something only against a preview that has
 * landed: for as long as `busy` is true, `predicted.current` still holds the
 * *previous* preview's impact. Clicking Save inside that window sends the
 * just-committed `rules` and compares the answer against that stale impact —
 * drawing the "something changed the library" warning for a race the client
 * caused, not the server, on the very six gestures this file's own doc
 * comment names as preview triggers.
 *
 * **No admin guard.** `/config/rules` is reachable by URL and this screen
 * renders the server's own 403 sentence, exactly as `/proposals` does. A
 * client that *decides* who may administer is the shape O93 closed.
 *
 * **One preview out at a time, and the newest list wins.** The server's
 * preview takes the derive lease without waiting for it (O51), so a second
 * sent while the first is out would be refused for the page's own lease. A
 * gesture made while one is out — a blur that starts a preview, then a click
 * on another rule before it lands — queues its list, which is sent as that
 * one settles and before its answer is read. The one out describes an older
 * list, so the sequence number the queued one takes drops its answer and its
 * failure, as the number drops one a derive's hold made stale: drawn, its
 * impact would stand under a Save button that states a number from it.
 *
 * **A successful save re-previews the list it just saved, and Save stays held
 * until that lands.** `save` does not touch `preview`, so without this the
 * button would keep stating the *pre-save* `works_rehomed` after the library
 * has been re-homed to match it — and a second click would send an
 * already-derived library through `apply` again, where every `Impact` field
 * comes back `0` against a `predicted.current` that still held the first save's
 * number. That mismatch would draw "the save differs from the preview it was
 * made from", blaming the server for a race the client caused. The mismatch
 * warning itself is kept off `failure` and in its own `saveMismatch` slot for
 * the identical reason `importNotice` is kept off it: the follow-up preview
 * clears `failure` on landing, and a warning stored there would be wiped by the
 * very preview run to produce the number it is standing next to.
 *
 * **A failed preview invalidates `preview` and `predicted.current`, not only
 * `busy`.** Without this, a preview that failed — a network blip, a 500, the
 * 400 the store raises for a rule id another admin's edit just deleted — would
 * leave both at whatever the *last successful* preview computed, for a rule
 * list that is no longer the one on screen. The button would stay enabled,
 * state that stale number, and a click would send the current (unpreviewed)
 * list to `apply` — which would then disagree with the stale
 * `predicted.current` and draw the same "something changed the library"
 * warning, again blaming the server for a client-side gap.
 *
 * **One writer at a time — O96, O97.** This screen has two writers of the rule
 * table, Save and Import, and `writer` is the single gate both take: a `'save'
 * | 'import' | undefined` ref, read synchronously, mirrored into `writing` for
 * the render. It is three-valued rather than a boolean each because `saving &&
 * importing` must never exist.
 *
 * **Every write's answer replaces the whole list**, so a gesture committed
 * inside either window would be applied, previewed and silently discarded.
 * `RuleList` and `PreviewTabs`' two override options freeze on `frozen`
 * (`writing !== undefined`), which is why the prop is not called `saving`: it
 * is true during an import too, and a name that stopped being true is the
 * staleness the open-items table exists to avoid.
 *
 * **`RuleForm` is deliberately not frozen**, because `disabled` on a focused
 * input makes the browser fire `blur` and this form commits on blur — the
 * mechanical guard would itself commit the half-typed edit into the window it
 * was meant to close. The form stays live and the *commit* is what the gate
 * acts on, with two answers rather than one. For a **save** it is *held* and
 * applied on top of `answer.rules`, taking that row's `id` and `ordinal` from
 * the server; the freeze is what makes that merge exact, since `answer.rules`
 * is positionally aligned to the list that was sent. For an **import** it is
 * *discarded*, and that is forced rather than chosen: an import mints fresh
 * ids and replaces every rule, so no index carries the edit across.
 *
 * **Both writers refuse rather than queue, in both directions** — spec §2.1 and
 * O97's §1. Each writes the rule table, so by the time a queued one could run
 * the other's write has already happened. **Import needs two guards** for the
 * drag's reason: the button's hold lands on a render, and the hidden file input
 * delivers whatever the button says, because a picker opened before the write
 * started is still open. **Save's second guard is defence rather than cover**,
 * and O101 is why: `beginImport` takes the import gate inside the discrete
 * event itself, which React flushes synchronously, so `frozen` and
 * `writer.current` change together and no click can land between them. A gate
 * taken a microtask later, as inside `file.text()`'s `.then`, would let a click
 * reach Save with the button still drawn enabled. The check stays — the
 * invariant is what is being stated, and a writer that took the gate outside a
 * discrete event would reopen the gap without touching `save` — but nothing
 * exercises it, and its message is unreachable with it.
 *
 * **Every refusal goes to `refused`**, which `commit` does not clear: a
 * refused save, a form commit discarded by an import, and an import refused
 * because a write was already in flight (O100). In `importNotice` that last
 * one would be erased by the running import's own answer, which is precisely
 * the failure `refused` exists to prevent. A *failed* save keeps the edit and
 * puts its reason in `heldNotice` rather than `failure` — spec §5.1 — and a
 * *failed* import clears the gate on **all three** of its failure shapes,
 * without which one bad file would hold both writers for the rest of the
 * session. They are not one, and they are caught in **two** places: the
 * request's own rejection in `sendImport`'s `.catch`, and the other two in
 * `beginImport`'s. The **read**'s rejection exists only because the gate is
 * taken before the read. And `importRules` is not `async` — it parses the
 * document as an argument, so a document that is not JSON throws
 * *synchronously*, out of `sendImport` before its own chain is built; what
 * makes that a rejected promise rather than an escaped throw is that
 * `sendImport` is called inside a `.then`, so it lands in `beginImport`'s
 * `.catch` too. Picking a file that is not JSON is the most ordinary way to
 * reach it, and the O97 test that drives one is what keeps it caught.
 *
 * **While a derive holds the lease, no preview is sent, and Save is held while
 * a derive or the job lock holds** (O51), as the server refuses each then
 * (`useDeriveHold`). The hold's sentence is drawn in the preview's place and
 * beside Save. The preview on screen describes the library as it was before
 * the derive, so it is dropped as the hold comes on, and the list on screen
 * is previewed once the hold ends. A refusal of a preview or a save for a
 * hold reads the status again, so that the hold is drawn from that read;
 * a refused preview is sent again once a read finds no derive holding.
 * Each preview and save takes the lease itself, so a read that overlaps one
 * draws no hold from the derive it finds, and a later read that overlaps
 * none does (`OwnRequests`): drawn from it, the hold would drop the preview
 * it found.
 *
 * **`selected` in `save`'s closure is not stale, and that is a property of the
 * guards rather than of this file.** Were it to change mid-save, the follow-up
 * preview's `rows` would be scoped to the rule that *was* open while the form
 * showed the new one. Nothing can change `selected` during a save — `RuleList`
 * is frozen, `PreviewTabs`' two options are held on `busy || frozen` (O95), and
 * a stored list is not taken while a write is out — so a `selectedRef` would
 * hide the invariant rather than state it. A new surface that sets `selected`
 * without a guard reopens the defect without touching `save`.
 */
import { useCallback, useEffect, useId, useRef, useState } from 'react'
import type { ChangeEvent } from 'react'
import { type Root, statusOf } from './config'
import type { Inbox } from './curation'
import { JOB_WAITS, useDeriveHold, useOwnRequests } from './deriveHold'
import { HeldButton } from './HeldButton'
import { useDrawnWait } from './hooks'
import {
  ACCEPTED_PROPOSALS,
  invalidateLibraryViews,
  LIBRARIES,
  ROOTS,
  RULES,
} from './pageReads'
import { PreviewTabs } from './PreviewTabs'
import { invalidate } from './reads'
import { RuleForm } from './RuleForm'
import { RuleList } from './RuleList'
import { DeriveSection } from './ScanReport'
import { PageHeader, PageLoading, Section } from './settingsLayout'
import {
  blankRule,
  type Impact,
  importRules,
  moved,
  type Preview,
  previewRules,
  renumber,
  type Rule,
  type Saved,
  saveRules,
  toDocument,
} from './rules'
import { messageOf, reasonOf } from './message'
import { usePage } from './usePage'
import { WaitWords } from './WaitWords'

/**
 * Field-by-field, matching `RuleForm`'s own dirty-check comment. `Impact` is
 * a flat record of numbers, so `Object.keys` alone is enough to compare it —
 * no nested walk is needed the way a `Rule` compare might.
 */
const impactChanged = (a: Impact, b: Impact) =>
  (Object.keys(a) as (keyof Impact)[]).some((k) => a[k] !== b[k])

/**
 * Whether two rule lists hold the same rules in the same order, compared as
 * `RuleForm` compares a draft with its rule: field by field and by value, so
 * a rule edited back to what it was is the same rule.
 */
const sameRules = (a: readonly Rule[], b: readonly Rule[]) =>
  a.length === b.length && a.every((rule, i) => sameRule(rule, b[i]))

const sameRule = (a: Rule, b: Rule) =>
  (Object.keys(a) as (keyof Rule)[]).every((k) => a[k] === b[k])

/**
 * Which write is in flight, if any. Both writers on this screen — Save and
 * Import — take this gate, and neither runs while the other holds it.
 *
 * **One three-valued variable rather than a boolean each.** Two booleans admit
 * `saving && importing`, a state that must never exist and that four separate
 * conditions would then have to agree about (O97). When every fix moves one
 * condition and the gates then disagree, the defect is the absence of a single
 * source of truth, not the gates.
 */
type Writer = 'save' | 'import' | undefined

export function RuleEditor() {
  const { ready, answers, failures, loadingDrawn } = usePage<
    [{ rules: Rule[] }, { roots: Root[] }, Inbox]
  >([RULES, ROOTS, ACCEPTED_PROPOSALS])
  const [stored, rootsRead, acceptedRead] = answers
  /**
   * The stored rules, once the body may be drawn: all three reads in. A read
   * that failed with nothing kept leaves no body, only its failure.
   */
  const listed =
    ready && rootsRead !== undefined && acceptedRead !== undefined
      ? stored?.rules
      : undefined
  const roots = rootsRead?.roots ?? []
  /** Only accepted proposals carry a `created_rule_id` — the badge's whole input. */
  const proposals = acceptedRead?.proposals ?? []
  /**
   * Why the body cannot be drawn: the failure of a read that failed with
   * nothing kept, the first in read order if more than one did. A read that
   * failed beside a kept answer is not what holds the body back. A re-read
   * that fails with the body drawn says nothing: what is drawn stays, and
   * the next write reads again.
   */
  const unread =
    listed === undefined
      ? failures.find((why, i) => why !== undefined && answers[i] === undefined)
      : undefined
  const [selected, setSelected] = useState<number | undefined>()
  /** The working copy: what the editor draws, previews and saves. */
  const [rules, setRules] = useState<Rule[]>(listed ?? [])
  /**
   * The list the server last answered with, as the editor took it: the
   * store's, or a write's answer. **The working copy is edited while it is
   * not the same as this** (`sameRules`): a gesture that changed nothing,
   * or one undone, leaves nothing edited.
   */
  const [base, setBase] = useState(listed)
  const edited = base !== undefined && !sameRules(rules, base)
  /**
   * The stored list the working copy last took or found the same, or that
   * a write's answer superseded: a stored list other than this is taken,
   * or found the same, once nothing is edited and no write is out.
   */
  const [seen, setSeen] = useState(listed)
  /** `listed` as last drawn, for a write's answer to supersede. */
  const listedNow = useRef(listed)
  listedNow.current = listed
  /**
   * The stored list the working copy was last taken from, with the rule
   * then open: each is previewed once it is drawn.
   */
  const [taken, setTaken] = useState<
    { rules: Rule[]; open: number | undefined } | undefined
  >(listed === undefined ? undefined : { rules: listed, open: undefined })
  /**
   * Part of the form's key: counts the times the open rule changed other
   * than through the form's own commit, so the form draws the rule it shows
   * rather than a draft seeded from the one before, which a blur would
   * write back.
   */
  const [formMade, setFormMade] = useState(0)
  const [preview, setPreview] = useState<Preview | undefined>()
  /**
   * A preview is out. True from the first render: the first preview is sent
   * once the body is drawn, and until it answers the pane draws no "No
   * preview yet", and "Previewing…" once the wait has held.
   */
  const [busy, setBusy] = useState(true)
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * Which proposals the last import reopened, if any. A slot of its own,
   * not `failure`: `sendImport` previews the imported list once it lands
   * (`commit`), and a preview that succeeds clears `failure`, so a notice
   * stored there would be wiped by the call that follows it, almost always
   * before a person could read it. Naming which proposals reopened is the
   * whole reason the import route reports them at all; it needs a slot
   * `runPreview` does not touch. Cleared only by the next import.
   */
  const [importNotice, setImportNotice] = useState<string | undefined>()
  /**
   * Set when a landed save's own impact differs from the preview it was made
   * from. Separate from `failure` for the reason this file's own top comment
   * gives: the follow-up preview `save` triggers on landing clears `failure`
   * when it succeeds, and a warning stored there would be wiped by the very
   * preview run to produce a true number for it. Cleared only by the next save.
   */
  const [saveMismatch, setSaveMismatch] = useState<string | undefined>()
  /**
   * Set when a `RuleForm` commit arrives while a save is in flight. Its own
   * slot for the reason `saveMismatch` and `importNotice` each have one: the
   * follow-up preview clears `failure` when it succeeds, and a notice stored
   * there is wiped by the very call that follows it. Cleared by the next save
   * and by `commit`. Spec §5, and §5.1 for why the *failed*-save reason lands
   * here too.
   */
  const [heldNotice, setHeldNotice] = useState<string | undefined>()
  /**
   * A gesture refused or discarded because a write was in flight.
   *
   * **Its own slot, because neither existing one can hold it.** `heldNotice` is
   * cleared by `commit`, by design and with its own comment saying why — and
   * the import's success path calls `commit`, so a refusal written there is
   * wiped by the very event it has to outlive. `importNotice` is cleared up
   * front by `beginImport` and overwritten by `sendImport`'s answer, so a
   * refusal written there survives until the import lands and then disappears,
   * leaving the person told the import worked and never told their own gesture
   * did nothing (O100).
   *
   * This is the fourth notice on this screen needing a slot `runPreview` does
   * not touch, after `importNotice`, `saveMismatch` and `heldNotice`, and the
   * pattern is worth naming: a message about a gesture that did *not* happen
   * cannot live in a slot owned by the gesture that did.
   *
   * Cleared when a write actually starts, which is the moment its claim stops
   * being true — and deliberately not by `commit`.
   */
  const [refused, setRefused] = useState<string | undefined>()
  const [saved, setSaved] = useState<Saved | undefined>()
  /** The preview whose impact `saved` is compared against. */
  const predicted = useRef<Preview | undefined>(undefined)
  /** The newest preview sent — see this file's own doc comment. */
  const latest = useRef(0)
  /** One of this page's previews is out. */
  const previewOut = useRef(false)
  /** The list a gesture made while a preview was out, sent as it settles. */
  const queued = useRef<{ rules: Rule[]; open: number | undefined }>(undefined)
  /** The synchronous gate — read before either writer starts. */
  const writer = useRef<Writer>(undefined)
  /** The same fact, for the render. */
  const [writing, setWriting] = useState<Writer>(undefined)
  /** A save specifically — what the Save button's own label reads. */
  const saving = writing === 'save'
  /** Any write at all — what every frozen surface reads. */
  const frozen = writing !== undefined
  // Taken while rendering, so the render that first draws the body draws
  // the stored rules rather than an empty list. A later stored list that
  // differs replaces the working copy only while nothing is edited, since
  // an edit is the person's and Save is how it reaches the store, and only
  // while no write is out, whose answer replaces the list itself. One that
  // came while either held is taken once neither does, unless a write's
  // answer marked it seen (the spec's §9, known limit 5).
  if (listed !== undefined && listed !== seen && !edited && !frozen) {
    setSeen(listed)
    if (base === undefined || !sameRules(listed, rules)) {
      const open =
        selected !== undefined && selected < listed.length
          ? selected
          : undefined
      if (open !== undefined && !sameRule(rules[open], listed[open])) {
        setFormMade((made) => made + 1)
      }
      setRules(listed)
      setBase(listed)
      setSelected(open)
      setTaken({ rules: listed, open })
    }
  }
  /** The previews and saves sent from here, each of which takes the lease. */
  const own = useOwnRequests()
  const hold = useDeriveHold(true, own)
  const { askAgain } = hold
  /** A derive holds the lease, for which the server refuses a preview. */
  const previewHeld = hold.derive
  /** `previewHeld` as last drawn, for a gesture's preview to read. */
  const previewHeldNow = useRef(previewHeld)
  previewHeldNow.current = previewHeld
  /** A derive or the job lock holds, for which the server refuses a save. */
  const jobHeld = hold.derive || hold.job
  /**
   * A preview the list on screen is owed: one a hold kept back or the
   * server refused, sent once a read finds no derive holding.
   */
  const owed = useRef(false)
  const rulesNow = useRef(rules)
  rulesNow.current = rules
  const selectedNow = useRef(selected)
  selectedNow.current = selected
  const saveWaitsId = useId()
  /**
   * Save is held: a write is out, a preview is out (see this file's own top
   * comment), no preview stands to compare the save against, or a derive or
   * the job lock holds. `save` turns a press down on this from the send; the
   * button is drawn held from `saveHeldDrawn`.
   */
  const saveHeld = frozen || busy || preview === undefined || jobHeld
  /**
   * A write or a preview, once it has lasted long enough to be drawn. One
   * wait and not two, because a save's answer starts the preview of what it
   * saved in the same render: Save stays drawn held from the one to the
   * other rather than flashing free between them.
   */
  const waitDrawn = useDrawnWait(frozen || busy)
  /**
   * No preview to save against is not a wait, so it is drawn held at once:
   * on the first draw, before the first preview has landed, and after a
   * preview has failed.
   */
  const saveHeldDrawn = waitDrawn || preview === undefined || jobHeld
  /** A write, once it has lasted: Import's held look, and each one's words. */
  const frozenDrawn = useDrawnWait(frozen)
  /** A preview, once it has lasted: "Previewing…". */
  const previewingDrawn = useDrawnWait(busy)
  /**
   * A `RuleForm` commit that arrived while a save was in flight — O96.
   *
   * The list is frozen for the duration (`RuleList`'s `frozen` prop), so
   * `selected` cannot move under this and at most one rule can be edited.
   * Held rather than applied, because `save`'s own `.then` replaces the whole
   * list with `answer.rules`; applied on landing, onto the server's row, so
   * the minted id survives.
   *
   * **The form is not the only surface left live — Import is too, and it is
   * *refused* rather than held.** An import cannot be held: it has already
   * written the rule table by the time anything here could apply it, so it
   * refuses inside the window and says so — in `refused` (O100).
   */
  const held = useRef<{ index: number; rule: Rule } | undefined>(undefined)
  /** The hidden file input the Import button triggers. */
  const fileInput = useRef<HTMLInputElement>(null)

  const runPreview = useCallback(
    (next: Rule[], open: number | undefined) => {
      if (previewHeldNow.current) {
        owed.current = true
        return
      }
      // This one previews the list on screen, which is all that was owed.
      owed.current = false
      if (previewOut.current) {
        queued.current = { rules: next, open }
        setBusy(true)
        return
      }
      const seq = ++latest.current
      previewOut.current = true
      setBusy(true)
      const settled = () => {
        previewOut.current = false
        const after = queued.current
        queued.current = undefined
        if (after !== undefined) runPreview(after.rules, after.open)
      }
      own
        .track(previewRules(next, open))
        .then((body) => {
          settled()
          if (seq !== latest.current) return
          setPreview(body)
          predicted.current = body
          setBusy(false)
          setFailure(undefined)
        })
        .catch((err: unknown) => {
          settled()
          if (seq !== latest.current) return
          setBusy(false)
          if (statusOf(messageOf(err)) === 409) {
            owed.current = true
            askAgain()
          } else {
            setFailure(`The rules were not previewed: ${reasonOf(err)}`)
          }
          // A failed preview describes nothing — leaving the last *successful*
          // preview in place would leave Save free and stating a number
          // computed for a rule list that is no longer the one on screen.
          // `preview === undefined` is what `saveHeld` already reads, so
          // invalidating both here is enough to hold Save without a second
          // condition to keep in sync.
          setPreview(undefined)
          predicted.current = undefined
        })
    },
    [askAgain, own],
  )

  useEffect(() => {
    if (taken !== undefined) runPreview(taken.rules, taken.open)
  }, [taken, runPreview])

  /** Whether the last read held the preview, as this effect last saw it. */
  const previewWasHeld = useRef(false)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `hold.reads` is a trigger: a read that finds no derive sends what a refusal owed.
  useEffect(() => {
    const was = previewWasHeld.current
    previewWasHeld.current = previewHeld
    if (previewHeld) {
      if (was) return
      // The derive changes what any preview on screen or out describes.
      latest.current += 1
      setBusy(false)
      setPreview(undefined)
      predicted.current = undefined
      owed.current = true
      return
    }
    if (!owed.current) return
    owed.current = false
    runPreview(rulesNow.current, selectedNow.current)
  }, [previewHeld, hold.reads, runPreview])

  /** Every editor gesture lands here: set the rules, then preview once. */
  const commit = (next: Rule[], open: number | undefined) => {
    setRules(next)
    setSelected(open)
    setSaved(undefined)
    // O96: the held-edit notice names a rule by *position* — "your edit to rule
    // 4" — and says it is the list on screen. Any mutation can make both halves
    // false: delete rule 2 and rule 4 is a different rule. `RuleList`'s confirm
    // dialog does the same, closing on every list change rather than
    // re-deriving where the row went, and for the same reason. The failed-save
    // path below deliberately sets the notice *after* calling this, so its own
    // message survives the clear.
    setHeldNotice(undefined)
    runPreview(next, open)
  }

  const save = () => {
    // A press on a held Save does nothing and says nothing, whether or not
    // the hold is drawn yet: the button is `aria-disabled`, which keeps
    // focus on it (`HeldButton`), so the press reaches this handler, and
    // the hold is drawn only once the wait has lasted.
    if (saveHeld) return
    if (writer.current !== undefined) {
      // **Two presses in one batch, before the render that holds Save.**
      // `beginImport` takes the gate inside the discrete change event, so
      // `frozen` and `saveHeld` are true by the time a click can be
      // dispatched, and the import case below is not reachable through the
      // UI (O101). What is stated is the invariant, not a path: a writer that
      // took the gate outside a discrete event would reopen that window, and
      // would reopen it here rather than in `save`.
      //
      // Only the import case says anything: a second Save during a save is the
      // same gesture twice and the person has nothing to act on.
      if (writer.current === 'import')
        setRefused(
          'An import was still running, so the save was not sent — nothing was saved ' +
            'and nothing was changed. Save again once the import has finished and its ' +
            'rules are on screen.',
        )
      return
    }
    writer.current = 'save'
    setWriting('save')
    setRefused(undefined)
    setFailure(undefined)
    setSaveMismatch(undefined)
    setHeldNotice(undefined)
    const made = predicted.current
    own
      .track(saveRules(rules))
      .then((answer) => {
        writer.current = undefined
        setWriting(undefined)
        setSaved(answer)
        if (made !== undefined && impactChanged(made.impact, answer.impact))
          setSaveMismatch(
            'The save differs from the preview it was made from. Something changed the library ' +
              'between the two — the numbers below are what actually happened.',
          )
        // O96: a commit the form made while this save was in flight is
        // applied *on top of* the answer rather than discarded by it.
        // `answer.rules` is positionally aligned to the list that was sent —
        // `rules::replace_all` preserves position and mints an id only for a
        // zero one — and `RuleList` was frozen for the duration, so `index`
        // still names the same rule. **`id` and `ordinal` come from the
        // answer** for that reason: carrying the edit's own `id: 0` forward
        // would mint a *second* row on the next save and orphan
        // `created_rule_id` on any accepted proposal that wrote the first,
        // reopening it for nothing — the very reopening `save_rules`' own doc
        // comment contrasts against `import_rules`.
        const pending = held.current
        held.current = undefined
        const next =
          pending === undefined
            ? answer.rules
            : answer.rules.map((rule, i) =>
                i === pending.index
                  ? { ...pending.rule, id: rule.id, ordinal: rule.ordinal }
                  : rule,
              )
        setRules(next)
        setBase(answer.rules)
        // A stored list read before this answer is taken to be older than
        // it, though another writer between can make it newer (the spec's
        // §9, known limit 5).
        setSeen(listedNow.current)
        if (pending !== undefined)
          setHeldNotice(
            `Your edit to rule ${pending.index + 1} arrived while the save was running, so ` +
              'it is not in the save reported below — it is applied to the list on screen ' +
              'and is unsaved. Save again to apply it.',
          )
        // The button's number has to describe *this* saved list, not the one a
        // save ago — so a fresh preview runs for it here, through the same
        // seq-guarded `runPreview` every other gesture uses, and Save stays
        // held (`busy`) until it lands. `selected` is not stale: `RuleList` is
        // frozen (O96), and `PreviewTabs`' two options are held on `busy ||
        // frozen` (O95), so no surface can change it while a save is in
        // flight. That is a property of *every* such surface being guarded,
        // not of this line — a new unguarded one reopens the defect without
        // touching `save`.
        runPreview(next, selected)
        // The list just changed under every accepted proposal's
        // `created_rule_id`, so the badges are re-read rather than assumed.
        void invalidate(RULES, ACCEPTED_PROPOSALS)
        // The save derived the library again, which can fold works or make
        // them, so the works Libraries counts for each library are re-read,
        // and the Library views, which draw those works, are marked.
        void invalidate(LIBRARIES)
        invalidateLibraryViews()
      })
      .catch((err: unknown) => {
        writer.current = undefined
        setWriting(undefined)
        // A refusal for a hold no read here has seen: read it, so it is drawn.
        if (statusOf(messageOf(err)) === 409) askAgain()
        const pending = held.current
        held.current = undefined
        if (pending === undefined) {
          // Nothing follows this, so `failure` survives to be read.
          setFailure(`The rules were not saved: ${reasonOf(err)}`)
          return
        }
        // O96, spec §5.1: the save wrote nothing, so the list is still the
        // one that was sent and the held edit applies to it unchanged — no id
        // came back to adopt. The failure text goes to `heldNotice` rather
        // than `failure` because `commit` fires a preview whose success path
        // calls `setFailure(undefined)`, and the reason a person most needs
        // to read here is the 409 `save_rules` answers while another library
        // job holds `library_job` (the serve crate's `LibraryJob` names them).
        setFailure(undefined)
        commit(
          rules.map((rule, i) => (i === pending.index ? pending.rule : rule)),
          selected,
        )
        // **After `commit`, not before** — `commit` clears `heldNotice` (see
        // its own comment), so setting this first would have it cleared by
        // the very call meant to apply the edit it describes. Both land in
        // one React batch and the last write wins.
        setHeldNotice(
          `The rules were not saved: ${reasonOf(err)}. Your edit to rule ` +
            `${pending.index + 1} arrived while the save was running — it is applied ` +
            'to the list on screen and is unsaved.',
        )
      })
  }

  /**
   * The whole import gesture, from the picked file rather than from its text —
   * **the gate is taken before the read, not after it** (O101). Taken inside a
   * `file.text().then(...)`, it would leave Import enabled and `writer` free
   * between picking a file and its bytes arriving: a microtask on a small
   * document, and however long the read takes on a large one, with nothing on
   * screen saying the gesture had started. That is also the route by which a
   * second pick reaches a gate meant to be held. Taking it earlier means the
   * read's *own* failure has to release it.
   *
   * Typed by what it needs — `text()` — rather than as `File`. The gesture
   * does not care where the bytes come from, and the narrower type is what
   * lets a test hold the read open, which is the window this function exists
   * to cover.
   */
  const beginImport = (file: { text: () => Promise<string> }) => {
    // Cleared up front, so a notice from a previous import cannot outlive
    // this one, whichever way it resolves: a refused import sets no notice
    // of its own.
    setImportNotice(undefined)
    // O96, and **the second of two guards**, for the reason the drag has two:
    // the Import button is held while a write is in flight, but a file
    // picker opened *before* that write started is still open and can still
    // deliver a file during it. This refuses rather than holds, and that is
    // forced rather than chosen — `POST .../rules/import` **writes the rule
    // table**, so by the time a held import could be applied the write has
    // already happened. Import is the one gesture on this screen whose loss
    // outlives the session: landing inside the save window would leave the
    // store holding the imported rules, the screen showing the pre-import
    // list `setRules` puts back, and a notice saying the import worked —
    // and the next Save would write the screen's list back over it, reopening
    // every proposal a second time.
    //
    // **Both refusals go to `refused`, and the import case is why** (O100).
    // `importNotice` is the slot the *running* import's own success path
    // overwrites, so a sentence there saying a second pick was refused would be
    // replaced, milliseconds later, by one saying an import had worked, and the
    // person would be left believing the file they picked second was the one in
    // the store. That is the failure §5 of the build design names as its whole
    // reason for creating `refused`. The save case goes there with it, because
    // a message about a gesture that did *not* happen cannot live in a slot
    // owned by the gesture that did.
    if (writer.current !== undefined) {
      setRefused(
        writer.current === 'save'
          ? 'A save was still running, so the import was not applied — nothing was ' +
              'imported and nothing was changed. Try it again once the save has finished.'
          : 'An import was already running, so this one was not applied — nothing ' +
              'was imported twice. Try it again once the first has finished.',
      )
      return
    }
    writer.current = 'import'
    setWriting('import')
    setRefused(undefined)
    const release = () => {
      writer.current = undefined
      setWriting(undefined)
    }
    file
      .text()
      .then((text) => sendImport(text, release))
      // **The read's own failure**, which only exists because the gate is
      // taken before it: a file the browser cannot read would otherwise leave
      // both writers dead for the session with a message and no way back.
      .catch((err: unknown) => {
        release()
        setFailure(`The rules were not imported: ${reasonOf(err)}`)
      })
  }

  /**
   * The request half, once the document's text has arrived and the gate is
   * already held. Separate from `beginImport` so the gesture's failure shapes
   * stay legible, and they are **three, caught in two places**: the read's
   * rejection and `importRules`' *synchronous* throw both land in
   * `beginImport`'s `.catch`, because this whole function is called from
   * inside a `.then`; the request's own rejection lands here.
   */
  const sendImport = (text: string, release: () => void) =>
    // **`importRules` throws synchronously and no `.catch` of its own can see
    // it.** It is not `async`: its body is
    // `expectBody(sendJson(..., documentOf(doc)))`, and the parse runs as
    // an *argument* — so a document that is not valid JSON throws out of the
    // call itself, before any promise exists for `.then`/`.catch` to attach
    // to. What catches it is that this call is made **inside** a `.then`,
    // where a synchronous throw is a rejected promise; the O97 test that
    // drives a non-JSON file through the input is what keeps it caught if
    // anyone moves the call out of the chain.
    importRules(text)
      .then((answer) => {
        release()
        // Every other editor gesture lands in `commit`; this is the shape it
        // already has (set the rules, close/leave the selection, drop the
        // last save, preview once), so it goes through the same funnel
        // rather than repeating it.
        commit(answer.rules, undefined)
        setBase(answer.rules)
        setSeen(listedNow.current)
        // The import replaced every rule and reopened the accepted proposals
        // whose rules it replaced.
        void invalidate(RULES, ACCEPTED_PROPOSALS)
        // `POST .../rules/import` writes the rule table and derives nothing —
        // the CLI's own `library rules --import` prints exactly this warning
        // for the identical route (`library.rs`). Saying nothing would leave
        // the imported list looking applied the moment it appeared, when in
        // fact the library's structure is still the old rules' until something
        // derives. The remedy named here is Save, not a scan command: on this
        // screen Save is the thing that derives.
        const reopened =
          answer.reopened_proposals.length > 0
            ? ` It also reopened ${answer.reopened_proposals.length} accepted ` +
              `proposals: ${answer.reopened_proposals.join(', ')} — an imported ` +
              'document carries none of this library’s rule ids, so every rule ' +
              'it replaces is a new row.'
            : ''
        setImportNotice(
          'The imported rules are stored, but the library has not been re-derived ' +
            'from them yet — its structure still reflects the rules from before this ' +
            `import. Save applies them.${reopened}`,
        )
      })
      .catch((err: unknown) => {
        // **The gate is cleared here too, and this is not symmetry for its
        // own sake.** A failed import that kept `writer.current` set would
        // hold Save *and* Import for the rest of the session, with the
        // screen showing only the server's sentence about the document — a
        // refused import is the ordinary case (`rules/import` rejects a
        // malformed document), so the leak would be reached by the first
        // person who picked the wrong file.
        release()
        setFailure(`The rules were not imported: ${reasonOf(err)}`)
      })

  const exportDoc = () => {
    try {
      return JSON.stringify(toDocument(rules, roots), null, 2)
    } catch (err: unknown) {
      // **Guarded, not called unconditionally.** This runs inline in the
      // Export link's `href` on every render, so calling `setFailure` without
      // checking against the current value fires again on the very next
      // render it causes — for as long as a rule names a root this library
      // does not have, which is exactly as long as this catch keeps being
      // reached. React stops a render that keeps updating its own state with
      // "Too many re-renders", which takes the whole screen down instead of
      // saying why the rules cannot be exported.
      const reason = `The rules cannot be exported: ${reasonOf(err)}`
      if (reason !== failure) setFailure(reason)
      return ''
    }
  }

  return (
    <section className="rule-editor">
      <PageHeader title="Rules" />
      {unread !== undefined && (
        <p role="alert">{`The rules were not loaded: ${reasonOf(unread)}`}</p>
      )}
      {failure !== undefined && <p role="alert">{failure}</p>}
      {hold.failure !== undefined && <p role="alert">{hold.failure}</p>}
      {saveMismatch !== undefined && (
        <p role="alert" className="save-mismatch">
          {saveMismatch}
        </p>
      )}
      {heldNotice !== undefined && (
        <p role="alert" className="held-edit">
          {heldNotice}
        </p>
      )}

      {refused !== undefined && (
        <p role="alert" className="refused">
          {refused}
        </p>
      )}
      {listed === undefined && <PageLoading drawn={loadingDrawn} />}

      {listed !== undefined && (
        <>
          <div className="editor-panes">
            <Section>
              <RuleList
                rules={rules}
                selected={selected}
                proposals={proposals}
                overrideCount={preview?.overrides.length ?? 0}
                frozen={frozen}
                onSelect={(index) => {
                  // The design runs a preview on selecting a *different* rule —
                  // `rows` is scoped to `selected`, and changing which is open
                  // is what repopulates it. Re-clicking the one already open
                  // changes nothing a preview could answer differently, so
                  // committing anyway would pay another derivation under the
                  // write lock and clear the "Saved:" line for no reason.
                  if (index !== selected) commit(rules, index)
                }}
                onToggle={(index) => {
                  if (index === selected) setFormMade((made) => made + 1)
                  commit(
                    rules.map((rule, i) =>
                      i === index ? { ...rule, enabled: !rule.enabled } : rule,
                    ),
                    selected,
                  )
                }}
                onReorder={(from, to) => {
                  const next = renumber(moved(rules, from, to))
                  // The selected rule travels with the drag rather than staying at
                  // an index that now holds a different rule.
                  const open =
                    selected === undefined
                      ? undefined
                      : selected === from
                        ? to
                        : selected
                  commit(next, open)
                }}
                onAdd={() => {
                  const next = renumber([...rules, blankRule(rules.length + 1)])
                  commit(next, next.length - 1)
                }}
                onDelete={(index) => {
                  const next = renumber(rules.filter((_, i) => i !== index))
                  commit(
                    next,
                    selected === undefined || selected === index
                      ? undefined
                      : selected > index
                        ? selected - 1
                        : selected,
                  )
                }}
              />
            </Section>

            <Section>
              {selected !== undefined && rules[selected] !== undefined ? (
                <RuleForm
                  // Local draft state lives in the form; a new key is what makes
                  // selecting a different rule redraw rather than keep the last
                  // one's half-typed text, and what makes the open rule changed
                  // from elsewhere (`formMade`) redraw rather than keep a draft
                  // of what it was.
                  key={`${formMade}:${selected}:${rules[selected].id}`}
                  rule={rules[selected]}
                  roots={roots}
                  onCommit={(edited) => {
                    // O96: `RuleForm` is deliberately not frozen — disabling a
                    // focused input makes the browser fire `blur`, and this form
                    // commits on blur, so the mechanical guard would itself
                    // commit the half-typed edit into the very window the item is
                    // about. So the field stays live and the *commit* waits.
                    if (writer.current === 'save') {
                      held.current = { index: selected, rule: edited }
                      return
                    }
                    if (writer.current === 'import') {
                      // **Discarded, not held**, and forced rather than chosen.
                      //
                      // The last clause of the notice is deliberately
                      // outcome-neutral: a refused document is the ordinary
                      // failure here, so the sentence has to hold for both
                      // endings. (**That the edit is discarded even when the
                      // import fails is a real asymmetry with O96's failed
                      // *save*, which keeps it** — the reasoning below only
                      // holds for an import that succeeded.) Holding works for
                      // a save because `answer.rules` is positionally aligned
                      // to the list that was sent. An import's answer is
                      // aligned to nothing — it replaces every rule and mints
                      // fresh ids — so this index does not name the rule being
                      // edited and applying the edit there would overwrite an
                      // unrelated rule with it.
                      setRefused(
                        `An import was running, so your edit to rule ${selected + 1} was not ` +
                          'applied — an import replaces every rule, so there is no row left to ' +
                          'apply it to. Make the edit again once the import has finished.',
                      )
                      return
                    }
                    commit(
                      rules.map((rule, i) => (i === selected ? edited : rule)),
                      selected,
                    )
                  }}
                />
              ) : (
                <p className="empty">
                  Pick a rule to edit it. The impact below is for the whole
                  list.
                </p>
              )}
            </Section>
          </div>

          <Section
            title="Preview"
            // On the heading's line, where there is room for it, and laid out
            // unseen until the preview has lasted, so drawing it moves nothing:
            // above the tabs' panel it would push what the panel shows down a
            // line and back up.
            actions={
              <p className={previewingDrawn ? 'busy' : 'busy wait-unseen'}>
                Previewing…
              </p>
            }
          >
            <PreviewTabs
              preview={preview}
              rules={rules}
              selected={selected}
              busy={busy}
              frozen={frozen}
              held={previewHeld ? JOB_WAITS : undefined}
              onReorder={(from, to) => {
                const next = renumber(moved(rules, from, to))
                commit(next, selected === from ? to : selected)
              }}
              // The same call `RuleList`'s own `onSelect` makes, guard included:
              // "Edit rule 4" from an override row is selecting rule 4, so it
              // costs one derivation, and selecting the rule already open costs
              // none.
              onSelectRule={(index) => {
                if (index !== selected) commit(rules, index)
              }}
            />
          </Section>

          {saved !== undefined && (
            <p className="saved">
              Saved: {saved.impact.works_rehomed} works re-homed,{' '}
              {saved.impact.series_merged} series merged.
              {saved.reopened_proposals.length > 0 &&
                ` Reopened proposals ${saved.reopened_proposals.join(', ')}.`}
            </p>
          )}
          {saved?.complaints.map((complaint) => (
            <p className="complaint" key={complaint}>
              {complaint}
            </p>
          ))}
          {saved !== undefined && (
            // Context: the save's own re-derive touches `<root>/.taiyaku/`,
            // the one place on disk this route writes, and `Impact` says
            // nothing about it, so a save that could not write a hand document
            // would answer with only a rehome count and silence.
            // `DeriveSection` is the renderer `ScanReport.tsx` already has for
            // this exact struct, reused rather than a second, narrower
            // vocabulary invented here; collapsed by default the same way
            // `RemoveRoot` reuses it, since most saves have nothing in these
            // fields worth a person's first glance.
            <details className="save-derive">
              <summary>What this save's re-derive found</summary>
              {/* h3, beside Preview beneath Rules: the save's report sits after
              the preview's box, not in it, so it is not one of its parts. */}
              <DeriveSection derive={saved.derive} level={3} />
            </details>
          )}

          <div className="editor-actions">
            {/*
             * Held on `busy` too, not only `frozen` — see this file's own top
             * comment. Without it, a click landing while a preview is out
             * sends `rules` against a `predicted.current` that has
             * not caught up yet, and the mismatch draws a warning that blames
             * the server for a race the client caused. Its words and Import's
             * keep the box of the wider of their two, so drawing a wait moves
             * nothing beside them.
             */}
            <HeldButton
              type="button"
              className="save keeps-wait-width"
              held={saveHeldDrawn}
              onClick={save}
              aria-describedby={jobHeld && !frozen ? saveWaitsId : undefined}
            >
              <WaitWords
                drawn={saving && frozenDrawn}
                words={`Save — apply to ${preview?.impact.works_rehomed ?? 0} works`}
                wait="Saving…"
              />
            </HeldButton>
            {jobHeld && !frozen && (
              <p id={saveWaitsId} className="save-waits">
                {JOB_WAITS}
              </p>
            )}

            <div className="import">
              <HeldButton
                type="button"
                className="keeps-wait-width"
                held={frozenDrawn}
                onClick={() => {
                  // Held from the send: a picker opened now would deliver a
                  // file `beginImport` can only refuse.
                  if (frozen) return
                  fileInput.current?.click()
                }}
              >
                <WaitWords
                  drawn={writing === 'import' && frozenDrawn}
                  words="Import"
                  wait="Importing…"
                />
              </HeldButton>
              <p>
                Import replaces every rule with the document’s, minting fresh
                ids, which reopens every accepted proposal whose rule it
                replaces.
              </p>
              {importNotice !== undefined && <p role="alert">{importNotice}</p>}
              <input
                ref={fileInput}
                type="file"
                accept="application/json"
                hidden
                onChange={(e: ChangeEvent<HTMLInputElement>) => {
                  const file = e.target.files?.[0]
                  e.target.value = ''
                  if (file === undefined) return
                  // The whole gesture, file and all — `beginImport` takes the
                  // gate before it reads, so this handler owns no part of it.
                  beginImport(file)
                }}
              />
            </div>

            <a
              className="export"
              download="rules.json"
              href={`data:application/json,${encodeURIComponent(exportDoc())}`}
            >
              Export
            </a>
          </div>
        </>
      )}
    </section>
  )
}
