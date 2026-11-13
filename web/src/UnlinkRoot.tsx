/**
 * Taking a directory out of one library, which another library keeps
 * (overlapping-libraries spec, decision 24).
 *
 * **It deletes nothing of the directory.** Its works, and everything anybody
 * recorded against them, stay with the libraries still holding it. What it
 * changes is who sees the directory and, where it is this library's last
 * directory, the library, which the server deletes with its grants. So the
 * confirm says those, from the rehearsal, as Move's does, and does not exist
 * until the rehearsal has answered: `RemoveRoot`'s rule.
 *
 * **Laid out as every removal's confirm is**: what it does, then what it
 * keeps, then the confirm and Cancel on one line. It has no disclosure,
 * since an unlink derives nothing.
 *
 * **A refusal is handed up, and the dialog goes with it.** What it quoted no
 * longer holds, or never did, and the server's sentence may send the person
 * elsewhere on the page: on the directory's last library, to Remove this
 * directory. The page draws it over the row of the button that opened this.
 */
import { type FocusEventHandler, useEffect, useRef, useState } from 'react'
import { type Root, statusOf } from './config'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait } from './hooks'
import {
  libraryRemovedSentence,
  namesOf,
  noLibrarySentence,
  type UnlinkAnswer,
  unlinkCost,
  unlinkDir,
} from './libraries'
import { messageOf, sentenceOf } from './message'
import type { NamedLibrary } from './MoveRoot'
import { WaitWords } from './WaitWords'

/** The confirm's words while the unlink is out. */
const REMOVING = 'Removing…'

export interface UnlinkRootProps {
  root: Root
  /** The library whose page this is, which the directory leaves. */
  from: NamedLibrary
  /** Every other library holding the directory, by name: each keeps it. */
  keptBy: string[]
  /**
   * The server would refuse the unlink now, as it would a move, and what is
   * quoted here may be from before what holds it. The dialog cancels itself
   * when this becomes true, as its Cancel does, and so not while its unlink
   * is out.
   */
  held?: boolean
  onCancel: () => void
  /** Called once the directory has left the library, with what that did. */
  onUnlinked: (answer: UnlinkAnswer) => void
  /**
   * The rehearsal or the unlink was refused while the dialog was drawn, with
   * why and the answer's status. Its owner draws the sentence and this
   * dialog no more.
   */
  onRefused: (sentence: string, status?: number) => void
  /** Focus coming into the dialog, for its owner, which may take it away. */
  onFocus?: FocusEventHandler<HTMLDivElement>
  /** Focus leaving the dialog. */
  onBlur?: FocusEventHandler<HTMLDivElement>
  /** The unlink was refused once this dialog had gone, with why. */
  onLateRefusal?: (sentence: string) => void
}

export function UnlinkRoot({
  root,
  from,
  keptBy,
  held = false,
  onCancel,
  onUnlinked,
  onRefused,
  onLateRefusal,
  onFocus,
  onBlur,
}: UnlinkRootProps) {
  const drawn = useDrawn()
  const [cost, setCost] = useState<UnlinkAnswer | undefined>()
  const [unlinking, setUnlinking] = useState(false)
  const unlinkingDrawn = useDrawnWait(unlinking)
  /** The rehearsal's wait, drawn once it has lasted; until then it keeps its room. */
  const costDrawn = useDrawnWait(cost === undefined)
  // Read when `held` changes, and not a dependency: an unlink that is out
  // when the hold comes answers into this dialog.
  const state = useRef({ unlinking, onCancel })
  state.current = { unlinking, onCancel }
  useEffect(() => {
    if (held && !state.current.unlinking) state.current.onCancel()
  }, [held])
  // Read by the rehearsal's answer, which may come after a render that
  // handed over a fresh closure.
  const refused = useRef(onRefused)
  refused.current = onRefused
  /** A ref as well as `unlinking`: two presses before a render both read false. */
  const inFlight = useRef(false)

  useEffect(() => {
    let cancelled = false
    unlinkCost(from.id, root.id)
      .then((answer) => {
        if (!cancelled) setCost(answer)
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          refused.current(sentenceOf(err), statusOf(messageOf(err)))
        }
      })
    return () => {
      cancelled = true
    }
  }, [from.id, root.id])

  const confirm = () => {
    if (inFlight.current) return
    inFlight.current = true
    setUnlinking(true)
    unlinkDir(from.id, root.id)
      .then((answer) => onUnlinked(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setUnlinking(false)
        const sentence = sentenceOf(err)
        if (drawn.current) refused.current(sentence, statusOf(messageOf(err)))
        else
          onLateRefusal?.(
            `${root.path} was not removed from ${from.name} (${sentence}).`,
          )
      })
  }

  const unlinked = cost?.unlinked
  const keepers = namesOf(keptBy)
  const removed =
    unlinked === undefined ? undefined : libraryRemovedSentence(unlinked, false)

  return (
    <div
      className="unlink-root"
      role="group"
      onFocus={onFocus}
      onBlur={onBlur}
      aria-label={`Remove ${root.path} from ${from.name}`}
    >
      {unlinked === undefined ? (
        <p className={costDrawn ? undefined : 'wait-unseen'}>
          Working out what this would do…
        </p>
      ) : (
        <>
          <p className="counts">
            Removing this directory from {from.name}:{' '}
            {unlinked.loses_sight.length === 0
              ? 'no account'
              : namesOf(unlinked.loses_sight)}{' '}
            will lose sight of it or a directory inside it.
          </p>
          {removed !== undefined && <p className="last-dir">{removed}</p>}
          {unlinked.left_with_no_library.length > 0 && (
            <p className="left-with-no-library">
              {noLibrarySentence(unlinked.left_with_no_library, from.name)}
            </p>
          )}
          <p className="kept">
            {keepers} {keptBy.length === 1 ? 'keeps' : 'keep'} the directory,
            its works and everything recorded against them; none of it is
            deleted.
          </p>
        </>
      )}
      {/* As every removal's confirm draws them: the confirm and Cancel on
          one line, in that order in the document too. */}
      <div className="settings-actions">
        {unlinked !== undefined && (
          <HeldButton
            type="button"
            className="confirm keeps-wait-width"
            onClick={confirm}
            held={unlinkingDrawn}
          >
            <WaitWords
              drawn={unlinkingDrawn}
              words={`Remove from ${from.name}`}
              wait={REMOVING}
            />
          </HeldButton>
        )}
        <HeldButton
          type="button"
          onClick={() => {
            if (!unlinking) onCancel()
          }}
          held={unlinkingDrawn}
        >
          Cancel
        </HeldButton>
      </div>
    </div>
  )
}
