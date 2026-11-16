import type { ReactElement } from 'react'
import type { Cohort } from './cohort'
import type { DecodeQueue } from './decode'
import type { Direction } from './modules/direction'
import { seamStyle, type ArtworkHug, type FitPrefs } from './modules/fit'
import { cellsFor, secondCellAlone, type Group, type Mode } from './pairing'
import { PageView } from './PageView'

/**
 * One group, drawn — the whole of what the page area puts inside `Viewport`,
 * in every mode.
 *
 * **The reconciliation invariant this component exists to hold.** Changing the
 * *element type* at this position forces React to unmount the outgoing subtree
 * and mount a fresh one, and a freshly mounted `PageView` has nothing decoded,
 * gate or no gate — see `cellsFor`'s own doc. So the two cells of `two` mode
 * are always both present, whether the group is an ordinary pair, a joined
 * spread or a solo slot: the same wrapping `<div>` and the same two child
 * `<PageView>`s on every render, with only their `src` and layout props
 * changing. `GroupView.test.tsx`'s first test is the detector for that,
 * asserting on `<img>` node identity across a wide → paired change.
 *
 * `drawn` and `onShown` are props rather than state held here, and that is
 * load-bearing rather than a style choice: `App` renders this from two
 * different arms of the same ternary, so a `two` → `scroll` → `two` excursion
 * unmounts it. Component-local state would come back empty, `secondCellAlone`
 * would fall to the target's own answer, and the layout would be keyed on the
 * prediction for exactly the frame that keying is what makes it flap. Held
 * above, the record survives the excursion.
 */
export interface GroupViewProps {
  group: Group
  mode: Mode
  /** The viewed edition's direction, override included — `cellsFor`'s. */
  reading: Direction
  src: (slot: number) => string
  alt: (slot: number) => string
  /**
   * Which edge a slot's artwork is pulled to, asked rather than computed here
   * so the page this draws and the geometry that measures it cannot disagree.
   */
  hug: (group: Group, slot: number) => ArtworkHug
  buffered: boolean
  decodeQueue: DecodeQueue
  cohort: Cohort
  fit: FitPrefs
  /** What the two cells have actually painted. `App` state — see above. */
  drawn: { first?: string; second?: string }
  /** `PageView.onShown`, routed back to whatever holds `drawn`. */
  onShown: (cell: 'first' | 'second', src: string) => void
}

export function GroupView({
  group,
  mode,
  reading,
  src,
  alt,
  hug,
  buffered,
  decodeQueue,
  cohort,
  fit,
  drawn,
  onShown,
}: GroupViewProps): ReactElement {
  if (mode !== 'two') {
    // Single and scroll mode put one slot per group, or — for a joined
    // spread — both halves in one group drawn as a single wide page; see
    // `pairing.ts`'s `groups` doc. Always a bare `PageView` here, in every
    // mode and every group shape, which keeps this position's element type
    // stable across every turn.
    const [slot] = group.slots
    return (
      <PageView
        src={src(slot)}
        alt={alt(slot)}
        hug={hug(group, slot)}
        buffered={buffered}
        decodeQueue={decodeQueue}
        cohort={cohort}
        fit={fit}
      />
    )
  }
  // Two fixed cells, always both present, whether `group` is an ordinary
  // pair, a joined spread, or a solo slot with no partner — see
  // `pairing.ts`'s `cellsFor` doc for the full mechanism. Branching the same
  // way the `mode !== 'two'` case above does — a bare `<PageView>` for a
  // spread/solo group, a `<div>` wrapping two for a pair — would flip the
  // element *type* React sees at this position on exactly the turns that
  // matter (entering or leaving a joined spread), forcing a remount that
  // throws away `decode.ts`'s whole gate: a freshly-mounted `PageView` has
  // nothing decoded, gate or no gate. Rendering the same wrapping `<div>` and
  // the same two child `<PageView>`s on every render, and only ever changing
  // their `src`/layout props, lets React reconcile by position instead — see
  // `docs/notes.md`'s two-page flicker measurement.
  const cells = cellsFor(group, reading)
  const { wide, first, second } = cells
  // Whether the second cell shows a real page or a mirror of the first, read
  // off what is drawn rather than off the target — `secondCellAlone` carries
  // the whole argument for why this one question follows the pixels and
  // `wide` above does not.
  const alone = secondCellAlone(cells, drawn)
  return (
    <div style={{ display: 'flex', width: '100%', height: '100%' }}>
      <div
        style={{
          flex: 1,
          minWidth: 0,
          height: '100%',
          // Not while `wide`: the second cell is collapsed to zero width and
          // gets no seam padding, so padding this one narrows the box the
          // single drawn page centres in and shifts it `gap / 4` off centre. A
          // joined spread or a solo slot has no seam to open.
          ...(alone ? {} : seamStyle(fit, 'first')),
        }}
      >
        <PageView
          src={src(first)}
          alt={alt(first)}
          buffered={buffered}
          decodeQueue={decodeQueue}
          cohort={cohort}
          onShown={(shown) => onShown('first', shown)}
          fit={fit}
          hug={hug(group, first)}
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

        `PageView`'s decode gate is per instance, so each cell would run
        `presentWhenDecoded` independently and an ordinary pair's two halves
        would present at different times — one new half beside one old half
        for as long as the slower of the two took to decode. Both cells
        present through one cohort (`cohort.ts`) instead, which holds the
        first decode that settles until the other has settled too, so a turn
        changes both halves in one frame. The faster half's wait is
        genuinely paid — and it is paid anyway, by the half the reader is
        still looking at.
      */}
      <div
        style={
          wide
            ? // A joined spread: collapse this cell so the one image spans the
              // whole group, which is the only way to draw a page that really
              // is twice as wide.
              {
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
                // Keeps its width while merely `alone`: an unpartnered page is
                // an ordinary page, so the group stays two cells wide and only
                // this half goes empty. Collapsing here, as conflating `alone`
                // with `wide` would, lets the first cell flex to full width and
                // back on consecutive turns wherever the aligned pairing leaves
                // a page without a partner.
                ...(alone
                  ? { visibility: 'hidden' as const }
                  : seamStyle(fit, 'second')),
              }
        }
        aria-hidden={alone || undefined}
      >
        <PageView
          src={src(second)}
          alt={alt(second)}
          buffered={buffered}
          decodeQueue={decodeQueue}
          cohort={cohort}
          onShown={(shown) => onShown('second', shown)}
          fit={fit}
          hug={hug(group, second)}
        />
      </div>
    </div>
  )
}
