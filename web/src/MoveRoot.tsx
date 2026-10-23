/**
 * Moving a directory to another library, and what that does.
 *
 * **Nothing is asked of the server until a target is chosen and the
 * rehearsal asked for**, and the confirm does not exist until the rehearsal
 * has answered. `RemoveRoot`'s rule, for a move: a rehearsal derives
 * the whole store, and a button that moves a directory under a
 * sentence that says nothing is what this component is for preventing.
 *
 * **The rehearsal is for one target.** Choosing another takes the counts and
 * the confirm away, so the confirm is never offered under counts taken for
 * another.
 *
 * **A refusal is the sentence and the list.** The server refuses a move
 * that cannot keep every reader's place, names the first few volumes in its
 * sentence and lists them all beside it.
 *
 * **An account whose only grant is the library the move empties is named
 * under the counts, and the confirm is still offered.** The move leaves it
 * seeing no library until an administrator grants one; the rehearsal names
 * it, and the move does what the rehearsal described.
 *
 * **It is a modal `<dialog>`, opened when it is drawn**, so the page beneath
 * holds still and is inert while it is open, and its owner closes it by not
 * drawing it. A request from the browser to close it (Escape) is turned down
 * and handed to `onCancel`, as Cancel is, so that the dialog is never closed
 * while its owner still draws it; while the move is out it is turned down
 * and nothing else happens, as Cancel is held then.
 *
 * **A wait is drawn only once it has lasted `WAIT_DRAWN_AFTER_MS`**: the
 * rehearsal's and the move's words, and the held look of what they hold.
 * What they hold is held from the send.
 */
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react'
import type { Root } from './config'
import { HeldButton } from './HeldButton'
import {
  type MoveAnswer,
  type MoveTarget,
  moveCost,
  moveDir,
  noLibrarySentence,
} from './libraries'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import { sentenceOf, volumesOf } from './message'
import { MoveCounts } from './MoveCounts'
import { PathWithBreaks } from './PathWithBreaks'
import { Refusal } from './Refusal'
import { DeriveSection } from './ScanReport'
import { Row } from './settingsLayout'
import { WaitWords } from './WaitWords'

/** A library, as far as a move needs one. */
export interface NamedLibrary {
  id: number
  name: string
}

export interface MoveRootProps {
  root: Root
  /** The library the directory is in. */
  from: NamedLibrary
  /** Every other library. */
  targets: NamedLibrary[]
  /**
   * Something else in the directory's library has begun, so the counts quoted
   * here are from before it. The dialog cancels itself when this becomes
   * true, as its Cancel does, and so not while its move is out.
   */
  held?: boolean
  onCancel: () => void
  /** Called once the directory has moved, with what the move did. */
  onMoved: (answer: MoveAnswer) => void
  /** The move was refused once this dialog had gone, with why. */
  onLateRefusal?: (sentence: string) => void
}

/** What each button says while its wait is drawn. */
const ASKING = 'Working out what this would do…'
const MOVING = 'Moving…'

/** `n` of a thing, in the number the count is. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export function MoveRoot({
  root,
  from,
  targets,
  held = false,
  onCancel,
  onMoved,
  onLateRefusal,
}: MoveRootProps) {
  const drawn = useDrawn()
  const dialog = useRef<HTMLDialogElement>(null)
  const title = useId()
  /** `''`, a library's id as text, or `'new'`. */
  const [choice, setChoice] = useState('')
  const [name, setName] = useState('')
  const [cost, setCost] = useState<MoveAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [volumes, setVolumes] = useState<string[]>([])
  const [asking, setAsking] = useState(false)
  const [moving, setMoving] = useState(false)
  const askingDrawn = useDrawnWait(asking)
  const movingDrawn = useDrawnWait(moving)
  /** A ref as well as `moving`: two presses before a render both read false. */
  const inFlight = useRef(false)
  /** The newest rehearsal. Answers need not arrive in order. */
  const latest = useRef(0)
  // Read when `held` changes, and not a dependency: a move that is refused
  // while the hold stands must leave its refusal drawn.
  const state = useRef({ moving, onCancel })
  state.current = { moving, onCancel }
  useEffect(() => {
    if (held && !state.current.moving) state.current.onCancel()
  }, [held])

  // Before the first paint, so that no frame draws it closed. Asked only of
  // a closed dialog: StrictMode runs this twice on one element.
  useLayoutEffect(() => {
    if (dialog.current?.open === false) dialog.current.showModal()
  }, [])
  // Escape during the move is turned down as a key, and not only as the
  // request to close it that the key makes: Chromium lets a page turn down
  // only so many of those with no press between them, and then closes the
  // dialog over its move. At the document, so that it is turned down
  // wherever focus is: on a control the move holds, which keeps it, or on
  // the page, where a click on the dialog's words leaves it.
  useEffect(() => {
    if (!moving) return
    const refuse = (e: KeyboardEvent) => {
      if (e.key === 'Escape') e.preventDefault()
    }
    document.addEventListener('keydown', refuse)
    return () => document.removeEventListener('keydown', refuse)
  }, [moving])

  const targetSelect = useRef<HTMLSelectElement>(null)
  const seeButton = useRef<HTMLButtonElement>(null)
  const confirmButton = useRef<HTMLButtonElement>(null)
  const volumesInTheWay = useRef<HTMLDetailsElement>(null)
  const deriveWouldFind = useRef<HTMLDetailsElement>(null)

  // A library chosen and then gone from `targets` (removed elsewhere, or
  // emptied by a move) is no choice: the select shows none, and what was
  // rehearsed for it goes. Focus in what goes would fall to the page, so it
  // goes to Move to, where a library is chosen again. Read here, while what
  // goes is still drawn: once it is gone, focus is on the page and where it
  // was cannot be told.
  const gone =
    choice !== '' &&
    choice !== 'new' &&
    !targets.some((t) => String(t.id) === choice)
  useEffect(() => {
    if (!gone) return
    const focus = document.activeElement
    if (
      [confirmButton, volumesInTheWay, deriveWouldFind].some(
        (goes) => goes.current?.contains(focus) === true,
      )
    ) {
      targetSelect.current?.focus()
    }
    setChoice('')
    latest.current += 1
    setCost(undefined)
    setFailure(undefined)
    setVolumes([])
    setAsking(false)
  }, [gone])

  const to: MoveTarget | undefined =
    choice === ''
      ? undefined
      : choice === 'new'
        ? name.trim() === ''
          ? undefined
          : { new_library: name }
        : { library: Number(choice) }

  // With no target, See what this would do is `disabled`. A list read that
  // takes the chosen library away (`gone`) does that while it may have
  // focus, which would fall to the page: it goes to Move to, where a
  // library is chosen again.
  useFocusLeavesHeld(to === undefined, seeButton, targetSelect)

  /** A different target is a different move: what was rehearsed is gone. */
  const forget = () => {
    latest.current += 1
    setCost(undefined)
    setFailure(undefined)
    setVolumes([])
    setAsking(false)
  }

  const refuse = (err: unknown) => {
    setFailure(sentenceOf(err))
    setVolumes(volumesOf(err))
  }

  const rehearse = () => {
    if (to === undefined || asking || moving) return
    const seq = ++latest.current
    setCost(undefined)
    setFailure(undefined)
    setVolumes([])
    setAsking(true)
    moveCost(root.id, to)
      .then((answer) => {
        if (seq !== latest.current) return
        setAsking(false)
        setCost(answer)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setAsking(false)
        refuse(err)
      })
  }

  const confirm = () => {
    if (to === undefined || inFlight.current) return
    inFlight.current = true
    setMoving(true)
    moveDir(root.id, to)
      .then((answer) => onMoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setMoving(false)
        if (!drawn.current) {
          onLateRefusal?.(
            `${root.path} was not moved from ${from.name} (${sentenceOf(err)}).`,
          )
          return
        }
        // A refused move leaves what was rehearsed behind it: the store is
        // not as the counts said, so they and the confirm go, and the
        // person asks again.
        setCost(undefined)
        refuse(err)
      })
  }

  const moved = cost?.moved

  return (
    <dialog
      ref={dialog}
      className="move-root settings-dialog"
      aria-labelledby={title}
      onCancel={(e) => {
        e.preventDefault()
        if (!moving) onCancel()
      }}
      // Some closes cannot be turned down (a phone's back gesture, once the
      // browser stops letting a page refuse), and a move that is out still
      // answers into this dialog, so it opens again.
      onClose={(e) => {
        if (moving) e.currentTarget.showModal()
      }}
    >
      <h2 id={title} className="dialog-title">
        Move{' '}
        <code>
          <PathWithBreaks path={root.path} />
        </code>
      </h2>
      <div className="settings-box">
        {/* O175: each `aria-label` repeats its row label's words. */}
        <Row label="Move to">
          {(ids) => (
            <select
              ref={targetSelect}
              id={ids.control}
              name="move-target"
              aria-label="Move to"
              value={choice}
              aria-disabled={movingDrawn}
              onChange={(e) => {
                // Held from the send; the select shows its value again.
                if (moving) return
                setChoice(e.target.value)
                forget()
              }}
            >
              <option value="">Choose a library…</option>
              {targets.map((library) => (
                <option key={library.id} value={String(library.id)}>
                  {library.name}
                </option>
              ))}
              <option value="new">A new library…</option>
            </select>
          )}
        </Row>
        {choice === 'new' && (
          <Row label="Its name">
            {(ids) => (
              <input
                type="text"
                id={ids.control}
                name="new-library"
                aria-label="Its name"
                value={name}
                readOnly={moving}
                aria-disabled={movingDrawn}
                onChange={(e) => {
                  if (moving) return
                  setName(e.target.value)
                  forget()
                }}
              />
            )}
          </Row>
        )}
        {/* Under the fields it rehearses, where their boxes start. What it
            answers is drawn after it, so that the answer is read after the
            question and before the buttons that act on it. */}
        <div className="settings-row move-rehearse">
          <div className="settings-row-control">
            <HeldButton
              ref={seeButton}
              type="button"
              name="rehearse"
              className="keeps-wait-width"
              onClick={rehearse}
              held={askingDrawn || movingDrawn}
              disabled={to === undefined}
            >
              <WaitWords
                drawn={askingDrawn}
                words="See what this would do"
                wait={ASKING}
              />
            </HeldButton>
          </div>
        </div>
        {failure !== undefined && <Refusal>{failure}</Refusal>}
        {volumes.length > 0 && (
          <details ref={volumesInTheWay} className="move-volumes">
            <summary>
              {count(volumes.length, 'volume', 'volumes')} in the way
            </summary>
            <ul>
              {volumes.map((volume, at) => (
                // By place as well: a list may name one volume twice.
                <li key={`${at}:${volume}`}>{volume}</li>
              ))}
            </ul>
          </details>
        )}

        {moved !== undefined && cost !== undefined && (
          <>
            <MoveCounts
              moved={moved}
              from={from.name}
              newLibrary={choice === 'new'}
            />
            {/* Here and not in `MoveCounts`, which the kept report shares:
                this says what the move will do, and that report says what it
                did. */}
            {moved.left_with_no_library.length > 0 && (
              <p className="left-with-no-library">
                {noLibrarySentence(
                  moved.left_with_no_library,
                  "this directory's library",
                )}
              </p>
            )}
            <details ref={deriveWouldFind} className="removal-derive">
              <summary>What deriving after the move would find</summary>
              <DeriveSection derive={cost.derive} level={5} />
            </details>
          </>
        )}
        {/* At the box's foot, after everything a rehearsal answers, so the
            consequences are read before the button that acts on them. The
            confirm says only "Move":
            the title names the directory, and the counts the library. */}
        <div className="settings-actions">
          {moved !== undefined && (
            <HeldButton
              ref={confirmButton}
              type="button"
              className="confirm keeps-wait-width"
              onClick={confirm}
              held={movingDrawn}
            >
              <WaitWords drawn={movingDrawn} words="Move" wait={MOVING} />
            </HeldButton>
          )}
          <HeldButton
            type="button"
            onClick={() => {
              if (!moving) onCancel()
            }}
            held={movingDrawn}
          >
            Cancel
          </HeldButton>
        </div>
      </div>
    </dialog>
  )
}
