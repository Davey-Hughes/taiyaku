/**
 * Accept, as a form — spec C3 §3 and §7.
 *
 * **Accepting writes a rule, and nothing else.** Derivation has exactly one
 * input — `scan_entry` swept through the rule set — so a direct write to
 * `series` or `edition` would be a second one, and the next `derive` would
 * decide the same material again without it. What this control produces is
 * therefore a `Rule`, and the route's contract is *the client shows the rule,
 * the server writes what it was shown*: `curation.ts`'s `Rule` round-trips
 * for that reason, and this form edits the offered one rather than deriving a
 * second.
 *
 * **A form and not a button, because the answer leaves fields undecided.**
 * "Yes, `Full Color Edition` is an edition of Go-Toubun" says nothing about
 * what that edition should be *called*, whether it is an official or a fan
 * edition, or what language it is in. `offered_rule` guesses the first two
 * from the suffix that made the proposal and never guesses the third; each is
 * offered here so a person can correct it before it is written.
 *
 * **And the pattern is editable because widening it is what the screen is
 * for.** The offered rule names one directory. C3 §3 chose a scannable inbox
 * over a triage loop so that a room full of proposals asking the same
 * question can be answered once — which is a widened pattern, and a widened
 * pattern is only safe to offer if what it would do is visible before it is
 * written. Every edit re-asks `POST /{id}/resolution`, and its answer is the
 * sentence over the button.
 *
 * **Widening needs `kind` as well as `pattern`, which is why there is a
 * control for it.** `rules::RuleKind` reads `prefix` as a literal `rel_path`
 * prefix, so a regex typed into a rule still marked `prefix` is compared as
 * its own source text and matches nothing — the form would answer "This rule
 * matches nothing" for a pattern that is exactly right. The two are edited
 * together.
 *
 * **The pattern, how it is read, and the three fields an accept decides are
 * what this form edits.** `root_id`, `match_on`, `set_series` and `note` are
 * sent back exactly as they arrived. `root_id` is the one worth naming:
 * `common_directory_pattern` scopes the offered rule to the single root its
 * subject sits under, and nothing here unscopes it, so a pattern widened in
 * this form is widened within that root. That is a limit on what the widest
 * pattern anybody types can reach and it is invisible in the pattern itself,
 * so the form says it under the box, where the widening happens.
 *
 * Unscoping is deliberately not one of the things this form offers, and the
 * reason is that it is the one widening the preview above the button could not
 * keep honest. The sweep scopes to `root_id` exactly as the write will, so
 * every other edit here is measured against what accepting it would do — but
 * `proposals.rs`'s I6 records that a NULL `root_id` means "every root,
 * forever", including a root nobody has added yet, and no sweep of the library
 * as it stands today can show that. `accept_with` takes a NULL one, so this is
 * a choice about what a screen should default anybody into rather than a
 * limitation: the store reserves it for "whoever deliberately widens a rule to
 * every root", and a text box on a triage form is not that.
 *
 * **Debounced, because `/resolution` sweeps the library.** That route
 * compiles every rule, reads every root and resolves every `scan_entry` row
 * through the same engine `derive` uses. It is on `spawn_blocking` and is
 * still the heaviest route in the API, and this form calls it while a pattern
 * is being typed; `debounce.ts` collapses a burst into the one call still
 * standing after the typing stops.
 *
 * **Accept is disabled whenever `refusals` returns anything** — every
 * condition `Resolution` carries, rather than the collateral one alone.
 * `accept_with` refuses on each of them with a 409, so a form mirroring fewer
 * offers a button that fails; and a guard drawn as a warning beside a live
 * button reads as a suggestion rather than as a refusal.
 *
 * **It is disabled while the answer in hand is not this rule's, too.** Each
 * edit does two things, and `Search.tsx`'s own comment is where both are
 * written down: it clears the resolution on screen, *and* it bumps the
 * sequence number so a sweep already in flight for the rule being replaced is
 * discarded rather than rendered. This file shipped only the first of the two,
 * and the second is what a widening actually needs: the mount sweep goes out a
 * quarter-second after the panel opens, so a person who widens the pattern
 * while it is still out had the narrow rule's "move 14 works" land under the
 * widened one with Accept live. The server does not backstop that — a widening
 * whose extra matches are the subjects of other open proposals lands in
 * `also_resolves`, not `collateral`, so no refusal fires and `accept_with`
 * writes the wide rule against the narrow rule's numbers.
 *
 * **That guard covers a stale `Resolution` and nothing else — in particular
 * not a stale `rule`, which is a different failure and is not fixed here.**
 * The paragraph above used to read as though it closed both. The five
 * controls seed from `rule` with `useState`, which runs once per mount, and
 * `candidate` spreads the *current* `rule` prop over them — so an instance
 * mounted against one offered rule and re-rendered against another sends a
 * splice: `pattern`, `kind` and the three `set_*` fields from the rule it
 * mounted with, `root_id`, `match_on`, `set_series` and `note` from the one
 * it is now being handed. Nothing in this file can see that; the fix is that
 * `ProposalRow` clears `detail` when the row collapses, so the form is
 * unmounted rather than kept across two different answers. This paragraph is
 * here so that the guarantee is not re-read out of the one above it.
 *
 * **And the reason it is disabled is attached to it, not merely near it.**
 * Whichever of the four states the form is in — still sweeping, refused,
 * broken, or clear — that message is what Accept is `aria-describedby`, so a
 * screen reader reading the control reads why it cannot be pressed rather than
 * finding an unexplained dead button. The block holding it is also a polite
 * live region, and it has to be one *statically*: the four states are separate
 * elements that replace each other, so a region declared on the message itself
 * would be created at the moment its own content appeared and announce
 * nothing. This is the higher-stakes half of the same thing the two failure
 * paragraphs already do with `role="alert"`.
 *
 * **A refused accept is still surfaced in the server's own words.** `api.ts`'s
 * `sendJson` carries a failed response's body into the error it throws, and
 * the 409 body names the series the accept would have moved. Reducing that to
 * a status would throw away the only part of it a person can act on.
 */
import {
  type FormEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from 'react'
import {
  acceptProposal,
  consequence,
  THE_FOLD,
  type Resolution,
  refusals,
  resolution,
  type Rule,
  type Side,
} from './curation'
import { debounce } from './debounce'
import { messageOf } from './message'

/**
 * The two `rules::RuleKind` values a rule written from a proposal can take.
 *
 * The enum also holds `builtin-volume` and `builtin-chapter`, which are
 * extractors reading a number out of a filename rather than shapes a pattern
 * is matched with. Neither answers "this directory is that series".
 */
const MATCH_KINDS = ['prefix', 'regex']

/** `edition.kind` (`001_initial.sql`), with the empty string for its NULL. */
const EDITION_KINDS = ['', 'raw', 'official', 'fan']

/**
 * `options`, with `current` appended when it holds no such value.
 *
 * A `<select>` whose `value` matches none of its `<option>`s shows the first
 * option instead, and the next edit to any *other* field then submits that
 * substitute — the rule silently rewritten by the control drawn to display
 * it. Appending keeps whatever arrived both visible and selected.
 */
const including = (options: string[], current: string): string[] =>
  options.includes(current) ? options : [...options, current]

/** A text box says "unset" with `''`; a nullable `set_*` column says it with null. */
const orNull = (value: string): string | null => (value === '' ? null : value)

/** How long the typing has to stop for before the library is swept. */
const RESOLUTION_DEBOUNCE_MS = 250

export function AcceptForm({
  id,
  rule,
  target,
  onAccepted,
}: {
  id: number
  rule: Rule
  target: Side
  /** The rule was written — the list and its counts both moved. */
  onAccepted: () => void
}) {
  const [pattern, setPattern] = useState(rule.pattern)
  const [matchKind, setMatchKind] = useState(rule.kind)
  const [editionKey, setEditionKey] = useState(rule.set_edition_key ?? '')
  const [editionKind, setEditionKind] = useState(rule.set_kind ?? '')
  const [language, setLanguage] = useState(rule.set_language ?? '')

  const [preview, setPreview] = useState<Resolution>()
  const [previewFailure, setPreviewFailure] = useState<string>()
  const [failure, setFailure] = useState<string>()
  const [accepting, setAccepting] = useState(false)
  const latest = useRef(0)
  const whyId = useId()

  /**
   * The rule as edited. Spread from the one the server offered, so every
   * field with no control above it goes back unchanged.
   */
  const candidate = useMemo<Rule>(
    () => ({
      ...rule,
      kind: matchKind,
      pattern,
      set_edition_key: orNull(editionKey),
      set_kind: orNull(editionKind),
      set_language: orNull(language),
    }),
    [rule, matchKind, pattern, editionKey, editionKind, language],
  )

  /**
   * One debounced sweep for the life of the component — a fresh one per
   * render would have nothing pending to collapse into, which is `Search`'s
   * reason for the same `useMemo`.
   *
   * The candidate is an argument rather than a closure read, so the call that
   * survives the debounce carries the rule that was standing when the typing
   * stopped. The sequence number is `Search`'s too: three sweeps in flight
   * may return in any order, and `cancelled` flags solve unmount rather than
   * overtaking.
   */
  const ask = useMemo(
    () =>
      debounce((candidateRule: Rule) => {
        const seq = ++latest.current
        resolution(id, candidateRule)
          .then((next) => {
            if (seq !== latest.current) return
            setPreview(next)
            setPreviewFailure(undefined)
          })
          .catch((err: unknown) => {
            if (seq !== latest.current) return
            setPreviewFailure(messageOf(err))
          })
      }, RESOLUTION_DEBOUNCE_MS),
    [id],
  )

  useEffect(() => {
    // **Two acts, not one** — `Search.tsx`'s `q === ''` branch is where the
    // pair is written down, and this file originally copied half of it.
    // Bumping the sequence invalidates the sweep *already sent* for the rule
    // being replaced: without it that sweep still satisfied
    // `seq === latest.current` when it answered, and rendered the previous
    // rule's measurement — its consequence sentence, its counts and an
    // enabled Accept — under the candidate that had replaced it. Clearing is
    // the other half, and covers the window before anything is sent at all.
    //
    // Every fetch effect on this screen carries some version of clear-then-
    // request; this is the one where the missing half was not a flicker but a
    // rule written against numbers measured for a different rule.
    latest.current += 1
    setPreview(undefined)
    setPreviewFailure(undefined)
    ask(candidate)
    return () => {
      // The row can be collapsed mid-debounce, and `debounce.ts` reserves
      // `cancel` for exactly that: there is no longer a screen for the
      // answer to land on.
      ask.cancel()
    }
  }, [ask, candidate])

  const reasons = preview === undefined ? [] : refusals(preview)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setAccepting(true)
    setFailure(undefined)
    acceptProposal(id, candidate)
      .then(() => {
        // **Deliberately not re-enabled here, and re-ordering the two lines
        // would not have been the fix.** `onAccepted` is `Proposals`'
        // `reload`, which returns as soon as it has bumped a counter — the
        // list it re-asks for is still in flight when this handler returns,
        // and this row stays on screen for the whole of that round trip.
        // Setting `accepting` false anywhere in this path, before that call
        // or after it, puts a live Accept back under the pointer for a
        // proposal the server has already moved to `accepted`; the second
        // POST comes back 409 and `sendJson` renders it as a red
        // `role="alert"` immediately after a successful accept, on the one
        // destructive control this screen has.
        //
        // What clears this state is the unmount: the reload lands without
        // this proposal in it, the row goes, and the form goes with it. If
        // the reload *fails* instead, the list keeps its own banner and its
        // Retry and this button stays dead — which is right, because the
        // accept it would repeat has already happened.
        onAccepted()
      })
      .catch((err: unknown) => {
        // The failure path does re-enable, and has to: nothing was written,
        // the row is not going anywhere, and a second attempt is the only
        // thing left to do.
        setAccepting(false)
        setFailure(messageOf(err))
      })
  }

  return (
    <form className="accept-form" onSubmit={submit}>
      <h2 className="lab">The rule this writes</h2>
      <div className="rule-fields">
        <label className="field pattern">
          <span>Pattern</span>
          <input
            type="text"
            value={pattern}
            spellCheck={false}
            onChange={(event) => setPattern(event.target.value)}
          />
        </label>
        {/* Under the box, because this is a fact about how wide the thing in
            the box can reach and the box is where widening happens. The panel
            prints `root` for each side well above the form, which says which
            root the material is in but not that the rule is confined to it. */}
        <p className="scope">
          {rule.root_id === null
            ? 'This rule is not tied to a root: however narrow the pattern, it is matched under every one.'
            : 'However wide this is made, it is matched under one root only — the one the subject sits under.'}
        </p>
        <label className="field match-kind">
          <span>Match as</span>
          <select
            value={matchKind}
            onChange={(event) => setMatchKind(event.target.value)}
          >
            {including(MATCH_KINDS, matchKind).map((kind) => (
              <option key={kind} value={kind}>
                {kind}
              </option>
            ))}
          </select>
        </label>
        <label className="field edition-key">
          <span>Edition key</span>
          <input
            type="text"
            value={editionKey}
            spellCheck={false}
            onChange={(event) => setEditionKey(event.target.value)}
          />
        </label>
        <label className="field edition-kind">
          <span>Edition kind</span>
          <select
            value={editionKind}
            onChange={(event) => setEditionKind(event.target.value)}
          >
            {including(EDITION_KINDS, editionKind).map((kind) => (
              <option key={kind} value={kind}>
                {kind === '' ? '(not set)' : kind}
              </option>
            ))}
          </select>
        </label>
        <label className="field language">
          <span>Language</span>
          <input
            type="text"
            value={language}
            spellCheck={false}
            onChange={(event) => setLanguage(event.target.value)}
          />
        </label>
      </div>
      {/* One element for all four states, present from the first render: a
          live region has to exist before the thing it announces changes, and
          each of the four below is created at the moment its own turn comes.
          `aria-live` rather than `role="status"`, so the two failure
          paragraphs keep the assertive `role="alert"` they need without a
          second live role wrapping them. */}
      <div className="why" id={whyId} aria-live="polite">
        {previewFailure !== undefined ? (
          <p className="preview-failure" role="alert">
            {previewFailure}
          </p>
        ) : preview === undefined ? (
          <p className="checking">Working out what this rule would do…</p>
        ) : reasons.length > 0 ? (
          /* The refusals in place of the consequence, not beneath it:
             `consequence` opens "Accepting will:", which is not true of a rule
             the server would refuse. */
          <ul className="refusals">
            {reasons.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        ) : (
          /* Two paragraphs, inside the same block Accept is
             `aria-describedby` — the fold this accept sets off is a
             consequence of pressing the button, so a screen reader reading
             the control reads it, and it sits below the sentence rather than
             inside it because `consequence` says what accepting is *for*.
             It read "what an accept leaves behind" while the accept left a
             duplicate standing; the fold closed that (O73) and the framing
             went with it. */
          <>
            <p className="consequence">
              {consequence(target, candidate, preview)}
            </p>
            <p className="the-fold">{THE_FOLD}</p>
          </>
        )}
      </div>
      <button
        type="submit"
        className="accept"
        aria-describedby={whyId}
        disabled={accepting || preview === undefined || reasons.length > 0}
      >
        Accept
      </button>
      {failure !== undefined && (
        <p className="accept-failure" role="alert">
          {failure}
        </p>
      )}
    </form>
  )
}
