/**
 * Everything about the truth document that can fail silently: which pages
 * a work presents, converting between the wire shape and `LabelBox`, marking
 * a page visited without losing the absent/empty distinction, saving under
 * `writeGate`'s rule, and converting a pointer position against the rendered
 * *image* rather than the element around it.
 *
 * A file of its own beside `notesWrite.ts`, for the reason that file's own
 * doc gives for splitting out of `notes.ts`: `Labeller.tsx` is DOM, refs and
 * effects, and everything here is synchronous or takes injected ports, so it
 * runs in the fast `node` project rather than needing a mounted component.
 */
import type { FetchedTruth } from './api'
import type { BoxAxis, Kind, LabelBox, Point } from './labelling'
import { writeGate } from './notesWrite'
import { artOffset, paintedBox, pointInSlot, type Box } from './notes'
import { orderedEditions } from './session'
import { cryptoRandom, ulid } from './ulid'
import type { TruthDoc, TruthEdition, TruthRegion } from './truth'

/** `api.ts`'s `listWorks`/`getSidecar`/`getTruth`/`putTruth`, injected. */
export interface TruthPorts {
  works(): Promise<string[]>
  sidecar(work: string): Promise<string>
  get(work: string): Promise<FetchedTruth>
  put(work: string, etag: string, doc: TruthDoc): Promise<FetchedTruth>
}

// ---------------------------------------------------------------------------
// Which pages an edition presents, from the sidecar the reader already reads.

/** One edition's page count and member digests, as the sidecar states them. */
export interface SidecarEditionInfo {
  pageCount: number
  members: { sha256: string }[]
}

/** Enough of the sidecar to drive the work/edition/page pickers. */
export interface SidecarInfo {
  /** Digit-key order — hub first, then the rest sorted. Same rule `App.tsx` reads editions by; not invented a second time. */
  editions: string[]
  byEdition: Record<string, SidecarEditionInfo>
  /**
   * The work's own title, for a person to read.
   *
   * The *id* beside it is derived from the sidecar's filename stem, and every
   * sidecar across this corpus is named `taiyaku.json` — so the picker reads
   * `taiyaku` for all four works, and a server restarted onto a different
   * volume looks exactly like one that was never restarted. That is the whole
   * reason this field is carried: it is the only thing on screen that changes
   * when the work does.
   *
   * Optional, though `taiyaku-core`'s `Work::title` is a required `String`:
   * this text arrives over the wire and is parsed against a hand-written
   * interface rather than a validated schema, so a sidecar without it must
   * degrade to showing the id rather than throwing and taking the whole picker
   * down with it.
   */
  title?: string
}

/** The shape `JSON.parse`d sidecar text has, as much of it as this needs. */
interface SidecarJson {
  hub: string
  editions: Record<string, { members: { sha256: string; pages: number }[] }>
  work?: { title?: string }
}

/**
 * Parse a sidecar into what the pickers need: an ordered edition list and,
 * per edition, a page count and the member digests a fresh `TruthEdition`
 * entry is stamped with.
 *
 * The page count is `taiyaku-core`'s `Edition::total_pages` restated in
 * TypeScript — the sum of every member's own `pages` — because the sidecar
 * carries that number directly and asking the wasm resolver for it would
 * pull `resolver.ts` into a file this whole split exists to keep out of.
 */
export function editionsFromSidecar(json: string): SidecarInfo {
  const parsed = JSON.parse(json) as SidecarJson
  const editions = orderedEditions(parsed.hub, Object.keys(parsed.editions))
  const byEdition: Record<string, SidecarEditionInfo> = {}
  for (const [name, edition] of Object.entries(parsed.editions)) {
    byEdition[name] = {
      pageCount: edition.members.reduce((sum, m) => sum + m.pages, 0),
      members: edition.members.map((m) => ({ sha256: m.sha256 })),
    }
  }
  return { editions, byEdition, title: parsed.work?.title }
}

// ---------------------------------------------------------------------------
// Absent vs. empty, and marking a page visited.

/**
 * A page's boxes, or `undefined` if nobody has opened it.
 *
 * The one place this document's central distinction is read back out:
 * `undefined` is "never opened", `[]` is "opened, and empty" — the two
 * `TruthEdition.pages` refuses to collapse.
 */
export function pageRegions(
  doc: TruthDoc | undefined,
  edition: string,
  page: number,
): TruthRegion[] | undefined {
  return doc?.editions[edition]?.pages[String(page)]
}

/**
 * Every page index of `edition` that has a key in `doc`, present or empty —
 * the progress a labeller of 85 pages needs to see.
 */
export function visitedPages(
  doc: TruthDoc | undefined,
  edition: string,
): ReadonlySet<number> {
  const pages = doc?.editions[edition]?.pages
  return new Set(pages ? Object.keys(pages).map(Number) : [])
}

/**
 * Record a page as visited: `regions` becomes its key, present whether or
 * not it holds anything. This is the one function that may write a key —
 * everything upstream of it either has real boxes to write or an explicit
 * `[]` for a page the reader opened and left with nothing on it. Never
 * called for a page nobody has navigated to.
 *
 * `members` is stamped fresh from the sidecar on every call rather than kept
 * from a prior visit, so an edition entry can never go stale while a reader
 * is actively labelling against the very sidecar these digests describe —
 * `TruthEdition.is_stale_for` is the check this keeps honest.
 */
export function withPage(
  doc: TruthDoc,
  edition: string,
  members: { sha256: string }[],
  page: number,
  regions: TruthRegion[],
): TruthDoc {
  const existing = doc.editions[edition]
  const next: TruthEdition = {
    members,
    pages: { ...existing?.pages, [String(page)]: regions },
  }
  return { ...doc, editions: { ...doc.editions, [edition]: next } }
}

// ---------------------------------------------------------------------------
// LabelBox <-> TruthRegion, the editor's shape and the wire shape.

/**
 * Mint a fresh `LabelBox` per region on load. Ids are not part of the wire
 * shape — `TruthRegion` has none — so there is nothing to preserve across a
 * reload and no reason for two loads of the same page to agree on one.
 */
export function regionsToBoxes(regions: readonly TruthRegion[]): LabelBox[] {
  return regions.map((region) => ({
    id: ulid(Date.now(), cryptoRandom),
    x: region.x,
    y: region.y,
    w: region.w,
    h: region.h,
    axis: region.axis,
    kind: region.kind,
    ignore: region.ignore ?? false,
  }))
}

/**
 * The inverse, for writing. `id` and `text` are dropped: `id` names nothing
 * on the wire, and `text` is reserved for a later OCR pass — nothing in this
 * editor sets it, so writing it back as `undefined` would be indistinguishable
 * from never touching the field, and omitting it here says that plainly.
 */
export function boxesToRegions(boxes: readonly LabelBox[]): TruthRegion[] {
  return boxes.map((box) => ({
    x: box.x,
    y: box.y,
    w: box.w,
    h: box.h,
    axis: box.axis as BoxAxis,
    kind: box.kind as Kind,
    ignore: box.ignore,
  }))
}

// ---------------------------------------------------------------------------
// Saving, under the same gate notes uses.

const NO_VALIDATOR =
  'These labels are not being saved: this device has no current copy of ' +
  'them to write against. Reload the page.'

/** `writeGate`, with truth's own sentence — see `notesWrite.ts`'s `writeGate` for the rule this reuses rather than repeats. */
export const truthWriteGate = (
  doc: TruthDoc | undefined,
  etag: string | undefined,
) => writeGate(doc, etag, NO_VALIDATOR)

export type SaveOutcome =
  | { kind: 'written'; doc: TruthDoc; etag: string | undefined }
  | { kind: 'refused'; notice: string }

const messageOf = (err: unknown) =>
  err instanceof Error ? err.message : String(err)

/**
 * Write a truth document. One attempt, no retry-and-merge: unlike notes,
 * nothing here guesses at combining two devices' labelling sessions, so a
 * stale validator is reported to the reader rather than papered over. The
 * boxes just drawn stay in the editor's own state regardless — nothing here
 * can lose them, only fail to have saved them yet.
 */
export async function saveTruth(
  work: string,
  etag: string,
  doc: TruthDoc,
  ports: Pick<TruthPorts, 'put'>,
): Promise<SaveOutcome> {
  try {
    const fresh = await ports.put(work, etag, doc)
    return { kind: 'written', doc: fresh.doc, etag: fresh.etag }
  } catch (err) {
    return { kind: 'refused', notice: messageOf(err) }
  }
}

/** The two state updates a save outcome ever produces, injected so this stays callable without a mounted component. */
export interface SaveDispatch {
  setEtag(etag: string | undefined): void
  setNotice(notice: string | undefined): void
}

/**
 * `truthWriteGate` then `saveTruth`, folded into the two setters
 * `Labeller.tsx`'s debounced writer calls through — the whole body of
 * `performSaveRef.current` moved here so the component holds only the ref
 * indirection `App.tsx`'s `writeNotesLatestRef` doc explains, not the policy
 * underneath it.
 */
export async function commitSave(
  work: string,
  doc: TruthDoc,
  etag: string | undefined,
  ports: Pick<TruthPorts, 'put'>,
  dispatch: SaveDispatch,
): Promise<void> {
  const gate = truthWriteGate(doc, etag)
  if (gate.kind === 'wait') return
  if (gate.kind === 'refuse') {
    dispatch.setNotice(gate.notice)
    return
  }
  const outcome = await saveTruth(work, gate.etag, gate.doc, ports)
  if (outcome.kind === 'written') {
    dispatch.setEtag(outcome.etag)
    dispatch.setNotice(undefined)
  } else {
    dispatch.setNotice(outcome.notice)
  }
}

// ---------------------------------------------------------------------------
// Opening a work: the sidecar and the truth document, together.

export interface OpenedWork {
  info: SidecarInfo
  doc: TruthDoc
  etag: string | undefined
}

/**
 * The two reads a work selection makes, in one call so `Labeller.tsx`'s
 * effect is a `.then` and five setters rather than the orchestration itself
 * — `startup.ts`'s `openWork` is the same split for the same reason.
 */
export async function openWork(
  ports: Pick<TruthPorts, 'sidecar' | 'get'>,
  work: string,
): Promise<OpenedWork> {
  const [json, truth] = await Promise.all([
    ports.sidecar(work),
    ports.get(work),
  ])
  return { info: editionsFromSidecar(json), doc: truth.doc, etag: truth.etag }
}

// ---------------------------------------------------------------------------
// A pointer position, against the rendered image rather than the element.

/** As much of a rendered element's box as this needs, so no real DOM is required to test it. */
export interface RectLike {
  left: number
  top: number
  width: number
  height: number
}

/** As much of an `<img>` as this needs — `rect` from `getBoundingClientRect()`, the natural size from the decoded bitmap. */
export interface ImageMeasure {
  rect: RectLike
  naturalWidth: number
  naturalHeight: number
}

/**
 * A pointer position, as fractions of the *painted* page — not the element
 * around it.
 *
 * `PageView` draws with `object-fit: contain` (`pageStyle`'s `maxWidth`/
 * `maxHeight: 100%`), so once a real page is on screen the element and the
 * image differ by the letterbox margin. `App.tsx`'s `pointOn` solves the
 * identical problem for the reader's own region drag, through the same
 * `paintedBox`/`pointInSlot` this calls rather than a second version of the
 * same arithmetic — two implementations of "where is the page" is exactly
 * how they would end up disagreeing.
 *
 * `image` is `undefined` when nothing has decoded yet, or when this is the
 * standalone editor with no page loaded at all — every keybinding and drag
 * test in `Labeller.test.tsx`/`Labeller.browser.test.tsx` drives it that way
 * — and the fallback is the plain element-relative arithmetic this always
 * used before a real page existed to letterbox against.
 */
export function pointOnPage(
  container: RectLike,
  image: ImageMeasure | undefined,
  clientX: number,
  clientY: number,
): Point {
  if (!image) {
    if (container.width === 0 || container.height === 0) return { x: 0, y: 0 }
    return {
      x: (clientX - container.left) / container.width,
      y: (clientY - container.top) / container.height,
    }
  }
  const painted = paintedBox(
    { w: image.naturalWidth, h: image.naturalHeight },
    { w: image.rect.width, h: image.rect.height },
  )
  return pointInSlot(
    { x: clientX - image.rect.left, y: clientY - image.rect.top },
    painted,
  )
}

/** A pixel rectangle, in the coordinates of the element that hosts the overlay. */
export interface PixelFrame {
  left: number
  top: number
  width: number
  height: number
}

/**
 * Where the box-editing overlay should sit: over the painted image when one
 * is on screen, over the whole container otherwise.
 *
 * The other half of `pointOnPage`'s reasoning — `RegionOverlay` positions its
 * children as percentages of *its own* box, so that box has to be the painted
 * page, not the letterboxed element around it, or every box drawn at a
 * correct fraction renders in the wrong place anyway. `artOffset` is the same
 * call `PageAnnotations` measures its own overlay through, for the note
 * margin tabs — this is that fix, for a second overlay that needs it.
 */
export function frameOfImage(
  container: RectLike,
  image: ImageMeasure | undefined,
): PixelFrame {
  if (!image)
    return { left: 0, top: 0, width: container.width, height: container.height }
  const art: Box = paintedBox(
    { w: image.naturalWidth, h: image.naturalHeight },
    { w: image.rect.width, h: image.rect.height },
  )
  const offset = artOffset(container, image.rect, art)
  return { left: offset.left, top: offset.top, width: art.w, height: art.h }
}
