import { act } from 'react'
import { describe, expect, it, onTestFinished } from 'vitest'
import { page } from 'vitest/browser'
// Required, for the reason `Filmstrip.browser.test.tsx` gives at its own
// import: the reader's custom properties and the line's band reach a page
// through this.
import '../src/index.css'
import { rerender } from './support/mount'
import { mouse, pen } from './support/mouse'
import {
  atAlong,
  firstReportPast,
  frame,
  gripOf,
  inReader,
  laterSide,
  lineOf,
  lineRow,
  markFrom,
  railOf,
  reader,
} from './support/readerBars'
import { swipeAlong, tapAt } from './support/touch'

/**
 * O286: the progress line wakes for a real mouse, in real time, and sleeps
 * a second after the last move or release, unless the pointer rests within
 * 16 px of the line or a hold is on. A finger and a pen wake nothing. When
 * it sleeps is heard by an observer on `data-awake` as the change is made.
 */

type Change = { change: 'set' | 'gone'; at: number }

/** Each time `data-awake` comes onto `line` or goes from it from here on, and when. */
function awakeChanges(line: HTMLElement): Change[] {
  const seen: Change[] = []
  const observer = new MutationObserver((records) => {
    const at = performance.now()
    for (const r of records) {
      seen.push({ change: r.oldValue === null ? 'set' : 'gone', at })
    }
  })
  observer.observe(line, {
    attributes: true,
    attributeFilter: ['data-awake'],
    attributeOldValue: true,
  })
  onTestFinished(() => observer.disconnect())
  return seen
}

/**
 * When the window last heard a mouse move or let go, from here on. Called
 * after the bars mount, so it hears each one after the wake has: a rest
 * waited out from here is never cut short of the wake's own.
 */
function mouseHeard(): { at: number } {
  const heard = { at: Number.NaN }
  const hear = (event: PointerEvent) => {
    if (event.pointerType === 'mouse') heard.at = performance.now()
  }
  window.addEventListener('pointermove', hear, true)
  window.addEventListener('pointerup', hear, true)
  onTestFinished(() => {
    window.removeEventListener('pointermove', hear, true)
    window.removeEventListener('pointerup', hear, true)
  })
  return heard
}

/**
 * Sends a real mouse input inside `act`, and returns `performance.now()`
 * read just before it was sent. The wake hears the input no sooner, so a
 * sleep timed from here is never read shorter than the wake's own second.
 */
async function sentAt(input: () => Promise<void>): Promise<number> {
  let at = Number.NaN
  await act(async () => {
    at = performance.now()
    await input()
  })
  return at
}

/** How long after `since` the line went to sleep, waiting frame by frame for up to 3 s; nothing if it did not. */
async function asleepAfter(
  seen: Change[],
  since: number,
): Promise<number | undefined> {
  for (;;) {
    const gone = seen.find(({ change, at }) => change === 'gone' && at > since)
    if (gone) return gone.at - since
    if (performance.now() - since > 3000) return undefined
    await frame()
  }
}

/** Waits frame by frame until `ms` have passed since `since`. */
async function untilPast(since: number, ms: number) {
  while (performance.now() - since < ms) await frame()
}

const goneSince = (seen: Change[], since: number) =>
  seen.filter(({ change, at }) => change === 'gone' && at > since)

const barOf = (host: HTMLElement) =>
  host.querySelector('.reader-bottom-bar') as HTMLElement

/**
 * The bars in a reader at 1440 by 900, with the mouse parked in the
 * window's corner before they mount, so that the first move a case makes
 * is a move to somewhere else.
 */
async function bars(reading: 'ltr' | 'rtl') {
  await page.viewport(1440, 900)
  await mouse.moveTo(1, 1)
  const host = inReader({ reading, currentIndex: 0 })
  await firstReportPast()
  return { host, line: lineOf(host) }
}

describe('the progress line wakes for a mouse', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`wakes on a real mouse move over the page, and sleeps a second after, ${reading}`, async () => {
      const { host, line } = await bars(reading)
      const seen = awakeChanges(line)
      const bar = barOf(host).getBoundingClientRect()
      const y = Math.round(bar.top / 2)
      // THE STATE THIS TEST NAMES: asleep, and the point is over the page,
      // well above the bars.
      expect(line.dataset.awake).toBeUndefined()
      expect(bar.top - y).toBeGreaterThan(100)

      const moved = await sentAt(() => mouse.moveTo(720, y))
      await frame()
      await frame()
      expect(line.dataset.awake).toBe('')

      const slept = await asleepAfter(seen, moved)
      expect(slept, 'asleep within 3 s').toBeDefined()
      expect(slept).toBeGreaterThanOrEqual(1000)
    })

    // Off the line's end, which is inset 12 px from the bar's, and 12 px
    // below its bottom, over the readout row. The move away goes 40 px above
    // the line, over the tiles: 40 px below it is under the window's edge.
    it(`stays awake while the mouse rests 12 px below the line near the bar's edge, and sleeps after it moves away, ${reading}`, async () => {
      const { host, line } = await bars(reading)
      const seen = awakeChanges(line)
      const heard = mouseHeard()
      const bar = barOf(host).getBoundingClientRect()
      const box = line.getBoundingClientRect()
      const x = bar.left + 4
      const y = box.bottom + 12
      // THE STATE THIS TEST NAMES: the point lies on the bar, past the
      // line's end, and in the zone.
      expect(box.left - x).toBeGreaterThan(4)
      expect(y).toBeLessThan(bar.bottom)

      const rested = await sentAt(() => mouse.moveTo(x, y))
      await untilPast(heard.at, 2000)
      expect(goneSince(seen, rested)).toEqual([])
      expect(line.dataset.awake).toBe('')

      const away = await sentAt(() => mouse.moveTo(x, box.top - 40))
      const slept = await asleepAfter(seen, away)
      expect(slept, 'asleep within 3 s').toBeDefined()
      expect(slept).toBeGreaterThanOrEqual(1000)
    })

    // Taken by the mark's middle and dragged 40 px above the line, out of
    // the zone, where only the hold keeps it awake. Straight up, so the rail
    // stays where it stood.
    it(`stays awake through a still hold of 2 s, and sleeps a second after the release, ${reading}`, async () => {
      const { host, line } = await bars(reading)
      const seen = awakeChanges(line)
      const heard = mouseHeard()
      const mark = markFrom(host)
      const x = atAlong(host, mark.start + mark.width / 2)
      const box = line.getBoundingClientRect()

      await act(async () => {
        await mouse.moveTo(x, lineRow(host))
        await mouse.press()
      })
      const rested = await sentAt(() => mouse.moveTo(x, box.top - 40))
      // THE STATE THIS TEST NAMES: a hold, resting out of the zone.
      expect(line.dataset.held).toBe('')
      await untilPast(heard.at, 2000)
      expect(goneSince(seen, rested)).toEqual([])
      expect(line.dataset.awake).toBe('')

      const released = await sentAt(() => mouse.release())
      expect(line.dataset.held).toBeUndefined()
      const slept = await asleepAfter(seen, released)
      expect(slept, 'asleep within 3 s').toBeDefined()
      expect(slept).toBeGreaterThanOrEqual(1000)
    })

    it(`wakes for no finger and no pen, ${reading}`, async () => {
      const { host, line } = await bars(reading)
      const seen = awakeChanges(line)
      const y = Math.round(barOf(host).getBoundingClientRect().top / 2)

      await swipeAlong(400, y, 0, 150)
      await act(async () => {
        for (let move = 0; move <= 4; move++) {
          await pen.moveTo(600 + 10 * move, y)
        }
      })
      await frame()
      await frame()
      await untilPast(performance.now(), 100)
      expect(seen).toEqual([])

      // THE CONTROL: a mouse over the same page wakes it.
      await act(async () => {
        await mouse.moveTo(700, y)
      })
      await frame()
      await frame()
      expect(line.dataset.awake).toBe('')
    })

    // A tap is the one touch that brings a mouse's events after it, to the
    // band here: just under the band, out of a tile's reach, Chromium's touch
    // adjustment hands the tap to it, as the press file's touch laptop case
    // finds.
    it(`wakes for no finger's tap on the band, ${reading}`, async () => {
      const { host, line } = await bars(reading)
      const seen = awakeChanges(line)
      const reached: string[] = []
      const onDown = (event: Event) => {
        if (event.target === gripOf(host)) reached.push(event.type)
      }
      for (const type of ['pointerdown', 'mousedown']) {
        window.addEventListener(type, onDown, true)
        onTestFinished(() => window.removeEventListener(type, onDown, true))
      }
      const box = line.getBoundingClientRect()
      const x = atAlong(host, box.width / 4)

      await tapAt(x, box.bottom + 6)
      await frame()
      await frame()
      await untilPast(performance.now(), 100)
      // THE STATE THIS TEST NAMES: the tap, and the mouse's events it
      // brings, reached the band.
      expect(reached).toEqual(['pointerdown', 'mousedown'])
      expect(seen).toEqual([])

      // THE CONTROL: a mouse over the same place wakes it.
      await act(async () => {
        await mouse.moveTo(x, box.bottom + 6)
      })
      await frame()
      await frame()
      expect(line.dataset.awake).toBe('')
    })

    // The bars hidden mid-hold, as Escape hides them through `App`, while
    // the button stays down; let go with no bars up, and shown again under
    // the pointer. The move after goes straight up, off the line, so the
    // new line hears nothing from it.
    it(`ends a hold the bars' hiding cuts off, ${reading}`, async () => {
      const overrides = { reading, currentIndex: 0 }
      const { host, line } = await bars(reading)
      const rail = railOf(host)
      const side = laterSide(rail)
      const y = lineRow(host)
      const mark = markFrom(host)
      const x = atAlong(host, mark.start + mark.width / 2)
      const stood = rail.scrollLeft

      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, side * 40)
        await frame()
        await frame()
      })
      // THE STATE THIS TEST NAMES: a hold that moves the rail.
      expect(line.dataset.held).toBe('')
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)

      rerender(reader({ ...overrides, visible: false }))
      await act(async () => {
        await mouse.along(x, y, side * 40, side * 100)
        await mouse.release()
      })
      rerender(reader(overrides))
      await firstReportPast()
      const again = lineOf(host)
      const railAgain = railOf(host)
      expect(again).not.toBe(line)
      expect(again.dataset.held).toBeUndefined()
      const seen = awakeChanges(again)
      const before = railAgain.scrollLeft

      const moved = await sentAt(() => mouse.moveTo(x + side * 100, y - 60))
      await act(async () => {
        await frame()
        await frame()
      })
      expect(railAgain.scrollLeft).toBe(before)
      const slept = await asleepAfter(seen, moved)
      expect(slept, 'asleep within 3 s').toBeDefined()
      expect(slept).toBeGreaterThanOrEqual(1000)
    })
  }
})
