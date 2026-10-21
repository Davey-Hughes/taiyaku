/**
 * Making a library: a name and one directory.
 *
 * **One directory, sent with the name.** The server refuses a library with no
 * directory, so the two go together and the server makes both or neither. More
 * directories are added on the library's own page, one at a time, each meeting
 * the same refusals.
 *
 * **The refusal is drawn in the form**, beside the button that was pressed,
 * and stays until what the form holds changes or the next attempt. A refusal
 * that comes once the form has gone is handed to the screen, which says it
 * on the page the person is on.
 */
import { type ReactNode, useRef, useState } from 'react'
import { Link } from 'react-router'
import { DirPicker } from './DirPicker'
import { useDrawn } from './hooks'
import { type Created, createLibrary } from './libraries'
import { sentenceOf } from './message'
import { Refusal } from './Refusal'

/** The name `DirPicker` gives its path box, which the submit reads. */
const PICKER_BOX = 'path'

export interface NewLibraryProps {
  /**
   * What the screen says above the page: a list that could not be read, or
   * an answer that came once the page it was asked from had gone.
   */
  notices?: ReactNode
  /**
   * The library is made; the screen keeps it. `stillHere` says this form is
   * still drawn, so the person is where they asked and its page is opened. A
   * form left before the answer came is not, even when the person has since
   * opened a fresh one at the same address.
   */
  onMade: (made: Created, stillHere: boolean) => void
  /**
   * The make was refused after this form was left, with the sentence that
   * says so and names the library.
   */
  onLateRefusal?: (sentence: string) => void
  /** Cancel was pressed: the screen goes back to the table. */
  onCancel: () => void
}

export function NewLibrary({
  notices,
  onMade,
  onLateRefusal,
  onCancel,
}: NewLibraryProps) {
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [creating, setCreating] = useState(false)
  /** Why the last library was not made. */
  const [failure, setFailure] = useState<string | undefined>()
  /** A ref as well as `creating`: two submits before a render both read false. */
  const inFlight = useRef(false)
  const drawn = useDrawn()

  /** Holds the button only: what is sent is read from the box. */
  const ready = name !== '' && path.trim() !== ''

  const submit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault()
    // The box itself, not `path`: the picker reports a keystroke a render
    // after it draws it, so an Enter pressed at once would send the path as
    // it stood before that keystroke. Without one box of that name nothing
    // is sent: `path` could be that stale value, and a library made at the
    // wrong directory is worse than a press that does nothing.
    const box = e.currentTarget.elements.namedItem(PICKER_BOX)
    if (!(box instanceof HTMLInputElement)) return
    const dir = box.value.trim()
    if (name === '' || dir === '' || inFlight.current) return
    inFlight.current = true
    setCreating(true)
    setFailure(undefined)
    createLibrary(name, [dir])
      .then((made) => {
        inFlight.current = false
        setCreating(false)
        onMade(made, drawn.current)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setCreating(false)
        const sentence = sentenceOf(err)
        if (drawn.current) setFailure(sentence)
        else onLateRefusal?.(`The library ${name} was not made (${sentence}).`)
      })
  }

  return (
    <div className="new-library">
      <p className="back">
        <Link to="/config/libraries">‹ Libraries</Link>
      </p>
      <h2>New library</h2>
      {notices}
      <form onSubmit={submit} className="add-library">
        {/* O175: the `aria-label` repeats the label's words. */}
        <label className="field">
          <span>Name</span>
          <input
            type="text"
            name="library-name"
            aria-label="Name"
            value={name}
            onChange={(e) => {
              setFailure(undefined)
              setName(e.target.value)
            }}
          />
        </label>
        <DirPicker
          onPath={(typed) => {
            setFailure(undefined)
            setPath(typed)
          }}
        />
        <div className="actions">
          <button type="submit" disabled={creating || !ready}>
            {creating ? 'Checking…' : 'Make the library'}
          </button>
          <button
            type="button"
            name="cancel"
            disabled={creating}
            onClick={onCancel}
          >
            Cancel
          </button>
        </div>
        {failure !== undefined && <Refusal>{failure}</Refusal>}
        <p className="why">
          The server checks that it can read the directory and that it overlaps
          no other directory. More directories can be added on the library's
          page: directories in one library are derived together, so a chapter
          release in one joins its volume in another as a second edition.
        </p>
      </form>
    </div>
  )
}
