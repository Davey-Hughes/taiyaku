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
import {
  describePosition,
  displacedBy,
  spanOf,
  type Pending,
} from './correction'
import { createDecodeQueue, decodeTo, type DecodeQueue } from './decode'
import { debounce, type Debounced } from './debounce'
import {
  canRedo,
  canUndo,
  current as currentHistory,
  emptyHistory,
  record,
  redo,
  undo,
  type History,
} from './history'
import { Gap } from './Gap'
import { CreditsPanel } from './CreditsPanel'
import { Help } from './Help'
import { intentOf, isInteractivePress, isInteractiveTarget } from './keys'
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
import {
  creditSlotsIn,
  creditsModule,
  PANEL_INTENT,
  skippedApartFrom,
  type CreditsPrefs,
} from './modules/credits'
import {
  directionModule,
  effectiveDirection,
  stepFor,
  setOverride,
  TOGGLE_INTENT,
  type Direction,
} from './modules/direction'
import { fitModule, seamStyle, type ArtworkHug } from './modules/fit'
import {
  enabledModules,
  helpModule,
  TOGGLE_INTENT as HELP_TOGGLE_INTENT,
  type HelpPrefs,
} from './modules/help'
import { motionModule } from './modules/motion'
import { prefetchModule, window as prefetchWindow } from './modules/prefetch'
import {
  chapterReadout,
  gapHealth,
  healthReadout,
  nextTurns,
  pagesPerMinute,
  positionLine,
  readoutsModule,
  timeLine,
} from './modules/readouts'
import { layoutModule } from './modules/layout'
import { registry } from './modules/registry'
import {
  regroupModule,
  toggleEditionBreak,
  TOGGLE_INTENT as REGROUP_TOGGLE_INTENT,
} from './modules/regroup'
import { TOGGLE_INTENT as SETTINGS_TOGGLE_INTENT } from './modules/settings'
import {
  IN_INTENT as ZOOM_IN_INTENT,
  OUT_INTENT as ZOOM_OUT_INTENT,
  WHOLE_INTENT as ZOOM_WHOLE_INTENT,
  zoomModule,
  type ZoomPrefs,
} from './modules/zoom'
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
import {
  cellOf,
  cellsFor,
  correctionTiles,
  groups,
  type Group,
  type Mode,
} from './pairing'
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
import {
  wasmResolver,
  type Chapter,
  type Position,
  type Resolver,
} from './resolver'
import {
  alignmentsOf,
  applyIntent,
  initialPrevious,
  orderedEditions,
  takeOffered,
  verdictFor,
  type Alignment,
  type ReadingIntent,
} from './session'
import { nextColour } from './palette'
import { cryptoRandom, ulid } from './ulid'
import { Viewport } from './Viewport'
import {
  WHOLE,
  clampRect,
  mapView,
  panBy,
  wheelBy,
  zoomAt,
  type Cell,
  type Drawn,
  type Frame,
  type MapKind,
  type Rect,
} from './viewport'

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
const sameNotes = (a: Note[], b: Note[]) =>
  JSON.stringify(a) === JSON.stringify(b)

/**
 * How near a marker counts as grabbing it, in page fractions — the space
 * `pointOn` below hands back, per `notes.ts`'s `Region`.
 *
 * A point has no area, so without this the only way to take hold of one would
 * be to hit an infinitely small spot. Roughly the radius `Annotations` draws a
 * pin at, so what a reader can grab is what they can see.
 */
const GRAB_SLOP = 0.02

/** One empty set rather than a fresh one per render, so nothing skipped stays cheap and referentially stable. */
const EMPTY_SLOTS: Set<number> = new Set()

/**
 * Which of `viewport.ts`'s derived transforms `onSwap: 'content'` maps
 * through.
 *
 * **A result, and it said this line was wrong.** It read `'letterbox'` until
 * 2026-08-08, on the guess that a letterbox "is centred by construction" — the
 * rule that took Tongari v09 from 98 runs to 7 — and that framing differences
 * between packagings arise the same way. The corpus says the reasoning was
 * sound about letterboxing and wrong about these editions: the shared artwork
 * is as tall as the taller page, and the wider one carries margin at its
 * sides, not its top and bottom.
 *
 * `docs/notes.md`, "Which derived transform maps a rect between editions", is
 * the measurement — 288 slots over twelve corpus volumes, `raw-mono` against
 * `en-mono` — and `mapRect`'s own doc carries the figures beside the
 * arithmetic. What `'pillarbox'` beat, in order: `'letterbox'`, which corrects
 * nothing on the axis that needs it and adds four times the error on the axis
 * that does not, so it loses to doing nothing at all; `'identity'`, which is
 * exactly right on the vertical and uncorrected on the horizontal, where
 * `'pillarbox'` cuts the median residual from 0.0296 of page width to 0.0195;
 * and `'reset'`, a half-page bound rather than a residual. None of the three
 * is what `mapView` corrects most visibly — that is the fit-whole letterbox
 * *inside* the page area, which `paintedIn` removes whichever one is chosen.
 *
 * Still the seam rather than the decision: had no candidate beaten throwing
 * the view away, the design's §4 promotion rule ships reset-on-swap, which is
 * a change to `modules/zoom.ts`'s `defaults.onSwap` and not to this constant.
 */
const CONTENT_MAP: MapKind = 'pillarbox'

/** A rect's own centre — where a keyboard zoom anchors, having no pointer to anchor on. */
const centreOf = (rect: Rect) => ({
  x: rect.x + rect.w / 2,
  y: rect.y + rect.h / 2,
})

/**
 * The natural size of the page `url` names, from the images the page area is
 * drawing right now.
 *
 * Matched by URL suffix rather than by threading slot identity down the tree,
 * the same rule `draftImage` and `PageAnnotations` already match by — `pageAt`
 * builds exactly this URL, and `two` mode draws two pages of which only one is
 * the slot being asked about.
 *
 * `undefined` covers three states that are one state for a caller: the page is
 * not on screen, or it is but has not decoded (`PageView`'s decode gate holds
 * the *outgoing* page's `src` on the element until the incoming one is ready,
 * so a swap pressed twice quickly genuinely has no frame to measure), or it
 * decoded to nothing. All three mean the transform cannot be computed, which
 * `mapView` and the design's §9 both answer by leaving the view alone.
 */
function frameOf(host: HTMLElement, url: string): Frame | undefined {
  const img = [...host.querySelectorAll('img')].find((candidate) =>
    candidate.src.endsWith(url),
  )
  if (!img || img.naturalWidth <= 0 || img.naturalHeight <= 0) return undefined
  return { w: img.naturalWidth, h: img.naturalHeight }
}

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
  return Array.from(
    { length: count },
    (_, index) => resolver.slotAt(edition, index)!,
  )
}

/**
 * The three reading modes, over the state `keys.ts`'s intents drive.
 *
 * `Space`, digits and arrows reach `session.ts`'s `applyIntent` via the
 * keydown listener below, which calls `swap` (for `toggle`/`view`) or
 * `navigate` (for `step`) — never both for one keypress. So `viewing` really
 * can diverge from `authority`, and `display` really can report `'gap'`:
 * landing, after a swap, on a slot with no counterpart in the edition just
 * swapped to. That state is drawn by `Gap`, in place of the page area, rather
 * than by an early return out of this whole function (that was O25) — a gap
 * has no slot to draw and no group to belong to, but `Chrome`, `Settings` and
 * the note panel below still mount, so `at` (below) is what the rest of this
 * function narrows on wherever a slot is actually needed.
 */
function App() {
  const [session, setSession] = useState<Session>()
  // The reading mode is a preference, not component state: it was a
  // `useState` until 2026-08-08 and so was forgotten on every reload, which a
  // reader who works in two-page mode met every time they opened the reader.
  // `layoutModule` holds it now, which is what makes it load, save and reach
  // the settings panel; `setMode` writes that slice and nothing else changes,
  // because every read below still reads a `Mode`.
  // The part of the drawn group the reader is looking at. `WHOLE` is
  // fit-whole, which is what every mode did before zoom existed and what
  // scroll mode still does — see the `Viewport` wrapper below, which is only
  // mounted for the two modes that draw one group into a fixed box.
  //
  // Every rect held here satisfies `w === h`; `Rect`'s own doc says why that
  // is load-bearing and which functions may break it. The four that write
  // this state — `zoomAt`, `panBy`, `clampRect` and `mapView` — all preserve
  // it, and nothing else may.
  const [view, setView] = useState<Rect>(WHOLE)
  // `Tab`'s target — not `session` state, since it names nothing about the
  // reading model, only whether the strip and readout row are on screen.
  const [chromeVisible, setChromeVisible] = useState(false)
  // `s`'s target (`modules/settings.ts`) — same reasoning as `chromeVisible`
  // above, and deliberately a separate flag rather than reusing it: the
  // panel and the chrome are shown and hidden independently.
  const [settingsVisible, setSettingsVisible] = useState(false)
  // `C`'s target (`modules/credits.ts`), the same shape `settingsVisible` has
  // and for the same reason: panel visibility is UI state with no bearing on
  // `ReadingState`, so it never reaches `applyIntent`.
  const [creditsVisible, setCreditsVisible] = useState(false)
  // `?`/`h`'s target (`modules/help.ts`), the same shape again. Both keys
  // carry one intent, so this is one flag and not two.
  const [helpVisible, setHelpVisible] = useState(false)

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
  const [notesHistory, setNotesHistory] = useState<History<Note[]>>(() =>
    emptyHistory<Note[]>([]),
  )
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
    Object.fromEntries(
      registry
        .all()
        .map((mod) => [mod.id, loadPrefs(mod.id, mod.defaults as object)]),
    ),
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
  const mode = sliceFor(prefsMap, layoutModule).mode
  const setMode = (next: Mode) =>
    setPrefsMap((map) => ({ ...map, [layoutModule.id]: { mode: next } }))
  const directionPrefs = sliceFor(prefsMap, directionModule)
  const motionPrefs = sliceFor(prefsMap, motionModule)
  const readoutsPrefs = sliceFor(prefsMap, readoutsModule)
  const prefetchPrefs = sliceFor(prefsMap, prefetchModule)
  const annotationsPrefs = sliceFor(prefsMap, annotationsModule)
  const regroupPrefs = sliceFor(prefsMap, regroupModule)
  const fitPrefs = sliceFor(prefsMap, fitModule)
  // Read here rather than beside `creditsPrefs` further down, because two
  // effects declared above the `if (!session) return null` guard close over
  // it — the wheel subscription and the swap's own geometry — and a binding
  // created after that guard is not in scope for either.
  const zoomPrefs = sliceFor(prefsMap, zoomModule)

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
  const [swapQueue, setSwapQueue] = useState<DecodeQueue>(() =>
    createDecodeQueue(prefetchPrefs.maxConcurrent),
  )
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

  // Whether the help overlay is open, for the same `Escape` branch and by the
  // same route. Kept in an effect rather than assigned during render like
  // `creditsPrefsRef` below, because this is plain state and not derived from
  // anything — the split `panelModeRef` above already draws, and for the same
  // reason: nothing recomputes this value, so a render that React discards has
  // nothing to recompute it back from.
  const helpVisibleRef = useRef(false)
  useEffect(() => {
    helpVisibleRef.current = helpVisible
  }, [helpVisible])

  // The grouping the page area is drawing right now, for the keydown effect —
  // which subscribes once at load and so cannot see `grouped`, computed below
  // at render from `mode`, the direction override and the reader's own parity
  // breaks. An arrow steps by group rather than by slot (`pairing.ts`'s
  // `stepToGroup`), and only this component knows what the groups are.
  const groupedRef = useRef<Group[]>(undefined)

  // The credits preferences, for the keydown handler — which subscribes once
  // at load and so cannot see `prefsMap`. Assigned during render like
  // `groupedRef`, and for the same reason: it is derived state, recomputed
  // from props and state every render, so a discarded render writes the same
  // value the retained one will.
  const creditsPrefsRef = useRef<CreditsPrefs>(creditsModule.defaults)

  // See `readingRef.current =` below for why this is a ref rather than a read.
  const readingRef = useRef<Direction>('rtl')

  // The zoom preferences, for the keydown handler, in exactly the shape and
  // for exactly the reason `creditsPrefsRef` above has: `=`, `-` and `0` need
  // `enabled`, `step` and `maxScale`, and that handler subscribes once at
  // load. Assigned during render beside it. The *rect* needs no ref — the
  // three cases write it through `setView`'s updater form, which hands them
  // the current value without closing over it.
  const zoomPrefsRef = useRef<ZoomPrefs>(zoomModule.defaults)

  // The help preferences, for the keydown handler, in `creditsPrefsRef`'s
  // shape and for its reason: `?` and `h` need `enabled`, and that handler
  // subscribes once at load. Assigned during render beside it, because like
  // `creditsPrefs` it is derived — recomputed from `prefsMap` every render, so
  // a discarded render writes what the retained one will.
  const helpPrefsRef = useRef<HelpPrefs>(helpModule.defaults)

  // What the page area is drawing right now, for the keydown handler and for
  // the swap effect below — the page holding the slot the reader is standing
  // on, which of the group's cells draws it, and which half of it the slot
  // actually names. Assigned during render like `groupedRef`, for the same
  // reason.
  //
  // `half` is `Drawn`'s, and it is the slot's rather than the image's: a
  // joined spread is one image drawn for two slots, so the URL and the frame
  // are the same on both and only this tells `mapView` which of the two the
  // reader is on. Absent for every ordinary slot.
  //
  // `undefined` is not "not measured yet": it is exactly the two states with
  // no page to zoom into. Scroll mode keeps fit-whole and mounts no
  // `Viewport` (the spec's scope decision), and a gap draws `Gap` in place of
  // the page area. Both read the same way at every use — there is nothing on
  // screen for a zoom, a pan or a swap's arithmetic to be about.
  const drawnRef = useRef<
    { url: string; cell: Cell; half?: 'L' | 'R' } | undefined
  >(undefined)

  // The page area itself, for the two things that genuinely need the box
  // rather than the image inside it: the wheel subscription (which must be
  // non-passive, see its own comment) and the swap, which measures both
  // pages' natural sizes out of it and takes the box's aspect to fit against.
  //
  // A `pageAreaRef` stood here before and was removed as O27, because it was
  // read by nothing: the annotation overlay measures the `<img>` it sits over,
  // not the area around it, and that distinction is the fix `notes.md` records
  // under "The annotation overlay was placed on the page area, not on the
  // page". Nothing here disturbs that — the region drag still reads its
  // geometry off the `<img>` via `draftImage`/`pointOn`. What this ref is for
  // is the box zoom is expressed in fractions *of*, which is the page area and
  // could not be the page.
  const pageAreaRef = useRef<HTMLDivElement>(null)

  // A swap whose incoming page has not been measured yet. Set by the keydown
  // handler, which is the last moment the outgoing page is still on screen to
  // measure; consumed by the effect below, once the incoming one has decoded.
  // `edition` is the one being *left*, which is what tells a swap that changed
  // nothing (a digit for the edition already showing) from one that did.
  const swapRef = useRef<{ from: Drawn; edition: string } | undefined>(
    undefined,
  )

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
  // Where the pointer was on the previous pan frame, in client pixels. A ref
  // rather than state and a *previous* position rather than the drag's origin:
  // `panBy` takes a delta and clamps, so each frame only needs the step since
  // the last one — accumulating against the origin instead would fight the
  // clamp, replaying the whole drag every frame once an edge is reached.
  const panRef = useRef<{ x: number; y: number } | null>(null)
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
      const editions = orderedEditions(
        sidecar.hub,
        Object.keys(sidecar.editions),
      )
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: session is read via setSession's updater below, not closed over; see above.
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
        // The help overlay closes first, ahead of everything else here.
        //
        // **This is a departure, and a deliberate one:** neither the settings
        // panel nor the credits panel closes on `Escape` — both are dismissed
        // by pressing their own key again or by their Close button, and this
        // one can be too. Matching them was the alternative and it is what
        // "consistent" would have meant, but the panel this most resembles is
        // the note panel a few lines below, which does close on `Escape`: a
        // `role="dialog"` a reader has just opened over the page is the thing
        // `Escape` means, and the plan's own browser check names it.
        //
        // First rather than last because closing this discards nothing, while
        // every branch under it either abandons a correction or flushes a
        // note. If both are open, spending the keypress on the one that costs
        // nothing and can be reopened with `?` is the cheaper mistake.
        //
        // Not gated on `enabled`, unlike the toggle further down: a reader who
        // switches the module off while the overlay is up would otherwise be
        // left with a panel whose own keys have just gone inert.
        if (helpVisibleRef.current) {
          event.preventDefault()
          setHelpVisible(false)
          return
        }
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
        const to =
          current.state.authority !== shown.edition
            ? current.state.authority
            : current.previous
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
                effectiveDirection(sidecarReading, prefs.override[edition]) ===
                'rtl'
                  ? 'ltr'
                  : 'rtl'
              return {
                ...map,
                [directionModule.id]: setOverride(prefs, edition, flipped),
              }
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
              [regroupModule.id]: toggleEditionBreak(
                sliceFor(map, regroupModule),
                shown.edition,
                shown.slot,
              ),
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
          case PANEL_INTENT: {
            // Gated on the preference here rather than inside the component,
            // so a reader who has switched the panel off pays no render and
            // `C` is genuinely inert — which is what "Offer the panel that
            // lists them" means.
            if (!creditsPrefsRef.current.panel) return
            setCreditsVisible((shown) => !shown)
            break
          }
          // The three zoom keys share one guard and differ only in what they
          // hand `zoomAt`. Gated on `drawnRef.current` as well as the
          // preference, so they are inert exactly where the `Viewport` wrapper
          // is not mounted or has nothing under it — scroll mode and a gap.
          // Without that, `=` in scroll mode would silently zoom a rect nobody
          // is drawing, and the reader would find it applied on their next
          // switch back to single.
          case ZOOM_IN_INTENT: {
            const zoom = zoomPrefsRef.current
            if (!zoom.enabled || !drawnRef.current) return
            // A keyboard zoom has no pointer to anchor on, so it anchors on
            // the rect's own centre — the one case where `zoomAt`'s doc's
            // "zooming about the centre feels broken" does not apply, because
            // there is no point the reader is aiming at for it to slide away
            // from.
            setView((rect) =>
              zoomAt(rect, centreOf(rect), zoom.step, zoom.maxScale),
            )
            break
          }
          case ZOOM_OUT_INTENT: {
            const zoom = zoomPrefsRef.current
            if (!zoom.enabled || !drawnRef.current) return
            setView((rect) =>
              zoomAt(rect, centreOf(rect), 1 / zoom.step, zoom.maxScale),
            )
            break
          }
          case ZOOM_WHOLE_INTENT: {
            if (!zoomPrefsRef.current.enabled || !drawnRef.current) return
            setView(WHOLE)
            break
          }
          case HELP_TOGGLE_INTENT: {
            // One case for two keys, which is what one intent across `?` and
            // `h` buys — see `modules/help.ts`. Gated on the preference here
            // rather than inside `Help`, exactly as `PANEL_INTENT` above is
            // and for its reason: switched off, both keys are genuinely inert
            // and nothing renders.
            if (!helpPrefsRef.current.enabled) return
            setHelpVisible((shown) => !shown)
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
      // What this keypress does to the view, decided **before** `setSession`
      // rather than after it. A swap has to measure the outgoing page, and the
      // only place that page is still on screen to be measured is here: React
      // batches an event handler's updates and re-renders once it returns, so
      // "after the state update" and "before it" are the same DOM either way —
      // but only one of the two says so.
      if (intent.kind === 'step') {
        // A turn carries both zoom and pan (spec §3) and re-clamps. Not a
        // mapping: same edition, same coordinate space.
        //
        // **Idempotent as the code stands, and kept anyway.** `clampRect`'s
        // own doc used to frame this as a rect carried from a pair onto a
        // singleton addressing space that does not exist; it now retracts that
        // sentence and dates the retraction, because it is true of a rect in
        // *slot* space and not of one in group space, where the group is
        // always the whole page-area box and a rect inside it stays inside it.
        // This comment pointed at the old wording for four commits after it
        // was withdrawn — the hazard of citing a neighbour's reasoning rather
        // than restating it.
        // Every producer of this state (`zoomAt`, `panBy`, `mapView`) already
        // clamps, so this changes nothing today. It is here because the bound
        // belongs to the state rather than to each producer's good manners,
        // and a turn is where a rect that had drifted would first be visible.
        setView(clampRect)
      } else {
        recordSwap()
      }
      setSession((prev) => {
        if (!prev) return prev
        // An arrow is a *physical* key and `intentOf` reports it as one; which
        // page that reaches depends on the work. `stepFor` is the single rule
        // (`modules/direction.ts`), shared with the filmstrip's layout and the
        // help's labels, so the three cannot disagree about which way is
        // forward — which they did until 2026-08-08, when the strip ran
        // left-to-right under a right-to-left work.
        const turned: ReadingIntent =
          intent.kind === 'step'
            ? { ...intent, by: stepFor(intent.by, readingRef.current) }
            : intent
        const { state, previous } = applyIntent(
          turned,
          prev.state,
          prev.previous,
          resolver,
          groupedRef.current,
        )
        return { ...prev, state, previous }
      })
    }
    /**
     * Measure the outgoing page and leave the rest of the swap to the effect
     * below, which runs once the incoming one has decoded.
     *
     * Two halves because the swap's two frames are available at two different
     * moments and neither can wait for the other: the outgoing page is on
     * screen now and gone after the re-render, and the incoming one does not
     * exist yet — `PageView`'s decode gate deliberately holds the old image
     * until the new one is ready to paint, so reading `naturalWidth` in this
     * handler would measure the page being left, twice.
     *
     * Anything that cannot be measured leaves `swapRef` clear, which the
     * effect reads as "this swap carries nothing" and the reader sees as a
     * view left where it was — the design's §9 answer for a swap onto a gap,
     * reached the same way for a swap pressed faster than a page decodes.
     */
    function recordSwap() {
      swapRef.current = undefined
      const drawn = drawnRef.current
      const host = pageAreaRef.current
      const leaving = sessionRef.current?.state.viewing
      if (!zoomPrefsRef.current.enabled || !drawn || !host || !leaving) return
      const frame = frameOf(host, drawn.url)
      if (!frame) return
      swapRef.current = {
        from: { frame, cell: drawn.cell, half: drawn.half },
        edition: leaving,
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [session?.resolver, session?.editions])

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
  const writeProgressRef = useRef<Debounced<
    [string, { edition: string; slot: number }]
  > | null>(null)
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
  const writeNotesLatestRef = useRef<((notes: Note[]) => Promise<void>) | null>(
    null,
  )

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
  // biome-ignore lint/correctness/useExhaustiveDependencies: session changes on every swap too; adding it here would write progress on every swap, not just every navigation. See above.
  useEffect(() => {
    if (!session) return
    if (!restored.current) {
      restored.current = true
      return
    }
    writeProgress(session.work, {
      edition: session.state.authority,
      slot: session.state.slot,
    })
  }, [
    session?.work,
    session?.state.authority,
    session?.state.slot,
    writeProgress,
  ])

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
  // biome-ignore lint/correctness/useExhaustiveDependencies: session changes on every swap too, and a swap is not a page turn; see above.
  useEffect(() => {
    if (!session) return
    const skip = !restoredTurn.current
    restoredTurn.current = true
    turnsRef.current = nextTurns(turnsRef.current, Date.now(), skip)
  }, [session?.state.authority, session?.state.slot])

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
  // biome-ignore lint/correctness/useExhaustiveDependencies: the three `session?.state.*` deps are triggers, not closure reads — the body touches only `mode` and a ref, and exists to re-scroll when the position changes. Removing them, which is what this rule suggests, would leave scroll mode never following the reader.
  useEffect(() => {
    if (mode !== 'scroll') return
    currentGroupRef.current?.scrollIntoView({ block: 'start' })
  }, [
    mode,
    session?.state.authority,
    session?.state.slot,
    session?.state.viewing,
  ])

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
  // biome-ignore lint/correctness/useExhaustiveDependencies: the three `session?.state.*` deps are triggers, not closure reads — this effect reads `sessionRef.current` and exists to re-run the prefetch window when the position moves. Removing them, which is what this rule suggests, would prefetch once and never again.
  useEffect(() => {
    const current = sessionRef.current
    if (!current || mode === 'scroll' || !prefetchPrefs.enabled) return
    const shownNow = display(current.state, current.resolver)
    if (shownNow.kind !== 'at') return
    const total = current.resolver.slotCount(shownNow.edition) ?? 0
    const targets = prefetchWindow(
      shownNow.slot,
      total,
      prefetchPrefs.ahead,
      prefetchPrefs.behind,
    )
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

  // Whether the page area is on screen to subscribe to — see the wheel
  // effect's own dependency note below. A boolean rather than `session`, so
  // "the reader moved" is not confused with "there is something to attach to".
  const loaded = session !== undefined

  // The wheel, subscribed by hand rather than through React's `onWheel`.
  //
  // React registers `wheel` on the root container as a **passive** listener —
  // `react-dom`'s `addTrappedEventListener` passes `passive: true` for exactly
  // `touchstart`, `touchmove` and `wheel`, checked in
  // `node_modules/react-dom/cjs/react-dom-client.development.js` at this
  // version (19.2) — and `preventDefault()` inside a passive listener does
  // nothing except log a console error, once per notch. Zoom has to stop the
  // browser acting on the same wheel: a trackpad pinch arrives here as a
  // `wheel` with `ctrlKey` set, and left alone it zooms the whole page's chrome
  // instead of the manga. So this is `{ passive: false }` on the page area
  // itself. The same event serving both gestures is why the design's table
  // lists "wheel / pinch" as one row.
  //
  // Declared ahead of the `if (!session) return null` below, like the two
  // effects above it, which is why `loaded` is in the dependencies: the page
  // area does not exist until the session does, and a ref to an element React
  // has not rendered yet is `null`. `loaded` rather than `session` itself, so
  // this subscribes when the page area appears rather than tearing the
  // listener down and putting it back on every page turn.
  //
  // biome-ignore lint/correctness/useExhaustiveDependencies: `loaded` is a mount trigger, not a closure read — the body reads only refs, `mode` and `zoomPrefs`. Removing it, which is what this rule suggests, would subscribe once against a page area that does not exist yet and never again.
  useEffect(() => {
    const host = pageAreaRef.current
    if (!host || !zoomPrefs.enabled || mode === 'scroll') return
    function onWheel(event: WheelEvent) {
      if (!host) return
      // Inert wherever the three zoom keys are, for the reason the
      // `ZOOM_IN_INTENT` case above states in full. Checked inside the handler
      // rather than beside `zoomPrefs.enabled` above, because `drawnRef` is
      // assigned during render and none of this effect's dependencies changes
      // when it flips: the page area still renders in a gap — it just draws
      // `Gap` in place of the group — so the listener stays attached across
      // one, and so does the `currentIndex === -1` case that draws nothing.
      //
      // Ahead of `preventDefault()`, so a wheel this declines is left to the
      // browser rather than swallowed. That is what scroll mode already does,
      // by attaching no listener at all.
      if (!drawnRef.current) return
      event.preventDefault()
      const box = host.getBoundingClientRect()
      // A zero-sized box is an element mid-teardown, not a viewport, and
      // dividing by it puts `NaN` into `at` — which `zoomAt` propagates into
      // the rect, `Viewport` turns into `scale(NaN)`, and the reader sees the
      // page vanish with no error anywhere. Task 1's review flagged that
      // neither `clampRect` nor `fitRect` screens non-finite sizes; this is
      // the boundary where DOM numbers enter, so it is the place to stop them
      // rather than hardening every pure function against a caller that
      // should not exist.
      if (box.width <= 0 || box.height <= 0) return
      // Proportional to the delta, and normalised across `deltaMode` first —
      // `wheelBy`'s doc has the whole rule and names every constant it guesses
      // at. What was here before was `deltaY < 0 ? step : 1 / step`, a full
      // step per event whatever its size, which at the defaults let a single
      // trackpad flick pin the view at `maxScale`.
      const by = wheelBy(event, zoomPrefs.step)
      setView((rect) =>
        zoomAt(
          rect,
          // Where the pointer is over the *group*, which is what `zoomAt`
          // anchors on — the page area's own fractions, mapped back through
          // the rect currently on screen. Computed inside the updater rather
          // than from a `view` this closure captured: two notches inside one
          // React batch would otherwise both anchor against the rect before
          // either of them, and the second would undo part of the first.
          {
            x: rect.x + ((event.clientX - box.left) / box.width) * rect.w,
            y: rect.y + ((event.clientY - box.top) / box.height) * rect.h,
          },
          by,
          zoomPrefs.maxScale,
        ),
      )
    }
    host.addEventListener('wheel', onWheel, { passive: false })
    return () => host.removeEventListener('wheel', onWheel)
  }, [mode, zoomPrefs, loaded])

  // The second half of a swap: the incoming page's own geometry, which does
  // not exist at the moment the key is pressed.
  //
  // `recordSwap` (in the keydown effect above) measured the outgoing page and
  // left the rest here, because `PageView`'s decode gate holds the old image
  // on screen until the new one is ready to paint — so the frame this needs
  // arrives one decode after the keypress, and a `ResizeObserver` would not
  // report it either (the element's layout size does not change, only its
  // content). A `load` listener in the capture phase is what does: `load` does
  // not bubble, but it does capture, so one listener on the page area hears
  // whichever of its images settles.
  //
  // Every early return below clears the pending swap rather than leaving it to
  // be applied against a later page. The one that does *not* is the case where
  // the image simply has not arrived yet — that one waits, and its cleanup
  // clears the swap only if a newer one has not already replaced it.
  //
  // biome-ignore lint/correctness/useExhaustiveDependencies: the three `session?.state.*` deps are triggers, not closure reads — they are what "the reader moved" means, and the body reads refs. Removing them, which is what this rule suggests, would leave a swap's geometry never finished.
  useEffect(() => {
    const pending = swapRef.current
    if (!pending) return
    const host = pageAreaRef.current
    // Nothing landed on: a swap into a gap draws `Gap` in place of the page
    // area, and the design's §9 says the view is left untouched and restored
    // when the reader swaps back — which is what dropping the pending swap
    // here achieves, since the swap back has no outgoing page to measure
    // either and so records nothing of its own.
    //
    // `session.state.viewing` equal to the edition being left is a swap that
    // changed nothing — a digit for the edition already on screen. `swap`
    // builds a fresh `ReadingState` either way, so this is the only thing that
    // tells the two apart.
    if (
      !host ||
      !drawnRef.current ||
      session?.state.viewing === pending.edition
    ) {
      swapRef.current = undefined
      return
    }
    const kind: MapKind = zoomPrefs.onSwap === 'reset' ? 'reset' : CONTENT_MAP
    const apply = () => {
      // Superseded by a later swap while this one waited. Reported as done so
      // the listener stands down; the newer swap has its own effect.
      if (swapRef.current !== pending) return true
      // Read here rather than captured when the effect ran. `mode` is not one
      // of this effect's dependencies — it is not what "the reader moved"
      // means — and adding it would be worse than useless: while a swap is
      // waiting this effect leaves a cleanup that clears `swapRef`, and React
      // runs that cleanup *before* the re-run that would read it, so a mode
      // switch would drop the swap rather than redo it. Captured, a switch
      // between single and two-page while the incoming page decodes leaves a
      // `cell` measured under the other mode — and `cell` is which half of the
      // group the page occupies, so a stale one lands the view about half a
      // group out. That does not wash out on its own: every later swap maps
      // the displaced rect through a correct transform and carries the
      // displacement with it, until the reader presses `0`. One deref here.
      const to = drawnRef.current
      // The mode switch was to scroll, or the reader has moved onto a gap:
      // there is no drawn page to carry the view onto. Dropped rather than
      // held, the same answer as the guard above and for the same reason.
      if (!to) {
        swapRef.current = undefined
        return true
      }
      const frame = frameOf(host, to.url)
      if (!frame) return false
      const box = host.getBoundingClientRect()
      // The wheel handler's zero-box guard, for the same reason — and here it
      // also protects the `w === h` invariant, since `fitRect` hands back an
      // unfitted rect for an aspect it cannot use.
      if (box.width <= 0 || box.height <= 0) return false
      swapRef.current = undefined
      setView((rect) =>
        mapView(
          rect,
          pending.from,
          { frame, cell: to.cell, half: to.half },
          box.width / box.height,
          kind,
        ),
      )
      return true
    }
    if (apply()) return
    const onLoad = () => {
      if (apply()) host.removeEventListener('load', onLoad, true)
    }
    host.addEventListener('load', onLoad, true)
    return () => {
      host.removeEventListener('load', onLoad, true)
      if (swapRef.current === pending) swapRef.current = undefined
    }
  }, [
    session?.state.authority,
    session?.state.slot,
    session?.state.viewing,
    zoomPrefs.onSwap,
  ])

  if (!session) return null
  const { work, resolver, reading, state, alignments, editions } = session
  const shown = display(state, resolver)

  // The page-drawing half of `Display`, or `undefined` while showing a gap.
  //
  // Narrowed here rather than by a branch that returns early. The early return
  // was O25: it handed back `Gap` *instead of* the reader's own tree, so
  // `Chrome`, `Settings` and the mode toolbar were never rendered while a gap
  // was showing — the keys fired, the state flipped, and nothing appeared. The
  // note panel was mounted inside that branch during M5c as a second copy,
  // which is the shape that let the credits panel be forgotten as well. After
  // this there is one mount site, so a panel added later is reachable in a gap
  // because there is nowhere else to put it.
  const at = shown.kind === 'at' ? shown : undefined

  // Takes one of `Gap`'s offered moves: `slot` is authority-space (what
  // `nearestPaired` returned), and `takeOffered` is what resolves it into
  // the viewed edition and commits authority there.
  function take(slot: number) {
    setSession((prev) =>
      prev
        ? { ...prev, state: takeOffered(prev.state, slot, prev.resolver) }
        : prev,
    )
  }

  // The filmstrip's click job: real navigation, via `reading.ts`'s
  // `navigate` directly — not `swap`. A tile is a destination the reader
  // chose, not another edition to peek at and return from.
  function goToSlot(slot: number) {
    setSession((prev) =>
      prev ? { ...prev, state: navigate(prev.state, slot) } : prev,
    )
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
    if (!pending || pending.b === undefined || etag === undefined || submitting)
      return
    const { from, to, a, b } = pending
    setSubmitting(true)
    try {
      const fresh = await patchAlignment(work, etag, {
        from,
        to,
        a: spanOf(a),
        b: spanOf(b),
      })
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
        const state =
          bSlot === undefined
            ? prev.state
            : navigate({ ...prev.state, viewing: to }, bSlot)
        // `alignments` alongside the resolver swap — `Sidecar::rewrite`
        // recomputes `quality` on every correction, so leaving this at
        // whatever loaded at session start would show a verdict the
        // document no longer states. See `session.ts`'s `alignmentsOf`.
        return {
          ...prev,
          resolver: nextResolver,
          alignments: alignmentsOf(fresh.json),
          state,
          previous: prev.state.viewing,
        }
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
            prev
              ? {
                  ...prev,
                  resolver: nextResolver,
                  alignments: alignmentsOf(fresh.json),
                }
              : prev,
          )
          setEtag(fresh.etag)
        } catch (rereadErr) {
          console.error(rereadErr)
        }
        setCorrectionNotice(
          'This work changed since it was read — check the pairing below and confirm again.',
        )
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
  async function writeNotes(
    notes: Note[],
    { retry = true } = {},
  ): Promise<void> {
    if (!notesDoc || notesEtag === undefined) return
    setNotesBusy(true)
    try {
      const fresh = await putNotes(work, notesEtag, {
        work: notesDoc.work,
        notes,
      })
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
          setNotesNotice(
            'These notes changed on another device; yours were kept on top.',
          )
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
    setPrefsMap((map) => ({
      ...map,
      [id]: withFieldValue(map[id] ?? (mod.defaults as object), key, raw),
    }))
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

  // The review list is built whatever `display` resolved to. A gap has no slot
  // to draw notes on, but the work still has notes and a reader in a gap is
  // exactly who asks where they are — found by driving this against the real
  // corpus, where pressing `a` on a gap opened nothing at all. Every row in
  // the list already carries its own "not reachable from here" marking, which
  // is the marking this state exists for.
  const allNotes = notesDoc?.notes ?? []
  const entries: ReviewEntry[] =
    panelMode === 'review' ? reviewList(allNotes, shown.edition, resolver) : []

  // While the editor is open every region is shown, whatever `reveal` says: a
  // reader cannot grab a box they cannot see, and an edit mode that hid the
  // things it lets you edit would be a puzzle. `reveal` goes back to governing
  // it the moment the editor closes.
  const notesPlacement = listPlacement(
    annotationsPrefs.notesList,
    viewportWidth,
  )
  const overlayPrefs =
    panelMode === 'edit'
      ? { ...annotationsPrefs, reveal: true }
      : annotationsPrefs

  const slots = allSlots(resolver, shown.edition)
  // `shown.edition` — not `state.viewing` directly, though they're always
  // equal (see `reading.ts`'s `Display`) — because this is the edition the
  // page area is about to draw, which is what an override keyed by edition
  // must line up with.
  const effectiveReading = effectiveDirection(
    reading,
    directionPrefs.override[shown.edition],
  )
  // Same `shown.edition` reasoning as `effectiveReading` above: breaks are
  // stored per edition, keyed by the edition whose slots `groups` is about
  // to pair.
  const creditsPrefs = sliceFor(prefsMap, creditsModule)
  const helpPrefs = sliceFor(prefsMap, helpModule)
  // Read once per render and shared by the skip filter below and the panel at
  // the foot of this component. `Resolver.credits` crosses the wasm boundary
  // and copies (`Array.from(sc.credits(edition))`), so calling it in both
  // places would allocate a second array every render for an answer already in
  // hand — the same redundant-crossing argument `NearestOffer` carries.
  const creditPages = resolver.credits(shown.edition)
  // Those page indices, converted to this edition's slots exactly once per
  // render. Empty — and so free — whenever `skip` is off or the sidecar names
  // no credit pages, which is every edition in the corpus today.
  // `at?.slot` is the exemption: a reader who took a credit page from the
  // panel is standing on it deliberately, so that one page is drawn while the
  // rest stay set aside. See `skippedApartFrom` for what went wrong without
  // it.
  const skippedSlots = creditsPrefs.skip
    ? skippedApartFrom(creditSlotsIn(slots, creditPages), slots, at?.slot)
    : EMPTY_SLOTS
  // One grouping, three surfaces: this array is what the page area draws,
  // what `groupedRef` gives the arrow keys, and what `Chrome` hands the
  // filmstrip. Passing `skippedSlots` into `groups` rather than filtering its
  // result is what makes "out of the reading flow" mean all three *and* work
  // in `two` mode — see `groups`' own doc for the measurement that settled
  // that.
  const grouped = groups(
    slots,
    mode,
    effectiveReading,
    regroupPrefs.breaks[shown.edition] ?? [],
    skippedSlots,
  )
  // Assigned during render rather than in an effect: the keydown handler must
  // never step against a grouping older than what is on screen, and an effect
  // would leave exactly one frame where it could. Writing a ref during render
  // is safe here because it is derived state, not a subscription — the value
  // is recomputed from props and state every render, so a discarded render
  // simply writes the same thing the retained one will.
  groupedRef.current = grouped
  creditsPrefsRef.current = creditsPrefs
  // The reading direction the *arrows* must follow, for the keydown listener —
  // which subscribes once at load and cannot see `effectiveReading`, computed
  // above at render from the sidecar and this reader's per-edition override.
  // Assigned during render like `creditsPrefsRef` and for the same reason: it
  // is derived state, recomputed every render, so a discarded render writes
  // the same value the retained one will.
  readingRef.current = effectiveReading
  zoomPrefsRef.current = zoomPrefs
  helpPrefsRef.current = helpPrefs
  // `-1` in a gap: there is no group of the viewed edition the reader is
  // standing on, and `-1` is already `Filmstrip`'s "mark nothing" — the same
  // value correction mode passes, for the same reason.
  // No `Math.max(…, 0)` clamp. That was harmless while every slot belonged to
  // some group, and became a trap once `skip` could remove one: `findIndex`
  // answering `-1` was clamped to `0`, so the page area drew the volume's
  // first group — the cover — while the readout correctly reported where the
  // reader was. `skippedApartFrom` is what keeps `-1` from arising at all;
  // this draws nothing rather than something wrong if it ever does.
  const currentIndex = at
    ? grouped.findIndex((g) => g.slots.includes(at.slot))
    : -1
  const current = currentIndex >= 0 ? grouped[currentIndex] : undefined

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
  const correctionGroups =
    pending && correctionSlots ? correctionTiles(correctionSlots) : undefined
  // Where the alignment currently resolves `pending.a` to, in `pending.to` —
  // the tile a reader disagrees with when they open correction mode, so it
  // has to be visible to disagree with it (see `Filmstrip`'s own doc on
  // `correcting`). `undefined` when `pending.a` currently pairs with nothing
  // in `to`, a real state `Resolver.resolve` can answer and this must not
  // crash on rather than a bug.
  const landedAtCounterpart =
    pending && resolver.resolve(pending.from, pending.to, pending.a)
  const pairedIndex = landedAtCounterpart
    ? resolver.indexOf(pending.to, landedAtCounterpart)
    : undefined
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
  // Empty in a gap rather than resolved against a slot that does not exist.
  // The note *count* readout still reports the work's total beside it, so a
  // reader sees "no notes here of 7" rather than the readout vanishing.
  const here = at ? notesHere(notesForHere, at.edition, at.slot, resolver) : []
  // The note on show is drawn by `pending` — dashed, and following the pointer
  // mid-drag — so drawing it a second time as an ordinary region would leave
  // two boxes on one note, one of them a frame behind.
  const hereBehind = here.filter((h) => h.note.id !== currentId)
  hereRef.current = here
  // Scroll mode draws every group, so it needs every slot's notes at once —
  // one resolve per note rather than one per page. Built only in that mode:
  // single and two-page draw one group and `here` above already answers it.
  //
  // And only on a page. The gap branch used to return before this line; now
  // that it does not, `at` is what keeps a gap from paying one resolve per
  // note on every render to build a map nothing consumes — the page area
  // draws `Gap` there, and no group is mounted to read it.
  const scrolledNotes =
    at && mode === 'scroll'
      ? notesBySlot(notesForHere, at.edition, resolver)
      : new Map<number, NoteHere[]>()

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

  // Where the readouts report from. In a gap that is the authority slot the
  // gap was found at — a real position in the work, which only the *viewed*
  // edition has no page for — and `name` is what puts the edition in front of
  // the position line so the change of scale is never silent. `Gap.tsx` says
  // the same thing in words directly above the row.
  const reportFrom =
    shown.kind === 'at'
      ? { edition: shown.edition, slot: shown.slot, name: undefined }
      : {
          edition: shown.from.edition,
          slot: shown.from.slot,
          name: shown.from.edition,
        }
  // Same `?? slot + 1` fallback the deleted gap branch used for
  // `nearestPaired`'s bound: an edition the resolver cannot count still
  // renders a line rather than `NaN%`.
  const reportTotal =
    resolver.slotCount(reportFrom.edition) ?? reportFrom.slot + 1
  const chapter: Chapter | undefined = resolver.chapterAt(
    reportFrom.edition,
    reportFrom.slot,
  )
  const positionText = positionLine({
    slot: reportFrom.slot,
    total: reportTotal,
    page: resolver.slotAt(reportFrom.edition, reportFrom.slot)!.page + 1,
    edition: reportFrom.name,
  })
  const chapterText = chapterReadout(chapter, alignments, hub)
  // `gapHealth` rather than `healthReadout` in a gap: it reports the pair
  // `Gap.tsx` already prints a verdict sentence for, so the view and the row
  // beneath it cannot contradict each other. See `gapHealth`'s own doc.
  const healthText =
    shown.kind === 'at'
      ? healthReadout(alignments, hub, shown.edition)
      : gapHealth(alignments, shown.from.edition, shown.edition)
  const slotsLeft = Math.max(reportTotal - (reportFrom.slot + 1), 0)
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
  const pageAt = (slot: number) =>
    pageUrl(work, shown.edition, slots[slot].page)
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

  // The page the swap measures against, assigned during render like
  // `groupedRef` and for the same reason — see the ref's own doc for what its
  // absence means. Declared here rather than beside `grouped` above because
  // `pageAt` is a `const` and would still be in its temporal dead zone there.
  //
  // `cellOf` is asked only in `two` mode: one image fills the group in the
  // other two, which is `'only'` without a group to consult (`Cell`'s doc).
  //
  // `half` comes off the same `slots` entry `pageAt` and `altAt` already read,
  // and for the same reason each of those exists: a joined spread's two slots
  // share one `page`, so the page index alone cannot tell them apart. `altAt`
  // says "page 5 R" out loud from this field; this hands the same fact to the
  // swap's arithmetic, which needs it to land on the R half rather than on the
  // middle of the spread. `undefined` for every ordinary slot, which is what
  // `Drawn.half` expects there.
  drawnRef.current =
    at && current && mode !== 'scroll'
      ? {
          url: pageAt(at.slot),
          cell:
            mode === 'two'
              ? cellOf(current, effectiveReading, at.slot)
              : 'only',
          half: slots[at.slot].half,
        }
      : undefined

  /**
   * Which edge of its own element a slot's artwork is pulled to — `fit.ts`'s
   * `ArtworkHug`, asked by slot rather than by cell.
   *
   * One answer for the three things that have to agree about it: the two
   * `PageView`s `renderGroup` draws, the marker overlay that measures one of
   * them, and `pointOn`'s drag arithmetic. Written out at each of the three, a
   * copy that fell out of step would draw the page one way and measure it
   * another — a region landing about 180px from where the reader swept it out,
   * `paintedBox` having the measurement — and nothing in `web/` could catch
   * that, since none of the three is reachable from a test.
   *
   * `cellOf` is the rule and is asked rather than repeated, for the reason its
   * own doc gives: the comparison written here would put a `reading === 'rtl'`
   * test back into this file. This is only the translation from a cell to an
   * edge, and the direction is the thing to get right — the *first* cell hugs
   * its **right** edge, because that is the side the seam is on. `ArtworkHug`
   * has the whole naming argument.
   *
   * `'centre'` everywhere else — single and scroll mode, a joined spread or a
   * solo slot (`cellOf`'s `'only'`), and a slot with no group on screen at
   * all. Nothing hugs in any of those, so nothing is off-centre in them
   * either.
   */
  const hugFor = (group: Group | undefined, slot: number): ArtworkHug => {
    if (mode !== 'two' || !group) return 'centre'
    const cell = cellOf(group, effectiveReading, slot)
    return cell === 'first'
      ? 'right-edge'
      : cell === 'second'
        ? 'left-edge'
        : 'centre'
  }
  /** The hug for the slot the reader is on, which is the one the overlay draws over. */
  const shownHug = at ? hugFor(current, at.slot) : 'centre'

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
  function draftImage(
    target: EventTarget | null,
  ): HTMLImageElement | undefined {
    if (!draft || panelMode !== 'edit' || draft.edition !== shown.edition)
      return undefined
    if (!(target instanceof HTMLImageElement)) return undefined
    return target.src.endsWith(pageAt(draft.slot)) ? target : undefined
  }

  /**
   * Where a pointer event falls, in fractions of the page it is over.
   *
   * **The hug is what `paintedBox` cannot infer from the two rects.**
   * *2026-08-08, closing what the previous version of this comment recorded
   * and did not fix.* Left to its default, that function splits the letterbox
   * evenly, which is what `object-fit: contain` does under `object-position`'s
   * own `50% 50%`. The two cells of a real pair pass a hug instead, so their
   * art sits flush against the seam — and a drag measured as though it were
   * centred lands the whole rectangle about 180px sideways of where it was
   * swept out, which is half of the 359px seam that closing it moved
   * (`paintedBox` has both measurements). Nothing changes where the fit is
   * width-bound and there is no horizontal letterbox left to distribute.
   *
   * `el` always comes from `draftImage`, which returns nothing without a
   * draft, so the `'centre'` below is unreachable rather than a third case.
   * Written out rather than asserted on: the cost of being wrong here is a
   * region 180px from where it was drawn, not a crash, and this file would
   * rather draw the centred page than throw at a reader mid-drag.
   *
   * `PageAnnotations` measures the same page through the same `paintedBox`, so
   * the rectangle a reader sweeps out and the marker drawn over it afterwards
   * cannot disagree about where the page is.
   */
  function pointOn(el: HTMLImageElement, clientX: number, clientY: number) {
    const rect = el.getBoundingClientRect()
    const painted = paintedBox(
      { w: el.naturalWidth, h: el.naturalHeight },
      { w: rect.width, h: rect.height },
      draft ? hugFor(current, draft.slot) : 'centre',
    )
    return pointInSlot(
      { x: clientX - rect.left, y: clientY - rect.top },
      painted,
    )
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
    // Pan takes the drag on a press that is on the drawn page, with the editor
    // closed or the modifier held. `gestureFor`'s four cases are untouched: it
    // is described in its own doc as "the most-changed rule of this milestone
    // [M5c] and lived where nothing could test it", and rewriting it to add a
    // fifth case would spend exactly the testability that comment records.
    //
    // **`drawnRef.current`** makes this inert wherever the three zoom keys
    // are, for the reason the `ZOOM_IN_INTENT` case above states in full — and
    // it subsumes the `mode !== 'scroll'` test that used to be written here,
    // since `drawnRef` is already `undefined` in scroll mode. Without it, a
    // drag anywhere over a gap — including off one of `Gap`'s own buttons —
    // pans a rect nobody is drawing: `Gap` renders *inside* the page area, so
    // this handler is on it.
    //
    // **The press must not be on a control.** "The editor is closed" is not
    // the same set as "the pointer is over the page", and the guard above does
    // not close the difference: the things that share the page area *with* a
    // drawn page are `NoteList` in its `side` placement and the note tabs
    // `PageAnnotations` draws (`pointerEvents: 'auto'` on those, deliberately,
    // so they can be clicked). Neither stops propagation, so without this test
    // a sweep to select text in the note list pans the manga underneath it,
    // and so does a drag begun on a tab.
    //
    // **This was `event.target instanceof HTMLImageElement` until 2026-08-08**,
    // which excluded the same controls by being narrower still: it panned only
    // on the drawn page. The comment here argued that was the narrowest test
    // true of the page and false of a button, and it was — but it also made
    // the letterbox around the page dead to a drag, and a reader trying it
    // said so. That cost was stated in this comment and judged acceptable on
    // the reasoning that the letterbox shrinks as the zoom grows; what the
    // reasoning missed is that a reader reaches for the nearest grey to drag
    // from, and being ignored there reads as the pan being broken rather than
    // as a boundary.
    //
    // `isInteractivePress` is the ancestor-aware sibling of the
    // `isInteractiveTarget` this file already uses for keys, and the ancestry
    // is the whole point: a press on a note-list entry lands on the *text*, so
    // the keydown rule — which asks only about the target — answers `false`
    // for exactly the case the image test was protecting against. Excluding
    // controls rather than requiring the page also means the next component
    // added inside the page area is included by default and has to opt out,
    // which is the right way round for a background gesture.
    //
    // `stopPropagation` on every child was the other candidate, and lost
    // again: it would put "and this is why the manga does not pan" in
    // `NoteList.tsx` and `Annotations.tsx`, where nobody looking for the pan
    // rule would find it.
    //
    // The one press this hands back that it used to keep is the modifier held
    // with the editor open, on something that is not a page image; it now
    // falls through to `gestureFor`, which reads it as `close` — rule 1 of
    // that function's own list ("looking away is how a person stops writing
    // one"), not a new behaviour invented here. With the editor *closed*
    // nothing is lost by taking the drag ahead of `gestureFor` at all, since
    // it answers `{ kind: 'ignore' }` for every press in that state.
    //
    // No `preventDefault` here, deliberately, though the region drag below
    // calls one to stop the browser's text selection fighting the rectangle.
    // A prevented `pointerdown` also stops focus moving, and this branch runs
    // on every press on the page rather than only while the editor is open: a
    // reader who clicked a mode button and then dragged the page would leave
    // focus on that button, where `isInteractiveTarget` stands the keydown
    // handler down and the arrow keys stop working.
    const panning =
      zoomPrefs.enabled &&
      drawnRef.current !== undefined &&
      !isInteractivePress(event.target) &&
      (panelMode !== 'edit' || event.shiftKey)
    if (panning) {
      panRef.current = { x: event.clientX, y: event.clientY }
      return
    }
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
      const held =
        edits[gesture.noteId] ??
        here.find((h) => h.note.id === gesture.noteId)?.note
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
    const from = panRef.current
    if (from) {
      const box = event.currentTarget.getBoundingClientRect()
      // The wheel handler's zero-box guard, for the same reason.
      if (box.width <= 0 || box.height <= 0) return
      setView((rect) => {
        // A drag moves the page under the viewport, so the rect moves the
        // other way: dragging right shows what was to the left. The step is
        // scaled by `rect.w`/`rect.h` because the rect is what one page area's
        // width is showing — at 4x, a pointer crossing the box has crossed a
        // quarter of the group.
        const next = panBy(
          rect,
          (-(event.clientX - from.x) / box.width) * rect.w,
          (-(event.clientY - from.y) / box.height) * rect.h,
        )
        // The rect back unchanged where the pan had nowhere to go, so React's
        // own `Object.is` bail-out applies. `panBy` says a drag at fit-whole
        // is "a no-op instead of a jitter", and it is the *rect* that is one:
        // without this the no-op still allocates a fresh, equal rect on every
        // pointer move, and every one of those re-renders this component and
        // re-runs `PageAnnotations`' measurement. Fit-whole is what a reader
        // who never zooms is in the whole time, so that is the common case
        // rather than an edge of one. Only `x` and `y` are compared because
        // `panBy` is the only writer here and it never touches the sizes.
        return next.x === rect.x && next.y === rect.y ? rect : next
      })
      panRef.current = { x: event.clientX, y: event.clientY }
      return
    }
    const started = dragRef.current
    if (!started) return
    setDragging(regionUnderPointer(started, event))
  }

  function endRegion(event: React.PointerEvent) {
    panRef.current = null
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
      const next = {
        ...map,
        [id]: { ...note, region: anchor, updated: new Date().toISOString() },
      }
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
        <PageView
          key={key}
          src={pageAt(slot)}
          alt={altAt(slot)}
          buffered={buffered}
          decodeQueue={swapQueue}
          fit={fitPrefs}
        />
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
        <div
          style={{
            flex: 1,
            minWidth: 0,
            height: '100%',
            ...seamStyle(fitPrefs, 'first'),
          }}
        >
          {/*
            The hug is `hugFor`, not a rule written out here, so the page this
            draws and the geometry that measures it cannot disagree — that
            function's doc has the naming argument (`right-edge` on the *left*
            cell is not backwards), why only a real pair hugs, and why screen
            position rather than reading direction decides it.

            It asks `cellOf` about `group`, which is `current` here: `two` mode
            renders exactly one group, the `mode !== 'two'` return above having
            taken scroll mode's many-group case out of this branch.
          */}
          <PageView
            src={pageAt(first)}
            alt={altAt(first)}
            buffered={buffered}
            decodeQueue={swapQueue}
            fit={fitPrefs}
            hug={hugFor(group, first)}
          />
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
              ? {
                  flex: '0 0 0',
                  width: 0,
                  minWidth: 0,
                  height: '100%',
                  overflow: 'hidden',
                }
              : {
                  flex: 1,
                  minWidth: 0,
                  height: '100%',
                  ...seamStyle(fitPrefs, 'second'),
                }
          }
          aria-hidden={wide || undefined}
        >
          <PageView
            src={pageAt(second)}
            alt={altAt(second)}
            buffered={buffered}
            decodeQueue={swapQueue}
            fit={fitPrefs}
            hug={hugFor(group, second)}
          />
        </div>
      </div>
    )
  }

  // The gap's offered moves, and where the strip should sit. Computed only in
  // a gap; the scan is bounded by the authority's slot count so a wholly
  // unpaired edition costs one scan rather than one per render.
  const gapMoves =
    shown.kind === 'gap'
      ? nearestPaired(
          state,
          resolver,
          resolver.slotCount(state.authority) ?? state.slot + 1,
        )
      : undefined
  // `nearestPaired` already resolved each offer against the edition being
  // looked at while finding it (see `nearest.ts`'s `NearestOffer`), so this is
  // only the display conversion — not a second `resolveOffered` call repeating
  // a resolution the scan already did. `Gap`'s `page` props are display-only
  // (see their doc there), so the +1 happens right here, at the boundary
  // handing a value to something a person reads — the same convention `altAt`
  // follows.
  const toMove = (offer: NearestOffer | undefined) =>
    offer && { slot: offer.slot, page: offer.page + 1 }
  // The tile the strip centres on while showing a gap: the nearest page that
  // exists, which is the same page `Gap`'s own button offers. `before` first —
  // a reader who has just walked forward into a gap came from behind it.
  const stripAnchor = gapMoves && (gapMoves.before?.at ?? gapMoves.after?.at)

  return (
    // `position: relative` is what `Chrome` positions itself against —
    // `position: absolute` removes it from this flex column's layout flow
    // entirely, so it can overlay the toolbar and the page area below
    // without either one changing size when it mounts or unmounts.
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        position: 'relative',
      }}
    >
      <div
        style={{
          flex: '0 0 auto',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: 8,
        }}
      >
        {(['single', 'two', 'scroll'] as const).map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => setMode(m)}
            aria-pressed={m === mode}
          >
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
          panRef.current = null
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
        {at && mode !== 'scroll' && (
          <PageAnnotations
            // Remounted when the mode changes, so the observer never keeps
            // watching an image the mode switch replaced — see
            // `PageAnnotations`' own doc.
            key={mode}
            match={pageAt(at.slot)}
            here={hereBehind}
            prefs={overlayPrefs}
            // Where the artwork sits inside the `<img>` this measures, which
            // is not the middle of it for either half of a pair — the same
            // value the `PageView` underneath is drawing with, from the same
            // `hugFor`.
            hug={shownHug}
            // The tab goes in the margin the hug leaves. `right-edge` is the
            // left cell of a real pair and only that: its artwork is against
            // the seam, so the outside is the only margin it has. Everything
            // else keeps the default and is unchanged. Derived from the hug
            // rather than asking `cellOf` a second time, so the two cannot
            // come to different conclusions about which cell this is.
            tabSide={shownHug === 'right-edge' ? 'left' : 'right'}
            // The live rectangle while dragging, and the settled one after the
            // pointer is released but before Save — so a reader can see what
            // they drew, and see it again when they reopen the note. Only
            // while the editor is open on *this* slot.
            pending={
              panelMode === 'edit' &&
              draft?.edition === shown.edition &&
              draft.slot === at.slot
                ? (dragging ?? draft.region)
                : undefined
            }
            drawing={dragging !== undefined}
            pendingBody={draft?.body}
            pendingColour={draft?.color}
            // The re-measure trigger, not a value the overlay reads — see
            // `PageAnnotations`' own comment on the dependency. Only here: the
            // scroll-mode overlays below sit under no `Viewport` and cannot go
            // stale for this reason.
            version={view}
            onOpen={openNote}
          />
        )}
        {/*
          Beside the page, inside the page area so it sits in the margin the
          page leaves rather than over the toolbar. Only while revealed: the
          list is a way to *read* the notes, and `r` is the key that asks for
          that.
        */}
        {at &&
          annotationsPrefs.reveal &&
          notesPlacement === 'side' &&
          mode !== 'scroll' && (
            <NoteList here={here} placement="side" onOpen={openNote} />
          )}
        {shown.kind === 'gap' ? (
          <Gap
            edition={shown.edition}
            from={{
              edition: shown.from.edition,
              page:
                resolver.slotAt(shown.from.edition, shown.from.slot)!.page + 1,
            }}
            verdict={verdictFor(alignments, shown.from.edition, shown.edition)}
            before={toMove(gapMoves?.before)}
            after={toMove(gapMoves?.after)}
            onTake={take}
          />
        ) : mode === 'scroll' ? (
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
                  here={(scrolledNotes.get(g.slots[0]) ?? []).filter(
                    (h) => h.note.id !== currentId,
                  )}
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
          current && (
            // Scroll mode's branch above is untouched: it mounts every group
            // at once and keeps fit-whole, which is the spec's scope decision
            // rather than an oversight. `WHOLE` while the module is disabled
            // rather than unmounting the wrapper, so turning zoom off does not
            // change what React reconciles here — a changed element type at
            // this position is what `cellsFor`'s doc records forcing a remount
            // and throwing away `PageView`'s decode gate.
            <Viewport rect={zoomPrefs.enabled ? view : WHOLE}>
              {renderGroup(current, 0)}
            </Viewport>
          )
        )}
      </div>
      {at && annotationsPrefs.reveal && notesPlacement === 'below' && (
        <NoteList here={here} placement="below" onOpen={openNote} />
      )}
      <Chrome
        visible={chromeVisible}
        // The strip runs the way the pages do — `stepFor`'s rule, applied to
        // layout rather than to a keypress. `effectiveReading` is the viewed
        // edition's, override included, which is the same value the page area
        // pairs its cells with.
        reading={effectiveReading}
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
        anchor={stripAnchor}
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
              Pick the <strong>{pending.to}</strong> page that matches{' '}
              <strong>{pending.from}</strong> {describePosition(pending.a)}.{' '}
              <em>Escape</em> cancels without writing anything.
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
                Pair <strong>{pending.from}</strong>{' '}
                {describePosition(pending.a)} with <strong>{pending.to}</strong>{' '}
                {describePosition(pending.b)}?
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
              {correctionNotice && (
                <p style={{ margin: 0, opacity: 0.85 }}>{correctionNotice}</p>
              )}
              {/* Both disabled while the write is on the wire — see
                  `submitting`'s doc. Cancel especially: it is the control
                  that most looks like it undoes the write, and it is the one
                  that cannot. */}
              <div style={{ display: 'flex', gap: 8 }}>
                <button
                  type="button"
                  disabled={submitting}
                  onClick={() => void confirmCorrection()}
                >
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
      {/*
        One panel, both states. The gap branch used to mount a second copy
        that forced `mode` to `'review'` and stubbed every write handler; this
        is the survivor, and `'edit'` is unreachable in a gap by two guards
        rather than by that forcing.

        **They are complementary, not redundant — neither alone is enough**,
        because they cover disjoint routes. `NEW_INTENT` returns early on
        `shown.kind !== 'at'`, which stops an editor being *opened* in a gap;
        `panelModeRef.current === 'edit'` in the dispatch guard stops an
        already-open one being *carried* into one. Remove the second and this
        is reachable: open an editor on a page, let a refused save drop focus
        to `<body>` (the state the dispatch guard's own comment records as
        measured, not hypothetical), then press Space. `toggle` calls `swap`,
        which changes only `viewing`, and on a slot the viewed edition has no
        counterpart for that is a gap — with `mode='edit'` rendering beside
        `Gap`, on a draft anchored to an edition no longer on screen, its
        region drag dead because `draftImage` requires
        `draft.edition === shown.edition`. Remove the first instead and `n`
        opens an editor in a gap directly, which the second cannot see because
        `panelModeRef.current` is still `undefined` when `n` is pressed.

        A change that removes either has to decide what a note anchored to no
        slot means before this line is safe again.
      */}
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
          editCurrent((current) => ({
            ...current,
            color: colour,
            updated: new Date().toISOString(),
          }))
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
      <CreditsPanel
        visible={creditsVisible}
        work={work}
        edition={shown.edition}
        pages={creditPages}
        skipping={creditsPrefs.skip}
        onGo={(page) => {
          // The panel deals in pages, the reader in slots. Converted through
          // `creditSlotsIn` rather than a `findIndex` of its own, so this file
          // holds no second page-to-slot rule: that function is the one place
          // the two spaces meet, which is the whole point of replacing the
          // expansion `creditSlots` used to do here. A joined spread yields
          // both its slots and the first is where to land, since its two slots
          // are one image and `groups` puts that one first.
          const [slot] = creditSlotsIn(slots, [page])
          if (slot === undefined) return
          goToSlot(slot)
          setCreditsVisible(false)
        }}
        onClose={() => setCreditsVisible(false)}
      />
      {/* `enabledModules` and not `registry.all()`: `Help` lists exactly the
          modules it is handed, and handing it all of them would advertise the
          keys of a module the reader has switched off. The filter is the
          caller's job by design — see that function's doc for what `enabled`
          means for a module that has no such preference, and for the one
          module whose off switch it cannot see. */}
      <Help
        visible={helpVisible}
        modules={enabledModules(registry.all(), prefsMap)}
        reading={effectiveReading}
        onClose={() => setHelpVisible(false)}
      />
    </div>
  )
}

export default App
