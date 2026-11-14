import { noteCountLine } from './annotations'
import type { Display } from '../reading'
import type { Chapter, Resolver } from '../resolver'
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

/** `positionLine`'s and `spokenPosition`'s inputs. `slot` and `total` are zero/whole-count as everywhere else internal; `page` is already display-ready (1-based) — the same boundary convention `App.tsx`'s `altAt` already applies before handing a page number to something a person reads. */
export interface PositionInput {
  slot: number
  total: number
  page: number
  /**
   * Whose position this is, said first when set.
   *
   * `readoutLines` sets it from `reportPoint`'s `name` — on a gap and on a
   * borrowed page, where the line reports the *authority's* slot and the
   * authority's slot count while the reader is looking at a different
   * edition. The percentage below is
   * already allowed to change on a swap — see this function's own doc for
   * why hiding that would be the dishonest option — but changing the *scale*
   * it is measured against with nothing on screen to say so is a different
   * and worse claim, and this is what says so.
   *
   * Absent on an edition's own page, which is every other case: the edition
   * is the one on screen, and naming it in every frame would be noise.
   */
  edition?: string
}

/** How far through, as a whole percentage — the one rounding `positionLine` and `spokenPosition` share. */
function percentOf(slot: number, total: number): number {
  return Math.round(((slot + 1) / total) * 100)
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
  const percent = percentOf(slot, total)
  const line = `slot ${slot + 1} / ${total} · p. ${page} · ${percent}%`
  return edition ? `${edition} ${line}` : line
}

/**
 * Where a turn landed, for a screen reader — spec 2026-09-26 §3.5: the page
 * and how far through, in words. The position line's own text, "slot 6 / 18 ·
 * p. 6 · 33%", read aloud is noise.
 *
 * The edition, when there is one, is said first and is not an abbreviation to
 * drop: on a borrowed page the page number and percentage are the authority
 * edition's, and hearing them with nothing saying whose would be the silent
 * change of scale `PositionInput.edition` describes. The comma is a pause.
 */
export function spokenPosition({
  slot,
  total,
  page,
  edition,
}: PositionInput): string {
  const line = `page ${page}, ${percentOf(slot, total)} percent`
  return edition ? `${edition}, ${line}` : line
}

/**
 * The chapter code a member's path names, stripped of its directory
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
 * progress. A readout gated on `state.viewing !== state.authority` would go
 * silent while the reader is settled on a spoke, which is the one time this
 * readout matters most: `reading.ts`'s `navigate` moves `authority` to wherever
 * `viewing` was, so a reader who has stepped into a spoke edition and kept
 * reading stays there, `viewing === authority`, for the rest of the session.
 * `(hub, viewing)` instead of `(authority, viewing)` for the same reason:
 * `authority` can wander to any edition, but "how much can the edition on
 * screen be trusted against the canonical source" is a question about `hub`,
 * not about wherever the reader's position happens to be currently recorded.
 *
 * `undefined` while `viewing` is the hub itself: there is no cross-edition
 * pairing to report on, and calling `verdictFor` on the hub against itself
 * would find no stored self-edge and read, per `healthLine`'s own contract,
 * as `verdict unknown` — a false alarm for the most common state of all,
 * reading the hub unswapped.
 *
 * `hasCounterpart` is passed as `true` unconditionally, and **what makes that
 * safe is the call site's own `shown.kind === 'at'` guard, not anything about
 * when `Chrome` mounts.** `Chrome` mounts in a gap, and `readoutLines`
 * chooses between this function and `gapHealth` on `shown.kind`.
 *
 * The distinction is load-bearing rather than pedantic. Anyone folding
 * `gapHealth` back into this function — the hub-vs-authority question the
 * design deliberately left open is the likely reason to try — must not assume
 * a counterpart always exists here. It does not: a `sound` pairing with a gap
 * at the slot on screen would then take `healthLine`'s one silent branch, and
 * the readout row would go quiet directly beneath `Gap`'s own verdict sentence.
 * That is the regression `gapHealth` exists to prevent, and no test would
 * catch it — the row simply says less.
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
 * a reader whose authority is a spoke and who swaps to the hub would get
 * silence in the readout row while `Gap` printed a verdict sentence about a
 * different pairing. This reports the same pair `Gap` does, so the view and
 * the row cannot contradict each other.
 *
 * **`Gap` is usually not on screen when this runs, and the rule survives
 * that.** With `modules/borrow.ts` at its default a borrowed page stands where
 * the gap would have been, and this line still reports the `from`↔`viewing`
 * pairing — which is the row's *only* statement that the position is unpaired
 * at all, since nothing in the page area says so until the mark is pressed and
 * `Gap` appears. The two must agree the moment it is, and reaching for
 * `healthReadout` here because "there is no `Gap` to contradict" would put
 * silence under exactly the screen that needs the sentence most.
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
 * and never claims more precision than "at this pace": a bare `22m` reads as a
 * deadline, `~22m` reads as a guess, and only the second is honest about what
 * the number is.
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
 * turn whatever span it covers into a rate — `nextTurns` is what decides how
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
 * `skip` exists for exactly one caller — `hooks.ts`'s `useReadingClock`,
 * on the first run after a session loads. Recording session-load itself as a
 * "turn" would make the interval between mount and the sidecar/wasm finishing
 * to load look like a page turn, and the first estimate would be derived
 * entirely from load latency, before a single arrow key had been pressed.
 * `skip` true returns `turns` unchanged, so that first call leaves no trace;
 * `useReadingClock` still owns *deciding* when a call is the one to skip (its
 * own `restored` ref), since that is effect wiring, not arithmetic.
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

/**
 * The readout row's five possible segments, already gated by whichever
 * `ReadoutsPrefs` a reader has enabled — `undefined` for a disabled or
 * silent one.
 */
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
 * chapter / notes — regardless of which fields `lines` sets or in what order
 * the caller built it.
 *
 * This is what keeps the row from reflowing as values change: the *order*
 * segments could appear in never depends on which ones currently have
 * something to say, only their presence does — a segment that goes quiet
 * (health on a sound pair) or comes and goes (the reading-time estimate,
 * before and after a second page turns) never reshuffles anything already
 * on screen, it only removes or adds itself at its own fixed position in
 * the join. `Chrome.tsx` pairs this with a fixed-height row whose text is
 * `white-space: nowrap`, so a change in the joined string's *length* (the
 * estimate a digit longer, say) never spills a second line out of the row.
 *
 * Health sits second, not last. `Chrome.tsx`'s `overflow: hidden` +
 * `text-overflow: ellipsis` clips from the end of this joined string, so
 * whatever is ordered last is the first thing a narrow viewport silently
 * drops — and on a narrow viewport with all five readouts on and worst-case
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

/**
 * Where the readouts report from.
 *
 * In a gap that is the **authority** slot the gap was found at — a real
 * position in the work, which only the *viewed* edition has no page for — and
 * `name` is set so the change of scale is never silent: the line reports one
 * edition's slot and slot count while the reader is looking at another, and
 * `positionLine` prefixes the edition to say so. `Gap.tsx` says the same thing
 * in words directly above the row — where a gap is what is on screen, which
 * with `modules/borrow.ts` is not the common case (see below).
 *
 * **`Display` has three arms and this has two, and what makes that sound is a
 * property of the third rather than of this function.** `withBorrow` copies a
 * gap's `from` into the `borrowed` it returns unchanged — same field, same
 * shape, same units, still the authority position the gap was found at — so
 * the `else` here answers for `gap` and `borrowed` together and answers the
 * same thing. That is load-bearing and is asserted nowhere: a `borrowed` arm
 * that renamed `from`, or set it to the **lender** (which is the obvious thing
 * to reach for, since the lender is whose page is on screen), would move every
 * readout onto a different edition's slot scale, with the edition prefix
 * dutifully naming the wrong one. It would still typecheck, and the row would
 * still render.
 *
 * `undefined` for `name` while reading, which is every other case: the edition
 * is the one on screen, and naming it in every frame would be noise.
 */
export function reportPoint(shown: Display): {
  edition: string
  slot: number
  name: string | undefined
} {
  return shown.kind === 'at'
    ? { edition: shown.edition, slot: shown.slot, name: undefined }
    : {
        edition: shown.from.edition,
        slot: shown.from.slot,
        name: shown.from.edition,
      }
}

/** Everything about the clock the reading-time line needs, with `now` supplied rather than read. */
export interface ReadoutClock {
  /** `Date.now()` at mount — "since the reader opened the reader", not since the sidecar loaded. */
  startedAt: number
  /** Recent page-turn timestamps, capped by `nextTurns`. */
  turns: number[]
  /**
   * The current time, as a parameter rather than a `Date.now()` in here.
   * `timeLine` is the one readout whose output moves on its own, and a test
   * that cannot fix the clock cannot assert on it.
   */
  now: number
}

/** The readout lines `Chrome` lays out, the progress fraction the bottom
 * bar's line is filled to (since style pass ③), and the line the reader's
 * live region speaks (spec 2026-09-26 §3.5), which `App.tsx` renders itself. */
export interface ReadoutLines {
  position: string
  chapter: string
  health: string | undefined
  time: string
  notes: string | undefined
  /** The position for the reader's live region — `spokenPosition`, or the empty string on a gap, where `Gap`'s own `role="status"` already speaks for it (spec 2026-09-26 §3.5). */
  spoken: string
  /**
   * How far through the edition, 0 to 1 — not a line of text, but the bottom
   * bar's progress line (spec 2026-09-17 §8.4), computed here from the same
   * `(slot + 1) / total` the position line turns into its percentage.
   * Clamped at 1 for a slot count that has gone backwards, the case `time`'s
   * `Math.max` guards — and this is exactly where the two *can* disagree:
   * `positionLine` has no such clamp, so in that state it prints a percentage
   * past 100% while this holds at 1.
   */
  progress: number
}

/**
 * Every field, from one `Display`.
 *
 * `Chrome` takes strings (and one fraction) and lays them out; **every branch
 * behind them is here** rather than in a render body where nothing can reach
 * it. Most of the calls are one-liners into the functions above, and the
 * value of the function is the ones that are not:
 *
 * - `?? slot + 1` for an edition the resolver cannot count, so a line still
 *   renders rather than `NaN%`.
 * - `Math.max(total - (slot + 1), 0)` for a slot count that has gone backwards,
 *   which would otherwise put a negative into the remaining-time estimate.
 * - `gapHealth` rather than `healthReadout` in a gap, so the view and the row
 *   beneath it cannot contradict each other — see `gapHealth`'s own doc for the
 *   silence that produces without it.
 * - `Math.min((from.slot + 1) / total, 1)` for `progress`, clamping the same
 *   slot count the same way — but unlike `time`'s clamp, this one has no
 *   matching clamp on `position`'s percentage, so the two fields can disagree
 *   in exactly that state (see `progress`'s own doc on `ReadoutLines`).
 *
 * The note count is deliberately against *every* note on the work rather than
 * the ones a display preference currently admits: "2 notes here of 7" is a fact
 * about the work, and a total that shrank when `travel` was switched off would
 * be a different and much stranger claim. That is the caller's `notes.total`,
 * and this only says which two numbers go in.
 */
export function readoutLines(
  shown: Display,
  resolver: Resolver,
  alignments: Alignment[],
  hub: string,
  notes: { here: number; total: number },
  clock: ReadoutClock,
): ReadoutLines {
  const from = reportPoint(shown)
  const total = resolver.slotCount(from.edition) ?? from.slot + 1
  const page = resolver.slotAt(from.edition, from.slot)!.page + 1
  return {
    position: positionLine({
      slot: from.slot,
      total,
      page,
      edition: from.name,
    }),
    chapter: chapterReadout(
      resolver.chapterAt(from.edition, from.slot),
      alignments,
      hub,
    ),
    health:
      shown.kind === 'at'
        ? healthReadout(alignments, hub, shown.edition)
        : gapHealth(alignments, shown.from.edition, shown.edition),
    time: timeLine({
      elapsedMs: clock.now - clock.startedAt,
      pagesPerMin: pagesPerMinute(clock.turns, clock.now),
      slotsLeft: Math.max(total - (from.slot + 1), 0),
    }),
    notes: noteCountLine(notes.here, notes.total),
    spoken:
      shown.kind === 'gap'
        ? ''
        : spokenPosition({
            slot: from.slot,
            total,
            page,
            edition: from.name,
          }),
    progress: Math.min((from.slot + 1) / total, 1),
  }
}
