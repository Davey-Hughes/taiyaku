/**
 * The library lists' reseat: when a list's column count changes, each cover
 * on screen or near it moves from where it was drawn to its new box, size
 * included, instead of snapping there. The Continue row above moves with
 * it (`continueRow.ts`), on the same clock and curve.
 */
import { type RefObject, useLayoutEffect } from 'react'
import { columnsIn, filledColumn, pxOf, spreadGap } from './continueRow.ts'
import {
  frameTime,
  mayReseat,
  NEAR_PX,
  onRowLaid,
  onScrollPutBack,
  RESEAT_EASING,
  RESEAT_MS,
  SNAP_PX,
  turnSnaps,
  uprightNow,
} from './reseatTiming.ts'

// Cheap while nothing changes: a cover's box is arithmetic while the column
// count holds. Its column comes from its index, the columns' width and
// pitch from the list's width and the tokens, its row's top from the rows'
// heights, which the grid's resolved row tracks give in one read. Those
// are read again only when the list's height, its count or its column
// count changes. Beyond that a frame reads where each list is drawn, and
// no cover.

/** What a list's style says, read only when its height changes. */
export type Style = {
  track: number
  gap: number
  fill: boolean
  /** The rows' heights, top to bottom, or undefined where unreadable. */
  rows: number[] | undefined
}

/** Where a list lays each cover, as arithmetic on its index. */
export type Geometry = {
  count: number
  columns: number
  /** A column's width, which is a cover's. */
  column: number
  /** From one column's left edge to the next one's. */
  pitch: number
  /** Each row's top, from the list's top. */
  tops: number[]
  heights: number[]
}

/** How a list lays its covers, and where it is drawn in the view. */
export type Placed = { geometry: Geometry; left: number; top: number }

/**
 * The translation and scale that draw a cover where it was, from its new
 * box: a reseat runs from these to none.
 */
export type Move = { dx: number; dy: number; scale: number }

type Seen = {
  style: Style
  /** Undefined where the arithmetic does not describe the list. */
  geometry: Geometry | undefined
  width: number
  height: number
  count: number
  /**
   * Where the list's box is drawn in the view, as last seen: where its
   * covers were drawn is where they start from, so a scroll the browser
   * writes in the frame the columns change (scroll anchoring) does not
   * shift their start.
   * A scroll since is read again where it is written: at its event, and
   * at the scroller swap's put-back (`onScrollPutBack`).
   */
  left: number
  top: number
  /** The size of that box, which a read between resizes must still find. */
  boxWidth: number
  boxHeight: number
  /** Whether the view stood taller than wide (`uprightNow`). */
  upright: boolean | undefined
}

type Running = Move & {
  animation: Animation
  /** Clears what the reseat holds on its list; once only. */
  end: () => void
}

const seen = new WeakMap<Element, Seen>()
const running = new WeakMap<Element, Running>()
const moving = new WeakMap<Element, number>()
const watched = new Set<HTMLElement>()

/**
 * Set from a window resize until the page is laid out at the new size: a
 * read in between (a scroll event, the Continue row's frame) would lay it
 * out early, and take this frame's boxes for last frame's. Cleared when the
 * lists' observer runs, which is after layout, or else by an animation
 * frame callback asked for in the resize event, which runs after any asked
 * for in an earlier frame, so those still see it set.
 */
let resizing = false

const columnOf = (geometry: Geometry, index: number) => index % geometry.columns
const rowOf = (geometry: Geometry, index: number) =>
  Math.floor(index / geometry.columns)

function styleOf(list: HTMLElement): Style {
  const style = getComputedStyle(list)
  const rows = style.gridTemplateRows
    .split(/\s+/)
    .map((track) => Number.parseFloat(track))
    .filter((track) => Number.isFinite(track))
  return {
    track: pxOf(style.getPropertyValue('--tile-min')),
    gap: pxOf(style.getPropertyValue('--tile-gap')),
    fill: Number(style.getPropertyValue('--tile-fill').trim()) === 1,
    rows,
  }
}

/**
 * Where a list `width` wide lays `count` covers, by its `style`: in the
 * fill layout the columns share the width at the least gap, in Smooth they
 * keep the track and the spare room spreads into the gaps. Undefined where
 * that is not how the grid laid them.
 */
export function geometryOf(
  width: number,
  style: Style,
  count: number,
): Geometry | undefined {
  const { track, gap, fill, rows } = style
  if (!(track > 0 && gap >= 0 && width > 0) || rows === undefined) {
    return undefined
  }
  const columns = columnsIn(width, track, gap)
  // The rows the grid laid must be the rows the arithmetic counts, or the
  // tracks belong to some other layout than the one this models.
  if (rows.length !== Math.ceil(count / columns)) return undefined
  const column = fill ? filledColumn(width, track, gap) : track
  const pitch = fill
    ? column + gap
    : columns > 1
      ? track + spreadGap(width, track, gap)
      : 0
  const tops: number[] = []
  let top = 0
  for (const height of rows) {
    tops.push(top)
    top += height + gap
  }
  return { count, columns, column, pitch, tops, heights: rows }
}

/**
 * The move that takes cover `index` from where `was` drew it to where
 * `now` lays it, in a view `view` tall: none for a cover beyond `NEAR_PX`
 * of the view both before and after, or one under `SNAP_PX` from its new
 * box in place and in width. A cover still `under` a move, `progress` of
 * the way through it eased, starts from where that move draws it.
 */
export function moveOf(
  index: number,
  was: Placed,
  now: Placed,
  view: number,
  under?: Move & { progress: number },
): Move | undefined {
  const old = was.geometry
  const next = now.geometry
  const before = rowOf(old, index)
  const after = rowOf(next, index)
  let x = was.left + columnOf(old, index) * old.pitch
  let y = was.top + old.tops[before]
  let width = old.column
  let height = old.heights[before]
  if (under !== undefined) {
    const { progress } = under
    const scale = under.scale + (1 - under.scale) * progress
    x += under.dx * (1 - progress)
    y += under.dy * (1 - progress)
    width *= scale
    height *= scale
  }
  const toX = now.left + columnOf(next, index) * next.pitch
  const toY = now.top + next.tops[after]
  const near = (top: number, tall: number) =>
    top + tall > -NEAR_PX && top < view + NEAR_PX
  if (!near(y, height) && !near(toY, next.heights[after])) return undefined
  const dx = x - toX
  const dy = y - toY
  return Math.abs(dx) < SNAP_PX &&
    Math.abs(dy) < SNAP_PX &&
    Math.abs(width - next.column) < SNAP_PX
    ? undefined
    : { dx, dy, scale: width / next.column }
}

/**
 * What a reseat moves: the tile's content, its first element, never the
 * tile. The browser holds the page still across a change of layout by an
 * element it anchors to, and corrects nothing while that element or one
 * around it runs a transform: the tile, left where layout puts it, is that
 * element (`library.css` keeps the content out of the anchor's choosing).
 */
const contentOf = (tile: Element): Element | null => tile.firstElementChild

function reseating(list: Element, change: number) {
  const now = (moving.get(list) ?? 0) + change
  moving.set(list, now)
  list.toggleAttribute('data-reseating', now > 0)
}

/**
 * Draws `tile`'s cover at its box at once. Its list's mark is cleared here
 * and not at the cancel event, which comes a frame later: a list that snaps
 * as the phone layout turns would clip that first frame for nothing.
 */
function snap(tile: Element) {
  const under = running.get(tile)
  if (under === undefined) return
  under.animation.cancel()
  under.end()
}

/** How far a reseat has come, eased: 0 where it starts, 1 where it ends. */
function progressOf(move: Running): number {
  const progress = move.animation.effect?.getComputedTiming().progress
  return typeof progress === 'number' ? progress : 1
}

function start(
  list: Element,
  tile: Element,
  content: Element,
  move: Move,
  time: number,
) {
  snap(tile)
  const { dx, dy, scale } = move
  // `transform-origin` is the content's top-left corner, the tile's
  // (`library.css`), so the scale leaves the corner where the translation
  // puts it.
  const animation = content.animate(
    [
      { transform: `translate(${dx}px, ${dy}px) scale(${scale})` },
      { transform: 'none' },
    ],
    { duration: RESEAT_MS, easing: RESEAT_EASING },
  )
  // Started at this frame, not when the animation is next ready: the frame
  // about to be drawn shows the cover where it was, and the Continue row,
  // laid on the same clock, keeps step.
  animation.startTime = time
  // Ended once: a finished animation that is then cancelled sends both
  // events, and one that `snap` has ended still sends its cancel event.
  let over = false
  const end = () => {
    if (over) return
    over = true
    if (running.get(tile) === now) running.delete(tile)
    reseating(list, -1)
  }
  const now: Running = { animation, dx, dy, scale, end }
  running.set(tile, now)
  reseating(list, 1)
  animation.addEventListener('finish', end)
  animation.addEventListener('cancel', end)
}

function snapAll(list: HTMLElement) {
  for (const tile of list.children) snap(tile)
}

/**
 * Moves the covers from where `was` drew them to where `now` lays them:
 * a reseat for each one on screen or near it that moves, a snap for the
 * rest. Where one cover read from the page is not where `now` puts it, the
 * arithmetic is wrong, and every cover snaps.
 */
function jumped(list: HTMLElement, was: Placed, now: Placed) {
  const view = window.innerHeight
  const tiles = list.children
  const moves: (Move | undefined)[] = []
  let probe = -1
  for (let index = 0; index < now.geometry.count; index++) {
    const under = running.get(tiles[index])
    const move = moveOf(
      index,
      was,
      now,
      view,
      under && { ...under, progress: progressOf(under) },
    )
    moves.push(move)
    const toY = now.top + now.geometry.tops[rowOf(now.geometry, index)]
    if (probe < 0 && toY >= 0 && toY < view) probe = index
  }
  if (probe >= 0) {
    // One read to check the model against the page. The tile's content is
    // drawn at the tile's new box plus whatever reseat it is still running;
    // a scale moves no top-left corner.
    const { geometry } = now
    const tile = tiles[probe]
    const drawn = (contentOf(tile) ?? tile).getBoundingClientRect()
    const under = running.get(tile)
    const rest = under === undefined ? 0 : 1 - progressOf(under)
    const x =
      now.left +
      columnOf(geometry, probe) * geometry.pitch +
      (under?.dx ?? 0) * rest
    const y =
      now.top + geometry.tops[rowOf(geometry, probe)] + (under?.dy ?? 0) * rest
    if (Math.abs(drawn.left - x) > 1.5 || Math.abs(drawn.top - y) > 1.5) {
      console.warn('reseat: the list is not laid where its arithmetic says', {
        list,
        tile: probe,
        drawn: [drawn.left, drawn.top],
        modelled: [x, y],
      })
      snapAll(list)
      return
    }
  }
  const time = frameTime()
  moves.forEach((move, index) => {
    const tile = tiles[index]
    const content = contentOf(tile)
    if (move === undefined || content === null) {
      snap(tile)
    } else {
      start(list, tile, content, move, time)
    }
  })
}

function laid(
  list: HTMLElement,
  width: number,
  height: number,
  upright: boolean | undefined,
) {
  const was = seen.get(list)
  const count = list.children.length
  // The one read every frame of a drag makes: where the list is drawn, for
  // the frame that changes next to start its covers from.
  const box = list.getBoundingClientRect()
  const fresh =
    was === undefined ||
    was.height !== height ||
    was.count !== count ||
    columnsIn(width, was.style.track, was.style.gap) !== was.geometry?.columns
  const style = fresh ? styleOf(list) : was.style
  const geometry = fresh
    ? geometryOf(width, style, count)
    : was.geometry === undefined
      ? undefined
      : was.width === width
        ? was.geometry
        : geometryOf(width, style, count)
  if (was !== undefined) {
    const old = was.geometry
    const edge =
      was.style.fill !== style.fill || turnSnaps(was.upright, upright)
    // A count change with the columns held still snaps a glide under way:
    // its reseats were measured against boxes the new covers no longer
    // keep.
    const counted = was.count !== count
    const changed =
      old === undefined ||
      geometry === undefined ||
      old.columns !== geometry.columns ||
      edge ||
      counted
    if (changed) {
      // Across the phone layout's edge, where `--tile-fill` turns, a list's
      // place on the page changes wholesale, so its covers snap, as they do
      // at a phone's turn while `TURN_RESEATS` says so; and so do they where
      // the covers themselves changed.
      if (
        old === undefined ||
        geometry === undefined ||
        edge ||
        counted ||
        !mayReseat()
      ) {
        snapAll(list)
      } else {
        jumped(
          list,
          { geometry: old, left: was.left, top: was.top },
          { geometry, left: box.left, top: box.top },
        )
      }
    }
  }
  seen.set(list, {
    style,
    geometry,
    width,
    height,
    count,
    left: box.left,
    top: box.top,
    boxWidth: box.width,
    boxHeight: box.height,
    upright,
  })
}

/** Reads where each list (in `within`, where given) is drawn again. */
function readPlaces(within?: EventTarget | null): void {
  for (const list of watched) {
    const was = seen.get(list)
    if (was === undefined) continue
    if (
      within instanceof Node &&
      within !== document &&
      !within.contains(list)
    ) {
      continue
    }
    const box = list.getBoundingClientRect()
    // Laid at a new size its observer has not run for yet (a cover size
    // read by a layout forced early, and a scroll anchored there): this is
    // the new layout's place, and the change must start from the old one's.
    // The sizes differ only until then: the observer watches this same box
    // (`border-box`), and nothing transforms a list or what holds it. Were
    // they to differ for good, no scroll would be read again.
    if (box.width !== was.boxWidth || box.height !== was.boxHeight) continue
    was.left = box.left
    was.top = box.top
  }
}

/**
 * Reads where each list is drawn again, between its resizes: the Continue
 * row above it laid at a new height, or a scroll. Skipped in a frame the
 * window was resized in until its animation frame callbacks have run: a
 * read there would lay the page out at the new size early, and the list's
 * observer, which reads it then, would take this frame's place for the
 * last one's.
 */
function lookAgain(within?: EventTarget | null): void {
  if (!resizing) readPlaces(within)
}

/** The observer and listeners every attached list shares. */
type Shared = { observer: ResizeObserver; stop: () => void }

let shared: Shared | undefined

function begin(): Shared {
  let frame = 0
  const observer = new ResizeObserver((entries) => {
    resizing = false
    const upright = uprightNow()
    for (const entry of entries) {
      laid(
        entry.target as HTMLElement,
        entry.contentRect.width,
        entry.contentRect.height,
        upright,
      )
    }
  })
  const resized = () => {
    if (resizing) return
    resizing = true
    frame = requestAnimationFrame(() => {
      frame = 0
      resizing = false
    })
  }
  const scrolled = (event: Event) => {
    lookAgain(event.target)
  }
  window.addEventListener('resize', resized)
  window.addEventListener('scroll', scrolled, { capture: true, passive: true })
  const unlisten = onRowLaid(() => {
    lookAgain()
  })
  // Said from the swap's observer, after layout, so a read there lays
  // nothing early; and where that observer runs after the lists', the
  // scroll event that follows can fall in a resized frame and be skipped.
  const unlistenPutBack = onScrollPutBack(() => {
    readPlaces()
  })
  return {
    observer,
    stop: () => {
      observer.disconnect()
      window.removeEventListener('resize', resized)
      window.removeEventListener('scroll', scrolled, { capture: true })
      unlisten()
      unlistenPutBack()
      cancelAnimationFrame(frame)
      resizing = false
    },
  }
}

/**
 * Reseats `list`'s covers at each change of its column count while
 * `drawn`. The observer and window listeners all lists share start with
 * the first attached and stop with the last, so nothing runs before a list
 * of covers is drawn; and where there is no `ResizeObserver` (jsdom),
 * nothing runs at all.
 */
export function useReseat(
  list: RefObject<HTMLUListElement | null>,
  drawn: boolean,
): void {
  useLayoutEffect(() => {
    const ul = list.current
    if (!drawn || ul === null || typeof ResizeObserver === 'undefined') return
    shared ??= begin()
    watched.add(ul)
    shared.observer.observe(ul, { box: 'border-box' })
    return () => {
      shared?.observer.unobserve(ul)
      watched.delete(ul)
      // A list no longer watched is drawn where it is laid at once, not
      // through a move nothing follows any more; and one attached again
      // starts from what it draws then.
      snapAll(ul)
      seen.delete(ul)
      if (watched.size > 0) return
      shared?.stop()
      shared = undefined
    }
  }, [list, drawn])
}
