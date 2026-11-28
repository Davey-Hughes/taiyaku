import type { CSSProperties } from 'react'
import type { ReaderModule } from '../module'
import type { Cell } from '../viewport'

/**
 * Page geometry: how large a page is drawn in its own box, and how far apart
 * the two pages of a spread sit.
 *
 * `docs/superpowers/specs/2026-08-02-reader-m5-design.md` specifies this
 * module with four settings, of which `trim` and `fillWidth`/`fillHeight` are
 * unbuilt, with their reasoning in that document. `gap` joins `upscaleToFit`
 * and `upscaleFilter` because a seam is geometry, which is the line that
 * design draws between `fit` and `filters`.
 *
 * **`upscaleToFit` defaults on, and that reverses the M5 design.** That
 * document chose off and argued it well: a low-resolution scan on a large
 * monitor is "a small page marooned in a large letterbox", so the enlargement
 * should be asked for. What it could not know is how the default would read:
 * beside a high-resolution edition that fills the page area's height, a
 * low-resolution one sits at 1:1 with space unused, and that reads as a bug
 * in the images, not a setting a reader had not found. The figures are in
 * `docs/superpowers/specs/2026-08-08-fit-seam-and-help-design.md` §1.
 *
 * `upscaleFilter` ships with it rather than after it. Enlarging line art with
 * a smoothing filter reads as mush; nearest-neighbour keeps the line crisp and
 * pays in visible stair-stepping. Which is better is a per-scan judgement, so
 * turning the default on without exposing the filter would trade one
 * complaint for another.
 */
export interface FitPrefs {
  upscaleToFit: boolean
  upscaleFilter: 'smooth' | 'crisp'
  /** Pixels between the two pages in `two` mode. */
  gap: number
}

export const fitModule: ReaderModule<FitPrefs> = {
  id: 'fit',
  label: 'Fit',
  defaults: { upscaleToFit: true, upscaleFilter: 'smooth', gap: 0 },
  options: { upscaleFilter: ['smooth', 'crisp'] },
  optionWords: {
    upscaleFilter: {
      smooth: 'Smooth (softer lines)',
      crisp: 'Crisp (sharp, blocky edges)',
    } satisfies Record<FitPrefs['upscaleFilter'], string>,
  },
}

/**
 * Which edge of its own box a page's artwork is pulled to.
 *
 * **Named for the edge, never for the cell that hugs it.** The left-hand cell
 * of a two-page pair passes `right-edge`, because the seam is on its right.
 * `'left' | 'right'` was the shorter spelling and is the one to avoid: at a
 * call site it reads equally well as "this is the left cell", so the backwards
 * version of the pair would look correct. There is no such reading of
 * `right-edge` — a cell is not an edge.
 */
export type ArtworkHug = 'centre' | 'left-edge' | 'right-edge'

/**
 * Which edge a cell's artwork is pulled to — the translation from
 * `viewport.ts`'s `Cell` to the hug above, and nothing else.
 *
 * One answer for the three things that have to agree about it: the two
 * `PageView`s `GroupView` draws, the marker overlay that measures one of them,
 * and `pointOn`'s drag arithmetic. Written out at each of the three, a copy
 * that fell out of step would draw the page one way and measure it another — a
 * region landing away from where the reader swept it out, `paintedBox` having
 * the measurement.
 *
 * **The *first* cell hugs its right edge**, because that is the side the seam
 * is on; `ArtworkHug` above has the whole naming argument. Which physical slot
 * is the first cell is `cellOf`'s question and is already
 * reading-direction-corrected by the time it reaches here, so nothing in this
 * function knows about `rtl` — writing the comparison out at a call site is
 * what would put a `reading === 'rtl'` test back into `App.tsx`.
 *
 * `'only'` is `'centre'`: one image filling its group has no seam to close.
 * Single and scroll mode never reach a cell at all, and their caller answers
 * `'centre'` without asking.
 */
export function hugForCell(cell: Cell): ArtworkHug {
  return cell === 'first'
    ? 'right-edge'
    : cell === 'second'
      ? 'left-edge'
      : 'centre'
}

/**
 * How one page sits in its own box.
 *
 * The whole of the upscale switch is `width`/`height`: `auto` on a replaced
 * element means "your intrinsic size, capped by `max-*`", which downscales a
 * large scan and leaves a small one alone; `100%` means "your box", which is
 * what lets a small one grow. `objectFit: contain` is what keeps the aspect
 * either way — with `auto` it has nothing to reconcile, since box and content
 * coincide, and with `100%` it is doing the work.
 *
 * `imageRendering` is omitted rather than set to a default keyword when it is
 * not wanted, so the browser's own choice stands and this never has an opinion
 * about downscaling — a filter is only meaningful while something is being
 * enlarged, which is why `crisp` is ignored while `upscaleToFit` is off.
 *
 * **`hug` is what closes the two-page seam, and while `upscaleToFit` is on
 * nothing that moves the element can.** With `object-position` unset, the two
 * `<img>` *elements* touch while their artwork sits apart: upscaling makes the
 * element fill its whole cell, and `objectFit: contain` then draws the art
 * inside that element wherever `object-position` says, which unset is
 * `50% 50%`. `100% 50%` on the left cell and `0% 50%` on the right put the art
 * against the seam.
 *
 * The centred case writes `50% 50%` out rather than omitting the declaration.
 * That is the opposite choice from `imageRendering` above and for a different
 * reason: `image-rendering`'s default is the browser's own judgement and any
 * keyword here would replace it, while `object-position`'s initial value *is*
 * `50% 50%`, so restating it decides nothing and costs no pixel. What it buys
 * is a value a test can read — which is the whole of `fit.test.ts`'s reach,
 * since `web/` has no DOM harness — and, if `index.css` ever grew a rule for
 * images, a page that still draws where this function says. It has none today.
 *
 * **The hug only bites while `upscaleToFit` is on**, which is the default.
 * With it off the element is sized to its own content, so box and art coincide
 * and `object-position` has no free space to distribute — the pages then sit
 * centred in their halves, `PageView`'s wrapper doing the centring.
 */
export function pageStyle(
  prefs: FitPrefs,
  hug: ArtworkHug = 'centre',
): CSSProperties {
  const filling = prefs.upscaleToFit
  return {
    maxWidth: '100%',
    maxHeight: '100%',
    width: filling ? '100%' : 'auto',
    height: filling ? '100%' : 'auto',
    objectFit: 'contain',
    objectPosition:
      hug === 'left-edge'
        ? '0% 50%'
        : hug === 'right-edge'
          ? '100% 50%'
          : '50% 50%',
    display: 'block',
    ...(filling && prefs.upscaleFilter === 'crisp'
      ? { imageRendering: 'pixelated' as const }
      : {}),
  }
}

/**
 * How one cell of a two-page group holds its page: the gap, and nothing else.
 *
 * The pages are pushed **toward each other** so the grey collects on the outer
 * edges instead of down the middle — by `pageStyle`'s `hug`, on the image, not
 * by anything here — and `gap` reopens a measured amount of it deliberately.
 * Half the gap goes on each cell so the pair stays centred in the page area;
 * putting the whole gap on one side would shift the spread.
 *
 * **A cell sets no `display: flex` with `justifyContent`, because it would do
 * nothing.** A cell justifying its page toward the seam is what would collect
 * the letterbox outside, but `PageView` renders a `width: 100%` wrapper
 * `<div>` around its `<img>`, so the cell's only child always exactly fills
 * whatever content box the padding leaves, in flex or block layout alike, and
 * there is no free space for `justify-content` to distribute. No test could
 * catch it — `justify-content` on a non-flex box, or on one with a single
 * full-width child, is silently inert, so an assertion can only see that the
 * declaration is present.
 *
 * `cell` is screen position and not reading order. `cellsFor` has already
 * applied the work's direction by the time these cells exist, so the first is
 * always the one on the left; a rule written in terms of `rtl` here would
 * apply that correction a second time.
 *
 * A negative gap is treated as zero. Nothing in the settings panel produces
 * one — a `number` field is a `number` field — but overlapping the two pages
 * is not a thing this can be asked for by accident.
 */
export function seamStyle(
  prefs: FitPrefs,
  cell: 'first' | 'second',
): CSSProperties {
  const half = `${Math.max(prefs.gap, 0) / 2}px`
  return cell === 'first' ? { paddingRight: half } : { paddingLeft: half }
}
