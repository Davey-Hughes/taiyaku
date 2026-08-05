import { useEffect, useRef, useState } from 'react'
import {
  getNotes,
  getProgress,
  getSidecarWithEtag,
  listWorks,
  pageUrl,
  patchAlignment,
  putNotes,
  putProgress,
} from './api'
import { PageAnnotations } from './Annotations'
import { Chrome } from './Chrome'
import { describePosition, displacedBy, spanOf, type Pending } from './correction'
import { createDecodeQueue, decodeTo, type DecodeQueue } from './decode'
import { debounce, type Debounced } from './debounce'
import { canRedo, canUndo, current as currentHistory, emptyHistory, record, redo, undo, type History } from './history'
import { Gap } from './Gap'
import { intentOf, isInteractiveTarget } from './keys'
import { sliceFor } from './module'
import {
  annotationsModule,
  NEW_INTENT,
  REDO_INTENT,
  noteCountLine,
  REVEAL_INTENT,
  REVIEW_INTENT,
  UNDO_INTENT,
} from './modules/annotations'
import { directionModule, effectiveDirection, setOverride, TOGGLE_INTENT, type Direction } from './modules/direction'
import { motionModule } from './modules/motion'
import { prefetchModule, window as prefetchWindow } from './modules/prefetch'
import {
  chapterReadout,
  healthReadout,
  nextTurns,
  pagesPerMinute,
  positionLine,
  readoutsModule,
  timeLine,
} from './modules/readouts'
import { registry } from './modules/registry'
import {
  regroupModule,
  toggleEditionBreak,
  TOGGLE_INTENT as REGROUP_TOGGLE_INTENT,
} from './modules/regroup'
import { TOGGLE_INTENT as SETTINGS_TOGGLE_INTENT } from './modules/settings'
import { nearestPaired, type NearestOffer } from './nearest'
import {
  editsToWrite,
  gestureFor,
  listPlacement,
  mergeOver,
  moveRegion,
  notesBySlot,
  notesHere,
  paintedBox,
  pointAt,
  pointInSlot,
  regionFromDrag,
  reviewList,
  withoutNote,
  type Note,
  type NoteHere,
  type NotesDoc,
  type Region,
  type ReviewEntry,
} from './notes'
import { NoteList } from './NoteList'
import { NotePanel, type PanelMode } from './NotePanel'
import { PageView } from './PageView'
import { cellsFor, correctionTiles, groups, type Group, type Mode } from './pairing'
import { loadPrefs, resetPrefs, savePrefs } from './prefs'
import { display, navigate, restore, type ReadingState } from './reading'
import { Settings } from './Settings'
import { withFieldValue } from './settingsPanel'
// This file must never become Vitest-reachable: `wasmResolver` imports
// `resolver.ts` as a value, and that module's `taiyaku-wasm` import only
// resolves once `./scripts/build-wasm.sh` has generated the gitignored
// `crates/taiyaku-wasm/pkg/`. A test importing App.tsx would pass locally
// (after that script has run) and break CI or a fresh clone (where it
// hasn't) — see docs/ci.md's `web` job notes and the global no-resolver-in-
// tests constraint.
import { wasmResolver, type Chapter, type Position, type Resolver } from './resolver'
import {
  alignmentsOf,
  applyIntent,
  initialPrevious,
  orderedEditions,
  takeOffered,
  verdictFor,
  type Alignment,
} from './session'
import { nextColour } from './palette'
import { cryptoRandom, ulid } from './ulid'

/**
 * How long a burst of `navigate` calls (arrow-stepping, a filmstrip click, a
 * gap take) waits for quiet before the position actually reaches the server.
 * Arrowing through twenty pages inside this window is one `PUT`, not twenty.
 */
const PROGRESS_DEBOUNCE_MS = 400

/**
 * How long a burst of note edits waits for quiet before reaching the server.
 *
 * Longer than the reading position's 400 ms: a position is a number and a note
 * is prose, so a reader pausing to think mid-sentence should not spend a round
 * trip on it.
 */
const NOTES_DEBOUNCE_MS = 700

/**
 * Whether two notes documents are the same, for history's sake.
 *
 * By serialised content rather than by reference: every write produces fresh
 * objects, so reference equality would record a step for a write that changed
 * nothing and leave undo needing two presses to do anything visible.
 */
const sameNotes = (a: Note[], b: Note[]) => JSON.stringify(a) === JSON.stringify(b)

/**
 * How near a marker counts as grabbing it, in slot fractions.
 *
 * A point has no area, so without this the only way to take hold of one would
 * be to hit an infinitely small spot. Roughly the radius `Annotations` draws a
 * pin at, so what a reader can grab is what they can see.
 */
const GRAB_SLOP = 0.02

/**
 * Enough of the sidecar to pick a work's first edition, its reading
 * direction and the verdict on a pair — `reading` is what `groups` needs to
 * know which side of a pair is drawn first, and `alignments` is what
 * `verdictFor` reads. Everything else (members, spreads, runs) stays the
 * aligner's business: the reader gets slots from the `Resolver`, below,
 * rather than deriving them here a second time.
 */
interface SidecarShape {
  work: { reading: Direction }
  editions: Record<string, unknown>
  hub: string
  alignments: Alignment[]
}

interface Session {
  work: string
  resolver: Resolver
  /** The sidecar's fact about the work. `d` layers a per-edition override on top of this — see `effectiveDirection`. */
  reading: Direction
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
  // `s`'s target (`modules/settings.ts`) — same reasoning as `chromeVisible`
  // above, and deliberately a separate flag rather than reusing it: the
  // panel and the chrome are shown and hidden independently.
  const [settingsVisible, setSettingsVisible] = useState(false)

  // `c`'s target. Set to a `Pending` assertion the moment correction mode
  // opens (`pending.b` still absent — meaning no counterpart has been picked
  // yet, not that one pairs with nothing; see `correction.ts`'s `Pending`),
  // grown a `b` once a tile is picked, and cleared only by `Escape` or a
  // write actually landing — nothing else undoes a correction, so nothing
  // else may drop this silently either.
  const [pending, setPending] = useState<Pending>()
  // Whether a correction is on the wire right now.
  //
  // A correction is irreversible, and the confirm banner is the one moment a
  // reader is asked to trust that. Without this, three things went wrong while
  // a `PATCH` was in flight: a second `Confirm` click started a second,
  // concurrent write against the same `etag`; `Escape` and `Cancel` cleared
  // `pending` and took the banner away, telling the reader nothing had
  // happened while the write was still going to land; and the completion
  // handler then navigated the reader — or, if they had pressed `c` again in
  // the meantime, silently replaced the correction they had just started.
  //
  // Disabling all three exits for the duration closes every one of those by
  // construction: there is no way to reach a second `pending` while the first
  // is still settling, so the completion handler cannot be stale.
  const [submitting, setSubmitting] = useState(false)
  // The keydown effect subscribes once at load, so its closure cannot see
  // this — same staleness `sessionRef` and `pendingRef` exist to route around.
  const submittingRef = useRef(false)
  useEffect(() => {
    submittingRef.current = submitting
  }, [submitting])
  // The sidecar's `ETag`, as of the last successful read or write — what a
  // correction's `If-Match` is checked against. Kept alongside `session`
  // rather than inside it: `session.resolver` is rebuilt from a correction's
  // response body, but the two are set together, not derived from one
  // another, so there is nothing gained by nesting this under `Session`.
  const [etag, setEtag] = useState<string>()
  // Set only on the paths `patchAlignment` can actually fail on after a
  // reader has already committed to a pick — a stale `ETag` (someone else's
  // correction landed first) or any other rejected write. Shown beside the
  // confirm/cancel controls rather than thrown away in a `console.error`:
  // `api.ts`'s own doc on `patchAlignment` is explicit that a correction is a
  // deliberate act the reader is watching the result of, not a background
  // write like `putProgress`.
  const [correctionNotice, setCorrectionNotice] = useState<string>()

  // The work's notes, and the validator a write has to be conditional on —
  // held together for the same reason `etag` sits beside `session` rather than
  // inside it: the two are set together on every read and every write, but
  // neither is derived from the other.
  //
  // `undefined` only before the load effect has answered. A work nobody has
  // annotated still comes back as a document (`api.ts`'s `getNotes`), so there
  // is no empty-versus-absent case to carry through the reader.
  const [notesDoc, setNotesDoc] = useState<NotesDoc>()
  const [notesEtag, setNotesEtag] = useState<string>()
  // Whether a note write is on the wire — `submitting`'s role, for the same
  // reasons: a second Save would race the first against one validator, and an
  // exit taken mid-write would tell the reader nothing had happened while it
  // was still going to land.
  const [notesBusy, setNotesBusy] = useState(false)
  // Every state of the notes this session has written, and where the reader
  // stands in it. Seeded on load, so the document as it arrived is the floor
  // undo cannot go below.
  const [notesHistory, setNotesHistory] = useState<History<Note[]>>(() => emptyHistory<Note[]>([]))
  const [notesNotice, setNotesNotice] = useState<string>()
  // Which surface the note panel is showing, or none. `n` opens the editor on
  // a fresh draft, a margin tab opens it on an existing note, `a` toggles the
  // review list, `Escape` closes either.
  const [panelMode, setPanelMode] = useState<PanelMode>()
  // Every note this editing session has changed, keyed by id, and which of
  // them the panel is showing.
  //
  // A map rather than one draft, because the editor is an **edit mode**: while
  // it is open every region on the page can be grabbed and moved, not only the
  // one belonging to the note being typed into. Grabbing another note's box
  // makes it the one on show, and the note you were on keeps its changes here
  // rather than being discarded for having lost focus — so `Save` writes
  // everything you touched, and nothing you did quietly goes missing because
  // you reached for a different box.
  const [edits, setEdits] = useState<Record<string, Note>>({})
  const [currentId, setCurrentId] = useState<string>()
  // The note the panel is showing. Derived rather than stored, so there is one
  // copy of every edit and no way for the shown note and the saved one to
  // disagree.
  const draft = currentId ? edits[currentId] : undefined
  /** Change the note on show, in place. */
  const editCurrent = (change: (note: Note) => Note) =>
    setEdits((map) => {
      if (!currentId || !map[currentId]) return map
      const next = { ...map, [currentId]: change(map[currentId]) }
      // Scheduled here rather than in each caller, so no way of changing a
      // note can forget to save it — which, with no Save button, would mean a
      // change that quietly never happened.
      scheduleSave(next)
      return next
    })

  // The keydown effect subscribes once at load, so it cannot see these — the
  // same staleness `sessionRef` and `pendingRef` exist to route around.
  const panelModeRef = useRef<PanelMode>(undefined)
  useEffect(() => {
    panelModeRef.current = panelMode
  }, [panelMode])
  const notesBusyRef = useRef(false)
  useEffect(() => {
    notesBusyRef.current = notesBusy
  }, [notesBusy])

  // Every registered module's own preference slice, in one map keyed by
  // `ReaderModule.id` rather than one hand-declared `useState` per module.
  // Built once, from `registry.all()`, the same source `Settings.tsx` reads
  // to generate the panel itself — a module that is registered is
  // preferences-wired for free, the claim `Settings.tsx`'s own doc makes
  // about the panel now also true of the state underneath it. Loaded once,
  // like `directionPrefs` and `regroupPrefs` used to be individually: there
  // is exactly one `work` for this component's lifetime (no work-switcher
  // exists yet), so there is nothing to reload against.
  const [prefsMap, setPrefsMap] = useState<Record<string, object>>(() =>
    Object.fromEntries(registry.all().map((mod) => [mod.id, loadPrefs(mod.id, mod.defaults as object)])),
  )
  // Persisted via an effect rather than inline in `setPrefsMap`'s callers —
  // several of them (the `d`/`b` handlers below) update this with a
  // functional updater, and an updater must stay pure (React may invoke it
  // more than once, e.g. under StrictMode), so the `savePrefs` side effect
  // lives here instead, same reasoning `directionPrefs`'s effect used to
  // carry alone. Writes every module's slice on any one module's change
  // rather than diffing for the one that actually moved: `savePrefs` is a
  // cheap `JSON.stringify` + `setItem` on a handful of small objects, so the
  // simplicity of "persist the whole map, always" costs nothing measurable
  // and cannot drift from which id actually changed the way a hand-maintained
  // diff could.
  useEffect(() => {
    for (const mod of registry.all()) {
      savePrefs(mod.id, prefsMap[mod.id] ?? (mod.defaults as object))
    }
  }, [prefsMap])

  // Typed reads of each module's own slice — `sliceFor` (`module.ts`)
  // restores the `P` a plain `Record<string, object>` can't carry, so every
  // site below keeps the same typed shape `directionPrefs`/`motionPrefs`/etc.
  // had as their own `useState` values. Derived on every render rather than
  // memoised: each is a cheap property lookup with a fallback, not a
  // computation worth caching.
  const directionPrefs = sliceFor(prefsMap, directionModule)
  const motionPrefs = sliceFor(prefsMap, motionModule)
  const readoutsPrefs = sliceFor(prefsMap, readoutsModule)
  const prefetchPrefs = sliceFor(prefsMap, prefetchModule)
  const annotationsPrefs = sliceFor(prefsMap, annotationsModule)
  const regroupPrefs = sliceFor(prefsMap, regroupModule)

  // Two decode queues, not one — a review found that sharing a single queue
  // between the live page turn and the prefetch window let stale prefetch
  // work starve the decode the reader is actually waiting on: every slot
  // change enqueues several prefetch jobs plus one swap job, and a FIFO
  // queue with no cancellation runs them in the order they arrived, so a
  // fast flick's *last* keypress — the slot the reader actually lands on —
  // could queue behind a handful of prefetch jobs for slots already left
  // behind. Each job holds a concurrency slot for a full round trip, so at
  // any real network latency the reader could be staring at the page they
  // started from well after they stopped pressing keys.
  //
  // The fix taken is separate queues rather than priority or cancellation:
  // `swapQueue` is what every mounted `PageView` decodes through (passed as
  // `decodeQueue` below), `prefetchQueue` is what the prefetch effect
  // further down decodes through, and neither can ever hold a slot the
  // other needs. Cancelling a stale prefetch job's underlying fetch was the
  // alternative the review named, but `decode.ts` has no such hook —
  // `<img>.decode()` cannot be aborted mid-flight without discarding the
  // element and losing whatever bytes it already has, which trades one
  // defect for a different one (re-fetching from zero on every supersede).
  // Two independent, already-existing queues need no new capability at all.
  //
  // Both lazily created once via `useState`'s initializer, then *rebuilt*
  // whenever `prefetchPrefs.maxConcurrent` changes — the settings panel can
  // now tune it live, and a queue whose cap was baked in at mount would make
  // that one field the exception to "change a setting, confirm it takes
  // effect" every other prefetch setting already satisfies (`ahead`/
  // `behind`/`enabled` are read fresh at render or effect time below, with
  // nothing cached). Rebuilding drops nothing already in flight: a job
  // queued against the old instance keeps running under its old cap, and
  // this only changes which instance the *next* `run` call goes to.
  const [swapQueue, setSwapQueue] = useState<DecodeQueue>(() => createDecodeQueue(prefetchPrefs.maxConcurrent))
  useEffect(() => {
    setSwapQueue(createDecodeQueue(prefetchPrefs.maxConcurrent))
  }, [prefetchPrefs.maxConcurrent])

  const [prefetchQueue, setPrefetchQueue] = useState<DecodeQueue>(() =>
    createDecodeQueue(prefetchPrefs.maxConcurrent),
  )
  useEffect(() => {
    setPrefetchQueue(createDecodeQueue(prefetchPrefs.maxConcurrent))
  }, [prefetchPrefs.maxConcurrent])

  // The keydown effect below subscribes once, after load (see its own comment),
  // so its closure's `session` is frozen at that instant — reading a module
  // key's action (currently just direction's `d`) needs the edition
  // currently being viewed, which changes on every swap. This ref is kept
  // current across every render instead, the same role `writeProgressRef`/
  // `restored` play for other state the effect can't close over live.
  const sessionRef = useRef<Session>(undefined)
  useEffect(() => {
    sessionRef.current = session
  }, [session])

  // `Escape` has to read the *current* `pending` inside the same keydown
  // effect that only resubscribes on load (see its own comment below) — the
  // same staleness `sessionRef` exists to route around, for the same reason.
  const pendingRef = useRef<Pending>(undefined)
  useEffect(() => {
    pendingRef.current = pending
  }, [pending])

  // The grouping the page area is drawing right now, for the keydown effect —
  // which subscribes once at load and so cannot see `grouped`, computed below
  // at render from `mode`, the direction override and the reader's own parity
  // breaks. An arrow steps by group rather than by slot (`pairing.ts`'s
  // `stepToGroup`), and only this component knows what the groups are.
  const groupedRef = useRef<Group[]>(undefined)

  // The page area, and the URL of the page currently drawn in it — both read
  // by the layout effect below, which cannot see anything computed after the
  // `if (!session) return null` guard.
  const pageAreaRef = useRef<HTMLDivElement>(null)
  const pageUrlRef = useRef<string>(undefined)

  // The window's width, for `auto` note-list placement. A listener rather than
  // a one-off read, because "is there room beside the page" stops being true
  // the moment somebody resizes the window, and a list that only answered at
  // load would crush the page until the next reload.
  const [viewportWidth, setViewportWidth] = useState(() =>
    typeof window === 'undefined' ? 1280 : window.innerWidth,
  )
  useEffect(() => {
    const onResize = () => setViewportWidth(window.innerWidth)
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  // Where a region drag started. Declared here with the other refs, ahead of
  // the `if (!session) return null` below, because hooks must run in the same
  // order on every render — the handlers that use it are defined further down,
  // where the page geometry they read is in scope.
  // The notes on the page right now, for the keydown handler — `n` picks the
  // next unused colour, which needs to know what is already here. Assigned
  // during render like `groupedRef`, for the same reason.
  const hereRef = useRef<NoteHere[]>([])
  // Stepping through history from a key, which the keydown effect cannot call
  // directly — it subscribes once at load and these close over state that
  // changes on every write.
  const stepBackRef = useRef<() => void>(() => undefined)
  const stepForwardRef = useRef<() => void>(() => undefined)
  const closeEditorRef = useRef<() => void>(() => undefined)
  const saveNotesRef = useRef<Debounced<[Note[]]> | null>(null)

  const dragRef = useRef<{
    x: number
    y: number
    el: HTMLImageElement
    /**
     * `draw` sweeps a new rectangle out from where the pointer went down.
     * `move` slides the region that was already under it, keeping its size —
     * `from` is what it looked like when the drag started, so the offset is
     * always measured against that rather than accumulated frame by frame,
     * which would drift.
     */
    intent: 'draw' | 'move'
    from?: Region
    /** Which note is being moved — not always the one the panel was showing when the drag began. */
    noteId?: string
  } | null>(null)
  // The rectangle under the pointer right now. State rather than a ref,
  // because the whole point is that it re-renders: a drag with no live
  // feedback is invisible, which is how the region gesture first shipped.
  const [dragging, setDragging] = useState<Region>()

  useEffect(() => {
    let cancelled = false
    async function load() {
      const [work] = await listWorks()
      if (!work) throw new Error('no works available')
      const fetched = await getSidecarWithEtag(work)
      const sidecar: SidecarShape = JSON.parse(fetched.json)
      const editions = orderedEditions(sidecar.hub, Object.keys(sidecar.editions))
      const [hub] = editions
      if (!hub) throw new Error(`${work} defines no editions`)
      const resolver = await wasmResolver(fetched.json)
      // Stored progress if there is any, slot 0 of the hub if there is none —
      // see `reading.ts`'s `restore`. Read after the resolver so a failure
      // resolving the sidecar surfaces before a second request is made, and
      // because `restore` now needs the resolver itself, to validate a
      // stored slot still exists rather than trusting stale JSON outright.
      const progress = await getProgress(work)
      // Notes are read alongside progress rather than before it: neither
      // depends on the other, and a work with no notes still answers a
      // document, so there is no branch here. A failure reading them must not
      // stop the reader from reading — a reader who cannot see their notes
      // should still get their pages — so this catches on its own rather than
      // letting the whole load effect reject.
      const notes = await getNotes(work).catch((err: unknown) => {
        console.error(err)
        return undefined
      })
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
        setEtag(fetched.etag)
        // An empty document rather than `undefined` where the read failed, so
        // the panel and the readout have something to count and a later
        // successful write is still impossible — `saveNote` refuses without an
        // `ETag`, which a failed read never produced.
        setNotesDoc(notes?.doc ?? { work, notes: [] })
        setNotesEtag(notes?.etag)
        setNotesHistory(emptyHistory(notes?.doc.notes ?? []))
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
      // `Escape` is not one of `intentOf`'s intents — it has nothing to do
      // with reading, only with backing out of a correction — so it is
      // checked directly, ahead of the dispatch below, and only while there
      // is actually a `pending` to abandon. Unlike every other branch here,
      // this deliberately does *not* stand down for a focused control: the
      // tile a reader just clicked (`onPickCounterpart`) is itself a button,
      // and `Escape` cancelling out of that click is exactly the case this
      // exists for — gating it on `isInteractiveTarget` would make it fire
      // everywhere except the one moment it matters most.
      if (event.key === 'Escape') {
        // The note panel closes on `Escape` too, but only where there is no
        // correction to abandon: a reader mid-correction pressing `Escape`
        // means the correction, which is the irreversible one.
        if (!pendingRef.current && panelModeRef.current) {
          // Not while a note write is on the wire, for the reason the
          // correction branch below stands down: taking the panel away would
          // tell the reader the note was abandoned while it is still going to
          // land.
          if (notesBusyRef.current) return
          event.preventDefault()
          // Flush, not drop. `Escape` used to mean "abandon this" back when a
          // Save button decided what was kept; with writes landing on their
          // own it means "I am finished here", and anything still in the
          // debounce is a change the reader made. Undo is what abandons now.
          closeEditorRef.current()
          return
        }
        if (!pendingRef.current) return
        // Not while the write is on the wire: taking the banner away here
        // would tell the reader the correction was abandoned when it is
        // still going to land. See `submitting`'s own doc above.
        if (submittingRef.current) return
        event.preventDefault()
        setPending(undefined)
        setCorrectionNotice(undefined)
        return
      }
      // One dispatch path: every key — built-in or module-declared — passes
      // through `intentOf`, which applies the modifier guard once. A module's
      // own key (currently just direction's `d`) comes back as `{ kind:
      // 'module', intent }` rather than a second, hand-matched branch here.
      const intent = intentOf(event, editions, registry.all())
      if (!intent) return
      // **Nothing reaches the page while the note editor is open.** It is a
      // dialog that takes text, so a keystroke means the note — whatever the
      // browser currently thinks is focused. `isInteractiveTarget` below only
      // stands down while focus is *in* a control, and a refused save or a
      // stray click is enough to break that: measured, with a server rejecting
      // a write, the editor stayed open holding the reader's text while focus
      // sat on `<body>`, so typing opened the settings panel instead of
      // writing the note. `Escape` is handled above this and still closes the
      // editor; everything else belongs to the note until it does.
      if (panelModeRef.current === 'edit') return
      if (intent.kind === 'chrome') {
        // `Tab`'s own default — focus movement — is left alone. Toggling the
        // chrome is a side effect layered on top of the browser's own `Tab`
        // handling, not a replacement for it; preventing it here is what
        // made every focusable control (mode buttons, all of the
        // filmstrip's tiles, `Gap`'s offered moves) mouse-only.
        setChromeVisible((visible) => !visible)
        return
      }
      if (intent.kind === 'correct') {
        // Same interactive-target guard as the module branch below — a
        // focused button's own keyboard activation should be the only thing
        // that runs. Reads `sessionRef.current`, not the `session` this
        // effect closed over at subscribe time, for the same reason the
        // module cases below do: that closure is frozen at load and never
        // sees a later swap or navigation.
        if (isInteractiveTarget(event.target)) return
        // Not while a correction is already on the wire — see `submitting`'s
        // own doc above. Without this, `disabled={submitting}` blurs the
        // just-clicked `Confirm` button the instant the write starts, so
        // `isInteractiveTarget` no longer stands this handler down, and a
        // `c` press mid-write would silently replace the pending correction
        // the completion handler in `confirmCorrection` is about to resolve.
        if (submittingRef.current) return
        event.preventDefault()
        const current = sessionRef.current
        if (!current) return
        const shown = display(current.state, current.resolver)
        // No slot to correct while showing a gap — there is nothing on
        // screen to name as `a`.
        if (shown.kind !== 'at') return
        // The counterpart to correct against: `state.authority` when the
        // reader has swapped away from it (that swap is what they are
        // comparing against), otherwise `previous` — the edition a `Space`
        // would return to, and so the most recent "other" edition in view.
        const to = current.state.authority !== shown.edition ? current.state.authority : current.previous
        if (to === shown.edition) return // nothing to correct against
        const slots = allSlots(current.resolver, shown.edition)
        setPending({ from: shown.edition, to, a: slots[shown.slot] })
        setCorrectionNotice(undefined)
        setChromeVisible(true) // the filmstrip is where a counterpart is picked
        return
      }
      if (intent.kind === 'module') {
        // Same interactive-target guard as the reader-intent branch below —
        // a focused button's own keyboard activation should be the only
        // thing that runs, whether the key belongs to the reader or a module.
        if (isInteractiveTarget(event.target)) return
        event.preventDefault()
        // A small, explicit switch rather than a generic action table: three
        // modules declare a key today, and each does something different
        // enough with it (a per-edition flip, a per-edition break, a plain
        // visibility toggle) that a table would need its own per-entry
        // shape anyway. The next module to claim a key adds the next case.
        switch (intent.intent) {
          case TOGGLE_INTENT: {
            const current = sessionRef.current
            if (!current) return
            const edition = current.state.viewing
            const sidecarReading = current.reading
            setPrefsMap((map) => {
              const prefs = sliceFor(map, directionModule)
              const flipped: Direction =
                effectiveDirection(sidecarReading, prefs.override[edition]) === 'rtl' ? 'ltr' : 'rtl'
              return { ...map, [directionModule.id]: setOverride(prefs, edition, flipped) }
            })
            break
          }
          case REGROUP_TOGGLE_INTENT: {
            const current = sessionRef.current
            if (!current) return
            // Breaks are slots within the *viewed* edition's own space —
            // the same space `groups` indexes into below — so this is
            // `shown.slot`, not `state.slot`, which is authority-space and
            // only equal to it when nothing has been swapped. A gap has no
            // slot to break on, so the key is inert there.
            const shown = display(current.state, current.resolver)
            if (shown.kind !== 'at') return
            setPrefsMap((map) => ({
              ...map,
              [regroupModule.id]: toggleEditionBreak(sliceFor(map, regroupModule), shown.edition, shown.slot),
            }))
            break
          }
          case NEW_INTENT: {
            const current = sessionRef.current
            if (!current) return
            const shown = display(current.state, current.resolver)
            // A gap has no slot to annotate — there is nothing on screen to
            // anchor to, the same reason `c` is inert there.
            if (shown.kind !== 'at') return
            // Always a new note, however many the slot already holds. One
            // note per panel is the ordinary case, and this key used to
            // *reopen* the note already there instead — which made a second
            // note on a page impossible, since nothing else creates one.
            //
            // The reasoning for reopening was to stop `n` pressed twice from
            // leaving duplicates, and it protected against nothing: a draft is
            // not a note until Save, so a second press only ever replaced an
            // unsaved draft. What it cost was real. Reopening an existing note
            // is what its margin tab is for.
            //
            // Nothing needed here to stop `n` replacing a draft mid-write:
            // no key reaches this switch at all while the editor is open (see
            // the guard at the dispatch above), so a reader has to close it
            // first — which is also what stops a second press throwing away
            // typing nobody asked to discard.
            const now = new Date().toISOString()
            const fresh: Note = {
              id: ulid(Date.now(), cryptoRandom),
              edition: shown.edition,
              slot: shown.slot,
              // The next colour nobody on this page is using, so two notes on
              // one page come out different without a reader choosing — see
              // `palette.ts`'s `nextColour`. Picked from what is on *this*
              // page rather than the whole work, since telling one note from
              // another only matters where they are drawn together.
              color: nextColour(hereRef.current.map((h) => h.note.color)),
              body: '',
              created: now,
              updated: now,
            }
            setEdits((map) => ({ ...map, [fresh.id]: fresh }))
            setCurrentId(fresh.id)
            setPanelMode('edit')
            setNotesNotice(undefined)
            break
          }
          case REVEAL_INTENT: {
            setPrefsMap((map) => {
              const prefs = sliceFor(map, annotationsModule)
              return {
                ...map,
                [annotationsModule.id]: { ...prefs, reveal: !prefs.reveal },
              }
            })
            break
          }
          case UNDO_INTENT: {
            stepBackRef.current()
            break
          }
          case REDO_INTENT: {
            stepForwardRef.current()
            break
          }
          case REVIEW_INTENT: {
            // Toggles, like `s` and `Tab` do — and closes the editor if that
            // is what is open, so `a` always means "show me the list" rather
            // than sometimes meaning "swap panels" and sometimes "close".
            if (notesBusyRef.current) return
            setPanelMode((open) => (open === 'review' ? undefined : 'review'))
            setNotesNotice(undefined)
            break
          }
          case SETTINGS_TOGGLE_INTENT: {
            // Unlike the two cases above, this needs nothing from the
            // current session — it only shows or hides the panel, the same
            // shape `Tab`'s `{ kind: 'chrome' }` branch above has, just
            // reached through the module contract instead of a second
            // hard-coded `Intent` kind (see `modules/settings.ts`).
            setSettingsVisible((visible) => !visible)
            break
          }
        }
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
        const { state, previous } = applyIntent(
          intent,
          prev.state,
          prev.previous,
          resolver,
          groupedRef.current,
        )
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

  /**
   * Notes reach the server on their own, a moment after the last change.
   *
   * There is no Save button any more. A reader writing a note has already
   * decided to write it, and the button's only real job was to be the thing
   * you *didn't* press when you changed your mind — which undo now does
   * properly, without asking anyone to hesitate.
   *
   * Debounced for the same reason the reading position is: typing a sentence
   * is one write, not forty. Longer than the position's 400 ms, because a
   * position is a number and a note is prose — a reader pauses mid-sentence
   * and should not spend a round trip on it.
   */
  const writeNotesRef = useRef<Debounced<[Note[]]> | null>(null)
  if (writeNotesRef.current === null) {
    writeNotesRef.current = debounce((notes: Note[]) => {
      void writeNotesLatestRef.current?.(notes)
    }, NOTES_DEBOUNCE_MS)
  }
  const saveNotes = writeNotesRef.current
  saveNotesRef.current = saveNotes
  // `debounce` holds the callback it was built with for the life of the
  // component, and `writeNotes` closes over state that changes every render —
  // so the debounce calls through this ref to reach the current one rather
  // than the one that existed at first render.
  const writeNotesLatestRef = useRef<((notes: Note[]) => Promise<void>) | null>(null)

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
      if (document.visibilityState === 'hidden') {
        writeProgress.flush()
        // A half-typed note is worth more than a page number, and it is the
        // one thing here that cannot be recomputed from anything.
        saveNotes.flush()
      }
    }
    const onHide = () => {
      writeProgress.flush()
      saveNotes.flush()
    }
    document.addEventListener('visibilitychange', onVisibilityChange)
    window.addEventListener('pagehide', onHide)
    return () => {
      document.removeEventListener('visibilitychange', onVisibilityChange)
      window.removeEventListener('pagehide', onHide)
    }
  }, [writeProgress, saveNotes])

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

  // The reading-time readout's raw material — `modules/readouts.ts`'s
  // `nextTurns`/`timeLine`/`pagesPerMinute` do the actual arithmetic; this
  // effect only decides *when* to call it. A separate effect from the
  // progress-write one above, deliberately: that one's dependency array is
  // load-bearing (must never gain `session`, must never fire on a swap)
  // precisely because it has a side effect worth guarding, a network write;
  // this one only appends to an in-memory ref, client-side only per the
  // brief, so there is no redundant-write hazard to guard against.
  //
  // A `restored`-style skip on the *first* run is still needed here, though
  // — measured live, not assumed: recording session-load itself as the first
  // "turn" makes the interval between mount and the sidecar/wasm finishing
  // to load look like a page turn, so the very first estimate this project
  // renders was `~6m left` derived entirely from load latency, before a
  // single arrow key had been pressed. `restoredTurn` tracks *whether* to
  // skip; `nextTurns` — pure, and covered directly by `readouts.test.ts`,
  // unlike this effect — is what actually skips.
  //
  // `session` deliberately does not join this array, the same as the
  // progress-write effect's own array above and for a directly analogous
  // reason: `session` changes identity on every swap too, and a swap is not
  // a page turn — it does not even move `state.slot` (see `reading.ts`'s
  // `swap`). Depending on the whole object would record one every time a
  // reader merely peeked at another edition, inflating the pace with moves
  // that were not reading at all.
  const turnsRef = useRef<number[]>([])
  const restoredTurn = useRef(false)
  // oxlint-disable react-hooks/exhaustive-deps -- session changes on every swap too, and a swap is not a page turn; see above.
  useEffect(() => {
    if (!session) return
    const skip = !restoredTurn.current
    restoredTurn.current = true
    turnsRef.current = nextTurns(turnsRef.current, Date.now(), skip)
  }, [session?.state.authority, session?.state.slot])
  // oxlint-enable react-hooks/exhaustive-deps

  // `elapsedMs` (below, at render) is a plain subtraction against this, fixed
  // once per page load — "session" means "since this reader opened the
  // reader," not since the sidecar finished loading a moment later, so it is
  // seeded eagerly rather than waiting on `session` to exist.
  const sessionStartRef = useRef(Date.now())

  // Nothing above re-renders on its own as time passes — `elapsedMs` and the
  // remaining-time estimate are both computed fresh at render, from `Date.now()`
  // against the refs above, but a ref changing never triggers a render. This
  // is the heartbeat that makes the estimate actually tick rather than only
  // updating on the next page turn. 30s rather than every second: the display
  // itself is minute-granularity (`modules/readouts.ts`'s `timeLine` never
  // shows seconds), so anything finer would re-render for a change nobody
  // could see.
  const [, tick] = useState(0)
  useEffect(() => {
    const id = setInterval(() => tick((t) => t + 1), 30_000)
    return () => clearInterval(id)
  }, [])

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
  // watching every mounted group, which is real work left to a later
  // milestone — recorded in docs/status.md's "What M5a leaves open".
  const currentGroupRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (mode !== 'scroll') return
    currentGroupRef.current?.scrollIntoView({ block: 'start' })
  }, [mode, session?.state.authority, session?.state.slot, session?.state.viewing])

  // Warms the browser's own cache for the slots `modules/prefetch.ts`'s
  // `window` names either side of wherever the reader is looking, so an
  // arrow key that lands there finds `PageView`'s own decode (`decodeTo`,
  // via `swapQueue` below) already resolved against a cached response
  // rather than starting a fetch from nothing. Runs through `prefetchQueue`,
  // not `swapQueue` — see that ref's own doc above for why the two must
  // stay separate. Skipped in scroll mode: every
  // group is already mounted there (see `PageView.tsx`'s `buffered` doc), so
  // "the slots around the current one" is not a meaningful subset to warm
  // ahead of the rest — the rest are either already requested or deliberately
  // deferred to `loading="lazy"`, and this would only fight that. Skipped
  // while disabled (`prefetchPrefs.enabled`) or mid-gap (nothing paired to
  // prefetch around).
  //
  // Declared here, ahead of the `if (!session) return null` below, for the
  // same reason `currentGroupRef`'s effect just above is: hooks must run in
  // the same order on every render, so this reads `sessionRef.current`
  // (kept fresh by the effect near the top of this component) rather than
  // the `session`/`shown`/`resolver` bindings this function only creates
  // after that guard.
  useEffect(() => {
    const current = sessionRef.current
    if (!current || mode === 'scroll' || !prefetchPrefs.enabled) return
    const shownNow = display(current.state, current.resolver)
    if (shownNow.kind !== 'at') return
    const total = current.resolver.slotCount(shownNow.edition) ?? 0
    const targets = prefetchWindow(shownNow.slot, total, prefetchPrefs.ahead, prefetchPrefs.behind)
    for (const slot of targets) {
      const position = current.resolver.slotAt(shownNow.edition, slot)
      if (!position) continue
      const url = pageUrl(current.work, shownNow.edition, position.page)
      void prefetchQueue.run(() => decodeTo(new Image(), url))
    }
  }, [
    mode,
    session?.state.authority,
    session?.state.slot,
    session?.state.viewing,
    prefetchPrefs.enabled,
    prefetchPrefs.ahead,
    prefetchPrefs.behind,
    prefetchQueue,
  ])

  if (!session) return null
  const { work, resolver, reading, state, alignments, editions } = session
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

  // The filmstrip's click job while `pending` is set: `slot` is a slot of
  // `pending.to` (the edition the strip is showing while correcting — see
  // `correcting`'s doc on `FilmstripProps`), converted to a `Position` here
  // because that is what `Pending.b`, `spanOf` and `displacedBy` all deal in.
  // Not a write itself — see `confirmCorrection` below for that, and the
  // task's own reasoning for why the two are not one step.
  function pickCounterpart(slot: number) {
    setPending((prev) => {
      if (!prev) return prev
      const b = resolver.slotAt(prev.to, slot)
      return b ? { ...prev, b } : prev
    })
    setCorrectionNotice(undefined)
  }

  // Writes `pending` — only reachable once `pending.b` is set, i.e. after
  // `pickCounterpart` above — and, on success, moves the reader onto the
  // page they just picked: the design's "switches to the counterpart
  // edition" is not only the filmstrip's tiles while choosing, it is also
  // where the reader lands once the choice is confirmed.
  //
  // `previous` is updated to the edition being left, same as every other
  // change to `viewing` in this file (`session.ts`'s own doc on `previous`
  // is explicit that this must happen on *every* change, not only a
  // `toggle`) — skipping it here would be exactly the drift that doc warns
  // against, just reached through a correction instead of a keypress.
  async function confirmCorrection() {
    // `submitting` guards re-entry as well as the exits: a second `Confirm`
    // click would otherwise start a concurrent write against the same `etag`,
    // and the loser of that race gets a `412` for a correction the reader
    // only asked for once.
    if (!pending || pending.b === undefined || etag === undefined || submitting) return
    const { from, to, a, b } = pending
    setSubmitting(true)
    try {
      const fresh = await patchAlignment(work, etag, { from, to, a: spanOf(a), b: spanOf(b) })
      const nextResolver = await wasmResolver(fresh.json)
      const bSlot = nextResolver.indexOf(to, b)
      // The outgoing resolver's `WasmSidecar` would otherwise be stranded —
      // see `resolver.ts`'s `Resolver.dispose` doc. Freed only now that
      // `nextResolver` exists to replace it, not before: a `wasmResolver`
      // parse failure above must leave the reader with a working resolver,
      // not none.
      resolver.dispose?.()
      setSession((prev) => {
        if (!prev) return prev
        const state = bSlot === undefined ? prev.state : navigate({ ...prev.state, viewing: to }, bSlot)
        // `alignments` alongside the resolver swap — `Sidecar::rewrite`
        // recomputes `quality` on every correction, so leaving this at
        // whatever loaded at session start would show a verdict the
        // document no longer states. See `session.ts`'s `alignmentsOf`.
        return { ...prev, resolver: nextResolver, alignments: alignmentsOf(fresh.json), state, previous: prev.state.viewing }
      })
      setEtag(fresh.etag)
      setPending(undefined)
      setCorrectionNotice(undefined)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      // The server's own wording for a `412` (`routes.rs`'s `patch_alignments`)
      // — matched by substring, not by re-parsing the status out of the
      // thrown message, since `api.ts` only promises the status is somewhere
      // in there. A stale validator means somebody else's correction is in
      // the document now, and it may be the one this reader was about to
      // make, so `pending` survives this catch untouched: re-fetching just
      // gets the reader a current `ETag` and resolver to confirm again
      // against, not a silently retried write.
      if (message.includes('changed since you read it')) {
        try {
          const fresh = await getSidecarWithEtag(work)
          const nextResolver = await wasmResolver(fresh.json)
          // Same dispose-then-replace and alignments-refresh reasoning as
          // the success path above — a 412 re-read swaps the resolver too,
          // and the document it re-reads can have a fresher `quality` block
          // than the one this session loaded with.
          resolver.dispose?.()
          setSession((prev) =>
            prev ? { ...prev, resolver: nextResolver, alignments: alignmentsOf(fresh.json) } : prev,
          )
          setEtag(fresh.etag)
        } catch (rereadErr) {
          console.error(rereadErr)
        }
        setCorrectionNotice('This work changed since it was read — check the pairing below and confirm again.')
      } else {
        setCorrectionNotice(message)
      }
    } finally {
      // In `finally`, not at the end of each arm: a throw from the success
      // path itself — `wasmResolver` failing to parse the document the server
      // just returned, say — would otherwise leave the banner disabled with
      // no way out but a reload.
      setSubmitting(false)
    }
  }

  // Writes the notes document with `next` applied, and on success closes the
  // editor. `confirmCorrection`'s shape exactly, because it is the same
  // problem: a conditional write a reader is watching, whose 412 means
  // somebody else's write landed first.
  //
  // The draft survives a 412 untouched. Re-reading gets the reader a current
  // document and validator to save against — it does not silently retry the
  // write, which could clobber whatever arrived.
  // The debounce built once at first render calls through here, so it always
  // reaches the current closure rather than the one that existed then.
  writeNotesLatestRef.current = (notes: Note[]) => writeNotes(notes)

  /**
   * Finish editing: send anything still pending, and put the page back.
   *
   * One function because there are now three ways to arrive here — `Escape`,
   * a click on the background around the page, and the review list's own
   * close — and "finished" has to mean the same thing whichever was used.
   */
  function closeEditor() {
    saveNotesRef.current?.flush()
    setPanelMode(undefined)
    setEdits({})
    setCurrentId(undefined)
    setNotesNotice(undefined)
  }

  /** Apply a set of edits over the stored notes and schedule the write. */
  function scheduleSave(map: Record<string, Note>) {
    saveNotes(editsToWrite(allNotes, map))
  }

  /**
   * Write a notes document, and remember it as a step a reader can come back
   * to.
   *
   * **On a stale validator it re-reads and re-applies rather than asking the
   * reader to try again.** With a Save button, a `412` could stop and say
   * "somebody else wrote; press Save again" — the reader was standing there
   * with their finger on it. Writes are automatic now, so there is nobody to
   * ask: the honest thing is to take the other device's document, lay this
   * reader's own notes back over it by id, and write that. Notes belonging to
   * *other* ids survive untouched, which is the part that matters; two devices
   * editing the same note is still last-write-wins, and always was.
   *
   * Retried once and once only. A second failure is not a race, it is
   * something wrong, and a loop would hammer the server with a reader watching
   * nothing happen.
   */
  async function writeNotes(notes: Note[], { retry = true } = {}): Promise<void> {
    if (!notesDoc || notesEtag === undefined) return
    setNotesBusy(true)
    try {
      const fresh = await putNotes(work, notesEtag, { work: notesDoc.work, notes })
      setNotesDoc(fresh.doc)
      setNotesEtag(fresh.etag)
      setNotesHistory((h) => record(h, fresh.doc.notes, sameNotes))
      setNotesNotice(undefined)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      // The server's own wording for a 412 (`routes.rs`'s `put_notes`),
      // matched by substring like the correction path's — `api.ts` only
      // promises the status is somewhere in the thrown message.
      if (message.includes('changed since you read them') && retry) {
        try {
          const fresh = await getNotes(work)
          setNotesDoc(fresh.doc)
          setNotesEtag(fresh.etag)
          const merged = mergeOver(fresh.doc.notes, notes)
          setNotesBusy(false)
          await writeNotes(merged, { retry: false })
          setNotesNotice('These notes changed on another device; yours were kept on top.')
          return
        } catch (rereadErr) {
          console.error(rereadErr)
        }
      }
      setNotesNotice(message)
    } finally {
      // In `finally` for the reason `confirmCorrection` gives: a throw from
      // the success path itself would otherwise leave every control disabled
      // with no way out but a reload.
      setNotesBusy(false)
    }
  }

  // `Settings.tsx`'s two callbacks, generic over every registered module
  // rather than a hand-written case per one — unlike the key-dispatch switch
  // above, which stays a small explicit switch because each key does
  // something behaviourally different (a per-edition flip, a per-edition
  // break, a plain visibility toggle). A settings-panel change is the same
  // shape for every module, always: look the id up in the registry, apply
  // the change to that id's own slice of the map. `registry.get(id)` is the
  // same lookup `Settings.tsx` itself does to find a module's `defaults`,
  // and `settingsPanel.ts`'s `withFieldValue` is what actually applies a
  // change — coercing the raw control value and refusing an unknown key, an
  // unparseable number, or an object-typed field — so this needs no
  // per-module knowledge of any of that. A module registered in
  // `modules/registry.ts` with nothing but a `defaults` shape is fully wired
  // through both callbacks the moment it is registered: no case to add here.
  function handleSettingChange(id: string, key: string, raw: unknown) {
    const mod = registry.get(id)
    if (!mod) return
    setPrefsMap((map) => ({ ...map, [id]: withFieldValue(map[id] ?? (mod.defaults as object), key, raw) }))
  }

  // Resets both halves: the stored slice (`resetPrefs`, so a reload does not
  // resurrect the old value) and the in-memory state (so the panel reflects
  // the defaults immediately, rather than only after the next load).
  // `resetPrefs` alone would satisfy only the second `docs/notes.md`-style
  // check ("reload, confirm it persisted") and silently fail the first
  // ("change a setting, confirm it takes effect") for exactly one direction
  // of change — a reset the reader could see do nothing until they reloaded.
  // Generic for the same reason `handleSettingChange` above is.
  function handleSettingReset(id: string) {
    const mod = registry.get(id)
    if (!mod) return
    resetPrefs(id)
    setPrefsMap((map) => ({ ...map, [id]: { ...(mod.defaults as object) } }))
  }

  /**
   * Step back or forward through what has been written, and write the result.
   *
   * The step is written rather than only shown: with automatic saving, what is
   * on the server *is* the document, so an undo that only changed the screen
   * would be undone again by the next keystroke. Any open edits are dropped —
   * a reader stepping back does not want the thing they just stepped back
   * from laid on top of it — and the debounce is cancelled for the same
   * reason.
   */
  function stepTo(next: History<Note[]>) {
    if (next === notesHistory) return
    saveNotes.cancel()
    setEdits({})
    setCurrentId(undefined)
    setNotesHistory(next)
    void writeNotes(currentHistory(next))
  }
  const stepBack = () => stepTo(undo(notesHistory))
  const stepForward = () => stepTo(redo(notesHistory))
  stepBackRef.current = stepBack
  stepForwardRef.current = stepForward
  closeEditorRef.current = closeEditor

  // Above the gap branch, deliberately. A gap has no slot to draw notes on,
  // but the work still has notes and a reader in a gap is exactly who wants to
  // ask where they are — found by driving this against the real corpus, where
  // pressing `a` on a gap opened nothing at all because this branch returns
  // before the panel is ever rendered.
  const allNotes = notesDoc?.notes ?? []
  const entries: ReviewEntry[] =
    panelMode === 'review' ? reviewList(allNotes, shown.edition, resolver) : []

  // While the editor is open every region is shown, whatever `reveal` says: a
  // reader cannot grab a box they cannot see, and an edit mode that hid the
  // things it lets you edit would be a puzzle. `reveal` goes back to governing
  // it the moment the editor closes.
  const notesPlacement = listPlacement(annotationsPrefs.notesList, viewportWidth)
  const overlayPrefs =
    panelMode === 'edit' ? { ...annotationsPrefs, reveal: true } : annotationsPrefs

  if (shown.kind === 'gap') {
    const within = resolver.slotCount(state.authority) ?? state.slot + 1
    const { before, after } = nearestPaired(state, resolver, within)
    // `nearestPaired` already resolved each offer against the edition being
    // looked at while finding it (see `nearest.ts`'s `NearestOffer`), so this
    // is only the display conversion — not a second `resolveOffered` call
    // repeating a resolution the scan already did. `Gap`'s `page` props are
    // display-only (see their doc there), so the +1 happens right here, at
    // the boundary handing a value to something a person reads — the same
    // convention `altAt` (below) follows.
    const toMove = (offer: NearestOffer | undefined) =>
      offer && { slot: offer.slot, page: offer.page + 1 }
    return (
      <>
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
        {/*
          The review list is reachable from a gap, where the page area is not.
          A reader standing on a slot the viewed edition has no counterpart for
          is the one most likely to ask where their notes went, and every note
          in the list is already marked with whether it can be reached from
          here — so this is the state the marking exists for.

          Only the list: `n` is inert in a gap (there is no slot to anchor to)
          and the editor has nothing to edit, so `panelMode` can only be
          `'review'` by the time a reader gets here. Passing `draft` through
          anyway rather than hard-coding `undefined`, so this cannot drift from
          that if the key ever does something else in a gap.
        */}
        <NotePanel
          mode={panelMode === 'review' ? 'review' : undefined}
          draft={draft}
          entries={entries}
          doc={notesDoc ?? { work, notes: [] }}
          work={work}
          viewing={shown.edition}
          busy={notesBusy}
          notice={notesNotice}
          onBody={() => undefined}
          onColour={() => undefined}
          onDelete={() => undefined}
          onClose={() => {
            setPanelMode(undefined)
            setNotesNotice(undefined)
          }}
          canUndo={canUndo(notesHistory)}
          canRedo={canRedo(notesHistory)}
          onUndo={stepBack}
          onRedo={stepForward}
          onGo={(entry) => {
            if (entry.here === undefined) return
            goToSlot(entry.here)
            setPanelMode(undefined)
          }}
        />
      </>
    )
  }

  const slots = allSlots(resolver, shown.edition)
  // `shown.edition` — not `state.viewing` directly, though they're always
  // equal (see `reading.ts`'s `Display`) — because this is the edition the
  // page area is about to draw, which is what an override keyed by edition
  // must line up with.
  const effectiveReading = effectiveDirection(reading, directionPrefs.override[shown.edition])
  // Same `shown.edition` reasoning as `effectiveReading` above: breaks are
  // stored per edition, keyed by the edition whose slots `groups` is about
  // to pair.
  const grouped = groups(slots, mode, effectiveReading, regroupPrefs.breaks[shown.edition] ?? [])
  // Assigned during render rather than in an effect: the keydown handler must
  // never step against a grouping older than what is on screen, and an effect
  // would leave exactly one frame where it could. Writing a ref during render
  // is safe here because it is derived state, not a subscription — the value
  // is recomputed from props and state every render, so a discarded render
  // simply writes the same thing the retained one will.
  groupedRef.current = grouped
  const currentIndex = Math.max(
    grouped.findIndex((g) => g.slots.includes(shown.slot)),
    0,
  )
  const current = grouped[currentIndex]

  // While correcting, the filmstrip shows `pending.to`'s tiles rather than
  // `shown.edition`'s — see `correcting`'s doc on `FilmstripProps`. The page
  // area above keeps rendering `shown.edition` throughout: a correction
  // never changes what is on screen until it is confirmed (`confirmCorrection`
  // is what moves the reader, once it lands). `slots`/`grouped`/`currentIndex`
  // above are untouched by any of this, on purpose — they stay exactly what
  // the page area itself renders from.
  const correctionSlots = pending && allSlots(resolver, pending.to)
  // `correctionTiles`, not `groups(correctionSlots, mode, ...)` — see its own
  // doc in `pairing.ts`. `groups` would fold a joined spread into one
  // two-slot tile the reader can only ever assert `slots[0]` of (I4 in the
  // M5d review), and in `two` mode would do the same to every ordinary pair.
  // Correcting needs the opposite of reading's pairing: one tile per slot,
  // so any slot — including one half of a spread — can be named on its own,
  // and the dashed marker below (`pairedIndex`) always lands on a tile that
  // names exactly the slot the alignment resolves to.
  const correctionGroups = pending && correctionSlots ? correctionTiles(correctionSlots) : undefined
  // Where the alignment currently resolves `pending.a` to, in `pending.to` —
  // the tile a reader disagrees with when they open correction mode, so it
  // has to be visible to disagree with it (see `Filmstrip`'s own doc on
  // `correcting`). `undefined` when `pending.a` currently pairs with nothing
  // in `to`, a real state `Resolver.resolve` can answer and this must not
  // crash on rather than a bug.
  const landedAtCounterpart = pending && resolver.resolve(pending.from, pending.to, pending.a)
  const pairedIndex = landedAtCounterpart ? resolver.indexOf(pending.to, landedAtCounterpart) : undefined
  // Named before the write, not after it: the 1:1 rule means this slot loses
  // its counterpart, and nothing undoes a correction short of making another.
  const displaced = pending && displacedBy(pending, resolver)

  // `editions[0]` is the hub — `orderedEditions` (`session.ts`) always
  // places it first, and `Session.editions` is built from exactly that.
  const hub = editions[0]

  // The four readout lines, computed here from `modules/readouts.ts`'s pure
  // functions and handed to `Chrome` as plain strings — see that module for
  // every branch below; this is wiring, not derivation. This supersedes the
  // old toolbar badge that used to show `${verdict} alignment` only while
  // `state.viewing !== state.authority`: that badge went silent the moment a
  // reader navigated (not merely swapped) into a spoke edition, since
  // `navigate` moves `authority` to match `viewing` — `healthReadout` below
  // has no such blind spot, because it checks the edition on screen against
  // the hub, not against wherever `authority` happens to have wandered.
  // The notes on the slot being drawn. `notesHere` is `notes.ts`'s, resolving
  // once per edition that holds notes; nothing about travel is decided here.
  //
  // `travel` off filters to the viewed edition's own notes *before* resolving
  // rather than after, so switching the setting off costs no resolution at
  // all — the module is then genuinely doing nothing, which is the property
  // M5b's manual pass checks for every module.
  // Stored notes with this session's edits laid over them, plus the ones this
  // session invented — so a box that has been nudged draws where it was nudged
  // to, not where the server still has it, and a note that has never been
  // saved draws at all.
  const editedNotes: Note[] = [
    ...allNotes.map((n) => edits[n.id] ?? n),
    ...Object.values(edits).filter((e) => !allNotes.some((n) => n.id === e.id)),
  ]
  const notesForHere = annotationsPrefs.travel
    ? editedNotes
    : editedNotes.filter((n) => n.edition === shown.edition)
  const here = notesHere(notesForHere, shown.edition, shown.slot, resolver)
  // The note on show is drawn by `pending` — dashed, and following the pointer
  // mid-drag — so drawing it a second time as an ordinary region would leave
  // two boxes on one note, one of them a frame behind.
  const hereBehind = here.filter((h) => h.note.id !== currentId)
  hereRef.current = here
  // Scroll mode draws every group, so it needs every slot's notes at once —
  // one resolve per note rather than one per page. Built only in that mode:
  // single and two-page draw one group and `here` above already answers it.
  const scrolledNotes =
    mode === 'scroll' ? notesBySlot(notesForHere, shown.edition, resolver) : new Map<number, NoteHere[]>()

  // Opening a note from its margin tab, shared by every mode's overlay.
  /**
   * Every stored note with this session's edits applied — what a write sends.
   *
   * A note this session invented and never filled in is dropped rather than
   * written: pressing `n` and then going off to move another box leaves one
   * behind, and saving it would put an empty marker on the page for a
   * keystroke the reader had already moved on from. A note that *existed* and
   * has been emptied is kept, because emptying one is an edit and deleting one
   * is a button.
   */
  function applyEdits(): Note[] {
    return editsToWrite(allNotes, edits)
  }

  /**
   * Show a note in the editor, bringing it into this session's edits.
   *
   * Whatever is already in `edits` for that id wins over the stored copy, so
   * clicking back onto a note you moved a moment ago shows it where you left
   * it rather than where the server still has it.
   */
  function openNote(id: string) {
    const found = edits[id] ?? allNotes.find((n) => n.id === id)
    if (!found) return
    setEdits((map) => (map[id] ? map : { ...map, [id]: found }))
    setCurrentId(id)
    setPanelMode('edit')
    setNotesNotice(undefined)
  }

  const chapter: Chapter | undefined = resolver.chapterAt(shown.edition, shown.slot)
  const positionText = positionLine({ slot: shown.slot, total: slots.length, page: slots[shown.slot].page + 1 })
  const chapterText = chapterReadout(chapter, alignments, hub)
  const healthText = healthReadout(alignments, hub, shown.edition)
  const slotsLeft = Math.max(slots.length - (shown.slot + 1), 0)
  const timeText = timeLine({
    elapsedMs: Date.now() - sessionStartRef.current,
    pagesPerMin: pagesPerMinute(turnsRef.current, Date.now()),
    slotsLeft,
  })
  // Counted against every note on the work, not against the ones `travel`
  // currently admits: "2 notes here of 7" is a fact about the work, and a
  // total that shrank when a display preference changed would be a different
  // and much stranger claim.
  const notesText = noteCountLine(here.length, allNotes.length)

  // `pageUrl` takes a page index, not a slot index — they diverge from the
  // first joined spread onward, so every slot is converted through the
  // `Resolver`-derived `slots` array before it reaches a URL.
  const pageAt = (slot: number) => pageUrl(work, shown.edition, slots[slot].page)
  // Assigned during render, the same way `groupedRef` is and for the same
  // reason: the layout effect below runs after this and needs the URL of the
  // page now on screen, which nothing above the guard can compute.
  pageUrlRef.current = pageAt(shown.slot)
  // 1-based — `alt` text is read by a person (directly, or by a screen
  // reader), the same category `Gap`'s page numbers below fall into, so it
  // gets the same conversion at the same kind of boundary: the internal,
  // zero-based `page` is untouched, only what's handed to something a
  // person reads is offset.
  const altAt = (slot: number) => {
    const { page, half } = slots[slot]
    const displayPage = page + 1
    return half ? `page ${displayPage} ${half}` : `page ${displayPage}`
  }

  // Which mechanism governs a mounted `PageView`'s image — see that
  // component's own `buffered` doc for the full reasoning. Scroll mode
  // mounts every group at once and leans on `loading="lazy"` to bound that
  // burst; decode-gating every one of them the instant it mounted would
  // force them all to fetch regardless of that attribute, trading a
  // (measured-ineffective, `docs/status.md`) mitigation for a guaranteed-
  // eager one. Single and two-page mode mount only the current group, so
  // there is nothing for `loading="lazy"` to defer there — see
  // `PageView.tsx`'s original comment on that, unchanged — and decode-gating
  // is what closes the flicker on the page turn itself. `prefetchPrefs.enabled`
  // gates it too: disabling the module must revert `PageView` to exactly its
  // pre-M5b behaviour, since that reversion is what makes the flicker
  // measurement in `docs/notes.md` a comparison rather than a memory.
  const buffered = mode !== 'scroll' && prefetchPrefs.enabled

  // Drag on the page, while the editor is open, to mark a region.
  //
  // Measured against the **painted image**, not the element: `PageView` draws
  // with `object-fit: contain`, so the element includes letterbox that is
  // background rather than page, and a fraction taken from the element would
  // put every region off by that inset. `notes.ts`'s `paintedBox` and
  // `pointInSlot` are that arithmetic, pure and tested; this only supplies the
  // two pixel points.
  //
  // The dragged image has to be the draft's own — `two` mode draws two pages
  // at once, and a drag on the other half would measure against a different
  // page while writing the region onto this note. Checked by `src` rather than
  // by threading slot identity down the tree, since `pageAt` already builds
  // exactly that URL. A swap while the editor is open makes `pageAt` name the
  // other edition's page, so the draft's own edition has to still be the one
  // on screen.
  /**
   * The page image a region gesture may act on: the one drawing the slot the
   * editor is anchored to, while the editor is open on this edition.
   *
   * `two` mode draws two pages at once and a drag on the other half would
   * measure against a different page, so the image is checked by the URL
   * `pageAt` already builds from the slot rather than by threading slot
   * identity down the tree.
   */
  function draftImage(target: EventTarget | null): HTMLImageElement | undefined {
    if (!draft || panelMode !== 'edit' || draft.edition !== shown.edition) return undefined
    if (!(target instanceof HTMLImageElement)) return undefined
    return target.src.endsWith(pageAt(draft.slot)) ? target : undefined
  }

  /** Where a pointer event falls, in fractions of the page it is over. */
  function pointOn(el: HTMLImageElement, clientX: number, clientY: number) {
    const rect = el.getBoundingClientRect()
    const painted = paintedBox(
      { w: el.naturalWidth, h: el.naturalHeight },
      { w: rect.width, h: rect.height },
    )
    return pointInSlot({ x: clientX - rect.left, y: clientY - rect.top }, painted)
  }

  /** What the region should be, given where the drag began and where the pointer is now. */
  function regionUnderPointer(
    started: NonNullable<typeof dragRef.current>,
    event: React.PointerEvent,
  ): Region {
    const from = pointOn(started.el, started.x, started.y)
    const to = pointOn(started.el, event.clientX, event.clientY)
    if (started.intent === 'move' && started.from) {
      return moveRegion(started.from, to.x - from.x, to.y - from.y)
    }
    return regionFromDrag(from, to)
  }

  /**
   * What a press on the page area does. The rule is `notes.ts`'s `gestureFor`,
   * which is where it can be tested; this reads the DOM for it and carries out
   * whichever answer comes back.
   */
  function beginRegion(event: React.PointerEvent) {
    const el = draftImage(event.target)
    const gesture = gestureFor({
      editing: panelMode === 'edit',
      onAnImage: event.target instanceof HTMLImageElement,
      onTheDraftsPage: el !== undefined,
      at: el ? pointOn(el, event.clientX, event.clientY) : { x: -1, y: -1 },
      here,
      slop: GRAB_SLOP,
    })

    if (gesture.kind === 'close') {
      closeEditor()
      return
    }
    if (gesture.kind === 'ignore' || !el) return

    // Not to stop a native image drag — `PageView` already sets
    // `draggable={false}` — but to stop the text selection a press-and-sweep
    // otherwise starts, which paints its own highlight over the page and
    // fights the rectangle being drawn.
    event.preventDefault()

    if (gesture.kind === 'move') {
      // Bring the grabbed note into this session's edits if it is not there
      // already, and show it. Its own region is where the move starts, so
      // grabbing one that was moved earlier continues from where it is now
      // rather than from where the server still has it.
      const held = edits[gesture.noteId] ?? here.find((h) => h.note.id === gesture.noteId)?.note
      if (!held) return
      setEdits((map) => (map[held.id] ? map : { ...map, [held.id]: held }))
      setCurrentId(held.id)
      setNotesNotice(undefined)
      dragRef.current = {
        x: event.clientX,
        y: event.clientY,
        el,
        intent: 'move',
        from: held.region,
        noteId: held.id,
      }
      return
    }
    dragRef.current = { x: event.clientX, y: event.clientY, el, intent: 'draw' }
  }

  function trackRegion(event: React.PointerEvent) {
    const started = dragRef.current
    if (!started) return
    setDragging(regionUnderPointer(started, event))
  }

  function endRegion(event: React.PointerEvent) {
    const started = dragRef.current
    dragRef.current = null
    setDragging(undefined)
    if (!started || !draft) return
    const region = regionUnderPointer(started, event)
    // A click rather than a drag, while drawing: the note stays anchored to
    // the whole slot, which is a real state rather than a failed one — and a
    // one-pixel region would be a marker a reader could never find again. A
    // *move* is exempt: clicking once inside a region without moving it is a
    // no-op that must not delete the region it landed on.
    // A click rather than a drag **plants a point**: a reader marking a spot
    // rather than an area, which is the third anchor a note can have. It used
    // to be discarded, which meant a click on the page did nothing at all and
    // the only way to mark a panel was to draw a box around it.
    const anchor =
      started.intent === 'draw' && (region.w < 0.01 || region.h < 0.01)
        ? pointAt(pointOn(started.el, started.x, started.y))
        : region
    const id = started.noteId ?? draft.id
    setEdits((map) => {
      const note = map[id]
      if (!note) return map
      const next = { ...map, [id]: { ...note, region: anchor, updated: new Date().toISOString() } }
      scheduleSave(next)
      return next
    })
  }

  function renderGroup(group: Group, key: number) {
    if (mode !== 'two') {
      // Single and scroll mode put one slot per group, or — for a joined
      // spread — both halves in one group drawn as a single wide page; see
      // `pairing.ts`'s `groups` doc. Always a bare `PageView` here, in every
      // mode and every group shape, which is what already kept this
      // position's element type stable across every turn — see the
      // two-page branch below for the case where it wasn't.
      const [slot] = group.slots
      return (
        <PageView key={key} src={pageAt(slot)} alt={altAt(slot)} buffered={buffered} decodeQueue={swapQueue} />
      )
    }
    // Two fixed cells, always both present, whether `group` is an ordinary
    // pair, a joined spread, or a solo slot with no partner — see
    // `pairing.ts`'s `cellsFor` doc for the full mechanism. This used to
    // branch the same way the `mode !== 'two'` case above does — a bare
    // `<PageView>` for a spread/solo group, a `<div>` wrapping two for a
    // pair — which flipped the element *type* React saw at this position on
    // exactly the turns that mattered (entering or leaving a joined
    // spread), forcing a remount that threw away `decode.ts`'s whole gate:
    // a freshly-mounted `PageView` has nothing decoded, gate or no gate.
    // Rendering the same wrapping `<div>` and the same two child
    // `<PageView>`s on every render, and only ever changing their
    // `src`/layout props, lets React reconcile by position instead — see
    // `docs/notes.md`'s two-page flicker measurement for the before and
    // after this made.
    const { wide, first, second } = cellsFor(group, effectiveReading)
    return (
      <div key={key} style={{ display: 'flex', width: '100%', height: '100%' }}>
        <div style={{ flex: 1, minWidth: 0, height: '100%' }}>
          <PageView src={pageAt(first)} alt={altAt(first)} buffered={buffered} decodeQueue={swapQueue} />
        </div>
        {/*
          Collapsed to zero width and hidden, not unmounted, when `wide`: a
          spread or solo group has only one real image to show, but keeping
          this cell's `PageView` mounted — with `cellsFor`'s mirrored `src`,
          the same URL `first` is already fetching, a harmless duplicate
          request for a cell nothing paints — is what lets it stay
          decode-gated too. The moment pairing resumes and this cell becomes
          the visible second half of a real pair, its `src` prop changing is
          a plain update on a surviving instance, the ordinary case
          `decode.ts` already handles, not a fresh mount with nothing
          decoded yet.

          `PageView`'s decode gate is per instance: this cell and the one
          beside it each run `presentWhenDecoded` independently, so an
          ordinary pair's two halves can present at different times — one
          new half beside one old half for as long as the slower of the two
          takes to decode. Neither half is ever blank or half-painted on its
          own; simultaneity across the pair is simply not something this
          build guarantees. Left independent deliberately rather than gated
          together: gating would mean the *faster* half always waits on the
          *slower* one, adding latency to the common case for a mismatch
          nothing has measured a reader noticing — particularly now that the
          swap/prefetch queue split above keeps either half's decode from
          being starved. A measurement showing otherwise is the reason to
          revisit this, not a guess made ahead of one.
        */}
        <div
          style={
            wide
              ? { flex: '0 0 0', width: 0, minWidth: 0, height: '100%', overflow: 'hidden' }
              : { flex: 1, minWidth: 0, height: '100%' }
          }
          aria-hidden={wide || undefined}
        >
          <PageView src={pageAt(second)} alt={altAt(second)} buffered={buffered} decodeQueue={swapQueue} />
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
      </div>
      <div
        style={{
          flex: '1 1 auto',
          minHeight: 0,
          overflow: mode === 'scroll' ? 'auto' : 'hidden',
          // What `Annotations`' `inset: 0` is measured against — the page
          // area, not the whole reader, so a margin tab sits beside the page
          // rather than beside the toolbar.
          position: 'relative',
          // What the pointer is for right now, while the editor is open:
          // sweep out a rectangle, or push the one already there. Without
          // this, that a region can be moved at all is invisible until a
          // reader happens to try it.
          cursor:
            panelMode === 'edit' && draft?.edition === shown.edition
              ? draft.region
                ? 'move'
                : 'crosshair'
              : undefined,
        }}
        ref={pageAreaRef}
        onPointerDown={beginRegion}
        onPointerMove={trackRegion}
        onPointerUp={endRegion}
        // A pointer released outside the page area (or taken away by the
        // browser) must not leave a rectangle stranded on screen with no drag
        // behind it.
        onPointerLeave={endRegion}
        onPointerCancel={() => {
          dragRef.current = null
          setDragging(undefined)
        }}
      >
        {/*
          Single and two-page mode draw one group, so one overlay over the
          page holding the slot being read. Scroll mode mounts every group at
          once and gets one overlay per group instead — see `renderGroup`'s
          caller below. Both go through `PageAnnotations`, which measures the
          page it sits over rather than assuming the box around it.
        */}
        {mode !== 'scroll' && (
          <PageAnnotations
            // Remounted when the mode changes, so the observer never keeps
            // watching an image the mode switch replaced — see
            // `PageAnnotations`' own doc.
            key={mode}
            match={pageAt(shown.slot)}
            here={hereBehind}
            prefs={overlayPrefs}
            // The live rectangle while dragging, and the settled one after the
            // pointer is released but before Save — so a reader can see what
            // they drew, and see it again when they reopen the note. Only
            // while the editor is open on *this* slot.
            pending={
              panelMode === 'edit' &&
              draft?.edition === shown.edition &&
              draft.slot === shown.slot
                ? (dragging ?? draft.region)
                : undefined
            }
            drawing={dragging !== undefined}
            pendingBody={draft?.body}
            pendingColour={draft?.color}
            onOpen={openNote}
          />
        )}
        {/*
          Beside the page, inside the page area so it sits in the margin the
          page leaves rather than over the toolbar. Only while revealed: the
          list is a way to *read* the notes, and `r` is the key that asks for
          that.
        */}
        {annotationsPrefs.reveal && notesPlacement === 'side' && mode !== 'scroll' && (
          <NoteList here={here} placement="side" onOpen={openNote} />
        )}
        {mode === 'scroll' ? (
          <div style={{ display: 'flex', flexDirection: 'column' }}>
            {grouped.map((g, i) => (
              <div
                key={i}
                ref={i === currentIndex ? currentGroupRef : undefined}
                // Positioned, so each group's own overlay measures against the
                // page in *this* group rather than the scrolling column.
                style={{ position: 'relative' }}
              >
                {renderGroup(g, i)}
                {/*
                  One overlay per mounted group, so a note is on its page while
                  scrolling exactly as it is in the other two modes. The notes
                  for every slot are resolved once for the whole edition
                  (`notesBySlot`) rather than once per group, which is one
                  resolve per note instead of one per page — 195 pages on the
                  primary corpus volume would otherwise scan the note list 195
                  times.
                */}
                <PageAnnotations
                  key={`${mode}:${i}`}
                  match={pageAt(g.slots[0])}
                  here={(scrolledNotes.get(g.slots[0]) ?? []).filter((h) => h.note.id !== currentId)}
                  prefs={overlayPrefs}
                  pending={
                    panelMode === 'edit' &&
                    draft?.edition === shown.edition &&
                    g.slots.includes(draft.slot)
                      ? (dragging ?? draft.region)
                      : undefined
                  }
                  drawing={dragging !== undefined}
                  pendingBody={draft?.body}
                  pendingColour={draft?.color}
                  onOpen={openNote}
                />
              </div>
            ))}
          </div>
        ) : (
          current && renderGroup(current, 0)
        )}
      </div>
      {annotationsPrefs.reveal && notesPlacement === 'below' && (
        <NoteList here={here} placement="below" onOpen={openNote} />
      )}
      <Chrome
        visible={chromeVisible}
        motion={motionPrefs}
        readouts={readoutsPrefs}
        position={positionText}
        time={timeText}
        chapter={chapterText}
        health={healthText}
        notes={notesText}
        work={work}
        edition={pending ? pending.to : shown.edition}
        slots={pending && correctionSlots ? correctionSlots : slots}
        groups={pending && correctionGroups ? correctionGroups : grouped}
        // No group of `pending.to`'s own is "current" while correcting — the
        // reader is still looking at `pending.from`, not `to` — so nothing
        // should draw the solid "where you are" border there. `pairedIndex`'s
        // dashed one is the only marker while this is unset.
        currentIndex={pending ? -1 : currentIndex}
        onNavigate={goToSlot}
        correcting={pending ? { to: pending.to, pairedIndex } : undefined}
        onPickCounterpart={pending ? pickCounterpart : undefined}
      />
      {pending && (
        <div
          role="status"
          style={{
            position: 'absolute',
            top: 16,
            left: 16,
            right: 16,
            zIndex: 1,
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            gap: 8,
            padding: '10px 16px',
            borderRadius: 8,
            background: 'rgba(24, 24, 24, 0.92)',
            color: 'rgba(255, 255, 255, 0.92)',
            textAlign: 'center',
          }}
        >
          {pending.b === undefined ? (
            <p style={{ margin: 0 }}>
              Pick the <strong>{pending.to}</strong> page that matches <strong>{pending.from}</strong>{' '}
              {describePosition(pending.a)}. <em>Escape</em> cancels without writing anything.
            </p>
          ) : (
            <>
              {/* The assertion itself, restated at the moment of confirming.
                  Without this the banner swapped the "pick a page" prompt for
                  the consequence line alone, so the one screen a reader
                  confirms an irreversible write from never said what was
                  being asserted — found by driving this against the real
                  corpus, where the strip has 235 tiles and the picked one
                  scrolls out of view as easily as not. */}
              <p style={{ margin: 0 }}>
                Pair <strong>{pending.from}</strong> {describePosition(pending.a)} with{' '}
                <strong>{pending.to}</strong> {describePosition(pending.b)}?
              </p>
              {/* Named before the write, not after it — see `displaced`'s own
                  doc above. Shown even when `displaced` is undefined, so a
                  reader sees the same confirmation shape either way rather
                  than wondering whether the absence of a warning is itself
                  the warning. */}
              <p style={{ margin: 0 }}>
                {displaced
                  ? `${pending.from} ${describePosition(displaced)} will lose its ${pending.to} counterpart.`
                  : `Nothing loses its counterpart — ${pending.to} ${describePosition(pending.b)} pairs with nothing today.`}
              </p>
              {correctionNotice && <p style={{ margin: 0, opacity: 0.85 }}>{correctionNotice}</p>}
              {/* Both disabled while the write is on the wire — see
                  `submitting`'s doc. Cancel especially: it is the control
                  that most looks like it undoes the write, and it is the one
                  that cannot. */}
              <div style={{ display: 'flex', gap: 8 }}>
                <button type="button" disabled={submitting} onClick={() => void confirmCorrection()}>
                  {submitting ? 'Writing…' : 'Confirm'}
                </button>
                <button
                  type="button"
                  disabled={submitting}
                  onClick={() => {
                    setPending(undefined)
                    setCorrectionNotice(undefined)
                  }}
                >
                  Cancel
                </button>
              </div>
            </>
          )}
        </div>
      )}
      <NotePanel
        mode={panelMode}
        draft={draft}
        entries={entries}
        doc={notesDoc ?? { work, notes: [] }}
        work={work}
        viewing={shown.edition}
        busy={notesBusy}
        notice={notesNotice}
        onBody={(body) =>
          editCurrent((current) =>
            // `updated` moves with the body, `created` never does — the two
            // are what a review list would sort by, and a note whose creation
            // time crept forward on every keystroke would sort by when it was
            // last touched while claiming to sort by when it was written.
            ({ ...current, body, updated: new Date().toISOString() }),
          )
        }
        onColour={(colour) =>
          editCurrent((current) =>
            ({ ...current, color: colour, updated: new Date().toISOString() }),
          )
        }
        // Every note this session touched, not only the one on show — the
        // point of an edit mode is that a reader can nudge three boxes and
        // save once, and a Save that wrote only the last one they happened to
        // click would silently drop the other two.
        onDelete={(id) => {
          // Deleting is deliberate and irreversible-looking, so it goes now
          // rather than in 700 ms — and it cancels whatever the debounce was
          // about to write, which would otherwise put the note straight back.
          saveNotes.cancel()
          setEdits((map) => {
            const rest = { ...map }
            delete rest[id]
            return rest
          })
          setCurrentId(undefined)
          setPanelMode(undefined)
          void writeNotes(withoutNote(applyEdits(), id))
        }}
        onClose={() => {
          // Flush rather than drop: with no Save button, closing the editor is
          // not "abandon this", it is "I am done here" — and anything still
          // sitting in the debounce is a change the reader made and expects to
          // find next time. Undo is what abandons.
          saveNotes.flush()
          setPanelMode(undefined)
          setEdits({})
          setCurrentId(undefined)
          setNotesNotice(undefined)
        }}
        canUndo={canUndo(notesHistory)}
        canRedo={canRedo(notesHistory)}
        onUndo={stepBack}
        onRedo={stepForward}
        onGo={(entry) => {
          if (entry.here === undefined) return
          goToSlot(entry.here)
          setPanelMode(undefined)
        }}
      />
      <Settings
        visible={settingsVisible}
        modules={registry.all()}
        prefs={prefsMap}
        onChange={handleSettingChange}
        onReset={handleSettingReset}
      />
    </div>
  )
}

export default App
