/**
 * The library screen: the continue strip, the filter chips, and the grid of
 * series and unfiled works.
 *
 * **The sidebar is `Shell`'s and the tiles are `SeriesGrid`'s.** This file drew
 * both until Plan B: the sidebar and the `/api/library/status` fetch moved to
 * `Shell` so three screens share one, and the tiles moved to `SeriesGrid` so
 * `/` and `/search` cannot disagree about them. What is left here is the part
 * that is only the library's — which chips exist, what they filter, and the
 * three-way empty state.
 *
 * Spec C1 §2 and §11 step 6. **No virtualisation** — §7's measured numbers are
 * 127 series and 111 volumes in the largest series, and covers carry
 * `loading="lazy"`, which is free. The figure that would change that is a
 * chapter-organised edition, and it should be produced rather than imagined.
 *
 * The chips filter and sort **client-side**, over data already held: §7's
 * numbers make a `?sort=` round trip a request for something the browser does
 * instantly. `?root=` is a query parameter for the opposite reason — it
 * changes which rows exist, and which rows exist is the grant's business.
 */
import { useEffect, useState } from 'react'
import { Link, useSearchParams } from 'react-router'
import { useBrowsePrefs } from './browseModule'
import { Cover } from './Cover'
import { EmptyState } from './EmptyState'
import {
  type Chip,
  type Grid,
  grid as fetchGrid,
  home as fetchHome,
  type HomeLists,
  matchesChip,
  type Sort,
  sorted,
  unreadable,
  type WorkCard,
  workCover,
} from './library'
import { SeriesGrid } from './SeriesGrid'
import { useLibraryStatus } from './shell'
import { messageOf } from './message'

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
      <Cover src={workCover(w.slug, w.opens)} title={w.title} />
      <span className="title">{w.title}</span>
      {w.volume !== null && <span className="meta">Vol. {w.volume}</span>}
    </>
  )
}

export function Library() {
  const prefs = useBrowsePrefs()
  const [params] = useSearchParams()
  const rootParam = params.get('root')
  const root = rootParam === null ? undefined : Number(rootParam)

  const status = useLibraryStatus()
  const [gridData, setGrid] = useState<Grid | undefined>()
  const [home, setHome] = useState<HomeLists | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [chip, setChip] = useState<Chip>('all')
  const [sort, setSort] = useState<Sort>('title')

  useEffect(() => {
    let cancelled = false
    Promise.all([fetchGrid(root), fetchHome()])
      .then(([grid, home]) => {
        if (cancelled) return
        setGrid(grid)
        setHome(home)
        // A retry that lands after an earlier failure must clear it — this
        // effect re-fires on `root` alone, so a reader who failed once and
        // then switched (or re-triggered) a load that succeeded would
        // otherwise keep the banner on screen forever, beside the data that
        // just proved it stale.
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [root])

  const visible = gridData
    ? sorted(
        gridData.series.filter((s) => matchesChip(s, chip)),
        sort,
      )
    : []

  return (
    <>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {prefs.continueStrip && home && home.continuing.length > 0 && (
        <section data-testid="continue" className="continue">
          <h2>Continue</h2>
          <ul>
            {home.continuing.map((w) => (
              <li key={w.slug}>
                {unreadable(w) ? (
                  // **O62 — the link is the treatment, not a badge.** I1 of
                  // the whole-branch review names this strip as the
                  // likeliest place to meet a work whose files just
                  // vanished: the reader was mid-read when they went, so
                  // this is exactly where an unguarded link would send them
                  // back into a page that cannot load. The volume keeps its
                  // name, same as `Series.tsx`'s `Table`.
                  <span className="tile-unreadable">
                    <ContinueCard w={w} />
                    <span className="unreadable">
                      files missing — cannot be opened
                    </span>
                  </span>
                ) : (
                  <Link to={`/read/${w.slug}`}>
                    <ContinueCard w={w} />
                  </Link>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}
      <div className="library-chips">
        {CHIPS.map((c) => (
          <button
            key={c.id}
            type="button"
            aria-pressed={chip === c.id}
            onClick={() => setChip(c.id)}
          >
            {c.label}
          </button>
        ))}
        <button
          type="button"
          className="sort"
          aria-pressed={sort === 'added'}
          onClick={() => setSort(sort === 'added' ? 'title' : 'added')}
        >
          Added
        </button>
      </div>
      {gridData &&
      status &&
      visible.length === 0 &&
      gridData.unfiled.length === 0 ? (
        <EmptyState
          kind={
            status.roots.length === 0
              ? 'no-roots'
              : status.scanned
                ? 'no-matches'
                : 'not-scanned'
          }
        />
      ) : (
        <SeriesGrid series={visible} unfiled={gridData?.unfiled ?? []} />
      )}
    </>
  )
}
