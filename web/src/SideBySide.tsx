/**
 * A list and the page opened from it, side by side where the width allows:
 * spec §3. Users' table and an account's page, a library's page and one of
 * its directories'.
 *
 * **The address says whether a page is open; the width says where.** With
 * no page, the list is drawn alone. With one, the two are drawn as a pair
 * when `useFitsPair` says the split is as wide as its probe, and the page is
 * drawn alone otherwise, with its back link as its way to the list.
 *
 * **Narrow, the list is not drawn at all**, rather than drawn hidden: a list
 * mounted out of sight still makes its reads (a library's page polls its
 * scans), and a phone has no use for them.
 *
 * **The list keeps one place in the tree**, after the probe and before the
 * page, whether or not a page is open, so opening the first row or moving
 * between rows keeps the one list, with its scroll and its state. The page
 * is the caller's to key per row.
 *
 * **Closing a page by its back link gives focus back to its row.** The back
 * link (`PageHeader`'s) carries the page's address as `closed` in the
 * router state, spelled as the row's link spells it (`BackTo.closes`), not
 * as an address typed into the bar may spell the same page. When the
 * navigation it pushes lands here with no page open, the list's link to
 * that address takes focus, since the link that had it went with the page.
 * Once per navigation, so the list drawing again later does not pull focus
 * from wherever the person has gone; and only where the list has such a
 * link, so a page whose row went with it leaves focus to whatever the
 * screen gives it.
 */
import { type ReactElement, type ReactNode, useEffect, useRef } from 'react'
import { useHref, useLocation, useNavigationType } from 'react-router'
import { useFitsPair } from './fitsPair'

/** The address a back link's navigation closed, if it carries one. */
const closedOf = (state: unknown): string | undefined => {
  const closed = (state as { closed?: unknown } | null)?.closed
  return typeof closed === 'string' ? closed : undefined
}

export interface SideBySideProps {
  list: ReactNode
  /**
   * The page open beside the list, or `null` when the address opens none.
   * An element, so nothing that draws nothing (`false`, `undefined`) can
   * stand for an open page and take the list away.
   */
  page: ReactElement | null
}

export function SideBySide({ list, page }: SideBySideProps) {
  const split = useRef<HTMLDivElement>(null)
  const probe = useRef<HTMLDivElement>(null)
  const listEl = useRef<HTMLDivElement>(null)
  const fits = useFitsPair(split, probe)
  const pageOpen = page !== null

  const { key, pathname, state } = useLocation()
  // The back link's own navigation is a push. A Back or Forward to the
  // entry it made, or a reload of that entry, is a pop carrying the same
  // state, but the page closed then, if any, need not be the one it names.
  const pushed = useNavigationType() === 'PUSH'
  const closed = pushed ? closedOf(state) : undefined
  // The row's link is the router's drawing of the same address, so this is
  // the `href` it carries.
  const closedHref = useHref(closed ?? pathname)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` is the trigger: focus is given once per navigation, and the rest is read as that navigation left it.
  useEffect(() => {
    if (pageOpen || closed === undefined) return
    const links = listEl.current?.querySelectorAll('a[href]') ?? []
    for (const link of links) {
      if (link.getAttribute('href') === closedHref) {
        ;(link as HTMLElement).focus()
        return
      }
    }
  }, [key])

  return (
    <div
      ref={split}
      className={pageOpen && fits ? 'split split-pair' : 'split'}
    >
      <div className="split-probe" aria-hidden="true">
        <div ref={probe} className="split-breakpoint" />
      </div>
      {(!pageOpen || fits) && (
        <div ref={listEl} className="split-list">
          {list}
        </div>
      )}
      {pageOpen && <div className="split-detail">{page}</div>}
    </div>
  )
}
