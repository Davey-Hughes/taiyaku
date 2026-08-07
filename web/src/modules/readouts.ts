import type { Chapter } from '../resolver'
import { verdictFor, type Alignment, type Verdict } from '../session'
import type { ReaderModule } from '../module'

/** Which readout lines a reader has switched on. */
export interface ReadoutsPrefs {
  position: boolean
  time: boolean
  chapter: boolean
  health: boolean
  /** The note count — `modules/annotations.ts`'s `noteCountLine`. */
  notes: boolean
}

/**
 * The status readouts — position, reading time, chapter, alignment health —
 * each independently switchable.
 *
 * Claims no keys: unlike `direction`'s `d` or `regroup`'s `b`, nothing here
 * is toggled mid-read. A reader turns a line on or off in settings, the same
 * way `motion`'s duration is tuned rather than pressed.
 */
export const readoutsModule: ReaderModule<ReadoutsPrefs> = {
  id: 'readouts',
  label: 'Readouts',
  defaults: {
    position: true,
    time: true,
    chapter: true,
    health: true,
    notes: true,
  },
}

/** `positionLine`'s inputs. `slot` and `total` are zero/whole-count as everywhere else internal; `page` is already display-ready (1-based) — the same boundary convention `App.tsx`'s `altAt` already applies before handing a page number to something a person reads. */
export interface PositionInput {
  slot: number
  total: number
  page: number
  /**
   * Whose position this is, prefixed to the line when set.
   *
   * Set by exactly one caller: `App.tsx` while showing a gap, where the line
   * reports the *authority's* slot and the authority's slot count while the
   * reader is looking at a different edition. The percentage below is
   * already allowed to change on a swap — see this function's own doc for
   * why hiding that would be the dishonest option — but changing the *scale*
   * it is measured against with nothing on screen to say so is a different
   * and worse claim, and this is what says so.
   *
   * Absent while reading, which is every other case: the edition is the one
   * on screen, and naming it in every frame would be noise.
   */
  edition?: string
}

/**
 * `slot 88 / 195 · p. 84 · 45%`.
 *
 * The percentage is derived from the 1-based slot (`slot + 1`), not the raw
 * zero-based one — otherwise the very first slot would read 0%, and the
 * last would read one short of 100%, exactly the off-by-one this exists to
 * avoid. It is never hidden: `raw-mono` and `en-mono` do not have the same
 * slot count, so the percentage changing on a swap is the honest answer —
 * hiding it would imply the two editions are the same length, which they
 * are not.
 */
export function positionLine({
  slot,
  total,
  page,
  edition,
}: PositionInput): string {
  const percent = Math.round(((slot + 1) / total) * 100)
  const line = `slot ${slot + 1} / ${total} · p. ${page} · ${percent}%`
  return edition ? `${edition} ${line}` : line
}

/**
 * The chapter code a member's stored path names, stripped of its directory
 * and extension and taken from after the last ` - ` separator if the
 * filename has one.
 *
 * A fan-scan filename is typically `<group> - <title> - <code>.cbz` (the
 * corpus's own `en-fan` members are exactly this shape:
 * `en-fan/Go-Toubun no Hanayome - Fan Translated - c005.5.cbz`), so the code
 * is whatever comes after the last dash-delimited segment. The extension is
 * stripped by matching only the trailing `.<no dot or slash>+`, not the
 * first dot encountered, so a code that is itself dotted (`c005.5`) survives
 * intact rather than being truncated at its own first dot.
 *
 * This parses a *filename*, never a chapter *number* — `taiyaku-core`'s own
 * `chapter_at` already refuses to guess a chapter number from a name (see
 * `notes.md`), and this does not reopen that: it extracts whatever label the
 * file already carries, verbatim, and does not interpret it as a sequence.
 */
function chapterCode(path: string): string {
  const base = path.split('/').pop() ?? path
  const stem = base.replace(/\.[^./]+$/, '')
  const parts = stem.split(' - ')
  return parts[parts.length - 1]
}

/**
 * `c002 · 3 of 7`, or `no chapter edition` where the work has none.
 *
 * The absent case is the common one — most sidecars in this corpus have no
 * chapter-organised edition at all — so it says so in words rather than
 * rendering nothing, the same reasoning `healthLine`'s unknown case rests
 * on: an empty readout reads as a bug, a labelled one reads as a fact about
 * this particular work.
 */
export function chapterLine(chapter: Chapter | undefined): string {
  if (!chapter) return 'no chapter edition'
  return `${chapterCode(chapter.path)} · ${chapter.member + 1} of ${chapter.of}`
}

/**
 * `chapterLine`'s text, marked with a leading `~` where it cannot be shown
 * as settled fact.
 *
 * `chapterAt` (`Resolver.chapterAt`) always resolves *through* whichever
 * alignments the sidecar stores — hub-and-spoke, so through the hub for any
 * edition that is not itself the chapter-organised one — and `Resolver`
 * reports neither which edition backs a chapter nor which edges its answer
 * composed through. Reporting the exact composed verdict would mean
 * reimplementing `taiyaku-core`'s run composition here, the one thing
 * `session.ts`'s `verdictFor` doc already refuses to do. What is knowable
 * without guessing is whether *any* alignment touching the hub is non-sound;
 * if one is, a chapter this sidecar can produce is not unconditionally
 * certain, and this marks it rather than presenting it clean. In this
 * corpus that is always true — `raw-mono → en-fan` is the sidecar's only
 * chapter-organised edge and it is `fragmented` — so every chapter shown
 * here carries the mark, on every edition, not only while viewing `en-fan`
 * itself. That is deliberately conservative: this cannot tell whether the
 * edition currently being viewed *is* the chapter edition (in which case
 * membership would be a plain fact about that edition, not a resolved one),
 * so it marks every chapter a fragile sidecar can produce rather than
 * risking the one case that would read as certain and be wrong.
 *
 * The `~` reuses `timeLine`'s own vocabulary for "derived, not stored" —
 * one convention for "this number is a resolution, not a sidecar fact"
 * rather than two unrelated ones in the same row. Never applied to the
 * absent case: "no chapter edition" is a definite fact about the work, not
 * an estimate of anything.
 */
export function chapterReadout(
  chapter: Chapter | undefined,
  alignments: Alignment[],
  hub: string,
): string {
  const line = chapterLine(chapter)
  if (!chapter) return line
  const fragile = alignments.some(
    (a) => (a.from === hub || a.to === hub) && a.quality?.verdict !== 'sound',
  )
  return fragile ? `~${line}` : line
}

/**
 * The alignment-health line: `<verdict> pair`, `<verdict> pair · no
 * counterpart here`, or silence on a sound pair with a counterpart.
 *
 * `verdict === undefined` reports `'verdict unknown'` rather than nothing —
 * the load-bearing case. `session.ts`'s `verdictFor` returns `undefined`
 * both for a pair with no stored edge (the sidecar is hub-and-spoke, so a
 * spoke-to-spoke pair composes through the hub and has none) and for a
 * stored edge whose `quality` block is absent. Rendering nothing there would
 * let a reader take silence for `sound`, which is exactly the twenty-two
 * needles' lesson: acceptance is a claim about calibration, and only the
 * verdict is a claim about the alignment.
 *
 * `hasCounterpart` is reported even on an otherwise-sound pair: `sound`
 * describes the pairing as a whole, not every slot in it — `Gap.tsx`'s own
 * sentence for a sound pairing says as much ("this is one isolated gap, not
 * a sign of trouble") — so a slot-level absence still earns a word rather
 * than being swallowed by the pairing's overall verdict.
 */
export function healthLine(
  verdict: Verdict | undefined,
  hasCounterpart: boolean,
): string | undefined {
  if (verdict === undefined) return 'verdict unknown'
  if (verdict === 'sound' && hasCounterpart) return undefined
  return hasCounterpart
    ? `${verdict} pair`
    : `${verdict} pair · no counterpart here`
}

/**
 * `healthLine`, for the edition currently on screen against the hub.
 *
 * Always computed from `(hub, viewing)`, not gated on whether a swap is in
 * progress. The toolbar badge this readout replaces only spoke up while
 * `state.viewing !== state.authority` — but `reading.ts`'s `navigate` moves
 * `authority` to wherever `viewing` was, so a reader who has stepped into a
 * spoke edition and kept reading stays there, `viewing === authority`, for
 * the rest of the session. A readout gated on "mid-swap" would go silent
 * exactly then — while the reader is settled on a spoke, which is the one
 * time this readout matters most. `(hub, viewing)` instead of `(authority,
 * viewing)` for the same reason: `authority` can wander to any edition, but
 * "how much can the edition on screen be trusted against the canonical
 * source" is a question about `hub`, not about wherever the reader's
 * position happens to be currently recorded.
 *
 * `undefined` while `viewing` is the hub itself: there is no cross-edition
 * pairing to report on, and calling `verdictFor` on the hub against itself
 * would find no stored self-edge and read, per `healthLine`'s own contract,
 * as `verdict unknown` — a false alarm for the most common state of all,
 * reading the hub unswapped.
 *
 * `hasCounterpart` is passed as `true` unconditionally, and **what makes that
 * safe is the call site's own `shown.kind === 'at'` guard, not anything about
 * when `Chrome` mounts.** This paragraph used to say the latter — that a gap
 * was handled by `Gap.tsx` before `Chrome` was ever rendered, so this could
 * not be called from one. That was true until O25 closed, and O25's fix is
 * exactly the removal of it: `Chrome` now mounts in a gap, and `App.tsx`
 * chooses between this function and `gapHealth` on `shown.kind` instead.
 *
 * The distinction is load-bearing rather than pedantic. Anyone folding
 * `gapHealth` back into this function — the hub-vs-authority question the
 * design deliberately left open is the likely reason to try — would, on the
 * old reasoning, believe a counterpart always exists here. It does not: a
 * `sound` pairing with a gap at the slot on screen would then take
 * `healthLine`'s one silent branch, and the readout row would go quiet
 * directly beneath `Gap`'s own verdict sentence. That is the regression
 * `gapHealth` exists to prevent, and no test would catch it — the row simply
 * says less.
 *
 * `healthLine` still takes the flag explicitly rather than this function
 * assuming it away, so its own contract does not depend on this one caller's
 * particular gating.
 */
export function healthReadout(
  alignments: Alignment[],
  hub: string,
  viewing: string,
): string | undefined {
  if (viewing === hub) return undefined
  return healthLine(verdictFor(alignments, hub, viewing), true)
}

/**
 * `healthLine` for a gap — the pairing the gap is actually about, and the
 * absence, together.
 *
 * **Deliberately not `healthReadout`, and the difference is the point.** That
 * function reports the `hub`↔`viewing` pair and answers `undefined` while
 * `viewing` is the hub itself; `Gap.tsx` reports the `from`↔`viewing` pair,
 * where `from` is the authority the gap was found at. In a gap those disagree:
 * a reader whose authority is a spoke and who swaps to the hub would see
 * `Gap`'s verdict sentence with silence in the readout row directly beneath
 * it. This reports the same pair `Gap` does, so the view and the row cannot
 * contradict each other.
 *
 * `hasCounterpart` is `false` unconditionally, which is what a gap *is* — and
 * it is why this returns `string` rather than `string | undefined`:
 * `healthLine`'s only silent branch requires a counterpart, so with `false`
 * there is always something to say. That is the intended behaviour, not an
 * accident of the current implementation — silence here would let a reader
 * take a gap for an ordinary page.
 *
 * Unlike `healthReadout` this takes the authority explicitly rather than
 * closing over `hub`, because in a gap the authority is the edition the
 * position is recorded against and the hub may be neither side of the pair.
 */
export function gapHealth(
  alignments: Alignment[],
  from: string,
  viewing: string,
): string {
  return healthLine(verdictFor(alignments, from, viewing), false)!
}

/** `timeLine`'s inputs — see its own doc for where each comes from. */
export interface TimeInput {
  /** Wall-clock time since this reading session began. */
  elapsedMs: number
  /** The pace `pagesPerMinute` derived from recent page turns; 0 before a second one. */
  pagesPerMin: number
  /** Slots remaining in the edition currently on screen. */
  slotsLeft: number
}

/** A duration under a minute reads as `<1m`, never `0m` — zero would claim nothing has happened yet, when a session that has just started plainly has. */
function minutesLabel(ms: number): string {
  const minutes = Math.round(ms / 60_000)
  return minutes < 1 ? '<1m' : `${minutes}m`
}

/**
 * Session elapsed, plus an estimate of what is left — client-side only.
 *
 * The elapsed figure is a plain duration. The remainder is a projection from
 * a recent, noisy rate (see `pagesPerMinute`), so it is always marked `~`
 * and never claims more precision than "at this pace" — the brief's "not
 * shown to the minute" is this: a bare `22m` reads as a deadline, `~22m`
 * reads as a guess, and only the second is honest about what the number is.
 *
 * Omits the estimate entirely — no `~` anywhere in the string — rather than
 * dividing by a rate of zero, which is exactly the state before a second
 * page has turned in this session. Reporting `~Infinitym left` or `~NaNm
 * left` would be a worse failure than saying nothing: a reader can tell
 * "no estimate yet" apart from a real number, but not apart from garbage
 * that merely looks like one.
 */
export function timeLine({
  elapsedMs,
  pagesPerMin,
  slotsLeft,
}: TimeInput): string {
  const elapsed = minutesLabel(elapsedMs)
  if (pagesPerMin <= 0) return `${elapsed} read`
  const remaining = Math.round(slotsLeft / pagesPerMin)
  return `${elapsed} read · ~${remaining}m left`
}

/**
 * Pages per minute across `turns`' own span — the pace `timeLine`'s estimate
 * is built from.
 *
 * `turns` is the caller's own rolling window of page-turn timestamps (ms);
 * this makes no claim about how large that window should be, only how to
 * turn whatever span it covers into a rate — `App.tsx` is what decides how
 * many recent turns to keep, since that is a UI-tuning choice, not a
 * derivation this pure function should own.
 *
 * 0 with fewer than two timestamps: a rate is an interval between two
 * points, and there is nothing to divide by before a second page has
 * turned. 0 rather than a divide-by-zero crash or `Infinity` on a
 * degenerate span (every turn sharing one timestamp) too — a rate this
 * function cannot honestly compute is reported the same way as no rate at
 * all, not as a number that happens to print.
 */
export function pagesPerMinute(turns: number[], now: number): number {
  if (turns.length < 2) return 0
  const span = now - Math.min(...turns)
  if (span <= 0) return 0
  return ((turns.length - 1) * 60_000) / span
}

/** How many recent page-turn timestamps `nextTurns` keeps — see its own doc. */
const TURN_HISTORY_LIMIT = 10

/**
 * The reading-time readout's turn history, advanced by one call.
 *
 * `skip` exists for exactly one caller — `App.tsx`'s turn-recording effect,
 * on the first run after a session loads. Recording session-load itself as a
 * "turn" makes the interval between mount and the sidecar/wasm finishing to
 * load look like a page turn: the very first estimate this project ever
 * rendered was `~6m left`, derived entirely from load latency, before a
 * single arrow key had been pressed — caught by driving a real browser, not
 * by any test, since nothing exercised this arithmetic directly. `skip` true
 * returns `turns` unchanged, so that first call leaves no trace; `App.tsx`
 * still owns *deciding* when a call is the one to skip (its own
 * `restoredTurn` ref), since that is effect wiring, not arithmetic.
 *
 * Capped at the ten most recent turns otherwise — `pagesPerMinute` wants a
 * *recent* pace, not a session-wide average that a reader who paused for an
 * hour would otherwise never recover from.
 */
export function nextTurns(
  turns: number[],
  now: number,
  skip: boolean,
): number[] {
  if (skip) return turns
  return [...turns, now].slice(-TURN_HISTORY_LIMIT)
}

/** The readout row's four possible segments, already gated by whichever `ReadoutsPrefs` a reader has enabled — `undefined` for a disabled or silent one. */
export interface RowLines {
  position?: string
  time?: string
  chapter?: string
  health?: string
  /** Derived by `modules/annotations.ts`, laid out here. */
  notes?: string
}

/**
 * The readout row's text, in the fixed order position / health / time /
 * chapter — regardless of which fields `lines` sets or in what order the
 * caller built it.
 *
 * This is what keeps the row from reflowing as values change: the *order*
 * segments could appear in never depends on which ones currently have
 * something to say, only their presence does — a segment that goes quiet
 * (health on a sound pair) or comes and goes (the reading-time estimate,
 * before and after a second page turns) never reshuffles anything already
 * on screen, it only removes or adds itself at its own fixed position in
 * the join. `Chrome.tsx` pairs this with a `white-space: nowrap` row of
 * fixed height, so a change in the joined string's *length* — the estimate
 * ticking a digit longer, say — can never grow the row taller either.
 *
 * Health sits second, not last. `Chrome.tsx`'s `overflow: hidden` +
 * `text-overflow: ellipsis` clips from the end of this joined string, so
 * whatever is ordered last is the first thing a narrow viewport silently
 * drops — and on a narrow viewport with all four readouts on and worst-case
 * values (a long marked chapter code, a full time estimate, `fragmented
 * pair · no counterpart here`), something has to go. Health is the one
 * segment this project cannot afford to lose first: it is the only readout
 * that ever says a pairing cannot be trusted, and `healthLine`'s own doc
 * already refuses to let silence be mistaken for `sound` — clipping it away
 * first would recreate exactly that failure by a different route. Position
 * stays first, ahead of health, as the one piece of orientation ("where am
 * I") a reader needs even more urgently than the trust judgment about it.
 * Time and chapter move last precisely because they are the two a reader
 * can most afford to lose to truncation: chapter is a convenience already
 * inferable from position, and the time estimate is explicitly a guess
 * (see `timeLine`).
 *
 * The note count is last of the five, and is the one segment whose derivation
 * lives in another module (`annotations`) while its layout lives here — the
 * same split every other line already has between `readouts`' own pure
 * functions and `App.tsx`'s wiring. Last because it is the only readout a
 * reader can also see on the page itself, as margin tabs, so losing it to a
 * narrow row costs the least of anything here.
 */
export function readoutRow(lines: RowLines): string {
  return [lines.position, lines.health, lines.time, lines.chapter, lines.notes]
    .filter(Boolean)
    .join(' · ')
}
