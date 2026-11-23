import { useEffect, useRef, useState } from 'react'
import { flushSync } from 'react-dom'
import {
  Unauthenticated,
  WorkMissing,
  WorkNotFound,
  getNotes,
  getProgress,
  getSidecarWithEtag,
  OPENING_SAVE_WAIT_MS,
  pageUrl,
  patchAlignment,
  progressSavedWithin,
  putNotes,
  putProgress,
  type StoredProgress,
} from './api'
import {
  alignedBreaksCached,
  emptyBreaksCache,
  pairingBreaks,
  type DerivedBreaksCache,
} from './alignedPairs'
import { PageAnnotations } from './Annotations'
import { type Destination, destination } from './back'
import { pageBoxLongEdge, requestedLongEdge } from './pageSize'
import { Borrowed } from './Borrowed'
import { Chrome } from './Chrome'
import {
  applyLanded,
  correctionView,
  counterpartEdition,
  counterpartPick,
  submitCorrection,
  type CorrectionPorts,
  type Pending,
} from './correction'
import { CorrectionBanner } from './CorrectionBanner'
import { createCohort, type Cohort } from './cohort'
import { decodeTo } from './decode'
import { debounce, type Debounced } from './debounce'
import {
  barsMounted,
  focusStop,
  ringAt,
  ringSizes,
  useKeyboardBars,
} from './focus'
import {
  canRedo,
  canUndo,
  emptyHistory,
  record,
  redo,
  stepHistory,
  undo,
  type History,
} from './history'
import { Gap } from './Gap'
import { gestureOfPress, type Press } from './gestures'
import { GroupView } from './GroupView'
import {
  useConstant,
  useDecodeQueue,
  useDrawnSrcs,
  useFlushOnHide,
  useLatest,
  usePrefsMap,
  useProgressWrite,
  useReadingClock,
  useScrollToCurrent,
  useWindowWidth,
} from './hooks'
import { Account } from './Account'
import { useGate } from './gate'
import { CreditsPanel } from './CreditsPanel'
import { Help } from './Help'
import { TurnButtons } from './TurnButtons'
import { Compared } from './Compared'
import { type Layout, PAGE_LAYOUTS, PAGE_LAYOUTS_COMPARED } from './layouts'
import { PageView } from './PageView'
import { controlKind, inPanel, intentOf, isInteractivePress } from './keys'
import { keyPlan } from './keyplan'
import { messageOf } from './message'
import {
  changeSlice,
  fieldChanged,
  sliceFor,
  sliceReset,
  withSlice,
} from './module'
import {
  annotationsModule,
  NEW_INTENT,
  REDO_INTENT,
  REVEAL_INTENT,
  REVIEW_INTENT,
  toggleReveal,
  UNDO_INTENT,
} from './modules/annotations'
import { borrowModule, borrowNote, withBorrow } from './modules/borrow'
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
  flipped,
  stepFor,
  TOGGLE_INTENT,
  type Direction,
} from './modules/direction'
import { fitModule, hugForCell, type ArtworkHug } from './modules/fit'
import {
  enabledModules,
  helpModule,
  TOGGLE_INTENT as HELP_TOGGLE_INTENT,
  type HelpPrefs,
} from './modules/help'
import { motionModule } from './modules/motion'
import { prefetchModule, prefetchTargets, superseded } from './modules/prefetch'
import { readoutLines, readoutsModule } from './modules/readouts'
import { layoutModule } from './modules/layout'
import { registry } from './modules/registry'
import {
  breaksFor,
  pairStartBefore,
  realign,
  regroupModule,
  TOGGLE_INTENT as REGROUP_TOGGLE_INTENT,
  withBreaks,
} from './modules/regroup'
import { TOGGLE_INTENT as SETTINGS_TOGGLE_INTENT } from './modules/settings'
import {
  IN_INTENT as ZOOM_IN_INTENT,
  OUT_INTENT as ZOOM_OUT_INTENT,
  panStep,
  swapMapKind,
  WHOLE_INTENT as ZOOM_WHOLE_INTENT,
  wheelStep,
  zoomable,
  zoomModule,
  zoomStep,
  type ZoomPrefs,
} from './modules/zoom'
import { nearestPaired, type NearestOffer } from './nearest'
import {
  afterDelete,
  anchorFrom,
  anchorNote,
  changeShown,
  editsToWrite,
  gestureFor,
  GRAB_SLOP,
  grabNote,
  listPlacement,
  newNote,
  NOTHING_OPEN,
  nothingOpen,
  notesBySlot,
  notesHere,
  openNote as openNoteInDraft,
  paintedBox,
  pointInSlot,
  pressTakesPan,
  regionUnderDrag,
  reviewList,
  sameNotes,
  shownNote,
  toggleReview,
  visibleNotes,
  withoutNote,
  type DragStart,
  type Draft,
  type Note,
  type NoteHere,
  type NotesDoc,
  type Region,
  type ReviewEntry,
} from './notes'
import {
  saveNotes as saveNotesDoc,
  writeGate,
  type NotesPorts,
} from './notesWrite'
import { NoteList } from './NoteList'
import { NotePanel } from './NotePanel'
import { work as workDetail, type WorkDetail } from './library'
import { cellOf, groups, type Group, type Mode } from './pairing'
import { loadPrefs, resetPrefs, savePrefs } from './prefs'
import { display, navigate, swap } from './reading'
import { labelOf, readerName, titleLines } from './readerTitle'
import { ReaderFailed, type LoadFailure } from './ReaderFailed'
import { ReaderHeading } from './ReaderHeading'
import { ringStep } from './ring'
import { Settings } from './Settings'
import { allSlots, openWork, type Session, type StartupPorts } from './startup'
import {
  carrySwap,
  frameOf,
  swapStep,
  type CarriedSwap,
  type DrawnPage,
} from './swapView'
// A test reaches this file only with `vi.mock('../src/resolver')`, as
// `App.test.tsx` does: `wasmResolver` imports `resolver.ts` as a value, and
// that module's `taiyaku-wasm` import only resolves once
// `./scripts/build-wasm.sh` has generated the gitignored
// `crates/taiyaku-wasm/pkg/`. A test importing App.tsx without that mock
// would pass locally (after that script has run) and break CI or a fresh
// clone (where it hasn't) — see docs/ci.md's `web` job notes and the global
// no-resolver-in-tests constraint.
import { wasmResolver } from './resolver'
import {
  applyIntent,
  takeOffered,
  verdictFor,
  type ReadingIntent,
} from './session'
import { cryptoRandom, ulid } from './ulid'
import { useDocumentTitle } from './title'
import { Viewport } from './Viewport'
import { WHOLE, clampRect, isWhole, mapView, type Rect } from './viewport'

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

/** One empty set rather than a fresh one per render, so nothing skipped stays cheap and referentially stable. */
const EMPTY_SLOTS: Set<number> = new Set()

/**
 * The width `auto` note-list placement assumes where there is no window to
 * measure. A desktop width, so the fallback is the layout a reader on a real
 * screen would get rather than the narrowest one.
 */
const NO_WINDOW_WIDTH = 1280

/**
 * How often the reading-time estimate re-renders. 30s rather than every
 * second: `modules/readouts.ts`'s `timeLine` never shows seconds, so anything
 * finer would re-render for a change nobody could see.
 */
const READING_TICK_MS = 30_000

/**
 * The two calls `notesWrite.ts` makes, bound once at module level rather than
 * rebuilt per render: the object's identity never matters to `saveNotes`, but
 * allocating one per render for a function called on a debounce would be
 * noise in every profile of the editor.
 */
const NOTES_PORTS: NotesPorts = { put: putNotes, get: getNotes }

const CORRECTION_PORTS: CorrectionPorts = {
  patch: patchAlignment,
  sidecar: getSidecarWithEtag,
  resolve: wasmResolver,
}

/**
 * The four reads a load makes, bound once at module level — the same reasoning
 * `NOTES_PORTS` above carries, applied to a function called exactly once.
 * `resolve` is `wasmResolver`, which `startup.ts` takes as a port rather than
 * importing so that its own tests can run at all (Rule 1: nothing but this
 * file value-imports `resolver.ts`).
 */
const STARTUP_PORTS: StartupPorts = {
  sidecar: getSidecarWithEtag,
  resolve: wasmResolver,
  // After every progress save still out, for as long as `OPENING_SAVE_WAIT_MS`
  // allows: a reader left and reopened at once (its series page, then Back)
  // flushed its place as it left, and a read that overtook that save would
  // reopen at the place before it.
  progress: async (slug) => {
    await progressSavedWithin(OPENING_SAVE_WAIT_MS)
    return getProgress(slug)
  },
  notes: getNotes,
}

/**
 * The three reading modes, over the state `keys.ts`'s intents drive.
 *
 * `Space`, digits and arrows reach `session.ts`'s `applyIntent` via the
 * keydown listener below, which calls `swap` (for `toggle`/`view`) or
 * `navigate` (for `step`) — never both for one keypress. So `viewing` really
 * can diverge from `authority`, and `display` really can report `'gap'`:
 * landing, after a swap, on a slot with no counterpart in the edition just
 * swapped to. That state is drawn in place of the page area rather than by an
 * early return out of `App` (that was O25) — by `Borrowed` where `withBorrow`
 * below found an edition to lend the page, and by `Gap` where it did not or
 * where the reader has switched `borrow` off. Neither has a slot to draw or a
 * group to belong to, but `Chrome`, `Settings` and the note panel below still
 * mount, so `at` (below) is what the rest of `App` narrows on wherever a slot
 * is actually needed.
 */
export interface AppProps {
  /** The slug in the URL. `<App key={work}>` is what makes a change remount. */
  work: string
  /** The way out (`back.ts`): `L`, the top bar's button and the failed load's take it. */
  onLeave: () => void
  /** What the way out is called; the library's name where none is given. */
  leaveTo?: Destination
  /**
   * Handed the series the library's answer for the work names, `null` for
   * none, as that answer lands, for the way out to go to (O431); never
   * before it lands or where it fails, when the series is not known.
   */
  onSeries?: (series: WorkDetail['series']) => void
  /**
   * Whether "Back from reading" was "Series page" when the route opened this
   * reader (O431), for the help's `L` row to say; "Series page" where none
   * is given.
   */
  putsSeriesUnder?: boolean
}

function App({
  work,
  onLeave,
  leaveTo = destination('/'),
  onSeries,
  putsSeriesUnder = true,
}: AppProps) {
  const [session, setSession] = useState<Session>()
  // O176: what the load threw, when it threw anything but a 401, as
  // `ReaderFailed.tsx`'s `LoadFailure` — the slug for a `WorkNotFound` or a
  // `WorkMissing`, and for anything else the sentence `messageOf` makes. No
  // `session` follows it, because `Routes.tsx` keys this component by `work`
  // and so no instance has its `work` changed under it. `ReaderFailed` draws
  // it, from beside the `if (!session) return null` far below: the first
  // place a second render path can sit with no hook after it.
  const [failure, setFailure] = useState<LoadFailure>()
  // The part of the drawn group the reader is looking at. `WHOLE` is
  // fit-whole, which is what scroll mode does — see the `Viewport` wrapper
  // below, which is only mounted for the two modes that draw one group into a
  // fixed box.
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

  // The top bar's title, edition labels and series link (spec 2026-09-17
  // §8.3): the library's, with metadata edits applied, which the sidecar's
  // `work.title` and `editions.*.label` are not. Asked once beside the load
  // and never waited on: until it answers, or if it fails, the bar names the
  // edition by its key (`readerTitle.ts`) and links to no series. No
  // stale-answer guard: `Routes.tsx` keys this component by `work`, so one
  // instance only ever asks about one work.
  const [detail, setDetail] = useState<WorkDetail>()
  // O173: until the work has loaded — and for good if it never does — the tab
  // says the app's name rather than the last screen's. `ReaderHeading` names
  // the work once the reader draws.
  useDocumentTitle(undefined)
  // A ref, so a new function from the route does not ask the library again.
  const onSeriesRef = useRef(onSeries)
  onSeriesRef.current = onSeries
  useEffect(() => {
    workDetail(work).then(
      (answer) => {
        setDetail(answer)
        onSeriesRef.current?.(answer.series)
      },
      (err: unknown) => {
        if (!(err instanceof Unauthenticated)) console.error(err)
      },
    )
  }, [work])

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
  // reader is asked to trust that. Without this, three things would go wrong
  // while a `PATCH` was in flight: a second `Confirm` click would start a
  // second, concurrent write against the same `etag`; `Escape` and `Cancel`
  // would clear `pending` and take the banner away, telling the reader nothing
  // had happened while the write was still going to land; and the completion
  // handler would then navigate the reader — or, if they had pressed `c` again
  // in the meantime, silently replace the correction they had just started.
  //
  // Disabling all three exits for the duration closes every one of those by
  // construction: there is no way to reach a second `pending` while the first
  // is still settling, so the completion handler cannot be stale.
  const [submitting, setSubmitting] = useState(false)
  // The keydown effect subscribes once at load, so its closure cannot see
  // this — same staleness `sessionRef` and `pendingRef` exist to route around.
  // `useLatest` (`hooks.ts`) is that mirror; the argument for why a listener
  // frozen at subscription needs one is in its own doc.
  const submittingRef = useLatest(submitting)
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
  // reasons: a second write would race the first against one validator, and
  // an exit taken mid-write would tell the reader nothing had happened while
  // it was still going to land.
  const [notesBusy, setNotesBusy] = useState(false)
  // Every state of the notes this session has written, and where the reader
  // stands in it. Seeded on load, so the document as it arrived is the floor
  // undo cannot go below.
  const [notesHistory, setNotesHistory] = useState<History<Note[]>>(() =>
    emptyHistory<Note[]>([]),
  )
  const [notesNotice, setNotesNotice] = useState<string>()
  // Which surface the note panel is showing, every note this editing session
  // has changed, and which of them the panel is showing — one `useState`,
  // because they are one thing: `notes.ts`'s `Draft`, whose transitions hold
  // the rules. `n` opens the editor on a fresh note, a margin tab opens it on
  // an existing one, `a` toggles the review list, `Escape` closes either.
  //
  // Every transition is applied through the functional updater rather than
  // from this render's value, which is what lets the keydown effect — frozen
  // at load — reach the current draft without a mirror ref of its own.
  const [editing, setEditing] = useState<Draft>(NOTHING_OPEN)
  const { panel: panelMode, edits, currentId } = editing
  // The note the panel is showing. Derived rather than stored, so there is one
  // copy of every edit and no way for the shown note and the saved one to
  // disagree.
  const draft = shownNote(editing)
  /** Change the note on show, in place. */
  const editCurrent = (change: (note: Note) => Note) =>
    setEditing((open) => {
      const next = changeShown(open, change)
      // The same draft back means there was nothing on show to change, and a
      // write scheduled for that would send the document as it already is.
      if (next === open) return open
      // Scheduled here rather than in each caller, so no way of changing a
      // note can forget to save it — which, with no Save button, would mean a
      // change that quietly never happened.
      scheduleSave(next.edits)
      return next
    })

  // The keydown effect subscribes once at load, so it cannot see these — the
  // same staleness `sessionRef` and `pendingRef` exist to route around.
  const panelModeRef = useLatest(panelMode)
  const notesBusyRef = useLatest(notesBusy)

  // Every registered module's own preference slice, in one map keyed by
  // `ReaderModule.id` rather than one hand-declared `useState` per module.
  // Built once, from `registry.all()`, the same source `Settings.tsx` reads
  // to generate the panel itself — a module that is registered is
  // preferences-wired for free, the claim `Settings.tsx`'s own doc makes
  // about the panel also true of the state underneath it. Loaded once: there
  // is exactly one `work` for this component's lifetime (no work-switcher
  // exists yet), so there is nothing to reload against.
  //
  // Loading it once and persisting it on every change is `hooks.ts`'s
  // `usePrefsMap` — including why the write is an effect rather than
  // something `setPrefsMap`'s callers do (an updater must stay pure) and why
  // it writes the whole map rather than the slice that moved. `registry` and
  // the two storage calls are handed in, which is what lets that hook be
  // tested without either.
  const [prefsMap, setPrefsMap] = usePrefsMap(
    registry.all(),
    loadPrefs,
    savePrefs,
  )

  // Typed reads of each module's own slice — `sliceFor` (`module.ts`) restores
  // the `P` a plain `Record<string, object>` can't carry, so every site below
  // is typed. Derived on every render rather than memoised: each is a cheap
  // property lookup with a fallback, not a computation worth caching.
  const chosen = sliceFor(prefsMap, layoutModule).mode
  // Compare needs a second edition, and the choice is kept across works, so on
  // a work of one it draws a single page, sized and pressed as one.
  const layout: Layout =
    chosen === 'compare' && (session?.editions.length ?? 0) < 2
      ? 'single'
      : chosen
  // Compared, the edition viewed is drawn as single draws it, with the next
  // edition's page beside it (`companionPage`).
  const mode: Mode = layout === 'compare' ? 'single' : layout
  // The reading mode is a preference, not component state. `layoutModule`
  // holds it, which is what makes it load, save and reach the settings panel;
  // `setMode` writes that slice and nothing else changes: `layout` is the
  // choice as it can be drawn, and `mode` the page layout drawn for it.
  //
  // `changeSlice`, not a hand-written spread of the map and of the slice —
  // `LayoutPrefs` has one field, so writing it out would work and would be
  // exactly the bug `direction.ts`'s `setOverride` exists to prevent.
  const setMode = (next: Layout) =>
    setPrefsMap((map) =>
      changeSlice(map, layoutModule, (prefs) => ({ ...prefs, mode: next })),
    )
  const directionPrefs = sliceFor(prefsMap, directionModule)
  const motionPrefs = sliceFor(prefsMap, motionModule)
  const readoutsPrefs = sliceFor(prefsMap, readoutsModule)
  const prefetchPrefs = sliceFor(prefsMap, prefetchModule)
  const annotationsPrefs = sliceFor(prefsMap, annotationsModule)
  const regroupPrefs = sliceFor(prefsMap, regroupModule)
  const fitPrefs = sliceFor(prefsMap, fitModule)
  const borrowPrefs = sliceFor(prefsMap, borrowModule)
  // Read here rather than beside `creditsPrefs` further down, because two
  // effects declared above the `if (!session) return null` guard close over
  // it — the wheel subscription and the swap's own geometry — and a binding
  // created after that guard is not in scope for either.
  const zoomPrefs = sliceFor(prefsMap, zoomModule)

  // Two decode queues, not one: sharing a single queue between the live page
  // turn and the prefetch window would let stale prefetch work starve the
  // decode the reader is actually waiting on. Every slot change enqueues
  // several prefetch jobs plus one swap job, and a FIFO queue with no
  // cancellation runs them in the order they arrived, so a fast flick's
  // *last* keypress — the slot the reader actually lands on — could queue
  // behind a handful of prefetch jobs for slots already left behind. Each job
  // holds a concurrency slot for a full round trip, so at any real network
  // latency the reader could be staring at the page they started from well
  // after they stopped pressing keys.
  //
  // Separate queues rather than priority or cancellation: `swapQueue` is what
  // every mounted `PageView` decodes through (passed as `decodeQueue` below),
  // `prefetchQueue` is what the prefetch effect further down decodes through,
  // and neither can ever hold a slot the other needs. Cancelling a stale
  // prefetch job's underlying fetch is the alternative, but `decode.ts` has
  // no such hook — `<img>.decode()` cannot be aborted mid-flight without
  // discarding the element and losing whatever bytes it already has, which
  // trades one defect for a different one (re-fetching from zero on every
  // supersede). Two independent, already-existing queues need no new
  // capability at all.
  //
  // Both built lazily and *rebuilt* whenever `prefetchPrefs.maxConcurrent`
  // changes, which is `hooks.ts`'s `useDecodeQueue` — where the argument for
  // rebuilding rather than baking the cap in at mount lives, along with what
  // it costs. Two calls, because the two queues must be two instances; that
  // they read the same setting is the setting's business, not the queue's.
  const swapQueue = useDecodeQueue(prefetchPrefs.maxConcurrent)
  const prefetchQueue = useDecodeQueue(prefetchPrefs.maxConcurrent)

  // What makes `two` mode's pair appear as a pair. Every `PageView` decodes
  // through `swapQueue` and presents through this, so the halves of a turn
  // become visible in one frame instead of one at a time. `cohort.ts` says why
  // one shared instance is right rather than one per group, and its entry in
  // `docs/superpowers/notes/2026-10-29-code-history-web-src.md` holds the
  // measurement that put it here. Built once and never rebuilt: unlike the
  // queues above it has no setting to track, since a cohort's membership is
  // decided by when its members join, not by any number a reader can tune.
  const [pageCohort] = useState<Cohort>(() => createCohort())

  // What the two cells of `two` mode are actually drawing, reported by each
  // `PageView` as it presents. The *geometry* below is derived from this rather
  // than from `current`: a keypress moves state at once while a cell's picture
  // waits for its decode, so a layout keyed on state would flap between one
  // wide page and two half-width ones during a flick, with the artwork never
  // changing at all. `cellsFor` gives both cells the same URL exactly when a
  // group is `wide` (a joined spread's two slots share one page, a solo slot
  // is mirrored into the second cell), so "both cells are drawing the same
  // source" *is* wide, read off the screen instead of predicted.
  //
  // The record itself is `hooks.ts`'s `useDrawnSrcs` — including why a report
  // that changes nothing hands back the same object. It stays `App` state
  // rather than `GroupView`'s for the reason `drawGroup`'s doc gives below.
  const [drawnSrcs, showDrawn] = useDrawnSrcs()

  // The keydown effect below subscribes once, after load (see its own comment),
  // so its closure's `session` is frozen at that instant — reading a module
  // key's action needs the edition currently being viewed, which changes on
  // every swap. This ref is kept current across every render instead.
  const sessionRef = useLatest(session)

  // `Escape` has to read the *current* `pending` inside the same keydown
  // effect that only resubscribes on load (see its own comment below) — the
  // same staleness `sessionRef` exists to route around, for the same reason.
  const pendingRef = useLatest(pending)

  // Whether the help overlay is open, for the same `Escape` branch and by the
  // same route. Kept in an effect rather than assigned during render like
  // `creditsPrefsRef` below, because this is plain state and not derived from
  // anything — the split `panelModeRef` above already draws, and for the same
  // reason: nothing recomputes this value, so a render that React discards has
  // nothing to recompute it back from.
  const helpVisibleRef = useLatest(helpVisible)

  // Settings and Credits, for `Escape` — the same route and the same reason as
  // `helpVisibleRef` above (spec 2026-09-26 §3.3).
  const settingsVisibleRef = useLatest(settingsVisible)
  const creditsVisibleRef = useLatest(creditsVisible)

  // The way out, for `L` and `Escape` in the same keydown effect, by the
  // same route: the route hands in a new one once the work's series is
  // known (O431), which can be after that effect has subscribed.
  const onLeaveRef = useLatest(onLeave)

  // The grouping the page area is drawing right now, for the keydown effect —
  // which subscribes once at load and so cannot see `grouped`, computed below
  // at render from `mode`, the direction override and the reader's own parity
  // breaks. An arrow steps by group rather than by slot (`pairing.ts`'s
  // `stepToGroup`), and only this component knows what the groups are.
  const groupedRef = useRef<Group[]>(undefined)

  // What `b` weighs a press against, for the same handler and by the same
  // route as `groupedRef`: the viewed edition's two-page grouping for a list
  // of the reader's own breaks, and the mode on screen. `b` judges every
  // press by the two-page grouping, whatever the mode, which `groupedRef`
  // holds only in `two`.
  const regroupRef = useRef<{
    layout: (own: number[]) => Group[]
    mode: Mode
  }>(undefined)

  // The slots the `credits` module has set aside, for the same handler and by
  // the same route as `groupedRef` above — and it is not derivable from that
  // grouping, which is why it is a second ref rather than a lookup through the
  // first. `groups` is given this set and emits nothing for what is in it, so
  // a skipped slot is absent from the result and so is a slot past the end;
  // the arrow out of a gap has to tell those apart. `docs/status.md`'s O40 is
  // what happens when the gap branch has neither: it reaches `nearestPaired`,
  // which resolves candidates straight off the resolver, and lands on a credit
  // page the reader asked to skip.
  const skippedRef = useRef<Set<number>>(EMPTY_SLOTS)

  // The memo behind `derivedBreaks` below, which cannot be a `useMemo`: hooks
  // have to sit above this component's `if (!session) return null`, and every
  // input that derivation takes is bound below it. Held rather than recomputed
  // because the walk is a wasm call per hub slot — see `alignedBreaksCached`,
  // where the caching rule lives so it can be tested.
  const alignedBreaksRef = useRef<DerivedBreaksCache>(emptyBreaksCache())

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
  // `Viewport` (the spec's scope decision), and where the viewed edition has
  // no slot the page area draws `Borrowed` or `Gap` in place of the group.
  // Both read the same way at every use — there is nothing on screen for a
  // zoom, a pan or a swap's arithmetic to be about. **A borrowed page is a
  // page and still leaves this `undefined`**, which is the whole of "a
  // borrowed page is not a reading position": the image the reader sees is not
  // one this ref will ever describe.
  const drawnRef = useRef<DrawnPage | undefined>(undefined)

  // The page area itself, for the two things that genuinely need the box
  // rather than the image inside it: the wheel subscription (which must be
  // non-passive, see its own comment) and the swap, which measures both
  // pages' natural sizes out of it and takes the box's aspect to fit against.
  // `leave-bars` also moves focus onto it.
  //
  // The annotation overlay measures the `<img>` it sits over, not the area
  // around it, as `notes.md` records under "The annotation overlay was placed
  // on the page area, not on the page" (O27). The region drag reads its
  // geometry off the `<img>` via `draftImage`/`pointOn`. What this ref is for
  // is the box zoom is expressed in fractions *of*, which is the page area and
  // could not be the page.
  const pageAreaRef = useRef<HTMLDivElement>(null)

  // The reader's root, which the ring reads its stops from (spec 2026-09-26
  // §3.1) — one reader on the page, but scoped anyway.
  const rootRef = useRef<HTMLDivElement>(null)

  // Whether `Escape` is the ring's way out or the reader's: `keyPlan`'s
  // `keyboardHoldsBars`. The ring puts focus in the bars as it brings them
  // up; `c` brings them up without it, so it says `shown`.
  const keyboardBars = useKeyboardBars()

  // A swap whose incoming page has not been measured yet. Set by the keydown
  // handler, which is the last moment the outgoing page is still on screen to
  // measure; consumed by the effect below, once the incoming one has decoded.
  // `edition` is the one being *left*, which is what tells a swap that changed
  // nothing (a digit for the edition already showing) from one that did.
  const swapRef = useRef<CarriedSwap | undefined>(undefined)

  // The window's width, for `auto` note-list placement — `hooks.ts`'s
  // `useWindowWidth`, which is a listener rather than a one-off read for the
  // reason that hook's own doc gives.
  const viewportWidth = useWindowWidth(NO_WINDOW_WIDTH)

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
  /**
   * The reader's own intents — a turn, a swap — reachable from a pointer
   * handler. `readerIntent` is declared inside the keydown effect, where its
   * closure over `resolver` and the grouped slots is built; the same problem
   * `closeEditorRef` above solves, solved the same way rather than by lifting
   * the function out of the effect and giving it a dependency list of its own.
   */
  const readerIntentRef = useRef<(intent: ReadingIntent) => void>(
    () => undefined,
  )
  // The notes writer, reachable from `closeEditor` above the point it is built:
  // the debounce is built further down, so naming it directly there would be
  // a use before its declaration.
  const saveNotesRef = useRef<Debounced<[Note[]]> | null>(null)

  // Where a region drag started. Declared here with the other refs, ahead of
  // the `if (!session) return null` below, because hooks must run in the same
  // order on every render — the handlers that use it are defined further down,
  // where the page geometry they read is in scope.
  const dragRef = useRef<DragStart | null>(null)
  /**
   * The press in progress, as `gestures.ts` reads it on release. Beside
   * `dragRef` and `panRef` because it is the third thing one press can be
   * carrying, and above `if (!session) return null` because it is a hook.
   */
  const pressRef = useRef<
    | (Press & { onControl: boolean; consumed: boolean; pointerId: number })
    | null
  >(null)
  // Where the pointer was on the previous pan frame, in client pixels. A ref
  // rather than state and a *previous* position rather than the drag's origin:
  // `panBy` takes a delta and clamps, so each frame only needs the step since
  // the last one — accumulating against the origin instead would fight the
  // clamp, replaying the whole drag every frame once an edge is reached.
  const panRef = useRef<{ x: number; y: number } | null>(null)
  // The rectangle under the pointer right now. State rather than a ref,
  // because the whole point is that it re-renders: a drag with no live
  // feedback is invisible.
  const [dragging, setDragging] = useState<Region>()

  /**
   * **Sign-in is decided above this component, and forgotten by unmounting.**
   *
   * `Gate.tsx` asks `me()` once per page load and renders nothing below it
   * until the answer arrives, so a mounted `App` is by construction a signed-in
   * one and there is no third state for this file to carry. That matters for
   * more than tidiness: `pageUrl`/`thumbUrl` are `<img src>` URLs rather than
   * fetches, so a signed-out reader would otherwise see broken images with
   * nothing saying why — the one failure `api.ts`'s `Unauthenticated` cannot
   * reach.
   *
   * A hand-written reset, a list of setters, cannot be kept complete by
   * review, and every new piece of state is one more chance to forget one: a
   * missed one leaks into the next account, as unsaved note bodies drawn to it
   * and then `PUT` under its session, or a pending correction whose Confirm
   * writes the previous reader's pairing into the shared sidecar. The gate
   * unmounts this whole component instead, which cannot.
   *
   * **The one thing an unmount does not cover on its own is the two debounced
   * writers**, which hold timers rather than state: `hooks.ts`'s
   * `useFlushOnHide` cleanup flushes both at the unmount commit — before any
   * next account can have signed in, and against a session the server has just
   * ended. See that hook's doc for why flushing and not cancelling is the right
   * answer there.
   */
  const gate = useGate()
  const [accountOpen, setAccountOpen] = useState(false)
  // Read by the window-level keydown listener, which subscribes once and must
  // not be torn down and re-added every time the panel opens — the same reason
  // `helpVisibleRef` and `panelModeRef` exist beside it.
  const accountOpenRef = useRef(false)
  accountOpenRef.current = accountOpen
  /**
   * Replaces Settings with the account panel (O229), one modal at a time. It
   * reads no state, only setters, so a frozen closure may call it.
   *
   * **It moves no focus.** Both setters land in one commit, and React runs a
   * commit's effect clean-ups before its new effects. So Settings'
   * `usePanelFocus` first hands focus where closing Settings puts it (spec
   * 2026-09-26 §3.3), and the account panel then records that as its opener.
   * Focusing anything here, or opening the panel in a commit before Settings
   * closes, makes it record the wrong opener.
   */
  function openAccount() {
    setSettingsVisible(false)
    setAccountOpen(true)
  }
  // O58: the server takes a correction from an administrator alone, so no one
  // else is offered `c` or its help row. A ref for `accountOpenRef`'s reason.
  const mayCorrect = gate?.who.admin === true
  const mayCorrectRef = useRef(mayCorrect)
  mayCorrectRef.current = mayCorrect

  // The four reads and the order they happen in are `startup.ts`'s
  // `openWork`, which is where the throw, the notes read's own `catch` and
  // the empty-document fallback all live and can be tested. What is left
  // here is the effect, the `cancelled` flag, five setters for a load that
  // answered and one for a load that did not.
  useEffect(() => {
    let cancelled = false
    openWork(STARTUP_PORTS, work)
      .then((opened) => {
        // Logged whether or not this load was superseded: a notes read that
        // failed did so, and a teardown racing it does not make that untrue.
        if (opened.notesError !== undefined) console.error(opened.notesError)
        if (cancelled) return
        setSession(opened.session)
        setEtag(opened.etag)
        setNotesDoc(opened.notes)
        setNotesEtag(opened.notesEtag)
        setNotesHistory(emptyHistory(opened.notes.notes))
      })
      .catch((err: unknown) => {
        if (cancelled) return
        // A 401 arriving here means the session ended between `Gate`'s `me()`
        // and this load — a sweep, a revocation from another browser, a
        // password change elsewhere. It is already routed: every call in
        // `openWork` goes through `api.ts`'s `request`, which fires
        // `whenSignedOut` before it throws, and the gate above this component
        // is what registered for it. Logging it here as well would put a stack
        // trace in the console of a reader who is being shown a login screen,
        // and it is not a failure to draw either: `Unauthenticated` carries a
        // message like any `Error`, and the gate is about to unmount this
        // component for the sign-in form.
        if (err instanceof Unauthenticated) return
        console.error(err)
        // O176. Anything else is a work that will not open — a slug the
        // library does not hold, a server that did not answer, a sidecar
        // naming no hub. Below the `cancelled` check with everything else
        // here, so a load that was superseded says nothing.
        //
        // O179, O91. The sidecar read's 404 is told apart by its type, as
        // the 401 above is: `api.ts` throws a `WorkMissing` for a work marked
        // missing and a `WorkNotFound` for any other, and what is kept is the
        // slug either carries, which `ReaderFailed` puts in a sentence of its
        // own; its comment says what answers 404. Every other failure is read
        // with `messageOf`, as `Library.tsx` and `Series.tsx` read theirs, and
        // shown as that.
        setFailure(
          err instanceof WorkMissing
            ? { kind: 'missing', work: err.work }
            : err instanceof WorkNotFound
              ? { kind: 'not-found', work: err.work }
              : { kind: 'thrown', message: messageOf(err) },
        )
      })
    return () => {
      cancelled = true
    }
  }, [work])

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
      // Every guard is `keyplan.ts`'s, computed from the mirror refs and the
      // derived ones assigned during render. `intentOf` and `controlKind` are
      // both pure, so calling them for every key costs a property read and
      // buys one place where the decision is made.
      const plan = keyPlan(
        event.key,
        intentOf(event, editions, registry.all()),
        {
          helpOpen: helpVisibleRef.current,
          panel: panelModeRef.current,
          correcting: pendingRef.current !== undefined,
          submitting: submittingRef.current,
          notesBusy: notesBusyRef.current,
          control: controlKind(event.target),
          accountOpen: accountOpenRef.current,
          keyboardHoldsBars: keyboardBars.held(),
          settingsOpen: settingsVisibleRef.current,
          creditsOpen: creditsVisibleRef.current,
          inPanel: inPanel(event.target),
          zoomable: zoomable(
            zoomPrefsRef.current,
            drawnRef.current !== undefined,
          ),
          creditsPanel: creditsPrefsRef.current.panel,
          helpEnabled: helpPrefsRef.current.enabled,
          mayCorrect: mayCorrectRef.current,
        },
      )
      if (!plan) return
      if (plan.preventDefault) event.preventDefault()
      switch (plan.action.kind) {
        case 'close-help':
          return setHelpVisible(false)
        case 'close-editor':
          return closeEditorRef.current()
        case 'cancel-correction':
          return cancelCorrection()
        case 'ring':
          return ringMove(event.target, event.shiftKey)
        case 'open-account':
          return openAccount()
        case 'close-account':
          return setAccountOpen(false)
        case 'close-settings':
          return setSettingsVisible(false)
        case 'close-credits':
          return setCreditsVisible(false)
        case 'leave-bars':
          // Onto the page before the bars unmount, so focus is not dropped to
          // `<body>` with the stop it was on (spec 2026-09-26 §3.1).
          pageAreaRef.current?.focus({ preventScroll: true })
          return setChromeVisible(false)
        case 'begin-correction':
          return beginCorrection()
        case 'leave-reader':
          return onLeaveRef.current()
        case 'module':
          return moduleKey(plan.action.intent)
        case 'reading':
          return readerIntent(plan.action.intent)
        // The reader claims the key and prevents its default, and nothing
        // happens — a `C` with the credits panel switched off, `=` with
        // nothing drawn, `a` while a note write is on the wire. See
        // `KeyAction`'s own doc for why this is not the same answer as
        // `undefined`.
        case 'inert':
          return
      }
    }
    function cancelCorrection() {
      setPending(undefined)
      setCorrectionNotice(undefined)
    }
    /**
     * `Tab` and `Shift+Tab`: one stop round the reader's ring — spec
     * 2026-09-26 §3.1. `ring.ts` decides where from counts; `focus.ts` reads
     * them off the page and moves focus.
     *
     * **Asked twice when the bars change**, because what the ring holds
     * depends on them: the bars' stops exist only while they are mounted
     * (`Chrome` returns `null` while hidden), and the note list below the page
     * takes focus only while they are not (O112). The first answer says which
     * way the bars must go; the second, asked of the page that change leaves,
     * says which stop. `flushSync` is what makes the second read see it.
     */
    function ringMove(from: EventTarget | null, backwards: boolean) {
      const root = rootRef.current
      if (!root) return
      const at = ringAt(root, from)
      let to = ringStep(at, ringSizes(root), backwards)
      // Up exactly while focus is on one of theirs — except while correcting,
      // when the correction holds them up.
      const wanted = to.region === 'bars' || pendingRef.current !== undefined
      if (wanted !== barsMounted(root)) {
        flushSync(() => setChromeVisible(wanted))
        to = ringStep(at, ringSizes(root), backwards)
      }
      focusStop(root, to)
    }
    /**
     * Open correction mode on the slot being read.
     *
     * The three guards here are the two `keyplan.ts` names as unable to come
     * with it — they dereference `sessionRef` and run `display()` against a
     * live resolver, neither of which a plan taking flags can do — plus
     * `counterpartEdition`'s own `undefined`.
     */
    function beginCorrection() {
      const current = sessionRef.current
      if (!current) return
      const shown = display(current.state, current.resolver)
      // No slot to correct while showing a gap — there is nothing on screen to
      // name as `a`.
      if (shown.kind !== 'at') return
      const to = counterpartEdition(
        current.state,
        current.previous,
        shown.edition,
      )
      if (to === undefined) return
      const slots = allSlots(current.resolver, shown.edition)
      setPending({ from: shown.edition, to, a: slots[shown.slot] })
      setCorrectionNotice(undefined)
      keyboardBars.shown()
      setChromeVisible(true) // the filmstrip is where a counterpart is picked
    }
    /** `d`: this edition the other way round. */
    function flipDirection() {
      const current = sessionRef.current
      if (!current) return
      const edition = current.state.viewing
      const sidecarReading = current.reading
      setPrefsMap((map) =>
        changeSlice(map, directionModule, (prefs) =>
          flipped(sidecarReading, prefs, edition),
        ),
      )
    }
    /**
     * `b`: realign the two-page pairing from the pair on screen (`realign`).
     * Breaks are slots of the *viewed* edition, the space `groups` indexes, so
     * this is `shown.slot`, not authority-space `state.slot`; a gap has none,
     * and a press that changes no pair saves nothing. A restore, a mode switch
     * or a swap can leave the reader on a pair's second slot: the press puts
     * them on its first (`pairStartBefore`), the page the break stands alone,
     * or they would be drawn on the next pair and a second press would not undo
     * the first.
     */
    function breakPairingHere() {
      const current = sessionRef.current
      const regroup = regroupRef.current
      if (!current || !regroup) return
      const shown = display(current.state, current.resolver)
      if (shown.kind !== 'at') return
      const first =
        regroup.mode === 'two'
          ? pairStartBefore(groupedRef.current ?? [], shown.slot)
          : undefined
      if (first !== undefined) {
        setSession((prev) =>
          prev ? { ...prev, state: navigate(prev.state, first) } : prev,
        )
      }
      setPrefsMap((map) => {
        const prefs = sliceFor(map, regroupModule)
        const next = realign(
          regroup.layout,
          breaksFor(prefs, current.work, shown.edition),
          shown.slot,
          regroup.mode,
        )
        return next === undefined
          ? map
          : withSlice(
              map,
              regroupModule,
              withBreaks(prefs, current.work, shown.edition, next),
            )
      })
    }
    /**
     * `n`: always a brand-new note, however many the slot already holds.
     *
     * Reopening the note already there instead would make a second note on a
     * page impossible, since nothing else creates one. A draft is not a note
     * until it is written, so a second press only ever replaces an unsaved
     * draft. Reopening an existing note is what its margin tab is for.
     *
     * Nothing is needed here to stop `n` replacing a draft mid-write: no
     * module key reaches this at all while the editor is open (`keyPlan`'s
     * `panel === 'edit'` refusal), so a reader has to close it first.
     */
    function startNote() {
      const current = sessionRef.current
      if (!current) return
      const shown = display(current.state, current.resolver)
      // A gap has no slot to annotate — there is nothing on screen to anchor
      // to, the same reason `c` is inert there.
      if (shown.kind !== 'at') return
      // Minted here rather than inside the updater below: an updater must stay
      // pure, and a clock and a random source are neither.
      const id = ulid(Date.now(), cryptoRandom)
      const now = new Date().toISOString()
      const anchor = current.resolver.memberPage(shown.edition, shown.slot)
      setEditing((open) =>
        newNote(open, shown, hereRef.current, id, now, anchor),
      )
      setNotesNotice(undefined)
    }
    /**
     * `a`: the review list. Toggles, like `s` does — open it, or close it if
     * it is what is already open.
     *
     * **It cannot reach a reader mid-edit.** `toggleReview` has a second arm,
     * where `panel: 'edit'` becomes `'review'`, and no keypress gets to it:
     * `keyplan.ts` returns no plan at all for anything but `Escape` while the
     * editor is open, because the editor is a dialog that takes text and a
     * keystroke there means the note. That is the behaviour a reader typing
     * a note wants, and the arm is reachable only by a caller that does
     * not exist.
     */
    function reviewNotes() {
      setEditing(toggleReview)
      setNotesNotice(undefined)
    }
    /**
     * A module's own key.
     *
     * A small explicit switch rather than a generic action table: each of
     * these does something different enough with its key (a per-edition flip,
     * a per-volume break, a rect step, a plain visibility toggle) that a
     * table would need its own per-entry shape anyway. Every case is one call
     * — the guards are `keyPlan`'s and the rules are their modules'.
     */
    function moduleKey(intent: string) {
      switch (intent) {
        case TOGGLE_INTENT:
          return flipDirection()
        case REGROUP_TOGGLE_INTENT:
          return breakPairingHere()
        case NEW_INTENT:
          return startNote()
        case REVEAL_INTENT:
          return setPrefsMap((map) =>
            changeSlice(map, annotationsModule, toggleReveal),
          )
        case UNDO_INTENT:
          return stepBackRef.current()
        case REDO_INTENT:
          return stepForwardRef.current()
        case REVIEW_INTENT:
          return reviewNotes()
        case SETTINGS_TOGGLE_INTENT:
          return setSettingsVisible((visible) => !visible)
        case PANEL_INTENT:
          return setCreditsVisible((shown) => !shown)
        case ZOOM_IN_INTENT: {
          const zoom = zoomPrefsRef.current
          return setView((rect) => zoomStep(rect, 'in', zoom))
        }
        case ZOOM_OUT_INTENT: {
          const zoom = zoomPrefsRef.current
          return setView((rect) => zoomStep(rect, 'out', zoom))
        }
        case ZOOM_WHOLE_INTENT: {
          const zoom = zoomPrefsRef.current
          return setView((rect) => zoomStep(rect, 'whole', zoom))
        }
        case HELP_TOGGLE_INTENT:
          return setHelpVisible((shown) => !shown)
      }
    }
    /**
     * Space, an arrow or a digit.
     *
     * What the keypress does to the view is decided **before** `setSession`
     * rather than after it. A swap has to measure the outgoing page, and the
     * only place that page is still on screen to be measured is here: React
     * batches an event handler's updates and re-renders once it returns, so
     * "after the state update" and "before it" are the same DOM either way —
     * but only one of the two says so.
     */
    function readerIntent(intent: ReadingIntent) {
      if (intent.kind === 'step') {
        // A turn carries both zoom and pan (spec §3) and re-clamps. Not a
        // mapping: same edition, same coordinate space.
        //
        // **Idempotent as the code stands, and kept anyway.** Every producer
        // of this state (`zoomAt`, `panBy`, `mapView`) already clamps, so this
        // changes nothing today. It is here because the bound belongs to the
        // state rather than to each producer's good manners, and a turn is
        // where a rect that had drifted would first be visible.
        setView(clampRect)
      } else {
        recordSwap()
      }
      setSession((prev) => {
        if (!prev) return prev
        // An arrow is a *physical* key and `intentOf` reports it as one;
        // which page that reaches depends on the work. `stepFor` is the
        // single rule (`modules/direction.ts`), shared with the filmstrip's
        // layout and the help's labels, so the three cannot disagree about
        // which way is forward.
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
          skippedRef.current,
        )
        return { ...prev, state, previous }
      })
    }
    readerIntentRef.current = readerIntent
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
      const drawn = drawnRef.current
      const host = pageAreaRef.current
      // Which of the measurable states this is in is `swapView.ts`'s
      // `carrySwap`; the only thing this can contribute is the measurement
      // itself, off the images the page area happens to be drawing.
      swapRef.current = carrySwap(
        drawn,
        host && drawn
          ? frameOf(host.querySelectorAll('img'), drawn.url)
          : undefined,
        sessionRef.current?.state.viewing,
        zoomPrefsRef.current.enabled,
      )
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [session?.resolver, session?.editions])

  // Built once and kept, not a module-level singleton and not a `useState`:
  // its pending timer must survive every render in between rather than being
  // replaced by a fresh, un-armed debounce on each one. `hooks.ts`'s
  // `useConstant` is that idiom, carrying the argument for why passing the
  // built value straight to `useRef` is the wrong shape — and a test that
  // watches it not rebuild.
  const writeProgress = useConstant(() =>
    debounce<[string, StoredProgress]>((work, position) => {
      putProgress(work, position).catch((err: unknown) => console.error(err))
    }, PROGRESS_DEBOUNCE_MS),
  )
  // The page a turn lands on is named here, as the turn is recorded, and not
  // when the debounce fires: by then a correction's re-read may have swapped
  // the resolver, and the new layout can show another page at the same slot
  // (O240). `sessionRef`'s effect is declared above `useProgressWrite`'s, so
  // in the same commit it has already run and holds this slot's session.
  // Built once, since `useProgressWrite` re-runs its effect when it changes.
  const recordProgress = useConstant(
    () => (work: string, position: { edition: string; slot: number }) => {
      const { edition, slot } = position
      const at = sessionRef.current?.resolver.memberPage(edition, slot)
      writeProgress(work, { edition, slot, at })
    },
  )

  /**
   * Notes reach the server on their own, a moment after the last change.
   *
   * There is no Save button. A reader writing a note has already decided to
   * write it, and changing their mind is what undo is for.
   *
   * Debounced for the same reason the reading position is: typing a sentence
   * is one write, not forty. Longer than the position's 400 ms, because a
   * position is a number and a note is prose — a reader pauses mid-sentence
   * and should not spend a round trip on it.
   */
  const saveNotes = useConstant(() =>
    debounce<[Note[]]>((notes) => {
      void writeNotesLatestRef.current?.(notes)
    }, NOTES_DEBOUNCE_MS),
  )
  saveNotesRef.current = saveNotes
  // `debounce` holds the callback it was built with for the life of the
  // component, and `writeNotes` closes over state that changes every render —
  // so the debounce calls through this ref to reach the current one rather
  // than the one that existed at first render.
  const writeNotesLatestRef = useRef<((notes: Note[]) => Promise<void>) | null>(
    null,
  )

  // A pending write must survive the tab going away, not just the session
  // staying open. Which two events say so, why both are subscribed rather
  // than one, and why both firing for the same close is not a double write
  // are all `hooks.ts`'s `useFlushOnHide`. Both of this component's debounced
  // writers are handed to it; nothing else here holds a pending write.
  useFlushOnHide(writeProgress, saveNotes)

  // Progress is written here, and only here — on a change to
  // `state.authority`/`state.slot`, which is exactly what `navigate` changes
  // and `swap` never does (`swap` only ever assigns `state.viewing`; see
  // `reading.ts`). The three fields are passed as scalars rather than as the
  // session holding them, which is what makes that enforcement structural:
  // there is no whole-session dependency for a later edit to reach for, and
  // `hooks.ts`'s `useProgressWrite` carries both that argument and the
  // skip-the-restoring-load rule, with tests over each.
  useProgressWrite(
    session?.work,
    session?.state.authority,
    session?.state.slot,
    recordProgress,
  )

  // The reading-time readout's raw material — when this session started, the
  // page turns it has seen, and the 30s heartbeat that makes an estimate
  // derived from them tick rather than only updating on the next turn. All
  // three are `hooks.ts`'s `useReadingClock`, which is also where the reason
  // for skipping the first run lives and why only the authority's position
  // reaches it — a swap is not a page turn.
  const clock = useReadingClock(
    session?.state.authority,
    session?.state.slot,
    READING_TICK_MS,
  )

  // Scroll mode mounts every group at once, so nothing else puts the reading
  // position on screen — `hooks.ts`'s `useScrollToCurrent`, which is where
  // that argument and the `IntersectionObserver` this deliberately is *not*
  // both live. The ref it hands back is attached below to whichever rendered
  // group is `currentIndex`'s.
  //
  // Called here, ahead of the `if (!session) return null` below, because
  // hooks must run in the same order on every render — `currentIndex` itself
  // is only computed after that guard, so this is passed the state it is
  // derived from instead.
  const currentGroupRef = useScrollToCurrent(
    mode,
    session?.state.authority,
    session?.state.slot,
    session?.state.viewing,
  )

  // How large a page to ask for — `pageSize.ts` holds the rule and the
  // measurement behind it. `1 / view.w` is the magnification: `view` is a
  // normalised rect over the group, so fit-whole (`WHOLE`, `w: 1`) is 1x and a
  // rect a quarter as wide is 4x. Once magnification wants more pixels than the
  // ladder offers this answers `undefined`, which asks for the original — so
  // zoom is sharp without a separate branch for it.
  //
  // Computed at render from values that already trigger one: `viewportWidth`
  // is state updated on resize, `mode` and `view` are state. `innerHeight` is
  // read directly rather than tracked, since every resize that changes it
  // changes `viewportWidth` too on any real window manager, and a height-only
  // change costs at most one rung until the next render.
  //
  // Declared here, ahead of the `if (!session) return null` below, because the
  // prefetch effect that follows builds its URLs from it, just as every cell
  // does, and the two have to match: the browser caches a page once per `?w=`,
  // so a page warmed at any other size warms nothing the reader draws (O110).
  const pageLongEdge = requestedLongEdge(
    pageBoxLongEdge(
      viewportWidth,
      window.innerHeight,
      layout === 'compare' ? 'two' : mode,
    ),
    window.devicePixelRatio,
    1 / view.w,
  )

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
  // Each URL carries `pageLongEdge`, the size a cell asks for, and the window
  // is warmed again whenever a resize or a zoom moves it a rung: the page a
  // turn lands on is drawn at the new size, so the old one warms nothing.
  //
  // Declared here, ahead of the `if (!session) return null` below, for the
  // same reason `currentGroupRef`'s effect above is: hooks must run in
  // the same order on every render, so this reads `sessionRef.current`
  // (kept fresh by the effect near the top of this component) rather than
  // the `session`/`shown`/`resolver` bindings this function only creates
  // after that guard.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the three `session?.state.*` deps are triggers, not closure reads — this effect reads `sessionRef.current` and exists to re-run the prefetch window when the position moves. Removing them, which is what this rule suggests, would prefetch once and never again.
  useEffect(() => {
    const current = sessionRef.current
    if (!current) return
    const shownNow = display(current.state, current.resolver)
    // Which pages are worth warming — groups rather than slots, and empty in
    // scroll mode, while disabled and in a gap — is `prefetchTargets`. What is
    // left here is the queue, the URL and the session ref the window is read
    // through. `groupedRef` is the grouping already on screen, assigned during
    // render for exactly this reason: an effect reading a stale one would warm
    // the pages of a layout the reader has left.
    const targets = prefetchTargets(
      groupedRef.current,
      shownNow,
      prefetchPrefs,
      mode,
      (edition, index) => current.resolver.slotAt(edition, index),
    )
    // Nothing to warm. The `kind` test is `prefetchTargets`' own gap rule
    // restated for the narrowing below rather than a second decision — a gap
    // has no targets, so the first half of this is what actually returns.
    if (targets.length === 0 || shownNow.kind !== 'at') return
    // The position this window is *for*, captured so each job can ask whether
    // it still matters when the queue reaches it — `superseded`'s own doc has
    // what that costs when nobody asks.
    const queuedFor = { edition: shownNow.edition, slot: shownNow.slot }
    const stale = () => {
      const now = sessionRef.current
      return superseded(queuedFor, now && display(now.state, now.resolver))
    }
    for (const target of targets) {
      const url = pageUrl(
        current.work,
        queuedFor.edition,
        target.page,
        pageLongEdge,
      )
      void prefetchQueue.run(() =>
        stale() ? Promise.resolve() : decodeTo(new Image(), url),
      )
    }
  }, [
    mode,
    pageLongEdge,
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
  // `touchstart`, `touchmove` and `wheel` — and `preventDefault()` inside a
  // passive listener does nothing except log a console error, once per notch.
  // Zoom has to stop the browser acting on the same wheel: a trackpad pinch
  // arrives here as a `wheel` with `ctrlKey` set, and left alone it zooms the
  // whole page's chrome instead of the manga. So this is `{ passive: false }`
  // on the page area itself. The same event serving both gestures is why the
  // design's table lists "wheel / pinch" as one row.
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
      // Inert wherever the three zoom keys are, for the reason `keyplan.ts`'s
      // `ZOOM_IN_INTENT` case states in full. Checked inside the handler
      // rather than beside `zoomPrefs.enabled` above, because `drawnRef` is
      // assigned during render and none of this effect's dependencies changes
      // when it flips: the page area still renders where the viewed edition
      // has no slot — it just draws `Borrowed`, or `Gap` where nothing can
      // lend, in place of the group — so the listener stays attached across
      // one, and so does the `currentIndex === -1` case that draws nothing.
      // A borrowed page makes that guard matter more rather than less: there
      // *is* an image under the pointer now, and `drawnRef` staying `undefined`
      // is the only thing that keeps a wheel from zooming a page `zoomable`
      // refuses to engage on.
      //
      // Ahead of `preventDefault()`, so a wheel this declines is left to the
      // browser rather than swallowed. That is what scroll mode already does,
      // by attaching no listener at all.
      if (!drawnRef.current) return
      event.preventDefault()
      const box = host.getBoundingClientRect()
      // The arithmetic is `modules/zoom.ts`'s `wheelStep`, including the
      // zero-box guard its `undefined` answer carries — this only supplies the
      // two DOM numbers. Called inside the updater rather than against a
      // `view` this closure captured: two notches inside one React batch would
      // otherwise both anchor against the rect before either of them, and the
      // second would undo part of the first.
      setView(
        (rect) =>
          wheelStep(
            rect,
            event,
            { x: event.clientX, y: event.clientY },
            box,
            zoomPrefs,
          ) ?? rect,
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
  // Which of the four answers a carried swap is owed is `swapView.ts`'s
  // `swapStep` — including the one that makes `done` a separate kind from
  // `drop`, which is what stops a swap pressed faster than a page decodes from
  // throwing away the newer swap's carry. What is left here is the two DOM
  // reads it needs, the ref it clears, and the listener.
  //
  // `mode` is deliberately **absent** from the dependencies below. It is not
  // what "the reader moved" means, and adding it would be worse than useless:
  // while a swap is waiting this effect leaves a cleanup that clears
  // `swapRef`, and React runs that cleanup *before* the re-run that would read
  // it, so a mode switch would drop the swap rather than redo it. What that
  // costs instead is handled by reading `drawnRef` fresh on every attempt
  // rather than capturing it — `swapStep`'s own doc has that argument.
  //
  // biome-ignore lint/correctness/useExhaustiveDependencies: the three `session?.state.*` deps are triggers, not closure reads — they are what "the reader moved" means, and the body reads refs. Removing them, which is what this rule suggests, would leave a swap's geometry never finished.
  useEffect(() => {
    const carried = swapRef.current
    if (!carried) return
    const host = pageAreaRef.current
    // No page area to measure against and nothing to attach the `load`
    // listener to. `swapStep` answers `drop` for an absent box and this is
    // that case, taken here because the listener has nowhere to go either.
    if (!host) {
      swapRef.current = undefined
      return
    }
    const mapKind = swapMapKind(zoomPrefs)
    const attempt = () => {
      const to = drawnRef.current
      const step = swapStep(
        carried,
        swapRef.current === carried,
        to,
        to ? frameOf(host.querySelectorAll('img'), to.url) : undefined,
        host.getBoundingClientRect(),
        session?.state.viewing,
        mapKind,
      )
      if (step.kind === 'wait') return false
      // `done` is finished without clearing: a later swap replaced this one
      // while it waited, and that newer swap's own effect owns the carry now.
      if (step.kind !== 'done') swapRef.current = undefined
      if (step.kind === 'apply')
        setView((rect) =>
          mapView(rect, step.from, step.to, step.areaAspect, step.mapKind),
        )
      return true
    }
    if (attempt()) return
    const onLoad = () => {
      if (attempt()) host.removeEventListener('load', onLoad, true)
    }
    host.addEventListener('load', onLoad, true)
    return () => {
      host.removeEventListener('load', onLoad, true)
      if (swapRef.current === carried) swapRef.current = undefined
    }
  }, [
    session?.state.authority,
    session?.state.slot,
    session?.state.viewing,
    zoomPrefs.onSwap,
  ])

  // O176: the load failed, so there is no reader to draw — `ReaderFailed`
  // says so and offers the way out. Here and not higher, for the reason every
  // hook above gives for being above: this is the first line after the last of
  // them. `!session` is part of the test so that a session, once there is one,
  // always draws. What is asked of `failure` is whether there is one, and
  // never what it holds: an `Error` may carry no message, and that one is
  // drawn with an empty alert.
  if (!session && failure !== undefined) {
    return (
      <ReaderFailed
        failure={failure}
        onLeave={onLeave}
        leavePhrase={leaveTo.phrase}
      />
    )
  }
  if (!session) return null
  // Not `work` — the prop already carries it, and `openWork(STARTUP_PORTS,
  // work)` is what makes `session.work` the same slug for the whole mounted
  // lifetime of this component (a changed slug remounts via `<App key={work}>`
  // rather than re-running this effect in place).
  const { resolver, reading, state, alignments, editions } = session
  // `editions` is already `orderedEditions`' output — `startup.ts` builds it
  // that way — so the hub is first and `withBorrow` needs no second ordering.
  const shown = withBorrow(
    display(state, resolver),
    state,
    resolver,
    editions,
    borrowPrefs,
  )

  // The one entry `<Settings>`'s `notes` map carries — `editions.length` is
  // exactly the count `lenderFor`'s candidates come from (`editions` minus
  // the edition being viewed minus the authority), so this is computed here,
  // beside `shown`, from the same `editions` `withBorrow` was just handed.
  // `borrowNote` returns `undefined` at three editions or more, which is
  // what keeps `borrow` out of this object entirely rather than in it with
  // an empty string.
  const borrowingNote = borrowNote(editions.length)
  const settingsNotes = borrowingNote
    ? { [borrowModule.id]: borrowingNote }
    : undefined

  // The page-drawing half of `Display`, or `undefined` while showing a gap.
  //
  // Narrowed here rather than by a branch that returns early (O25), which
  // would hand back `Gap` *instead of* the reader's own tree, so `Chrome`,
  // `Settings` and the mode toolbar would never render while a gap was showing
  // — the keys would fire, the state flip, and nothing appear. There is one
  // mount site, so a panel added later is reachable in a gap because there is
  // nowhere else to put it.
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

  // The filmstrip's click job while `pending` is set. The slot→`Position`
  // conversion itself is `correction.ts`'s `counterpartPick` — what stays here
  // is the `undefined`-`pending` guard `setPending`'s updater needs and
  // clearing a notice a previous pick or a refused write may have set.
  function pickCounterpart(slot: number) {
    setPending((prev) => (prev ? counterpartPick(prev, resolver, slot) : prev))
    setCorrectionNotice(undefined)
  }

  /**
   * Write `pending` — only reachable once `pending.b` is set, i.e. after
   * `pickCounterpart` above.
   *
   * The write itself, its 412 re-read and the three outcomes are
   * `correction.ts`'s `submitCorrection`. What is left here is what only a
   * component can do: the re-entry guard, the busy flag, the one
   * `resolver.dispose?.()` — which stays with the caller because a replacement
   * has to exist before the outgoing one is freed, and `submitCorrection`
   * deliberately never frees anything — and turning one outcome into state.
   */
  async function confirmCorrection() {
    // `submitting` guards re-entry as well as the exits: a second `Confirm`
    // click would otherwise start a concurrent write against the same `etag`,
    // and the loser of that race gets a `412` for a correction the reader
    // only asked for once.
    if (!pending || pending.b === undefined || etag === undefined || submitting)
      return
    setSubmitting(true)
    try {
      const outcome = await submitCorrection(
        work,
        etag,
        { ...pending, b: pending.b },
        CORRECTION_PORTS,
      )
      if (outcome.kind === 'refused') {
        setCorrectionNotice(outcome.notice)
        return
      }
      if (outcome.kind === 'stale') {
        // A failed re-read is logged rather than shown: the reader is told the
        // pairing moved under them, which is the fact they can act on.
        if (outcome.error !== undefined) console.error(outcome.error)
        // `pending` survives untouched — the re-read gets the reader a current
        // validator and resolver to confirm against, not a retried write.
        // Both or neither: the two arrive together from one document, and
        // installing a resolver beside the alignments of an older one is the
        // drift `alignmentsOf` closes.
        const { resolver: next, alignments: fresh, etag: freshEtag } = outcome
        if (next && fresh) {
          resolver.dispose?.()
          setSession((prev) =>
            prev ? { ...prev, resolver: next, alignments: fresh } : prev,
          )
          setEtag(freshEtag)
        }
        setCorrectionNotice(outcome.notice)
        return
      }
      // The outgoing resolver's `WasmSidecar` would otherwise be stranded —
      // see `resolver.ts`'s `Resolver.dispose` doc. Freed only now that a
      // replacement exists, not before.
      resolver.dispose?.()
      setSession((prev) => (prev ? applyLanded(prev, outcome) : prev))
      setEtag(outcome.etag)
      setPending(undefined)
      setCorrectionNotice(undefined)
    } finally {
      // In `finally`, not at the end of each arm: a throw from the success
      // path itself would otherwise leave the banner disabled with no way out
      // but a reload.
      setSubmitting(false)
    }
  }

  // The debounce built once at first render calls through here, so it always
  // reaches the current closure rather than the one that existed then.
  writeNotesLatestRef.current = (notes: Note[]) => writeNotes(notes)

  /**
   * Finish editing: send anything still pending, and put the page back.
   *
   * One function because there are three ways to arrive here — `Escape`,
   * a click on the background around the page, and the review list's own
   * close — and "finished" has to mean the same thing whichever was used.
   */
  function closeEditor() {
    saveNotesRef.current?.flush()
    setEditing(nothingOpen())
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
   * The write itself — including the stale-validator re-read and merge — is
   * `notesWrite.ts`'s `saveNotes`, which takes its two calls as ports so that
   * its 412 path can be tested at all. Whether there is anything to write at
   * all is that module's `writeGate`. What is left is what only a component
   * can do: the busy flag, and turning one outcome into the five pieces of
   * state a reader sees.
   *
   * **`notesBusy` is set once around the whole call.**
   */
  async function writeNotes(notes: Note[]): Promise<void> {
    const gate = writeGate(notesDoc, notesEtag)
    // A validator that never arrived, or one a write's response did not carry
    // back, is the end of saving for this session, and the reader has to be
    // told, or they would go on typing into an editor that could no longer
    // save a word of it. `writeGate` is where that decision and its wording
    // live and are tested.
    if (gate.kind === 'refuse') return setNotesNotice(gate.notice)
    if (gate.kind !== 'write') return
    setNotesBusy(true)
    try {
      const outcome = await saveNotesDoc(
        work,
        gate.etag,
        gate.doc,
        notes,
        NOTES_PORTS,
      )
      if (outcome.kind === 'refused') {
        // A failed re-read is logged rather than shown: the reader is told
        // about the refusal that started it, which is the one they can do
        // something about.
        if (outcome.error !== undefined) console.error(outcome.error)
        setNotesNotice(outcome.notice)
        return
      }
      setNotesDoc(outcome.doc)
      setNotesEtag(outcome.etag)
      // `written` and `merged` both mean this document is what the server
      // holds now, so both are a step a reader can come back to.
      setNotesHistory((h) => record(h, outcome.doc.notes, sameNotes))
      setNotesNotice(outcome.kind === 'merged' ? outcome.notice : undefined)
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
  // something behaviourally different (a per-edition flip, a per-volume
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
    setPrefsMap((map) => fieldChanged(map, registry, id, key, raw))
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
    // The registry lookup stays here and not only inside `sliceReset`, because
    // `resetPrefs` writes storage and must not run for an id no module claims.
    if (!registry.get(id)) return
    resetPrefs(id)
    setPrefsMap((map) => sliceReset(map, registry, id))
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
    const step = stepHistory(notesHistory, next)
    if (!step) return
    saveNotes.cancel()
    // The edits go, the panel stays. A reader stepping back does not want the
    // thing they just stepped back from laid on top of it; they have not asked
    // for the panel they are standing in to close.
    setEditing((open) => ({ ...NOTHING_OPEN, panel: open.panel }))
    setNotesHistory(step.history)
    void writeNotes(step.write)
  }
  const stepBack = () => stepTo(undo(notesHistory))
  const stepForward = () => stepTo(redo(notesHistory))
  stepBackRef.current = stepBack
  stepForwardRef.current = stepForward
  closeEditorRef.current = closeEditor

  // The review list is built whatever `display` resolved to. A gap has no slot
  // to draw notes on, but the work still has notes and a reader in a gap is
  // exactly who asks where they are. Every row in the list already carries its
  // own "not reachable from here" marking, which is the marking this state
  // exists for.
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

  // `editions[0]` is the hub — `orderedEditions` (`session.ts`) always places
  // it first, and `Session.editions` is built from exactly that. Bound here,
  // above the grouping: `alignedBreaks` needs it too, and two bindings of one
  // fixed value is one more place for them to disagree.
  const hub = editions[0]

  const slots = allSlots(resolver, shown.edition)
  // `shown.edition` — not `state.viewing` directly, though they're always
  // equal (see `reading.ts`'s `Display`) — because this is the edition the
  // page area is about to draw, which is what an override keyed by edition
  // must line up with.
  const effectiveReading = effectiveDirection(
    reading,
    directionPrefs.override[shown.edition],
  )
  // Compared, the edition after the one viewed stands beside it as a spread's
  // other page does: the page read in the cell its reading order puts first.
  const companion =
    layout === 'compare'
      ? editions[(editions.indexOf(shown.edition) + 1) % editions.length]
      : undefined
  const readCell = effectiveReading === 'rtl' ? 'second' : 'first'
  // Same `shown.edition` reasoning as `effectiveReading` above: breaks are
  // stored per volume and edition, under the edition whose slots `groups` is
  // about to pair.
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
  // no credit pages. `at?.slot` is the exemption: a reader who took a credit
  // page from the panel is standing on it deliberately, so that one page is
  // drawn while the rest stay set aside. See `skippedApartFrom` for what went
  // wrong without it.
  const skippedSlots = creditsPrefs.skip
    ? skippedApartFrom(creditSlotsIn(slots, creditPages), slots, at?.slot)
    : EMPTY_SLOTS
  // Where this edition has to restart its pairing so the pairs it draws are the
  // hub's, mapped across — see `alignedPairs.ts` for the solo groups it costs,
  // and `docs/notes.md` ("off by 1") for the measurement. Cached through a ref
  // rather than `useMemo` because every hook in this component has to sit above
  // the `if (!session) return null` far above, and all four of these are bound
  // below it; `alignedBreaksCached` is that memo written out, with tests.
  // `effectiveReading` is this edition's, which `groups` does not read; it is
  // threaded rather than dropped so that a grouping rule which does start
  // reading it cannot silently get the wrong one here.
  const derivedBreaks = alignedBreaksCached(
    alignedBreaksRef.current,
    resolver,
    hub,
    shown.edition,
    effectiveReading,
  )
  // One grouping, three surfaces: this array is what the page area draws,
  // what `groupedRef` gives the arrow keys, and what `Chrome` hands the
  // filmstrip. Passing `skippedSlots` into `groups` rather than filtering its
  // result is what makes "out of the reading flow" mean all three *and* work
  // in `two` mode — see `groups`' own doc for the measurement that settled
  // that.
  const groupedAs = (as: Mode, own: number[]) =>
    groups(
      slots,
      as,
      effectiveReading,
      pairingBreaks(own, derivedBreaks),
      skippedSlots,
    )
  const grouped = groupedAs(mode, breaksFor(regroupPrefs, work, shown.edition))
  // Assigned during render rather than in an effect: the keydown handler must
  // never step against a grouping older than what is on screen, and an effect
  // would leave exactly one frame where it could. Writing a ref during render
  // is safe here because it is derived state, not a subscription — the value
  // is recomputed from props and state every render, so a discarded render
  // simply writes the same thing the retained one will.
  groupedRef.current = grouped
  regroupRef.current = { layout: (own) => groupedAs('two', own), mode }
  // Assigned beside the grouping it was just used to build, so the arrow keys
  // and the page area can never disagree about what is set aside.
  skippedRef.current = skippedSlots
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
  // No `Math.max(…, 0)` clamp: it would turn `findIndex`'s `-1` into `0`, so
  // the page area would draw the volume's first group — the cover — while the
  // readout correctly reported where the reader was. `skippedApartFrom` is
  // what keeps `-1` from arising at all; this draws nothing rather than
  // something wrong if it ever does.
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
  //
  // The four of them come back together from `correction.ts`'s
  // `correctionView` rather than being derived one at a time here: they are
  // read together and they are wrong together, and the dashed marker landing
  // on a tile that names a different slot than the banner warns about is the
  // failure one call makes impossible.
  const correcting = pending && correctionView(pending, resolver)

  // The four readout lines, computed here from `modules/readouts.ts`'s pure
  // functions and handed to `Chrome` as plain strings — see that module for
  // every branch below; this is wiring, not derivation. `healthReadout` below
  // checks the edition on screen against the hub, not against wherever
  // `authority` happens to have wandered.
  // The notes on the slot being drawn. `visibleNotes` and `notesHere` are both
  // `notes.ts`'s — stored notes with this session's edits over them, filtered
  // to the viewed edition when `travel` is off, then resolved once per edition
  // that holds notes. Every reason for that order is in that file; nothing
  // about travel is decided here.
  const notesForHere = visibleNotes(
    allNotes,
    edits,
    annotationsPrefs.travel,
    shown.edition,
  )
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
  // And only on a page: `at` is what keeps a gap from paying one resolve per
  // note on every render to build a map nothing consumes — the page area draws
  // `Borrowed` there (or `Gap`, where nothing can lend), and neither mounts a
  // group to read it. A borrowed page is drawn solo in every mode, `scroll`
  // included, so there is no column of groups here even then.
  const scrolledNotes =
    at && mode === 'scroll'
      ? notesBySlot(notesForHere, at.edition, resolver)
      : new Map<number, NoteHere[]>()

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

  // Opening a note from its margin tab, shared by every mode's overlay.
  /**
   * Show a note in the editor, bringing it into this session's edits.
   *
   * Whatever is already in `edits` for that id wins over the stored copy, so
   * clicking back onto a note you moved a moment ago shows it where you left
   * it rather than where the server still has it.
   */
  function openNote(id: string) {
    const next = openNoteInDraft(editing, id, allNotes)
    // The same draft back means neither this session nor the document knows
    // that id, so there is nothing to show and nothing to clear the notice for.
    if (next === editing) return
    setEditing(next)
    setNotesNotice(undefined)
  }

  // The readout lines, from `modules/readouts.ts`'s `readoutLines` — the
  // `?? slot + 1` fallback for an edition the resolver cannot count, the
  // `Math.max(…, 0)` for a slot count that has gone backwards, the gap's
  // report-from-the-authority rule and the `gapHealth`-in-a-gap choice are all
  // in there, where a test reaches them. This is the clock, which only a
  // component has.
  const readouts = readoutLines(
    shown,
    resolver,
    alignments,
    hub,
    { here: here.length, total: allNotes.length },
    {
      startedAt: clock.startedAt,
      turns: clock.turns.current,
      now: Date.now(),
    },
  )

  // `pageLongEdge`'s question for a **borrowed** page, and neither of its two
  // inputs is right for one — which is why this is a second binding rather than
  // a reuse of `pageLongEdge`.
  //
  // `1 / view.w` would be a magnification the page can never be drawn at.
  // `swapStep` answers `drop` when the swap lands somewhere with nothing drawn,
  // and `drop` does not call `setView` — so a reader who was zoomed to 4× when
  // they swapped onto this arrives with `view` still at 4×, on a page `zoomable`
  // refuses to engage on. They cannot zoom back out to undo it. Multiplied
  // through, that walks up the ladder and, past its top rung, `requestedLongEdge`
  // answers `undefined` — **the original bytes**, the whole page at full
  // resolution, for a page drawn fit-whole. `1` is what it is actually drawn at.
  //
  // `'single'` rather than `mode` for the mirror reason: `pageBoxLongEdge`
  // halves the width in `'two'` because each cell there holds one page, and a
  // borrowed page is drawn **solo across the whole page area in every mode** —
  // `App.tsx` tests this arm before it consults `mode` at all. Passing `mode`
  // would ask for a box half the size of the one it is drawn into, which is the
  // one direction `pageSize.ts` refuses to err in: erring high costs a rung of
  // decode, erring low is a visibly soft page.
  const borrowedLongEdge = requestedLongEdge(
    pageBoxLongEdge(viewportWidth, window.innerHeight, 'single'),
    window.devicePixelRatio,
    1,
  )
  // `pageUrl` takes a page index, not a slot index — they diverge from the
  // first joined spread onward, so every slot is converted through the
  // `Resolver`-derived `slots` array before it reaches a URL.
  const pageAt = (slot: number) =>
    pageUrl(work, shown.edition, slots[slot].page, pageLongEdge)
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
  /**
   * Which edge of its own element a slot's artwork is pulled to, asked by slot
   * rather than by cell.
   *
   * The translation from a cell to an edge is `modules/fit.ts`'s `hugForCell`,
   * where it can be tested and where the naming argument lives. What is left
   * here is the guard that needs bindings below `if (!session) return null`:
   * `'centre'` in single and scroll mode, and for a slot with no group on
   * screen at all. Nothing hugs in any of those, so nothing is off-centre in
   * them either.
   */
  const hugFor = (group: Group | undefined, slot: number): ArtworkHug =>
    companion
      ? hugForCell(readCell)
      : mode !== 'two' || !group
        ? 'centre'
        : hugForCell(cellOf(group, effectiveReading, slot))

  /** The hug for the slot the reader is on, which is the one the overlay draws over. */
  const shownHug = at ? hugFor(current, at.slot) : 'centre'
  drawnRef.current =
    at && current && mode !== 'scroll'
      ? {
          url: pageAt(at.slot),
          cell: companion
            ? readCell
            : mode === 'two'
              ? cellOf(current, effectiveReading, at.slot)
              : 'only',
          half: slots[at.slot].half,
          // Carried so the swap measures the page where it is drawn rather
          // than where a centred page would be (see `Drawn.hug`).
          hug: shownHug,
        }
      : undefined

  // Which mechanism governs a mounted `PageView`'s image — see that
  // component's own `buffered` doc for the full reasoning. Scroll mode
  // mounts every group at once and leans on `loading="lazy"` to bound that
  // burst; decode-gating every one of them the instant it mounted would
  // force them all to fetch regardless of that attribute, trading a
  // (measured-ineffective, `docs/status.md`) mitigation for a guaranteed-
  // eager one. Single and two-page mode mount only the current group, so
  // there is nothing for `loading="lazy"` to defer there — see
  // `PageView.tsx`'s comment on that — and decode-gating is what closes the
  // flicker on the page turn itself. `prefetchPrefs.enabled` gates it too:
  // disabling the module must revert `PageView` to exactly its behaviour
  // without decode-gating, since that reversion is what makes the flicker
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
   * **The hug is what `paintedBox` cannot infer from the two rects.** Left to
   * its default, that function splits the letterbox evenly, which is what
   * `object-fit: contain` does under `object-position`'s own `50% 50%`. The two
   * cells of a real pair pass a hug instead, so their art sits flush against
   * the seam — and a drag measured as though it were centred would land the
   * whole rectangle sideways of where it was swept out (`paintedBox` has the
   * measurements). Nothing changes where the fit is width-bound and there is no
   * horizontal letterbox left to distribute.
   *
   * `el` always comes from `draftImage`, which returns nothing without a draft,
   * so the `'centre'` below is unreachable rather than a third case. Written
   * out rather than asserted on: the cost of being wrong here is a region off
   * from where it was drawn, not a crash, and this file would rather draw the
   * centred page than throw at a reader mid-drag.
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

  /** Whether a point in page fractions is on the painted page at all. */
  const inside = (at: { x: number; y: number }) =>
    at.x >= 0 && at.x <= 1 && at.y >= 0 && at.y <= 1

  /**
   * The two pixel points a drag is made of, in page fractions.
   *
   * The rule over them is `notes.ts`'s `regionUnderDrag`; this is the DOM
   * reading it cannot do.
   */
  function regionUnderPointer(
    started: DragStart,
    event: React.PointerEvent,
  ): Region {
    return regionUnderDrag(
      started.intent,
      pointOn(started.el, started.x, started.y),
      pointOn(started.el, event.clientX, event.clientY),
      started.from,
    )
  }

  /**
   * What a press on the page area does. The rule is `notes.ts`'s `gestureFor`,
   * which is where it can be tested; this reads the DOM for it and carries out
   * whichever answer comes back.
   */
  function beginRegion(event: React.PointerEvent) {
    // The press, for `gestures.ts` to read when it comes up. Recorded here
    // rather than measured again on release: where a press went *down* is
    // what a reader aimed, and the page area's box can change under a
    // gesture (the chrome does not resize it, but an orientation change
    // does).
    const box = event.currentTarget.getBoundingClientRect()
    pressRef.current = {
      x: event.clientX - box.left,
      y: event.clientY - box.top,
      t: event.timeStamp,
      // Read from the *down* target: a press that began on a control was
      // never a page gesture, whatever it is over when it ends.
      onControl: isInteractivePress(event.target),
      consumed: false,
      // Which pointer this is, so `endPress` can refuse a release that
      // belongs to a different one — a second finger's in a multi-touch
      // press, or an unrelated `pointerup` that reaches the page area after
      // this one's own release already went missing (`onPointerLeave`'s
      // doc, below).
      pointerId: event.pointerId,
    }
    // Whether this press pans is `notes.ts`'s `pressTakesPan`, whose doc
    // carries the whole argument — why `zoomable` is one flag and not two, why
    // a control is excluded rather than the page required, and what the
    // ancestor-aware `isInteractivePress` is for. All four arguments are read
    // from the event and from state; none of the reasoning is here.
    //
    // No `preventDefault` in this branch, deliberately, though the region drag
    // below calls one to stop the browser's text selection fighting the
    // rectangle. A prevented `pointerdown` also stops focus moving, and this
    // runs on every press on the page rather than only while the editor is
    // open: a reader who clicked a mode button and then dragged the page would
    // leave focus on that button, where a key the button keeps would go to it
    // instead of the page.
    const panning = pressTakesPan({
      zoomable: zoomable(zoomPrefs, drawnRef.current !== undefined),
      onControl: isInteractivePress(event.target),
      editing: panelMode === 'edit',
      shift: event.shiftKey,
    })
    if (panning) {
      panRef.current = { x: event.clientX, y: event.clientY }
      return
    }
    const el = draftImage(event.target)
    // On an image, and on the *painted* page rather than in the letterbox
    // `object-fit: contain` leaves inside the same element. The `<img>` fills
    // the page area at every size tested, so "on an image" alone is true of
    // every press in the reader and `gestureFor`'s rule 1 — looking away
    // finishes the note — could never fire. `pointInSlot` is unclamped for
    // exactly this reason.
    //
    // `pointOn` only makes sense against the image it was given: `el`'s own hug
    // (`hugFor(current, draft.slot)`), painted inside `el`'s own rect. In `two`
    // mode `event.target` is often the *facing* cell — a sibling image with the
    // opposite hug and, in general, a different rect — so judging it against
    // the draft's painted box is a wrong frame of reference, not an
    // approximation: it can place genuine artwork outside `0..1` and genuine
    // letterbox inside it. A press on any image that is not the draft's own is
    // on the page regardless of where it falls, because rule 2
    // (`onTheDraftsPage: false`) answers `ignore` for the whole of it —
    // `onAnImage` only has to be right about images `pointOn` was not given.
    //
    // Before the image has decoded, `naturalWidth` is 0 and `paintedBox` hands
    // back the whole element box, so the fraction `pointInSlot` computes lands
    // in `0..1` for any press the element could have received — `inside` reads
    // it as on the page, as there is no letterbox to be in yet. (In jsdom,
    // where an unmeasured element's box is zero-area as well, the fraction is
    // `{x: 0, y: 0}` by `pointInSlot`'s own zero-box arm; the answer is the
    // same by a different route, which is why no test can tell the two apart.)
    const onPage =
      event.target instanceof HTMLImageElement &&
      (el === undefined || inside(pointOn(el, event.clientX, event.clientY)))
    const gesture = gestureFor({
      editing: panelMode === 'edit',
      onAnImage: onPage,
      onTheDraftsPage: el !== undefined,
      at: el ? pointOn(el, event.clientX, event.clientY) : { x: -1, y: -1 },
      here,
      slop: GRAB_SLOP,
    })

    if (gesture.kind === 'close') {
      closeEditor()
      // Spent: this press has done its work, and its release must not also
      // turn a page (`gestures.ts`'s `consumed`).
      if (pressRef.current) pressRef.current.consumed = true
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
      setEditing((open) => grabNote(open, held))
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
      // The wheel handler's zero-box guard, for the same reason — and here it
      // also gates the `panRef` write below, so a frame measured against a
      // box mid-teardown neither moves the rect nor becomes the origin the
      // next frame steps from.
      if (box.width <= 0 || box.height <= 0) return
      // The frame itself is `modules/zoom.ts`'s `panStep`, whose doc has the
      // sign convention and the `Object.is` bail-out this depends on.
      setView((rect) =>
        panStep(rect, box, from, { x: event.clientX, y: event.clientY }),
      )
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
    // `anchorFrom` is the click-plants-a-point rule; the point it plants is
    // where the press went down, which only the DOM can say.
    const anchor = anchorFrom(
      started.intent,
      regionUnderPointer(started, event),
      pointOn(started.el, started.x, started.y),
    )
    const id = started.noteId ?? draft.id
    const now = new Date().toISOString()
    const anchorOf = (edition: string, slot: number) =>
      sessionRef.current?.resolver.memberPage(edition, slot)
    setEditing((open) => {
      const next = anchorNote(open, id, anchor, now, anchorOf)
      // The same draft back means this session never had that note to anchor,
      // so there is nothing to write either.
      if (next === open) return open
      scheduleSave(next.edits)
      return next
    })
  }

  /**
   * A pointer that left the page area: finish whatever it was doing, the same
   * as `endRegion` alone, and drop the press record with it, when it is this
   * pointer's own record to drop.
   *
   * A pointer that left the area finished no *gesture*, which is why this stays
   * off `gestureOfPress` and `endPress`'s classification entirely. But leaving
   * `pressRef.current` set behind it would leave a stale record for a later
   * `pointerup` to be paired with — see `endPress`'s own doc for what that
   * causes. **Gated on `pointerId`, the same as `endPress` and
   * `onPointerCancel`**, so that clearing it cannot drop a record that was not
   * this pointer's own to drop.
   *
   * It is not reachable for a *second* finger's leave to drop a *first*
   * finger's record — `beginRegion` assigns `pressRef.current` unconditionally
   * on every `pointerdown`, so a second finger's own press has already
   * overwritten the first finger's record before either one's leave or cancel
   * can run; there is no earlier record left by then for a later leave to drop.
   * What *is* reachable, and what this guard actually repairs, in a multi-touch
   * press: finger A down (record = A); finger B down (record = B, overwriting
   * A); A lifts — `endPress` refuses it, the ids do not match, B's record is
   * left alone; **A's own `pointerout`/`pointerleave`, arriving right after A's
   * own release, would wipe the slot if it cleared unconditionally** — dropping
   * B's still-open record, though it was never A's own to drop — so B's later
   * release would find nothing to pair itself with. The guard below stops
   * exactly that: A's leave carries A's `pointerId`, which by then no longer
   * matches the record it would be clearing.
   */
  function leaveRegion(event: React.PointerEvent) {
    endRegion(event)
    if (pressRef.current?.pointerId === event.pointerId) {
      pressRef.current = null
    }
  }

  /**
   * A press released on the page area: finish whatever it was doing, then ask
   * `gestures.ts` what it meant.
   *
   * Bound to `onPointerUp` only, and only for the pointer that began the press
   * it is about to classify. **The bars are siblings of the page area, drawn
   * over it, not children of it** — so a press that goes down on the page and
   * is released over a bar reaches no `onPointerUp` here at all *with a mouse*,
   * where the release lands wherever the pointer really is; `onPointerLeave`
   * sees it instead. **A touch pointer does not take that path.** Implicit
   * pointer capture — the same mechanism `Settings.tsx`'s own scrim guard has
   * to work around — delivers `pointerup` to the page area regardless of where
   * the finger actually lifts, bar or no bar, so this function does run for
   * that release on a touch screen. A release this close to its own down is not
   * travel for `swipe` to judge at all — it is a *tap* (rule 3 of spec §9.2,
   * `Math.abs(dx) <= TAP_SLOP && Math.abs(dy) <= TAP_SLOP && …TAP_HOLD`),
   * classified against where the press went *down*, not where it came up. A
   * finger that lands on the page a few pixels above the bar and lifts onto it
   * is exactly such a tap, so it **turns or toggles**, the same as if it had
   * lifted where it landed. Only a longer travel reaches `swipe` at all, and a
   * slip onto a bar is refused there by `SWIPE_MIN`: it moves ~0 px *across*
   * the page, so `swipe` returns `none` on `Math.abs(dx) < SWIPE_MIN` before
   * the axis test is ever evaluated — that test only decides a drag already at
   * least `SWIPE_MIN` across. Zoomed in it never reaches that line at all,
   * because `!press.atWhole` refuses it on the guard above. The browser's own
   * touch-scroll slop is what ends the window, and `TAP_SLOP` is 10, so every
   * slip this rule calls a tap is well short of it and is never cancelled
   * first. **That slop's figure and its emulation are in O142**
   * (`docs/status.md`), rather than here: it is a browser constant under a
   * browser version nobody pins, and a source comment is not where anyone would
   * re-measure it. **The mouse does not share this**: its `pointerup` lands
   * wherever the cursor really is — over the bar — so `onPointerLeave` sees it
   * instead of this function, and the same slip does nothing. This follows from
   * "where the press went down decides", §9.2's own rule, rather than being a
   * bug in it — filed as O142 (`docs/status.md`) rather than changed, since a
   * reader who aimed at the outer third and slipped onto the bar asked for a
   * turn and gets one.
   *
   * `leaveRegion` (above) is what a release over a bar means for the record on
   * a mouse, where that path is actually taken: it clears `pressRef.current`,
   * so a *later*, unrelated `pointerup` on the page area — one with no
   * `pointerdown` of its own — finds nothing to pair itself with rather than
   * being classified against wherever the abandoned press began.
   * `onPointerCancel` clears it too, for the same reason, when the browser
   * takes the pointer away instead of a release ever arriving.
   *
   * **All three — this function, `leaveRegion` and `onPointerCancel` — gate
   * that clear on `event.pointerId`.** Two presses can be live at once (a
   * second finger, mid-drag, in a multi-touch press), and only one `pressRef`
   * slot exists to hold either. A release, a leave or a cancel that is not the
   * one this record belongs to must leave the record alone — it may yet be
   * finished by its own pointer's `pointerup` — rather than being spent on an
   * id it was never a press of.
   */
  function endPress(event: React.PointerEvent) {
    endRegion(event)
    const started = pressRef.current
    if (!started || started.pointerId !== event.pointerId) return
    pressRef.current = null
    const box = event.currentTarget.getBoundingClientRect()
    const gesture = gestureOfPress({
      down: started,
      up: {
        x: event.clientX - box.left,
        y: event.clientY - box.top,
        t: event.timeStamp,
      },
      width: box.width,
      onControl: started.onControl,
      consumed: started.consumed,
      editing: panelMode === 'edit',
      panelOpen:
        helpVisible ||
        settingsVisible ||
        creditsVisible ||
        panelMode === 'review',
      scrolling: mode === 'scroll',
      // By value: `clampRect` allocates on every turn, so identity with
      // `WHOLE` would be lost after the first one (`viewport.ts`'s `isWhole`).
      atWhole: isWhole(view),
    })
    switch (gesture.kind) {
      case 'none':
        return
      case 'toggle-chrome':
        return setChromeVisible((visible) => !visible)
      case 'close-panels':
        // Every one that is open, so no precedence between them has to be
        // invented. `toggleReview` only has a `'review'` arm to close here,
        // because `'edit'` was answered by `gestures.ts` before this.
        setHelpVisible(false)
        setSettingsVisible(false)
        setCreditsVisible(false)
        if (panelMode === 'review') setEditing(toggleReview)
        return
      case 'turn':
        // Physical, as the keys are. `readerIntent` is the one caller that
        // puts a step through `stepFor`, so the mirroring is inherited.
        return readerIntentRef.current({ kind: 'step', by: gesture.by })
    }
  }

  /**
   * One group, drawn — `GroupView.tsx`, which is where the reconciliation
   * invariant that keeps `PageView`'s decode gate alive across a turn lives,
   * with a test over it.
   *
   * `drawnSrcs` stays here rather than moving inside that component. It is
   * written by the two `PageView` callbacks this passes and read by nothing
   * else, which reads like component state — but the two call sites below are
   * in different arms of the same ternary, so a reader going two -> scroll ->
   * two unmounts and remounts everything at that position. As `App` state this
   * record survives that; as component state it would come back empty and the
   * layout would key on the target for as long as it took both cells to report,
   * which is the flicker the record exists to stop.
   */
  const drawGroup = (group: Group) => (
    <GroupView
      group={group}
      mode={mode}
      reading={effectiveReading}
      src={pageAt}
      alt={altAt}
      hug={hugFor}
      buffered={buffered}
      decodeQueue={swapQueue}
      cohort={pageCohort}
      fit={fitPrefs}
      drawn={drawnSrcs}
      onShown={showDrawn}
    />
  )

  // The gap's offered moves, and where the strip should sit. Computed in a
  // borrowed state as well as in a gap: the mark reveals the same offers, and
  // the filmstrip still anchors on `stripAnchor` — a reader looking at a
  // borrowed page is standing in exactly the place a gap would have put them,
  // and the page under them being drawn from elsewhere does not make the
  // nearest real page of their own edition any less the way out. The scan is
  // bounded by the authority's slot count so a wholly unpaired edition costs
  // one scan rather than one per render.
  //
  // `skippedSlots` is passed for O40: these are the buttons the arrow keys
  // take, so a set consulted by one and not the other would have the screen
  // naming a destination the key refuses. `session.ts`'s `step` case is given
  // the same set through `skippedRef`, and its doc says so from the other end.
  const gapMoves =
    shown.kind === 'gap' || shown.kind === 'borrowed'
      ? nearestPaired(
          state,
          resolver,
          resolver.slotCount(state.authority) ?? state.slot + 1,
          skippedSlots,
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
  // One `<Gap>`, built from whichever arm is showing: rendered directly in a
  // gap, and handed to `Borrowed` as the thing its mark reveals. Two call
  // sites constructing the same seven props is the shape that lets them
  // drift.
  const gapView = (
    edition: string,
    from: { edition: string; slot: number },
  ) => (
    <Gap
      reading={effectiveReading}
      edition={edition}
      from={{
        edition: from.edition,
        page: resolver.slotAt(from.edition, from.slot)!.page + 1,
      }}
      verdict={verdictFor(alignments, from.edition, edition)}
      before={toMove(gapMoves?.before)}
      after={toMove(gapMoves?.after)}
      onTake={take}
    />
  )
  // The companion's page for the place the reader is at is found as a swap to
  // it would find it, and a gap is said.
  const companionPage = (edition: string) => {
    const there = display(swap(state, edition), resolver)
    const at =
      there.kind === 'at' ? resolver.slotAt(edition, there.slot) : undefined
    const label = labelOf(detail, edition)
    return at ? (
      <PageView
        src={pageUrl(work, edition, at.page, pageLongEdge)}
        alt={`${label}, page ${at.page + 1}`}
        hug={hugForCell(readCell === 'first' ? 'second' : 'first')}
        buffered={buffered}
        decodeQueue={swapQueue}
        cohort={pageCohort}
        fit={fitPrefs}
      />
    ) : (
      <p className="compared-none">{label} has no page here.</p>
    )
  }
  // The borrowed page, and the one `slotAt` all three of its page values are
  // built from. `Borrowed` wants the lender's page 1-based for the mark and the
  // `alt`, 0-based for the URL, and its `half` beside both — and asking the
  // resolver three times for one slot is the shape that lets those answers
  // disagree.
  //
  // There is one arm `lenderFor` can return through, and the assertion holds
  // for it: a resolved lender's slot came back from `indexOf`, which only
  // answers when the position it was asked about actually exists. Taken as
  // parameters rather than read off `shown`, so the narrowing the caller has
  // already done is what types them — `gapView`'s shape, immediately above, for
  // the same reason.
  const borrowedView = (
    edition: string,
    from: { edition: string; slot: number },
    lender: { edition: string; slot: number },
  ) => {
    const { page, half } = resolver.slotAt(lender.edition, lender.slot)!
    return (
      <Borrowed
        // For the swap that goes straight from one borrowed position to
        // another — two editions both lacking the slot under the reader —
        // which is the only *move* that leaves this mounted and changes what
        // it is showing. Every other move unmounts it and takes the mark's
        // toggle with it; see `Borrowed`'s own doc for why that covers the
        // rest.
        //
        // **A move is not the only way, and this key does not cover the other
        // one.** Replacing the resolver answers a different lender for the same
        // three values: `wasmResolver`'s own doc names both paths — a
        // correction landing, and a stale-`ETag` re-read — and either can carry
        // a document where this slot now resolves elsewhere. The key would be
        // identical while `src` changed, under a `buffered={false}` `PageView`
        // whose own comment says no mounted instance ever sees that happen.
        //
        // Unreachable today, and by a guard rather than by luck: both paths run
        // inside `correction.ts`'s `commitCorrection`, which only a pending
        // correction reaches, and `beginCorrection` above returns unless
        // `display` answers `at` — which a borrowed page does not. Written down
        // rather than fixed because the fix has a price: adding the lender to
        // the key remounts the borrowed page on every re-read, discarding a
        // decoded image and the mark's own toggle for a document that usually
        // says the same thing. Whoever makes a correction reachable from this
        // state pays it then.
        key={`${state.authority}:${state.slot}:${edition}`}
        lender={{ edition: lender.edition, page: page + 1, half }}
        src={pageUrl(work, lender.edition, page, borrowedLongEdge)}
        fit={fitPrefs}
        offers={gapView(edition, from)}
        viewing={edition}
        reading={effectiveReading}
        before={toMove(gapMoves?.before)}
        after={toMove(gapMoves?.after)}
        onTake={take}
      />
    )
  }
  // The tile the strip centres on while showing a gap or a borrowed page: the
  // nearest page that exists, which is the same page `Gap`'s own button
  // offers. `before` first — a reader who has just walked forward into a gap
  // came from behind it. Borrowed included for the reason `gapMoves` above
  // gives: the page drawn there belongs to another edition, so it is no more a
  // tile in this strip than a gap was.
  const stripAnchor = gapMoves && (gapMoves.before?.at ?? gapMoves.after?.at)

  // What the top bar says — `readerTitle.ts`.
  const heading = titleLines(detail, shown.edition)

  return (
    // `position: relative` is what `Chrome` positions itself against —
    // `position: absolute` removes it from this flex column's layout flow
    // entirely, so it can overlay the page area without the page area's own
    // size changing when the bars mount or unmount. A `<main>` (O163): the
    // reader's one landmark.
    <main
      className="reader"
      ref={rootRef}
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100%',
        position: 'relative',
      }}
    >
      {/* O163, O173: the reader's heading and its tab's title, here rather
          than in the top bar, which is not drawn while the bars are down. */}
      <ReaderHeading name={readerName(heading)} />
      <div
        className="page-area"
        style={{
          flex: '1 1 auto',
          minHeight: 0,
          overflow: mode === 'scroll' ? 'auto' : 'hidden',
          // The browser's double-tap zoom, and nothing else. Without this a
          // double tap on a phone turns two pages *and* zooms the document;
          // with it, two taps turn two pages, which is what they say. Pinch
          // and scrolling are untouched — `touch-action: none` would take
          // both, and §9.2 keeps the pinch the browser's.
          touchAction: 'manipulation',
          // O430: a double click, or two quick turning clicks, would otherwise
          // select a gap's sentence, or in Firefox the page image, and
          // `::selection` tints the page with the accent. Nothing in the comic
          // reader reads a selection. Here and not on `.reader`, which also
          // holds the book reader's text and the comic reader's panels, both
          // selectable. The prefixed one is for Safari.
          userSelect: 'none',
          WebkitUserSelect: 'none',
          // What `Annotations`' `inset: 0` is measured against — the page area,
          // not the whole reader. The two share a top edge; they differ at the
          // bottom, where the root also holds the note list placed below the
          // page in flow (`<NoteList placement="below">`, after this page area)
          // — a margin tab measured against the root could reach into that list
          // instead of stopping at the page.
          position: 'relative',
          // Scroll mode's `scrollIntoView({ block: 'start' })` (`hooks.ts`'s
          // `useScrollToCurrent`) puts the current group's top at the page
          // area's own top edge — which the top bar draws over while it is
          // shown. `scrollPaddingTop` moves the *target* down to clear the bar
          // without moving the page area itself, so its size stays what O112
          // needs it to stay. Conditional on `chromeVisible` rather than set
          // unconditionally: with the chrome hidden there is no bar to clear,
          // and a fixed offset would move the scroll target for no reason the
          // moment the bars come down.
          scrollPaddingTop: chromeVisible
            ? 'calc(var(--safe-top) + var(--reader-top-bar))'
            : undefined,
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
        tabIndex={-1}
        data-reader-page=""
        onPointerDown={beginRegion}
        onPointerMove={trackRegion}
        onPointerUp={endPress}
        // A pointer released outside the page area must not leave a
        // rectangle stranded on screen with no drag behind it, or a press
        // record behind it for a later, unrelated release to be paired with
        // — `leaveRegion`'s own doc has the second half.
        onPointerLeave={leaveRegion}
        onPointerCancel={(event) => {
          panRef.current = null
          dragRef.current = null
          // Gated on `pointerId`, the same as `leaveRegion` and `endPress`
          // — see `leaveRegion`'s own doc for the reachable multi-touch
          // sequence this guards. Not a second finger's cancel dropping a
          // first finger's record: `beginRegion` overwrites the one shared
          // slot unconditionally on every `pointerdown`, so an earlier
          // finger's record is already gone before a later finger could
          // cancel anything. What is reachable is the mirror of
          // `leaveRegion`'s case — a finger whose own record has since been
          // superseded by another's press clearing, on its own cancel, the
          // record that by then belongs to that other, still-open press.
          if (pressRef.current?.pointerId === event.pointerId) {
            pressRef.current = null
          }
          setDragging(undefined)
        }}
      >
        {/*
          Single and two-page mode draw one group, so one overlay over the
          page holding the slot being read. Scroll mode mounts every group at
          once and gets one overlay per group instead — see `drawGroup`'s
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
            // else keeps the default. Derived from the hug rather than asking
            // `cellOf` a second time, so the two cannot come to different
            // conclusions about which cell this is.
            tabSide={shownHug === 'right-edge' ? 'left' : 'right'}
            // The live rectangle while dragging, and the settled one after the
            // pointer is released — so a reader can see what they drew, and
            // see it again when they reopen the note. Only while the editor is
            // open on *this* slot.
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
          Beside the page, inside the page area. Being inside it does not keep
          the list clear of the top bar, since the two share a top edge: that
          is left to where `NoteList`'s side style docks it. Only while
          revealed: the list is a way to *read* the notes, and `r` is the key
          that asks for that.
        */}
        {at &&
          annotationsPrefs.reveal &&
          notesPlacement === 'side' &&
          mode !== 'scroll' && (
            <NoteList here={here} placement="side" onOpen={openNote} />
          )}
        {shown.kind === 'gap' ? (
          gapView(shown.edition, shown.from)
        ) : shown.kind === 'borrowed' ? (
          borrowedView(shown.edition, shown.from, shown.lender)
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
                {drawGroup(g)}
                {/*
                  One overlay per mounted group, so a note is on its page while
                  scrolling exactly as it is in the other two modes. The notes
                  for every slot are resolved once for the whole edition
                  (`notesBySlot`) rather than once per group, which is one
                  resolve per note instead of one per page — resolving per group
                  would scan the note list once per page.
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
          current &&
          // Scroll mode's branch above mounts every group at once and keeps
          // fit-whole, which is the spec's scope decision rather than an
          // oversight. `WHOLE` while the module is disabled rather than
          // unmounting the wrapper, so turning zoom off does not change what
          // React reconciles here — a changed element type at this position
          // is what `cellsFor`'s doc records forcing a remount and throwing
          // away `PageView`'s decode gate.
          (companion ? (
            <Viewport rect={zoomPrefs.enabled ? view : WHOLE}>
              <Compared
                reading={effectiveReading}
                fit={fitPrefs}
                companion={companionPage(companion)}
              >
                {drawGroup(current)}
              </Compared>
            </Viewport>
          ) : (
            <Viewport rect={zoomPrefs.enabled ? view : WHOLE}>
              {drawGroup(current)}
            </Viewport>
          ))
        )}
        {/*
          Meant as VoiceOver's way to turn a page (spec 2026-09-26 §3.4) —
          not confirmed on a device (O162). Not while the
          reader's keys are stood down — the note editor, where a press on
          the page draws a region, and Account — and not in scroll mode,
          where they would scroll away with the column and VoiceOver's swipe
          already moves from page to page.
        */}
        {mode !== 'scroll' && panelMode !== 'edit' && !accountOpen && (
          <TurnButtons
            reading={effectiveReading}
            onStep={(by) => readerIntentRef.current({ kind: 'step', by })}
          />
        )}
      </div>
      {at && annotationsPrefs.reveal && notesPlacement === 'below' && (
        <NoteList
          here={here}
          placement="below"
          onOpen={openNote}
          covered={chromeVisible}
        />
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
        position={readouts.position}
        time={readouts.time}
        chapter={readouts.chapter}
        health={readouts.health}
        notes={readouts.notes}
        progress={readouts.progress}
        title={heading.title}
        editionLine={heading.editionLine}
        series={detail?.series ?? undefined}
        layouts={editions.length > 1 ? PAGE_LAYOUTS_COMPARED : PAGE_LAYOUTS}
        mode={layout}
        onMode={setMode}
        settingsOpen={settingsVisible}
        onSettings={() => setSettingsVisible((open) => !open)}
        // Gated as `?` and `h` are (`keyplan.ts`'s `helpEnabled`).
        onHelp={
          helpPrefs.enabled
            ? () => setHelpVisible((shown) => !shown)
            : undefined
        }
        helpOpen={helpVisible}
        work={work}
        edition={pending ? pending.to : shown.edition}
        slots={correcting ? correcting.slots : slots}
        groups={correcting ? correcting.tiles : grouped}
        // No group of `pending.to`'s own is "current" while correcting — the
        // reader is still looking at `pending.from`, not `to` — so nothing
        // should draw the solid "where you are" border there. `pairedIndex`'s
        // dashed one is the only marker while this is unset.
        currentIndex={pending ? -1 : currentIndex}
        anchor={stripAnchor}
        onNavigate={goToSlot}
        correcting={
          pending && correcting
            ? { to: pending.to, pairedIndex: correcting.pairedIndex }
            : undefined
        }
        onPickCounterpart={pending ? pickCounterpart : undefined}
        onLeave={onLeave}
        leaveWord={leaveTo.word}
      />
      {pending && (
        <CorrectionBanner
          pending={pending}
          displaced={correcting?.displaced}
          notice={correctionNotice}
          submitting={submitting}
          onConfirm={() => void confirmCorrection()}
          onCancel={() => {
            setPending(undefined)
            setCorrectionNotice(undefined)
          }}
        />
      )}
      {/*
        One panel, both states. `'edit'` is unreachable in a gap by two guards.

        **They are complementary, not redundant — neither alone is enough**,
        because they cover disjoint routes. `NEW_INTENT` returns early on
        `shown.kind !== 'at'`, which stops an editor being *opened* in a gap;
        `panelModeRef.current === 'edit'` in the dispatch guard stops an
        already-open one being *carried* into one. Remove the second and this is
        reachable: open an editor on a page, let a refused save drop focus to
        `<body>` (a state the dispatch guard's own comment records), then press
        Space. `toggle` calls `swap`, which changes only `viewing`, and on a
        slot the viewed edition has no counterpart for that is a gap — with
        `mode='edit'` rendering beside `Gap`, on a draft anchored to an edition
        no longer on screen, its region drag dead because `draftImage` requires
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
        // point of an edit mode is that a reader can nudge three boxes, and a
        // write that sent only the last one they happened to click would
        // silently drop the other two.
        onDelete={(id) => {
          // Deleting is deliberate and irreversible-looking, so it goes now
          // rather than in 700 ms — and it cancels whatever the debounce was
          // about to write, which would otherwise put the note straight back.
          saveNotes.cancel()
          setEditing((open) => afterDelete(open, id))
          void writeNotes(withoutNote(applyEdits(), id))
        }}
        onClose={() => {
          // Flush rather than drop: with no Save button, closing the editor is
          // not "abandon this", it is "I am done here" — and anything still
          // sitting in the debounce is a change the reader made and expects to
          // find next time. Undo is what abandons.
          saveNotes.flush()
          setEditing(nothingOpen())
          setNotesNotice(undefined)
        }}
        canUndo={canUndo(notesHistory)}
        canRedo={canRedo(notesHistory)}
        onUndo={stepBack}
        onRedo={stepForward}
        onGo={(entry) => {
          if (entry.here === undefined) return
          goToSlot(entry.here)
          // The panel closes; the edits stay, because going to a note is not
          // finishing with the ones already touched.
          setEditing((open) => ({ ...open, panel: undefined }))
        }}
      />
      {/*
        Sign-out is the gate's to act on, not this component's: it drops back to
        `<Login>` and unmounts everything under it, this reader included. The
        `?.` is the no-gate case — a test that mounted `App` on its own, where
        there is nothing above to sign out of. `main.tsx` puts every route under
        a gate, so a reader never meets it.
      */}
      <Account
        visible={accountOpen}
        onClose={() => {
          setAccountOpen(false)
        }}
        onSignedOut={() => gate?.signOut()}
      />
      {/*
        `borrowNote` takes `editions.length` alone, not `state` or
        `shown.edition` — a property of the work, not of the page on screen —
        so this map does not need to be recomputed as a reader turns pages,
        only as editions themselves change. `undefined` from `borrowNote`
        (three editions or more) leaves `settingsNotes` with no entry for
        `borrowModule.id`, and `Settings` renders that module's section with
        no note.
      */}
      <Settings
        visible={settingsVisible}
        modules={registry.all()}
        prefs={prefsMap}
        notes={settingsNotes}
        onChange={handleSettingChange}
        onReset={handleSettingReset}
        onClose={() => setSettingsVisible(false)}
        onAccount={openAccount}
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
          // the two spaces meet. A joined spread yields both its slots and the
          // first is where to land, since its two slots are one image and
          // `groups` puts that one first.
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
        mayCorrect={mayCorrect}
        putsSeriesUnder={putsSeriesUnder}
        onClose={() => setHelpVisible(false)}
      />
      {/*
        The one voice for where the reader stands (spec 2026-09-26 §3.5):
        always mounted, so a change is spoken, and polite, so it waits for
        whatever VoiceOver is saying.
      */}
      <div className="visually-hidden" aria-live="polite">
        {readouts.spoken}
      </div>
    </main>
  )
}

export default App
