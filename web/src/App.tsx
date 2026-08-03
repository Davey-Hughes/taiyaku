import { useEffect, useRef, useState } from 'react'
import { getProgress, getSidecar, listWorks, pageUrl, putProgress } from './api'
import { Chrome } from './Chrome'
import { debounce, type Debounced } from './debounce'
import { Gap } from './Gap'
import { intentOf, isInteractiveTarget } from './keys'
import { nearestPaired } from './nearest'
import { PageView } from './PageView'
import { groups, type Group, type Mode } from './pairing'
import { display, navigate, restore, type ReadingState } from './reading'
// This file must never become Vitest-reachable: `wasmResolver` imports
// `resolver.ts` as a value, and that module's `taiyaku-wasm` import only
// resolves once `./scripts/build-wasm.sh` has generated the gitignored
// `crates/taiyaku-wasm/pkg/`. A test importing App.tsx would pass locally
// (after that script has run) and break CI or a fresh clone (where it
// hasn't) — see docs/ci.md's `web` job notes and the global no-resolver-in-
// tests constraint.
import { wasmResolver, type Position, type Resolver } from './resolver'
import {
  applyIntent,
  initialPrevious,
  orderedEditions,
  resolveOffered,
  takeOffered,
  verdictFor,
  type Alignment,
} from './session'

/**
 * How long a burst of `navigate` calls (arrow-stepping, a filmstrip click, a
 * gap take) waits for quiet before the position actually reaches the server.
 * Arrowing through twenty pages inside this window is one `PUT`, not twenty.
 */
const PROGRESS_DEBOUNCE_MS = 400

/**
 * Enough of the sidecar to pick a work's first edition, its reading
 * direction and the verdict on a pair — `reading` is what `groups` needs to
 * know which side of a pair is drawn first, and `alignments` is what
 * `verdictFor` reads. Everything else (members, spreads, runs) stays the
 * aligner's business: the reader gets slots from the `Resolver`, below,
 * rather than deriving them here a second time.
 */
interface SidecarShape {
  work: { reading: 'rtl' | 'ltr' }
  editions: Record<string, unknown>
  hub: string
  alignments: Alignment[]
}

interface Session {
  work: string
  resolver: Resolver
  reading: 'rtl' | 'ltr'
  /** Digit keys index into this, in display order — see `orderedEditions`. */
  editions: string[]
  /** Read once at load, for `verdictFor` — see `SidecarShape`. */
  alignments: Alignment[]
  state: ReadingState
  /**
   * What `toggle` returns to, when currently viewing the authority. Seeded
   * at load via `session.ts`'s `initialPrevious`, never with `authority`
   * itself — see the doc there for why that seed matters.
   */
  previous: string
}

/** Every slot of one edition, in order. `Resolver` has no bulk accessor. */
function allSlots(resolver: Resolver, edition: string): Position[] {
  const count = resolver.slotCount(edition) ?? 0
  return Array.from({ length: count }, (_, index) => resolver.slotAt(edition, index)!)
}

/**
 * The three reading modes, over the state `keys.ts`'s intents drive.
 *
 * `Space`, digits and arrows reach `session.ts`'s `applyIntent` via the
 * keydown listener below, which calls `swap` (for `toggle`/`view`) or
 * `navigate` (for `step`) — never both for one keypress. So `viewing` really
 * can diverge from `authority`, and `display` really can report `'gap'`:
 * landing, after a swap, on a slot with no counterpart in the edition just
 * swapped to. That state is drawn by `Gap`, below, rather than folded into
 * the page-rendering branch that follows it — a gap has no slot to draw and
 * no group to belong to, so keeping it a separate early return is what lets
 * the rest of this function go on assuming `shown.kind === 'at'` throughout.
 */
function App() {
  const [session, setSession] = useState<Session>()
  const [mode, setMode] = useState<Mode>('single')
  // `Tab`'s target — not `session` state, since it names nothing about the
  // reading model, only whether the strip and readout row are on screen.
  const [chromeVisible, setChromeVisible] = useState(false)

  useEffect(() => {
    let cancelled = false
    async function load() {
      const [work] = await listWorks()
      if (!work) throw new Error('no works available')
      const json = await getSidecar(work)
      const sidecar: SidecarShape = JSON.parse(json)
      const editions = orderedEditions(sidecar.hub, Object.keys(sidecar.editions))
      const [hub] = editions
      if (!hub) throw new Error(`${work} defines no editions`)
      const resolver = await wasmResolver(json)
      // Stored progress if there is any, slot 0 of the hub if there is none —
      // see `reading.ts`'s `restore`. Read after the resolver so a failure
      // resolving the sidecar surfaces before a second request is made, and
      // because `restore` now needs the resolver itself, to validate a
      // stored slot still exists rather than trusting stale JSON outright.
      const progress = await getProgress(work)
      if (!cancelled) {
        const state = restore(progress, hub, resolver)
        setSession({
          work,
          resolver,
          reading: sidecar.work.reading,
          editions,
          alignments: sidecar.alignments,
          state,
          // Not `hub`: seeding `previous` with the edition already being
          // shown would make the first `Space` (and every one after it)
          // compute a no-op target. `initialPrevious` seeds one step ahead
          // in display order instead, so the first `Space` lands where the
          // first `2` would.
          previous: initialPrevious(editions, state.authority),
        })
      }
    }
    load().catch((err: unknown) => {
      if (!cancelled) console.error(err)
    })
    return () => {
      cancelled = true
    }
  }, [])

  // Depends on `resolver`/`editions` rather than `session` itself, so this
  // subscribes once after load rather than on every keypress — the handler
  // reads current state through `setSession`'s updater instead of a closure.
  // Adding `session` to satisfy exhaustive-deps would defeat that: the
  // listener would be torn down and re-added on every keystroke instead of
  // once, for no behavioural gain.
  // oxlint-disable react-hooks/exhaustive-deps -- session is read via setSession's updater below, not closed over; see above.
  useEffect(() => {
    if (!session) return
    const { resolver, editions } = session
    function onKeyDown(event: KeyboardEvent) {
      const intent = intentOf(event, editions)
      if (!intent) return
      if (intent.kind === 'chrome') {
        // `Tab`'s own default — focus movement — is left alone. Toggling the
        // chrome is a side effect layered on top of the browser's own `Tab`
        // handling, not a replacement for it; preventing it here is what
        // made every focusable control (mode buttons, all of the
        // filmstrip's tiles, `Gap`'s offered moves) mouse-only.
        setChromeVisible((visible) => !visible)
        return
      }
      // A reader key belongs to the page, not to whatever a previous `Tab`
      // happened to focus. Without this, tabbing onto a mode button and
      // pressing Space to actually click it would also fire the edition
      // toggle — not because two listeners raced, but because this
      // window-level listener has no way to know a focused button is about
      // to handle the very same keypress itself. Standing down leaves that
      // button's own keyboard activation as the only thing that runs.
      if (isInteractiveTarget(event.target)) return
      event.preventDefault()
      setSession((prev) => {
        if (!prev) return prev
        const { state, previous } = applyIntent(intent, prev.state, prev.previous, resolver)
        return { ...prev, state, previous }
      })
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [session?.resolver, session?.editions])
  // oxlint-enable react-hooks/exhaustive-deps

  // A ref rather than a module-level singleton or a `useState`: created once,
  // on the first render, and its pending timer must survive every render in
  // between rather than being replaced by a fresh, un-armed debounce on each
  // one. `useRef(debounce(...))` would still only ever keep the first
  // render's value, but it evaluates `debounce(...)` — allocating a fresh
  // closure and timer variable — on every render regardless, since a `useRef`
  // argument is not lazy the way `useState`'s initializer function is. The
  // `if` below is the idiom for a ref whose initial value is itself worth not
  // recomputing: every render after the first sees `current` already set and
  // skips straight past it.
  const writeProgressRef = useRef<Debounced<[string, { edition: string; slot: number }]> | null>(null)
  if (writeProgressRef.current === null) {
    writeProgressRef.current = debounce((work, position) => {
      putProgress(work, position).catch((err: unknown) => console.error(err))
    }, PROGRESS_DEBOUNCE_MS)
  }
  const writeProgress = writeProgressRef.current

  // A pending write must survive the tab going away, not just the session
  // staying open. `visibilitychange` firing `hidden` is the modern, reliable
  // signal for that — it fires on mobile when a tab is backgrounded or the
  // app is switched away from, which is precisely where `beforeunload` is
  // unreliable (many mobile browsers never fire it at all) and is
  // increasingly discouraged since it also blocks the back/forward cache.
  // `pagehide` is added alongside it, not instead of it, for the cases
  // `visibilitychange` itself doesn't cover — some browsers route a direct
  // close of a background tab through `pagehide` without a preceding
  // `visibilitychange`. Either one flushing is enough to be safe, and both
  // firing for the same close is not a double write: `Debounced.flush`
  // clears the pending call the moment it sends, so whichever one runs
  // second finds nothing left to flush.
  useEffect(() => {
    function onVisibilityChange() {
      if (document.visibilityState === 'hidden') writeProgress.flush()
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    window.addEventListener('pagehide', writeProgress.flush)
    return () => {
      document.removeEventListener('visibilitychange', onVisibilityChange)
      window.removeEventListener('pagehide', writeProgress.flush)
    }
  }, [writeProgress])

  // Progress is written here, and only here — on a change to
  // `state.authority`/`state.slot`, which is exactly what `navigate` changes
  // and `swap` never does (`swap` only ever assigns `state.viewing`; see
  // `reading.ts`). That makes this effect's dependency array the enforcement:
  // a toggle or a digit press does not touch either dependency, so it cannot
  // retrigger this effect, and there is no `if` here distinguishing the two —
  // there is nothing to distinguish.
  //
  // The first time this effect runs with a `session` is the load effect's own
  // `setSession`, restoring rather than navigating — writing back the exact
  // position `getProgress` just returned would be a pointless `PUT` on every
  // load, so `restored` skips exactly that one run and no other.
  //
  // Do not add `session` to this array to satisfy exhaustive-deps: `session`
  // itself changes on every swap too (`swap` returns a new `{...state,
  // viewing: to}` object), so depending on the whole session would fire this
  // effect — and write progress — on every swap, not just every navigation.
  // That is precisely the drift `docs/notes.md` warns about: a swap moving
  // the reader one page per press, silently, with every test still green.
  const restored = useRef(false)
  // oxlint-disable react-hooks/exhaustive-deps -- session changes on every swap too; adding it here would write progress on every swap, not just every navigation. See above.
  useEffect(() => {
    if (!session) return
    if (!restored.current) {
      restored.current = true
      return
    }
    writeProgress(session.work, { edition: session.state.authority, slot: session.state.slot })
  }, [session?.work, session?.state.authority, session?.state.slot, writeProgress])
  // oxlint-enable react-hooks/exhaustive-deps

  // Scroll mode mounts every group at once, so — unlike single/two mode,
  // where only the current group is ever rendered — nothing else puts the
  // reading position on screen. Without this, entering scroll mode at slot
  // 150 shows slot 0, the top of the list. `currentGroupRef` is attached
  // below to whichever rendered group is `currentIndex`'s, and this
  // re-centers on it whenever the mode is scroll and the position changes.
  //
  // This is declared here, ahead of the `if (!session) return null` below,
  // because hooks must run in the same order on every render — `currentIndex`
  // itself is only computed after that guard, so the dependency array below
  // reads the state it's derived from instead.
  //
  // Deliberately *not* the reverse: this does not write `state.slot` from
  // where the reader scrolls to by hand. That needs an `IntersectionObserver`
  // watching every mounted group, which is real work left to the next
  // milestone — see the report for what that leaves open.
  const currentGroupRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (mode !== 'scroll') return
    currentGroupRef.current?.scrollIntoView({ block: 'start' })
  }, [mode, session?.state.authority, session?.state.slot, session?.state.viewing])

  if (!session) return null
  const { work, resolver, reading, state, alignments } = session
  const shown = display(state, resolver)

  // Takes one of `Gap`'s offered moves: `slot` is authority-space (what
  // `nearestPaired` returned), and `takeOffered` is what resolves it into
  // the viewed edition and commits authority there.
  function take(slot: number) {
    setSession((prev) => (prev ? { ...prev, state: takeOffered(prev.state, slot, prev.resolver) } : prev))
  }

  // The filmstrip's click job: real navigation, via `reading.ts`'s
  // `navigate` directly — not `swap`. A tile is a destination the reader
  // chose, not another edition to peek at and return from.
  function goToSlot(slot: number) {
    setSession((prev) => (prev ? { ...prev, state: navigate(prev.state, slot) } : prev))
  }

  if (shown.kind === 'gap') {
    const within = resolver.slotCount(state.authority) ?? state.slot + 1
    const { before, after } = nearestPaired(state, resolver, within)
    // `nearestPaired` returns authority-space slots; `Gap` wants to show the
    // page each one lands on in the edition actually being looked at, so
    // each offer is resolved via `session.ts`'s `resolveOffered` — the same
    // resolution `takeOffered` uses to commit the move, not a second copy of
    // it kept here.
    // `resolveOffered` and `slotAt` both answer in the same zero-based space
    // as everything else internal; `Gap`'s `page` props are display-only
    // (see their doc there), so the +1 happens right here, at the boundary
    // handing a value to something a person reads — the same convention
    // `displaySlot` below follows, and `altAt` above.
    const toMove = (slot: number | undefined) => {
      if (slot === undefined) return undefined
      const resolved = resolveOffered(state, slot, resolver)
      return resolved && { slot, page: resolved.page + 1 }
    }
    return (
      <Gap
        edition={shown.edition}
        from={{
          edition: shown.from.edition,
          page: resolver.slotAt(shown.from.edition, shown.from.slot)!.page + 1,
        }}
        verdict={verdictFor(alignments, shown.from.edition, shown.edition)}
        before={toMove(before)}
        after={toMove(after)}
        onTake={take}
      />
    )
  }

  const slots = allSlots(resolver, shown.edition)
  const grouped = groups(slots, mode, reading)
  const currentIndex = Math.max(
    grouped.findIndex((g) => g.slots.includes(shown.slot)),
    0,
  )
  const current = grouped[currentIndex]

  // The verdict travels with the swap: acceptance is a claim about
  // calibration, only the verdict is a claim about the alignment, so a swap
  // into a non-`sound` pair says so beside the page rather than presenting
  // it as an unremarkable one. Silent while `viewing === authority` (nothing
  // has been swapped into) and while the pair is `sound` (nothing to flag).
  const verdict = state.viewing !== state.authority
    ? verdictFor(alignments, state.authority, state.viewing)
    : undefined

  // `pageUrl` takes a page index, not a slot index — they diverge from the
  // first joined spread onward, so every slot is converted through the
  // `Resolver`-derived `slots` array before it reaches a URL.
  const pageAt = (slot: number) => pageUrl(work, shown.edition, slots[slot].page)
  // 1-based, like `Chrome`'s `displaySlot` — `alt` text is read by a person
  // (directly, or by a screen reader), the same category `Gap`'s page
  // numbers below fall into, so it gets the same conversion at the same kind
  // of boundary: the internal, zero-based `page` is untouched, only what's
  // handed to something a person reads is offset.
  const altAt = (slot: number) => {
    const { page, half } = slots[slot]
    const displayPage = page + 1
    return half ? `page ${displayPage} ${half}` : `page ${displayPage}`
  }

  function renderGroup(group: Group, key: number) {
    // A joined spread is one wide page occupying two slots — draw it once,
    // not as two halves.
    if (group.spread || group.slots.length === 1) {
      const [slot] = group.slots
      return <PageView key={key} src={pageAt(slot)} alt={altAt(slot)} />
    }
    const [a, b] = group.slots
    const [left, right] = reading === 'rtl' ? [b, a] : [a, b]
    return (
      <div key={key} style={{ display: 'flex', width: '100%', height: '100%' }}>
        <div style={{ flex: 1, minWidth: 0, height: '100%' }}>
          <PageView src={pageAt(left)} alt={altAt(left)} />
        </div>
        <div style={{ flex: 1, minWidth: 0, height: '100%' }}>
          <PageView src={pageAt(right)} alt={altAt(right)} />
        </div>
      </div>
    )
  }

  return (
    // `position: relative` is what `Chrome` positions itself against —
    // `position: absolute` removes it from this flex column's layout flow
    // entirely, so it can overlay the toolbar and the page area below
    // without either one changing size when it mounts or unmounts.
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', position: 'relative' }}>
      <div style={{ flex: '0 0 auto', display: 'flex', alignItems: 'center', gap: 8, padding: 8 }}>
        {(['single', 'two', 'scroll'] as const).map((m) => (
          <button key={m} type="button" onClick={() => setMode(m)} aria-pressed={m === mode}>
            {m}
          </button>
        ))}
        {verdict && verdict !== 'sound' && (
          <span role="status" style={{ marginLeft: 'auto', opacity: 0.8 }}>
            {shown.edition}: {verdict} alignment
          </span>
        )}
      </div>
      <div
        style={{
          flex: '1 1 auto',
          minHeight: 0,
          overflow: mode === 'scroll' ? 'auto' : 'hidden',
        }}
      >
        {mode === 'scroll' ? (
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            {grouped.map((g, i) => (
              <div key={i} ref={i === currentIndex ? currentGroupRef : undefined}>
                {renderGroup(g, i)}
              </div>
            ))}
          </div>
        ) : (
          current && renderGroup(current, 0)
        )}
      </div>
      <Chrome
        visible={chromeVisible}
        work={work}
        edition={shown.edition}
        slots={slots}
        groups={grouped}
        currentIndex={currentIndex}
        onNavigate={goToSlot}
        displaySlot={shown.slot + 1}
        total={slots.length}
      />
    </div>
  )
}

export default App
