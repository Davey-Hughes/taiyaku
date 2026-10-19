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
}

export interface LateAnswers<Line extends Said> {
  /**
   * Places the line for the answer to a write sent from the page `from`.
   * `next.at` is the page it belongs on. While the person is still on
   * `from`, with the screen still drawn, `sent` is true: the screen sends
   * them to `next.at`, and the line takes focus there. Otherwise the line is
   * drawn, without focus, on the page they are on. It sets nothing, so the
   * screen sets the line, with whatever else the answer changes, in one
   * transition.
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
  /** False once another Configuration tab has replaced the screen. */
  const mounted = useRef(false)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
    }
  }, [])

  const place = useCallback((from: string, next: Omit<Line, 'focus'>) => {
    const sent = mounted.current && here.current === from
    return {
      sent,
      line: sent
        ? { ...next, focus: true }
        : { ...next, at: here.current, focus: false },
    }
  }, [])

  const say = useCallback((sentence: string) => {
    const n = ++lateCount.current
    setLate((held) => [...held, { at: here.current, sentence, n }])
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
  // in view. Taken once per line: the list reading again leaves focus where
  // it is. A line drawn on a page the person went to themselves is neither
  // focused nor scrolled to: they are doing something else there.
  useEffect(() => {
    const el = lineEl.current
    if (shown?.focus !== true || el === null) return
    el.focus({ preventScroll: true })
    reveal(el)
  }, [shown, lineEl])

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
    arrived: arrival === pathname,
    setArrival,
    cancel,
    drawNotices,
  }
}
