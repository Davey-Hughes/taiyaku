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
import { type ReactNode, useId, useRef, useState } from 'react'
import { LIBRARIES, libraryNameClash } from './configAddress'
import { DirPicker } from './DirPicker'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import { type Created, createLibrary, type Library } from './libraries'
import { sentenceOf } from './message'
import { NameClash } from './NameClash'
import { Refusal } from './Refusal'
import { PageHeader, Row, Section } from './settingsLayout'
import { WaitWords } from './WaitWords'

/** The name `DirPicker` gives its path box, which the submit reads. */
const PICKER_BOX = 'path'

export interface NewLibraryProps {
  /**
   * The libraries the list holds, so that a name one of them holds is
   * refused as it is typed; `undefined` while the list is unread, when only
   * the server refuses it.
   */
  libraries: readonly Pick<Library, 'id' | 'name'>[] | undefined
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

/** The submit's words while the server checks the directory. */
const CHECKING = 'Checking…'

export function NewLibrary({
  libraries,
  notices,
  onMade,
  onLateRefusal,
  onCancel,
}: NewLibraryProps) {
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [creating, setCreating] = useState(false)
  const creatingDrawn = useDrawnWait(creating)
  /** Why the last library was not made. */
  const [failure, setFailure] = useState<string | undefined>()
  /** A ref as well as `creating`: two submits before a render both read false. */
  const inFlight = useRef(false)
  const drawn = useDrawn()

  /**
   * Why the server would refuse the name typed, as the list judges it. The
   * button is held at once while there is one, since it is no wait: the
   * server still judges every make, against a list another admin may have
   * changed.
   */
  const clash = libraryNameClash(name, libraries ?? [])
  /**
   * The clash as it is said: not while a refusal in the same words is
   * drawn, which an edit takes away, so that one sentence is never said
   * twice. Beside a refusal in other words it is said, since it is what
   * holds Make the library.
   */
  const saidClash = failure === clash ? undefined : clash
  const clashNote = useId()
  const nameBox = useRef<HTMLInputElement>(null)
  const makeButton = useRef<HTMLButtonElement>(null)
  const cancelButton = useRef<HTMLButtonElement>(null)
  useFocusLeavesHeld(clash !== undefined || creatingDrawn, makeButton, nameBox)
  // The wait holds Cancel too, which a person may have tabbed to.
  useFocusLeavesHeld(creatingDrawn, cancelButton, nameBox)

  /** Holds the button only: what is sent is read from the box. */
  const ready = name !== '' && path.trim() !== '' && clash === undefined

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
    if (name === '' || dir === '' || clash !== undefined || inFlight.current) {
      return
    }
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
    <div className="settings-page new-library">
      <PageHeader
        back={{ to: LIBRARIES, label: 'Libraries' }}
        title="New library"
        description="A library starts with one directory. More can be added on its page. Every directory is derived together, whichever library holds it: a library decides who sees its directories."
      />
      {notices}
      <Section>
        <form onSubmit={submit} className="add-library settings-group">
          {/* O175: the `aria-label` is its row label's words as written. */}
          <Row label="Name">
            {(ids) => (
              <input
                ref={nameBox}
                type="text"
                id={ids.control}
                name="library-name"
                aria-label="Name"
                aria-invalid={saidClash === undefined ? undefined : true}
                aria-describedby={
                  saidClash === undefined ? undefined : clashNote
                }
                value={name}
                onChange={(e) => {
                  setFailure(undefined)
                  setName(e.target.value)
                }}
              />
            )}
          </Row>
          <Row
            label="Directory path"
            help="The server checks that it can read the directory and that it overlaps no other directory."
          >
            {(ids) => (
              <DirPicker
                ids={ids}
                onPath={(typed) => {
                  setFailure(undefined)
                  setPath(typed)
                }}
              />
            )}
          </Row>
          {/* At the box's foot, after the last field, where a form's
              submit is looked for; the refusal beneath answers it. */}
          <div className="settings-actions">
            <button
              ref={makeButton}
              type="submit"
              className="keeps-wait-width"
              disabled={creatingDrawn || !ready}
            >
              <WaitWords
                drawn={creatingDrawn}
                words="Make the library"
                wait={CHECKING}
              />
            </button>
            <button
              ref={cancelButton}
              type="button"
              name="cancel"
              disabled={creatingDrawn}
              onClick={() => {
                if (!creating) onCancel()
              }}
            >
              Cancel
            </button>
          </div>
          {failure !== undefined && <Refusal>{failure}</Refusal>}
          <NameClash id={clashNote} sentence={saidClash} />
        </form>
      </Section>
    </div>
  )
}
