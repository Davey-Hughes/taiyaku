import { act, Profiler } from 'react'
import { describe, expect, it, onTestFinished } from 'vitest'
import { page } from 'vitest/browser'
// Required, for the reason `Filmstrip.browser.test.tsx` gives at its own
// import: the reader's custom properties and the line's band reach a page
// through this.
import '../src/index.css'
import { Chrome, type ChromeProps } from '../src/Chrome'
import { mount, rerender } from './support/mount'
import { mouse } from './support/mouse'
import {
  atAlong,
  centre,
  firstReportPast,
  flick,
  frame,
  gripOf,
  inReader,
  LONG,
  laterSide,
  lineOf,
  lineRow,
  loadThumbnailsLate,
  markBox,
  markFrom,
  props,
  railOf,
  rememberLetGo,
  SHORT,
  tileMiddle,
  travel,
  untilAtRest,
  VERY_LONG,
  widthDue,
} from './support/readerBars'

/**
 * O286: the progress line as a scrollbar for a mouse, with a real mouse
 * (`support/mouse.ts`) on a real strip. A press on the mark drags it; a
 * press on the line away from it centres the mark on the pointer first.
 * Where the pointer is, is measured from the line's start as the reader sees
 * it (`atAlong`, `markFrom`), so each case is the same in both directions.
 */

const LENGTHS = [
  ['long', LONG],
  ['short', SHORT],
] as const

/** Moves the held pointer from `from` to `to` px along the line, 15 px a step, as a hand moves. */
async function dragAlong(
  host: HTMLElement,
  y: number,
  from: number,
  to: number,
) {
  const steps = Math.ceil(Math.abs(to - from) / 15)
  for (let step = 1; step <= steps; step++) {
    await mouse.moveTo(atAlong(host, from + ((to - from) * step) / steps), y)
  }
}

/**
 * How far, in px along the line, the mark stands from where the rail's
 * place would put it. A held mark is placed from the pointer, so a mark
 * under the pointer says nothing of the rail; this is what does.
 */
function railFrom(
  host: HTMLElement,
  mark: { start: number; width: number },
): number {
  const rail = railOf(host)
  const line = lineOf(host).getBoundingClientRect()
  const along = Math.abs(rail.scrollLeft) / travel(rail)
  return along * (line.width - mark.width) - mark.start
}

describe('the progress line under a mouse', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    for (const [kind, length] of LENGTHS) {
      it(`keeps the mark under the pointer where it was taken, through a drag of the mark, ${kind} ${reading}`, async () => {
        await page.viewport(1440, 900)
        const host = inReader({ ...length, reading, currentIndex: 0 })
        const rail = railOf(host)
        await firstReportPast()
        // A volume that overflows, which a short one has to as well.
        expect(travel(rail)).toBeGreaterThan(0)
        const y = lineRow(host)
        const stood = rail.scrollLeft
        const taken = markFrom(host)
        const from = taken.start + 0.3 * taken.width
        const misses: string[] = []

        await act(async () => {
          await mouse.moveTo(atAlong(host, from), y)
          await mouse.press()
          for (let move = 1; move <= 10; move++) {
            const at = from + 20 * move
            await mouse.moveTo(atAlong(host, at), y)
            await frame()
            await frame()
            const mark = markFrom(host)
            const off = mark.start + 0.3 * mark.width - at
            if (Math.abs(off) > 0.5) misses.push(`${move}: ${off.toFixed(2)}`)
            const stands = railFrom(host, mark)
            if (Math.abs(stands) > 0.5) {
              misses.push(`${move}: rail ${stands.toFixed(2)}`)
            }
          }
        })

        // THE STATE THIS TEST NAMES: a rail gone far enough with the pointer
        // that a mark left where it was taken would be seen to be.
        expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)
        const line = lineOf(host).getBoundingClientRect()
        const along = Math.abs(rail.scrollLeft) / travel(rail)
        expect(
          along * (line.width - taken.width) - taken.start,
        ).toBeGreaterThan(100)

        expect(misses).toEqual([])
      })

      it(`centres the mark on a press of the line away from it, and goes on as a drag, ${kind} ${reading}`, async () => {
        await page.viewport(1440, 900)
        const host = inReader({ ...length, reading, currentIndex: 0 })
        const rail = railOf(host)
        await firstReportPast()
        // THE STATE THIS TEST NAMES: a volume that overflows, the mark at the
        // line's start, and the press well clear of it.
        expect(travel(rail)).toBeGreaterThan(0)
        const y = lineRow(host)
        const pressed = 0.75 * lineOf(host).getBoundingClientRect().width
        const before = markFrom(host)
        expect(before.start).toBeCloseTo(0, 0)
        expect(pressed - (before.start + before.width)).toBeGreaterThan(100)

        const misses: string[] = []
        const check = (move: number, at: number) => {
          const mark = markFrom(host)
          const off = mark.start + mark.width / 2 - at
          if (Math.abs(off) > 0.5) misses.push(`${move}: ${off.toFixed(2)}`)
          const stands = railFrom(host, mark)
          if (Math.abs(stands) > 0.5) {
            misses.push(`${move}: rail ${stands.toFixed(2)}`)
          }
        }
        await act(async () => {
          await mouse.moveTo(atAlong(host, pressed), y)
          await mouse.press()
          await frame()
          await frame()
          check(0, pressed)
          for (let move = 1; move <= 3; move++) {
            const at = pressed + 20 * move
            await mouse.moveTo(atAlong(host, at), y)
            await frame()
            await frame()
            check(move, at)
          }
        })

        expect(misses).toEqual([])
      })

      // The pointer goes 100 px past each end of the line, outside the
      // window: the line has the pointer captured, so it hears it there.
      it(`stops at either end of the volume, ${kind} ${reading}`, async () => {
        await page.viewport(1440, 900)
        const host = inReader({
          ...length,
          reading,
          currentIndex: Math.floor(length.groups.length / 2),
        })
        const rail = railOf(host)
        await firstReportPast()
        // THE STATE THIS TEST NAMES: the rail stands clear of both its ends.
        expect(Math.abs(rail.scrollLeft)).toBeGreaterThan(100)
        expect(travel(rail) - Math.abs(rail.scrollLeft)).toBeGreaterThan(100)

        const y = lineRow(host)
        const width = lineOf(host).getBoundingClientRect().width
        const taken = markFrom(host)
        const from = taken.start + taken.width / 2

        await act(async () => {
          await mouse.moveTo(atAlong(host, from), y)
          await mouse.press()
          await dragAlong(host, y, from, -100)
          await frame()
          await frame()
        })
        expect(Math.abs(rail.scrollLeft)).toBeLessThanOrEqual(0.5)
        expect(Math.abs(markFrom(host).start)).toBeLessThanOrEqual(0.5)

        await act(async () => {
          await dragAlong(host, y, -100, width + 100)
          await frame()
          await frame()
        })
        expect(
          Math.abs(Math.abs(rail.scrollLeft) - travel(rail)),
        ).toBeLessThanOrEqual(1)
        const mark = markFrom(host)
        expect(Math.abs(mark.start + mark.width - width)).toBeLessThanOrEqual(
          0.5,
        )
      })
    }

    // With the reader's animation on, a drag of the strip let go while the
    // pointer is moving glides on; one of the line does not. The press is
    // 3 px above the line, in the band and over the rail's padding, and
    // clear of the mark, so a band that took no press would hand it to the
    // rail.
    it(`carries nothing on after the release, with the reader's animation on, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({
        reading,
        currentIndex: 60,
        motion: { animate: true, ms: 80 },
      })
      const rail = railOf(host)
      await firstReportPast()
      const line = lineOf(host).getBoundingClientRect()
      const pressed = line.width / 4
      const x = atAlong(host, pressed)
      const y = line.top - 3
      const mark = markFrom(host)
      const letGo = rememberLetGo(rail)

      // THE STATE THIS TEST NAMES: the press lands in the band, clear of the
      // mark, where the rail lies under the band.
      const grip = gripOf(host).getBoundingClientRect()
      expect(y).toBeGreaterThanOrEqual(grip.top)
      expect(mark.start - pressed).toBeGreaterThan(20)
      expect(document.elementsFromPoint(x, y)).toContain(rail)

      await flick(x, y, laterSide(rail))
      const released = letGo.at(-1) as number
      await untilAtRest()
      expect(Math.abs(rail.scrollLeft - released)).toBeLessThanOrEqual(1)

      // THE CONTROL: the same flick on a tile glides the rail on.
      const strip = rail.getBoundingClientRect()
      await flick(
        Math.round(strip.left + strip.width / 2),
        Math.round(strip.top + strip.height / 2),
        laterSide(rail),
      )
      const tileLetGo = letGo.at(-1) as number
      expect(tileLetGo).not.toBe(released)
      await untilAtRest()
      expect(Math.abs(rail.scrollLeft - tileLetGo)).toBeGreaterThan(100)
    })

    it(`stops a glide the rail is running, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({
        reading,
        currentIndex: 60,
        motion: { animate: true, ms: 80 },
      })
      const rail = railOf(host)
      await firstReportPast()
      const { x, y } = tileMiddle(rail, 62)
      await flick(x, y, laterSide(rail))

      // THE STATE THIS TEST NAMES: the rail is gliding.
      const read = rail.scrollLeft
      await act(async () => {
        await frame()
      })
      expect(rail.scrollLeft).not.toBe(read)

      let held = Number.NaN
      await act(async () => {
        await mouse.moveTo(centre(markBox(host)), lineRow(host))
        await mouse.press()
        await frame()
        await frame()
        held = rail.scrollLeft
      })
      await untilAtRest()
      expect(Math.abs(rail.scrollLeft - held)).toBeLessThanOrEqual(1)
    })

    // A drag of the line writes the rail's `scrollLeft` and nothing else, as
    // the wheel does, and the mark follows the rail by the path that renders
    // nothing (`Chrome.test.tsx`, "moves the mark for a scroll with no
    // render"). Five moves of 10 px, short of what rebuilds the window.
    it(`moves the rail with no commit of the bars or the strip, ${reading}`, async () => {
      await page.viewport(1440, 900)
      let commits = 0
      const bars = (overrides: Partial<ChromeProps>) => (
        <div className="reader" style={{ position: 'fixed', inset: 0 }}>
          <Profiler
            id="bars"
            onRender={() => {
              commits += 1
            }}
          >
            <Chrome {...props(overrides)} />
          </Profiler>
        </div>
      )
      const host = mount(bars({ reading, currentIndex: 60 }))
      const rail = railOf(host)
      await firstReportPast()
      const first = () => rail.querySelector('button')?.dataset.group
      const firstBefore = first()
      const stood = rail.scrollLeft
      const taken = markFrom(host)
      const from = taken.start + taken.width / 2
      const y = lineRow(host)
      const before = commits
      expect(before).toBeGreaterThan(0)

      await act(async () => {
        await mouse.moveTo(atAlong(host, from), y)
        await mouse.press()
        for (let move = 1; move <= 5; move++) {
          await mouse.moveTo(atAlong(host, from + 10 * move), y)
          await frame()
        }
        await frame()
      })

      // THE STATE THIS TEST NAMES: the rail has moved, and its window was
      // not rebuilt.
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)
      expect(first()).toBe(firstBefore)

      expect(commits).toBe(before)

      // THE CONTROL: the count does move for a render of the bars.
      await act(async () => {
        await mouse.release()
      })
      rerender(bars({ reading, currentIndex: 60, position: 'slot 2' }))
      expect(commits).toBe(before + 1)
    })
  }
})

/** Whether a spacer, which stands where tiles are not rendered, shows in the rail's visible box. */
function spacerInView(rail: HTMLElement): boolean {
  const view = rail.getBoundingClientRect()
  return [...rail.querySelectorAll('[data-testid="strip-spacer"]')].some(
    (spacer) => {
      const box = spacer.getBoundingClientRect()
      return box.right > view.left && box.left < view.right
    },
  )
}

/**
 * Where the pointer's last `pointerdown` fell, in window pixels. Read from
 * the event, so a case reckons from the place the line was pressed at and
 * not the place it asked the mouse to go.
 */
function rememberPress(): { x: number } {
  const pressed = { x: Number.NaN }
  const onDown = (event: PointerEvent) => {
    pressed.x = event.clientX
  }
  window.addEventListener('pointerdown', onDown, true)
  onTestFinished(() => window.removeEventListener('pointerdown', onDown, true))
  return pressed
}

/**
 * The rail's length changes under a held press: thumbnails load, and the
 * window rebuilds after a jump. The rail is stood again from the widths it
 * has then, so the mark stays under the pointer and a hold past the end
 * stays at the end.
 */
describe("a hold while the rail's length changes", () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    // Each sample is read in a frame of its own, asked for in the frame
    // before, so it runs ahead of the strip's own frame: what it reads is
    // what the frame before it painted, and anything written since.
    it(`keeps a still-held mark under the pointer in every frame as thumbnails load late, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)
      await firstReportPast()
      const y = lineRow(host)
      const taken = markFrom(host)
      const stood = rail.scrollLeft
      const length = rail.scrollWidth
      const pressed = rememberPress()

      await act(async () => {
        await mouse.moveTo(atAlong(host, taken.start + 0.3 * taken.width), y)
        await mouse.press()
      })
      const box = line.getBoundingClientRect()
      const pointer =
        laterSide(rail) === 1 ? pressed.x - box.left : box.right - pressed.x
      const grab = (pointer - taken.start) / taken.width
      const off = () => {
        const mark = markFrom(host)
        return mark.start + grab * mark.width - pointer
      }

      const samples: number[] = []
      // Not the `requestAnimationFrame` `untilAtRest` counts, or this loop
      // would keep the rail from ever being at rest.
      const nextFrame = window.requestAnimationFrame.bind(window)
      let sampling = true
      onTestFinished(() => {
        sampling = false
      })
      const sample = () => {
        if (!sampling) return
        samples.push(off())
        nextFrame(sample)
      }
      nextFrame(sample)
      await loadThumbnailsLate(rail)
      await untilAtRest()
      sampling = false

      // THE STATE THIS TEST NAMES: the rail grew by enough that a mark
      // placed by the new share alone, over a rail still where it stood,
      // would stand more than 2 px off the pointer.
      expect(rail.scrollWidth).toBeGreaterThan(length)
      const due = widthDue(rail, line)
      const byShare =
        (Math.abs(stood) / travel(rail)) * (box.width - due) + grab * due
      expect(Math.abs(byShare - pointer)).toBeGreaterThan(2)
      expect(samples.length).toBeGreaterThan(2)

      expect(
        samples.flatMap((s, i) =>
          Math.abs(s) > 1 ? [`${i}: ${s.toFixed(2)}`] : [],
        ),
      ).toEqual([])
      expect(Math.abs(samples.at(-1) as number)).toBeLessThanOrEqual(0.5)
      // And the rail stands where the mark says it does.
      const mark = markFrom(host)
      const along = Math.abs(rail.scrollLeft) / travel(rail)
      expect(
        Math.abs(along * (box.width - mark.width) - mark.start),
      ).toBeLessThanOrEqual(0.5)
    })

    it(`comes to rest with tiles in view and the mark under the pointer when held still after a jump into a long volume's middle, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...VERY_LONG, reading, currentIndex: 0 })
      const rail = railOf(host)
      const line = lineOf(host)
      await firstReportPast()
      const y = lineRow(host)
      const at = 0.6 * line.getBoundingClientRect().width
      const pressed = rememberPress()

      let jumped = false
      await act(async () => {
        await mouse.moveTo(atAlong(host, at), y)
        await mouse.press()
        jumped = spacerInView(rail)
      })
      // THE STATE THIS TEST NAMES: the press jumped the rail past the tiles
      // its window had, so a spacer stood where the reader looks.
      expect(jumped).toBe(true)

      await untilAtRest()
      // Still held: the rest came with the button down.
      expect(line.dataset.held).toBe('')
      expect(spacerInView(rail)).toBe(false)
      const box = line.getBoundingClientRect()
      const pointer =
        laterSide(rail) === 1 ? pressed.x - box.left : box.right - pressed.x
      const mark = markFrom(host)
      expect(
        Math.abs(mark.start + mark.width / 2 - pointer),
      ).toBeLessThanOrEqual(0.5)
      expect(Math.abs(railFrom(host, mark))).toBeLessThanOrEqual(0.5)
    })

    it(`reaches the end of a volume held past it while its length grows, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      const rail = railOf(host)
      await firstReportPast()
      const y = lineRow(host)
      const width = lineOf(host).getBoundingClientRect().width
      const taken = markFrom(host)
      const from = taken.start + taken.width / 2

      await act(async () => {
        await mouse.moveTo(atAlong(host, from), y)
        await mouse.press()
        await dragAlong(host, y, from, width + 100)
      })
      // Until the window has rebuilt at the end, so that its thumbnails are
      // the ones that load.
      await untilAtRest()
      const stood = Math.abs(rail.scrollLeft)
      expect(Math.abs(stood - travel(rail))).toBeLessThanOrEqual(1)

      await loadThumbnailsLate(rail)
      await untilAtRest()

      // THE STATE THIS TEST NAMES: the rail grew, by more than a rail left
      // where it stood would be seen to stop short of its end.
      expect(travel(rail) - stood).toBeGreaterThan(1)
      expect(
        Math.abs(Math.abs(rail.scrollLeft) - travel(rail)),
      ).toBeLessThanOrEqual(1)
    })

    // A drag of the line moves a long volume's rail many pixels for each of
    // the pointer's, so it meets the window's rebuilds at nearly every move.
    // Each frame has an `act` of its own, so a render the strip asks for in
    // one lands before the next. The time allowed is for a move every 15 px
    // across the whole line, and three frames after each.
    it(`drags a long volume's mark from end to end with tiles in view as each move settles, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ ...VERY_LONG, reading, currentIndex: 0 })
      const rail = railOf(host)
      await firstReportPast()
      const y = lineRow(host)
      const width = lineOf(host).getBoundingClientRect().width
      const taken = markFrom(host)
      const from = taken.start + taken.width / 2

      // THE STATE THIS TEST NAMES: past the window the strip opened with, a
      // spacer wider than the rail's visible box stands for the rest of the
      // volume, so a rail carried to its end over that window would show
      // nothing else.
      const spacers = rail.querySelectorAll('[data-testid="strip-spacer"]')
      const widest = Math.max(
        ...[...spacers].map((s) => s.getBoundingClientRect().width),
      )
      expect(widest).toBeGreaterThan(rail.clientWidth)

      await act(async () => {
        await mouse.moveTo(atAlong(host, from), y)
        await mouse.press()
      })
      const misses: string[] = []
      const steps = Math.ceil((width - from) / 15)
      for (let step = 1; step <= steps; step++) {
        await act(async () => {
          await mouse.moveTo(
            atAlong(host, Math.min(from + 15 * step, width)),
            y,
          )
        })
        for (let f = 0; f < 3; f++) {
          await act(async () => {
            await frame()
          })
        }
        if (spacerInView(rail)) misses.push(`${step}: ${rail.scrollLeft}`)
      }

      expect(misses).toEqual([])
      expect(
        Math.abs(Math.abs(rail.scrollLeft) - travel(rail)),
      ).toBeLessThanOrEqual(1)
      const mark = markFrom(host)
      expect(Math.abs(mark.start + mark.width - width)).toBeLessThanOrEqual(0.5)
    }, 30_000)
  }
})
