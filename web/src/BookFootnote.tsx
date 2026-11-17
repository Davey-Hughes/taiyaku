/**
 * A footnote opened from its reference: the note's block, drawn with its
 * marks, and a way to go to it in the text.
 */
import {
  type CSSProperties,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import { useBackdropClose } from './backdropClose'
import type { Block } from './book'
import { pieces } from './bookNotes'
import { Runs } from './BookRuns'

/**
 * The tallest the note's card is drawn, a share of the window's height. Set
 * on the card here in pixels, not in `book.css` in `vh`: on a phone `vh` is
 * the height with the browser's toolbar hidden, taller than the window while
 * it shows, so a card placed by the room the window leaves could be drawn
 * taller than that room and run off the screen.
 */
const CARD_HEIGHT = 0.45

/**
 * The note `block` of `edition` of `work`, centred on the reference it was
 * opened from (`near`, that reference's box) and set beside it, as placed
 * below. A modal `<dialog>` over the whole screen, outside the text, so a tap
 * beside the note lands on the dialog itself and closes it rather than
 * reaching the text's taps; Escape closes it too, the reader's key handler or
 * the browser's close request.
 */
export function BookFootnote({
  block,
  work,
  edition,
  furigana,
  lang,
  page,
  near,
  onGo,
  onClose,
}: {
  block: Block
  work: string
  edition: string
  furigana: boolean
  lang?: string
  page?: string
  near?: { left: number; right: number; top: number; bottom: number }
  onGo: () => void
  onClose: () => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  useLayoutEffect(() => {
    if (dialog.current?.open === false) dialog.current.showModal()
  }, [])
  const backdrop = useBackdropClose(onClose)
  // A resize lays the text out again and the reader scrolls back to its
  // place, so the reference no longer stands where `near` was measured, and
  // a card set by that box can land off the new screen. Measuring the
  // reference again from here would read it before the reader's scroll,
  // which runs after this card's effects, and paged it may be on another
  // page: the card sits mid-screen instead, on any screen.
  const [resized, setResized] = useState(false)
  useEffect(() => {
    const moved = () => setResized(true)
    window.addEventListener('resize', moved)
    return () => window.removeEventListener('resize', moved)
  }, [])
  const screen = window.innerHeight
  const placed: CSSProperties = { maxHeight: screen * CARD_HEIGHT }
  if (near && !resized) {
    Object.assign(placed, {
      '--footnote-x': `${(near.left + near.right) / 2}px`,
    })
    // Under a reference in the top half and over one in the bottom half, so
    // the note never runs off the screen's nearer edge, where the card at its
    // tallest fits there; else the other side; and where it fits neither,
    // beside a reference as tall as a figure can be, mid-screen.
    const fitsUnder = near.bottom + 4 + screen * CARD_HEIGHT <= screen
    const fitsOver = near.top - 4 >= screen * CARD_HEIGHT
    if (fitsUnder && (near.bottom < screen / 2 || !fitsOver)) {
      placed.top = near.bottom + 4
    } else if (fitsOver) placed.bottom = screen - near.top + 4
  }
  const byReference = placed.top !== undefined || placed.bottom !== undefined
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the click is a tap beside the note; a keyboard closes it with Escape, the dialog's own cancel
    <dialog
      ref={dialog}
      className="book-footnote"
      // The reader's page, which `.book-text` alone carries and this dialog
      // sits outside of.
      data-page={page}
      aria-label="Footnote"
      onCancel={(e) => {
        // Closed by its owner no longer drawing it, so the browser never
        // closes a dialog React still holds open.
        e.preventDefault()
        onClose()
      }}
      onPointerDown={backdrop.onPointerDown}
      onClick={backdrop.onClick}
    >
      <div
        className="book-footnote-card"
        lang={lang}
        data-placed={byReference || undefined}
        style={placed}
      >
        <p className="book-footnote-text">
          <Runs
            runs={pieces(block.text, [], [], block.marks).flatMap(
              (p) => p.runs,
            )}
            work={work}
            edition={edition}
            furigana={furigana}
          />
        </p>
        <button type="button" className="reader-button" onClick={onGo}>
          Go to note
        </button>
      </div>
    </dialog>
  )
}
