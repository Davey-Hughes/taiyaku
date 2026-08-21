import { useEffect, useRef, useState } from 'react'
import { createDecodeQueue, type DecodeQueue } from './decode'
import { initialPrefs, persistPrefs, type ReaderModule } from './module'
import { nextTurns } from './modules/readouts'
import type { Mode } from './pairing'

/**
 * The plumbing `App.tsx` cannot express as a pure module: refs that mirror
 * state for a listener frozen at subscription, values built once for the
 * component's life, and subscriptions whose only decision is when to
 * re-subscribe.
 *
 * **This file is where the extraction map's Rule 2 is spent, and it is spent
 * on the condition that made it affordable.** That rule — a pure module beats
 * a custom hook — exists because a hook's branches can only be reached by
 * mounting something, so moving branching logic into one relabels uncovered
 * statements rather than covering them. Every hook below is mounted and
 * exercised by `hooks.test.tsx`, in `jsdom`, which is a project CI runs. A
 * hook added here without a test in that file is the failure Rule 2 names,
 * and the rule stands against it.
 *
 * **Nothing here reaches for a `Resolver`, a `Session` or the network.** Each
 * hook takes the plain values its own subscription depends on — the three
 * primitives a reading position is made of rather than the session holding
 * them — which is what keeps the dependency arrays honest: with the triggers
 * passed in as scalars, most of these need no `useExhaustiveDependencies`
 * suppression at all, where the same effects inline in `App.tsx` needed one
 * each to say "the session is a trigger, not a closure read".
 */

/**
 * A ref that always holds the latest `value`, for a listener that cannot see
 * the render it was subscribed from.
 *
 * `App.tsx` subscribes its keydown listener once per load — deliberately, so
 * that a keypress does not tear the listener down and put it back — which
 * freezes that closure's view of every piece of state at the moment of
 * subscription. Six of them are read by the handler's guards, and this is
 * how each one reaches it: written in an effect after every commit, read
 * through `.current` when a key is actually pressed.
 *
 * The initial value is `value` itself rather than a separate seed, so there
 * is no window — however brief — where the ref disagrees with the render it
 * was created in.
 */
export function useLatest<T>(value: T): { readonly current: T } {
  const ref = useRef(value)
  useEffect(() => {
    ref.current = value
  }, [value])
  return ref
}

/**
 * A value built on the first render and kept for the component's life.
 *
 * `useRef(create())` is the shape this replaces and the shape that is wrong:
 * a `useRef` argument is not lazy the way `useState`'s initializer is, so it
 * evaluates `create()` on *every* render and throws all but the first away.
 * For the two debounced writers `App.tsx` builds that is not merely wasteful
 * — each allocation carries its own timer variable, and only the retained
 * one's timer is ever armed.
 *
 * `useState(create)[0]` would also work and is rejected for what it implies:
 * a state slot advertises that something may set it, and nothing ever sets
 * these.
 *
 * `null` is the "not built yet" marker, so a `create` that returns `null`
 * would be called again on every render. Nothing here does, and a caller
 * that wanted to should say so with a wrapper object rather than teaching
 * this hook a second sentinel.
 */
export function useConstant<T>(create: () => T): T {
  const ref = useRef<T | null>(null)
  if (ref.current === null) ref.current = create()
  return ref.current
}

/**
 * A decode queue, rebuilt whenever its concurrency cap changes.
 *
 * The settings panel can tune `maxConcurrent` live, and a queue whose cap was
 * baked in at mount would make that one field the exception to "change a
 * setting, confirm it takes effect" every other prefetch setting already
 * satisfies. Rebuilding drops nothing already in flight: a job queued against
 * the old instance keeps running under its old cap, and this only changes
 * which instance the *next* `run` call goes to.
 *
 * The effect runs on mount as well as on a change, so the queue the
 * initializer built is replaced by an identical one on the first commit.
 * That is one wasted allocation at mount and it is deliberate: the
 * alternative is a "have I run before" ref whose only job is to save it.
 */
export function useDecodeQueue(maxConcurrent: number): DecodeQueue {
  const [queue, setQueue] = useState<DecodeQueue>(() =>
    createDecodeQueue(maxConcurrent),
  )
  useEffect(() => {
    setQueue(createDecodeQueue(maxConcurrent))
  }, [maxConcurrent])
  return queue
}

/** What each cell of a two-page group is currently drawing, by cell. */
export interface DrawnSrcs {
  first?: string
  second?: string
}

/**
 * What the two cells of `two` mode are actually drawing, reported by each
 * `PageView` as it presents.
 *
 * Held here rather than inside `GroupView` because a two → scroll → two
 * excursion unmounts and remounts everything at that position: as component
 * state the record would come back empty and the layout would key on the
 * turn's target for as long as it took both cells to report, which is the
 * flicker this record was introduced to stop.
 *
 * **A report that changes nothing returns the same object.** Every
 * `PageView` reports on every present, including the ones that present what
 * they were already drawing, and a fresh object for each would re-render the
 * whole page area for no visible change.
 *
 * The setter is rebuilt on every render rather than memoised, which is what
 * the inline handlers this replaced also did — `GroupView` passes it down to
 * two `PageView`s that wrap it in fresh closures of their own regardless, so
 * a stable identity here would buy nothing and would change how often
 * anything downstream re-runs.
 */
export function useDrawnSrcs(): [
  DrawnSrcs,
  (cell: 'first' | 'second', src: string) => void,
] {
  const [drawn, setDrawn] = useState<DrawnSrcs>({})
  const show = (cell: 'first' | 'second', src: string) =>
    setDrawn((was) => (was[cell] === src ? was : { ...was, [cell]: src }))
  return [drawn, show]
}

/**
 * Every registered module's preference slice, loaded once and persisted on
 * every change.
 *
 * Persisted from an effect rather than inside the setter's callers: several
 * of them update this with a functional updater, and an updater must stay
 * pure (React may invoke it more than once, e.g. under StrictMode), so the
 * `save` side effect lives here instead.
 *
 * Writes every module's slice on any one module's change rather than diffing
 * for the one that actually moved: `save` is a cheap serialise plus a
 * `setItem` on a handful of small objects, so "persist the whole map, always"
 * costs nothing measurable and cannot drift from which id actually changed
 * the way a hand-maintained diff could.
 *
 * Loaded once, from the initializer: there is exactly one work for the
 * reader's lifetime, so there is nothing to reload against. `modules` and
 * `save` are in the effect's dependencies rather than suppressed out of it —
 * both are module-level constants at the one call site, so naming them costs
 * nothing and keeps the array honest.
 */
export function usePrefsMap(
  modules: ReaderModule<unknown>[],
  load: (id: string, defaults: object) => object,
  save: (id: string, prefs: object) => void,
): [
  Record<string, object>,
  React.Dispatch<React.SetStateAction<Record<string, object>>>,
] {
  const [map, setMap] = useState<Record<string, object>>(() =>
    initialPrefs(modules, load),
  )
  useEffect(() => {
    persistPrefs(modules, map, save)
  }, [modules, map, save])
  return [map, setMap]
}

/**
 * The window's width, tracked rather than read once.
 *
 * "Is there room beside the page" stops being true the moment somebody
 * resizes the window, and a layout that only answered at load would crush the
 * page until the next reload.
 *
 * `fallback` is what a document-less environment gets. It is not dead code
 * on the way to a server renderer that does not exist: a `window.innerWidth`
 * read at module evaluation time is the shape that throws under any
 * pre-render, and answering a width instead is a one-token guard against a
 * whole class of it.
 */
export function useWindowWidth(fallback: number): number {
  const [width, setWidth] = useState(() =>
    typeof window === 'undefined' ? fallback : window.innerWidth,
  )
  useEffect(() => {
    const onResize = () => setWidth(window.innerWidth)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])
  return width
}

/** Something with a pending write that can be forced through early. */
interface Flushable {
  flush(): void
}

/**
 * Send both pending writes before the page can go away.
 *
 * A pending write must survive the tab going away, not just the session
 * staying open. `visibilitychange` firing `hidden` is the modern, reliable
 * signal for that — it fires on mobile when a tab is backgrounded or the app
 * is switched away from, which is precisely where `beforeunload` is
 * unreliable (many mobile browsers never fire it at all) and is increasingly
 * discouraged since it also blocks the back/forward cache.
 *
 * `pagehide` is added alongside it, not instead of it, for the cases
 * `visibilitychange` itself doesn't cover — some browsers route a direct
 * close of a background tab through `pagehide` without a preceding
 * `visibilitychange`. Either one flushing is enough to be safe, and both
 * firing for the same close is not a double write: a `Debounced.flush`
 * clears the pending call the moment it sends, so whichever runs second
 * finds nothing left to flush.
 *
 * Two named parameters rather than a list, because the order is the argument:
 * the note goes second only in the reading, never in the outcome — a
 * half-typed note is worth more than a page number, and it is the one thing
 * here that cannot be recomputed from anything.
 *
 * **The cleanup flushes as well as unsubscribing, which is the third way the
 * reader leaves.** Until 2026-08-09 it only removed the two listeners, so a
 * pending write survived its own component: `App.tsx`'s two debounces were
 * left armed and could fire `putProgress` or `putNotes` up to 700 ms after the
 * thing that scheduled them had gone. It surfaced as a stray `putProgress`
 * arriving in a *later test* than the one that caused it, under coverage
 * instrumentation slow enough to let a 400 ms timer outlive its `afterEach`.
 *
 * **`App` unmounts for three ordinary reasons now, and this cleanup is what
 * covers all three.** It was the SPA root when the paragraph above was
 * written, so no reader could meet the case; since spec C1 §11 step 2 it is
 * one route of four. Leaving the reader for the library unmounts it, so does
 * moving between works — `Routes.tsx`'s `<App key={work}>` — and so does
 * signing out, which is `Gate.tsx` dropping everything below it. That last one
 * is the one with teeth: `App` used to cancel both writers by hand on a
 * sign-out, and the reason was that a timer firing afterwards would write the
 * previous reader's work under whoever had signed in by then. Flushing here
 * closes that without a second list to keep complete, because the flush
 * happens *at* the unmount commit — the account cannot have changed yet, and
 * the session the write goes out under is the one that just ended.
 *
 * **Flush and not cancel, at both call sites, for two different reasons.** For
 * the position, cancelling would drop the last turn a reader made and reopen
 * them a page or two back — small, but a loss with nothing bought for it,
 * where flushing costs one `PUT` the tab was going to send anyway (and
 * `putProgress` carries `keepalive: true` precisely so it lands). For the
 * note, cancelling would destroy up to 700 ms of prose that exists in no other
 * place: not on the server, not in `notesHistory`, which only records what a
 * write returned. That is the same ranking this hook's parameter order already
 * states, and it is why `cancel` — which `notesWrite`'s own callers do use, for
 * undo and delete, where the pending write is *stale* — is wrong here, where
 * it is the freshest thing anyone has.
 *
 * Flushing also stays correct if this cleanup ever runs for something other
 * than an unmount. It does not today: both arguments come from `useConstant`,
 * so the dependencies below never change identity. But if one day they did,
 * an early flush is a write sent a moment sooner, and an early cancel is a
 * reader's work deleted.
 */
export function useFlushOnHide(position: Flushable, notes: Flushable): void {
  useEffect(() => {
    function onVisibilityChange() {
      if (document.visibilityState === 'hidden') {
        position.flush()
        notes.flush()
      }
    }
    const onHide = () => {
      position.flush()
      notes.flush()
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    window.addEventListener('pagehide', onHide)
    return () => {
      document.removeEventListener('visibilitychange', onVisibilityChange)
      window.removeEventListener('pagehide', onHide)
      position.flush()
      notes.flush()
    }
  }, [position, notes])
}

/**
 * Write the reading position whenever it moves, and never on the load that
 * restored it.
 *
 * **The first run is skipped, and exactly the first.** The load that opens a
 * work sets the position this was just told, so writing it back would be a
 * pointless `PUT` on every load. `restored` skips that one run and no other:
 * it is not a general "have things settled" guard, and a second skip would
 * silently lose a real page turn.
 *
 * **The three parts of the position are passed as scalars, and that is what
 * makes the dependency array honest.** Inline in a component this took the
 * whole session and needed an `useExhaustiveDependencies` suppression to
 * explain that the session must *not* join the array — a session's identity
 * changes on every swap too, and a swap is not a navigation (it assigns
 * `viewing` and nothing else), so depending on the object would write
 * progress every time a reader merely peeked at another edition. With the
 * three fields handed in there is no object to be tempted by and no
 * suppression to read past.
 */
export function useProgressWrite(
  work: string | undefined,
  edition: string | undefined,
  slot: number | undefined,
  write: (work: string, position: { edition: string; slot: number }) => void,
): void {
  const restored = useRef(false)
  useEffect(() => {
    if (work === undefined || edition === undefined || slot === undefined)
      return
    if (!restored.current) {
      restored.current = true
      return
    }
    write(work, { edition, slot })
  }, [work, edition, slot, write])
}

/** When this reader opened the reader, and every page turn since. */
export interface ReadingClock {
  /** Fixed at mount — "since this reader opened the reader", not since the sidecar loaded. */
  startedAt: number
  /** Turn timestamps, newest last, bounded by `nextTurns`. */
  turns: { readonly current: number[] }
}

/**
 * The reading-time readout's raw material: a start, a log of turns, and the
 * heartbeat that makes an estimate derived from them actually tick.
 *
 * **The first run is skipped, measured rather than assumed.** Recording the
 * session load itself as a turn makes the interval between mount and the
 * sidecar finishing to load look like a page turn, and the very first
 * estimate this project ever rendered was `~6m left` derived entirely from
 * load latency, before a single arrow key had been pressed. `nextTurns` —
 * pure, and covered by `readouts.test.ts` — is what actually skips; this
 * only decides when to ask.
 *
 * **A swap is not a turn.** Only the authority's edition and slot are
 * passed, so peeking at another edition records nothing: `swap` assigns
 * `viewing` alone, and counting it would inflate the pace with moves that
 * were not reading.
 *
 * `tickMs` rather than a second: the display is minute-granularity, so
 * anything finer re-renders for a change nobody could see. The interval is
 * why this holds a `useState` it never reads — nothing else re-renders as
 * time passes, because a ref changing never triggers one.
 */
export function useReadingClock(
  edition: string | undefined,
  slot: number | undefined,
  tickMs: number,
): ReadingClock {
  const turns = useRef<number[]>([])
  const restored = useRef(false)
  useEffect(() => {
    if (edition === undefined || slot === undefined) return
    const skip = !restored.current
    restored.current = true
    turns.current = nextTurns(turns.current, Date.now(), skip)
  }, [edition, slot])

  const startedAt = useRef(Date.now())
  const [, tick] = useState(0)
  useEffect(() => {
    const id = setInterval(() => tick((t) => t + 1), tickMs)
    return () => clearInterval(id)
  }, [tickMs])

  return { startedAt: startedAt.current, turns }
}

/**
 * Keep the reading position on screen while every group is mounted at once.
 *
 * Scroll mode mounts the whole volume, so — unlike single and two-page mode,
 * where only the current group is ever rendered — nothing else puts the
 * reader's position on screen. Without this, entering scroll mode at slot 150
 * shows slot 0, the top of the list. The returned ref goes on whichever
 * rendered group is the current one.
 *
 * Deliberately *not* the reverse: this does not write the position from where
 * the reader scrolls to by hand. That needs an `IntersectionObserver` over
 * every mounted group, which is real work left to a later milestone.
 */
export function useScrollToCurrent(
  mode: Mode,
  authority: string | undefined,
  slot: number | undefined,
  viewing: string | undefined,
): React.RefObject<HTMLDivElement | null> {
  const current = useRef<HTMLDivElement>(null)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `authority`, `slot` and `viewing` are triggers, not closure reads — the body touches only `mode` and the ref, and exists to re-scroll when the position changes. Removing them, which is what this rule suggests, would leave scroll mode never following the reader.
  useEffect(() => {
    if (mode !== 'scroll') return
    current.current?.scrollIntoView({ block: 'start' })
  }, [mode, authority, slot, viewing])
  return current
}
