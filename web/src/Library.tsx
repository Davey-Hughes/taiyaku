/**
 * The library screen: the sidebar, the chips, the continue strip and the grid.
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
import { EmptyState } from './EmptyState'
import { useGate } from './gate'
import {
  type Chip,
  type Grid,
  grid as fetchGrid,
  home as fetchHome,
  type HomeLists,
  type LibraryStatus,
  matchesChip,
  type Sort,
  sorted,
  status as fetchStatus,
} from './library'
import { Sidebar } from './Sidebar'

const CHIPS: { id: Chip; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'multi', label: 'Multi-edition' },
  { id: 'unread', label: 'Unread' },
]

/**
 * A series cover, or a placeholder carrying its title — §8's third empty state.
 *
 * **A cold cover is the common first-run case, not an edge one.** Every scanned
 * series generates its cover on the first request for it, so the first visit to
 * a fresh library is exactly when one can 404 or time out — and a bare `<img>`
 * degrades to an empty box that says nothing and names nothing.
 *
 * The failure is remembered **by `src` rather than as a flag**, so a series
 * whose cover URL changes is retried rather than left broken for the life of
 * the page: `<li key={s.id}>` keeps this component mounted across a re-sort or
 * a chip, and a boolean set once would survive both.
 *
 * `alt=""` on the image and a visible title in the placeholder is not an
 * inconsistency: the `<a>` already carries `.title` as text, so a cover that
 * loads is decorative and an `alt` would have a screen reader say the name
 * twice. The placeholder's text is the *drawing*, standing in for artwork, and
 * `aria-hidden` keeps that from becoming the same repetition.
 */
function Cover({ src, title }: { src: string; title: string }) {
  const [failed, setFailed] = useState<string>()
  if (failed === src)
    return (
      <span className="cover-missing" aria-hidden="true">
        {title}
      </span>
    )
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      onError={() => {
        setFailed(src)
      }}
    />
  )
}

export function Library() {
  // **The roots list is an admin's, and only the gate knows who is asking.**
  // `Sidebar`'s own doc has always said its caller passes that default; until
  // 2026-08-21 the caller passed a hardcoded `false`, so the doc was false as
  // written and four things this branch shipped were unreachable from any UI:
  // the `?root=` parameter this component reads below, the names in
  // `status.roots`, `browse.rs`'s `named()` collision fallback — whose own doc
  // reasons about exactly this reader ("a non-admin reader has no business
  // being handed the server's filesystem layout") — and the `useSearchParams`
  // reading two lines down. `undefined` (no gate above, which means a test
  // mounting this screen alone) reads as "no account known" and so as not an
  // admin: what an account may *see* is the server's to enforce, and this
  // chooses only what to draw.
  //
  // A default rather than the last word: §11 step 8's `browse` module makes it
  // a preference, and a preference has to start somewhere.
  const admin = useGate()?.who.admin ?? false
  const [params] = useSearchParams()
  const rootParam = params.get('root')
  const root = rootParam === null ? undefined : Number(rootParam)

  const [gridData, setGrid] = useState<Grid | undefined>()
  const [home, setHome] = useState<HomeLists | undefined>()
  const [status, setStatus] = useState<LibraryStatus | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [chip, setChip] = useState<Chip>('all')
  const [sort, setSort] = useState<Sort>('title')

  useEffect(() => {
    let cancelled = false
    Promise.all([fetchGrid(root), fetchHome(), fetchStatus()])
      .then(([grid, home, status]) => {
        if (cancelled) return
        setGrid(grid)
        setHome(home)
        setStatus(status)
        // A retry that lands after an earlier failure must clear it — this
        // effect re-fires on `root` alone, so a reader who failed once and
        // then switched (or re-triggered) a load that succeeded would
        // otherwise keep the banner on screen forever, beside the data that
        // just proved it stale.
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
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
    <div className="library">
      <Sidebar roots={status?.roots ?? []} showRoots={admin} />
      <main>
        {failure !== undefined && <p role="alert">{failure}</p>}
        {home && home.continuing.length > 0 && (
          <section data-testid="continue" className="continue">
            <h2>Continue</h2>
            <ul>
              {home.continuing.map((w) => (
                <li key={w.slug}>
                  <Link to={`/read/${w.slug}`}>{w.title}</Link>
                </li>
              ))}
            </ul>
          </section>
        )}
        <div className="chips">
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
          <ul className="grid">
            {visible.map((s) => (
              <li key={s.id}>
                <Link to={`/series/${s.id}`}>
                  <Cover src={s.cover} title={s.title} />
                  <span className="title">{s.title}</span>
                  {s.aligned > 0 && (
                    <span className="aligned">⇄ {s.aligned}</span>
                  )}
                </Link>
              </li>
            ))}
            {gridData?.unfiled.map((w) => (
              <li key={w.slug}>
                {/* An unfiled work has no series page — §5.4 — so it links
                    straight into the reader. */}
                <Link to={`/read/${w.slug}`}>
                  <span className="title">{w.title}</span>
                </Link>
              </li>
            ))}
          </ul>
        )}
      </main>
    </div>
  )
}
