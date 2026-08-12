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
  addBox,
  type BoxAxis,
  cycleAt,
  type Kind,
  type LabelBox,
  type Point,
  removeBox,
  setAxis,
  setKind,
  toggleIgnore,
} from './labelling'
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

/** Index order is the `1`-`6` key order, and it matches `taiyaku-core`'s `Kind`. */
const KINDS: readonly Kind[] = [
  'dialogue',
  'narration',
  'sfx',
  'logo',
  'credits',
  'other',
]

/**
 * How long a burst of box edits — or of page flips, which also count, see
 * the persistence effect below — waits for quiet before reaching the server.
 * `App.tsx`'s `NOTES_DEBOUNCE_MS` picked for the same reason: this is
 * discrete edits rather than a position, so it sits with prose rather than
 * with the 400 ms reading position.
 */
const TRUTH_DEBOUNCE_MS = 700

/** `App.tsx`'s `NO_WINDOW_WIDTH` — a desktop width, so a document-less environment gets the layout a real screen would rather than the narrowest one. */
const NO_WINDOW_WIDTH = 1280

const messageOf = (err: unknown) =>
  err instanceof Error ? err.message : String(err)

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
  const cursor = useRef<Point>({ x: 0, y: 0 })

  // --- Selection: work, edition, page. Populated only when `ports` is given. ---
  const [works, setWorks] = useState<string[]>()
  const [work, setWork] = useState<string>()
  const [sidecarInfo, setSidecarInfo] = useState<SidecarInfo>()
  const [edition, setEdition] = useState<string>()
  const [page, setPage] = useState(0)
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
  // one-call split `startup.ts`'s `openFirstWork` uses for its own five.
  useEffect(() => {
    if (!ports || !work) return
    let cancelled = false
    openWork(ports, work)
      .then((opened) => {
        if (cancelled) return
        setSidecarInfo(opened.info)
        setEdition((current) =>
          current && opened.info.editions.includes(current)
            ? current
            : opened.info.editions[0],
        )
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
  }, [ports, work])

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
  // biome-ignore lint/correctness/useExhaustiveDependencies: `work`/`edition` are triggers, not closure reads — the body always does the same thing, and must do it exactly when either changes.
  useEffect(() => {
    setPage(0)
    // The page this resets to was chosen by nobody.
    deliberate.current = false
  }, [work, edition])

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
      const kind = KINDS[Number(event.key) - 1]
      if (kind) return setBoxes((current) => setKind(current, selected, kind))
      // `null` is a real axis value, set by `n` — a radial effect has no
      // reading direction. `undefined` means "not an axis key at all". The
      // two must stay apart: collapsing them with `axis ?? null` below would
      // make `v`/`h`/`n` indistinguishable from every other keystroke.
      const axis: BoxAxis | undefined =
        event.key === 'v'
          ? 'v'
          : event.key === 'h'
            ? 'h'
            : event.key === 'n'
              ? null
              : undefined
      if (axis !== undefined)
        return setBoxes((current) => setAxis(current, selected, axis))
      if (event.key === 'i')
        return setBoxes((current) => toggleIgnore(current, selected))
      if (event.key === 'Delete') {
        setBoxes((current) => removeBox(current, selected))
        setSelected(undefined)
      }
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
        }}
        onPointerMove={(e) => {
          cursor.current = toFractions(e.clientX, e.clientY)
        }}
        onPointerUp={(e) => {
          const from = dragStart.current
          dragStart.current = undefined
          if (!from) return
          const to = toFractions(e.clientX, e.clientY)
          setBoxes((current) =>
            addBox(current, {
              x: from.x,
              y: from.y,
              w: to.x - from.x,
              h: to.y - from.y,
            }),
          )
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
    </div>
  )
}
