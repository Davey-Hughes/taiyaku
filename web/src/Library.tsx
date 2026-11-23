/**
 * The library screen: the search bar, the continue strip, the filter chips,
 * and the grid of series and unfiled works, which a search's hits replace.
 *
 * **The sidebar is `Shell`'s and the tiles are `SeriesGrid`'s.** The sidebar
 * and the `/api/library/status` fetch are `Shell`'s, so every screen under it
 * shares them, and the tiles are `SeriesGrid`'s, so the grid and the search
 * hits cannot disagree about them. What is here is the part that is only the
 * library's — which chips exist, what they filter, and which empty state an
 * empty grid is.
 *
 * Spec C1 §2 and §11 step 6. **No virtualisation** — §7's numbers do not call
 * for it, and covers carry `loading="lazy"`, which is free. The figure that
 * would change that is a chapter-organised edition, and it should be produced
 * rather than imagined.
 *
 * The chips filter and sort **client-side**, over data already held: §7's
 * numbers make a round trip to the server a request for something the browser
 * does instantly. They are in the address all the same, as `?show=` and
 * `?sort=` (spec `2026-10-28-loading-library-design.md` §5.1), so that Back and
 * the sidebar's library switch bring them back; the server is never asked for
 * them. `?library=` is sent to the server for the opposite reason — it changes
 * which rows exist, and which rows exist is the grant's business. It narrows
 * the grid and the continue strip alike (spec F §3).
 *
 * **The grid and the home lists are read through the store** (`usePage`), as
 * one set, so a return draws what was kept in its first render and reads it
 * again, and a first visit draws both in one step.
 */
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  Link,
  useLocation,
  useNavigationType,
  useSearchParams,
} from 'react-router'
import { type BrowsePrefs, coverSizeOf, useBrowsePrefs } from './browseModule'
import { useContinueRow } from './continueRow'
import { Cover } from './Cover'
import { type EmptyKind, EmptyState } from './EmptyState'
import { tabbablesIn } from './focus'
import { useGate } from './gate'
import {
  type Chip,
  editionDownload,
  type Grid,
  type HomeLists,
  type LibraryStatus,
  libraryParam,
  matchesChip,
  readerPath,
  sorted,
  unreadable,
  type View,
  viewIn,
  type WorkCard,
  withView,
} from './library'
import { useDrawnWait } from './hooks'
import { gridRead, homeRead, LIBRARY_STATUS } from './pageReads'
import { invalidate } from './reads'
import {
  askScanFeed,
  jobsHolding,
  libraryStep,
  type ScanFeed,
  stepLine,
  stepWord,
  useScanFeed,
} from './scanFeed'
import { SeriesGrid } from './SeriesGrid'
import { PageLoading } from './settingsLayout'
import { useLibraryStatus, useStatusFailed } from './shell'
import { useDocumentTitle } from './title'
import { messageOf } from './message'
import { ViewMenu } from './ViewMenu'
import { SearchBar } from './SearchBar'
import { useKeptScroll } from './scrollKeeper'
import { boxStateOf, SEARCH_BOX_ID, useSearchHits } from './searchHits'
import { inRowView, layUnderRow } from './tilePanel'
import { usePage } from './usePage'
import { useVolumePanel } from './volumePanel'
import { VolumePanel } from './VolumePanel'

/** What this page's View panel offers: the preferences that act here. */
const LIBRARY_VIEW: readonly (keyof BrowsePrefs)[] = [
  'continueStrip',
  'panel',
  'coverSize',
]

const CHIPS: { id: Chip; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'multi', label: 'Multi-edition' },
  { id: 'unread', label: 'Unread' },
]

/**
 * One card in the continue strip — spec 2026-09-17 §7.2. The cover is the
 * thumbnail of the edition the reader will open for this account (`opens`,
 * O84), not the series cover, because the strip is about a volume. What the
 * card cannot show is where the reader is in it: `progress` holds a slot, and
 * the home route sends no page — O132.
 */
function ContinueCard({ w }: { w: WorkCard }) {
  return (
    <>
      <Cover
        src={w.editions.find((e) => e.key === w.opens)?.cover ?? null}
        title={w.title}
      />
      <span className="title">{w.title}</span>
      {w.volume !== null && <span className="meta">Vol. {w.volume}</span>}
    </>
  )
}

/**
 * The first stop of the first element after `card` (a Continue card, or the
 * row's section) that has one: an unreadable card has none.
 */
function firstAfter(card: Element): HTMLElement | undefined {
  let next = card.nextElementSibling
  while (next !== null) {
    const first = tabbablesIn(next)[0]
    if (first !== undefined) return first
    next = next.nextElementSibling
  }
  return undefined
}

/**
 * Which first-run screen an empty grid is (spec C1 §8). A library's page asks
 * of its own library, and the page of every library of them all: one library
 * read and another not is "nothing to read yet" on the first and "not read
 * yet" on the second. With no library listed, `restricted` says whose doing
 * that is: an account granted none, or a server with none. A library's page
 * asks it too: an account granted none is told so there as well, rather than
 * that this one library is not to be seen.
 */
function emptyKind(
  status: LibraryStatus,
  library: number | undefined,
  held: boolean,
  filtered: boolean,
): EmptyKind {
  const entry =
    library === undefined
      ? undefined
      : status.libraries.find((l) => l.id === library)
  if (library !== undefined && entry === undefined)
    return status.libraries.length === 0 && status.restricted
      ? 'no-grant'
      : 'no-library'
  if (library === undefined && status.libraries.length === 0)
    return status.restricted ? 'no-grant' : 'no-libraries'
  // A filter or a search hides works, so under one neither `scanned` value
  // says whether the library has any (a first scan cancelled between works
  // leaves some with `scanned` false), and a scan holding it is a line
  // beside no match.
  if (filtered) return 'no-matches'
  // `held`: a scan holds what this asks of, which is neither unread nor
  // empty while it runs (O311).
  if (held) return 'scanning'
  const read =
    entry === undefined
      ? status.libraries.some((l) => l.scanned)
      : entry.scanned
  // Unfiltered, an empty grid over a read library is a library with nothing
  // to draw, not a filter that matched nothing (O520).
  return read ? 'no-works' : 'not-scanned'
}

/**
 * The step of a scan holding one of `libraries`, `undefined` while none
 * does: with its count for an administrator, whom the feed sends its jobs,
 * and the word alone for anyone else, who is sent no count.
 */
function scanLine(
  feed: ScanFeed | undefined,
  libraries: readonly number[],
): string | undefined {
  for (const id of libraries) {
    const step = libraryStep(feed, id)
    if (step === undefined) continue
    const job = jobsHolding(feed, id)[0]
    return job === undefined ? stepWord(step) : stepLine(job)
  }
  return undefined
}

/**
 * An empty view's state, and a search's that found nothing, with the scan
 * holding its library. Only this reads the feed, so a tab polls it only
 * while the view is empty: a view drawing works has no scan to show, and a
 * reader's tab left on one would otherwise poll for as long as it is open.
 * A search can find nothing before the status lands, and is then no match.
 */
function EmptyView({
  status,
  library,
  filtered,
  query,
}: {
  status: LibraryStatus | undefined
  library: number | undefined
  /** A chip or a search hides works the view would otherwise draw. */
  filtered: boolean
  query?: string
}) {
  const feed = useScanFeed()
  // An answer kept from before the view was drawn was read for a page that
  // has gone, at any age: no feed page beside this one keeps it fresh. So
  // only an answer since then says whether a scan holds the library.
  const [kept] = useState(feed)
  const since = feed === kept ? undefined : feed
  /**
   * The libraries the feed lists and the status did not, each of which has had
   * the status read again (O322): the shell reads it as it mounts, so a view
   * drawn before a library was made or granted says there is none while a
   * scan holds it. Once per library while the view is drawn: a library the
   * feed and the status disagree about, such as one whose grant changed
   * between the two reads, would otherwise have the status read every poll.
   */
  const readAgainFor = useRef(new Set<number>())
  useEffect(() => {
    if (since === undefined || status === undefined) return
    const unlisted = since.libraries.filter(
      ({ id }) =>
        !readAgainFor.current.has(id) &&
        !status.libraries.some((listed) => listed.id === id),
    )
    if (unlisted.length === 0) return
    for (const { id } of unlisted) readAgainFor.current.add(id)
    void invalidate(LIBRARY_STATUS)
  }, [since, status])
  const [answered, setAnswered] = useState(false)
  useEffect(() => {
    let drawn = true
    // Settles on a failure too, so a feed that cannot be read never holds
    // the view: it then draws the status's state.
    const ask = () => {
      void askScanFeed().then(() => {
        if (drawn) setAnswered(true)
      })
    }
    // A hidden tab asks nothing, so its ask settles at once with no answer,
    // and the status's state would stand for a scan until the tab is shown:
    // the view waits, and asks then.
    const onShown = () => {
      if (document.visibilityState === 'hidden') return
      document.removeEventListener('visibilitychange', onShown)
      ask()
    }
    if (document.visibilityState === 'hidden')
      document.addEventListener('visibilitychange', onShown)
    else ask()
    return () => {
      drawn = false
      document.removeEventListener('visibilitychange', onShown)
    }
  }, [])
  const scan = scanLine(
    since,
    library !== undefined
      ? [library]
      : (status?.libraries.map((l) => l.id) ?? []),
  )
  const kind =
    status === undefined
      ? 'no-matches'
      : emptyKind(status, library, scan !== undefined, filtered)
  // Only a state a scan would replace waits: drawn first, "Not read yet"
  // would stand for a library being scanned until the answer lands.
  const waiting =
    !answered &&
    status !== undefined &&
    emptyKind(status, library, true, filtered) === 'scanning'
  const waitDrawn = useDrawnWait(waiting)
  if (waiting) return <PageLoading drawn={waitDrawn} />
  return (
    <EmptyState
      kind={kind}
      query={query}
      step={scan}
      scanning={scan !== undefined}
    />
  )
}

export function Library() {
  const prefs = useBrowsePrefs()
  const [params, setParams] = useSearchParams()
  /** `'invalid'` is no library: the page says so, and asks the server nothing. */
  const library = libraryParam(params.get('library'))
  /** The library the search and the tiles carry, where the page names one. */
  const narrowed = typeof library === 'number' ? library : undefined
  const view = viewIn(params)
  const { chip, sort } = view

  const status = useLibraryStatus()
  /** The library this page is narrowed to, once the status names it. */
  const entry =
    typeof library === 'number'
      ? status?.libraries.find((l) => l.id === library)
      : undefined
  useDocumentTitle(entry?.name ?? 'Library')

  // **Only what was read for the library now asked for is drawn**, and only
  // once both its reads are in hand: `usePage` answers for the addresses it
  // is given, so a switch draws nothing of the library left, neither its
  // rows nor its failure, while a switch to a library kept in this tab draws
  // it at once. Where both reads fail the grid's failure is the one said,
  // since the grid is what the view is for.
  const {
    ready,
    answers: [gridAnswer, homeAnswer],
    failures: [gridFailure, homeFailure],
    loadingDrawn,
  } = usePage<[Grid, HomeLists]>(
    library === 'invalid' ? [] : [gridRead(library), homeRead(library)],
  )
  const gridData = ready ? gridAnswer : undefined
  const home = ready ? homeAnswer : undefined
  const failed = gridFailure ?? homeFailure
  const failure = failed === undefined ? undefined : messageOf(failed)

  // **The box follows what the grid shows.** Its scope is the library the
  // view shows (spec §2), so where the grid's empty state says there is
  // none — "No such library", "No library yet" on a server with none, or
  // "No library shared" for an account that sees none (O221) — there is
  // nothing to search, and that state says why. A value that is no id is
  // "No such library" before anything is asked; the rest is known once the
  // status lands.
  const unsearchable =
    library === 'invalid' ||
    (status !== undefined &&
      (library === undefined
        ? status.libraries.length === 0
        : entry === undefined))
  // Until the status lands the grid names no empty state and draws only the
  // rows it has, so the box comes with those rows or with the status: drawn
  // any earlier, it would be taken away again when the status names one of
  // the states above.
  const offered =
    !unsearchable &&
    (status !== undefined ||
      (gridData !== undefined &&
        (gridData.series.length > 0 || gridData.unfiled.length > 0)))

  // The search bar's query, scoped to the library this view shows. While it
  // holds anything the page is in search mode — the hits replace the grid,
  // and the tabs, the sort and the continue strip go, since the search
  // endpoint takes none of them — unless the view is known to have nothing
  // to search. The layout does not wait for the status, so a link's query
  // never draws the tabs and the grid first. The call does wait, until the
  // box is offered: a `?q=` the page arrives with (the redirect from
  // `/search` keeps one) sends nothing whose answer — "Nothing matched" — the
  // empty state above would then contradict. That holds for an arrival's
  // query too, which is read at once rather than after the typing delay.
  const q = unsearchable ? '' : (params.get('q') ?? '')
  const searching = q !== ''
  const location = useLocation()
  const arrival = useNavigationType()
  // The box writes `?q=` with `replace` and its tag (O223). Back or Forward
  // restores an entry with the tag it was left with, a push is never the
  // box's, and a replace without the tag is someone else's, as the redirect
  // from `/search` is: so only a replace carrying the tag is a key typed.
  const typed = arrival === 'REPLACE' && boxStateOf(location.state) !== null
  const {
    hits,
    failure: searchFailure,
    waiting: searchWaiting,
  } = useSearchHits(offered ? q : '', narrowed, typed)
  // Where the hits go, the view waits for the box to be offered (for the
  // status, or for rows) and then for the search: one wait over both, so a
  // line drawn before the query is sent stays drawn after it. A failed read
  // ends the wait before the offer, and the failure holds until a read of it
  // answers: the failure is what the page says meanwhile.
  const statusFailed = useStatusFailed()
  const searchWait =
    (!offered && failure === undefined && !statusFailed) || searchWaiting
  const searchLoadingDrawn = useDrawnWait(searching && searchWait)
  const placeholder =
    typeof library !== 'number'
      ? 'Search every library'
      : entry
        ? `Search ${entry.name}`
        : 'Search this library'

  // The phone bar's search icon arrives here with `focusSearch` in the
  // navigation's state, from any screen; on this one it focuses the box
  // itself, inside the tap, which is what lets iOS raise its keyboard. The
  // state stays on the history entry, so Back or Forward to it (a `POP`) is
  // a return to the grid and focuses nothing: a focus there would raise the
  // phone's keyboard over what the reader came back to.
  const focusSearch =
    (location.state as { focusSearch?: boolean } | null)?.focusSearch === true
  // biome-ignore lint/correctness/useExhaustiveDependencies: `location.key` is a trigger — each push or replace arrival focuses the box once.
  useEffect(() => {
    if (focusSearch && arrival !== 'POP') {
      document.getElementById(SEARCH_BOX_ID)?.focus()
    }
  }, [location.key])

  /**
   * Writes a pressed chip or sort into the address with `replace`, as the
   * search box writes `?q=`: a press adds no entry for Back to walk out of.
   * Every other parameter stays, and so does the box's tag (O223). The rest
   * of the navigation's state is for an arrival, as `focusSearch` is:
   * carried over a press, it would focus the box again.
   */
  const choose = (next: View) => {
    setParams(withView(params, next), {
      replace: true,
      state: boxStateOf(location.state),
    })
  }

  const continueList = useRef<HTMLUListElement>(null)
  const continueSection = useRef<HTMLElement>(null)
  // One condition for the strip and its hook, so they cannot disagree.
  const stripDrawn =
    !searching &&
    prefs.continueStrip &&
    home !== undefined &&
    home.continuing.length > 0
  useContinueRow(
    continueList,
    stripDrawn,
    coverSizeOf(prefs),
    home?.continuing.length ?? 0,
  )
  // The scroll is put back once the body this address draws is there: the
  // hits where a query is, the grid where none is. After the Continue row's
  // hook, whose layout effect sizes its cards in the same commit: put back
  // before that, the place would be read in a page about to grow above the
  // view, and a browser that anchors the page carries the view down with it.
  useKeptScroll(searching ? !searchWait : ready, { library: true })

  const { panel } = prefs
  const mayDownload = useGate()?.who.can_download === true
  const rowPanelId = useId()
  // A close by Escape, or by the card leaving the row's view, gives focus
  // back to the card's `i`, or in `on-select` mode, which draws none, to
  // its link: only a readable card, which has a link, opens a panel.
  const backTo = (card: HTMLElement) =>
    card.querySelector<HTMLElement>(
      panel === 'button' ? ':scope > .panel-open' : ':scope > a',
    )
  // A card's panel lies under the row, over the chips and the grid (O417), as a
  // child of the row's section: the list scrolls sideways and is masked for its
  // fade, so it would clip the panel. Laid from the card, the row and the
  // section, and again when any of their sizes changes: the card's with the
  // cover size, the row's and the section's with the page area; and when the
  // card moves and nothing resizes: as the row scrolls, as `useContinueRow`
  // lays its column and gap, and when a read draws the row from a new answer.
  const follow = (box: HTMLElement, card: HTMLElement) => {
    const row = continueList.current
    const section = continueSection.current
    if (row === null || section === null) return
    layUnderRow(box, card, row, section)
    // jsdom has no observer and lays nothing out to follow.
    if (typeof ResizeObserver === 'undefined') return
    // Taken as in view, since a panel opens on a card whose `i` or link was
    // just pressed, pointed at or focused. When a new answer lays the panel
    // again, the observers' first report finds a card that answer moved out
    // of the row's view and closes its panel, as a scroll would (O417).
    let inView = true
    // Every signal lays the panel through this, outside React, from the one
    // card's box and the row's and the section's, so no frame renders and a
    // frame's reads do not grow with the number of cards (O304).
    const lay = () => {
      // Read before the panel is laid: read after its write, the page would
      // be laid out again for the read.
      const was = inView
      inView = inRowView(card, row)
      layUnderRow(box, card, row, section)
      // A panel whose card has wholly left the row's view closes (O417), after
      // the editor's save. Focus or the pointer coming back into the panel
      // leaves the close waiting; the card coming back into view drops it,
      // unless a read brings it back (O439).
      // Focus in the panel goes to the card where it is: scrolled into view, it
      // would undo the person's scroll. Asked once as the card leaves: asked at
      // each scroll, a close held by a failed save would send it each time.
      if (was && !inView) {
        rowPanel.leave(
          box.contains(document.activeElement)
            ? rowPanel.refocus(backTo(card), { preventScroll: true })
            : undefined,
        )
      } else if (!was && inView) {
        rowPanel.stay()
      }
    }
    // Laid in the event itself: the browser sends `scroll` at most once a
    // frame, before it paints, so a scroll lays the panel once a frame at
    // most, and before the frame is painted.
    row.addEventListener('scroll', lay, { passive: true })
    const resized = new ResizeObserver(lay)
    resized.observe(card)
    resized.observe(row)
    resized.observe(section)
    // `useContinueRow` lays the row through its style: frame by frame
    // through a slide, in an animation frame, which moves the card and may
    // resize nothing; and at a cover size or a width. Heard in the microtask
    // after the write, so the frame is laid before it is painted. `style`
    // alone: the row's scroll marks are other attributes, and the panel
    // this lays is the row's sibling, so laying it is not heard here.
    const restyled = new MutationObserver(lay)
    restyled.observe(row, { attributeFilter: ['style'] })
    return () => {
      row.removeEventListener('scroll', lay)
      resized.disconnect()
      restyled.disconnect()
    }
  }
  // The cards a panel can be drawn under, as the hook's works: a read after
  // a press in the panel that leaves its card unreadable then sends focus to
  // `main`, as one that takes the card away does. One array per read, which
  // the hook compares to tell a read's answer from a render.
  const continuing = home?.continuing
  const panelCards = useMemo(
    () => continuing?.filter((w) => !unreadable(w)),
    [continuing],
  )
  const rowPanel = useVolumePanel({
    mode: panel,
    works: panelCards,
    follow,
    // A new answer can move the open card one place or more with nothing
    // resized or restyled: a card dropped or added before it. The panel is
    // then laid in the commit that draws the row, after `useContinueRow`'s
    // layout effect (its hook is called first) has laid the row.
    laidFor: continuing,
    backTo,
  })
  // The card whose panel is drawn: drawn itself, and readable, since an
  // unreadable card has no panel in any mode (O417).
  const openCard =
    stripDrawn && panel !== 'off'
      ? panelCards?.find((w) => w.slug === rowPanel.open)
      : undefined
  // A panel its card can no longer draw closes: the strip gone, the panel
  // switched off, or the card gone from the row or unreadable on a read. Left
  // open, it would come back under the card, with nobody having asked, once
  // the card is drawn again (O417).
  const orphaned = rowPanel.open !== undefined && openCard === undefined
  // biome-ignore lint/correctness/useExhaustiveDependencies: `orphaned` is the trigger; `close` reads only the hook's refs and setter, so any render's serves.
  useEffect(() => {
    if (orphaned) rowPanel.close()
  }, [orphaned])
  // Tab joins the open card and its panel (O417), which lies after the
  // row's list in the document, away from its card: from the card's last
  // stop into the panel, from the panel's last stop on to the next card
  // with a stop, which the document's order would pass for the chips, and
  // from the row's last stop on past a panel under an earlier card;
  // Shift+Tab mirrors each, as nesting gives the series page. Every other
  // Tab keeps the document's order, which passes a panel with no stop; in
  // `button` mode one that missed the panel closes it, as focus outside does.
  const tabAcross = (event: KeyboardEvent) => {
    if (event.key !== 'Tab' || event.ctrlKey || event.metaKey || event.altKey)
      return
    const card = rowPanel.tile.current
    const box = rowPanel.box.current
    const list = continueList.current
    const section = continueSection.current
    if (card === null || box === null || list === null || section === null)
      return
    const inPanel = tabbablesIn(box)
    const cardEnd = tabbablesIn(card).at(-1)
    // While the row's last stop is not in the open card, its panel lies past
    // the row's last stop in the document: Tab from that stop would meet it a
    // second time, and Shift+Tab from the stop after the row would meet it
    // before the cards after its own. Both go between those two stops, as on
    // the series page.
    const listEnd = tabbablesIn(list).at(-1)
    const away = listEnd !== undefined && !card.contains(listEnd)
    const from = event.target
    let to: HTMLElement | undefined
    if (event.shiftKey) {
      // The panel itself holds focus where a Split or an Undo left it.
      if (from === box || from === inPanel[0]) to = cardEnd
      else if (from === firstAfter(card)) to = inPanel.at(-1)
      else if (away && from === firstAfter(section)) to = listEnd
    } else if (from === cardEnd) {
      to = inPanel[0]
    } else if (from === inPanel.at(-1)) {
      to = firstAfter(card)
    } else if (away && from === listEnd) {
      to = firstAfter(section)
    }
    if (to === undefined) return
    event.preventDefault()
    to.focus()
  }
  // Heard on the document: the stop after the row lies outside its section.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `openCard` is the trigger; `tabAcross` reads only refs, so the render that drew the panel's serves.
  useEffect(() => {
    if (openCard === undefined) return
    document.addEventListener('keydown', tabAcross)
    return () => {
      document.removeEventListener('keydown', tabAcross)
    }
  }, [openCard])
  // The open card and its panel are one tile to the pointer, as a tile and
  // the panel nested in it are on the series page: a pointer crossing from
  // one to the other enters neither, so it drops no move or close waiting
  // on the editor's save.
  const crossed = (event: React.MouseEvent) => {
    const from = event.relatedTarget
    return (
      from instanceof Node &&
      ((rowPanel.tile.current?.contains(from) ?? false) ||
        (rowPanel.box.current?.contains(from) ?? false))
    )
  }
  // ↓ Download at the panel's foot asks of the edition the card shows, the
  // one its link opens, as a volume's tile asks of the one it shows (O283).
  // That edition has no file missing, or the card would be unreadable. An
  // `on-select` panel does not offer it, as a volume's does not.
  const shown = openCard?.editions.find((e) => e.key === openCard.opens)
  const download = panel === 'button' &&
    mayDownload &&
    openCard !== undefined &&
    shown !== undefined && (
      <a
        className="get"
        href={editionDownload(openCard.slug, shown.key)}
        download
        aria-label={`download ${openCard.title} — ${shown.label}`}
      >
        ↓ Download
      </a>
    )

  const visible = gridData
    ? sorted(
        gridData.series.filter((s) => matchesChip(s, chip)),
        sort,
      )
    : []

  return (
    <>
      {/* O163: one h1 per screen, for a screen reader's heading list; the
          library shows no title of its own, so it is hidden. A library's
          page is named for its library. */}
      <div
        className="page-head"
        // `library.css` keeps the page's scrollbar gutter while this is set:
        // on the head, drawn searching or not, rather than the Continue row.
        data-panel={panel === 'off' ? undefined : panel}
      >
        <h1 className="visually-hidden">{entry?.name ?? 'Library'}</h1>
        {offered && <SearchBar placeholder={placeholder} />}
        <ViewMenu keys={LIBRARY_VIEW} />
      </div>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {searching ? (
        <div className="search-hits">
          {searchFailure !== undefined && <p role="alert">{searchFailure}</p>}
          {hits === undefined ? (
            <PageLoading drawn={searchLoadingDrawn} />
          ) : hits.series.length === 0 && hits.unfiled.length === 0 ? (
            <EmptyView
              status={status}
              library={narrowed}
              filtered
              query={hits.query}
            />
          ) : (
            <SeriesGrid
              series={hits.series}
              unfiled={hits.unfiled}
              library={narrowed}
            />
          )}
        </div>
      ) : (
        <>
          {stripDrawn && (
            <section
              ref={continueSection}
              data-testid="continue"
              className="continue"
            >
              <h2>Continue</h2>
              <ul ref={continueList}>
                {home.continuing.map((w) => {
                  const lost = unreadable(w)
                  const selects = !lost && panel === 'on-select'
                  const drawsPanel = openCard?.slug === w.slug
                  return (
                    <li
                      key={w.slug}
                      ref={drawsPanel ? rowPanel.tile : undefined}
                      onFocusCapture={
                        selects
                          ? () => {
                              rowPanel.focused(w.slug)
                            }
                          : undefined
                      }
                      onMouseEnter={
                        selects
                          ? (event) => {
                              if (drawsPanel && crossed(event)) return
                              rowPanel.pointed(w.slug)
                            }
                          : undefined
                      }
                    >
                      {lost ? (
                        // **O62 — the link is the treatment, not a badge.** This
                        // strip is the likeliest place to meet a work whose files
                        // just vanished: the reader was mid-read when they went,
                        // so this is exactly where an unguarded link would send
                        // them back into a page that cannot load. The volume
                        // keeps its name, same as `Series.tsx`'s `Table`.
                        <span className="tile-unreadable">
                          <ContinueCard w={w} />
                          <span className="unreadable">
                            files missing — cannot be opened
                          </span>
                        </span>
                      ) : (
                        <Link to={readerPath(w)}>
                          <ContinueCard w={w} />
                        </Link>
                      )}
                      {/* In the card, never in its link, and not a child of
                        the list, whose first two children `useContinueRow`
                        reads the row's pitch from. */}
                      {!lost && panel === 'button' && (
                        <button
                          type="button"
                          className="panel-open"
                          aria-expanded={drawsPanel}
                          aria-controls={drawsPanel ? rowPanelId : undefined}
                          aria-label={`details for ${w.title}`}
                          onClick={(event) => {
                            rowPanel.toggle(w.slug, event.currentTarget)
                          }}
                        >
                          i
                        </button>
                      )}
                    </li>
                  )
                })}
              </ul>
              {openCard !== undefined && (
                <VolumePanel
                  id={rowPanelId}
                  state={rowPanel}
                  work={openCard}
                  foot={download}
                  // On select, focus or the pointer coming into the panel
                  // chooses its card, as the card's `li` holding the panel
                  // would: a move to another card that waits on the
                  // editor's save is then dropped, as on the series page.
                  onFocusCapture={
                    panel === 'on-select'
                      ? () => {
                          rowPanel.focused(openCard.slug)
                        }
                      : undefined
                  }
                  onMouseEnter={
                    panel === 'on-select'
                      ? (event) => {
                          if (!crossed(event)) rowPanel.pointed(openCard.slug)
                        }
                      : undefined
                  }
                />
              )}
            </section>
          )}
          <div className="library-chips">
            {/* O156: the filters are one-of-N, so they are grouped under one
            name. "Added" sorts rather than filters, and
            stays outside. */}
            <div className="library-chip-group" role="group" aria-label="Show">
              {CHIPS.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  aria-pressed={chip === c.id}
                  onClick={() => choose({ ...view, chip: c.id })}
                >
                  {c.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="sort"
              aria-pressed={sort === 'added'}
              onClick={() =>
                choose({ ...view, sort: sort === 'added' ? 'title' : 'added' })
              }
            >
              Added
            </button>
          </div>
          {library === 'invalid' ? (
            <EmptyState kind="no-library" />
          ) : !ready ? (
            <PageLoading drawn={loadingDrawn} />
          ) : gridData &&
            status &&
            visible.length === 0 &&
            gridData.unfiled.length === 0 ? (
            <EmptyView
              status={status}
              library={library}
              filtered={chip !== 'all'}
            />
          ) : (
            <SeriesGrid
              series={visible}
              unfiled={gridData?.unfiled ?? []}
              library={narrowed}
            />
          )}
        </>
      )}
    </>
  )
}
