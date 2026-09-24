/**
 * The rule editor — spec C4 §1, and the build design's §5. The one screen
 * Komga structurally cannot have.
 *
 * **It previews on commit and never while typing.** A preview costs a median
 * 281 ms on a real library (130 series, 1858 works) and 495 ms with a rule
 * appended, with the diff 1.3 ms of it — the derivation is the entire cost.
 * So a preview runs on: a field blur or Enter, an enable toggle, a
 * drag-reorder, an add, a delete, and selecting a different rule. The last is
 * the one that is not obvious: `rows` is scoped to the selected rule, so
 * changing which rule is open is the only way to populate the Matched files
 * and Provenance tabs for it — and it is one gesture, not a stream. A person
 * clicking down a five-rule list pays five derivations; a person typing a
 * regex pays none. The alternatives — an explicit Preview button, a long
 * typing debounce — are weighed and refused in the build design's §5.3.
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
 * **Save is disabled while a preview is in flight, not only while saving.**
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
 * **A sequence number on the preview, because responses need not arrive in
 * order.** Several commits can be in flight — a blur that starts a preview,
 * then a click on another rule before it lands — and without this an earlier
 * response arriving later would write a stale impact under a Save button that
 * states a number from it. `Roots.tsx` and `Search.tsx` carry the same
 * counter for the same reason.
 *
 * **A successful save re-previews the list it just saved, and Save stays
 * disabled until that lands.** Whole-branch review, I1. `save` does not
 * touch `preview`, so without this the button kept stating the *pre-save*
 * `works_rehomed` after the library had already been re-homed to match it —
 * and a second click sent an already-derived library through `apply` again,
 * where every `Impact` field comes back `0` against a `predicted.current`
 * that still held the first save's number. That mismatch drew "the save
 * differs from the preview it was made from", blaming the server for a race
 * the client caused, one click after the in-flight-preview fix closed the
 * same failure for every other gesture. The mismatch warning itself is kept
 * off `failure` and in its own `saveMismatch` slot for the identical reason
 * `importNotice` is kept off it: the follow-up preview this fix triggers
 * clears `failure` on landing, and a warning stored there would be wiped by
 * the very preview run to produce the number it is standing next to.
 *
 * **A failed preview invalidates `preview` and `predicted.current`, not only
 * `busy`.** Whole-branch review, I2. Without this, a preview that failed —
 * a network blip, a 500, the 400 the store raises for a rule id another
 * admin's edit just deleted — left both at whatever the *last successful*
 * preview computed, for a rule list that is no longer the one on screen. The
 * button stayed enabled, stated that stale number, and a click sent the
 * current (unpreviewed) list to `apply` — which then disagreed with the
 * stale `predicted.current` and drew the same "something changed the
 * library" warning, again blaming the server for a client-side gap.
 *
 * **One writer at a time — O96, then O97.** This screen has two writers of
 * the rule table, Save and Import, and `writer` is the single gate both take:
 * a `'save' | 'import' | undefined` ref, read synchronously, mirrored into
 * `writing` for the render. It is three-valued rather than a boolean each
 * because `saving && importing` must never exist, and O96 is what two
 * booleans agreeing by hand cost — it guarded an import during a save and
 * left a save during an import, which became O97.
 *
 * **Every write's answer replaces the whole list**, so a gesture committed
 * inside either window was applied, previewed and silently discarded.
 * `RuleList` and `PreviewTabs`' two override options freeze on `frozen`
 * (`writing !== undefined`), which is why the prop is not called `saving`:
 * it is true during an import too, and a name that stopped being true is the
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
 * **Both writers refuse rather than queue, in both directions** — spec §2.1
 * and O97's §1. Each writes the rule table, so by the time a queued one could
 * run the other's write has already happened. **Import needs two guards** for
 * the drag's reason: the button's `disabled` lands on a render, and the
 * hidden file input delivers whatever the button says, because a picker
 * opened before the write started is still open. **Save's second guard is now
 * defence rather than cover**, and O101 is why: the import gate used to be
 * taken inside `file.text()`'s `.then`, a microtask after the change event
 * returned, so a click could reach Save with the button still drawn enabled.
 * `beginImport` takes it inside the discrete event itself, which React
 * flushes synchronously, so `frozen` and `writer.current` change together and
 * no click can land between them. The check stays — the invariant is what is
 * being stated, and a writer that took the gate outside a discrete event
 * would reopen the gap without touching `save` — but nothing exercises it,
 * and its message is unreachable with it.
 *
 * **Every refusal goes to `refused`**, which `commit` does not clear:
 * a refused save, a form commit discarded by an import, and — since O100 —
 * an import refused because a write was already in flight. That last one
 * lived in `importNotice` and was erased by the running import's own answer,
 * which is precisely the failure `refused` was created to prevent, in the
 * function the reasoning was written for. A *failed* save keeps the edit and
 * puts its reason in `heldNotice` rather than `failure` — spec §5.1 — and a
 * *failed* import clears the gate on **all three** of its failure shapes,
 * without which one bad file would disable both writers for the rest of the
 * session. They are not one, and they are caught in **two** places: the
 * request's own rejection in `sendImport`'s `.catch`, and the other two in
 * `beginImport`'s. The **read**'s rejection exists only because the gate is
 * now taken before the read. And `importRules` is not `async` — it evaluates
 * `JSON.parse(doc)` as an argument, so a document that is not JSON throws
 * *synchronously*, out of `sendImport` before its own chain is built; what
 * makes that a rejected promise rather than an escaped throw is that
 * `sendImport` is called inside a `.then`, so it lands in `beginImport`'s
 * `.catch` too. Picking a file that is not JSON is the most ordinary way to
 * reach it, and the O97 test that drives one is what keeps it caught.
 *
 * **`selected` in `save`'s closure is no longer stale, and that is a property
 * of the guards rather than of this file.** It used to be: selecting a rule
 * mid-save left the follow-up preview's `rows` scoped to the rule that *was*
 * open while the form showed the new one. Nothing can change `selected`
 * during a save now — `RuleList` is frozen and `PreviewTabs`' two options
 * have been `disabled={busy || frozen}` since O95 — so a `selectedRef` would
 * hide the invariant rather than state it. A new surface that sets `selected`
 * without a guard reopens the defect without touching `save`.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import type { ChangeEvent } from 'react'
import { listRoots, type Root } from './config'
import { listProposals, type Proposal } from './curation'
import { PreviewTabs } from './PreviewTabs'
import { RuleForm } from './RuleForm'
import { RuleList } from './RuleList'
import { DeriveSection } from './ScanReport'
import {
  blankRule,
  type Impact,
  importRules,
  listRules,
  moved,
  type Preview,
  previewRules,
  renumber,
  type Rule,
  type Saved,
  saveRules,
  toDocument,
} from './rules'
import { messageOf } from './message'

/**
 * Field-by-field, matching `RuleForm`'s own dirty-check comment. `Impact` is
 * a flat record of numbers, so `Object.keys` alone is enough to compare it —
 * no nested walk is needed the way a `Rule` compare might.
 */
const impactChanged = (a: Impact, b: Impact) =>
  (Object.keys(a) as (keyof Impact)[]).some((k) => a[k] !== b[k])

/** Only accepted proposals carry a `created_rule_id` — the badge's whole input. */
const acceptedProposals = () =>
  listProposals({ state: 'accepted' }).then((inbox) => inbox.proposals)

/**
 * Which write is in flight, if any. Both writers on this screen — Save and
 * Import — take this gate, and neither runs while the other holds it.
 *
 * **One three-valued variable rather than a boolean each.** Two booleans admit
 * `saving && importing`, a state that must never exist and that four separate
 * conditions would then have to agree about. O96 shipped two guards that
 * agreed by hand and O97 is what that cost — a save started during an import,
 * which no condition was asked about. This is #115's `phaseOf` lesson one size
 * down: when every fix moves one condition and the gates then disagree, the
 * defect is the absence of a single source of truth, not the gates.
 */
type Writer = 'save' | 'import' | undefined

export function RuleEditor() {
  const [rules, setRules] = useState<Rule[]>([])
  const [roots, setRoots] = useState<Root[]>([])
  const [proposals, setProposals] = useState<Proposal[]>([])
  const [selected, setSelected] = useState<number | undefined>()
  const [preview, setPreview] = useState<Preview | undefined>()
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * Which proposals the last import reopened, if any. Separate from
   * `failure` for the reason `Roots.tsx`'s `addFailure` is separate from its
   * `failure`, and read that file's own doc comment on the pair: `sendImport`
   * fires a follow-up `runPreview` right after landing, and that preview's
   * success path unconditionally clears `failure` — so a notice stored there
   * was wiped by the very call that had just populated it, almost always
   * before a person could read it. Naming which proposals reopened is the
   * whole reason the import route reports them at all; it needs a slot
   * `runPreview` does not touch. Cleared only by the next import.
   */
  const [importNotice, setImportNotice] = useState<string | undefined>()
  /**
   * Set when a landed save's own impact differs from the preview it was made
   * from. Separate from `failure` for the reason this file's own top comment
   * gives on I1: the follow-up preview `save` triggers on landing clears
   * `failure` when it succeeds, and a warning stored there would be wiped by
   * the very preview run to produce a true number for it. Cleared only by
   * the next save.
   */
  const [saveMismatch, setSaveMismatch] = useState<string | undefined>()
  /**
   * Set when a `RuleForm` commit arrives while a save is in flight. Its own
   * slot for the reason `saveMismatch` and `importNotice` each have one: the
   * follow-up preview clears `failure` when it succeeds, and a notice stored
   * there is wiped by the very call that follows it. Cleared only by the next
   * save. Spec §5, and §5.1 for why the *failed*-save reason lands here too.
   */
  const [heldNotice, setHeldNotice] = useState<string | undefined>()
  /**
   * A gesture refused or discarded because a write was in flight.
   *
   * **Its own slot, because neither existing one can hold it.** `heldNotice`
   * is cleared by `commit`, by design and with its own comment saying why —
   * and the import's success path calls `commit`, so a refusal written there
   * is wiped by the very event it has to outlive. `importNotice` is cleared up
   * front by `beginImport` and overwritten by `sendImport`'s answer, so a
   * refusal written there survives until the import lands and then disappears,
   * leaving the person told the import worked and never told their own gesture
   * did nothing. **That is not hypothetical and it shipped**: `beginImport`'s
   * own refusal for a second pick lived in `importNotice` until O100, and was
   * erased by the running import's answer every time.
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
  /** The newest `previewRules` call — see this file's own doc comment. */
  const latest = useRef(0)
  /** The synchronous gate — read before either writer starts. */
  const writer = useRef<Writer>(undefined)
  /** The same fact, for the render. */
  const [writing, setWriting] = useState<Writer>(undefined)
  /** A save specifically — what the Save button's own label reads. */
  const saving = writing === 'save'
  /** Any write at all — what every frozen surface reads. */
  const frozen = writing !== undefined
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
   * *refused* rather than held.** An earlier version of this comment claimed
   * the form was the only one, which was the guard-that-does-not-exist shape
   * O95's third review pass kept finding. An import cannot be held: it has
   * already written the rule table by the time anything here could apply it,
   * so it refuses inside the window and says so — in `refused`, since O100.
   */
  const held = useRef<{ index: number; rule: Rule } | undefined>(undefined)
  /** The hidden file input the Import button triggers. */
  const fileInput = useRef<HTMLInputElement>(null)

  const runPreview = useCallback((next: Rule[], open: number | undefined) => {
    const seq = ++latest.current
    setBusy(true)
    previewRules(next, open)
      .then((body) => {
        if (seq !== latest.current) return
        setPreview(body)
        predicted.current = body
        setBusy(false)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setBusy(false)
        setFailure(messageOf(err))
        // I2: a failed preview describes nothing — leaving the last
        // *successful* preview in place would keep Save enabled and stating
        // a number computed for a rule list that is no longer the one on
        // screen. `preview === undefined` is what the Save button's own
        // `disabled` already checks, so invalidating both here is enough to
        // disable it without a second condition to keep in sync.
        setPreview(undefined)
        predicted.current = undefined
      })
  }, [])

  useEffect(() => {
    Promise.all([listRules(), listRoots(), acceptedProposals()])
      .then(([listed, rootList, accepted]) => {
        setRules(listed.rules)
        setRoots(rootList.roots)
        setProposals(accepted)
        setFailure(undefined)
        runPreview(listed.rules, undefined)
      })
      .catch((err: unknown) => setFailure(messageOf(err)))
  }, [runPreview])

  /** Every editor gesture lands here: set the rules, then preview once. */
  const commit = (next: Rule[], open: number | undefined) => {
    setRules(next)
    setSelected(open)
    setSaved(undefined)
    // O96: the held-edit notice names a rule by *position* — "your edit to
    // rule 4" — and says it is the list on screen. Any mutation can make both
    // halves false: delete rule 2 and rule 4 is a different rule. This is
    // `RuleList`'s own M3 for its confirm dialog, which closes on every list
    // change rather than re-deriving where the row went, and for the same
    // reason. The failed-save path below deliberately sets the notice *after*
    // calling this, so its own message survives the clear.
    setHeldNotice(undefined)
    runPreview(next, open)
  }

  const save = () => {
    if (writer.current !== undefined) {
      // **Unreachable through the UI since O101, and kept** — see this file's
      // own top comment. `beginImport` takes the gate inside the discrete
      // change event, so `frozen` is already true by the time any click can
      // be dispatched and React skips a click handler whose props say
      // `disabled`. What is stated here is the invariant, not a path: a
      // writer that took the gate outside a discrete event would reopen the
      // window this closed, and would reopen it here rather than in `save`.
      //
      // Only the import case says anything: a second Save during a save is the
      // same gesture twice and the person has nothing to act on, which is the
      // behaviour O96 shipped and this keeps.
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
    saveRules(rules)
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
        if (pending !== undefined)
          setHeldNotice(
            `Your edit to rule ${pending.index + 1} arrived while the save was running, so ` +
              'it is not in the save reported below — it is applied to the list on screen ' +
              'and is unsaved. Save again to apply it.',
          )
        // I1: the button's number has to describe *this* saved list, not the
        // one a save ago — so a fresh preview runs for it here, through the
        // same seq-guarded `runPreview` every other gesture uses, and Save
        // stays disabled (`busy`) until it lands. `selected` is not stale:
        // O96 froze `RuleList`, and `PreviewTabs`' two options have been
        // `disabled={busy || frozen}` since O95, so no surface can change it
        // while a save is in flight. That is a property of *every* such
        // surface being guarded, not of this line — a new unguarded one
        // reopens the defect without touching `save`.
        runPreview(next, selected)
        // The list just changed under every accepted proposal's
        // `created_rule_id`, so the badges are re-read rather than assumed.
        acceptedProposals()
          .then(setProposals)
          .catch(() => {})
      })
      .catch((err: unknown) => {
        writer.current = undefined
        setWriting(undefined)
        const pending = held.current
        held.current = undefined
        if (pending === undefined) {
          // Unchanged: nothing follows this, so `failure` survives to be read.
          setFailure(messageOf(err))
          return
        }
        // O96, spec §5.1: the save wrote nothing, so the list is still the
        // one that was sent and the held edit applies to it unchanged — no id
        // came back to adopt. The failure text goes to `heldNotice` rather
        // than `failure` because `commit` fires a preview whose success path
        // calls `setFailure(undefined)`, and the reason a person most needs
        // to read here is the 409 `save_rules` answers while a scan holds
        // `library_job`.
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
          `The save failed: ${messageOf(err)}. Your edit to rule ${pending.index + 1} ` +
            'arrived while it was running — it is applied to the list on screen and is ' +
            'unsaved.',
        )
      })
  }

  /**
   * The whole import gesture, from the picked file rather than from its text
   * — **the gate is taken before the read, not after it** (O101). It used to
   * be taken inside a `file.text().then(...)`, so between picking a file and
   * its bytes arriving Import was still enabled and `writer` was still free:
   * a microtask on a small document, and however long the read takes on a
   * large one, with nothing on screen saying the gesture had started. That is
   * also the route by which a second pick reaches a gate meant to be held.
   * Taking it earlier means the read's *own* failure has to release it, which
   * is the third instance on this screen of a gate that must not leak.
   *
   * Typed by what it needs — `text()` — rather than as `File`. The gesture
   * does not care where the bytes come from, and the narrower type is what
   * lets a test hold the read open, which is the window this function exists
   * to cover.
   */
  const beginImport = (file: { text: () => Promise<string> }) => {
    // Cleared up front, the same as `Roots.tsx`'s `addFailure` — so a stale
    // notice from a previous import cannot outlive this one, whichever way
    // it resolves.
    setImportNotice(undefined)
    // O96, and **the second of two guards**, for the reason the drag has two:
    // the Import button is `disabled` while a write is in flight, but a file
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
    // They used to go to `importNotice`, which is the slot the *running*
    // import's own success path overwrites — so the sentence saying a second
    // pick was refused was replaced, milliseconds later, by one saying an
    // import had worked, and the person was left believing the file they
    // picked second was the one in the store. That is the failure §5 of the
    // build design names as its whole reason for creating `refused`, reached
    // in the function that reasoning was written for. The save case survived
    // in `importNotice` by luck rather than by design — nothing in `save`
    // writes that state — and is moved with it, because a message about a
    // gesture that did *not* happen cannot live in a slot owned by the
    // gesture that did.
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
      // **The read's own failure**, which only exists because the gate is now
      // taken before it: a file the browser cannot read would otherwise leave
      // both writers dead for the session with a message and no way back.
      .catch((err: unknown) => {
        release()
        setFailure(messageOf(err))
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
    // `expectBody(sendJson(..., JSON.parse(doc)))`, and `JSON.parse` runs as
    // an *argument* — so a document that is not valid JSON throws out of the
    // call itself, before any promise exists for `.then`/`.catch` to attach
    // to. What catches it is that this call is made **inside** a `.then`,
    // where a synchronous throw is a rejected promise; the `try` this used to
    // need went with the restructure, and the O97 test that drives a
    // non-JSON file through the input is what keeps it caught if anyone moves
    // the call back out of the chain.
    importRules(text)
      .then((answer) => {
        release()
        // Every other editor gesture lands in `commit`; this is the shape it
        // already has (set the rules, close/leave the selection, drop the
        // last save, preview once), so it goes through the same funnel
        // rather than repeating it.
        commit(answer.rules, undefined)
        // I3: `POST .../rules/import` writes the rule table and derives
        // nothing — the CLI's own `library rules --import` prints exactly
        // this warning for the identical route (`library.rs`). This screen
        // used to say nothing at all, which left the imported list looking
        // applied the moment it appeared, when in fact the library's
        // structure is still the old rules' until something derives — the
        // exact state this branch exists to make unreachable, reached
        // through the editor's own Import button. The remedy named here is
        // Save, not a scan command: on this screen Save is the thing that
        // derives.
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
        // disable Save *and* Import for the rest of the session, with the
        // screen showing only the server's sentence about the document — a
        // refused import is the ordinary case (`rules/import` rejects a
        // malformed document), so the leak would be reached by the first
        // person who picked the wrong file.
        release()
        setFailure(messageOf(err))
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
      // reached. React's own render-phase-update budget is not infinite: a
      // bare `setFailure(messageOf(err))` here crashed the whole screen with
      // "Too many re-renders" rather than showing the message, confirmed by
      // a minimal reproduction of the same pattern outside this component.
      const reason = messageOf(err)
      if (reason !== failure) setFailure(reason)
      return ''
    }
  }

  return (
    <section className="rule-editor">
      <h2>Rules</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}
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

      <div className="editor-panes">
        <RuleList
          rules={rules}
          selected={selected}
          proposals={proposals}
          overrideCount={preview?.overrides.length ?? 0}
          frozen={frozen}
          onSelect={(index) => {
            // M1: the design runs a preview on selecting a *different*
            // rule — `rows` is scoped to `selected`, and changing which is
            // open is what repopulates it. Re-clicking the one already open
            // changes nothing a preview could answer differently, so
            // committing anyway paid another ~281 ms derivation under the
            // write lock and cleared the "Saved:" line for no reason.
            if (index !== selected) commit(rules, index)
          }}
          onToggle={(index) =>
            commit(
              rules.map((rule, i) =>
                i === index ? { ...rule, enabled: !rule.enabled } : rule,
              ),
              selected,
            )
          }
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

        <div className="editor-right">
          {selected !== undefined && rules[selected] !== undefined ? (
            <RuleForm
              // Local draft state lives in the form; a new key is what makes
              // selecting a different rule redraw rather than keep the last
              // one's half-typed text.
              key={`${selected}:${rules[selected].id}`}
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
                  // outcome-neutral. An earlier version promised that the
                  // import's rules would replace the list — true when it
                  // lands, and a promise this screen breaks when it does not.
                  // A refused document is the ordinary failure here, so the
                  // sentence has to hold for both endings. (**That the edit
                  // is discarded even when the import fails is a real
                  // asymmetry with O96's failed *save*, which keeps it** —
                  // the reasoning below only holds for an import that
                  // succeeded. It is left as it is rather than fixed here:
                  // holding across an import is a behaviour change, not a
                  // wording one.)
                  // Holding works for a save because `answer.rules` is
                  // positionally aligned to the list that was sent. An
                  // import's answer is aligned to nothing — it replaces every
                  // rule and mints fresh ids — so this index does not name the
                  // rule being edited and applying the edit there would
                  // overwrite an unrelated rule with it.
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
              Pick a rule to edit it. The impact below is for the whole list.
            </p>
          )}
        </div>
      </div>

      <PreviewTabs
        preview={preview}
        rules={rules}
        selected={selected}
        busy={busy}
        frozen={frozen}
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
        // Context: the save's own re-derive touches `<root>/.taiyaku/`, the
        // one place on disk this route writes, and `Impact` says nothing
        // about it — a save that could not write a hand document used to
        // answer with only a rehome count and silence. `DeriveSection` is
        // the renderer `ScanReport.tsx` already has for this exact struct,
        // reused rather than a second, narrower vocabulary invented here;
        // collapsed by default the same way `RemoveRoot` reuses it, since
        // most saves have nothing in these fields worth a person's first
        // glance.
        <details className="save-derive">
          <summary>What this save's re-derive found</summary>
          <DeriveSection derive={saved.derive} />
        </details>
      )}

      <div className="editor-actions">
        {/*
         * Disabled on `busy` too, not only `frozen` — see this file's own
         * top comment. Without it, a click landing inside the ~300-500 ms a
         * preview takes sends `rules` against a `predicted.current` that has
         * not caught up yet, and the mismatch draws a warning that blames
         * the server for a race the client caused.
         */}
        <button
          type="button"
          className="save"
          disabled={frozen || busy || preview === undefined}
          onClick={save}
        >
          {saving
            ? 'Saving…'
            : `Save — apply to ${preview?.impact.works_rehomed ?? 0} works`}
        </button>

        <div className="import">
          <button
            type="button"
            disabled={frozen}
            onClick={() => fileInput.current?.click()}
          >
            {writing === 'import' ? 'Importing…' : 'Import'}
          </button>
          <p>
            Import replaces every rule with the document’s, minting fresh ids,
            which reopens every accepted proposal whose rule it replaces.
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
    </section>
  )
}
