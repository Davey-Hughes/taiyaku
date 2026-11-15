import { act } from 'react'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// Required, for the reason `Filmstrip.browser.test.tsx` gives at its own
// import: the reader's custom properties and the line's band reach a page
// through this.
import '../src/index.css'
import type { ChromeProps } from '../src/Chrome'
import { mount } from './support/mount'
import { mouse, pen } from './support/mouse'
import { near, pointerRulesAs, pressArea } from './support/pointer'
import {
  atAlong,
  centre,
  FITS,
  firstReportPast,
  frame,
  gripOf,
  inReader,
  laterSide,
  lineOf,
  lineRow,
  markFrom,
  markOf,
  railOf,
  reader,
  tileMiddle,
  travel,
  untilAtRest,
} from './support/readerBars'
import { swipeAlong, tapAt } from './support/touch'

/**
 * O286: the press band over the progress line, with a real mouse, pen and
 * finger. Only a mouse's primary button is answered, in a 10 px band that
 * covers neither a tile nor the readout's text, and a press there leaves
 * focus, the selection and the page as they were, whether or not there is a
 * mark to drag.
 */

/** Each time `data-held` comes onto `line` or goes from it from here on, read when called. */
function heldChanges(line: HTMLElement): () => string[] {
  const seen: string[] = []
  const record = (records: MutationRecord[]) => {
    for (const r of records) seen.push(r.oldValue === null ? 'set' : 'gone')
  }
  const observer = new MutationObserver(record)
  observer.observe(line, {
    attributes: true,
    attributeFilter: ['data-held'],
    attributeOldValue: true,
  })
  onTestFinished(() => observer.disconnect())
  return () => {
    record(observer.takeRecords())
    return seen
  }
}

/** The bars in a reader, and a button outside them that can hold focus. */
function withElsewhere(overrides: Partial<ChromeProps>) {
  const host = mount(
    <>
      {reader(overrides)}
      <button type="button">elsewhere</button>
    </>,
  )
  const elsewhere = host.querySelector(':scope > button') as HTMLElement
  return { host, elsewhere }
}

/** Selects the readout row's text, as a reader may have before reaching for the line, and returns it. */
function selectReadout(host: HTMLElement): string {
  const text = host.querySelector('.reader-readout > span') as HTMLElement
  getSelection()?.selectAllChildren(text)
  return text.textContent ?? ''
}

describe('the press band over the progress line', () => {
  for (const reading of ['ltr', 'rtl'] as const) {
    // At a window narrow enough that half the band lies within the 400 px
    // that `pressArea` walks out from its middle.
    it(`takes a press only in a 10 px band centred on the line, across its width, ${reading}`, async () => {
      onTestFinished(pointerRulesAs('mouse'))
      await page.viewport(720, 900)
      const host = inReader({ reading, currentIndex: 0 })
      await firstReportPast()
      const lineElement = lineOf(host)
      for (const a of lineElement.getAnimations({ subtree: true })) a.finish()
      const line = lineElement.getBoundingClientRect()

      // THE STATE THIS TEST NAMES: the line at rest, 2 px on whole rows, so
      // 4 px either side of it is 10.
      expect(lineElement.dataset.awake).toBeUndefined()
      expect(line.height).toBe(2)
      expect(Number.isInteger(line.top)).toBe(true)

      expect(pressArea(gripOf(host))).toEqual(
        near({
          top: line.top - 4,
          bottom: line.bottom + 4,
          left: line.left,
          right: line.right,
          width: line.width,
          height: 10,
        }),
      )
    })

    // What a tile and the readout's text take on their own is read with the
    // band drawn nowhere, so a band over either would be seen to cut it. At
    // the narrow window of the case above, for the same reason.
    it(`leaves a tile's press above the band and the readout's text below it as they were, ${reading}`, async () => {
      onTestFinished(pointerRulesAs('mouse'))
      await page.viewport(720, 900)
      const onNavigate = vi.fn()
      const host = inReader({ reading, currentIndex: 0, onNavigate })
      await firstReportPast()
      const rail = railOf(host)
      const grip = gripOf(host)
      const line = lineOf(host).getBoundingClientRect()
      const band = pressArea(grip)
      const inView = [...rail.querySelectorAll('button')].filter((tile) => {
        const box = tile.getBoundingClientRect()
        return box.left >= 0 && box.right <= window.innerWidth
      })
      // The last of the lowest, so not the current tile: a press that turned
      // to the page the reader is on would not be taken for this click.
      const lowest = inView.reduce((a, b) =>
        b.getBoundingClientRect().bottom >= a.getBoundingClientRect().bottom
          ? b
          : a,
      )
      const text = host.querySelector('.reader-readout > span') as HTMLElement
      const middle = line.top + line.height / 2
      const x = Math.round(centre(lowest.getBoundingClientRect()))
      const above = middle - 12
      grip.style.display = 'none'
      const tileAlone = pressArea(lowest)
      const textAlone = pressArea(text)
      const clickedAlone = document.elementFromPoint(x, above)
      grip.style.display = ''

      expect(tileAlone.bottom).toBeLessThanOrEqual(band.top)
      expect(textAlone.top).toBeGreaterThanOrEqual(band.bottom)

      const click = async () => {
        await act(async () => {
          await mouse.moveTo(x, above)
          await mouse.press()
          await mouse.release()
          await frame()
        })
      }
      // THE STATE THIS TEST NAMES: the click lands inside the keep zone, on
      // the tile's own press.
      expect(middle - above).toBeLessThanOrEqual(16 + line.height / 2)
      expect(lowest.contains(clickedAlone)).toBe(true)
      const slot = Number(lowest.dataset.group)

      await click()
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(slot)

      // THE CONTROL: the same click with no band anywhere does the same.
      onNavigate.mockClear()
      grip.style.display = 'none'
      await click()
      grip.style.display = ''
      expect(onNavigate).toHaveBeenCalledExactlyOnceWith(slot)

      // Below the band, on the readout row, clear of the mark: a press the
      // band took there would jump the rail and hold the line.
      const below = middle + 12
      const along = line.width / 4
      const bx = atAlong(host, along)
      const held = heldChanges(lineOf(host))
      // THE STATE THIS TEST NAMES: in the keep zone and below the band's 4 px,
      // over a rail that has somewhere to go.
      expect(below - line.bottom).toBeGreaterThan(4)
      expect(below - line.bottom).toBeLessThanOrEqual(16)
      expect(travel(rail)).toBeGreaterThan(0)
      const mark = markFrom(host)
      expect(along - (mark.start + mark.width)).toBeGreaterThan(20)
      const stood = rail.scrollLeft

      await act(async () => {
        await mouse.moveTo(bx, below)
        await mouse.press()
        await mouse.along(bx, below, 0, 60)
        await frame()
        await frame()
      })
      expect(rail.scrollLeft).toBe(stood)
      expect(held()).toEqual([])
      await act(async () => {
        await mouse.release()
      })
    })

    it(`draws no band on a phone, ${reading}`, async () => {
      onTestFinished(pointerRulesAs('phone'))
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      await firstReportPast()
      const rail = railOf(host)
      const line = lineOf(host).getBoundingClientRect()
      const x = centre(line)
      const y = line.top - 3
      // THE STATE THIS TEST NAMES: 3 px above the line lies in the rail's
      // box, where a band would lie over it.
      const box = rail.getBoundingClientRect()
      expect(y).toBeGreaterThanOrEqual(box.top)
      expect(y).toBeLessThan(box.bottom)

      expect(gripOf(host).getClientRects()).toHaveLength(0)
      expect(rail.contains(document.elementFromPoint(x, y))).toBe(true)
    })

    // A tap where a mouse's press would jump the rail, clear of the mark. A
    // tap on the band's own rows never reaches it: Chromium's touch
    // adjustment hands it to the tile above. Just under the band, out of the
    // tile's reach, it hands the tap to the band, so the line's own rule is
    // what decides there.
    it(`on a touch laptop, a mouse drags the line and a finger does not, ${reading}`, async () => {
      onTestFinished(pointerRulesAs('touch laptop'))
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      await firstReportPast()
      const rail = railOf(host)
      const line = lineOf(host)
      const y = lineRow(host)
      const tapped = line.getBoundingClientRect().width / 4
      const pressedOn: EventTarget[] = []
      const onDown = (event: PointerEvent) => {
        if (event.target) pressedOn.push(event.target)
      }
      window.addEventListener('pointerdown', onDown, true)
      onTestFinished(() =>
        window.removeEventListener('pointerdown', onDown, true),
      )
      // THE STATE THIS TEST NAMES: the band is drawn, and well clear of the
      // mark where the finger presses.
      expect(gripOf(host).getClientRects().length).toBeGreaterThan(0)
      expect(markFrom(host).start - tapped).toBeGreaterThan(20)
      const held = heldChanges(line)
      const stood = rail.scrollLeft

      await tapAt(
        atAlong(host, tapped),
        line.getBoundingClientRect().bottom + 6,
      )
      await act(async () => {
        await frame()
        await frame()
      })
      // THE STATE THIS TEST NAMES: the finger's press reached the band.
      expect(pressedOn).toHaveLength(1)
      expect(pressedOn[0]).toBe(gripOf(host))
      expect(rail.scrollLeft).toBe(stood)
      expect(held()).toEqual([])

      // THE CONTROL: a finger's swipe on the tiles moves the strip.
      const tile = tileMiddle(rail, 60)
      await swipeAlong(tile.x, tile.y, 0, -laterSide(rail) * 150)
      await untilAtRest()
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(50)

      // And a mouse on the mark drags it.
      const swiped = rail.scrollLeft
      const mark = markFrom(host)
      const x = atAlong(host, mark.start + mark.width / 2)
      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, 40)
        await frame()
        await frame()
      })
      expect(Math.abs(rail.scrollLeft - swiped)).toBeGreaterThan(100)
      await act(async () => {
        await mouse.release()
      })
    })

    it(`takes no pen press, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 60 })
      await firstReportPast()
      const rail = railOf(host)
      const line = lineOf(host)
      const y = lineRow(host)
      const mark = markFrom(host)
      const x = atAlong(host, mark.start + mark.width / 2)
      // THE STATE THIS TEST NAMES: the pen lands on the band, over the mark.
      expect(document.elementFromPoint(x, y)).toBe(gripOf(host))
      expect(document.elementsFromPoint(x, y)).toContain(markOf(host))
      const held = heldChanges(line)
      const stood = rail.scrollLeft

      await act(async () => {
        await pen.moveTo(x, y)
        await pen.press()
        for (let move = 1; move <= 4; move++) {
          await pen.moveTo(x + 10 * move, y)
        }
        await frame()
        await frame()
      })
      expect(rail.scrollLeft).toBe(stood)
      expect(held()).toEqual([])
      await act(async () => {
        await pen.release()
      })

      // THE CONTROL: the same press and move with a mouse drags it.
      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, 40)
        await frame()
        await frame()
      })
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)
      await act(async () => {
        await mouse.release()
      })
    })

    // A drag of the mark, then a press of the line away from it (a jump),
    // each let go. The current tile is the eleventh, so its slot is not one a
    // call with no argument would match.
    it.each(['the current tile', 'elsewhere', 'nothing'] as const)(
      `turns no page, opens no tile, selects no text, and leaves focus where it was: %s, ${reading}`,
      async (where) => {
        await page.viewport(1440, 900)
        const onNavigate = vi.fn()
        const { host, elsewhere } = withElsewhere({
          reading,
          currentIndex: 10,
          onNavigate,
        })
        await firstReportPast()
        const rail = railOf(host)
        const line = lineOf(host)
        const current = rail.querySelector(
          '[aria-current="true"]',
        ) as HTMLElement
        const focused =
          where === 'the current tile'
            ? current
            : where === 'elsewhere'
              ? elsewhere
              : document.body
        act(() => {
          if (focused === document.body) {
            ;(document.activeElement as HTMLElement | null)?.blur()
          } else {
            focused.focus({ preventScroll: true })
          }
        })
        const text = selectReadout(host)
        // THE STATE THIS TEST NAMES: focus is where the case puts it, the
        // readout's text is selected, and the press lands on the band over a
        // mark that has somewhere to go.
        expect(document.activeElement).toBe(focused)
        expect(text).not.toBe('')
        expect(getSelection()?.toString()).toBe(text)
        expect(travel(rail)).toBeGreaterThan(0)
        const y = lineRow(host)
        const taken = markFrom(host)
        const from = taken.start + taken.width / 2
        expect(document.elementFromPoint(atAlong(host, from), y)).toBe(
          gripOf(host),
        )
        const kept = (after: string) => {
          expect(document.activeElement, after).toBe(focused)
          expect(getSelection()?.toString(), after).toBe(text)
          expect(onNavigate, after).not.toHaveBeenCalled()
        }

        const stood = rail.scrollLeft
        await act(async () => {
          await mouse.moveTo(atAlong(host, from), y)
          await mouse.press()
          for (let move = 1; move <= 4; move++) {
            await mouse.moveTo(atAlong(host, from + 15 * move), y)
          }
          await mouse.release()
          await frame()
          await frame()
        })
        expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)
        kept('a drag of the mark')

        const mark = markFrom(host)
        const off = mark.start + mark.width + 40
        const dragged = rail.scrollLeft
        await act(async () => {
          await mouse.moveTo(atAlong(host, off), y)
          await mouse.press()
          await mouse.release()
          await frame()
          await frame()
        })
        expect(rail.scrollLeft).not.toBe(dragged)
        kept('a press of the line off the mark')

        expect(line.getAttribute('aria-hidden')).toBe('true')
        for (const el of [line, gripOf(host), markOf(host)]) {
          expect(el.hasAttribute('tabindex'), el.className).toBe(false)
        }

        // THE CONTROL: the keys are the tile's as before the presses.
        if (focused === current) {
          await act(async () => {
            await userEvent.keyboard(' ')
            await frame()
          })
          expect(onNavigate).toHaveBeenCalledExactlyOnceWith(10)
        }
      },
    )

    it(`moves nothing and leaves focus where it was on a press of the band where the strip shows the whole volume, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const { host, elsewhere } = withElsewhere({
        ...FITS,
        reading,
        currentIndex: 0,
      })
      await firstReportPast()
      const rail = railOf(host)
      const line = lineOf(host)
      act(() => elsewhere.focus())
      const text = selectReadout(host)
      const box = line.getBoundingClientRect()
      const x = centre(box)
      const y = lineRow(host)
      // THE STATE THIS TEST NAMES: nothing to drag, and the press lands on
      // the band, with focus outside the bars and the readout selected.
      expect(markOf(host).hidden).toBe(true)
      expect(travel(rail)).toBe(0)
      expect(document.elementFromPoint(x, y)).toBe(gripOf(host))
      expect(document.activeElement).toBe(elsewhere)
      expect(text).not.toBe('')
      expect(getSelection()?.toString()).toBe(text)
      const held = heldChanges(line)

      await act(async () => {
        await mouse.moveTo(x, y)
        await mouse.press()
        await mouse.along(x, y, 0, 40)
        await mouse.release()
        await frame()
        await frame()
      })

      expect(document.activeElement).toBe(elsewhere)
      expect(getSelection()?.toString()).toBe(text)
      expect(held()).toEqual([])
    })

    // Up over the tiles and back with the button held, where a hover would
    // open a correction's preview. Each step in an `act` of its own, so what
    // a step renders is drawn before it is looked for.
    it(`drags the line while correcting, and opens no preview, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({
        reading,
        currentIndex: 0,
        correcting: { to: 'ed2' },
      })
      await firstReportPast()
      const rail = railOf(host)
      const preview = () => host.querySelector('.reader-preview-img')
      const y = lineRow(host)
      const taken = markFrom(host)
      const from = taken.start + 0.3 * taken.width
      const overTiles = tileMiddle(rail, 2).y
      const stood = rail.scrollLeft
      const previewed: number[] = []
      const step = async (n: number, send: () => Promise<void>) => {
        await act(async () => {
          await send()
          await frame()
          await frame()
        })
        if (preview()) previewed.push(n)
      }

      await step(0, async () => {
        await mouse.moveTo(atAlong(host, from), y)
        await mouse.press()
      })
      for (let move = 1; move <= 4; move++) {
        await step(move, () => mouse.moveTo(atAlong(host, from + 15 * move), y))
      }
      await step(5, () => mouse.moveTo(atAlong(host, from + 60), overTiles))
      await step(6, () => mouse.moveTo(atAlong(host, from + 60), y))
      // THE STATE THIS TEST NAMES: the pointer went over a tile, and the
      // rail went with the line.
      expect(
        document
          .elementFromPoint(atAlong(host, from + 60), overTiles)
          ?.closest('button'),
      ).not.toBeNull()
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)
      const mark = markFrom(host)
      expect(
        Math.abs(mark.start + 0.3 * mark.width - (from + 60)),
      ).toBeLessThanOrEqual(0.5)

      expect(previewed).toEqual([])
      await act(async () => {
        await mouse.release()
        await frame()
      })

      // THE CONTROL: a mouse resting on a tile with no button down does
      // open one.
      await act(async () => {
        await mouse.moveTo(atAlong(host, from + 60), overTiles)
        await frame()
      })
      expect(preview()).not.toBeNull()
    })

    // Out past the window's side with the line held, let go there, and back
    // over the line with no button down.
    it(`ends a hold let go outside the window, ${reading}`, async () => {
      await page.viewport(1440, 900)
      const host = inReader({ reading, currentIndex: 0 })
      await firstReportPast()
      const rail = railOf(host)
      const line = lineOf(host)
      const y = lineRow(host)
      const taken = markFrom(host)
      const from = taken.start + taken.width / 2
      const width = line.getBoundingClientRect().width
      const heard: number[] = []
      const onMove = (event: PointerEvent) => heard.push(event.clientX)
      line.addEventListener('pointermove', onMove)
      onTestFinished(() => line.removeEventListener('pointermove', onMove))
      const stood = rail.scrollLeft

      await act(async () => {
        await mouse.moveTo(atAlong(host, from), y)
        await mouse.press()
        for (let move = 1; move <= 4; move++) {
          await mouse.moveTo(atAlong(host, from + 15 * move), y)
        }
        await frame()
        await frame()
      })
      // THE STATE THIS TEST NAMES: a hold that moves the rail.
      expect(line.dataset.held).toBe('')
      expect(Math.abs(rail.scrollLeft - stood)).toBeGreaterThan(100)

      await act(async () => {
        await mouse.moveTo(-50, y)
        await mouse.release()
        await frame()
        await frame()
      })
      // THE STATE THIS TEST NAMES: the line heard the pointer outside the
      // window.
      expect(Math.min(...heard)).toBeLessThan(0)
      const letGo = rail.scrollLeft

      await act(async () => {
        for (let move = 0; move <= 4; move++) {
          await mouse.moveTo(atAlong(host, width / 2 + 15 * move), y)
        }
        await frame()
        await frame()
      })
      expect(rail.scrollLeft).toBe(letGo)
      expect(line.dataset.held).toBeUndefined()
    })
  }
})
