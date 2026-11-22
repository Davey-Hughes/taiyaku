import { act } from 'react'
import { describe, expect, it, onTestFinished } from 'vitest'
import { page } from 'vitest/browser'
// Required, for the reason `Filmstrip.browser.test.tsx` gives at its own
// import: the reader's custom properties, the line's height and the mark's
// colour all reach a page through this.
import '../src/index.css'
import type { ChromeProps } from '../src/Chrome'
import { rerender } from './support/mount'
import { mouse } from './support/mouse'
import { areaOf, overlapOf, pointerRulesAs } from './support/pointer'
import {
  BARELY,
  centre,
  FITS,
  farEnd,
  firstReportPast,
  flick,
  frame,
  inReader,
  LONG,
  laterSide,
  lineOf,
  loadThumbnailsLate,
  MARK_MAX_PX,
  MARK_MIN_PX,
  markBox,
  markOf,
  onShowPx,
  railOf,
  reader,
  rememberLetGo,
  SHORT,
  scrollRail,
  tileMiddle,
  travel,
  untilAtRest,
  VERY_LONG,
  widthDue,
} from './support/readerBars'

/**
 * O216: the mark on the progress line, with a real strip over a real line.
 *
 * `Chrome.test.tsx` holds what `Chrome` writes on the mark, a `clamp()` and a
 * `calc()` that jsdom leaves unresolved, and `Filmstrip.test.tsx` when the
 * strip tells and what it tells from numbers given by hand. Neither can say
 * where the mark lands. Here the rail's numbers are the browser's, the strip
 * tells them itself, and what is read is the mark's own box against the
 * line's and the tiles'.
 *
 * Nothing answers `/api` in this project, so the thumbnails the strip asks
 * for never load and every tile stands at its floor: the same footing
 * `Filmstrip.browser.test.tsx` describes. The one case about a thumbnail
 * loading makes its own.
 *
 * **Both reading directions, every case that measures a box.** The rail is
 * `row-reverse` for a work read right to left, its `scrollLeft` runs
 * negative, and the line is mirrored: each of the three is a way to be right
 * in one direction and wrong in the other.
 */

describe('the mark on the progress line, laid out', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`stands flush with the line's start while the rail stands at its own, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host).getBoundingClientRect()

      // THE STATE THIS TEST NAMES: a rail that overflows, at its start.
      expect(travel(rail)).toBeGreaterThan(0)
      expect(rail.scrollLeft).toBe(0)

      // The line starts on the side the first tile stands on: the side later
      // tiles are not.
      const mark = markBox(host)
      if (laterSide(rail) === 1) {
        expect(mark.left).toBeCloseTo(line.left, 0)
      } else {
        expect(mark.right).toBeCloseTo(line.right, 0)
      }
      // Inside the line whichever end it stands at.
      expect(mark.left).toBeGreaterThanOrEqual(line.left - 0.5)
      expect(mark.right).toBeLessThanOrEqual(line.right + 0.5)
    })

    it(`stands flush with the line's end once the rail is scrolled to its own, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()

      await scrollRail(rail, farEnd(rail))

      // THE STATE THIS TEST NAMES: the rail has gone as far as it goes.
      expect(Math.abs(rail.scrollLeft)).toBeCloseTo(travel(rail), 0)

      const line = lineOf(host).getBoundingClientRect()
      const mark = markBox(host)
      if (later === 1) {
        expect(mark.right).toBeCloseTo(line.right, 0)
      } else {
        expect(mark.left).toBeCloseTo(line.left, 0)
      }
      expect(mark.left).toBeGreaterThanOrEqual(line.left - 0.5)
      expect(mark.right).toBeLessThanOrEqual(line.right + 0.5)
    })

    // A volume a little longer than the strip is wide. The rail's travel is
    // a small part of its length, so a pixel of scroll is next to nothing
    // against the length and a visible step along the line, which is as long
    // as the rail is wide. Taken to its end a pixel at a time, the mark has
    // to stand where the rail does after every one of them.
    it(`follows a rail that barely overflows to its end a pixel at a time, ${reading}`, async () => {
      await page.viewport(2400, 900)
      const host = inReader({ ...BARELY, reading, currentIndex: 0 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()

      // THE STATE THIS TEST NAMES: a rail that overflows by a small part of
      // its width, with pixels enough of travel to step through.
      expect(travel(rail)).toBeGreaterThan(8)
      expect(travel(rail)).toBeLessThan(rail.clientWidth / 10)

      /** How far the mark stands from the line's start, and the room it has to move in. */
      const stand = () => {
        const line = lineOf(host).getBoundingClientRect()
        const mark = markBox(host)
        return {
          fromStart:
            later === 1 ? mark.left - line.left : line.right - mark.right,
          room: line.width - mark.width,
        }
      }
      const end = farEnd(rail)
      await scrollRail(rail, end - later * 4)
      for (const short of [3, 2, 1, 0]) {
        await scrollRail(rail, end - later * short)
        const { fromStart, room } = stand()
        const along = Math.abs(rail.scrollLeft) / travel(rail)
        expect
          .soft(fromStart, `${short} px short of the rail's end`)
          .toBeCloseTo(along * room, 0)
      }

      // At the rail's end, flush with the line's.
      expect(Math.abs(rail.scrollLeft)).toBe(travel(rail))
      const { fromStart, room } = stand()
      expect(fromStart).toBeCloseTo(room, 0)
    })

    // The line's own box, and the side its fill grows from. The line is
    // turned round for a work read right to left, and turned about its own
    // middle: it lies where it lay, across the bar, and the fill starts at
    // the end the volume starts at.
    it(`draws the line across the bar and fills it from the end the volume starts at, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0, progress: 0.25 })
      const rail = railOf(host)
      const bar = (
        host.querySelector('.reader-bottom-bar') as HTMLElement
      ).getBoundingClientRect()
      const lineElement = lineOf(host)
      const line = lineElement.getBoundingClientRect()
      const margins = getComputedStyle(lineElement)
      const inset = Number.parseFloat(margins.marginLeft)
      expect(inset).toBeGreaterThan(0)
      expect(Number.parseFloat(margins.marginRight)).toBe(inset)

      expect(line.left).toBeCloseTo(bar.left + inset, 0)
      expect(line.right).toBeCloseTo(bar.right - inset, 0)

      const fill = (
        host.querySelector('.reader-progress-fill') as HTMLElement
      ).getBoundingClientRect()
      expect(fill.width).toBeCloseTo(line.width / 4, 0)
      if (laterSide(rail) === 1) {
        expect(fill.left).toBeCloseTo(line.left, 0)
      } else {
        expect(fill.right).toBeCloseTo(line.right, 0)
      }
    })

    // The bars are hidden and shown with the rail where it stood: the strip
    // that comes back centres on the same tile, so its rail's numbers are
    // the ones the last strip told. The mark that comes back with it is a
    // new element, and has to be drawn as the old one was.
    it(`is drawn again as it was when the bars come back with the rail where it stood, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      await firstReportPast()
      const first = markOf(host)
      const before = markBox(host)
      const stood = railOf(host).scrollLeft
      expect(stood).not.toBe(0)

      rerender(reader({ reading, currentIndex: 60, visible: false }))
      expect(host.querySelector('.reader-bottom-bar')).toBeNull()
      rerender(reader({ reading, currentIndex: 60 }))

      // THE STATE THIS TEST NAMES: a new mark, over a rail that stands where
      // the last one stood.
      expect(markOf(host)).not.toBe(first)
      expect(railOf(host).scrollLeft).toBe(stood)
      const after = markBox(host)
      expect(after.width).toBeCloseTo(before.width, 0)
      expect(after.width).toBeGreaterThan(0)
      expect(after.left).toBeCloseTo(before.left, 0)
    })

    // The rail is moved toward later pages, so its tiles slide toward the
    // side the earlier ones are on, and the mark has to go the other way:
    // toward the side the later pages lie on, as a scrollbar's thumb does.
    //
    // The step is short of what rebuilds the strip's window, so nothing
    // renders, and the observer's first report is let past first: what moves
    // the mark here is the scroll's own telling.
    it(`moves toward the side the later pages lie on as the rail is scrolled to them, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()
      const tile = () =>
        centre(
          (
            rail.querySelector('[data-group="60"]') as HTMLElement
          ).getBoundingClientRect(),
        )

      // THE STATE THIS TEST NAMES: the rail stands in the middle of its
      // travel, with room to go on.
      const step = 300
      expect(Math.abs(rail.scrollLeft)).toBeGreaterThan(step)
      expect(travel(rail) - Math.abs(rail.scrollLeft)).toBeGreaterThan(step)
      const markBefore = markBox(host)
      const tileBefore = tile()
      const window = () =>
        [...rail.querySelectorAll('button')].map((b) => b.dataset.group)
      const windowBefore = window()

      await scrollRail(rail, rail.scrollLeft + later * step)

      // The tiles went toward the earlier side, by the step, and they are the
      // tiles that were there: the window was not rebuilt.
      expect(tile() - tileBefore).toBeCloseTo(-later * step, 0)
      expect(window()).toEqual(windowBefore)
      // The mark went toward the later side: as far along the room its width
      // leaves it on the line as the step is along the rail's travel.
      const line = lineOf(host).getBoundingClientRect()
      const markAfter = markBox(host)
      const moved = centre(markAfter) - centre(markBefore)
      expect.soft(moved * later).toBeGreaterThan(0)
      expect
        .soft(moved)
        .toBeCloseTo(
          later * (step / travel(rail)) * (line.width - markAfter.width),
          0,
        )
    })

    // At rest the mark's insets on a mouse's device come from the rule that
    // thickens the line, and on a phone from the stylesheet's top-level rule
    // alone, so each device's drawing is read.
    for (const device of ['mouse', 'phone'] as const) {
      it(`is 64 px where the share on show is narrower, one pixel proud of the line at rest, ${device} ${reading}`, async () => {
        onTestFinished(pointerRulesAs(device))
        await page.viewport(1440, 900)
        const host = inReader({ ...VERY_LONG, reading, currentIndex: 0 })
        const rail = railOf(host)
        const lineElement = lineOf(host)
        for (const a of lineElement.getAnimations({ subtree: true })) {
          a.finish()
        }
        const line = lineElement.getBoundingClientRect()

        // THE STATE THIS TEST NAMES: the line at rest, and the share on show
        // narrower than the lower bound, so that bound decides.
        expect(lineElement.dataset.awake).toBeUndefined()
        const onShow = onShowPx(rail, lineElement)
        expect(onShow).toBeGreaterThan(0)
        expect(onShow).toBeLessThan(MARK_MIN_PX - 1)

        const mark = markBox(host)
        expect(mark.width).toBeCloseTo(MARK_MIN_PX, 0)
        expect(line.height).toBe(2)
        expect([mark.top, mark.bottom]).toEqual([line.top - 1, line.bottom + 1])
      })
    }

    it(`is the share on show where that lies between 64 and 160 px, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const lineElement = lineOf(host)
      for (const a of lineElement.getAnimations({ subtree: true })) a.finish()

      // THE STATE THIS TEST NAMES: the line at rest, and the share on show
      // between the bounds, so neither decides.
      expect(lineElement.dataset.awake).toBeUndefined()
      const onShow = onShowPx(rail, lineElement)
      expect(onShow).toBeGreaterThan(MARK_MIN_PX + 1)
      expect(onShow).toBeLessThan(MARK_MAX_PX - 1)

      expect(markBox(host).width).toBeCloseTo(onShow, 0)
    })

    it(`is 160 px where the share on show is wider, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...SHORT, reading, currentIndex: 0 })
      const rail = railOf(host)
      const lineElement = lineOf(host)
      for (const a of lineElement.getAnimations({ subtree: true })) a.finish()

      // THE STATE THIS TEST NAMES: the line at rest, a rail that overflows,
      // and the share on show wider than the upper bound, so that bound
      // decides.
      expect(lineElement.dataset.awake).toBeUndefined()
      expect(travel(rail)).toBeGreaterThan(0)
      const onShow = onShowPx(rail, lineElement)
      expect(onShow).toBeGreaterThan(MARK_MAX_PX + 1)

      expect(markBox(host).width).toBeCloseTo(MARK_MAX_PX, 0)
    })

    // The cases above read the mark's box, and a box is the same whether or
    // not what holds it clips it: a line with `overflow: hidden` leaves the
    // box a pixel proud and draws none of either row. What is at a point is
    // what is drawn there, so this asks what is at a point in each of the
    // two rows: every element, since the press band lies over the mark.
    //
    // A row is asked for by its own whole number. Between two rows the
    // browser's answer leans upward, and a point half a pixel above a box
    // would be read as inside it.
    it(`is drawn in the row above the line and the row below it, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const line = lineOf(host).getBoundingClientRect()
      const mark = markOf(host)
      const x = centre(markBox(host))
      const at = (row: number) => document.elementsFromPoint(x, row)
      // THE STATE THIS TEST NAMES: the line lies on whole rows, two of them,
      // and the topmost element at its middle is the press band, not the
      // mark.
      expect(Number.isInteger(line.top)).toBe(true)
      expect(line.height).toBe(2)
      expect(
        document
          .elementFromPoint(x, line.top + line.height / 2)
          ?.classList.contains('reader-progress-grip'),
      ).toBe(true)

      expect(at(line.top - 1)).toContain(mark)
      expect(at(line.bottom)).toContain(mark)
      // On the line's own two rows as well.
      expect(at(line.top)).toContain(mark)
      expect(at(line.top + 1)).toContain(mark)
      // THE CONTROL: a row further out either way is not the mark's.
      expect(at(line.top - 2)).not.toContain(mark)
      expect(at(line.bottom + 1)).not.toContain(mark)
    })

    // A drag of the strip writes the rail's `scrollLeft` as the wheel does,
    // and the mark has to go with it. A real mouse, held: the mark is read
    // with the button still down.
    it(`follows a real drag of the strip, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()
      /** How far the mark stands from the line's start, and the room it has to move in. */
      const stand = () => {
        const line = lineOf(host).getBoundingClientRect()
        const mark = markBox(host)
        return {
          fromStart:
            later === 1 ? mark.left - line.left : line.right - mark.right,
          room: line.width - mark.width,
        }
      }
      const before = stand()
      const stood = rail.scrollLeft
      const { x, y } = tileMiddle(rail, 62)
      // Toward the middle of the window, so the pointer stays inside it.
      const by = x > window.innerWidth / 2 ? -600 : 600

      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, by)
        // The `scroll` the last write fires, and the frame the rail's
        // listener takes after it.
        await frame()
        await frame()
      })

      // THE STATE THIS TEST NAMES: the rail has gone with the pointer, and
      // far enough that a mark left where it was would be seen to be.
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(598)
      const { fromStart, room } = stand()
      const along = Math.abs(rail.scrollLeft) / travel(rail)
      expect(Math.abs(along * room - before.fromStart)).toBeGreaterThan(20)

      expect(fromStart).toBeCloseTo(along * room, 0)

      await act(async () => {
        await mouse.release()
        await frame()
      })
    })

    // A drag let go while the pointer is moving carries the rail on, with
    // the reader's animation on as it is here, and the mark has to go on
    // with it. A real mouse again: the mark is read once the rail has come
    // to rest, well on from where the mouse let it go. The flick is at 1 px
    // a millisecond however long this machine takes between two moves
    // (`flick`).
    it(`follows a glide of the strip to where it comes to rest, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({
        reading,
        currentIndex: 60,
        motion: { animate: true, ms: 80 },
      })
      const rail = railOf(host)
      const later = laterSide(rail)
      await firstReportPast()
      /** How far the mark stands from the line's start, and the room it has to move in. */
      const stand = () => {
        const line = lineOf(host).getBoundingClientRect()
        const mark = markBox(host)
        return {
          fromStart:
            later === 1 ? mark.left - line.left : line.right - mark.right,
          room: line.width - mark.width,
        }
      }
      const { x, y } = tileMiddle(rail, 62)
      const letGo = rememberLetGo(rail)

      await flick(x, y, 1)
      const atTheRelease = stand()
      await untilAtRest()

      // THE STATE THIS TEST NAMES: the rail has gone on since it was let go
      // by the 325 px a glide from that speed carries it, far enough that a
      // mark left where it was then would be seen to be.
      const on = Math.abs(rail.scrollLeft - (letGo.at(-1) as number))
      expect(on).toBeGreaterThanOrEqual(325 - 2)
      expect(on).toBeLessThanOrEqual(325 + 2)
      const { fromStart, room } = stand()
      const along = Math.abs(rail.scrollLeft) / travel(rail)
      expect(Math.abs(along * room - atTheRelease.fromStart)).toBeGreaterThan(8)

      expect(fromStart).toBeCloseTo(along * room, 0)
    })

    // The same bars and the same strip throughout: a longer volume is handed
    // to them and then the short one again, as a change of edition does.
    it(`hides the mark while the strip shows the whole volume, and draws it once it does not, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...FITS, reading, currentIndex: 0 })
      const rail = railOf(host)
      const mark = markOf(host)

      // THE STATE THIS TEST NAMES: tiles are drawn, and none is out of sight.
      expect(rail.querySelectorAll('button')).toHaveLength(3)
      expect(travel(rail)).toBe(0)
      // Hidden, and by that not drawn: the stylesheet's rule for the mark
      // sets nothing that would show a hidden element.
      expect(mark.hidden).toBe(true)
      expect(getComputedStyle(mark).display).toBe('none')

      rerender(reader({ ...LONG, reading, currentIndex: 0 }))
      expect(railOf(host)).toBe(rail)
      expect(markOf(host)).toBe(mark)
      expect(travel(rail)).toBeGreaterThan(0)
      expect(mark.hidden).toBe(false)
      // Drawn where a strip mounted on the long volume draws it: at the
      // line's start, as wide as the rail's numbers say.
      const line = lineOf(host).getBoundingClientRect()
      const box = markBox(host)
      expect(box.width).toBeCloseTo(widthDue(rail, lineOf(host)), 0)
      if (laterSide(rail) === 1) {
        expect(box.left).toBeCloseTo(line.left, 0)
      } else {
        expect(box.right).toBeCloseTo(line.right, 0)
      }

      rerender(reader({ ...FITS, reading, currentIndex: 0 }))
      expect(travel(rail)).toBe(0)
      expect(mark.hidden).toBe(true)
      expect(getComputedStyle(mark).display).toBe('none')
    })

    // The thumbnails the strip asks for never load here, so the late ones
    // are made once the strip has drawn and told (`loadThumbnailsLate`).
    it(`is as wide as the rail's numbers say after thumbnails load late, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)
      await firstReportPast()

      // THE FIXTURE: the share on show lies between the bounds, so the mark
      // is the share and a change to it shows.
      expect(onShowPx(rail, line)).toBeGreaterThan(MARK_MIN_PX + 1)
      expect(onShowPx(rail, line)).toBeLessThan(MARK_MAX_PX - 1)
      const before = rail.scrollWidth
      const stale = markBox(host).width
      expect(stale).toBeCloseTo(widthDue(rail, line), 0)

      await loadThumbnailsLate(rail)

      // THE STATE THIS TEST NAMES: the rail is longer than it was, still
      // where it stood, its share on show still between the bounds, and by
      // enough that a mark left as it was would show.
      expect(rail.scrollWidth).toBeGreaterThan(before)
      expect(rail.scrollLeft).toBe(0)
      expect(onShowPx(rail, line)).toBeGreaterThan(MARK_MIN_PX + 1)
      const due = widthDue(rail, line)
      expect(stale - due).toBeGreaterThan(5)

      expect(markBox(host).width).toBeCloseTo(due, 0)
    })

    it(`is as wide as the rail's numbers say after the window is made narrower, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)
      await firstReportPast()

      // THE FIXTURE: the share on show lies between the bounds, so the mark
      // is the share and a change to it shows.
      expect(onShowPx(rail, line)).toBeGreaterThan(MARK_MIN_PX + 1)
      expect(onShowPx(rail, line)).toBeLessThan(MARK_MAX_PX - 1)
      const wide = rail.clientWidth
      const stale = markBox(host).width
      expect(stale).toBeCloseTo(widthDue(rail, line), 0)

      // Waited for by what it does and not by time: first the rail is laid
      // out narrower, then two frames, since the observer is called in the
      // frame that lays the narrower window out, after that frame's own
      // callbacks have run.
      await act(async () => {
        await page.viewport(1000, 900)
        while (rail.clientWidth >= wide) await frame()
        await frame()
        await frame()
      })

      // THE STATE THIS TEST NAMES: a narrower rail over the same tiles, still
      // overflowing, its share on show still between the bounds, and by
      // enough that a mark left as it was would show.
      expect(rail.clientWidth).toBeLessThan(wide)
      expect(travel(rail)).toBeGreaterThan(0)
      expect(onShowPx(rail, line)).toBeGreaterThan(MARK_MIN_PX + 1)
      expect(onShowPx(rail, line)).toBeLessThan(MARK_MAX_PX - 1)
      const due = widthDue(rail, line)
      expect(stale - due).toBeGreaterThan(5)

      expect(markBox(host).width).toBeCloseTo(due, 0)
    })
  }

  // The class and the token meeting: `index.css.test.ts` holds the token's
  // value and `classNames.test.ts` that the class has a rule.
  it('is drawn in the reader’s mark colour, which lets the line through', async () => {
    await page.viewport(1440, 900)
    const host = inReader({ currentIndex: 0 })
    const mark = markOf(host)
    expect(mark.hidden).toBe(false)
    expect(getComputedStyle(mark).backgroundColor).toBe(
      'rgba(236, 228, 212, 0.5)',
    )
  })
})

/** All five readouts on, each at its longest: longer than a phone's row. */
const LONGEST: Partial<ChromeProps> = {
  readouts: {
    position: true,
    time: true,
    chapter: true,
    health: true,
    notes: true,
  },
  position: 'digital-colored-comics slot 188 / 195 · p. 190 · 96%',
  health: 'fragmented pair · no counterpart here',
  time: '12m read · ~22m left',
  chapter: '~c002 · 3 of 7',
  notes: '12 notes here of 140',
}

/** The row's side padding, as `Chrome.tsx` sets it. */
const PADDING_PX = 12

const readoutOf = (host: HTMLElement) =>
  host.querySelector('.reader-readout') as HTMLElement

/**
 * The box the row's text is in, checked to be the row's one node: text
 * straight in the flex row is an anonymous item, which `text-overflow`
 * cannot reach.
 */
function textOf(row: HTMLElement): HTMLElement {
  expect(row.children.length).toBe(1)
  expect(row.childNodes.length).toBe(1)
  return row.children[0] as HTMLElement
}

describe('O141: the readout row at a phone’s width', () => {
  it('ends in … inside its padding, on one line, 32 px high', async () => {
    await page.viewport(390, 844)
    const row = readoutOf(inReader(LONGEST))
    const text = textOf(row)
    // THE FIXTURE TELLS: text longer than its box, which is what the "…" is for.
    expect(text.scrollWidth).toBeGreaterThan(text.clientWidth)
    expect(text.textContent).toBe(
      'digital-colored-comics slot 188 / 195 · p. 190 · 96% · fragmented pair · no counterpart here · 12m read · ~22m left · ~c002 · 3 of 7 · 12 notes here of 140',
    )
    // `text-overflow` acts only on a block box's own text: a flex or grid
    // span would hand its text to an anonymous item, the O141 defect again.
    expect(getComputedStyle(text).display).toBe('block')
    expect(getComputedStyle(text).textOverflow).toBe('ellipsis')
    const rowBox = row.getBoundingClientRect()
    const box = text.getBoundingClientRect()
    expect(box.left).toBeGreaterThanOrEqual(rowBox.left + PADDING_PX - 0.5)
    expect(box.right).toBeLessThanOrEqual(rowBox.right - PADDING_PX + 0.5)
    expect(box.top).toBeGreaterThanOrEqual(rowBox.top)
    expect(box.bottom).toBeLessThanOrEqual(rowBox.bottom)
    expect(rowBox.height).toBe(32)
  })

  it.each([
    [390, 844],
    [1280, 800],
  ])(
    'a readout that fits is whole and centred, at %i×%i',
    async (width, height) => {
      await page.viewport(width, height)
      const row = readoutOf(inReader({ position: 'slot 1 / 3' }))
      const text = textOf(row)
      expect(text.scrollWidth).toBeLessThanOrEqual(text.clientWidth)
      // The glyphs, not the box: a box stretched across the row is centred
      // however its text sits in it.
      const glyphs = document.createRange()
      glyphs.selectNodeContents(text)
      const off =
        centre(glyphs.getBoundingClientRect()) -
        centre(row.getBoundingClientRect())
      expect(Math.abs(off)).toBeLessThanOrEqual(1)
    },
  )

  it('is whole at a desktop’s width', async () => {
    await page.viewport(1280, 800)
    const text = textOf(readoutOf(inReader(LONGEST)))
    expect(text.scrollWidth).toBeLessThanOrEqual(text.clientWidth)
  })
})

/**
 * O425: Help ends the readout row wherever a mouse or a trackpad is, and a
 * touch screen alone draws none. The row's text gives way to it, so at a
 * phone's width, where the text is cut short, the two never meet and nothing
 * leaves the row; under a finger it is a 44 px press (spec 2026-09-17 §8.3),
 * which the row grows to hold.
 */
describe('O425: Help at the readout row’s end', () => {
  const helpOf = (row: HTMLElement) =>
    row.querySelector('button[aria-label="Help"]') as HTMLElement

  it('stands clear of the text and inside the row, at 390×844', async () => {
    await page.viewport(390, 844)
    const row = readoutOf(inReader({ ...LONGEST, onHelp: () => {} }))
    const text = row.querySelector('span') as HTMLElement
    const help = helpOf(row)
    // THE FIXTURE TELLS: text longer than the whole row, which would run on
    // under Help or push it out of the row if it did not give way.
    expect(text.scrollWidth).toBeGreaterThan(row.clientWidth)
    const rowBox = row.getBoundingClientRect()
    const textBox = text.getBoundingClientRect()
    const helpBox = help.getBoundingClientRect()
    expect(overlapOf(areaOf(textBox), areaOf(helpBox))).toBe(0)
    expect(textBox.left).toBeGreaterThanOrEqual(rowBox.left + PADDING_PX - 0.5)
    expect(helpBox.right).toBeLessThanOrEqual(rowBox.right - PADDING_PX + 0.5)
    expect(helpBox.top).toBeGreaterThanOrEqual(rowBox.top)
    expect(helpBox.bottom).toBeLessThanOrEqual(rowBox.bottom)
    expect(row.scrollWidth).toBe(row.clientWidth)
    const bar = row.closest('.reader-bottom-bar') as HTMLElement
    expect(bar.scrollWidth).toBe(bar.clientWidth)
    expect(rowBox.height).toBe(32)
  })

  it.each([
    [390, 844],
    [1280, 800],
  ])(
    'stands at the row’s end beside a short readout, whole and centred in the room left, at %i×%i',
    async (width, height) => {
      await page.viewport(width, height)
      const row = readoutOf(
        inReader({ position: 'slot 1 / 3', onHelp: () => {} }),
      )
      const text = row.querySelector('span') as HTMLElement
      expect(text.scrollWidth).toBeLessThanOrEqual(text.clientWidth)
      const rowBox = row.getBoundingClientRect()
      const helpBox = helpOf(row).getBoundingClientRect()
      expect(helpBox.right).toBeCloseTo(rowBox.right - PADDING_PX, 1)
      // The glyphs, between the row's padding and Help less the row's 4 px
      // gap: the readout is still centred, in the room Help leaves it.
      const glyphs = document.createRange()
      glyphs.selectNodeContents(text)
      const room = (rowBox.left + PADDING_PX + helpBox.left - 4) / 2
      expect(
        Math.abs(centre(glyphs.getBoundingClientRect()) - room),
      ).toBeLessThanOrEqual(1)
    },
  )

  it('leaves a short readout centred in the whole row under a touch screen alone, Help not drawn', async () => {
    onTestFinished(pointerRulesAs('phone'))
    await page.viewport(390, 844)
    const row = readoutOf(
      inReader({ position: 'slot 1 / 3', onHelp: () => {} }),
    )
    // The fixture tells: Help is in the row, undrawn.
    expect(helpOf(row).checkVisibility()).toBe(false)
    const glyphs = document.createRange()
    glyphs.selectNodeContents(row.querySelector('span') as HTMLElement)
    const off =
      centre(glyphs.getBoundingClientRect()) -
      centre(row.getBoundingClientRect())
    expect(Math.abs(off)).toBeLessThanOrEqual(1)
  })

  it.each([
    ['mouse', true, 32],
    ['touch laptop', true, 32],
    ['tablet with trackpad', true, 44],
    ['phone', false, 44],
  ] as const)(
    'is drawn under a %s’s pointers: %s, in a row %i px tall',
    async (device, drawn, height) => {
      onTestFinished(pointerRulesAs(device))
      await page.viewport(390, 844)
      const row = readoutOf(inReader({ ...LONGEST, onHelp: () => {} }))
      expect([
        helpOf(row).checkVisibility(),
        row.getBoundingClientRect().height,
      ]).toEqual([drawn, height])
    },
  )

  /**
   * Where Help is drawn under a finger, a trackpad is there too, and so is
   * the progress line's grip, which takes the top of the row under it,
   * Help's included.
   */
  it('is 44 px under a finger, inside a row grown to hold it, and takes a press below the progress line’s grip', async () => {
    onTestFinished(pointerRulesAs('tablet with trackpad'))
    await page.viewport(390, 844)
    const row = readoutOf(inReader({ ...LONGEST, onHelp: () => {} }))
    const help = helpOf(row)
    const box = help.getBoundingClientRect()
    expect(box.width).toBeGreaterThanOrEqual(44)
    expect(box.height).toBeGreaterThanOrEqual(44)
    const bar = row.closest('.reader-bottom-bar') as HTMLElement
    const grip = (
      bar.querySelector('.reader-progress-grip') as HTMLElement
    ).getBoundingClientRect()
    // The fixture tells: the grip is drawn, and reaches past Help's top.
    expect(grip.bottom).toBeGreaterThan(box.top)
    // A press a pixel inside each edge, the top's below the grip, and at the
    // middle, lands on Help. Hit tested point by point rather than walked
    // out (`pressArea`): the bar's bottom is the window's, where a walk
    // cannot go past.
    const middle = { x: box.left + box.width / 2, y: box.top + box.height / 2 }
    const reached = [
      [middle.x, middle.y],
      [middle.x, grip.bottom + 1],
      [middle.x, box.bottom - 1],
      [box.left + 1, middle.y],
      [box.right - 1, middle.y],
    ].map(([x, y]) => document.elementFromPoint(x, y) === help)
    expect(reached).toEqual([true, true, true, true, true])
    const rowBox = row.getBoundingClientRect()
    expect(box.top).toBeGreaterThanOrEqual(rowBox.top)
    expect(box.bottom).toBeLessThanOrEqual(rowBox.bottom)
    expect(rowBox.height).toBe(44)
  })
})
