/**
 * The `/api/library/proposals/*` curation API — spec C3 §7. Knows HTTP, and
 * the handful of pure functions that turn one field of an answer into the
 * words a screen prints for it (`strength`, `kindLabel`, `relation`,
 * `consequence`, `refusals`); it knows nothing about *which* screen, and
 * mounts nothing.
 *
 * Separate from `library.ts` for the same reason that module is separate
 * from `api.ts`: these routes are admin-only and write the library's
 * organisation, where `library.ts` only ever reads what a reader may
 * browse. All three share `getJson` (and now `sendJson`), so the 401 rule
 * stays one rule across every module that talks to the server.
 */
import { getJson, sendJson } from './api'

/**
 * One `proposal` row, as both routes echo it. The list sends a superset —
 * `ListedProposal` below — and the detail view sends exactly this.
 */
export interface Proposal {
  id: number
  /** `edition-of` | `same-series` | `attach-chapters` | `align`. */
  kind: string
  subject_id: number
  /** `null` for `attach-chapters`, which has no second series to name. */
  target_id: number | null
  score: number
  reason: string
  /** `open` | `accepted` | `dismissed`. */
  state: string
  /**
   * The rule an accepted proposal wrote. `null` for every other state, and
   * also, transiently, for an `accepted` proposal whose rule was since
   * deleted through C4's editor — the next `raise` is what notices that and
   * reopens the proposal.
   */
  created_rule_id: number | null
}

/**
 * One inbox row *as the list sends it*: a `Proposal` plus the titles of the
 * series it names. `GET /api/library/proposals` is the only route that sends
 * these — the detail route's `subject`/`target` carry their own titles
 * already.
 *
 * **The collapsed row's only means of saying which proposal it is.** C3 §3
 * chose a scannable list over a triage loop so a person can see that 29 of
 * 37 rows ask one question and answer it once; that needs rows told apart at
 * a glance. The row drew `reason` alone before this, which *looked* like it
 * named the subject because every `raise_*` in the store happens to open its
 * sentence with the subject's quoted title — measured at 700px, the column
 * is ten characters wide and every `Edition Of` row reads `"Monster - `.
 */
export interface ListedProposal extends Proposal {
  subject_title: string
  /** `null` wherever `target_id` is: the proposal names one series only. */
  target_title: string | null
}

/** Volume 1 of a side, and the pages the evidence panel draws from it. */
export interface FirstVolume {
  work_slug: string
  edition_key: string
  page_count: number
  /**
   * Every distinct format across the edition's members, ascending.
   *
   * A list, not a rendered sentence: the store used to join these into one
   * string and stopped, so the client shows each entry as it arrives rather
   * than parsing a sentence back apart.
   */
  formats: string[]
  /**
   * Two interior page indices into `page_count`, or fewer where the edition
   * is too short to have two distinct ones.
   */
  interior: number[]
}

/** One end of a proposal, as the expanded row draws it. */
export interface Side {
  series_id: number
  title: string
  /**
   * Every directory this side's material sits in, distinct and ascending.
   *
   * A list, for the same reason `FirstVolume.formats` above is: it used to
   * arrive as a single rendered sentence and stopped.
   *
   * **Empty where nothing currently resolves to this series' title** — not
   * only "never scanned". A renamed `sort_title` or an edited rule can leave
   * a series with a real `volume_count` and `first_volume` while nothing in
   * the live rule set resolves to it any more.
   */
  directories: string[]
  volume_count: number
  volumes: number[]
  root_name: string | null
  first_volume: FirstVolume | null
}

/** One signal behind a proposal's score, with its direction. */
export interface Reason {
  text: string
  /** `+` for the signal, `-` against, `?` a fact with no direction. */
  sign: '+' | '-' | '?'
}

/**
 * The rule an accept would write, or would have written.
 *
 * Round-trips: `proposalDetail` returns one showing what an accept would do,
 * and `resolution`/`acceptProposal` send one back unchanged — C3 §7 is "the
 * client shows the rule, the server writes what it was shown".
 */
export interface Rule {
  /** `null` means every root. */
  root_id: number | null
  /** `prefix` for the narrow default; `regex` once widened. */
  kind: string
  pattern: string
  match_on: string
  set_series: string | null
  set_edition_key: string | null
  set_kind: string | null
  set_language: string | null
  note: string
}

/** The expanded row: a proposal's evidence, both sides, and its rule. */
export interface Detail {
  proposal: Proposal
  subject: Side
  /** Absent for a kind that names only a subject. */
  target: Side | null
  reasons: Reason[]
  /**
   * The rule an accept would write right now, or `null` when an accept
   * would be refused.
   *
   * **Not proof this proposal can never be accepted.** The server checks
   * state before anything else, so every `accepted` and every `dismissed`
   * proposal reads `null` here too — not only an `open` one whose target was
   * renamed out from under it, or whose subject's entries span more than one
   * directory. A row drawn from one of the inbox's non-open tabs must not
   * read this as "this proposal cannot be accepted".
   */
  rule: Rule | null
}

/**
 * What a candidate rule would do, without doing it.
 *
 * Six fields; five of them are refusals an accept then makes for the same
 * reason — see `refusals` below, which turns these into the sentences a
 * form shows. `also_resolves` is the sixth and is not one: it counts other
 * open proposals this rule would also answer.
 */
export interface Resolution {
  also_resolves: number[]
  /** Series this rule decides for that no proposal named. A refusal. */
  collateral: string[]
  /** Series this rule decides for that are a target it accepts into. A refusal. */
  targets_reached: string[]
  /** `scan_entry` rows this rule would decide at least one field for. Zero is a refusal. */
  entries_matched: number
  /** Whether any matched entry belongs to this proposal's own subject today. False is a refusal. */
  subject_reached: boolean
  /** Works with at least one member changing series. Zero is a refusal. */
  works_moved: number
}

/** The inbox: every proposal matching the filter, and tab counts by kind. */
export interface Inbox {
  proposals: ListedProposal[]
  counts: Record<string, number>
}

/** What an accept wrote. */
export interface Accepted {
  rule_id: number
}

/** `?state=&kind=&min_score=` — an absent field asks the server's own default. */
export interface ProposalsFilter {
  state?: string
  kind?: string
  min_score?: number
}

const queryString = (filter: ProposalsFilter): string => {
  const parts: string[] = []
  if (filter.state !== undefined)
    parts.push(`state=${encodeURIComponent(filter.state)}`)
  if (filter.kind !== undefined)
    parts.push(`kind=${encodeURIComponent(filter.kind)}`)
  if (filter.min_score !== undefined)
    parts.push(`min_score=${filter.min_score}`)
  return parts.length === 0 ? '' : `?${parts.join('&')}`
}

export const listProposals = (filter: ProposalsFilter = {}) =>
  getJson<Inbox>(`/api/library/proposals${queryString(filter)}`)

/**
 * §6.1's score threshold, as the inbox arrives at it.
 *
 * Here rather than in `Proposals.tsx` because two screens now ask the same
 * question at it: the inbox itself, and the sidebar's open count. A badge
 * counting one set and a screen listing another is a badge that sends
 * somebody to an empty list, and the only way that cannot happen is for the
 * number to be one number.
 *
 * "Show weak proposals" drops the *screen* to 0; the badge stays here, and
 * that is deliberate rather than an oversight. The control is component
 * state and starts false on every mount, so the list a person lands on is
 * always this threshold's — which makes this the count of what pressing the
 * entry actually shows them.
 */
export const INBOX_MIN_SCORE = 1

/**
 * Every kind's count added up — the number the inbox's own header shows.
 *
 * `counts` is computed by the server within the state *and* the threshold the
 * request carried, and deliberately not within its `kind` (each tab counts
 * its own total). So this sums to "open proposals at or above the threshold
 * asked for", which is what both the header and the sidebar entry mean by
 * "open".
 */
export const inboxTotal = (counts: Record<string, number>): number =>
  Object.values(counts).reduce((sum, n) => sum + n, 0)

export const proposalDetail = (id: number) =>
  getJson<Detail>(`/api/library/proposals/${id}`)

/**
 * `sendJson` resolves `T | undefined` because two of this module's own
 * routes answer a 204 with nothing to parse — `unacceptProposal` and
 * `dismissProposal` below. `resolution` and `acceptProposal` are not among
 * them: the server always answers both with a body (`Resolution` and
 * `Accepted` respectively), so narrowing that promise once, here, is what
 * lets a caller write `refusals(await resolution(...))` under
 * `strictNullChecks` without every call site repeating a `!`.
 *
 * Narrowed in this module rather than by overloading `sendJson` itself:
 * `sendJson` is shared plumbing that other routes — including these two
 * routes' own siblings two lines down — legitimately answer with a 204, so
 * teaching it "a body argument means the reply is never empty" would be a
 * lie told to every future caller, not just these two.
 */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

/** The dry run: what `rule` would do to proposal `id`, without writing it. */
export const resolution = (id: number, rule: Rule): Promise<Resolution> =>
  expectBody(
    sendJson<Resolution>(
      `/api/library/proposals/${id}/resolution`,
      'POST',
      rule,
    ),
  )

/** Write exactly the rule shown — the server's own contract for this route. */
export const acceptProposal = (id: number, rule: Rule): Promise<Accepted> =>
  expectBody(
    sendJson<Accepted>(`/api/library/proposals/${id}/accept`, 'POST', rule),
  )

/** Undo an accept: the rule is deleted and the proposals it answered reopen. */
export const unacceptProposal = (id: number) =>
  sendJson<void>(`/api/library/proposals/${id}/accept`, 'DELETE')

/** The answer is no. Persists, so the next scan does not raise it again. */
export const dismissProposal = (id: number) =>
  sendJson<void>(`/api/library/proposals/${id}/dismiss`, 'POST')

// --- pure helpers ------------------------------------------------------------

export const strength = (score: number): 'strong' | 'weak' =>
  score >= 2 ? 'strong' : 'weak'

const KIND_LABELS: Record<string, string> = {
  'edition-of': 'Edition Of',
  'same-series': 'Same Series',
  'attach-chapters': 'Attach Chapters',
  align: 'Align',
}

/**
 * The words a screen prints for a proposal's `kind`, falling back to the raw
 * string for one this client has never heard of.
 *
 * Beside `strength` above because it is the same shape — one field of a
 * `Proposal` turned into the word shown for it — and in this module rather
 * than in a component because both the filter tabs and the row itself print
 * it, and the fallback is the kind of behaviour that survives in one place
 * and drifts in two.
 */
export const kindLabel = (kind: string): string => KIND_LABELS[kind] ?? kind

const KIND_RELATIONS: Record<string, string> = {
  'edition-of': 'an edition of',
  'same-series': 'the same series as',
}

/**
 * How a collapsed row reads its subject's relation to its target — the
 * mockup's `↳ an edition of Go-Toubun no Hanayome`.
 *
 * **Vaguer than `kindLabel` for a kind it does not know, on purpose.** The
 * fallback there is the raw kind string, which is honest because a kind is a
 * name; here the fallback would have to be a *claim* about two series, and
 * this client cannot make one for a kind the server invented after it
 * shipped. `related to` says only what a row with a target already says by
 * having one.
 *
 * Beside `kindLabel` rather than in the component for the same reason it is:
 * a fallback that lives in two places drifts in one of them.
 */
export const relation = (kind: string): string =>
  KIND_RELATIONS[kind] ?? 'related to'

/**
 * The sentence the expanded row shows above its accept controls.
 *
 * States the consequence in the terms that bite, and states the *absence*
 * of one too: nothing on disk is touched, which is the difference between
 * this accept and the ComicInfo write-back C3 §6 keeps deliberately
 * separate.
 *
 * `set_language` is omitted when null rather than rendered as "language
 * unknown" — the form offers it, and an accept that did not set it made no
 * claim about language at all.
 *
 * **It says "from N works" rather than "move N works", because nothing on
 * disk moves.** This read `move ${works_moved} works` until a reproduction
 * from a clean database — scan a scratch tree of four archives, accept,
 * rescan — showed the two matched volumes arriving as the new edition of the
 * target's works *and* their original works still standing under the old
 * series over the same files: six `member` rows over four files. That was
 * O73; `works_moved`'s own doc comment in `proposals.rs` already argues that
 * a wrong number in this line "is a false statement to the person deciding",
 * and a wrong *verb* is the same thing, so the sentence was fixed on the spot
 * rather than waiting on the store. [`THE_FOLD`] is the other half, and the
 * panel draws it beneath this — it once named the duplication O73 was about;
 * now that the fold closes it on the next scan, it names what the fold moves,
 * what it cannot, and what it discards.
 *
 * **And it says when.** The same reproduction showed the accept changing no
 * `edition` row at all: it writes the rule and marks the proposal accepted,
 * and the next scan is what derives anything from it.
 */
export function consequence(
  target: { title: string },
  rule: Pick<Rule, 'set_edition_key' | 'set_kind' | 'set_language'>,
  res: Resolution,
): string {
  const parts = [`add edition ${rule.set_edition_key ?? '(none)'}`]
  if (rule.set_kind) parts.push(`kind ${rule.set_kind}`)
  if (rule.set_language) parts.push(`language ${rule.set_language}`)
  const more =
    res.also_resolves.length > 0
      ? `, resolves ${res.also_resolves.length} more proposal${
          res.also_resolves.length === 1 ? '' : 's'
        }`
      : ''
  return `Accepting will: ${parts.join(' · ')} to series ${target.title}, from ${
    res.works_moved
  } works${more}, and keep every volume number as-is. Nothing on disk is touched, and nothing in the library changes until the next scan.`
}

/**
 * What an accept also does on the next scan, drawn beneath [`consequence`].
 *
 * A separate line rather than another clause of that sentence, because it is
 * not part of what accepting is *for*: the new edition is created and is
 * correct — measured, with `edition.kind` following the rule since
 * `derive::write_edition_metadata` — and folding the now-superseded works
 * away is a second effect nobody asked for in so many words. "Accepting
 * will:" is a statement of intent, and this belongs outside it.
 *
 * **No count in it, deliberately.** The number is already in the sentence
 * above and reading `works_moved` twice is two places for it to be wrong;
 * this line names the *kind* of thing that happens, which does not vary with
 * how many works are in reach.
 *
 * **Rewritten when O73 closed, 2026-08-27, because the fact changed rather
 * than the wording.** Until then this line named O73 — the duplication left
 * behind by a scan that created the new edition and emptied nothing. The fold
 * built to close it (`derive::superseded`, `fold::absorb`) now removes the
 * superseded works on that same scan and carries a reader's position, notes
 * and marked pages onto the surviving edition, proved against real archives
 * in `docs/notes.md`. What the sentence could not promise before it closed —
 * that the fold always succeeds — it still cannot: `fold::absorb` refuses
 * per work, leaving it standing, wherever moving a reader's own state would
 * not be safe (an edition key nothing maps, a note id one reader holds on
 * both works in two copies that differ — two copies identical in every
 * column are one note, and since O78 the fold keeps the surviving work's and
 * goes ahead). That refusal is the rarer path now rather than the only one,
 * which is why it is named here in a clause rather than carrying its own
 * line.
 *
 * **"Moves onto this edition instead" was an unqualified promise the store
 * has never made, and the qualification is the point of this control.** This
 * is what *causes* the fold, so `works_moved`'s own doc in `proposals.rs`
 * governs it — "a wrong number in this line is a false statement to the
 * person deciding" — and three of `fold::Moved`'s own fields exist because
 * three things do not move. `collisions`: a page the surviving edition
 * already has marked keeps its own marks and the incoming ones die with the
 * work. `lost_positions`: when both sides hold a place in one edition the
 * older is destroyed. `discarded`: the work's `work_tag` rows and its
 * `collection_item` places — a reader's own curation — and its `credit` and
 * `alignment` rows, hand corrections among them, which is **O76**. All three
 * states are constructed by this branch's own tests, so none is a hypothesis.
 * Two clauses therefore replace the promise: the move is what the fold *can*
 * move, and what it cannot carry is named rather than left to be discovered.
 */
export const THE_FOLD =
  'Accepting folds these works together on the next scan: the volumes named above are removed as their own works, and the reader positions, notes and marked pages on them move onto this edition wherever they can — a page this edition already has marked keeps its own marks, and where both sides hold a place in one edition only the newer survives. What the fold cannot carry it discards with the works: their tags, their places in reading lists, and any alignment corrected by hand. A fold that cannot move a reader’s own state safely refuses instead, leaving the volumes standing rather than losing anything.'

/**
 * The refusal sentences a candidate rule runs into — one line per condition
 * `Resolution` carries, in field order, naming what it would have moved
 * wherever the condition has something to name. Empty means an accept is
 * safe to offer.
 *
 * **Enumerates all five refusal fields, not fewer.** `also_resolves` is the
 * sixth field on `Resolution` and is deliberately absent here: answering
 * more proposals is the reason a rule gets widened, not a reason to refuse
 * it. A form that checks fewer than the five below offers an accept the
 * server then refuses on the field nobody mirrored.
 */
export function refusals(res: Resolution): string[] {
  const lines: string[] = []
  if (res.collateral.length > 0)
    lines.push(
      `This decides for series nobody asked about: ${res.collateral.join(', ')}.`,
    )
  if (res.targets_reached.length > 0)
    lines.push(
      `This also stamps its own volumes: ${res.targets_reached.join(', ')}.`,
    )
  if (res.entries_matched === 0) lines.push('This rule matches nothing.')
  if (!res.subject_reached)
    lines.push('This rule never reaches the proposal’s own subject.')
  if (res.works_moved === 0) lines.push('Accepting this would change nothing.')
  return lines
}
