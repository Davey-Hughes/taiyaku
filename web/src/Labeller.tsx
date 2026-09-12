import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { pageUrl } from './api'
import { debounce } from './debounce'
import { useConstant, useFlushOnHide, useWindowWidth } from './hooks'
import {
  type BoxDefaults,
  cycleAt,
  FIRST_BOX,
  type LabelBox,
  newBox,
  normaliseRect,
  type Point,
  type Rect,
} from './labelling'
import { keysOf, LABEL_KEYS } from './labelKeys'
import { loadPrefs, savePrefs } from './prefs'
import { pageBoxLongEdge, requestedLongEdge } from './pageSize'
import { PageView } from './PageView'
import { RegionOverlay } from './RegionOverlay'
import type { TruthDoc } from './truth'
import {
  boxesToRegions,
  commitSave,
  frameOfImage,
  openWork,
  pageRegions,
  pointOnPage,
  regionsToBoxes,
  type SidecarInfo,
  type TruthPorts,
  visitedPages,
  withPage,
  type PixelFrame,
} from './truthWrite'
import { messageOf } from './message'

/**
 * How long a burst of box edits — or of page flips, which also count, see
 * the persistence effect below — waits for quiet before reaching the server.
 * `App.tsx`'s `NOTES_DEBOUNCE_MS` picked for the same reason: this is
 * discrete edits rather than a position, so it sits with prose rather than
 * with the 400 ms reading position.
 */
const TRUTH_DEBOUNCE_MS = 700

/**
 * How far the pointer must move before a press becomes a drag, in client
 * pixels.
 *
 * **This is what makes a box selectable.** Capturing the pointer on
 * `pointerdown` — which the previous version did, so that a drag running off
 * the page still finished on the element that started it — retargets the
 * compatibility mouse events to the capturing element for the life of the
 * capture. `click` is one of them. So every click on a box was delivered to
 * the page instead of to the box, `RegionOverlay`'s `onSelect` never fired,
 * and nothing could be selected by pointer at all: the only way in was `Tab`,
 * which reads the cursor ref rather than a click. Reported from a labelling
 * session as "I can't edit any box I create".
 *
 * Capturing on the first *move past this distance* keeps both behaviours,
 * because the two gestures no longer overlap: a click never captures, and a
 * drag has always moved. The slop is what separates them — a mouse jitters by
 * a pixel or two under a real finger, and treating that as a drag would put
 * the bug straight back for anyone with an unsteady hand.
 */
const DRAG_SLOP = 4

/** Whether the pointer has moved far enough from the press for this to be a drag. */
function isDrag(
  pressed: { x: number; y: number },
  at: { clientX: number; clientY: number },
): boolean {
  return (
    Math.abs(at.clientX - pressed.x) > DRAG_SLOP ||
    Math.abs(at.clientY - pressed.y) > DRAG_SLOP
  )
}

/**
 * What the labeller remembers between sittings.
 *
 * Through `prefs.ts` rather than `localStorage` directly, which that module
 * states as a rule and is worth keeping true: it is the only writer, so no key
 * here is the first one nobody thought about on a shared origin.
 *
 * Where you were and what you were doing, and nothing about the work — the
 * labels themselves are server-side, because they must survive a different
 * machine, and this is the reader's own place in a long mechanical job.
 * Losing it costs a page lookup; losing a label costs the page again.
 */
const PREFS_ID = 'labeller'

interface LabellerPrefs extends BoxDefaults {
  /** `undefined` until a work has been opened once. */
  edition: string | undefined
  page: number
}

const NO_PREFS: LabellerPrefs = { ...FIRST_BOX, edition: undefined, page: 0 }

/** `App.tsx`'s `NO_WINDOW_WIDTH` — a desktop width, so a document-less environment gets the layout a real screen would rather than the narrowest one. */
const NO_WINDOW_WIDTH = 1280

export interface LabellerProps {
  initialBoxes?: readonly LabelBox[]
  initialSelected?: string
  /**
   * The server ports, and the work/edition/page picker they drive.
   *
   * `undefined` is what every keybinding and drag test in this file uses:
   * the standalone box editor, no fetch, no picker — the same shape
   * `PageView`'s own `decodeQueue`/`makeImage` default for a caller that has
   * not been wired up, so a test gets the plain behaviour rather than a
   * network call it never asked for. `label.tsx` is the one real caller and
   * always passes the real ports.
   */
  ports?: TruthPorts
}

/**
 * Draw text-run boxes on a page, and load and save them against a work's
 * truth document.
 *
 * A second entry rather than a mode inside `App.tsx`: the extraction map
 * already calls that file too large, and none of this state is a reader's.
 *
 * Coordinates are converted against the **rendered image**, never the
 * element around it or the window — `truthWrite.ts`'s `pointOnPage`, which is
 * `App.tsx`'s own `pointOn` reused rather than a second version of the same
 * arithmetic. `PageView` letterboxes the page inside its box, so a
 * conversion that stopped at the element would be off by the letterbox
 * margin at every aspect ratio but one.
 */
export function Labeller({
  initialBoxes = [],
  initialSelected,
  ports,
}: LabellerProps) {
  const [boxes, setBoxes] = useState<LabelBox[]>([...initialBoxes])
  const [selected, setSelected] = useState<string | undefined>(initialSelected)
  const pageRef = useRef<HTMLDivElement>(null)
  const dragStart = useRef<Point | undefined>(undefined)
  /** Where the press landed in client pixels, for telling a drag from a click. */
  const pressedAt = useRef<{ x: number; y: number } | undefined>(undefined)
  /**
   * Whether this gesture has passed `DRAG_SLOP` and become a drag.
   *
   * Gates all three of the preview, the capture and the box that gets
   * committed — a press that never moved must leave nothing behind. It used to
   * gate none of them, so a click on a box to select it also created a box two
   * pixels wide wherever the hand wobbled, and those specks go into the ground
   * truth like any other label.
   */
  const dragging = useRef(false)
  /**
   * Whether `work` and `edition` have settled once since mount.
   *
   * They arrive `undefined` and become real a round trip later, which the
   * page-reset effect below would otherwise read as the reader changing
   * edition.
   */
  const settled = useRef(false)
  /** The box being dragged right now, or `undefined` when nothing is. */
  const [drag, setDrag] = useState<Rect | undefined>(undefined)
  /**
   * The remembered slice, read once.
   *
   * Through `useConstant` rather than on each render, so a save made moments
   * later cannot feed back in and fight what is on screen.
   */
  const remembered = useConstant(() => loadPrefs(PREFS_ID, NO_PREFS))
  /**
   * What the next drawn box starts as: kind and reading axis both.
   *
   * A page is rarely a mixture in equal parts — a spread of sound effects is a
   * spread of sound effects, and a page of vertical Japanese is a page of it —
   * so carrying the last one forward removes a keystroke per box across the run
   * of them. It follows whatever the keys last did rather than being set by its
   * own control, so there is nothing extra to remember to change and nothing
   * that can disagree with the box on screen.
   */
  const [carried, setCarried] = useState<BoxDefaults>({
    kind: remembered.kind,
    axis: remembered.axis,
  })
  const cursor = useRef<Point>({ x: 0, y: 0 })

  // --- Selection: work, edition, page. Populated only when `ports` is given. ---
  const [works, setWorks] = useState<string[]>()
  const [work, setWork] = useState<string>()
  const [sidecarInfo, setSidecarInfo] = useState<SidecarInfo>()
  const [edition, setEdition] = useState<string>()
  const [page, setPage] = useState(remembered.page)
  const [doc, setDoc] = useState<TruthDoc>()
  const [etag, setEtag] = useState<string>()
  /**
   * Which work `doc` and `etag` were loaded for.
   *
   * `work` changes the instant the picker does; `doc` and `etag` change one
   * network round trip later. In between, the component holds one work's id
   * and another work's document, and every consumer below that pairs the two
   * would pair them wrongly — the *boxes on screen* would be folded into the
   * wrong document, and the save scheduled for it would carry the wrong `work`
   * and the wrong `If-Match`. Normally that is a 412 and a notice, but two
   * never-labelled works serialise to identical bytes and therefore identical
   * ETags, so the precondition *passes* and one work's boxes land in the
   * other's `taiyaku.truth.json` — a box on the wrong art, which
   * `taiyaku-core`'s `TruthEdition::is_stale_for` calls the sharpest form of
   * the staleness problem.
   */
  const [loadedWork, setLoadedWork] = useState<string>()
  const [notice, setNotice] = useState<string>()
  const [frame, setFrame] = useState<PixelFrame>()

  /**
   * The truth document on screen — `undefined` while a *different* work's is
   * still in flight. Every read of the document goes through this rather than
   * through `doc`, so there is one place the pairing above is enforced.
   */
  const shownDoc = loadedWork === work ? doc : undefined

  const pageCount =
    edition !== undefined
      ? sidecarInfo?.byEdition[edition]?.pageCount
      : undefined
  const visited =
    edition !== undefined ? visitedPages(shownDoc, edition) : new Set<number>()
  const clampPage = (value: number): number => {
    if (!Number.isFinite(value)) return 0
    const withFloor = Math.max(0, Math.trunc(value))
    return pageCount !== undefined
      ? Math.min(withFloor, pageCount - 1)
      : withFloor
  }

  const toFractions = useCallback((clientX: number, clientY: number): Point => {
    const container = pageRef.current
    if (!container) return { x: 0, y: 0 }
    const img = container.querySelector('img')
    const outer = container.getBoundingClientRect()
    const image = img
      ? {
          rect: img.getBoundingClientRect(),
          naturalWidth: img.naturalWidth,
          naturalHeight: img.naturalHeight,
        }
      : undefined
    return pointOnPage(outer, image, clientX, clientY)
  }, [])

  // The list of works, on mount.
  useEffect(() => {
    if (!ports) return
    let cancelled = false
    ports
      .works()
      .then((list) => {
        if (cancelled) return
        setWorks(list)
        setWork((current) => current ?? list[0])
      })
      .catch((err: unknown) => {
        if (!cancelled) setNotice(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [ports])

  // A work opened: its sidecar (for the edition/page pickers) and its truth
  // document, together — `truthWrite.ts`'s `openWork`, the same two-reads-in-
  // one-call split `startup.ts`'s `openWork` uses for its own four.
  useEffect(() => {
    if (!ports || !work) return
    let cancelled = false
    openWork(ports, work)
      .then((opened) => {
        if (cancelled) return
        setSidecarInfo(opened.info)
        setEdition((current) => {
          if (current && opened.info.editions.includes(current)) return current
          // The remembered edition, but only if this work actually has one by
          // that name: every sidecar in the corpus is `taiyaku.json`, so the
          // work id cannot tell two volumes apart and a remembered `en-fan`
          // must not survive onto a work that has no such edition.
          const last = remembered.edition
          if (last && opened.info.editions.includes(last)) return last
          return opened.info.editions[0]
        })
        setDoc(opened.doc)
        setEtag(opened.etag)
        // Stamped together with the two above, and never anywhere else: this
        // is what says the document and the validator now on hand are this
        // work's.
        setLoadedWork(work)
        setNotice(undefined)
      })
      .catch((err: unknown) => {
        if (!cancelled) setNotice(messageOf(err))
      })
    return () => {
      cancelled = true
    }
    // `remembered.edition` never changes — `remembered` is read once through
    // `useConstant` — so naming it here costs no extra run and keeps this
    // honest rather than suppressed.
  }, [ports, work, remembered.edition])

  /**
   * Whether the page on screen was *chosen* rather than merely landed on.
   *
   * Landing is not labelling. `setPage(0)` below fires on mount and again on
   * every work or edition change, so without this the act of opening the
   * labeller — or switching from `raw-mono` to `en-mono` — wrote a present,
   * empty key for page 0. That key is a claim, and a strong one: the
   * absent/empty distinction reads a present `[]` as *"a person looked at this
   * page and there is no text on it"*.
   *
   * Measured against the real dev set, that is nine phantom pages against 45
   * real ones — one per edition a labeller opens. `score` counts every
   * prediction on a page whose truth is empty as **spurious**, and page 0 is a
   * cover, which carries a title logo and an author's name. So nine covers'
   * worth of *correct* detections would have been charged against every
   * candidate, in the one term — `spurious_per_page` — that the promotion rule
   * states per page rather than as a raw count.
   */
  const deliberate = useRef(false)

  // A different work or a different edition's page 40 is not the page the
  // reader was just looking at.
  // `work`/`edition` are triggers rather than closure reads: the body always
  // does the same thing, and must do it exactly when either changes. The
  // suppression that used to sit here went stale once `settled` was added and
  // biome stopped asking, so it is gone rather than left saying nothing.
  useEffect(() => {
    // The first time these settle is the restore, not a change: `work` and
    // `edition` go from `undefined` to real values one round trip after mount,
    // and resetting then would throw away the page just restored — the reader
    // would land on page 0 every reload however carefully it was remembered.
    if (!settled.current) {
      if (work && edition !== undefined) settled.current = true
      return
    }
    setPage(0)
    // The page this resets to was chosen by nobody.
    deliberate.current = false
  }, [work, edition])

  // Remember where this got to. `deliberate` is untouched by any of it: a page
  // restored on load was chosen in some earlier sitting, not now, so it records
  // no visit until the reader goes somewhere on purpose — the same rule that
  // keeps the landing on page 0 out of the document.
  useEffect(() => {
    if (!ports || edition === undefined) return
    savePrefs<LabellerPrefs>(PREFS_ID, { ...carried, edition, page })
  }, [ports, edition, page, carried])

  // Enter (edition, page): load its boxes, or `[]` for a page never opened.
  //
  // `hasDoc` is `shownDoc`, not `doc`: it goes false the instant the reader
  // picks a different work and true again only once *that* work's document has
  // arrived. Keyed on `doc` alone it fired in between, filling the editor with
  // the previous work's boxes under the new work's name — and since the
  // persistence effect below runs on every `boxes` change, that reload was
  // itself enough to schedule a save of one work's labels against another.
  const hasDoc = shownDoc !== undefined
  // biome-ignore lint/correctness/useExhaustiveDependencies: `shownDoc` is read fresh from the closure below but deliberately excluded from the array — the persistence effect two below writes `doc` on every edit, and depending on it here would reload `boxes` from that very write and fight the reader mid-edit. `hasDoc` is the real trigger: this work's document arriving.
  useEffect(() => {
    if (!ports || !work || !edition || !hasDoc) return
    const regions = pageRegions(shownDoc, edition, page)
    setBoxes(regionsToBoxes(regions ?? []))
    setSelected(undefined)
  }, [ports, work, edition, page, hasDoc])

  // Persist: whenever `boxes` changes — an edit, or having just navigated
  // here — fold it into `doc` and schedule a save. This is also what marks a
  // page visited: the effect above calls `setBoxes` on arrival even when the
  // page held nothing, so this fires and writes a present `[]` key for a page
  // the reader opened and left untouched. A page never navigated to never
  // reaches this effect at all — the absent/empty distinction Task 10 exists
  // to preserve.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `ports`/`work`/`edition`/`page`/`shownDoc`/`sidecarInfo`/`etag` are read fresh from the closure, not tracked — the trigger is `boxes` changing. Tracking `doc` here would also fire this on the `setDoc` two lines below, looping forever.
  useEffect(() => {
    if (!ports || !work || !edition || !shownDoc) return
    // An automatic landing on an empty page this document has never keyed
    // writes nothing — see `deliberate` above for the nine phantom pages this
    // is here to keep out of the ground truth.
    //
    // `alreadyKeyed` is the third term rather than a redundant one: once a box
    // has been drawn here the page *is* keyed, and deleting that last box must
    // still persist. Guarding on `deliberate` and emptiness alone would have
    // made the delete a silent no-op, leaving the box in the document with
    // nothing on screen to show for it.
    const alreadyKeyed = pageRegions(shownDoc, edition, page) !== undefined
    if (!deliberate.current && boxes.length === 0 && !alreadyKeyed) return
    const members = sidecarInfo?.byEdition[edition]?.members ?? []
    const next = withPage(
      shownDoc,
      edition,
      members,
      page,
      boxesToRegions(boxes),
    )
    setDoc(next)
    // The work and the validator are handed to the debounce **here**, beside
    // the document they belong to, rather than read off the latest render when
    // the timer eventually fires. See `loadedWork` above for what a fired
    // timer used to do to a reader who switched works inside the 700 ms.
    scheduleSave(next, work, etag)
  }, [boxes])

  // Which work `work` is *right now*, for a save that is already in flight.
  //
  // `performSaveRef.current` below is reassigned on every render, so the
  // debounce always reaches the newest closure — but the `await` inside it
  // outlives even that, and the reader may have moved on by the time the
  // server answers.
  const workRef = useRef(work)
  workRef.current = work

  // `debounce` holds the callback it was built with for the life of the
  // component, and a save closes over `ports`, which changes — so the debounce
  // calls through this ref to reach the current save rather than the one that
  // existed when it was first built. `App.tsx`'s `writeNotesLatestRef` is the
  // identical route, down to the `null` sentinel for "no render has assigned
  // one yet"; `commitSave` (`truthWrite.ts`) is the policy underneath it.
  //
  // The work and the etag, unlike `ports`, are **arguments** rather than
  // closure reads. They describe the edit rather than the moment the write
  // happens, and reading them off the latest render is what let a burst of
  // edits in one work be written into whichever work was on screen 700 ms
  // later.
  const performSaveRef = useRef<
    | ((
        next: TruthDoc,
        forWork: string,
        forEtag: string | undefined,
      ) => Promise<void>)
    | null
  >(null)
  performSaveRef.current = async (
    next: TruthDoc,
    forWork: string,
    forEtag: string | undefined,
  ) => {
    if (!ports) return
    await commitSave(forWork, next, forEtag, ports, {
      // The other half of the same rule: a validator that came back for one
      // work must not become the `If-Match` the *next* work is written under.
      setEtag: (value) => {
        if (workRef.current === forWork) setEtag(value)
      },
      setNotice,
    })
  }
  const scheduleSave = useConstant(() =>
    debounce<[TruthDoc, string, string | undefined]>(
      (next, forWork, forEtag) => {
        void performSaveRef.current?.(next, forWork, forEtag)
      },
      TRUTH_DEBOUNCE_MS,
    ),
  )

  // A pending write must survive the tab going away, not just this component
  // staying mounted — `hooks.ts`'s `useFlushOnHide`, the same two events
  // `App.tsx` subscribes for `writeProgress`/`saveNotes`. There is only one
  // debounced writer here, so it is handed to both parameters: a second
  // `flush()` on an already-flushed `Debounced` is a no-op by that type's own
  // contract.
  useFlushOnHide(scheduleSave, scheduleSave)

  // Where the box-editing overlay sits: over the painted image once one is on
  // screen, over the whole page area otherwise. Re-measures on every page
  // change, on a resize of either box, and once the incoming image decodes —
  // `naturalWidth` is 0 until then, which is what the `load` listener closes.
  // `PageAnnotations` measures its own overlay through the identical
  // `paintedBox`/`artOffset` pair, for the identical reason.
  const windowWidth = useWindowWidth(NO_WINDOW_WIDTH)
  const longEdge = requestedLongEdge(
    pageBoxLongEdge(windowWidth, window.innerHeight, 'single'),
    window.devicePixelRatio,
    1,
  )
  const src =
    ports && work && edition !== undefined
      ? pageUrl(work, edition, page, longEdge)
      : undefined
  // biome-ignore lint/correctness/useExhaustiveDependencies: `src` is the trigger — a page change, which is the only thing that changes which `<img>` this measures. The body re-queries the DOM fresh on every run rather than closing over anything else.
  useLayoutEffect(() => {
    const host = pageRef.current
    if (!host) return
    const measure = () => {
      const img = host.querySelector('img')
      if (!img) {
        setFrame(undefined)
        return
      }
      const outer = host.getBoundingClientRect()
      const inner = img.getBoundingClientRect()
      if (inner.width === 0 || inner.height === 0) return
      if (img.naturalWidth === 0 || img.naturalHeight === 0) return
      setFrame(
        frameOfImage(outer, {
          rect: inner,
          naturalWidth: img.naturalWidth,
          naturalHeight: img.naturalHeight,
        }),
      )
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(host)
    const img = host.querySelector('img')
    if (img) {
      observer.observe(img)
      img.addEventListener('load', measure)
    }
    return () => {
      observer.disconnect()
      img?.removeEventListener('load', measure)
    }
  }, [src])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Tab') {
        event.preventDefault()
        setSelected(cycleAt(boxes, cursor.current, selected))
        return
      }
      if (selected === undefined) return
      const binding = LABEL_KEYS.find(
        (b) => b.apply !== undefined && b.keys.includes(event.key),
      )
      const apply = binding?.apply
      if (!apply) return
      const next = apply(boxes, selected, event.key)
      setBoxes(next)
      // Whatever the edit did, the selected box's kind afterwards is the one
      // to carry forward. Read off the result rather than special-cased to the
      // digit row, so it cannot fall out of step with which keys set a kind.
      const edited = next.find((box) => box.id === selected)
      if (edited) setCarried({ kind: edited.kind, axis: edited.axis })
      if (binding?.clearsSelection) setSelected(undefined)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [boxes, selected])

  const selectedBox = boxes.find((box) => box.id === selected)
  const axisOf = selectedBox?.axis
  return (
    <div
      className="labeller"
      style={{
        boxSizing: 'border-box',
        display: 'flex',
        flexDirection: 'column',
        height: '100vh',
        padding: 24,
        gap: 12,
      }}
    >
      {ports && (
        <div
          className="labeller-controls"
          style={{
            display: 'flex',
            gap: 12,
            alignItems: 'center',
            flexWrap: 'wrap',
          }}
        >
          <label>
            work{' '}
            <select
              value={work ?? ''}
              onChange={(e) => setWork(e.target.value)}
            >
              {(works ?? []).map((w) => (
                <option key={w} value={w}>
                  {w}
                </option>
              ))}
            </select>
          </label>
          {/*
            The title, because the id beside it cannot tell the four works
            apart: it is the sidecar's filename stem, and every sidecar in this
            corpus is `taiyaku.json`. Restarting `serve` onto the next volume
            mid-session therefore leaves the picker looking identical, and
            labelling ten pages into the wrong volume is not noticed until the
            page numbers stop making sense. This is the one thing on screen
            that changes when the work does.
          */}
          {sidecarInfo?.title && (
            <span data-testid="work-title">{sidecarInfo.title}</span>
          )}
          <label>
            edition{' '}
            <select
              value={edition ?? ''}
              disabled={!sidecarInfo}
              onChange={(e) => setEdition(e.target.value)}
            >
              {(sidecarInfo?.editions ?? []).map((ed) => (
                <option key={ed} value={ed}>
                  {ed}
                </option>
              ))}
            </select>
          </label>
          <label>
            page{' '}
            <input
              type="number"
              min={0}
              max={pageCount !== undefined ? pageCount - 1 : undefined}
              value={page}
              onChange={(e) => {
                // Typing a page number is the choice `deliberate` is about: a
                // page reached this way records an empty key on arrival, which
                // is how "I looked here and there is no text" gets said at all.
                deliberate.current = true
                setPage(clampPage(Number(e.target.value)))
              }}
            />
          </label>
          {pageCount !== undefined && (
            <span data-testid="truth-progress">
              {visited.size} / {pageCount} pages labelled
            </span>
          )}
          {notice && (
            <span role="alert" data-testid="truth-notice">
              {notice}
            </span>
          )}
        </div>
      )}
      <div
        ref={pageRef}
        data-testid="label-page"
        className="label-page"
        style={{ position: 'relative', flex: 1, minHeight: 0 }}
        onPointerDown={(e) => {
          dragStart.current = toFractions(e.clientX, e.clientY)
          // Where the press landed, in client pixels, so the move handler can
          // tell a drag from a click. Deliberately *not* captured here — see
          // `DRAG_SLOP`.
          pressedAt.current = { x: e.clientX, y: e.clientY }
        }}
        onPointerMove={(e) => {
          const at = toFractions(e.clientX, e.clientY)
          cursor.current = at
          const from = dragStart.current
          // The press becomes a drag here, once and for all, and `DRAG_SLOP`
          // is what decides. Everything downstream keys off `dragging` rather
          // than off the press: below the slop this gesture previews nothing,
          // commits nothing and takes no capture — it is a click, and a click
          // belongs to whatever is under it.
          const pressed = pressedAt.current
          if (from && pressed && !dragging.current && isDrag(pressed, e)) {
            dragging.current = true
            // Best-effort: the DOM spec requires `setPointerCapture` to throw
            // `NotFoundError` when the id names no *active* pointer — a
            // pointer that ended between dispatch and this handler, and every
            // synthetic event a test sends. Capture is an enhancement, since
            // without it the drag still draws and still commits and only a
            // release outside the page is lost. Failing the move handler over
            // it would take the preview with it.
            try {
              e.currentTarget.setPointerCapture(e.pointerId)
            } catch {
              // Nothing to do: see above.
            }
          }
          if (!dragging.current) return
          setDrag(
            from &&
              normaliseRect({
                x: from.x,
                y: from.y,
                w: at.x - from.x,
                h: at.y - from.y,
              }),
          )
        }}
        onPointerUp={(e) => {
          const from = dragStart.current
          const wasDrag = dragging.current
          dragStart.current = undefined
          pressedAt.current = undefined
          dragging.current = false
          setDrag(undefined)
          // A press that never became a drag adds nothing. `addBox` already
          // refuses a zero extent, but a wobble is not zero.
          if (!from || !wasDrag) return
          const to = toFractions(e.clientX, e.clientY)
          const box = newBox(
            {
              x: from.x,
              y: from.y,
              w: to.x - from.x,
              h: to.y - from.y,
            },
            carried,
          )
          if (!box) return
          setBoxes((current) => [...current, box])
          // Selected as it is drawn, so the kind and axis keys land on it
          // without a click or a `Tab` first.
          setSelected(box.id)
        }}
        // A drag cancelled by the system — a gesture the browser took over, a
        // window losing focus mid-drag — sends no `pointerup`, so without this
        // the preview outlives the drag it was describing.
        onPointerCancel={() => {
          dragStart.current = undefined
          pressedAt.current = undefined
          dragging.current = false
          setDrag(undefined)
        }}
      >
        {src && (
          <PageView
            src={src}
            alt={`${work} ${edition} page ${page + 1}`}
            buffered={false}
          />
        )}
        <div
          style={
            frame
              ? {
                  position: 'absolute',
                  left: frame.left,
                  top: frame.top,
                  width: frame.width,
                  height: frame.height,
                }
              : { position: 'absolute', inset: 0 }
          }
        >
          <RegionOverlay
            boxes={boxes}
            selectedId={selected}
            onSelect={setSelected}
          />
          {/*
            The box as it is being drawn. Positioned in the same percentages
            `RegionOverlay` uses and inside the same measured frame, so what is
            on screen mid-drag is where the box lands — and drawn from
            `normaliseRect`, the function that decides that, rather than from
            the raw pointer delta. An upward drag, or one clamped at the sheet
            edge, would otherwise preview a rectangle the labeller is not about
            to get.
          */}
          {drag && (
            <div
              className="region-preview"
              data-testid="region-preview"
              style={{
                position: 'absolute',
                left: `${drag.x * 100}%`,
                top: `${drag.y * 100}%`,
                width: `${drag.w * 100}%`,
                height: `${drag.h * 100}%`,
                pointerEvents: 'none',
              }}
            />
          )}
        </div>
      </div>
      <div className="labeller-readout">
        <span data-testid="box-count">{boxes.length}</span>
        {/*
          The kind, beside the axis, because pressing `1`-`6` otherwise
          produced no confirmation of any sort: nothing styled `data-kind` and
          nothing printed it, so 1,500 boxes would have been tagged blind
          against the one input this instrument has. `index.css` carries the
          other half — a per-kind border and a letter badge on the box itself,
          which is where a labeller is actually looking.
        */}
        <span data-testid="kind-readout">{selectedBox?.kind ?? ''}</span>
        <span data-testid="axis-readout">
          {axisOf === null ? '—' : (axisOf ?? '')}
        </span>
      </div>
      {/*
        The keys, on screen rather than in a document beside it. Every row is
        generated from `labelKeys.ts`'s `LABEL_KEYS`, the same table the handler
        dispatches from — `keys.ts`'s rule, applied here: one description, so a
        binding cannot change what it does and keep an old label.
      */}
      <div className="labeller-legend" data-testid="labeller-legend">
        {/*
          The drag is not a key and so is not in the table, but a legend that
          omits the primary action is not a legend. It is stated here, once.
        */}
        <span className="labeller-legend-row">
          <kbd>drag</kbd> draw a box around one text run
        </span>
        {LABEL_KEYS.map((binding) => (
          <span className="labeller-legend-row" key={binding.keys.join()}>
            <kbd>{keysOf(binding)}</kbd> {binding.label}
          </span>
        ))}
      </div>
    </div>
  )
}
