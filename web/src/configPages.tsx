/**
 * What the Configuration screens drawn as pages over one list — Libraries
 * and Users — do alike once a write answers: where the answer is said, what
 * takes focus, and when what was said goes.
 *
 * **Each screen has one line a write's answer draws**: Libraries' report of
 * a removal or a move, Users' "Removed <name>.". What the line carries, the
 * page it belongs on and how it is drawn are the screen's own. Whether it is
 * drawn there or on the page the person is on, whether the person is sent
 * there, and whether it takes focus are decided here, by `place`.
 *
 * **An answer that comes once the person has gone to another page of the
 * screen is said on the page they are on**, and leaves focus and the page
 * where they are, since nothing they were doing there went with it. A line
 * said so lands in a polite live region drawn with the page, since nothing
 * takes focus to have it read. Whatever is said goes when the person leaves
 * the page it is drawn on. Once another tab has replaced the screen it is
 * said nowhere.
 */
import {
  type Dispatch,
  type ReactNode,
  type RefObject,
  type SetStateAction,
  startTransition,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react'
import { useLocation, useNavigate } from 'react-router'
import { reveal } from './reveal'

/** A screen's line: the page it is drawn on, and whether it takes focus. */
interface Said {
  at: string
  /** Taken when the answer itself moved the person to `at`. */
  focus?: boolean
  /**
   * Which answer the line says, given by `place`, so that each answer takes
   * focus once, two in the same words too. A line `follow` carries keeps it,
   * and does not take focus again at the new address.
   */
  n?: number
}

/** Whether the address `at` is the page `page` or a page beneath it. */
const under = (at: string, page: string): boolean =>
  at === page || at.startsWith(`${page}/`)

/** `at`, where it is `from` or beneath it, at the same place beneath `to`. */
const moved = (at: string, from: string, to: string): string =>
  under(at, from) ? `${to}${at.slice(from.length)}` : at

export interface LateAnswers<Line extends Said> {
  /**
   * Places the line for the answer to a write sent from the page `from`.
   * `next.at` is the page it belongs on. While the person is still on
   * `from`, with the screen still drawn, `sent` is true: the screen sends
   * them to `line.at`, that page, and the line takes focus there. Otherwise
   * the line is drawn, without focus, on the page they are on. It sets
   * nothing, so the screen sets the line, with whatever else the answer
   * changes, in one transition. A `from` and a `next.at` a rename has moved
   * (`follow`) are read at their new addresses.
   */
  place: (
    from: string,
    next: Omit<Line, 'focus'>,
  ) => { sent: boolean; line: Omit<Line, 'focus'> & { focus: boolean } }
  /** The screen's line, while the page it is drawn on is the one drawn. */
  shown: Line | undefined
  setLine: Dispatch<SetStateAction<Line | undefined>>
  /** A refusal whose page or form is gone, said on the page the person is on. */
  say: (sentence: string) => void
  /**
   * A rename moved the page at `from`, and every page beneath it, to `to`.
   * What is drawn there goes with it: the line, the late refusals and an
   * arrival. Libraries takes its line away first, since a rename is a write
   * answered on the library's pages (O255). An answer to a write sent from
   * beneath `from` before the rename is placed as though sent from the same
   * page beneath `to`. Called in the transition that moves the person to the
   * new address, so that nothing is drawn for a frame at an address the
   * person is not at.
   */
  follow: (from: string, to: string) => void
  /**
   * The page drawn now, for a screen's own answers to read after a page
   * change. Not for drawing: it is not state.
   */
  here: RefObject<string>
  /** Whether the page drawn is the one a make or Cancel sent the person to. */
  arrived: boolean
  setArrival: Dispatch<SetStateAction<string | undefined>>
  /** The make form's Cancel: back to `table`, whose heading takes focus. */
  cancel: () => void
  /**
   * The notices every page draws at its head: `head` (the list's failure),
   * then the late refusals, then `drawn`, the screen's drawing of `shown`.
   */
  drawNotices: (head: ReactNode, drawn: ReactNode) => ReactNode
}

/**
 * `table` is the screen's table, where Cancel goes. `lineEl` is the element
 * the screen draws its line in, which takes focus when the line is one the
 * answer moved the person to.
 */
export function useLateAnswers<Line extends Said>(
  table: string,
  lineEl: RefObject<HTMLElement | null>,
): LateAnswers<Line> {
  const [line, setLine] = useState<Line | undefined>()
  /**
   * The refusals whose page or form went before they came, each with the
   * page it is drawn on, and a number to tell two in the same words apart.
   */
  const [late, setLate] = useState<
    { at: string; sentence: string; n: number }[]
  >([])
  const lateCount = useRef(0)
  /**
   * The page a make or the make form's Cancel sent the person to. Its
   * heading takes focus once, since the button that had it went with the
   * form.
   */
  const [arrival, setArrival] = useState<string | undefined>()
  const { pathname } = useLocation()
  const navigate = useNavigate()

  /**
   * The page drawn now. `place` and `say` are made once, so an answer that
   * arrives after a page change reads the page through this, not `pathname`.
   */
  const here = useRef(pathname)
  here.current = pathname
  /**
   * The renames still to be read into an answer's `from`, oldest first. One
   * goes once the person is at its old address again: a page there now is
   * not the page that was renamed, and a write sent from it is its own.
   */
  const follows = useRef<{ from: string; to: string }[]>([])
  /** Which answer each line says (`Said.n`). */
  const lineCount = useRef(0)
  /** False once another Configuration tab has replaced the screen. */
  const mounted = useRef(false)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  /**
   * `at` read through every rename still held. The page drawn is read so too:
   * between a rename and the commit that moves the person, it is still the
   * old address.
   */
  const followed = useCallback(
    (at: string) =>
      follows.current.reduce((read, { from, to }) => moved(read, from, to), at),
    [],
  )

  const place = useCallback(
    (from: string, next: Omit<Line, 'focus'>) => {
      const now = followed(here.current)
      const sent = mounted.current && followed(from) === now
      const n = ++lineCount.current
      return {
        sent,
        line: sent
          ? { ...next, at: followed(next.at), focus: true, n }
          : { ...next, at: now, focus: false, n },
      }
    },
    [followed],
  )

  const say = useCallback(
    (sentence: string) => {
      const n = ++lateCount.current
      setLate((held) => [...held, { at: followed(here.current), sentence, n }])
    },
    [followed],
  )

  const follow = useCallback((from: string, to: string) => {
    follows.current = [...follows.current, { from, to }]
    setLine((held) =>
      held === undefined || !under(held.at, from)
        ? held
        : { ...held, at: moved(held.at, from, to) },
    )
    setLate((held) =>
      held.map((one) => ({ ...one, at: moved(one.at, from, to) })),
    )
    setArrival((held) => (held === undefined ? held : moved(held, from, to)))
  }, [])

  // What is said belongs to the page it was drawn on, and so does an
  // arrival: a page opened again later is opened by the person.
  useEffect(() => {
    setLine((held) =>
      held !== undefined && held.at !== pathname ? undefined : held,
    )
    setLate((held) =>
      held.every((one) => one.at === pathname)
        ? held
        : held.filter((one) => one.at === pathname),
    )
    // Kept on the page it names: a page first drawn after the move, once a
    // read brings its item into the list, still takes focus for it.
    setArrival((held) => (held === pathname ? held : undefined))
    follows.current = follows.current.filter(
      ({ from }) => !under(pathname, from),
    )
  }, [pathname])

  const cancel = useCallback(() => {
    startTransition(() => {
      setArrival(table)
      navigate(table)
    })
  }, [table, navigate])

  const shown = line?.at === pathname ? line : undefined
  const lateShown = late.filter((one) => one.at === pathname)
  // A line the answer moved the person to replaces the page they were acting
  // on, and focus was on a button that no longer exists. Focusing the line
  // has it read. Focus alone scrolls only as far as the nearest edge, which
  // for a line taller than the window is its foot, so its head is put at the
  // top of the view, clear of the phone bar, as well, when it is not already
  // in view. Taken once per answer (`Said.n`): the list reading again leaves
  // focus where it is. A line drawn on a page the person went to themselves
  // is neither focused nor scrolled to: they are doing something else there.
  const taking = shown?.focus === true ? shown.n : undefined
  useEffect(() => {
    const el = lineEl.current
    if (taking === undefined || el === null) return
    el.focus({ preventScroll: true })
    reveal(el)
  }, [taking, lineEl])

  const drawNotices = (head: ReactNode, drawn: ReactNode): ReactNode => (
    <>
      {head}
      {lateShown.map((one) => (
        <p key={one.n} role="alert">
          {one.sentence}
        </p>
      ))}
      {shown?.focus === true && drawn}
      {/* A line that takes focus is read as it does, and is kept out of
          this region so it is not read twice. One that does not is read only
          by landing in a live region, and a region drawn together with what
          it says is often not read at all, so this one is drawn with the
          page, empty, before any answer can come. */}
      <div aria-live="polite">{shown?.focus !== true && drawn}</div>
    </>
  )

  return {
    place,
    shown,
    setLine,
    say,
    follow,
    here,
    arrived: arrival === pathname,
    setArrival,
    cancel,
    drawNotices,
  }
}

/**
 * A key for the page of one thing in a screen's list, a library or an
 * account, that changes only when the page goes from one thing to another,
 * so that one thing's page never inherits another's state. While the list
 * is read `id` is `undefined`, and the page drawn then keeps its key once
 * the list names the thing: its header is the drawn page's own, with
 * whatever has focus in it.
 */
export function usePageKey<Id>(id: Id | undefined): number {
  const [page, setPage] = useState({ id, key: 0 })
  if (id !== undefined && id !== page.id) {
    const key = page.id === undefined ? page.key : page.key + 1
    setPage({ id, key })
    return key
  }
  return page.key
}
