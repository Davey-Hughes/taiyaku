/**
 * Choosing one folder by its path.
 *
 * **A field and a button, not a form.** It may be drawn inside another
 * form, and a form inside a form is not one. Enter in the box picks, and is
 * kept from submitting whatever form is around it.
 *
 * **Without an `action` it is a field and no button.** The form that makes a
 * library takes one folder, so its own submit is what sends the path, read
 * from the box, and Enter submits that form. `onPath` reports what the box
 * holds a render after it changes, which is soon enough to hold a button and
 * too late for an Enter pressed straight after a keystroke.
 *
 * **The box empties when the pick was taken, and not before.** The caller
 * adds the folder and answers a promise; a refusal rejects it, and the typed
 * path stays for the correction. An answer that is not a promise is taken at
 * once.
 *
 * **Browse… is a way to find a path, not a second way to choose one.** It
 * lists directories beneath the prefixes the server was started with, and
 * entering one writes its path into the box. The button beside the box is
 * still what picks it, so a browsed path and a typed one take the same
 * route and meet the same refusals. Nothing is asked of the server until the
 * browser is opened. With no prefix configured it says so, and the box is
 * what is left; the server sends the same empty listing when it was told and
 * none of what it was told exists, so the sentence names both.
 *
 * No path is checked here. Whether the server can read a folder is a question
 * a browser cannot ask, and the refusal's own sentence says which check
 * failed.
 */
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { type FolderListing, listFolders, READING_DELAY_MS } from './libraries'
import { sentenceOf } from './message'

export interface FolderPickerProps {
  /** What the button says. Without one there is no button, and no pick. */
  action?: string
  /**
   * The picked path. A rejected promise keeps the typed path for a retry.
   *
   * Typed `=> unknown` rather than as a union with `void`, which biome
   * refuses: what matters here is that a returned promise is awaited, not
   * what a caller that returns nothing is called.
   */
  onPick?: (path: string) => unknown
  /**
   * Nothing is picked while held, by the button or by Enter. The box can still
   * be typed in and the folders browsed, which only reads listings.
   */
  disabled?: boolean
  /**
   * The path in the box changed, by hand or by browsing, so
   * what the caller said about the path that was there is stale.
   */
  onEdit?: () => void
  /**
   * The text in the box, each time it changes: typed, browsed to, or emptied
   * once a pick was taken. A caller with a button of its own holds it from
   * this while the box is empty.
   */
  onPath?: (path: string) => void
  /**
   * The caller's own controls, drawn after Browse… on the buttons' line and
   * before the browser, so a Cancel stays with the buttons it belongs to
   * whether or not the browser is open, and is reached in that order.
   */
  children?: ReactNode
}

export function FolderPicker({
  action,
  onPick,
  disabled = false,
  onEdit,
  onPath,
  children,
}: FolderPickerProps) {
  const [path, setPath] = useState('')
  // In a ref, so a caller's new closure on each render does not count as a
  // change of path.
  const told = useRef(onPath)
  told.current = onPath
  useEffect(() => {
    told.current?.(path)
  }, [path])
  const [picking, setPicking] = useState(false)
  /** A ref as well as `picking`: two presses before a render both read false. */
  const inFlight = useRef(false)

  // Checked here as well as on the button, since Enter in the box picks too.
  const pick = () => {
    if (onPick === undefined || path === '' || disabled || inFlight.current)
      return
    inFlight.current = true
    setPicking(true)
    const picked = path
    const settle = () => {
      inFlight.current = false
      setPicking(false)
    }
    // A promise built around the call, so a throw settles the picker as a
    // rejection does.
    new Promise((resolve) => resolve(onPick(picked))).then(
      () => {
        settle()
        // Only the path that was taken is emptied: whatever was typed while
        // the pick was pending is the next one.
        setPath((typed) => (typed === picked ? '' : typed))
      },
      () => {
        // The caller drew the refusal. The path stays to be corrected.
        settle()
      },
    )
  }

  const [open, setOpen] = useState(false)
  const [listing, setListing] = useState<FolderListing | undefined>()
  const [browseFailure, setBrowseFailure] = useState<string | undefined>()
  /** The newest listing asked for. Answers need not arrive in order. */
  const latest = useRef(0)

  /**
   * A listing is being read. The group says it is busy, and the listing it
   * had stays drawn as it was, so nothing moves under the pointer that
   * pressed.
   */
  const [reading, setReading] = useState(false)
  /** The read has outlasted `READING_DELAY_MS`, so the person is told. */
  const [slow, setSlow] = useState(false)
  const slowTimer = useRef<ReturnType<typeof setTimeout>>(undefined)
  useEffect(() => () => clearTimeout(slowTimer.current), [])
  /** The box was typed in since the listing in flight was asked for. */
  const typed = useRef(false)
  /** The next listing was entered or climbed to, so focus follows it. */
  const moved = useRef(false)
  const where = useRef<HTMLParagraphElement>(null)

  const go = (to?: string, entering = false) => {
    const seq = ++latest.current
    setReading(true)
    // One timer, the newest read's: an older read cannot say a newer is slow.
    clearTimeout(slowTimer.current)
    slowTimer.current = setTimeout(() => setSlow(true), READING_DELAY_MS)
    const settle = () => {
      clearTimeout(slowTimer.current)
      setReading(false)
      setSlow(false)
    }
    typed.current = false
    listFolders(to)
      .then((answer) => {
        if (seq !== latest.current) return
        settle()
        // Cleared here and not at the press: the refusal is drawn above the
        // listing, and taking it away while the next is read moves the list.
        setBrowseFailure(undefined)
        moved.current = entering
        setListing(answer)
        // A path typed while the listing was being read is the person's.
        if (answer.path !== null && !typed.current) {
          onEdit?.()
          setPath(answer.path)
        }
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        settle()
        setBrowseFailure(sentenceOf(err))
      })
  }

  const browser = useRef<HTMLDivElement>(null)

  // Entering a folder or going up replaces what the person was looking at,
  // and the button they pressed may be gone with it: focus goes to the line
  // that says where they now are, which also has it read. Only when focus is
  // still in the browser or has fallen to the page: a listing can take
  // seconds, and a person who has gone on to type the path in the box, or
  // anywhere else, keeps their place.
  useEffect(() => {
    if (listing === undefined || !moved.current) return
    moved.current = false
    const at = document.activeElement
    if (at === document.body || browser.current?.contains(at)) {
      where.current?.focus()
    }
  }, [listing])

  const toggle = () => {
    if (!open && listing === undefined) go()
    setOpen(!open)
  }

  return (
    <div className="folder-picker">
      {/* O175: the `aria-label` repeats the label's words, which `.field`
          draws in capitals. */}
      <label className="field">
        <span>Folder path</span>
        <input
          type="text"
          name="path"
          aria-label="Folder path"
          value={path}
          placeholder="/media/Manga/Volumes"
          onChange={(e) => {
            typed.current = true
            onEdit?.()
            setPath(e.target.value)
          }}
          onKeyDown={(e) => {
            if (e.key !== 'Enter' || action === undefined) return
            e.preventDefault()
            pick()
          }}
        />
      </label>
      {action !== undefined && (
        <button
          type="button"
          name="pick"
          onClick={pick}
          disabled={path === '' || disabled || picking}
        >
          {action}
        </button>
      )}
      <button type="button" name="browse" aria-expanded={open} onClick={toggle}>
        Browse…
      </button>
      {children}
      {open && (
        <div
          ref={browser}
          className="folder-browser"
          role="group"
          aria-label="Browse for a folder"
          aria-busy={reading}
        >
          {browseFailure !== undefined && <p role="alert">{browseFailure}</p>}
          {listing === undefined ? null : (
            <>
              <p className="where" ref={where} tabIndex={-1}>
                {listing.path === null ? (
                  'Folders this server may browse'
                ) : (
                  <code>{listing.path}</code>
                )}
              </p>
              {listing.path !== null && (
                // O207: where Up goes is the server's answer, the canonical
                // path's parent while that is inside a browsable folder and
                // none once it is not, so it is read and never cut from the
                // path. The arrow is for the eye; the name is where it goes.
                <button
                  type="button"
                  name="up"
                  onClick={() => go(listing.parent ?? undefined, true)}
                >
                  <span aria-hidden="true">↑ </span>
                  {listing.parent === null
                    ? 'Back to all browsable folders'
                    : `Up to ${lastPart(listing.parent)}`}
                </button>
              )}
              {listing.folders.length === 0 ? (
                <p className="empty">
                  {listing.path === null
                    ? 'No folders to browse: either the server was not told ' +
                      'which it may browse (--folders, or TAIYAKU_FOLDERS), ' +
                      'or none of the folders it was told about exist. ' +
                      'Type the path instead.'
                    : 'No folders in here.'}
                </p>
              ) : (
                <ul className="folder-list">
                  {listing.folders.map((folder) => (
                    <li key={folder.path}>
                      <button
                        type="button"
                        name="enter"
                        onClick={() => go(folder.path, true)}
                      >
                        {folder.name}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
          {/* After the listing, so saying it moves nothing above. */}
          {slow && <p className="reading">Reading folders…</p>}
        </div>
      )}
    </div>
  )
}

/**
 * The folder Up names, by its last part: the line above it holds the whole
 * path. A trailing slash is not a part, and the root has none but `/`.
 */
function lastPart(path: string): string {
  return path.split('/').findLast((part) => part !== '') ?? '/'
}
