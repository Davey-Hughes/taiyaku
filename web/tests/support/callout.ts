import { act } from 'react'
import { expect } from 'vitest'
import { tokenColour } from './colour'
import { lengthOf } from './length'
import { mouse } from './mouse'

/**
 * Where every element under `root` is drawn in `root`'s content, outside
 * `except`: what a callout lays over must stand where it stood before the
 * callout came. In the content, so that `root` scrolling itself to bring a
 * refusal into view, as the reader's panel does, is not taken for a move.
 */
export function layoutOf(root: Element, except: Element | null = null) {
  return [...root.querySelectorAll('*')]
    .filter((el) => except === null || !except.contains(el))
    .map((el) => {
      const { top, left, width, height } = el.getBoundingClientRect()
      const at = [top + root.scrollTop, left + root.scrollLeft, width, height]
      return `${el.tagName}.${el.className}: ${at.join(',')}`
    })
}

/** The controls a callout covers whole or not at all. */
const CONTROLS = 'button, input, select, textarea, a[href]'

const px = (length: string) => Number.parseFloat(length) || 0

/**
 * `said` is drawn as a callout is (`settings.css`), as the rename's is: a
 * box with an edge, on the raised surface, with a shadow; out of the flow,
 * drawn straight after `under`, the row of the control it answers, and
 * laid over the rows below that row's foot, as wide as it; or, `above`, as
 * a refusal is where it would cross its section's foot, over the rows
 * above, as wide as it, its own foot at that row's top; or, `pulledUp`,
 * as one is where the rows above have no room either, its foot at the foot
 * of its section's content and its top over the row it answers; or over
 * the control given as `over`, whole, to that control's end. Inside its
 * section (`expectInsideItsSection`), and drawn above whatever it lies
 * over, as it is painted, taking a press there.
 */
export async function expectCallout(
  said: Element,
  under: Element,
  {
    above = false,
    pulledUp = false,
    over,
  }: { above?: boolean; pulledUp?: boolean; over?: Element } = {},
) {
  // Two frames: a callout is laid out again by a ResizeObserver, whose
  // answer comes in the frame after the change it answers.
  for (let frame = 0; frame < 2; frame++) {
    await new Promise((resolve) => requestAnimationFrame(resolve))
  }
  const callout = said.parentElement as HTMLElement
  expect(callout.className).toBe('callout')
  expect(getComputedStyle(callout).position).toBe('absolute')
  const style = getComputedStyle(said)
  expect([style.borderTopWidth, style.borderTopStyle]).toEqual(['1px', 'solid'])
  expect(style.boxShadow).not.toBe('none')
  expect(style.backgroundColor).toBe(tokenColour(callout, '--surface'))

  let drawnAfter = callout.previousElementSibling
  while (drawnAfter?.matches('.callout')) {
    drawnAfter = drawnAfter.previousElementSibling
  }
  expect(drawnAfter, 'drawn straight after the row it answers').toBe(under)
  const box = said.getBoundingClientRect()
  const row = under.getBoundingClientRect()
  if (over !== undefined) {
    const control = over.getBoundingClientRect()
    expect(
      [
        control.left - box.left,
        box.right - control.right,
        control.top - box.top,
        box.bottom - control.bottom,
      ].every((by) => by > -0.5),
      'over the whole of its control',
    ).toBe(true)
    expect(
      Math.abs(box.right - control.right),
      'to its control’s end',
    ).toBeLessThan(0.5)
  } else {
    expect(Math.abs(box.left - row.left), 'from the row’s start').toBeLessThan(
      0.5,
    )
    expect(Math.abs(box.right - row.right), 'to its end').toBeLessThan(0.5)
    if (pulledUp) {
      expect(box.top, 'pulled up over the row it answers').toBeLessThan(
        row.bottom - 0.5,
      )
      expect(
        Math.abs(box.bottom - contentOf(said).bottom),
        'its foot at the foot of its section’s content',
      ).toBeLessThan(0.5)
    } else {
      expect(
        Math.abs(above ? box.bottom - row.top : box.top - row.bottom),
        above
          ? 'its foot at the top of the row it answers'
          : 'its top at the foot of the row it answers',
      ).toBeLessThan(0.5)
    }
  }
  expectInsideItsSection(said)
  // Hit testing is how a press finds its target, so this reads both that
  // the callout is drawn above what it lies over and that it takes a press
  // there (`settings.css`).
  const covered: string[] = []
  for (const x of [box.left + 2, (box.left + box.right) / 2, box.right - 2]) {
    for (const y of [box.top + 2, (box.top + box.bottom) / 2, box.bottom - 2]) {
      const hit = document.elementFromPoint(x, y)
      if (!said.contains(hit)) {
        covered.push(`${hit?.tagName}.${hit?.className}`)
      }
    }
  }
  expect(covered, 'drawn above what it lies over, taking a press').toEqual([])
}

/**
 * Where the content of `said`'s section is drawn, which a callout lies
 * inside: in a settings box, a padding's width inside its edge, which is
 * its own padding at its sides and, at its top and foot, the padding each
 * of its rows carries (`--space-3`); in a reader panel's section, which
 * draws only the line over it, under that line by its padding, and as wide
 * and as low as the section, whose sides are the panel's padding inside
 * its edge, and whose foot the next section's margin keeps off the line
 * over that one.
 */
function contentOf(said: Element) {
  const section = said.closest('.settings-box, section')
  if (section === null) throw new Error('the callout is in no section')
  const edge = section.getBoundingClientRect()
  const style = getComputedStyle(section)
  if (section.matches('.settings-box')) {
    const side = px(style.borderLeftWidth) + px(style.paddingLeft)
    const end = px(style.borderTopWidth) + lengthOf(section, '--space-3')
    return {
      section,
      top: edge.top + end,
      bottom: edge.bottom - end,
      left: edge.left + side,
      right: edge.right - side,
    }
  }
  return {
    section,
    top: edge.top + px(style.borderTopWidth) + px(style.paddingTop),
    bottom: edge.bottom,
    left: edge.left,
    right: edge.right,
  }
}

/**
 * `said`'s box lies inside its section's content (`contentOf`), never on
 * or across an edge its section draws, so no other section's heading is
 * under it; and each control of the section that it lies over, it covers
 * whole.
 */
export function expectInsideItsSection(said: Element) {
  const { section, ...content } = contentOf(said)
  const box = said.getBoundingClientRect()
  const past = Object.entries({
    top: box.top - content.top,
    bottom: content.bottom - box.bottom,
    left: box.left - content.left,
    right: content.right - box.right,
  }).filter(([, by]) => by < -0.5)
  expect(
    past,
    'edges of its section’s content it crosses, and by how much',
  ).toEqual([])
  const cut = [...section.querySelectorAll(CONTROLS)]
    .filter((control) => !said.contains(control))
    .filter((control) => {
      const drawn = control.getBoundingClientRect()
      const across =
        Math.min(box.right, drawn.right) - Math.max(box.left, drawn.left)
      const down =
        Math.min(box.bottom, drawn.bottom) - Math.max(box.top, drawn.top)
      const whole =
        drawn.left >= box.left - 0.5 &&
        drawn.right <= box.right + 0.5 &&
        drawn.top >= box.top - 0.5 &&
        drawn.bottom <= box.bottom + 0.5
      return across > 0.5 && down > 0.5 && !whole
    })
    .map((control) => `${control.tagName} ${control.textContent ?? ''}`)
  expect(cut, 'controls it covers in part').toEqual([])
}

/**
 * The line `said`, saying the two passwords of its form differ, is laid
 * under `confirmRow`, Confirm password's row, or over the control given as
 * `over` (`expectCallout`), and over no box of its form, not even Confirm
 * password's own (the user's ruling of 2026-10-26): so a real press at the
 * middle of `confirm`, which must not have focus, gives it focus. Soft, so
 * that each is read where the line lies wrong.
 */
export async function expectLaidUnderConfirm(
  said: Element,
  confirm: HTMLElement,
  confirmRow: Element,
  { over }: { over?: Element } = {},
) {
  const boxes = [...(said.closest('form')?.querySelectorAll('input') ?? [])]
  expect(boxes).toContain(confirm)
  expect(document.activeElement, 'focus before the press').not.toBe(confirm)
  expect
    .soft(
      boxes
        .filter((box) => liesOver(said, box))
        .map((box) => box.getAttribute('aria-label')),
      'boxes the line lies over',
    )
    .toEqual([])
  const at = confirm.getBoundingClientRect()
  await pressAt(at.left + at.width / 2, at.top + at.height / 2)
  expect
    .soft(document.activeElement, 'focused by a press on Confirm password')
    .toBe(confirm)
  await expectCallout(said, confirmRow, { over })
}

/**
 * The room the row holding `button`, the row under Confirm password, keeps
 * beyond what `button` and its margins take, above it or below it.
 */
export function roomKept(button: Element) {
  const row = button.parentElement
  if (!row) throw new Error('the button is in no row')
  const style = getComputedStyle(row)
  const content =
    row.getBoundingClientRect().height -
    [
      style.paddingTop,
      style.paddingBottom,
      style.borderTopWidth,
      style.borderBottomWidth,
    ].reduce((sum, length) => sum + px(length), 0)
  const margins = getComputedStyle(button)
  return (
    content -
    button.getBoundingClientRect().height -
    px(margins.marginTop) -
    px(margins.marginBottom)
  )
}

/**
 * Where a part of a form is drawn, `[left, top, width, height]`, read from
 * the form's top left corner.
 */
export type DrawnInForm = readonly [number, number, number, number]

/**
 * The row under Confirm password, which holds `button`, `button` itself,
 * and the boxes of their form, all above it, in order, are each drawn at
 * the rect `drawn` gives it, read from the form's top left corner, so that
 * where the form stands on the page does not count. Where the line saying
 * the two passwords differ takes one line, the row keeps no room for it,
 * and `drawn` is where a row keeping none draws them: `roomKept` reads the
 * row's height alone, not where its button stands. Read once the fonts
 * have come, since a button is as wide as its words: a layout first, so
 * that the faces its words are drawn in are asked for before `ready` is
 * read. Soft, so that each part drawn elsewhere is read.
 */
export async function expectRowDrawnAt(
  button: Element,
  drawn: {
    row: DrawnInForm
    button: DrawnInForm
    boxes: readonly DrawnInForm[]
  },
) {
  const row = button.parentElement
  const form = button.closest('form')
  if (!row || !form) throw new Error('the button is in no row of a form')
  form.getBoundingClientRect()
  await document.fonts.ready
  const corner = form.getBoundingClientRect()
  const inForm = (part: Element) => {
    const { left, top, width, height } = part.getBoundingClientRect()
    return [left - corner.left, top - corner.top, width, height].map(
      (at) => Math.round(at * 100) / 100,
    )
  }
  const boxes = [...form.querySelectorAll('input')]
  expect(boxes, 'the form’s boxes').toHaveLength(drawn.boxes.length)
  expect.soft(inForm(row), 'where the row is drawn').toEqual(drawn.row)
  expect.soft(inForm(button), 'where its button is drawn').toEqual(drawn.button)
  boxes.forEach((box, at) => {
    expect
      .soft(
        inForm(box),
        `where ${box.getAttribute('aria-label') ?? box.name} is drawn`,
      )
      .toEqual(drawn.boxes[at])
  })
}

/**
 * The row holding `button` keeps its room with the words of `said`, the
 * line laid over it (`DifferRoom`): drawn unseen, and hidden from a screen
 * reader, which reads them in the line alone.
 */
export function expectRoomUnseen(button: Element, said: Element) {
  const room = button.parentElement?.querySelector(':scope > .differ-room')
  expect(
    [
      room?.textContent,
      room?.getAttribute('aria-hidden'),
      room?.checkVisibility({ visibilityProperty: true }),
    ],
    'the words the row keeps its room with',
  ).toEqual([said.textContent, 'true', false])
}

/** How many lines `said`'s words are drawn on. */
export function linesOf(said: Element) {
  const words = document.createRange()
  words.selectNodeContents(said)
  return words.getClientRects().length
}

/** Whether `said`'s box lies over any part of `other`'s. */
export function liesOver(said: Element, other: Element) {
  const a = said.getBoundingClientRect()
  const b = other.getBoundingClientRect()
  return (
    Math.min(a.right, b.right) - Math.max(a.left, b.left) > 0.5 &&
    Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 0.5
  )
}

/**
 * How many times `region`'s words changed while `run` ran and two frames
 * after, outside `act`, so that React draws each change as it would for a
 * person: a status region emptied and filled again is read out again.
 */
export async function changesOf(region: Element, run: () => Promise<unknown>) {
  const global = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
  const records: MutationRecord[] = []
  const observer = new MutationObserver((seen) => {
    records.push(...seen)
  })
  observer.observe(region, {
    childList: true,
    subtree: true,
    characterData: true,
  })
  global.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await run()
    for (let frame = 0; frame < 2; frame++) {
      await new Promise((resolve) => requestAnimationFrame(resolve))
    }
  } finally {
    global.IS_REACT_ACT_ENVIRONMENT = true
    observer.disconnect()
  }
  return records.length
}

/**
 * The middle of where `over` lies over `under`, which it must lie over by
 * more than 4px each way: where a press or a tap that `over` lets through
 * would land on `under`.
 */
export function middleWhereOver(over: Element, under: Element) {
  const a = over.getBoundingClientRect()
  const b = under.getBoundingClientRect()
  const left = Math.max(a.left, b.left)
  const right = Math.min(a.right, b.right)
  const top = Math.max(a.top, b.top)
  const bottom = Math.min(a.bottom, b.bottom)
  expect(
    [right - left, bottom - top].every((side) => side > 4),
    'lies over it',
  ).toBe(true)
  return { x: (left + right) / 2, y: (top + bottom) / 2 }
}

/**
 * A real mouse pressed and then released, a frame apart, at the middle of
 * where `over` lies over `under` (`middleWhereOver`). Outside `act`, so that
 * what the press changes, such as a refusal going as focus leaves its form,
 * is drawn between the press and the release, as it would be for a person.
 */
export async function pressWhereOver(over: Element, under: Element) {
  const { x, y } = middleWhereOver(over, under)
  await pressAt(x, y)
}

/**
 * A real mouse pressed and released at the middle of `el`, as `pressAt`
 * presses, and what it started given `ms` to settle.
 */
export async function pressOn(el: Element | null, ms = 120) {
  if (el === null) throw new Error('nothing to press')
  const at = el.getBoundingClientRect()
  await pressAt((at.left + at.right) / 2, (at.top + at.bottom) / 2)
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms))
  })
}

/**
 * A real mouse pressed and then released, a frame apart, at a point in the
 * frame, outside `act`, as `pressWhereOver` presses: whatever is drawn
 * topmost there takes it, as it would a person's.
 */
export async function pressAt(x: number, y: number) {
  const global = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
  global.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await mouse.moveTo(x, y)
    await mouse.press()
    await new Promise((resolve) => requestAnimationFrame(resolve))
    await mouse.release()
    await new Promise((resolve) => requestAnimationFrame(resolve))
  } finally {
    global.IS_REACT_ACT_ENVIRONMENT = true
  }
}

/**
 * A real mouse pressed on the middle of `said`, and on the middle of where
 * it lies over `control` (`pressWhereOver`), reaches nothing beneath it:
 * the callout takes each press, so `control` is sent no part of either,
 * which for a held button is no press of it and for a select opens no
 * list, and neither it nor any other control under the callout takes
 * focus. What `said` says stays.
 */
export async function expectPressesTaken(said: Element, control: Element) {
  const words = said.textContent
  const section = said.closest('.settings-box, section')
  const under = [...(section?.querySelectorAll(CONTROLS) ?? [])].filter(
    (other) => liesOver(said, other),
  )
  expect(under, 'controls the callout lies over').toContain(control)
  const reached: string[] = []
  const note = (event: Event) => {
    reached.push(event.type)
  }
  const kinds = ['pointerdown', 'mousedown', 'mouseup', 'click'] as const
  for (const kind of kinds) control.addEventListener(kind, note)
  const focused: (string | undefined)[] = []
  const focusUnder = () =>
    under.find((other) => other.contains(document.activeElement))?.tagName
  try {
    const box = said.getBoundingClientRect()
    await pressAt((box.left + box.right) / 2, (box.top + box.bottom) / 2)
    focused.push(focusUnder())
    await pressWhereOver(said, control)
    focused.push(focusUnder())
  } finally {
    for (const kind of kinds) control.removeEventListener(kind, note)
  }
  expect(reached, 'what reached the control under the callout').toEqual([])
  expect(focused, 'focus under the callout after each press').toEqual([
    undefined,
    undefined,
  ])
  expect(said.textContent).toBe(words)
}
