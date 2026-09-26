/**
 * The proposals inbox — spec C3 §3, §4 and §7.
 *
 * **A scannable list, not a one-at-a-time queue.** With dozens of proposals
 * that are mostly one kind, the point of this screen is letting a person
 * *see* that — the per-kind tab counts are the overview spec C3 chose over a
 * triage loop, so 29 of 37 questions can be answered once, as a widened rule,
 * instead of 37 times. The tabs read `counts` from `curation.ts`'s `Inbox`,
 * which is computed within whichever state this screen asks for and *not*
 * narrowed by the `kind` filter — each tab counts its own total regardless of
 * which one is currently selected.
 *
 * **Open by default, and the other two states one press away (O72).** An
 * inbox is exactly the set of proposals still waiting on a decision, so
 * `open` is what the screen lands on: mixing accepted and dismissed rows into
 * that list would leave nothing to sort them apart by. Spec C3 §4 asks for
 * the rest all the same — "accepted and dismissed proposals stay listed under
 * their own filters. A dismissal you disagree with later is a thing you have
 * to be able to find." The state control is those filters. Until it existed
 * this screen sent `state: 'open'` and nothing else, so §4 was met at the
 * store and the API and nowhere a person could reach.
 *
 * **The header, the tab counts and the empty states all read within the
 * state the list on screen was fetched for**, because `counts` does: the
 * server counts within the state the request carried. So the header says "2
 * dismissed" rather than "2 open" over a list of dismissals, and an empty
 * dismissed list says nothing has been dismissed rather than that there is
 * nothing to answer. **That is the answer's state, not the pressed
 * control's**, and the two differ for as long as a request is on its way:
 * reading the control, pressing Dismissed over 37 open proposals drew "37
 * dismissed" above the open rows until the dismissed list landed. The
 * alternative — emptying the list the moment a state is pressed — was not
 * taken: it blanks the screen on every switch, and its header would still
 * need a word for "nothing loaded", where this one only ever describes rows
 * actually on screen.
 *
 * **What a row lets a person do is the row's business, not this screen's.**
 * `ProposalRow` reads its own proposal's `state`: an open row offers Dismiss,
 * a dismissed one Reopen, and an accepted one nothing — O74 is why there is
 * no Undo, since un-accepting leaves what the rule derived standing.
 *
 * **The score threshold defaults to 1, not 0.** The scale is `{0, 1, 2, 3}`;
 * 0 means "the suffix matches no edition word and the volume counts differ",
 * the measured false-positive shape. "Show weak proposals" drops the
 * threshold to 0 rather than filtering anything out permanently — a weak
 * proposal must stay reachable, never suppressed.
 *
 * **The state control, the kind tabs and the threshold are independent
 * filters.** Each control sets its own piece of state and touches no other, so
 * selecting a kind cannot silently change what the threshold is, revealing
 * weak proposals cannot silently drop back to every kind, and moving to
 * another state keeps both. `Proposals.test.tsx` asserts the
 * whole filter object on each click rather than the one field the click was
 * about, which is what makes that independence testable.
 *
 * One row expanded at a time, `useState<number | null>` rather than a set of
 * ids: this screen owns which row is open and `ProposalRow` owns what an open
 * row draws, so the evidence — two sides of pages, paths, facts and reasons —
 * is fetched by the row that is being judged rather than by the list.
 *
 * **The collapsed line is a `<div>` holding a disclosure `<button>`, not one
 * big `<button>`.** Accept and Dismiss controls belong *in* that line, and an
 * interactive descendant of a `<button>` is invalid HTML — so the wrapper is
 * a plain element and the disclosure control is one child of it, leaving room
 * for siblings. The whole line is not clickable; the disclosure button is,
 * and it carries the `aria-expanded` that names what it does.
 *
 * **`reloads` is a dependency and nothing else.** Accepting or dismissing a
 * proposal changes both the list and the counts, and neither would move until
 * a filter was touched if the effect only watched the filters. Bumping the
 * counter re-runs the fetch with the filters unchanged; `reload` is what each
 * row is handed as `onChanged`, and the failure banner's Retry is its other
 * caller.
 *
 * **The list, the counts and the state they were asked for are one piece of
 * state, `undefined` until the first answer lands.** They arrive in one
 * response and describe one moment, so holding them apart let them be set
 * apart; and an `[]`/`{}` pair cannot
 * be told from an inbox that answered with nothing in it, which is exactly
 * the distinction the empty state below rests on. `Search.tsx`'s `hits` and
 * `Library.tsx`'s `gridData` are the same shape for the same reason: neither
 * draws an empty state until it has heard back.
 *
 * **A kind with nothing open is absent from `counts`, not zero**, because
 * `counts` comes from a SQL `GROUP BY`. Answering the last proposal of the
 * selected kind therefore used to take its tab out of the row *underneath the
 * person's own selection*: the list went blank, no tab read pressed, the
 * header still said "N open", and nothing said what had happened or how to
 * get back.
 *
 * **The selected kind keeps its tab, at zero, and the screen says so.** The
 * other defensible answer — fall back to `All` — was rejected deliberately:
 * it changes what is listed without the person touching a control, and it
 * throws away the one fact worth reporting, which is that they have just
 * cleared that kind out. Keeping the selection means the tab has to stay
 * drawn, or a filter would be in force with nothing on screen holding
 * `aria-pressed` — so `tabs` appends the selected kind when `counts` no
 * longer carries it. `EmptyState` then names the kind and points at `All`,
 * which is the control that undoes it and is already on screen.
 */
import { useEffect, useState } from 'react'
import {
  INBOX_MIN_SCORE,
  type Inbox,
  inboxTotal,
  kindLabel,
  listProposals,
  PROPOSAL_STATES,
  type ProposalState,
} from './curation'
import { EmptyState } from './EmptyState'
import { ProposalRow } from './ProposalRow'
import { useInboxRefresh } from './shell'
import { messageOf } from './message'

export function Proposals() {
  const [loaded, setLoaded] = useState<
    { inbox: Inbox; state: ProposalState } | undefined
  >()
  const [state, setState] = useState<ProposalState>('open')
  const [kind, setKind] = useState<string | undefined>(undefined)
  const [showWeak, setShowWeak] = useState(false)
  const [expanded, setExpanded] = useState<number | null>(null)
  const [failure, setFailure] = useState<string | undefined>()
  const [reloads, setReloads] = useState(0)

  // **The sidebar's count moves with this list, or the two contradict each
  // other on this very screen.** `Shell` fetches the count once, when it
  // mounts; an accept or a dismiss re-reads the list below but would leave
  // that badge reading one more than the header beside it. A
  // no-op wherever no shell sits above this screen, which is every test that
  // mounts it alone.
  const refreshInbox = useInboxRefresh()

  /** Ask the server the same question again — see this file's doc comment. */
  const reload = () => {
    refreshInbox()
    setReloads((n) => n + 1)
  }

  // `hooks.ts`'s `useScrollFollow` carries the same suppression for the same
  // shape: a dependency the body never reads, listed because its *change* is
  // the signal.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `reloads` is a trigger, not a closure read — the body reads only `state`, `kind` and `showWeak`. Removing it, which is what this rule suggests, would leave an accept, a dismiss or a reopen showing a stale list until a filter was touched.
  useEffect(() => {
    let cancelled = false
    listProposals({
      state,
      kind,
      min_score: showWeak ? 0 : INBOX_MIN_SCORE,
    })
      .then((next) => {
        if (cancelled) return
        setLoaded({ inbox: next, state })
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [state, kind, showWeak, reloads])

  const inbox = loaded?.inbox
  // What the list on screen answers — see this file's doc comment.
  const shown = loaded?.state ?? state
  const proposals = inbox?.proposals ?? []
  const counts = inbox?.counts ?? {}
  const total = inboxTotal(counts)

  // The selected kind keeps its tab even once its last proposal is answered
  // and the server stops reporting it — see this file's doc comment.
  const tabs: [string, number][] = Object.entries(counts)
  if (kind !== undefined && !(kind in counts)) {
    tabs.push([kind, 0])
  }

  return (
    <div className="proposals">
      <h1>
        {total} {shown}
      </h1>
      {failure !== undefined && (
        <div className="failure">
          <p role="alert">{failure}</p>
          <button type="button" className="retry" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {/* The three states, as C3 §4's "their own filters". A row of its own
          because a state and a kind are two filters, not two values of one —
          and pills rather than tabs because it is the coarser of the two
          (spec §12.3). */}
      <div className="proposal-states">
        {PROPOSAL_STATES.map(({ state: s, label }) => (
          <button
            key={s}
            type="button"
            className="state"
            aria-pressed={state === s}
            onClick={() => setState(s)}
          >
            {label}
          </button>
        ))}
      </div>
      {/* One-of-N, so it wears the underlined tab look `config-nav` and the
          rule preview wear (§12.3). It could not until O150: the score
          threshold used to sit in this row wearing the same pill as a kind,
          and ②'s review found the two drawing the same accent underline side
          by side. */}
      <div className="proposal-tabs">
        <button
          type="button"
          className="tab"
          aria-pressed={kind === undefined}
          onClick={() => setKind(undefined)}
        >
          All {total}
        </button>
        {tabs.map(([k, n]) => (
          <button
            key={k}
            type="button"
            className="tab"
            aria-pressed={kind === k}
            onClick={() => setKind(k)}
          >
            {kindLabel(k)} {n}
          </button>
        ))}
      </div>
      {/* **The label does not flip.** It read "Show weak proposals" or "Hide
          weak proposals" while `aria-pressed` said the same thing beside it —
          the same fact twice, disagreeing for as long as a click was in
          flight. A checkbox says its state with its box.

          It also takes a coupling out of `EmptyState`. That component names
          "Show weak proposals" in its copy, and the two agreed only because
          the branches happened to line up: `inbox-weak-hidden` and
          `decided-weak-hidden` are chosen on `!showWeak` below, which was
          exactly when the button read "Show". An unconditional label stays
          true however those branches move later. (An earlier version of this
          comment claimed the copy had *disagreed* with the control; it never
          could — ⑤c's Task 3 review traced the branches.) */}
      <div className="checks weak-check">
        <label>
          <input
            type="checkbox"
            checked={showWeak}
            onChange={(e) => {
              setShowWeak(e.target.checked)
            }}
          />
          Show weak proposals
        </label>
      </div>
      {inbox !== undefined && proposals.length === 0 ? (
        <EmptyState
          kind={
            shown === 'open'
              ? kind !== undefined && total > 0
                ? 'kind-answered'
                : showWeak
                  ? 'inbox-clear'
                  : 'inbox-weak-hidden'
              : kind !== undefined && total > 0
                ? 'decided-kind-none'
                : showWeak
                  ? 'decided-none'
                  : 'decided-weak-hidden'
          }
          label={kind === undefined ? undefined : kindLabel(kind)}
          decided={shown === 'open' ? undefined : shown}
        />
      ) : (
        <ul className="proposal-list">
          {proposals.map((p) => (
            <ProposalRow
              key={p.id}
              proposal={p}
              expanded={expanded === p.id}
              onToggle={() => setExpanded((e) => (e === p.id ? null : p.id))}
              onChanged={reload}
            />
          ))}
        </ul>
      )}
    </div>
  )
}
