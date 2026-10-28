import { act } from 'react'
import { expect } from 'vitest'
import { userEvent } from 'vitest/browser'

/**
 * What a `<summary>` shows in one state: the cursor over it, the marker
 * it draws, and how far its words start from its content box's edge,
 * which is the room an inside marker takes.
 */
function shown(summary: HTMLElement) {
  const style = getComputedStyle(summary)
  const words = document.createRange()
  words.selectNodeContents(summary)
  const start =
    summary.getBoundingClientRect().left +
    Number.parseFloat(style.borderLeftWidth) +
    Number.parseFloat(style.paddingLeft)
  return {
    cursor: style.cursor,
    marker: style.listStyleType,
    inset: words.getClientRects()[0].left - start,
    fontSize: Number.parseFloat(style.fontSize),
  }
}

/**
 * A `<summary>` that reads as something to press: the pointer a button
 * takes over it, and the browser's disclosure marker before its words,
 * pointing along the line while closed and turned down once a real mouse
 * has opened it. `named` says which summary a failure is about.
 */
export async function expectDisclosure(summary: HTMLElement, named: string) {
  const details = summary.parentElement as HTMLDetailsElement
  expect(details.open, `${named} is closed at first`).toBe(false)
  const closed = shown(summary)
  await act(async () => {
    await userEvent.click(summary)
  })
  expect(details.open, `${named} opens when pressed`).toBe(true)
  const open = shown(summary)
  for (const [state, read, marker] of [
    ['closed', closed, 'disclosure-closed'],
    ['open', open, 'disclosure-open'],
  ] as const) {
    expect.soft(read.cursor, `${named}, ${state}: the cursor`).toBe('pointer')
    expect.soft(read.marker, `${named}, ${state}: the marker`).toBe(marker)
    expect
      .soft(
        read.inset,
        `${named}, ${state}: the marker's room before its words`,
      )
      .toBeGreaterThanOrEqual(read.fontSize / 2)
  }
}
