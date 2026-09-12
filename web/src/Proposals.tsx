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
 * **This screen only ever asks for `state: 'open'`, and nothing in this app
 * yet shows any other state.** An inbox is exactly the set of proposals still
 * waiting on a decision, and hardcoding that here is the right call: without
 * it, accepted and dismissed rows would mix into the list with no control to
 * sort them apart. But spec C3 §4's "a dismissal you disagree with later is a
 * thing you have to be able to find" is *not* discharged by that — it is met
 * at the store and API layers only, where `state` is a query parameter
 * `curation.ts` can send. No screen sends anything but `open`, so today the
 * answer to "where do I find a dismissal" is "nowhere in the UI". A state
 * control is the gap that closes it.
 *
 * **The score threshold defaults to 1, not 0.** The scale is `{0, 1, 2, 3}`;
 * 0 means "the suffix matches no edition word and the volume counts differ",
 * the measured false-positive shape. "Show weak proposals" drops the
 * threshold to 0 rather than filtering anything out permanently — a weak
 * proposal must stay reachable, never suppressed.
 *
 * **The kind tabs and the threshold are independent filters.** Each control
 * sets its own piece of state and touches no other, so selecting a kind
 * cannot silently change what the threshold is, and revealing weak proposals
 * cannot silently drop back to every kind. `Proposals.test.tsx` asserts the
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
 * **The list and the counts are one piece of state, `undefined` until the
 * first answer lands.** They arrive in one response and describe one moment,
 * so holding them apart let them be set apart; and an `[]`/`{}` pair cannot
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
} from './curation'
import { EmptyState } from './EmptyState'
import { ProposalRow } from './ProposalRow'
import { useInboxRefresh } from './shell'
import { messageOf } from './message'

export function Proposals() {
  const [inbox, setInbox] = useState<Inbox | undefined>()
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: `reloads` is a trigger, not a closure read — the body reads only `kind` and `showWeak`. Removing it, which is what this rule suggests, would leave an accept or a dismiss showing a stale list until a filter was touched.
  useEffect(() => {
    let cancelled = false
    listProposals({
      state: 'open',
      kind,
      min_score: showWeak ? 0 : INBOX_MIN_SCORE,
    })
      .then((next) => {
        if (cancelled) return
        setInbox(next)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [kind, showWeak, reloads])

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
      <h1>{total} open</h1>
      {failure !== undefined && (
        <div className="failure">
          <p role="alert">{failure}</p>
          <button type="button" className="retry" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {/* `chips` is `Library.tsx`'s filter-row idiom, styled in `index.css`
          down to the pressed state; `proposal-tabs` names this particular row
          and carries no rules of its own. */}
      <div className="proposal-tabs chips">
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
        <button
          type="button"
          className="weak-toggle"
          aria-pressed={showWeak}
          onClick={() => setShowWeak((w) => !w)}
        >
          {showWeak ? 'Hide weak proposals' : 'Show weak proposals'}
        </button>
      </div>
      {inbox !== undefined && proposals.length === 0 ? (
        <EmptyState
          kind={
            kind !== undefined && total > 0
              ? 'kind-answered'
              : showWeak
                ? 'inbox-clear'
                : 'inbox-weak-hidden'
          }
          label={kind === undefined ? undefined : kindLabel(kind)}
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
