import type { CSSProperties } from 'react'
import { thumbUrl } from './api'

// The panel is `.docked-panel` in `styles/reader.css`, shared with `Help` and
// the notes panel (spec 2026-09-17 §8.5). This comment used to record that the
// reason for copying it had expired at four copies: three copies of this docked
// shell are one class now, and `Settings`' own — the fourth this comment
// counted — is `.reader-dialog-panel`.

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 10,
  padding: '6px 0',
  border: 'none',
  borderTop: '1px solid var(--reader-hair)',
  background: 'none',
  color: 'inherit',
  font: 'inherit',
  textAlign: 'left',
  cursor: 'pointer',
  width: '100%',
}

// A credits card is recognisable at a glance and unrecognisable from its page
// number alone, which is the whole reason the list carries thumbnails. Small,
// because the list is short — four pages on the primary corpus volume — and
// this is for telling a donation card from a staff page, not for reading it.
const thumbStyle: CSSProperties = {
  height: 56,
  width: 'auto',
  objectFit: 'contain',
  display: 'block',
  flex: '0 0 auto',
}

export interface CreditsPanelProps {
  visible: boolean
  work: string
  /** The edition being read. The list is its credit pages, not the work's. */
  edition: string
  /**
   * Zero-based page indices, straight from `Resolver.credits` — the sidecar's
   * own unit, the same kind of value `spreads` holds. Converted to 1-based
   * only in the label below, which is the boundary convention `altAt` and
   * `Gap`'s page numbers already follow.
   */
  pages: number[]
  /** Whether `skip` is on, so the panel can say what the list currently means. */
  skipping: boolean
  /** Take the reader to a page. Zero-based; `App.tsx` converts to a slot. */
  onGo: (page: number) => void
  onClose: () => void
}

/**
 * The pages a scanlation group asked for in exchange for the work, gathered in
 * one place.
 *
 * **Set aside, never hidden.** `modules/credits.ts` is normative: a group's
 * credit is what it asked for, so `skip` keeps those pages out of the reading
 * flow and this is the section of their own they go to. Taking an entry works
 * whether or not `skip` is on — nothing here is meant to become unreachable,
 * and a reader who lands on a skipped page steps off it with the next arrow —
 * by the ordinary path, because `skippedApartFrom` stops that page being
 * skipped while they are on it, so it belongs to a group like any other.
 *
 * **No labels.** The sidecar records page indices and nothing else. Which of
 * these is a donation card and which is a staff-credits page is something
 * `docs/status.md` §4 established by looking at them — four pages on the
 * primary volume, three the same donation card and one naming three people —
 * not something the format carries. A panel that named them would be inventing
 * a distinction the document cannot support, so it shows the page and lets a
 * reader see.
 *
 * The empty case is a sentence rather than an empty box, following
 * `chapterLine`'s rule that "an empty readout reads as a bug, a labelled one
 * reads as a fact about this particular work". It is the common case: a raw
 * edition has no scanlation credits at all, and no sidecar in the corpus has
 * had `taiyaku credits --write` run against it yet.
 */
export function CreditsPanel({
  visible,
  work,
  edition,
  pages,
  skipping,
  onGo,
  onClose,
}: CreditsPanelProps) {
  if (!visible) return null
  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label={`Credit pages in ${edition}`}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Credit pages — {edition}</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      {pages.length === 0 ? (
        <p className="reader-muted" style={{ margin: 0 }}>
          No credit pages found in {edition}.
        </p>
      ) : (
        <>
          {/* What the setting beside this panel is currently doing, said where
              the reader is looking at its consequences rather than only in the
              settings panel two keys away. */}
          <p className="reader-muted" style={{ margin: 0 }}>
            {skipping
              ? 'These are kept out of the reading flow. Taking one still goes there.'
              : 'These are still in the reading flow. Switch on “skip” in settings to set them aside.'}
          </p>
          {pages.map((page) => (
            <button
              key={page}
              type="button"
              style={rowStyle}
              onClick={() => onGo(page)}
            >
              <img
                className="reader-thumb"
                src={thumbUrl(work, edition, page)}
                alt=""
                style={thumbStyle}
              />
              {/* 1-based, the one conversion in this file: read by a person,
                  the same category `Gap`'s page numbers fall into. `page`
                  itself stays zero-based for `thumbUrl` and `onGo`, which is
                  what the server and the resolver expect. */}
              <span>page {page + 1}</span>
            </button>
          ))}
        </>
      )}
    </div>
  )
}
