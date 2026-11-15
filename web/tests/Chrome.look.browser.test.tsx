import { act } from 'react'
import { describe, expect, it, onTestFinished } from 'vitest'
import { cdp, page } from 'vitest/browser'
// Required, for the reason `Filmstrip.browser.test.tsx` gives at its own
// import: the line's rules, its registered number and the tokens it is
// drawn in reach a page through this.
import '../src/index.css'
import { Chrome, type ChromeProps } from '../src/Chrome'
import { channels, tokenColour } from './support/colour'
import { apart, drawn, pixelAt } from './support/drawn'
import { mount } from './support/mount'
import { mouse } from './support/mouse'
import {
  farEnd,
  firstReportPast,
  frame,
  inReader,
  LONG,
  lineOf,
  loadThumbnailsLate,
  markBox,
  markOf,
  props,
  railOf,
  SHORT,
  scrollRail,
} from './support/readerBars'

/**
 * O286: how the progress line is drawn and eased as it wakes and sleeps.
 * Awake it is drawn thicker, and the line, its fill and the mark grow as one
 * about the line's centre, with nothing else in the bars moving; the fill's
 * end at the reader's place is round and its start square. Every case runs
 * in both reading directions, since the line is mirrored for a work read
 * right to left.
 */

/** The reader's animation setting on, so the line's easing runs. */
const ANIMATED = { animate: true, ms: 200 }

/** At 1440 by 900, a point over the page, clear of both bars and far from the line. */
const OVER_PAGE = { x: 720, y: 300 }

const fillOf = (host: HTMLElement) =>
  host.querySelector('.reader-progress-fill') as HTMLElement

/** Every transition on the line and under it run to its end, so a size read is the one it settles at. */
function settle(line: HTMLElement) {
  for (const a of line.getAnimations({ subtree: true })) a.finish()
}

/** The line's own transition of `--grow`, if one is running. */
const growOf = (line: HTMLElement) =>
  line
    .getAnimations()
    .find(
      (a): a is CSSTransition =>
        a instanceof CSSTransition && a.transitionProperty === '--grow',
    )

/** The line's `--grow` as it is computed now. */
const grown = (line: HTMLElement) =>
  Number.parseFloat(getComputedStyle(line).getPropertyValue('--grow'))

/** Frame by frame until `done`, for up to 3 s. */
async function until(done: () => boolean, what: string) {
  const deadline = performance.now() + 3000
  while (!done()) {
    if (performance.now() > deadline) throw new Error(`no ${what} in 3 s`)
    await frame()
  }
}

/**
 * What `read` returns at the next change to the line's `data-awake`, read
 * as the change is made: a transition the change starts is caught before
 * any of it has run, however late this machine gets to it.
 */
function atNextChange<T>(line: HTMLElement, read: () => T) {
  const seen: { done: boolean; value?: T } = { done: false }
  const observer = new MutationObserver(() => {
    if (seen.done) return
    seen.done = true
    observer.disconnect()
    seen.value = read()
  })
  observer.observe(line, { attributes: true, attributeFilter: ['data-awake'] })
  onTestFinished(() => observer.disconnect())
  return async () => {
    await until(() => seen.done, 'change to the line')
    return seen.value as T
  }
}

/** The line's `--grow` transition, paused where it stands. */
function pausedGrow(line: HTMLElement) {
  const grow = growOf(line)
  grow?.pause()
  return grow
}

/**
 * The bars in a reader at 1440 by 900, with the mouse parked over the page
 * before they mount, so that the first move a case makes is a move.
 */
async function bars(overrides: Partial<ChromeProps>) {
  await page.viewport(1440, 900)
  await mouse.moveTo(OVER_PAGE.x - 40, OVER_PAGE.y)
  const host = inReader(overrides)
  await firstReportPast()
  return { host, line: lineOf(host) }
}

/** A real mouse move over the page, which wakes the line, and sleeps it a second later. */
async function moveOverPage() {
  await act(async () => {
    await mouse.moveTo(OVER_PAGE.x + 40, OVER_PAGE.y)
  })
}

/**
 * Wakes the line with a real mouse move to rest 12 px under it and left of
 * its left end, off the band, where it stays awake; waits for the frame that
 * wakes it.
 */
async function wakeNear(host: HTMLElement, line: HTMLElement) {
  const bar = (
    host.querySelector('.reader-bottom-bar') as HTMLElement
  ).getBoundingClientRect()
  const box = line.getBoundingClientRect()
  await act(async () => {
    await mouse.moveTo(bar.left + 4, box.bottom + 12)
  })
  await until(() => line.dataset.awake === '', 'wake')
}

describe('the progress line drawn asleep and awake', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    for (const [kind, volume] of [
      ['long', LONG],
      ['short', SHORT],
    ] as const) {
      it(`is 2 px with a 2 px fill and a 4 px mark at rest, and 4, 4 and 8 awake, all about the line's centre, ${kind} ${reading}`, async () => {
        const { host, line } = await bars({
          ...volume,
          reading,
          motion: ANIMATED,
        })
        const fill = fillOf(host)
        const colour = tokenColour(line, '--line')
        const shadows = (dy: number) =>
          `${colour} 0px ${-dy}px 0px 0px, ${colour} 0px ${dy}px 0px 0px`
        // The fill's and the mark's top and bottom, from the line's.
        const drawnNow = () => {
          settle(line)
          const box = line.getBoundingClientRect()
          const filled = fill.getBoundingClientRect()
          const mark = markBox(host)
          return {
            height: box.height,
            shadow: getComputedStyle(line).boxShadow,
            fill: [filled.top - box.top, filled.bottom - box.bottom],
            mark: [mark.top - box.top, mark.bottom - box.bottom],
          }
        }

        // THE STATE THIS TEST NAMES: asleep, with the mark drawn.
        expect(line.dataset.awake).toBeUndefined()
        expect(markOf(host).hidden).toBe(false)
        expect(drawnNow(), 'at rest').toEqual({
          height: 2,
          shadow: shadows(0),
          fill: [0, 0],
          mark: [-1, 1],
        })

        await wakeNear(host, line)
        expect(drawnNow(), 'awake').toEqual({
          height: 2,
          shadow: shadows(1),
          fill: [-1, 1],
          mark: [-3, 3],
        })
      })
    }

    it(`moves no other box as it wakes or sleeps, ${reading}`, async () => {
      const { host, line } = await bars({ reading, motion: ANIMATED })
      // Pictures that have loaded, so no thumbnail's box changes as its
      // request fails while the case reads.
      await loadThumbnailsLate(railOf(host))
      const fill = fillOf(host)
      const mark = markOf(host)
      const boxes = [
        ...host.querySelectorAll(
          '.reader-bottom-bar, .reader-bottom-bar *, .reader-top-bar, .reader-top-bar *',
        ),
      ].filter((el) => el !== fill && el !== mark)
      const rects = () => boxes.map((el) => el.getBoundingClientRect().toJSON())

      // THE STATE THIS TEST NAMES: asleep, the line and both bars' contents
      // among the boxes read.
      expect(line.dataset.awake).toBeUndefined()
      expect(boxes).toContain(line)
      expect(boxes).toContain(host.querySelector('.reader-readout'))
      expect(boxes).toContain(host.querySelector('.reader-back'))
      const asleep = rects()

      const woken = atNextChange(line, () => pausedGrow(line))
      await moveOverPage()
      const grow = await woken()
      expect(grow, 'a --grow transition').toBeDefined()
      if (grow) grow.currentTime = 75
      // Partway: the line's number neither asleep's nor awake's.
      expect(grown(line)).toBeGreaterThan(0.1)
      expect(grown(line)).toBeLessThan(0.9)
      expect(rects(), 'mid-wake').toEqual(asleep)

      settle(line)
      expect(rects(), 'awake').toEqual(asleep)

      const slept = atNextChange(line, () => true)
      await slept()
      settle(line)
      expect(line.dataset.awake).toBeUndefined()
      expect(rects(), 'asleep again').toEqual(asleep)
    })

    it(`grows the line, the fill and the mark as one, ${reading}`, async () => {
      const { host, line } = await bars({ reading, motion: ANIMATED })
      const fill = fillOf(host)

      const woken = atNextChange(line, () => pausedGrow(line))
      await moveOverPage()
      const grow = await woken()
      // THE STATE THIS TEST NAMES: the wake started a transition of the
      // line's number, for the case to stop at points along it.
      expect(grow, 'a --grow transition').toBeDefined()
      if (!grow) return
      expect(grow.effect?.getTiming()).toMatchObject({
        duration: 150,
        easing: 'ease-out',
      })
      for (const ms of [0, 37, 75, 112, 150]) {
        grow.currentTime = ms
        const now = grown(line)
        if (ms > 0 && ms < 150) {
          expect(now, `--grow at ${ms} ms`).toBeGreaterThan(0)
          expect(now, `--grow at ${ms} ms`).toBeLessThan(1)
        }
        const filled = (fill.getBoundingClientRect().height - 2) / 2
        const marked = (markBox(host).height - 4) / 4
        expect(Math.abs(filled - now), `the fill at ${ms} ms`).toBeLessThan(
          0.02,
        )
        expect(Math.abs(marked - now), `the mark at ${ms} ms`).toBeLessThan(
          0.02,
        )
      }
      grow.finish()

      const sleeping = atNextChange(line, () => pausedGrow(line))
      const sleep = await sleeping()
      expect(line.dataset.awake).toBeUndefined()
      expect(sleep?.effect?.getTiming()).toMatchObject({
        duration: 300,
        easing: 'ease-in-out',
      })
    })

    for (const asks of ['the reader’s animation off', 'reduced motion']) {
      it(`grows at once with ${asks}, ${reading}`, async () => {
        if (asks === 'reduced motion') {
          const emulate = (value: string) =>
            cdp().send('Emulation.setEmulatedMedia', {
              features: [{ name: 'prefers-reduced-motion', value }],
            })
          onTestFinished(async () => {
            await emulate('')
          })
          await emulate('reduce')
        }
        const { host, line } = await bars({
          reading,
          ...(asks === 'reduced motion' && { motion: ANIMATED }),
        })

        // THE STATE THIS TEST NAMES: asleep, and the one thing asking for
        // no easing is the one named.
        expect(line.dataset.awake).toBeUndefined()
        const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches
        if (asks === 'reduced motion') {
          expect(reduced).toBe(true)
          expect(line.dataset.still).toBeUndefined()
        } else {
          expect(reduced).toBe(false)
          expect(line.dataset.still).toBe('')
        }

        const woken = atNextChange(line, () => ({
          running: line.getAnimations().length,
          mark: markBox(host).height,
        }))
        await moveOverPage()
        expect(await woken()).toEqual({ running: 0, mark: 8 })
      })
    }
  }
})

describe('the progress line’s fill', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    it(`rounds the fill's end at the reader's place and squares its start, as computed, ${reading}`, async () => {
      const { host, line } = await bars({ reading, progress: 0.5 })
      const fill = fillOf(host)
      const corners = () => {
        settle(line)
        const style = getComputedStyle(fill)
        return {
          height: fill.getBoundingClientRect().height,
          end: [style.borderTopRightRadius, style.borderBottomRightRadius],
          start: [style.borderTopLeftRadius, style.borderBottomLeftRadius],
        }
      }

      // THE STATE THIS TEST NAMES: asleep, the fill half the line.
      expect(line.dataset.awake).toBeUndefined()
      const box = line.getBoundingClientRect()
      const filled = fill.getBoundingClientRect()
      expect(filled.width).toBeCloseTo(box.width / 2, 1)

      // It starts where the line starts for the reader.
      if (reading === 'ltr') expect(filled.left).toBeCloseTo(box.left, 1)
      else expect(filled.right).toBeCloseTo(box.right, 1)

      expect(corners(), 'at rest').toEqual({
        height: 2,
        end: ['1px', '1px'],
        start: ['0px', '0px'],
      })
      await wakeNear(host, line)
      expect(corners(), 'awake').toEqual({
        height: 4,
        end: ['2px', '2px'],
        start: ['0px', '0px'],
      })
    })

    // Four times over, so that the round end spans pixels enough to tell
    // from a square one. 1240 by 700 is drawn at its own size in this
    // project's window, so a pixel of the screenshot is one of the page.
    it(`rounds the fill's end at the reader's place and squares its start, as drawn, ${reading}`, async () => {
      await page.viewport(1240, 700)
      const host = mount(
        <div
          className="reader"
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            bottom: 0,
            width: 300,
            zoom: 4,
          }}
        >
          <Chrome {...props({ reading, progress: 0.5 })} />
        </div>,
      )
      await firstReportPast()
      const line = lineOf(host)
      const fill = fillOf(host)
      const rail = railOf(host)
      await scrollRail(rail, farEnd(rail))
      await wakeNear(host, line)
      settle(line)

      const box = line.getBoundingClientRect()
      const filled = fill.getBoundingClientRect()
      const mark = markBox(host)
      const [end, start] =
        reading === 'ltr'
          ? [filled.right - 1, filled.left]
          : [filled.left, filled.right - 1]
      const image = await drawn(line)
      // THE STATE THIS TEST NAMES: awake, the mark clear of the fill's two
      // ends, and the screenshot a pixel to a pixel of the page.
      expect(line.dataset.awake).toBe('')
      // 4 px clear of either side of each column read.
      for (const x of [end, start]) {
        expect(
          mark.right < x - 4 || mark.left > x + 5,
          `the mark at ${x}`,
        ).toBe(true)
      }
      expect([image.width, image.height]).toEqual([box.width, box.height])

      const accent = channels(tokenColour(line, '--accent'))
      const column = (x: number) =>
        Array.from({ length: image.height }, (_, y) =>
          apart(pixelAt(image, Math.floor(x - box.left), y), accent),
        )
      const atEnd = column(end)
      const atStart = column(start)
      // A pixel more than 60 from the accent is not the accent; one nearer
      // is, its edge smoothed. Every row of the screenshot is a fill row.
      const ACCENT = 60
      expect(filled.top).toBeLessThan(box.top)
      expect(filled.bottom).toBeGreaterThan(box.bottom)
      const [top, bottom] = [0, image.height - 1]
      const middle = [image.height / 2 - 1, image.height / 2]
      expect(atEnd[top], `the end's top, ${atEnd}`).toBeGreaterThan(ACCENT)
      expect(atEnd[bottom], `the end's bottom, ${atEnd}`).toBeGreaterThan(
        ACCENT,
      )
      for (const y of middle) {
        expect(atEnd[y], `the end's middle, ${atEnd}`).toBeLessThan(ACCENT)
      }
      for (const y of atStart.keys()) {
        expect(atStart[y], `the start, ${atStart}`).toBeLessThan(ACCENT)
      }
    })

    for (const at of [0, 1]) {
      it(`${at === 0 ? 'draws nothing at 0 %' : 'stays inside the line at 100 %'}, asleep and awake, ${reading}`, async () => {
        const { host, line } = await bars({ reading, progress: at })
        const fill = fillOf(host)
        const box = line.getBoundingClientRect()
        const read = () => {
          settle(line)
          return fill.getBoundingClientRect()
        }

        // THE STATE THIS TEST NAMES: asleep, the line drawn across the bar.
        expect(line.dataset.awake).toBeUndefined()
        expect(box.width).toBeGreaterThan(1000)
        for (const when of ['asleep', 'awake']) {
          if (when === 'awake') await wakeNear(host, line)
          const filled = read()
          if (at === 0) {
            expect(filled.width, when).toBe(0)
          } else {
            expect(filled.width, when).toBeCloseTo(box.width, 1)
            expect(filled.left, when).toBeGreaterThanOrEqual(box.left - 0.05)
            expect(filled.right, when).toBeLessThanOrEqual(box.right + 0.05)
          }
        }
      })
    }
  }
})
