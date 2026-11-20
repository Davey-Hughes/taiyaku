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
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Link, useParams, useSearchParams } from 'react-router'
import { MetadataEditor, type TitleFlush } from './MetadataEditor'
import { type PatchResult, patchWork, type WorkEdit } from './metadata'
import { type BrowsePrefs, useBrowsePrefs } from './browseModule'
import { useGate } from './gate'
import { Cover } from './Cover'
import { HeldButton } from './HeldButton'
import {
  type AlignmentCard,
  type ContentMatch,
  type EditionCard,
  type KeptApart,
  libraryParam,
  readerPath,
  type SeriesCard,
  unreadable,
  work as fetchWork,
  type WorkCard,
  type WorkDetail,
  editionDownload,
} from './library'
import { splitMatch, undoSplit } from './matches'
import { messageOf, sentenceOf } from './message'
import { landFocus } from './landFocus'
import { invalidateLibraryViews, seriesRead } from './pageReads'
import { useReseat } from './reseat'
import { useKeptScroll } from './scrollKeeper'
import { Refusal } from './Refusal'
import { PageLoading } from './settingsLayout'
import { layPanel } from './tilePanel'
import { useDocumentTitle } from './title'
import { usePage } from './usePage'
import { ViewMenu } from './ViewMenu'

/** What this page's View panel offers: the preferences that act here. */
const SERIES_VIEW: readonly (keyof BrowsePrefs)[] = [
  'layout',
  'editionSelector',
  'panel',
  'coverSize',
]

interface Loaded {
  series: SeriesCard
  works: WorkCard[]
}

/**
 * What a volume's panel draws from `/api/library/works/{slug}`, or why it
 * could not be read. `editions` are the work's own, which carry the content
 * match marks an administrator is sent; the series' cards carry none.
 */
type Detail =
  | {
      alignments: AlignmentCard[]
      editions: EditionCard[]
      kept_apart: KeptApart[]
    }
  | { error: string }

/** The parts of work `w` its panel draws. */
function detailOf(w: WorkDetail): Detail {
  return {
    alignments: w.alignments,
    editions: w.editions,
    kept_apart: w.kept_apart ?? [],
  }
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
 * `patchWork`, then every Library view address marked stale: this page's
 * series is read again at once, so the volume's title follows the save, and
 * the grid, home and search kept for a later arrival are read then. Only once
 * the save has answered, and succeeded: a read sent before would answer with
 * the title from before it.
 */
async function saveWork(slug: string, fields: WorkEdit): Promise<PatchResult> {
  const saved = await patchWork(slug, fields)
  invalidateLibraryViews()
  return saved
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
 * One volume's alignment detail (§3's panel), read from the work alone, since
 * a verdict per row of a list would cost a read per row. A pair with no
 * verdict says "not aligned": a blank would say "not loaded". With no
 * alignment, `editionCount` (the tile's own) tells one edition, nothing to
 * pair, from two or more not aligned yet, a normal state: a derive aligns only
 * editions whose files the match pass scored at or above its bar, whether
 * joined by content or by name, and `taiyaku align` aligns others.
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
            : 'No alignment recorded.'}
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

/** A volume number as the series screens write it: `v05`, `v05.5`. */
function volumeTag(volume: number): string {
  const text = String(volume)
  const dot = text.indexOf('.')
  const whole = dot < 0 ? text : text.slice(0, dot)
  return `v${whole.padStart(2, '0')}${dot < 0 ? '' : text.slice(dot)}`
}

/** A marked edition: one whose own file a pair joined to this work. */
type Marked = EditionCard & { matched: ContentMatch }

/** One Split as the panel draws it: its pairs, and each volume it keeps this one apart from, once. */
interface KeptSplit {
  split: number
  pairs: number[]
  slugs: string[]
  from: string[]
}

/**
 * `kept` by the Split that set each pair apart, in the order drawn: one line
 * and one Undo per Split, as Undo restores a whole Split.
 */
function bySplit(kept: KeptApart[]): KeptSplit[] {
  const splits: KeptSplit[] = []
  for (const k of kept) {
    let one = splits.find((s) => s.split === k.split)
    if (one === undefined) {
      one = { split: k.split, pairs: [], slugs: [], from: [] }
      splits.push(one)
    }
    one.pairs.push(k.id)
    if (!one.slugs.includes(k.slug)) {
      one.slugs.push(k.slug)
      one.from.push(
        k.volume === null ? k.series : `${k.series} ${volumeTag(k.volume)}`,
      )
    }
  }
  return splits
}

/** `names` as a sentence lists them: "a", "a and b", "a, b and c". */
function listed(names: string[]): string {
  if (names.length < 2) return names.join('')
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/**
 * Where focus goes once a press's answer has been read again: the button
 * named `name` of the first of `pairs` drawn, else the panel.
 */
interface Landing {
  name: 'split' | 'undo-split'
  pairs: number[]
}

/**
 * A volume's content matches, an administrator's (spec §6): each edition
 * joined by content with its score and Split, and each Split that kept the
 * volume apart with one Undo. Split asks first, since it moves an edition off
 * the volume at once; Undo only lets the derive join every pair of that Split
 * again. Each press derives the whole store, so `onAnswer` reads the series
 * and the volume again. One press out at a time, held by a ref too: the hold
 * is drawn only after a render, and two presses inside one would send twice.
 */
function MatchMarks({
  slug,
  editions,
  kept,
  onAnswer,
  focusPanel,
}: {
  slug: string
  editions: EditionCard[]
  kept: KeptApart[]
  onAnswer: (to: Landing) => Promise<void>
  focusPanel: () => void
}) {
  /** The key of the edition whose Split is asking to be confirmed. */
  const [asking, setAsking] = useState<string | undefined>()
  /** The press out: `split:<key>` or `undo:<pair>`. */
  const [out, setOut] = useState<string | undefined>()
  const sending = useRef(false)
  /** Why the last press was refused. */
  const [failure, setFailure] = useState<string | undefined>()
  const root = useRef<HTMLDivElement>(null)
  /** Set as Cancel closes a question: that edition's Split… takes focus back. */
  const backTo = useRef<string | undefined>(undefined)
  useEffect(() => {
    const key = backTo.current
    if (asking !== undefined || key === undefined) return
    backTo.current = undefined
    const splits = root.current?.querySelectorAll<HTMLButtonElement>(
      'button[name="split"]',
    )
    ;[...(splits ?? [])].find((b) => b.dataset.key === key)?.focus()
  }, [asking])
  const marked = editions.filter((e): e is Marked => e.matched != null)
  if (marked.length === 0 && kept.length === 0) return null

  const send = (what: string, press: () => Promise<Landing>) => {
    if (sending.current) return
    sending.current = true
    setOut(what)
    setFailure(undefined)
    press()
      .then(
        (to) => {
          // The question goes as the Split answers, and with it the button
          // that had focus, which the panel holds until the read lands.
          const question = root.current?.querySelector('.split-confirm')
          if (question?.contains(document.activeElement)) focusPanel()
          setAsking(undefined)
          return onAnswer(to)
        },
        (err: unknown) => {
          setFailure(sentenceOf(err))
        },
      )
      .finally(() => {
        sending.current = false
        setOut(undefined)
      })
  }

  return (
    <div className="match-marks" ref={root}>
      <ul>
        {marked.map((e) => (
          <li key={e.key}>
            <div className="mark">
              <p>
                <span className="pair">{e.key}</span> matched by content ·{' '}
                {e.matched.score.toFixed(2)}
              </p>
              <HeldButton
                type="button"
                name="split"
                data-key={e.key}
                data-pair={e.matched.id}
                held={out !== undefined || asking === e.key}
                onClick={() => {
                  setFailure(undefined)
                  setAsking(e.key)
                }}
              >
                Split…
              </HeldButton>
            </div>
            {asking === e.key && (
              <div
                className="split-confirm"
                role="group"
                aria-label={`Split ${e.key} off this volume`}
              >
                <p>
                  Split {e.key} off this volume? Its file goes back to where its
                  own name puts it, unless it matches another volume.
                </p>
                <div className="split-actions">
                  <HeldButton
                    type="button"
                    className="confirm"
                    name="confirm-split"
                    held={out !== undefined}
                    onClick={() => {
                      send(`split:${e.key}`, () =>
                        splitMatch(e.matched.id, slug, e.key).then(
                          (answer): Landing => ({
                            name: 'undo-split',
                            pairs: answer.split,
                          }),
                        ),
                      )
                    }}
                  >
                    {out === `split:${e.key}` ? 'Splitting…' : `Split ${e.key}`}
                  </HeldButton>
                  <HeldButton
                    type="button"
                    name="cancel-split"
                    held={out !== undefined}
                    onClick={() => {
                      backTo.current = e.key
                      setAsking(undefined)
                    }}
                  >
                    Cancel
                  </HeldButton>
                </div>
              </div>
            )}
          </li>
        ))}
        {bySplit(kept).map((s) => (
          <li key={`split-${s.split}`}>
            <div className="mark">
              <p>kept apart from {listed(s.from)}</p>
              <HeldButton
                type="button"
                name="undo-split"
                data-pair={s.pairs[0]}
                held={out !== undefined}
                onClick={() => {
                  send(`undo:${s.split}`, () =>
                    undoSplit(s.pairs[0]).then(
                      (): Landing => ({ name: 'split', pairs: s.pairs }),
                    ),
                  )
                }}
              >
                {out === `undo:${s.split}` ? 'Undoing…' : 'Undo'}
              </HeldButton>
            </div>
          </li>
        ))}
      </ul>
      {failure !== undefined && <Refusal>{failure}</Refusal>}
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

  const [open, setOpen] = useState<string | undefined>()
  // Keyed by slug and kept for the life of the page. **The cache is what makes
  // `on-select` affordable**: the panel then follows the pointer and the focus
  // ring, and without this that is one request per volume per hover. A failure
  // is cached too, as its own message — retrying on every re-render would turn
  // a 500 into a request loop.
  //
  // **Not reset when the series changes.** `open` and `detail` are keyed by
  // *slug*, and slugs are per-work, not per-series-position, so a leftover
  // `open` from series A never matches a `w.slug` in series B's list: the
  // panel simply renders as already closed, which is the state a fresh view
  // should start in anyway. Clearing `detail` on top of that would only throw
  // away a cache entry that stays valid — and the whole point of keying it by
  // slug was to survive exactly this kind of navigation.
  const [detail, setDetail] = useState<Record<string, Detail>>({})
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
  /**
   * The open volume's panel: its editor, its alignment detail and, in
   * `button` mode, ↓ Download.
   */
  const openPanel = useRef<HTMLDivElement>(null)
  /** The open volume's tile, which its panel is laid from. */
  const openTile = useRef<HTMLLIElement>(null)
  /** The open panel's editor, whose `flush` every close waits on. */
  const editor = useRef<TitleFlush>(null)
  /** The newest `choosePanel` call; one waiting on a save is then dropped. */
  const choosing = useRef<symbol>(undefined)
  /**
   * The volume whose panel is open as of the last change `choosePanel`
   * made, which `open` holds only from the next render.
   */
  const opened = useRef<string>(undefined)
  /**
   * True only while a close sends focus back to its tile: in `on-select`
   * mode that focus would otherwise reveal the panel again.
   */
  const refocusing = useRef(false)

  // The open panel lies over the rows below its tile (O283), laid from the
  // tile's width and place and the gap its grid lays the columns apart by,
  // and laid again when either's width changes. The tile's width follows
  // the cover size, and in the phone layout the page area's too, as a filled
  // column; beside the sidebar it is the size, so only the grid, which fills
  // the area (`main` has no `max-width`), follows a window or sidebar that
  // changes the area, and the gap and the tile's column follow the grid.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `open` and `layout` are triggers; the body reads the refs.
  useLayoutEffect(() => {
    const panelBox = openPanel.current
    const tile = openTile.current
    if (panelBox === null || tile === null) return
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
  }, [open, layout])

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

  /**
   * Every change of which volume's panel is open: the open editor's `flush`
   * first, asked again after each `true` (text typed while a save was out),
   * then the change once it answers `null`; none on `false`, the failure or
   * the unwritten reason then shown in the panel still open. A call made
   * while one waits replaces it.
   * `after` runs once the change is made. The open volume asked for again is
   * no change, so its tile's focus and pointer send nothing.
   */
  const choosePanel = (next: string | undefined, after?: () => void): void => {
    const call = Symbol()
    choosing.current = call
    const ask = () => {
      const answer =
        next === opened.current ? null : (editor.current?.flush() ?? null)
      if (answer === null) {
        opened.current = next
        setOpen(next)
        after?.()
        return
      }
      void answer.then((stored) => {
        if (stored && choosing.current === call) ask()
      })
    }
    ask()
  }

  /**
   * A close's `after` that focuses `target`. Focus left in the panel would
   * be dropped with it, and the next Tab would start again at the top of
   * the page. Focus the person has moved elsewhere, while the close waited
   * on a save, stays where they put it.
   */
  const refocus = (target: HTMLElement | null) => {
    const from = document.activeElement
    return () => {
      const now = document.activeElement
      const dropped =
        now === document.body || openPanel.current?.contains(now) === true
      if (now !== from && !dropped) return
      refocusing.current = true
      try {
        target?.focus()
      } finally {
        refocusing.current = false
      }
    }
  }

  /**
   * An Escape's close sends focus back to `tile`: its `i`, or in
   * `on-select` mode, which has none, its link, its corner ↓, or the box an
   * unreadable volume is drawn in, which takes focus without being a Tab
   * stop.
   */
  const focusBack = (tile: HTMLElement) =>
    refocus(
      panel === 'button'
        ? tile.querySelector<HTMLElement>(':scope > .tile-body > .panel-open')
        : (tile.querySelector<HTMLElement>(
            ':scope > .tile-body > a:not(.get)',
          ) ??
            tile.querySelector<HTMLElement>(':scope > .tile-body > a.get') ??
            tile.querySelector<HTMLElement>(
              ':scope > .tile-body > .tile-unreadable',
            )),
    )

  // The open panel's closes, as `usePopover`'s are: an Escape, caught on
  // the way down and stopped so that it closes nothing below it, unless it
  // is the Title field's own or ends a composition; a press outside the
  // tile, though not on the scroller itself, whose scrollbar may be the way
  // to ↓ Download; and in `button` mode, focus arriving outside. In
  // `on-select` mode, focus on another tile moves the panel instead.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `choosePanel` and `focusBack` read only refs, `setOpen` and `panel`, so those from the render that opened the panel serve until it closes or the mode changes.
  useEffect(() => {
    if (open === undefined) return
    const inside = (target: EventTarget | null) =>
      target instanceof Node && (openTile.current?.contains(target) ?? false)
    const onKey = (event: KeyboardEvent) => {
      const tile = openTile.current
      if (event.key !== 'Escape' || tile === null) return
      if (event.isComposing || event.keyCode === 229) return
      const { target } = event
      if (
        target instanceof Element &&
        target.matches("textarea[name='title'][data-editing]")
      ) {
        return
      }
      event.stopPropagation()
      choosePanel(undefined, focusBack(tile))
    }
    const onPress = (event: Event) => {
      const { target } = event
      if (inside(target) || target === document.documentElement) return
      if (target instanceof Element && target.matches('.library > main')) {
        return
      }
      choosePanel(undefined)
    }
    const onFocus = (event: FocusEvent) => {
      if (!inside(event.target)) choosePanel(undefined)
    }
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('pointerdown', onPress)
    if (panel === 'button') document.addEventListener('focusin', onFocus)
    return () => {
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('pointerdown', onPress)
      document.removeEventListener('focusin', onFocus)
    }
  }, [open, panel])

  const reveal = (slug: string) => {
    choosePanel(slug)
    if (detail[slug] !== undefined || pending.current.has(slug)) return
    pending.current.add(slug)
    fetchWork(slug)
      .then((w) => {
        pending.current.delete(slug)
        setDetail((prev) => ({ ...prev, [slug]: detailOf(w) }))
      })
      .catch((err: unknown) => {
        pending.current.delete(slug)
        setDetail((prev) => ({
          ...prev,
          [slug]: { error: messageOf(err) },
        }))
      })
  }

  /**
   * After a Split or an Undo of volume `slug`'s panel has answered: the
   * series is read again, and the volume's detail with it, since the derive
   * the press ran moves editions between works. Every other volume's detail
   * is dropped, to be read again as its panel opens, since the derive can
   * have moved its marks too. Resolves once the volume's read has landed, so
   * the panel's press is held until it draws the new state.
   */
  const rereadAfterMatch = (slug: string, to: Landing): Promise<void> => {
    if (loaded !== undefined) pressedOn.current = { slug, from: loaded }
    invalidateLibraryViews()
    setDetail((prev) => {
      const kept = prev[slug]
      return kept === undefined ? {} : { [slug]: kept }
    })
    return fetchWork(slug)
      .then(detailOf, (err: unknown): Detail => ({ error: messageOf(err) }))
      .then((next) => {
        landing.current = { ...to, slug }
        setDetail((prev) => ({ ...prev, [slug]: next }))
      })
  }

  /**
   * Focus onto the open panel itself, where a press of its marks takes away
   * the control that had it.
   */
  const focusPanel = () => {
    if (openPanel.current !== null) landFocus(openPanel.current)
  }

  /**
   * The volume a press of its marks was made on, and the series as drawn
   * then. A derive can empty that volume and delete its work, so the series
   * read after the press draws no tile for it, and its panel goes with the
   * control that had focus: focus then goes to the page's `main`, as the
   * skip link's does.
   */
  const pressedOn = useRef<{ slug: string; from: Loaded } | undefined>(
    undefined,
  )
  useEffect(() => {
    const pressed = pressedOn.current
    if (pressed === undefined || loaded === undefined) return
    if (loaded === pressed.from) return
    pressedOn.current = undefined
    if (loaded.works.some((w) => w.slug === pressed.slug)) return
    if (document.activeElement !== document.body) return
    const main = document.querySelector<HTMLElement>('.library > main')
    if (main !== null) landFocus(main)
  }, [loaded])

  /** Where focus goes once `rereadAfterMatch`'s read is drawn. */
  const landing = useRef<(Landing & { slug: string }) | undefined>(undefined)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `detail` is the trigger; the body reads the refs.
  useEffect(() => {
    const to = landing.current
    if (to === undefined) return
    landing.current = undefined
    const box = openPanel.current
    if (box === null || opened.current !== to.slug) return
    // Focus the person has put elsewhere meanwhile stays where they put it.
    const now = document.activeElement
    if (now !== null && now !== document.body && !box.contains(now)) return
    const target = to.pairs
      .map((pair) =>
        box.querySelector<HTMLElement>(
          `button[name="${to.name}"][data-pair="${pair}"]`,
        ),
      )
      .find((button): button is HTMLElement => button !== null)
    if (target === undefined) focusPanel()
    else target.focus({ preventScroll: true })
  }, [detail])

  return (
    <>
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
            <ul ref={volumes} className="volumes">
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
                /** The open panel's detail, once it has been read. */
                const known = detail[w.slug]
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
                    ref={open === w.slug ? openTile : undefined}
                    className={shown ? undefined : 'without'}
                    onFocusCapture={
                      panel === 'on-select'
                        ? () => {
                            if (!refocusing.current) reveal(w.slug)
                          }
                        : undefined
                    }
                    onMouseEnter={
                      panel === 'on-select'
                        ? () => {
                            // Not while focus is in the open panel: a pointer
                            // passing over other tiles, the rows the panel
                            // lies over among them, would take the panel from
                            // the person typing in it. Nor while its field
                            // holds a title whose save failed, which the move
                            // would send again at every tile passed. Focus
                            // moving to this tile is a choice, and still
                            // reveals it.
                            if (
                              openPanel.current?.contains(
                                document.activeElement,
                              ) ||
                              editor.current?.failed() === true
                            ) {
                              return
                            }
                            reveal(w.slug)
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
                          aria-expanded={open === w.slug}
                          aria-label={`details for ${w.title}`}
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
                          onClick={(event) => {
                            if (open === w.slug) {
                              // A click need not focus the button, and focus
                              // in the panel would go with it.
                              choosePanel(
                                undefined,
                                refocus(event.currentTarget),
                              )
                            } else {
                              reveal(w.slug)
                            }
                          }}
                        >
                          i
                        </button>
                      )}
                    </div>
                    {open === w.slug && (
                      // The drawn panel, placed absolutely by `library.css`
                      // under its tile and over the rows below, so nothing
                      // moves as it opens; the effect above sets its left
                      // and width. One block, so that a pointer's reveal can
                      // ask whether focus is in it. The Title form and its
                      // stamp section, the alignment detail, ↓ Download.
                      <div ref={openPanel} className="tile-panel">
                        {mayEdit && (
                          // **In the panel, not beside the title** — C3 §5's
                          // "contextual, from the thing being corrected": the
                          // correction is prompted by looking at a volume,
                          // and this is the surface a volume already opens.
                          // There is no metadata screen to navigate to.
                          //
                          // A save reads the series again (`saveWork`), so
                          // the tile's title follows it; the field keeps the
                          // title as typed (O283).
                          <MetadataEditor
                            ref={editor}
                            slug={w.slug}
                            title={w.title}
                            onPatch={saveWork}
                            canStamp={mayStamp}
                          />
                        )}
                        {mayEdit ? (
                          // One section for how the volume's editions relate:
                          // the alignment lines, then the content match marks
                          // under them, set off by a rule above and below. Not
                          // one row per edition: an alignment is a stored pair
                          // of any two keys (`browse::alignments_of`), not each
                          // edition against the hub. A reader, sent no marks,
                          // keeps the bare alignment detail.
                          <section
                            className="edition-group"
                            aria-label="Editions"
                          >
                            <AlignmentPanel
                              editionCount={w.editions.length}
                              state={detail[w.slug]}
                            />
                            {known !== undefined && !('error' in known) && (
                              // Keyed by volume, so a press's confirmation or
                              // refusal never shows under another volume's
                              // marks.
                              <MatchMarks
                                key={w.slug}
                                slug={w.slug}
                                editions={known.editions}
                                kept={known.kept_apart}
                                onAnswer={(to) => rereadAfterMatch(w.slug, to)}
                                focusPanel={focusPanel}
                              />
                            )}
                          </section>
                        ) : (
                          <AlignmentPanel
                            editionCount={w.editions.length}
                            state={detail[w.slug]}
                          />
                        )}
                        {panel === 'button' && download}
                      </div>
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
