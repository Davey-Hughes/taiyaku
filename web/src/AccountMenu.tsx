/**
 * The sidebar's foot: one button, "More", opening upward onto the entries an
 * administrator visits rarely — Proposals, Configuration and Account. The
 * theme switch sits above it, in the column itself (`Shell.tsx`).
 *
 * **Anyone else has only Account behind it**, and a menu of one entry is a
 * click for nothing, so for them "More" is that link itself. Account takes no
 * role (O94): every account has a password to change. The other two are
 * admin-only, and `admin` chooses only what is drawn: what an account may
 * *see* is the server's to enforce, and a non-admin who types `/config` or
 * `/proposals` is shown the server's own refusal there.
 *
 * **The count rides on the button as well as on the entry inside**, because
 * C3 §3's point is that somebody learns the inbox is non-empty without going
 * to look, and a count shut inside a closed panel says nothing. It counts at
 * `curation.ts`'s `INBOX_MIN_SCORE`, the threshold the inbox's own first
 * request uses, so the badge and the screen it leads to cannot disagree;
 * this file draws the number it is handed.
 *
 * **A disclosure, not an ARIA menu or dialog**: the panel holds only links,
 * and a `menu` would promise arrow-key movement between its items that a list
 * of links does not have. Opening leaves focus on the button, and the panel
 * follows it in the document, drawn above it, so Tab from the button enters
 * the panel; Escape closes it and puts focus back on the button
 * (`popover.ts`). Both forms are a `nav` of their own, because these links
 * are navigation; the theme switch above, a setting, stays outside it.
 *
 * The column marks the screen you are on, and these three are inside a
 * closed panel, so More is marked on their behalf with `data-here`, in either
 * form: a non-admin reaches `/proposals` and `/config` too. `aria-current`
 * stays each link's own, true only where it leads.
 */
import { NavLink, useLocation } from 'react-router'
import { usePopover } from './popover'

export interface AccountMenuProps {
  admin: boolean
  /**
   * Open proposals at `curation.ts`'s `INBOX_MIN_SCORE`, once known.
   *
   * `undefined` covers never fetched, still in flight and failed alike — the
   * same three-states-as-one `StatusContext` uses — and draws no badge.
   * Zero is *not* that: an inbox somebody has emptied is worth saying.
   */
  openProposals?: number
}

/** The screens behind More; on one of them, More is marked. */
const INSIDE = ['/proposals', '/config', '/account']

export function AccountMenu({ admin, openProposals }: AccountMenuProps) {
  const { pathname } = useLocation()
  const here = INSIDE.some(
    (p) => pathname === p || pathname.startsWith(`${p}/`),
  )
  const { open, toggle, wrap, button, panelId } = usePopover<HTMLElement>()

  if (!admin) {
    return (
      <nav className="account-menu" aria-label="More">
        <NavLink
          to="/account"
          className="account-button"
          data-here={here || undefined}
        >
          <span className="account-name">More</span>
        </NavLink>
      </nav>
    )
  }
  return (
    <nav className="account-menu" aria-label="More" ref={wrap}>
      <button
        ref={button}
        type="button"
        className="account-button"
        data-here={here || undefined}
        aria-expanded={open}
        aria-controls={panelId}
        aria-label={
          openProposals === undefined
            ? 'More'
            : `More, ${openProposals} open proposals`
        }
        onClick={toggle}
      >
        <span className="account-name">More</span>
        {openProposals !== undefined && (
          <span className="count">{openProposals}</span>
        )}
        <svg viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 7.5l3-3 3 3" />
        </svg>
      </button>
      <div id={panelId} className="account-panel" hidden={!open}>
        <ul className="account-links">
          <li>
            {/* O164: without a label the name is the word and the badge run
                together, saying nothing of what the number counts. The label
                says it in the inbox heading's words, and only while there is
                a number to say. */}
            <NavLink
              to="/proposals"
              aria-label={
                openProposals === undefined
                  ? undefined
                  : `Proposals, ${openProposals} open`
              }
            >
              Proposals
              {openProposals !== undefined && (
                <span className="count">{openProposals}</span>
              )}
            </NavLink>
          </li>
          <li>
            <NavLink to="/config">Configuration</NavLink>
          </li>
          <li>
            <NavLink to="/account">Account</NavLink>
          </li>
        </ul>
      </div>
    </nav>
  )
}
