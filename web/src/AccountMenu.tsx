/**
 * The sidebar's foot: one button, "More", opening upward onto the entries an
 * administrator visits rarely — Proposals, Configuration and Account. The
 * build's label sits above it and the theme switch below it, in the column
 * itself (`Shell.tsx`).
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
 * are navigation; the theme switch below, a setting, stays outside it.
 *
 * The column marks the screen you are on, and these three are inside a
 * closed panel, so More is marked on their behalf with `data-here`, in either
 * form: a non-admin reaches `/proposals` and `/config` too. `aria-current`
 * stays each link's own, true only where it leads.
 */
import { useLayoutEffect, useRef } from 'react'
import { NavLink, useLocation } from 'react-router'
import { usePopover } from './popover'

export interface AccountMenuProps {
  admin: boolean
  /**
   * Open proposals at `curation.ts`'s `INBOX_MIN_SCORE`, once known.
   *
   * `undefined` covers never fetched, still in flight and failed alike — the
   * same three-states-as-one `useLibraryStatus` uses — and draws no badge.
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
  const panel = useRef<HTMLDivElement>(null)

  // The panel hangs from More's top edge, and the column clips it at its own
  // top, where no scroll reaches: in a window too short for the panel above
  // More, as a phone held on its side can be, the first entries would be out
  // of reach. So while it is open it is held to the room above it, and
  // scrolls inside that (`shell.css`). Measured, because that room turns on
  // the height of More and of everything below it, which no stylesheet
  // knows; and measured again whenever one of the boxes that set it changes
  // size (`upToClip`), as larger text changes them as surely as a smaller
  // window does. Written only when it changes: the panel is none of those
  // boxes, so the change it makes to its own size asks for nothing more.
  // jsdom has no `ResizeObserver`, and lays nothing out to measure.
  useLayoutEffect(() => {
    const drawn = panel.current
    if (!open || !drawn || typeof ResizeObserver === 'undefined') return
    const { boxes, clip } = upToClip(drawn)
    let held = ''
    const hold = () => {
      const room = drawn.getBoundingClientRect().bottom - clipTop(clip)
      const next = `${Math.max(0, room)}px`
      if (next === held) return
      held = next
      drawn.style.setProperty('--room-above', next)
    }
    hold()
    const resized = new ResizeObserver(hold)
    for (const box of boxes) resized.observe(box)
    return () => {
      resized.disconnect()
    }
  }, [open])

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
      <div id={panelId} ref={panel} className="account-panel" hidden={!open}>
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

/**
 * The boxes from `el`'s parent up to the nearest that clips it, `clip`, or
 * to the root where none does. Their sizes set the room above `el`: More's
 * own, the foot's, which holds what stands below More, and the column's,
 * which is the window's height.
 */
function upToClip(el: HTMLElement): {
  boxes: HTMLElement[]
  clip?: HTMLElement
} {
  const boxes: HTMLElement[] = []
  for (let box = el.parentElement; box; box = box.parentElement) {
    boxes.push(box)
    if (getComputedStyle(box).overflowY !== 'visible') {
      return { boxes, clip: box }
    }
  }
  return { boxes }
}

/**
 * Where `clip` begins, past its padding (the drawer's is the notch's safe
 * area), and never above the window's top.
 */
function clipTop(clip: HTMLElement | undefined): number {
  if (clip === undefined) return 0
  const top =
    clip.getBoundingClientRect().top +
    Number.parseFloat(getComputedStyle(clip).paddingTop)
  return Math.max(0, top)
}
