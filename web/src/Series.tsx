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
 * **The alignment mark is the whole point of the page.** Go-Toubun is 14
 * volumes in `Manga`, 14 in `Manga (RAW)` and a fan release, and only volumes
 * 1, 3 and 8 are aligned — a page that lists volumes and editions without
 * saying which are paired is a Komga screen with extra badges.
 * `WorkCard.aligned` is one bool per volume and no list route carries more;
 * the verdict and the paired count come from `/api/library/works/{slug}` and
 * from nowhere else, which is what the panel is for.
 */
import { useEffect, useRef, useState } from 'react'
import { Link, useParams } from 'react-router'
import { MetadataEditor } from './MetadataEditor'
import { patchWork } from './metadata'
import { useBrowsePrefs } from './browseModule'
import { useGate } from './gate'
import { Cover } from './Cover'
import {
  type AlignmentCard,
  type EditionCard,
  oneSeries as fetchSeries,
  type SeriesCard,
  unreadable,
  work as fetchWork,
  type WorkCard,
  editionDownload,
  workCover,
} from './library'

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
// `unreadable` — O62 — now lives in `library.ts`, not here: the whole-branch
// review's I1 found it applied only inside `Table` below, while `browseModule`
// makes the grid the *default* layout — so the grid rendered every reader link
// and every per-edition download unguarded. `Library.tsx`'s Continue strip and
// `SeriesGrid.tsx`'s unfiled works needed the identical guard, which is what
// made it a shared pure function rather than a fourth copy. See its doc
// comment in `library.ts` for the "every edition, not any" reasoning.

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
                <Link to={`/read/${w.slug}`}>{w.title}</Link>
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
                      // alone stopped a screen reader announcing the page
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
  )
}

/**
 * One volume's alignment detail — §3's panel.
 *
 * **`verdict` and `paired` come from `/api/library/works/{slug}` and from
 * nowhere else.** `WorkCard.aligned` is a bool, deliberately: an
 * `alignments_of` per row would put back the N+1 the whole-branch review
 * closed in `home`, at 111 queries on the largest series. That is what makes
 * the `button` default cheap rather than merely tidy.
 *
 * A pair with no verdict is **stated as unaligned**, not left blank. The two
 * are different facts — "these editions have no alignment" against "the panel
 * has not loaded" — and a blank cell says the second while meaning the first.
 *
 * **An empty `alignments` is two different facts, told apart by `editionCount`.**
 * `derive` never writes an alignment itself — `taiyaku align` is a separate
 * step a reader runs later — so a freshly scanned library has zero alignments
 * on every volume regardless of how many editions it holds. "One edition, so
 * nothing to pair" is true only when `editionCount < 2`; a volume with two or
 * more editions and no alignments has something to pair and simply has not
 * been aligned yet, which is the common case here and is worded as a normal
 * state rather than a problem. `editionCount` comes from the caller's own
 * `WorkCard.editions.length` rather than a second fetch — the tile already
 * holds it, and widening the per-slug `detail` cache to carry it too would
 * duplicate data this component does not need cached.
 */
function AlignmentPanel({
  editionCount,
  state,
}: {
  editionCount: number
  state: { alignments: AlignmentCard[] } | { error: string } | undefined
}) {
  if (state === undefined)
    return (
      <div className="alignment-panel">
        <p>Reading…</p>
      </div>
    )
  if ('error' in state)
    return (
      <div className="alignment-panel">
        <p role="alert">{state.error}</p>
      </div>
    )
  if (state.alignments.length === 0)
    return (
      <div className="alignment-panel">
        <p>
          {editionCount < 2
            ? 'This volume has one edition, so there is nothing to pair it with.'
            : 'No alignment has been recorded between this volume’s editions yet — run taiyaku align to add one.'}
        </p>
      </div>
    )
  return (
    <div className="alignment-panel">
      <ul>
        {state.alignments.map((a) => (
          <li key={`${a.from}→${a.to}`}>
            <span className="pair">
              {a.from} → {a.to}
            </span>{' '}
            {a.verdict === null ? (
              <span className="verdict">not aligned</span>
            ) : (
              <>
                <span className="verdict">{a.verdict}</span>
                {a.paired !== null && a.slots !== null && (
                  <span className="paired">
                    {' '}
                    · {a.paired} of {a.slots}
                  </span>
                )}
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}

/**
 * The cover, and the alignment mark on it — the part of a grid tile shared by
 * a readable volume (inside a `<Link>`) and an unreadable one (inside a plain
 * `<span>`, below). Factored out rather than written twice: a copy in each
 * branch is exactly the kind of drift I1 found in `unreadable` itself, one
 * level up.
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
      <Cover
        src={workCover(w.slug, (shown ?? w.editions[0])?.key ?? '')}
        title={w.title}
      />
      {w.aligned && (
        <span className="aligned" title="has an aligned edition">
          ⇄
        </span>
      )}
    </span>
  )
}

export function Series() {
  const { layout, editionSelector, panel } = useBrowsePrefs()
  // `undefined` where no gate sits above this screen, and `gate.ts`'s own
  // rule is that the safe reading of that is "no account known" rather than a
  // permissive one — so the control appears only on an explicit `true`.
  //
  // **Absent rather than disabled** (spec C2 §8): a disabled button invites a
  // reader to find out why, and there is nothing on this screen that can tell
  // them.
  const mayDownload = useGate()?.who.can_download === true
  // **Admin, because every metadata PATCH route is** (`routes.rs`'s grant
  // table). Rendering the affordance to a reader would invite a correction
  // and then answer 403, which is worse than not offering it.
  const mayEdit = useGate()?.who.admin === true
  // **The column alone — never `admin || can_stamp`.** Every other capability
  // on `who` reads `admin || column` (`mayDownload` above is one, and
  // `mayEdit` folds `admin` in the same way), because O86 makes role
  // dominate: an administrator is unrestricted in every dimension the schema
  // has. `can_stamp` is the one documented exception to that rule (`Me`'s own
  // comment in `api.ts`, and design doc §1.1): every capability O86 folds
  // widens what an account may *read*, and this one instead decides whether
  // it may *rewrite files taiyaku does not own* — a risk an admin role does
  // not, by itself, answer for. `admin || can_stamp` was rejected for the
  // same reason `mayDownload` offers nothing rather than a disabled control:
  // it would draw this section for an administrator who does not hold the
  // column, and the server answers that write with 403.
  //
  // `mayEdit` above already requires `admin` to show the editor at all, so
  // the section this gates needs both — admin AND can_stamp — which is
  // narrower than either alone. That is intended, not an oversight to
  // "simplify" by dropping one half.
  const mayStamp = useGate()?.who.can_stamp === true
  const { id } = useParams()
  // **Parsed before the effect, not inside it.** `Number('not-a-number')` is
  // `NaN`, and `/api/library/series/NaN` is a request the server answers with
  // a 400 that says nothing useful to the person who mistyped a URL.
  //
  // **Digits alone are not enough.** `/^\d+$/` accepts a string of any
  // length, but `Number` on one longer than `Number.MAX_SAFE_INTEGER`'s 16
  // digits either rounds to a *different*, wrong-but-plausible integer
  // (`Number('9007199254740993') === 9007199254740992`) or, further out,
  // loses integer shape entirely (`Number('9'.repeat(26)) === 1e+26`) — the
  // latter would even reach `fetchSeries` and produce a request to
  // `/api/library/series/1e+26`. Real ids are small auto-increment integers,
  // so this only guards against an id no server could have issued; the
  // `Number.isSafeInteger` check turns that into the same honest "is not a
  // series" path a non-numeric `id` already takes, rather than a silently
  // wrong lookup.
  const seriesId =
    id !== undefined && /^\d+$/.test(id) && Number.isSafeInteger(Number(id))
      ? Number(id)
      : undefined

  const [loaded, setLoaded] = useState<Loaded | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  // **`'all'` rather than the first edition**, so the page with the selector
  // on renders identically to the page with it off until the reader chooses.
  // A default of "the first edition" would silently dim every volume the hub
  // does not cover, on a screen nobody had touched.
  const [chosen, setChosen] = useState('all')

  const [open, setOpen] = useState<string | undefined>()
  // Keyed by slug and kept for the life of the page. **The cache is what makes
  // `on-select` affordable**: the panel then follows the pointer and the focus
  // ring, and without this that is one request per volume per hover. A failure
  // is cached too, as its own message — retrying on every re-render would turn
  // a 500 into a request loop.
  //
  // **Not reset when the series changes**, unlike `chosen` above. `chosen`
  // holds an *edition key*, and edition keys collide across series — `'en'`
  // exists in most of them — so a leftover `chosen` from series A can silently
  // select a real edition in series B. `open` and `detail` are keyed by
  // *slug*, and slugs are per-work, not per-series-position, so a leftover
  // `open` from series A never matches a `w.slug` in series B's list: the
  // panel simply renders as already closed, which is the state a fresh view
  // should start in anyway. Clearing `detail` on top of that would only throw
  // away a cache entry that stays valid — and the whole point of keying it by
  // slug was to survive exactly this kind of navigation.
  const [detail, setDetail] = useState<
    Record<string, { alignments: AlignmentCard[] } | { error: string }>
  >({})
  // `detail[slug] !== undefined` only tells us a request has *settled* — a
  // second `reveal()` for the same slug while the first is still in flight
  // would sail past it and fire again. That is not hypothetical: in
  // `on-select` mode a tile carries both `onMouseEnter` and
  // `onFocusCapture`, and a real click moves focus to the link before the
  // click fires, so hovering a tile and then clicking it routes through
  // both handlers before the first request can have returned. A `Set` of
  // slugs currently in flight, marked *before* the fetch is issued, closes
  // that gap; a `ref` rather than `useState` so marking it is synchronous
  // and never itself schedules a render. It must not double as a loading
  // sentinel in `detail` — the "Reading…" state is `detail[slug] ===
  // undefined`, and seeding that with a placeholder would change what the
  // panel shows while the real request is still outstanding.
  const pending = useRef<Set<string>>(new Set())

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

  const reveal = (slug: string) => {
    setOpen(slug)
    if (detail[slug] !== undefined || pending.current.has(slug)) return
    pending.current.add(slug)
    fetchWork(slug)
      .then((w) => {
        pending.current.delete(slug)
        setDetail((prev) => ({ ...prev, [slug]: { alignments: w.alignments } }))
      })
      .catch((err: unknown) => {
        pending.current.delete(slug)
        setDetail((prev) => ({
          ...prev,
          [slug]: { error: err instanceof Error ? err.message : String(err) },
        }))
      })
  }

  useEffect(() => {
    // A changing series must also forget which edition the reader had
    // chosen for the *previous* one, or a selector built from series A's
    // edition list lingers into series B's. Unreachable today — the only
    // link to `/series/:id` comes from a different route, so `Series`
    // always remounts — but it is the same class of bug `Routes.tsx`'s
    // `key={work}` already guards against elsewhere, and Task 10 adds a
    // search screen whose results link here without a remount in between.
    setChosen('all')
    if (seriesId === undefined) {
      setFailure(`${id ?? ''} is not a series`)
      return
    }
    let cancelled = false
    fetchSeries(seriesId)
      .then((next) => {
        if (cancelled) return
        setLoaded(next)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [seriesId, id])

  return (
    <>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {loaded && (
        <>
          <h1>{loaded.series.title}</h1>
          {showEditionSelector && (
            <label className="edition-selector">
              <span>Edition</span>
              <select
                value={chosen}
                onChange={(e) => {
                  setChosen(e.target.value)
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
            <ul className="volumes">
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
                // **`onFocusCapture`, plus `onMouseEnter`.** Hover and focus
                // both, because "select" with a mouse and "select" with a
                // keyboard are the same intent and only one of them is a
                // click; a click still navigates, since nothing here calls
                // `preventDefault`. Capture rather than bubble: React maps
                // either to the bubbling native `focusin`, so today the two
                // are interchangeable here — this `<li>` sits on that event's
                // path regardless of phase, verified by swapping one for the
                // other and re-running this file's tests. Capture is kept
                // anyway because it is the phase a later bubble-phase
                // `stopPropagation` — added to some interactive child the
                // panel grows — cannot defeat; that is a guard against a bug
                // that does not exist yet, not a description of one that does.
                return (
                  <li
                    key={w.slug}
                    className={shown ? undefined : 'without'}
                    onFocusCapture={
                      panel === 'on-select'
                        ? () => {
                            reveal(w.slug)
                          }
                        : undefined
                    }
                    onMouseEnter={
                      panel === 'on-select'
                        ? () => {
                            reveal(w.slug)
                          }
                        : undefined
                    }
                  >
                    {unreadable(w) ? (
                      // **O62 — the link is the treatment, not a badge.**
                      // I1 of the whole-branch review: this is the grid, the
                      // *default* layout (`browseModule.defaults.layout` is
                      // `'grid'`), so the guard `Table` already carries has
                      // to be applied here too, not only there. Same rule as
                      // `Table`'s cell: a work whose members are gone must
                      // not offer a link that fails at the first page, and
                      // the volume keeps its name — withholding that too
                      // would read as a library that has lost the work
                      // rather than one that cannot open it.
                      <span className="tile-unreadable">
                        <VolumeCover w={w} shown={shown} />
                        <span className="title">{w.title}</span>{' '}
                        <span className="unreadable">
                          files missing — cannot be opened
                        </span>
                      </span>
                    ) : (
                      <Link to={`/read/${w.slug}`}>
                        <VolumeCover w={w} shown={shown} />
                        <span className="title">{w.title}</span>
                      </Link>
                    )}
                    {mayDownload && shown && shown.missing === 0 && (
                      // **A sibling of the tile above, not a child.** A
                      // nested anchor is invalid HTML and browsers reparent
                      // it; the `panel-open` button below is already a
                      // sibling for the same structural reason.
                      //
                      // `shown.missing === 0`, per edition rather than per
                      // work: `shown` is whichever edition this tile is
                      // currently displaying (the hub under `'all'`, or the
                      // chosen key), and that can be the broken edition of an
                      // otherwise-readable volume — a sibling edition keeps
                      // the reader link above, but downloading files that
                      // are not on disk is not an offer this control should
                      // make either way.
                      //
                      // `download` is bare, with no value: the server sends
                      // `Content-Disposition` with an ASCII `filename=` and
                      // an RFC 6266 `filename*=`, and a value here would
                      // override that careful name with a guess.
                      <a
                        className="get"
                        href={editionDownload(w.slug, shown.key)}
                        download
                        aria-label={`download ${w.title} — ${shown.label}`}
                      >
                        ↓
                      </a>
                    )}
                    {panel === 'button' && (
                      <button
                        type="button"
                        className="panel-open"
                        aria-expanded={open === w.slug}
                        aria-label={`alignment detail for ${w.title}`}
                        // **Toggle lives here, not inside `reveal`.** `reveal`
                        // is also `on-select`'s hover/focus handler, where
                        // pointing at a tile a second time must still show
                        // it — toggling there would close the panel on the
                        // second hover instead. The button is the one caller
                        // that means "open *or* close this one", so this is
                        // the only place that decides between them; `reveal`
                        // stays a plain "make this the open one" and the
                        // per-slug `detail` cache and `pending` guard are
                        // untouched either way, so closing loses nothing
                        // fetched and reopening never refetches.
                        onClick={() => {
                          if (open === w.slug) {
                            setOpen(undefined)
                          } else {
                            reveal(w.slug)
                          }
                        }}
                      >
                        i
                      </button>
                    )}
                    {open === w.slug && (
                      <AlignmentPanel
                        editionCount={w.editions.length}
                        state={detail[w.slug]}
                      />
                    )}
                    {open === w.slug && mayEdit && (
                      // **In the panel, not beside the title** — C3 §5's
                      // "contextual, from the thing being corrected": the
                      // correction is prompted by looking at a volume, and
                      // this is the surface a volume already opens. There is
                      // no metadata screen to navigate to, by design.
                      //
                      // The page's own `w.title` is not re-fetched after a
                      // save; the editor shows the server's re-projected
                      // title itself, so the corrected name is on screen
                      // either way. Refreshing the whole series behind it is
                      // a larger change than this task, and O67's own case —
                      // a title wrong about its own contents — is answered
                      // by the edit landing, not by the list re-sorting.
                      <MetadataEditor
                        slug={w.slug}
                        title={w.title}
                        onPatch={patchWork}
                        canStamp={mayStamp}
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
