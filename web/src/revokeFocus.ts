import { useLayoutEffect, useRef } from 'react'

/** A row's id in its list. */
type RowId = number | string

/** What a row's Revoke is kept under: its list and its row's id. */
const nameOf = (list: string, id: RowId) => `${list} ${String(id)}`

/** A row of a list whose Revoke takes it away. */
export interface RevokedRow {
  id: RowId
}

/** What a screen's Revokes are given by `useFocusAfterRevoke`. */
export interface FocusAfterRevoke<List extends string> {
  /** Called at a Revoke's press, with its list and its row's id. */
  pressed: (list: List, id: RowId) => void
  /** The `ref` of a row's Revoke, which keeps it while it is drawn. */
  revokeRef: (
    list: List,
    id: RowId,
  ) => (button: HTMLButtonElement) => () => void
}

/**
 * Where focus goes when the row of the Revoke pressed last goes.
 *
 * It moves only where focus is on the page's body as the row goes: where
 * it falls when the Revoke that had it goes with its row, and where it
 * already was after a press that never focused the button, as Safari's
 * mouse press is. Focus put anywhere else before the row goes is left
 * there. It moves to the nearest row left in the same list, the next one,
 * else the one before, passing over a row whose Revoke is `disabled`, such
 * as the current session's; with none, to `landing`'s place for that list.
 * A dialog whose rows and landing places are its own keeps focus so.
 *
 * The rows around the one pressed are read from its list as last drawn.
 * One that went in the same render has no Revoke drawn, and is passed
 * over; a row that goes while no Revoke and no landing place is drawn, as
 * in a closed panel, moves nothing.
 */
export function useFocusAfterRevoke<List extends string>(
  lists: Readonly<Record<List, readonly RevokedRow[]>>,
  landing: (list: List) => HTMLElement | null,
): FocusAfterRevoke<List> {
  const press = useRef<{ list: List; id: RowId } | null>(null)
  const drawn = useRef(lists)
  /** Each drawn row's Revoke, by `nameOf`. */
  const buttons = useRef(new Map<string, HTMLButtonElement>())

  // After every render, so that `drawn` holds each list as last drawn.
  useLayoutEffect(() => {
    const was = drawn.current
    drawn.current = lists
    const pressed = press.current
    if (pressed === null) return
    if (lists[pressed.list].some((row) => row.id === pressed.id)) return
    press.current = null
    if (document.activeElement !== document.body) return
    const before = was[pressed.list]
    const at = before.findIndex((row) => row.id === pressed.id)
    const nearest = [
      ...before.slice(at + 1),
      ...before.slice(0, Math.max(at, 0)).reverse(),
    ]
    // A row gone with the one pressed has no Revoke drawn to find.
    const to = nearest
      .map((row) => buttons.current.get(nameOf(pressed.list, row.id)))
      .find((button) => button !== undefined && !button.disabled)
    const target = to ?? landing(pressed.list)
    target?.focus()
  })

  return {
    pressed: (list, id) => {
      press.current = { list, id }
    },
    revokeRef: (list, id) => (button) => {
      const name = nameOf(list, id)
      buttons.current.set(name, button)
      return () => {
        buttons.current.delete(name)
      }
    },
  }
}
