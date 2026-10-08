/**
 * Moving a folder to another library, and what that does.
 *
 * **Nothing is asked of the server until a target is chosen and the
 * rehearsal asked for**, and the confirm does not exist until the rehearsal
 * has answered. `RemoveRoot`'s rule, for a move: a rehearsal derives
 * the whole store, and a button that moves a folder under a
 * sentence that says nothing is what this component is for preventing.
 *
 * **The rehearsal is for one target.** Choosing another takes the counts and
 * the confirm away, so the button never says one library over counts taken
 * for another.
 *
 * **A refusal is the sentence and the list.** The server refuses a move
 * that cannot keep every reader's place, names the first few volumes in its
 * sentence and lists them all beside it.
 */
import { useEffect, useRef, useState } from 'react'
import type { Root } from './config'
import {
  type MoveAnswer,
  type MoveTarget,
  moveCost,
  moveFolder,
} from './libraries'
import { sentenceOf, volumesOf } from './message'
import { MoveCounts } from './MoveCounts'
import { Refusal } from './Refusal'
import { DeriveSection } from './ScanReport'

/** A library, as far as a move needs one. */
export interface NamedLibrary {
  id: number
  name: string
}

export interface MoveRootProps {
  root: Root
  /** The library the folder is in. */
  from: NamedLibrary
  /** Every other library. */
  targets: NamedLibrary[]
  /**
   * Something else in the folder's library has begun, so the counts quoted
   * here are from before it. The dialog cancels itself when this becomes
   * true, as its Cancel does, and so not while its move is out.
   */
  held?: boolean
  onCancel: () => void
  /** Called once the folder has moved, with what the move did. */
  onMoved: (answer: MoveAnswer) => void
}

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
}: MoveRootProps) {
  /** `''`, a library's id as text, or `'new'`. */
  const [choice, setChoice] = useState('')
  const [name, setName] = useState('')
  const [cost, setCost] = useState<MoveAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [volumes, setVolumes] = useState<string[]>([])
  const [asking, setAsking] = useState(false)
  const [moving, setMoving] = useState(false)
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

  // A library chosen and then gone from `targets` (removed from another
  // card, or emptied by a move) is no choice: the select shows none.
  const gone =
    choice !== '' &&
    choice !== 'new' &&
    !targets.some((t) => String(t.id) === choice)
  useEffect(() => {
    if (!gone) return
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
    if (to === undefined) return
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
    moveFolder(root.id, to)
      .then((answer) => onMoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setMoving(false)
        // A refused move leaves what was rehearsed behind it: the store is
        // not as the counts said, so they and the confirm go, and the
        // person asks again.
        setCost(undefined)
        refuse(err)
      })
  }

  const moved = cost?.moved

  return (
    <div
      className="move-root"
      role="group"
      aria-label={`Move the folder at ${root.path}`}
    >
      <div className="fields">
        {/* O175: each `aria-label` repeats its label's words. */}
        <label className="field">
          <span>Move to</span>
          <select
            name="move-target"
            aria-label="Move to"
            value={choice}
            disabled={moving}
            onChange={(e) => {
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
        </label>
        {choice === 'new' && (
          <label className="field">
            <span>Its name</span>
            <input
              type="text"
              name="new-library"
              aria-label="Its name"
              value={name}
              disabled={moving}
              onChange={(e) => {
                setName(e.target.value)
                forget()
              }}
            />
          </label>
        )}
      </div>
      <button
        type="button"
        name="rehearse"
        onClick={rehearse}
        disabled={to === undefined || asking || moving}
      >
        {asking ? 'Working out what this would do…' : 'See what this would do'}
      </button>

      {failure !== undefined && <Refusal>{failure}</Refusal>}
      {volumes.length > 0 && (
        <details className="move-volumes">
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
          <button
            type="button"
            className="confirm"
            onClick={confirm}
            disabled={moving}
          >
            {moving ? 'Moving…' : `Move ${root.path} to ${moved.library}`}
          </button>
          <details className="removal-derive">
            <summary>What deriving after the move would find</summary>
            <DeriveSection derive={cost.derive} level={5} />
          </details>
        </>
      )}
      <button type="button" onClick={onCancel} disabled={moving}>
        Cancel
      </button>
    </div>
  )
}
