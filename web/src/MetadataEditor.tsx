/**
 * A correction to one thing already on screen — spec C3 §5.
 *
 * **Contextual, not a metadata section.** There is no route to navigate to:
 * the same editor mounts wherever the thing being corrected already renders
 * — a volume's own title, on the series page — because "the correction is
 * always prompted by looking at something wrong" (§5's own words). This file
 * knows nothing about a route or which caller mounted it; `Series.tsx`
 * decides when one belongs on screen and which PATCH route `onPatch` calls.
 *
 * **Scope: the one field every one of §5's contextual entry points can edit
 * with no data it does not already have.** `metadata.ts`'s `PatchWork`,
 * `patchSeries`, `patchEdition` and `patchMember` also cover volume number,
 * series membership, edition label/kind/language/credits and a member's
 * chapter — none of those are wired to a screen yet. See this branch's own
 * task report for why: a title correction from a volume's row needs only the
 * slug `library.ts` already hands the page, but the sibling entry point the
 * spec also asks for — a series page correcting its own title — would have
 * to replace `LibraryDoc.series.titles` wholesale (`librarydoc::set_series_titles`
 * in `taiyaku-library`, which `patch_series` calls, replaces a series' whole
 * title set rather than merging into it), and nothing the browse API returns
 * for a series carries that array back — `SeriesCard.title` is already the
 * merged display string, never the underlying `{ title, kind, primary }`
 * entries. Wiring that up here would silently drop a series' native/alt
 * titles the moment anyone corrected the romaji one, so it is not wired up
 * until a read path exists to carry the current array back.
 *
 * **`persisted: false` is stated, not hidden, and it is not an alarm.**
 * `metadata.ts`'s own module doc is explicit that this answer is not a
 * failure — the edit is already safe from the next scan, and only the
 * `<stem>.library.json` beside the library did not land (a read-only root,
 * or a work with no on-disk address at all — every sidecar-imported work).
 * So the note below renders as plain text, not `role="alert"`, which stays
 * reserved for the one path that really did lose the edit: a thrown
 * 409/404/403 from a hard failure.
 */
import { type FormEvent, useState } from 'react'
import { type PatchResult, reasons, type WorkEdit } from './metadata'

export function MetadataEditor({
  slug,
  title,
  onPatch,
}: {
  slug: string
  title: string
  /**
   * Same shape as `patchWork` itself — the caller hands over whichever of
   * the four PATCH routes fits the thing being corrected. `slug` is a loose
   * name for "whatever identifier that route takes": a work's slug today,
   * but nothing here inspects it.
   */
  onPatch: (slug: string, fields: WorkEdit) => Promise<PatchResult>
}) {
  const [value, setValue] = useState(title)
  const [current, setCurrent] = useState(title)
  const [saving, setSaving] = useState(false)
  // Three states, not two. `undefined` means "nothing to say" — either
  // nothing has been saved yet, or the last save reached disk cleanly. `[]`
  // means the last save succeeded but did not reach `<stem>.library.json`
  // with no reason to name — a work imported from a sidecar, whose document
  // has no on-disk address at all (`hand::path_for` answers `None`, so
  // `hand::Saved::why` is `None` too, and `reasons()` defaults that to `[]`
  // rather than `undefined`). A non-empty array is the same outcome with a
  // reason attached — a read-only root's own `io::Error`. The JSX below
  // branches on all three: `unwritten !== undefined` decides whether the note
  // renders at all, and `unwritten.length > 0` decides, independently,
  // whether it renders a reason.
  const [unwritten, setUnwritten] = useState<string[]>()
  const [failure, setFailure] = useState<string>()

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setSaving(true)
    setFailure(undefined)
    onPatch(slug, { title: value })
      .then((result) => {
        setSaving(false)
        // The re-projected document, not the value just typed — the two
        // agree on the ordinary path, but a document is the server's word
        // for what it actually holds now.
        setCurrent(result.title ?? value)
        setUnwritten(result.persisted ? undefined : reasons(result))
      })
      .catch((err: unknown) => {
        setSaving(false)
        setFailure(err instanceof Error ? err.message : String(err))
      })
  }

  return (
    <form className="metadata-editor" onSubmit={submit}>
      <label className="field title">
        <span>Title</span>
        <input
          type="text"
          name="title"
          value={value}
          onChange={(event) => {
            setValue(event.target.value)
          }}
        />
      </label>
      <button type="submit" disabled={saving}>
        Save
      </button>
      <p className="current">{current}</p>
      {unwritten !== undefined && (
        <div className="unwritten">
          <p>
            Saved — but this correction could not be saved beside your library
            {unwritten.length > 0 ? ':' : '.'}
          </p>
          {unwritten.length > 0 && (
            <ul>
              {unwritten.map((why) => (
                <li key={why}>{why}</li>
              ))}
            </ul>
          )}
        </div>
      )}
      {failure !== undefined && (
        <p className="failure" role="alert">
          {failure}
        </p>
      )}
    </form>
  )
}
