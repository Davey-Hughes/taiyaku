/**
 * One inbox row: the line it shows collapsed, and the evidence it opens —
 * spec C3 §2.
 *
 * **The evidence is fetched when the row opens, not with the list.** The
 * inbox asks for every open proposal at once and a panel costs two sides of
 * pages, paths, facts and reasons; fetching all of them up front would pay
 * for 37 panels to show one. The row that is open is the row that is being
 * judged.
 *
 * **A cover *and* two interior pages per side, which is a decision rather
 * than decoration.** C3 §2: a colour edition's cover is a different image
 * from the mono edition's — different palette, sometimes different art
 * entirely — so a panel showing covers alone makes the true case look false
 * exactly where the judgement is hardest. An interior page is the same panel
 * layout in both. The interior indices come from the store, which chooses
 * them as fractions of the page count so they land inside the member; this
 * draws however many arrive, because a short member yields fewer and a
 * one-page member none.
 *
 * **Each thumbnail links to the page at full size.** Judging panel layout
 * from a thumbnail is the thing this panel exists for, and a thumbnail is
 * where that judgement starts rather than where it ends — so the `<img>` is
 * `thumbUrl` and the link around it is `pageUrl`. Neither goes through
 * `api.ts`'s 401 wrapper; that module's own comment says why.
 *
 * **The panel states that nothing has compared the artwork.** C3 §2's second
 * decision. Every signal in the reason list is about names and counts, while
 * this project already owns `duplicates --against`, which answers the same
 * question from the pages themselves — and it is not wired in, because the
 * fingerprinting code sits in a crate that depends on the server. So the
 * `Compare v01` control ships present, disabled and honest: an absent number
 * that looked like a low score would be worse than a control admitting it
 * has not run.
 *
 * **Three fields arrive as lists and are drawn as lists.** `directories`,
 * `formats` and `volumes` were each a rendered sentence in the store once and
 * stopped being one, so nothing here parses a sentence back apart:
 * `directories` renders as the one path, the first plus a count, or the state
 * where nothing resolves to this series at all.
 *
 * **`rule: null` is not "this proposal cannot be accepted".** The server
 * checks state before it previews anything, so every accepted and every
 * dismissed proposal reads `null` there too — which is why the line saying so
 * is drawn only for a proposal still `open`. `curation.ts`'s `Detail.rule`
 * carries the same warning at the other end of the wire. The other direction
 * *is* safe to read, and the panel reads it: a non-null `rule` is one
 * `offered_rule` derived, which it does only for an open proposal naming a
 * target — so that is where `AcceptForm` is mounted, and there is no second
 * state check beside it.
 *
 * **And that line names no causes, which is a correction.** It used to offer
 * three — a subject spanning more than one directory, a renamed target, a
 * kind nothing can be accepted for — joined by "or", which reads as the whole
 * list. `offered_rule` has at least six refusals: those three, plus a subject
 * sitting under the same directory name in more than one root, plus a suffix
 * that slugifies to nothing, plus a proposal that is not `open`. Worse, the
 * first of the three was drawn *against* the panel four inches above it:
 * `evidence` hands `offered_rule` the same paths it hands `side`, so a
 * subject with no directories at all makes `common_directory_pattern` refuse
 * — and the row then said "Nothing on disk resolves to this series today"
 * and "its subject spans more than one directory" at once. The store already
 * words each refusal; a summary here that had to stay in step with six of
 * them would go stale, so this says only what it knows.
 *
 * **The line's own control follows the proposal's `state`, not the screen's
 * filter.** An open proposal is dismissed from here, a dismissed one is
 * reopened from here (O72), and an accepted one draws no control at all:
 * the only undo an accept has is `unacceptProposal`, which leaves what the
 * rule derived standing, so a button labelled Undo would say something false
 * (O74). Reading the row's own `state` means a row can never offer the wrong
 * one because the list it sits in was asked for something else.
 *
 * **Both writes share one runner, and it holds a ref as well as `disabled`.**
 * `disabled` only takes effect once React has re-rendered, so two presses in
 * one frame both reached the handler; a reopen sent twice answers the second
 * with a 409 about a proposal the first had already reopened.
 *
 * **The collapsed line names both series** — spec C3 §3's scannability is a
 * person seeing that 29 of 37 rows ask the same question, and rows that
 * cannot be told apart are not a list, they are a queue with a scrollbar.
 * `ListedProposal`'s two title fields are what the line reads; it drew the
 * `reason` sentence alone before, which only looked like an identity because
 * every `raise_*` in the store happens to open with the subject's quoted
 * title, and which named the target at no width at all.
 */
import { useEffect, useRef, useState } from 'react'
import { AcceptForm } from './AcceptForm'
import { pageUrl, thumbUrl } from './api'
import {
  type Detail,
  dismissProposal,
  type FirstVolume,
  kindLabel,
  type ListedProposal,
  proposalDetail,
  relation,
  reopenProposal,
  type Side,
  strength,
} from './curation'
import { messageOf } from './message'
import { requestedTileEdge, TILE_HEIGHT_PX } from './tileSize'

/** Which end of the proposal a panel is drawing, as a word and as a label. */
const SIDES = {
  subject: 'Subject',
  target: 'Proposed target',
} as const

type Which = keyof typeof SIDES

/**
 * One page of volume 1, thumbnail-sized, linking to the page itself.
 *
 * `target="_blank"` rather than navigating: the evidence panel is a
 * comparison, and losing it to open one of the things being compared undoes
 * the comparison.
 */
function PageTile({
  volume,
  index,
  alt,
}: {
  volume: FirstVolume
  index: number
  alt: string
}) {
  return (
    <a
      className="page"
      href={pageUrl(volume.work_slug, volume.edition_key, index)}
      target="_blank"
      rel="noreferrer"
    >
      <img
        src={thumbUrl(
          volume.work_slug,
          volume.edition_key,
          index,
          requestedTileEdge(TILE_HEIGHT_PX, window.devicePixelRatio),
        )}
        alt={alt}
        loading="lazy"
      />
    </a>
  )
}

/** One end of the proposal: its pages, its path, its facts and its volumes. */
function SidePanel({ side, which }: { side: Side; which: Which }) {
  const first = side.first_volume
  return (
    <div className="side">
      <p className="side-role">{SIDES[which]}</p>
      <h2 className="side-title">{side.title}</h2>
      {first === null ? (
        <p className="no-volume">
          No volume has been scanned for this side yet.
        </p>
      ) : (
        <div className="pages">
          <PageTile volume={first} index={0} alt={`${which} volume 1 cover`} />
          {/* Keyed by the page index rather than by position, which the
              volume chips below deliberately are not — so the difference is
              worth saying out loud. `interior_indices` (`proposals.rs`)
              returns at most *two* values and `dedup()`s them, and `dedup`
              only removes neighbours: at that length every pair is a
              neighbouring pair, so the two cannot collide. A store that ever
              returned three would have to sort them, or this has to move to
              `key={at}` the way `.volume-list` did. */}
          {first.interior.map((index) => (
            <PageTile
              key={index}
              volume={first}
              index={index}
              alt={`${which} volume 1 page ${index + 1}`}
            />
          ))}
        </div>
      )}
      {side.directories.length === 0 ? (
        <p className="no-directory">
          Nothing on disk resolves to this series today — it has not been
          scanned, or a rename or a rule moved it.
        </p>
      ) : (
        <p className="directories">
          <code>{side.directories[0]}</code>
          {side.directories.length > 1 && (
            <span className="more">+{side.directories.length - 1} more</span>
          )}
        </p>
      )}
      <dl className="facts">
        <div className="fact">
          <dt>volumes</dt>
          <dd>{side.volume_count}</dd>
        </div>
        {first !== null && (
          <>
            <div className="fact">
              <dt>v01 pages</dt>
              <dd>{first.page_count}</dd>
            </div>
            <div className="fact">
              <dt>format</dt>
              <dd>{first.formats.join(' · ') || '—'}</dd>
            </div>
          </>
        )}
        <div className="fact">
          <dt>root</dt>
          <dd>{side.root_name ?? '—'}</dd>
        </div>
      </dl>
      {/* Keyed by position, not by the number: the store's own query carries
          no `DISTINCT`, so a series holding two works numbered 1 sends two
          chips reading `1`. Nothing in this list holds state, and the whole
          list is redrawn from one array. */}
      <p className="volume-list">
        {side.volumes.map((volume, at) => (
          <span key={at} className="vol">
            {volume}
          </span>
        ))}
      </p>
    </div>
  )
}

/** Everything the store hands over for one proposal, drawn. */
function Evidence({
  detail,
  onAccepted,
}: {
  detail: Detail
  onAccepted: () => void
}) {
  return (
    <>
      <div className="sides">
        <SidePanel side={detail.subject} which="subject" />
        {detail.target !== null && (
          <SidePanel side={detail.target} which="target" />
        )}
      </div>
      <h2 className="lab">Why this was proposed</h2>
      <ul className="reasons">
        {detail.reasons.map((reason) => (
          <li key={reason.text}>
            <span className="sign">{reason.sign}</span>
            <span className="text">{reason.text}</span>
          </li>
        ))}
      </ul>
      <div className="measured">
        <p>
          <b>Nothing here compared the artwork.</b> Every line above is a fact
          about <i>names and counts</i>. <code>duplicates --against</code>{' '}
          already answers “are these the same volume” from the pages themselves,
          and it has not been run.
        </p>
        <button type="button" className="compare" disabled>
          Compare v01
        </button>
        <p className="not-yet">
          Not wired up yet: the fingerprinting code has to leave{' '}
          <code>taiyaku-cli</code> first.
        </p>
      </div>
      {detail.proposal.state === 'open' && detail.rule === null && (
        <p className="no-rule">
          There is no rule this proposal could be accepted as, as it stands.
        </p>
      )}
      {/* **Both halves are checked, and only one of them is a type
          nuisance.** `offered_rule` refuses before it derives anything unless
          the proposal is `open` and names a target, so a non-null `rule`
          already means there is a target side — but `Detail.target` is
          `Side | null` on the wire and this is the file that has to say what
          it draws when it is null. The form needs the target's title for the
          sentence over its button, so it takes a `Side` rather than accepting
          one and rendering `to series undefined`. */}
      {detail.rule !== null && detail.target !== null && (
        <AcceptForm
          id={detail.proposal.id}
          rule={detail.rule}
          target={detail.target}
          onAccepted={onAccepted}
        />
      )}
    </>
  )
}

export function ProposalRow({
  proposal,
  expanded,
  onToggle,
  onChanged,
}: {
  proposal: ListedProposal
  expanded: boolean
  onToggle: () => void
  /** A write moved both the list and its counts — ask the server again. */
  onChanged: () => void
}) {
  const [detail, setDetail] = useState<Detail>()
  const [failure, setFailure] = useState<string>()
  const [writeFailure, setWriteFailure] = useState<string>()
  const [writing, setWriting] = useState(false)
  const inFlight = useRef(false)

  useEffect(() => {
    if (!expanded) {
      // **Cleared, not merely left unread.** A collapsed row draws nothing
      // from `detail`, so keeping it looked free — and it was not.
      // Re-expanding renders the *previous* detail for one commit while the
      // new request is in flight, which mounts `AcceptForm` against the rule
      // that detail carried; the form seeds all five of its controls with
      // `useState`, which runs once per mount, so the fresh rule arriving
      // underneath it changes only the fields with no control
      // (`set_series`, `note`, `root_id`, `match_on`) and the rule the
      // button then sends is spliced from two different answers — with the
      // half that changed invisible. `offered_rule` genuinely re-derives per
      // request, off live `scan_entry` rows and the target's current title,
      // so another accept, a background scan or a rename between two expands
      // is enough. C3 §7 is "the client shows the rule, the server writes
      // what it was shown", and a spliced rule is neither.
      //
      // Clearing costs a second "Reading the evidence…" on a re-expand. That
      // is the honest state: the answer on screen would otherwise be one
      // nobody has asked the server for since the row was last open.
      //
      // `failure` is deliberately *not* cleared here. It is cleared in the
      // `.then` below, and `clears a failure once an answer arrives` is the
      // test for that — clearing it here too would make that test pass with
      // the `.then`'s own line deleted.
      setDetail(undefined)
      return
    }
    let cancelled = false
    proposalDetail(proposal.id)
      .then((evidence) => {
        if (cancelled) return
        setDetail(evidence)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [expanded, proposal.id])

  /** Send one of the line's writes — see this file's doc comment. */
  const answer = (write: (id: number) => Promise<unknown>) => {
    if (inFlight.current) return
    inFlight.current = true
    setWriting(true)
    setWriteFailure(undefined)
    write(proposal.id)
      .then(() => {
        inFlight.current = false
        setWriting(false)
        onChanged()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setWriting(false)
        setWriteFailure(messageOf(err))
      })
  }

  return (
    <li className={strength(proposal.score)}>
      <div className="row">
        <button
          type="button"
          className="disclosure"
          aria-expanded={expanded}
          onClick={onToggle}
        >
          <span className="kind">{kindLabel(proposal.kind)}</span>
          <span className="what">
            <span className="subject">{proposal.subject_title}</span>
            {proposal.target_title !== null && (
              <span className="arrow">
                ↳ {relation(proposal.kind)} {proposal.target_title}
              </span>
            )}
          </span>
          <span className="score">{strength(proposal.score)}</span>
        </button>
        {/* Accepting is a form rather than a click — C3 §3 — so the accept
            control lives in the panel beside the rule it would write, and
            this line carries the decisions that need no form. */}
        {proposal.state === 'open' && (
          <button
            type="button"
            className="dismiss"
            disabled={writing}
            onClick={() => answer(dismissProposal)}
          >
            Dismiss
          </button>
        )}
        {proposal.state === 'dismissed' && (
          <button
            type="button"
            className="reopen"
            disabled={writing}
            onClick={() => answer(reopenProposal)}
          >
            Reopen
          </button>
        )}
      </div>
      {writeFailure !== undefined && (
        <p className="write-failure" role="alert">
          {writeFailure}
        </p>
      )}
      {expanded && (
        <div className="proposal-detail">
          {/* The one place the `reason` sentence is drawn. The collapsed
              line above names the two series instead: `reason` opens with the
              subject's title in quotes — that is how every `raise_*` in the
              store words one — so beside a column that names the subject it
              was the same words twice, and the part that differs is past
              character sixty, which no width reached. */}
          <p className="full-reason">{proposal.reason}</p>
          {failure !== undefined ? (
            <p className="evidence-failure" role="alert">
              {failure}
            </p>
          ) : detail === undefined ? (
            <p className="loading">Reading the evidence…</p>
          ) : (
            <Evidence detail={detail} onAccepted={onChanged} />
          )}
        </div>
      )}
    </li>
  )
}
