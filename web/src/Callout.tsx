import {
  type ReactNode,
  type RefObject,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'

/**
 * What answers a form, laid right under the row of the control it answers,
 * over the rows of its section that follow, or, for a refusal with no room
 * there, over the rows above that row (the user's rulings of 2026-10-26;
 * `settings.css`): why a press was refused, or why what is typed would be.
 * No room is kept for it, so neither its coming nor its going moves
 * anything. A refusal in it is a `Refusal`, an alert drawn when it comes;
 * what is said as it is typed is a polite status drawn empty for as long as
 * its box is, as `NameClash` is. Either takes a press, so that one on it
 * reaches nothing beneath (`settings.css`).
 *
 * It is drawn straight after the row it answers, and `placeCallout` lays it
 * over the rows below or above, or, given a control to lie `over`, over that
 * control alone. A control it lies over keeps its focus beneath it, and the
 * ring that marks that focus is drawn on the callout (`data-over-focus`),
 * which is what is seen there.
 */
export function Callout({
  children,
  over,
}: {
  children: ReactNode
  over?: RefObject<HTMLElement | null>
}) {
  const ref = useRef<HTMLDivElement>(null)
  const [overFocus, setOverFocus] = useState(false)
  // After every render: its words, and the rows it lies over, are drawn by
  // its form's screen, so any render of it may change where it lies.
  useLayoutEffect(() => {
    const callout = ref.current
    if (callout === null || !laidOut()) return
    placeCallout(callout, over)
    setOverFocus(coversFocus(callout))
  })
  // A change of its section's size, such as a narrower window or a list
  // that lands, moves the rows it lies over, and focus moves with no
  // render of it.
  useLayoutEffect(() => {
    const callout = ref.current
    const section = callout === null ? null : sectionOf(callout)
    if (callout === null || section === null || !laidOut()) return
    const resized = new ResizeObserver(() => {
      placeCallout(callout, over)
      setOverFocus(coversFocus(callout))
    })
    resized.observe(section)
    const focusMoved = () => {
      setOverFocus(coversFocus(callout))
    }
    document.addEventListener('focusin', focusMoved)
    document.addEventListener('focusout', focusMoved)
    return () => {
      resized.disconnect()
      document.removeEventListener('focusin', focusMoved)
      document.removeEventListener('focusout', focusMoved)
    }
  }, [over])
  return (
    <div
      className="callout"
      ref={ref}
      data-over-focus={overFocus ? '' : undefined}
    >
      {children}
    </div>
  )
}

/** A document with no ResizeObserver lays nothing out to read (jsdom). */
const laidOut = () => typeof ResizeObserver !== 'undefined'

/** What a callout lies inside: a settings box or a reader panel's section. */
const sectionOf = (callout: Element) =>
  callout.closest<HTMLElement>('.settings-box, section')

const px = (length: string) => Number.parseFloat(length) || 0

/**
 * Lays `callout` over the rows of its section from the foot of the row it
 * is drawn after, down as far as its words need, and on over the content
 * of each row it reaches into, whole, so that no control and no line of
 * words is left half covered. Where that would take it below the content
 * of the section's last row, it ends there instead, and reaches up over the
 * rows above as far as its words need, each of them whole: a box's content
 * is a padding's width inside its edge, so a callout whose words fit in it
 * never meets the edge, nor the next section's heading beyond it. A
 * refusal lies over the rows above the row it answers instead, where they
 * have room for it (`overRowsAbove`), and is pulled up over that row only
 * where they have none. Given a control to lie `over`, it lies over that
 * alone (`placeOver`). Set as `top` and `height`,
 * and `left` and `width` over a control, since it lies out of the flow and
 * moves nothing; with nothing to say it takes none of them, and no room.
 */
function placeCallout(
  callout: HTMLElement,
  over: RefObject<HTMLElement | null> | undefined,
) {
  for (const side of ['top', 'height', 'left', 'width']) {
    callout.style.removeProperty(side)
  }
  // Before anything is read of the layout: an empty callout is drawn with
  // every keystroke in its form.
  const said = callout.firstElementChild
  if (!said?.hasChildNodes()) return
  const section = sectionOf(callout)
  const answered = rowBefore(callout)
  const frame = callout.parentElement
  if (section === null || answered === null || frame === null) return
  // A control drawn after the callout has no ref yet when the callout is
  // first laid out; its section's ResizeObserver, whose first answer comes
  // once the control is drawn, lays the callout out then.
  if (over !== undefined) {
    if (over.current !== null) placeOver(callout, said, over.current, frame)
    return
  }
  const rows = rowsOf(section).map(contentOf)
  const last = rows.at(-1)
  if (last === undefined) return
  const needs = said.getBoundingClientRect().height
  const answeredAt = answered.getBoundingClientRect()
  const foot = answeredAt.bottom
  let top = foot
  let bottom = foot + needs
  for (const row of rows) {
    if (row.top < foot - 0.5) continue
    if (row.top >= bottom - 0.5) break
    bottom = Math.max(bottom, row.bottom)
  }
  if (bottom > last.bottom + 0.5) {
    const above = said.matches('[role="alert"]')
      ? overRowsAbove(rows, answeredAt.top, needs)
      : null
    if (above !== null) {
      ;({ top, bottom } = above)
    } else {
      bottom = last.bottom
      top = bottom - needs
      for (const row of [...rows].reverse()) {
        if (row.top >= foot - 0.5) continue
        if (row.bottom <= top + 0.5) break
        top = Math.min(top, row.top)
      }
    }
  }
  const origin = frame.getBoundingClientRect().top + frame.clientTop
  callout.style.top = `${top - origin}px`
  callout.style.height = `${bottom - top}px`
}

/**
 * Where a refusal lies that cannot lie under the row it answers inside its
 * section: over the rows above that row, its foot at that row's top, up as
 * far as its words need, each row whole. Not over that row, whose box a
 * person edits to take the refusal away, and which on the New key row
 * holds Create too. `null` where the rows above have no room for its words
 * inside the section's content; it is then pulled up as a status is.
 */
function overRowsAbove(
  rows: { top: number; bottom: number }[],
  answeredTop: number,
  needs: number,
) {
  const bottom = answeredTop
  let top = bottom - needs
  for (const row of [...rows].reverse()) {
    if (row.top >= bottom - 0.5) continue
    if (row.bottom <= top + 0.5) break
    top = Math.min(top, row.top)
  }
  const contentTop = Math.min(...rows.map((row) => row.top))
  return top >= contentTop - 0.5 ? { top, bottom } : null
}

/**
 * Lays `callout` over `control` alone, whole, so that the rest of its row,
 * such as its label and the help under it, stays in view: centred on it, as
 * tall as its words need and no less tall than it, and as wide as it, or
 * wider toward its row's start where its words need that to stay on one
 * line, into the room beside it that holds nothing, but never over the
 * column of its row's label and help.
 */
function placeOver(
  callout: HTMLElement,
  said: Element,
  control: HTMLElement,
  frame: HTMLElement,
) {
  const at = control.getBoundingClientRect()
  const text = control
    .closest('.settings-row')
    ?.querySelector('.settings-row-text')
    ?.getBoundingClientRect()
  const start =
    text !== undefined && text.right <= at.left + 0.5 ? text.right : at.left
  callout.style.width = 'max-content'
  const words = said.getBoundingClientRect().width
  const width = Math.min(Math.max(words, at.width), at.right - start)
  const origin = frame.getBoundingClientRect()
  callout.style.left = `${at.right - width - origin.left - frame.clientLeft}px`
  callout.style.width = `${width}px`
  const height = Math.max(said.getBoundingClientRect().height, at.height)
  const top = (at.top + at.bottom - height) / 2
  callout.style.top = `${top - origin.top - frame.clientTop}px`
  callout.style.height = `${height}px`
}

/**
 * The row drawn before `callout` in its form or section: the row of the
 * control it answers. Parts that take no room, such as an empty live
 * region, a visually hidden one or another callout, are passed over.
 */
function rowBefore(callout: Element) {
  for (
    let before = callout.previousElementSibling;
    before !== null;
    before = before.previousElementSibling
  ) {
    if (takesRoom(before)) return before
  }
  return null
}

const takesRoom = (part: Element) =>
  !part.matches('.callout') &&
  getComputedStyle(part).position !== 'absolute' &&
  part.getBoundingClientRect().height > 0

/**
 * The rows of `container`, top to bottom: each part of it that takes room,
 * and for a list, a form or a group laid out as a block, each of its rows
 * in turn, as a box draws them.
 */
function rowsOf(container: Element): Element[] {
  return [...container.children].flatMap((part) => {
    if (!takesRoom(part)) return []
    const isGroup =
      part.matches('form, ul, ol, fieldset, .settings-group') &&
      getComputedStyle(part).display === 'block'
    return isGroup ? rowsOf(part) : [part]
  })
}

/**
 * Where `row`'s content is drawn: inside the line over it and the block
 * padding a box gives each of its rows, which are the box's room, not the
 * row's; to its edges for a control, or a part that draws an edge of its
 * own all round, such as the secret's box, whose edge is part of it.
 */
function contentOf(row: Element) {
  const { top, bottom } = row.getBoundingClientRect()
  const style = getComputedStyle(row)
  if (
    row.matches('button, input, select, textarea, a') ||
    px(style.borderBottomWidth) > 0
  )
    return { top, bottom }
  return {
    top: top + px(style.borderTopWidth) + px(style.paddingTop),
    bottom: bottom - px(style.paddingBottom),
  }
}

/**
 * Whether the control with focus is drawn under `callout` while it says
 * something, and is marked as focused there (`:focus-visible`), so that its
 * ring is drawn on the callout (`settings.css`).
 */
function coversFocus(callout: HTMLElement) {
  const focused = document.activeElement
  if (
    !callout.firstElementChild?.hasChildNodes() ||
    focused === null ||
    !focused.matches(':focus-visible')
  )
    return false
  const over = callout.getBoundingClientRect()
  const at = focused.getBoundingClientRect()
  const x = (at.left + at.right) / 2
  const y = (at.top + at.bottom) / 2
  return x > over.left && x < over.right && y > over.top && y < over.bottom
}
