/**
 * One series and its volumes — spec C1 §3 and §11 step 7.
 *
 * **Not three layouts.** §3's three mockups are three positions of three
 * switches (`layout`, `editionSelector`, `panel`), so this file is a grid and
 * a table plus two optional attachments. §11 step 8 makes the switches
 * preferences, read from `browseModule.ts`'s `BrowsePrefsContext` rather than
 * taken as props — `Shell` owns the state and `Routes.tsx` mounts this
 * component with nothing else to hand it.
 *
 * **The alignment mark is the whole point of the page.** A series can hold the
 * same volumes in more than one edition with only some of them aligned, and a
 * page that lists volumes and editions without saying which are paired is a
 * Komga screen with extra badges. `WorkCard.aligned` is one bool per volume
 * and no list route carries more; the verdict and the paired count come from
 * `/api/library/works/{slug}` and from nowhere else, which is what the panel
 * is for.
 *
 * **The series is read through the store** (`usePage`), so a return draws
 * what was kept in its first render and reads it again, and a move to a
 * series not kept draws nothing of the one left while it loads. The chosen
 * edition is `?edition=` (spec `2026-10-28-loading-library-design.md` §5.1), so
 * that Back brings it back.
 */
import { useRef } from 'react'
import { Link, useParams, useSearchParams } from 'react-router'
import { useBack } from './back'
import { type BrowsePrefs, useBrowsePrefs } from './browseModule'
import { useGate } from './gate'
import { Cover } from './Cover'
import {
  type EditionCard,
  libraryParam,
  readerPath,
  type SeriesCard,
  unreadable,
  type WorkCard,
  editionDownload,
} from './library'
import { messageOf } from './message'
import { seriesRead } from './pageReads'
import { useReseat } from './reseat'
import { useKeptScroll } from './scrollKeeper'
import { PageLoading } from './settingsLayout'
import { layPanel } from './tilePanel'
import { useDocumentTitle } from './title'
import { usePage } from './usePage'
import { ViewMenu } from './ViewMenu'
import { useVolumePanel } from './volumePanel'
import { VolumePanel } from './VolumePanel'

/** What this page's View panel offers: the preferences that act here. */
const SERIES_VIEW: readonly (keyof BrowsePrefs)[] = [
  'layout',
  'editionSelector',
  'panel',
  'coverSize',
  'alwaysShowAligned',
]

interface Loaded {
  series: SeriesCard
  works: WorkCard[]
}

/**
 * Every edition key in this series, in the order the volumes first mention it.
 *
 * **First-seen rather than sorted**, and that is deliberate: `derive.rs` gives
 * the hub edition to the first member it resolves, so first-seen puts the
 * edition most of this series is held in on the left, where a reader scanning
 * for gaps looks first. Sorting by key would put `en` before `raw` on a shelf
 * that is mostly raw.
 *
 * The label travels with the key because a `WorkCard` carries only the
 * editions **this caller may see** — a grant can hide an edition from volume 1
 * and not from volume 3, so the first volume to mention a key is not always
 * the first volume.
 */
// `unreadable` (O62) lives in `library.ts`, not here: `Table` below and the
// grid both apply it, since `browseModule` makes the grid the default layout,
// and `Library.tsx`'s Continue strip and `SeriesGrid.tsx`'s unfiled works need
// the identical guard. That is why it is a shared pure function rather than a
// fourth copy. See its doc comment in `library.ts` for which edition it
// asks about.

function editionColumns(works: WorkCard[]): { key: string; label: string }[] {
  const seen = new Map<string, string>()
  for (const w of works) {
    for (const e of w.editions) {
      if (!seen.has(e.key)) seen.set(e.key, e.label)
    }
  }
  return [...seen].map(([key, label]) => ({ key, label }))
}

/**
 * The edition `?edition=` names, or `'all'` where it names none that one of
 * `works` holds. Edition keys collide across series, `'en'` exists in most of
 * them, so a key is read against this series' own editions: a leftover one
 * that this series lacks dims nothing, and one it has is the reader's choice
 * however it arrived.
 */
function editionIn(params: URLSearchParams, works: WorkCard[]): string {
  const key = params.get('edition')
  return key !== null && editionColumns(works).some((c) => c.key === key)
    ? key
    : 'all'
}

/**
 * One row per volume, one column per edition — §3's preset B.
 *
 * **The dashed cell is what this layout is for.** A grid says which volumes
 * exist; this says which editions each volume is *missing*, which is the
 * question someone pairing two releases actually asks. A blank cell would say
 * the same thing to a sighted reader and nothing at all to a screen reader,
 * hence the `aria-label` — §9's rule that this screen is testable in `jsdom`
 * cuts both ways: what is easy to assert is also what is easy to leave out.
 *
 * **"Missing" names two different facts below, deliberately kept apart by
 * name.** `className="missing"` is this volume never *having* the column's
 * edition at all — `held` is `undefined`, so there is nothing to have lost.
 * `data-unreadable` is the opposite: the edition exists (`held` is `Some`),
 * and some of the files that make it up are gone from disk since the last
 * scan (`held.missing > 0`). Conflating the two in one class or one word
 * would read a volume that never had a raw release the same as one whose raw
 * release just vanished, which are different repairs.
 *
 * **Two things for a phone** (spec 2026-09-17 §7.2): the table sits in a box
 * that scrolls sideways, so a series with several editions never makes the
 * page do so; and each page count is `pages`, the one cell `library.css`
 * draws in the figures face — the table itself takes the UI face, so the
 * digits inside a volume's file name are the title's own.
 */
function Table({
  works,
  mayDownload,
}: {
  works: WorkCard[]
  mayDownload: boolean
}) {
  const columns = editionColumns(works)
  return (
    <div className="table-scroll">
      <table className="editions">
        <thead>
          <tr>
            <th scope="col">Volume</th>
            {columns.map((c) => (
              <th scope="col" key={c.key}>
                {c.label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {works.map((w) => (
            <tr key={w.slug}>
              <td>
                {unreadable(w) ? (
                  // **O62 — the link is the treatment, not a badge.** Spec C1
                  // §8 asks for a work whose members are gone to be "shown as
                  // unreadable rather than offered and then failing at the
                  // first page", and a mark beside a live link still sends a
                  // reader into a page that cannot load. The volume keeps its
                  // name: withholding that too would read as a library that
                  // has lost the work rather than one that cannot open it.
                  <>
                    <span className="title">{w.title}</span>{' '}
                    <span className="unreadable">
                      files missing — cannot be opened
                    </span>
                  </>
                ) : (
                  <Link to={readerPath(w)}>{w.title}</Link>
                )}
                {w.aligned && (
                  <span className="aligned" title="has an aligned edition">
                    {' '}
                    ⇄
                  </span>
                )}
              </td>
              {columns.map((c) => {
                const held = w.editions.find((e) => e.key === c.key)
                return held ? (
                  // **The page count *is* the control.** Spec C2 §8 asked for
                  // "the same control as a column", which does not survive this
                  // layout's shape: there is one column per edition and no
                  // single edition a row is already showing, so a separate
                  // column would have to pick one arbitrarily — inside the one
                  // view whose whole purpose is comparing them side by side.
                  // §8's "one control rather than three" was an argument about
                  // the tile, which has an edition selector and therefore a
                  // settled question; a row has neither. The number in the cell
                  // is already the per-edition fact, so linking it adds no
                  // element to the layout at all.
                  <td
                    key={c.key}
                    className="pages"
                    // Per edition, because `missing` is: one edition of a
                    // volume can have lost its files while another is intact,
                    // and offering the intact one is right.
                    {...(held.missing > 0
                      ? {
                          'data-unreadable': 'true',
                          'aria-label': `${c.label}, ${held.missing} of ${held.members} files missing`,
                        }
                      : {})}
                  >
                    {held.missing > 0 ? (
                      held.pages
                    ) : mayDownload ? (
                      <a
                        className="get"
                        href={editionDownload(w.slug, c.key)}
                        download
                        // **The count is in the label, not only in the
                        // content.** `aria-label` *replaces* an element's
                        // content for its accessible name, and this cell holds
                        // nothing but the anchor — so labelling it "download …"
                        // alone would stop a screen reader announcing the page
                        // count at all, which is the one fact the cell exists
                        // to carry.
                        aria-label={`download ${w.title} — ${c.label}, ${held.pages} pages`}
                      >
                        {held.pages}
                      </a>
                    ) : (
                      held.pages
                    )}
                  </td>
                ) : (
                  <td
                    key={c.key}
                    className="missing"
                    aria-label={`no ${c.label} edition`}
                  >
                    —
                  </td>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/**
 * The cover, and the alignment mark on it — the part of a grid tile shared by
 * a readable volume (inside a `<Link>`) and an unreadable one (inside a plain
 * `<span>`, below). Factored out rather than written twice: a copy in each
 * branch would drift. With no `shown`, the volume lacks the chosen edition and
 * is drawn dimmed, over the cover of the edition it leads with.
 */
function VolumeCover({
  w,
  shown,
}: {
  w: WorkCard
  shown: EditionCard | undefined
}) {
  return (
    <span className="cover-box">
      <Cover src={(shown ?? w.editions[0])?.cover ?? null} title={w.title} />
      {w.aligned && (
        <span className="aligned" title="has an aligned edition">
          ⇄
        </span>
      )}
    </span>
  )
}

export function Series() {
  const { layout, editionSelector, panel, alwaysShowAligned } = useBrowsePrefs()
  // `undefined` where no gate sits above this screen, and `gate.ts`'s own
  // rule is that the safe reading of that is "no account known" rather than a
  // permissive one — so the control appears only on an explicit `true`.
  //
  // **Absent rather than disabled** (spec C2 §8): a disabled button invites a
  // reader to find out why, and there is nothing on this screen that can tell
  // them.
  const mayDownload = useGate()?.who.can_download === true
  const { id } = useParams()
  // **Parsed before it is read.** `Number('not-a-number')` is `NaN`, and
  // `/api/library/series/NaN` is a request the server answers with a 400
  // that says nothing useful to the person who mistyped a URL.
  //
  // **Digits alone are not enough.** `/^\d+$/` accepts a string of any
  // length, but `Number` on one longer than `Number.MAX_SAFE_INTEGER`'s 16
  // digits either rounds to a *different*, wrong-but-plausible integer
  // (`Number('9007199254740993') === 9007199254740992`) or, further out,
  // loses integer shape entirely (`Number('9'.repeat(26)) === 1e+26`) — the
  // latter would even reach `seriesRead` and produce a request to
  // `/api/library/series/1e+26`. Real ids are small auto-increment integers,
  // so this only guards against an id no server could have issued; the
  // `Number.isSafeInteger` check turns that into the same honest "is not a
  // series" path a non-numeric `id` already takes, rather than a silently
  // wrong lookup.
  const seriesId =
    id !== undefined && /^\d+$/.test(id) && Number.isSafeInteger(Number(id))
      ? Number(id)
      : undefined
  const [params, setParams] = useSearchParams()
  // **The library the page was opened from** (spec §3.6): its tile counted
  // what that library holds, so the page lists that. A `?library=` that is no
  // library id names a library that does not exist, which answers as a series
  // that does not exist, so nothing is asked.
  const library = libraryParam(params.get('library'))
  const body =
    seriesId === undefined || library === 'invalid'
      ? undefined
      : seriesRead(seriesId, library)

  // **Only the series the address names is drawn**, and only once its read
  // is in hand: `usePage` answers for the address it is given, so a move to
  // another series draws nothing of the one left, neither its volumes nor its
  // failure, while a move to one kept in this tab draws it at once.
  const {
    ready,
    answers: [answer],
    failures: [failed],
    loadingDrawn,
  } = usePage<[Loaded]>(body === undefined ? [] : [body])
  const loaded = ready ? answer : undefined
  useDocumentTitle(loaded?.series.title)
  useKeptScroll(ready)
  const back = useBack()
  const failure =
    body === undefined
      ? `${id ?? ''} is not a series`
      : failed === undefined
        ? undefined
        : messageOf(failed)
  /**
   * Writes a chosen edition into the address with `replace`, as the Library
   * view writes its chip: a choice adds no entry for Back to walk out of.
   * Every other parameter stays; "all" is left out.
   */
  const choose = (key: string) => {
    const next = new URLSearchParams(params)
    if (key === 'all') next.delete('edition')
    else next.set('edition', key)
    setParams(next, { replace: true })
  }

  // The open panel lies over the rows below its tile (O283), laid from the
  // tile's width and place and the gap its grid lays the columns apart by,
  // and laid again when either's width changes. The tile's width follows
  // the cover size, and in the phone layout the page area's too, as a filled
  // column; beside the sidebar it is the size, so only the grid, which fills
  // the area (`main` has no `max-width`), follows a window or sidebar that
  // changes the area, and the gap and the tile's column follow the grid.
  const follow = (panelBox: HTMLElement, tile: HTMLElement) => {
    layPanel(panelBox, tile)
    // jsdom has no observer and lays nothing out to follow.
    if (typeof ResizeObserver === 'undefined') return
    const resized = new ResizeObserver(() => {
      layPanel(panelBox, tile)
    })
    resized.observe(tile)
    if (tile.parentElement !== null) resized.observe(tile.parentElement)
    return () => {
      resized.disconnect()
    }
  }

  /**
   * Where an Escape's close sends focus back to in `tile`: its `i`, or in
   * `on-select` mode, which has none, its link, its corner ↓, or the box an
   * unreadable volume is drawn in, which takes focus without being a Tab
   * stop.
   */
  const backTo = (tile: HTMLElement) =>
    panel === 'button'
      ? tile.querySelector<HTMLElement>(':scope > .tile-body > .panel-open')
      : (tile.querySelector<HTMLElement>(':scope > .tile-body > a:not(.get)') ??
        tile.querySelector<HTMLElement>(':scope > .tile-body > a.get') ??
        tile.querySelector<HTMLElement>(
          ':scope > .tile-body > .tile-unreadable',
        ))

  const volumePanel = useVolumePanel({
    mode: panel,
    works: loaded?.works,
    follow,
    laidFor: layout,
    backTo,
  })

  // **Both attachments are grid-only, by decision rather than accident.** The
  // selector's job is "which edition's cover do I see", and the table shows
  // no covers — it shows every edition as its own column with page counts
  // and a dashed cell where a volume lacks one, so the table already answers
  // what the selector asks. §3's three presets are grid+selector+button,
  // table with *both* off, and grid+panel-on-select: the spec never pairs
  // the table with either attachment, so preset B turns them off
  // deliberately. Folded into one named condition rather than checked with
  // two scattered `&&`s at the render site below.
  //
  // The `i` button and its hover/focus reveal need no matching guard of
  // their own: both live only inside the grid `<ul>` further down, and
  // `layout === 'table'` renders `<Table>` instead of that markup entirely,
  // so neither can appear there regardless. (That the table has no *way* to
  // reach alignment detail at all is a deferred gap, tracked separately —
  // this comment does not claim otherwise.)
  const showEditionSelector = editionSelector && layout !== 'table'

  // The covers move to their new columns in the grid only: the table has
  // no columns of covers to move.
  const volumes = useRef<HTMLUListElement>(null)
  useReseat(volumes, loaded !== undefined && layout !== 'table')
  // **`'all'` rather than the first edition**, so the page with the selector
  // on renders identically to the page with it off until the reader chooses.
  // A default of "the first edition" would silently dim every volume the hub
  // does not cover, on a screen nobody had touched.
  //
  // **Not applied while the selector is hidden**: nothing on the page would
  // then say why volumes are dimmed or which edition's covers are drawn. The
  // address keeps `?edition=`, so switching the selector on brings it back.
  const chosen = showEditionSelector
    ? editionIn(params, loaded?.works ?? [])
    : 'all'

  return (
    <>
      {/* Drawn in every state, the first thing on the page, so it is where
          it was when the series lands and when it never does. A button: a
          step back is no address to link to. */}
      <button type="button" className="page-back" onClick={back.go}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M15 5l-7 7 7 7" />
        </svg>
        {back.word}
      </button>
      {/* O163: one h1 on this screen in every state. Until the series has
          loaded, and when it never does, there is no title to show, so the
          heading is hidden and says what the screen is. */}
      {!loaded && <h1 className="visually-hidden">Series</h1>}
      {failure !== undefined && <p role="alert">{failure}</p>}
      {!ready && <PageLoading drawn={loadingDrawn} />}
      {loaded && (
        <>
          <div className="page-head">
            <h1>{loaded.series.title}</h1>
            <ViewMenu keys={SERIES_VIEW} />
          </div>
          {showEditionSelector && (
            <label className="edition-selector">
              <span>Edition</span>
              <select
                value={chosen}
                onChange={(e) => {
                  choose(e.target.value)
                }}
              >
                <option value="all">All editions</option>
                {editionColumns(loaded.works).map((c) => (
                  <option key={c.key} value={c.key}>
                    {c.label}
                  </option>
                ))}
              </select>
            </label>
          )}
          {layout === 'table' ? (
            <Table works={loaded.works} mayDownload={mayDownload} />
          ) : (
            <ul
              ref={volumes}
              className="volumes"
              data-aligned={alwaysShowAligned ? 'always' : undefined}
            >
              {loaded.works.map((w) => {
                // `undefined` under `'all'` means "whatever this volume leads
                // with", which is `editions[0]` — the hub, by `derive.rs`'s
                // ordering. Under a chosen key it means the volume does not
                // have that edition, which is a fact to show rather than to
                // hide: a series page that drops volumes when you pick an
                // edition is answering "which volumes exist" differently
                // depending on a control that was supposed to be about
                // covers.
                const shown =
                  chosen === 'all'
                    ? w.editions[0]
                    : w.editions.find((e) => e.key === chosen)
                // One download per tile (O283): at the foot of the open panel
                // where the tile has `i`, and otherwise on the cover's corner
                // in `i`'s place, so a reader with the panel off keeps one.
                // An `on-select` panel does not repeat it.
                const download = mayDownload &&
                  shown &&
                  shown.missing === 0 && (
                    // Outside the tile's link wherever it is placed: a nested
                    // anchor is invalid HTML, and browsers reparent it.
                    // `shown.missing === 0` asks of the edition this tile
                    // shows, which can be the broken one of a readable volume,
                    // and files not on disk are not offered. `download` is
                    // bare: the server's `Content-Disposition` carries an
                    // ASCII `filename=` and an RFC 6266 `filename*=`, which a
                    // value here would override with a guess.
                    <a
                      className="get"
                      href={editionDownload(w.slug, shown.key)}
                      download
                      aria-label={`download ${w.title} — ${shown.label}`}
                    >
                      {panel === 'button' ? '↓ Download' : '↓'}
                    </a>
                  )
                // **`onFocusCapture`, plus `onMouseEnter`.** Hover and focus
                // both, because "select" with a mouse and "select" with a
                // keyboard are the same intent and only one of them is a click;
                // a click still navigates, since nothing here calls
                // `preventDefault`. Capture rather than bubble: React maps
                // either to the bubbling native `focusin`, so the two are
                // interchangeable here — this `<li>` sits on that event's path
                // regardless of phase. Capture is kept anyway because it is the
                // phase a later bubble-phase `stopPropagation` — added to some
                // interactive child the panel grows — cannot defeat; that is a
                // guard against a bug that does not exist yet, not a
                // description of one that does.
                return (
                  <li
                    key={w.slug}
                    ref={
                      volumePanel.open === w.slug ? volumePanel.tile : undefined
                    }
                    className={shown ? undefined : 'without'}
                    onFocusCapture={
                      panel === 'on-select'
                        ? () => {
                            volumePanel.focused(w.slug)
                          }
                        : undefined
                    }
                    onMouseEnter={
                      panel === 'on-select'
                        ? () => {
                            volumePanel.pointed(w.slug)
                          }
                        : undefined
                    }
                  >
                    {/* Everything the tile draws but its panel, as the one
                        element a glide moves (`reseat.ts`): the tile stays
                        where layout puts it, for the browser to hold the page
                        by. The open panel stays out of it, under the tile's
                        laid box and above every cover. */}
                    <div className="tile-body">
                      {unreadable(w) ? (
                        // **O62 — the link is the treatment, not a badge.** This
                        // is the grid, the *default* layout
                        // (`browseModule.defaults.layout` is `'grid'`), so the
                        // guard `Table` carries has to be applied here too. Same
                        // rule as `Table`'s cell: a work whose members are gone
                        // must not offer a link that fails at the first page, and
                        // the volume keeps its name — withholding that too would
                        // read as a library that has lost the work rather than
                        // one that cannot open it.
                        <span
                          className="tile-unreadable"
                          tabIndex={panel === 'on-select' ? -1 : undefined}
                        >
                          <VolumeCover w={w} shown={shown} />
                          <span className="title">{w.title}</span>{' '}
                          <span className="unreadable">
                            files missing — cannot be opened
                          </span>
                        </span>
                      ) : (
                        <Link to={readerPath(w)}>
                          <VolumeCover w={w} shown={shown} />
                          <span className="title">{w.title}</span>
                        </Link>
                      )}
                      {panel !== 'button' && download}
                      {panel === 'button' && (
                        <button
                          type="button"
                          className="panel-open"
                          aria-expanded={volumePanel.open === w.slug}
                          aria-label={`details for ${w.title}`}
                          onClick={(event) => {
                            volumePanel.toggle(w.slug, event.currentTarget)
                          }}
                        >
                          i
                        </button>
                      )}
                    </div>
                    {volumePanel.open === w.slug && (
                      // The drawn panel, placed absolutely by `library.css`
                      // under its tile and over the rows below, so nothing
                      // moves as it opens; `follow` above sets its left and
                      // width. In `button` mode ↓ Download is its foot.
                      <VolumePanel
                        state={volumePanel}
                        work={w}
                        foot={panel === 'button' && download}
                      />
                    )}
                  </li>
                )
              })}
            </ul>
          )}
        </>
      )}
    </>
  )
}
