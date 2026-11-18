import { useEffect, useMemo, useRef, useState } from 'react'
import { type Book, contents, type Entry, editionOrder, entryAt } from './book'
import {
  context,
  type Here,
  type Match,
  ready,
  searchBook,
  stepFrom,
  warm,
} from './bookSearch'
import { usePanelFocus } from './focus'
import { useLatest } from './hooks'

/** How long typing must pause before the book is searched. */
const TYPED_MS = 120

/** The most matches listed; past it the count says so, and stepping still reaches every one. */
const MOST_LISTED = 500

interface BookSearchProps {
  visible: boolean
  book: Book
  /** The edition read, searched alone unless every edition is asked for. */
  edition: string
  /** `edition`'s contents, which its matches are grouped under. */
  entries: Entry[]
  /** The match last gone to, which Enter steps on from. */
  found?: Match
  /** Where the reader is, asked at each step: Enter steps from it once `found` is no longer listed. */
  here: () => Here | undefined
  onPick: (m: Match) => void
  /** Every match of the query standing, for the reader to step through once closed. */
  onResults: (matches: Match[]) => void
  onClose: () => void
}

/**
 * Find in a book: the edition read or every one, each match listed under its
 * chapter with the text around it. Kept mounted while hidden, drawing
 * nothing, so the query and the switch are there again when it opens.
 */
export function BookSearch({
  visible,
  book,
  edition,
  entries,
  found,
  here,
  onPick,
  onResults,
  onClose,
}: BookSearchProps) {
  const panelRef = useRef<HTMLDivElement>(null)
  const boxRef = useRef<HTMLInputElement>(null)
  usePanelFocus(panelRef, visible)
  useEffect(() => {
    if (visible) boxRef.current?.focus()
  }, [visible])
  useEffect(() => {
    if (visible) return warm(book, editionOrder(book))
  }, [visible, book])
  const [typed, setTyped] = useState('')
  const [query, setQuery] = useState('')
  const [all, setAll] = useState(false)
  useEffect(() => {
    const wait = setTimeout(() => setQuery(typed), TYPED_MS)
    return () => clearTimeout(wait)
  }, [typed])
  const searched = useMemo(
    () => (all ? editionOrder(book) : [edition]),
    [book, all, edition],
  )
  const results = useMemo(
    () => matchesOf(book, query, searched),
    [book, query, searched],
  )
  useEffect(() => onResults(results), [results, onResults])
  const chapters = useMemo(
    () =>
      Object.fromEntries(
        searched.map((k) => [
          k,
          k === edition ? entries : contents(book.editions[k].blocks),
        ]),
      ),
    [book, searched, edition, entries],
  )
  const picked = useLatest(onPick)
  const closed = useLatest(onClose)
  // Kept while the reader scrolls under the panel: 500 rows rebuilt a scroll
  // at a time is what scrolling with it open would cost.
  const rows = useMemo(() => {
    const listed = groups(results.slice(0, MOST_LISTED), chapters)
    return listed.map(({ edition: key, chapters: runs }) => (
      <section
        key={key}
        lang={book.editions[key].language ?? undefined}
        aria-label={all ? book.editions[key].label : undefined}
      >
        {all && (
          <h3 className="book-search-edition">{book.editions[key].label}</h3>
        )}
        {runs.map(({ entry, matches }) => (
          <div key={entry?.block ?? -1}>
            {entry && (
              <h4 className="book-search-chapter">{entry.lines.join(' · ')}</h4>
            )}
            <ol className="book-contents">
              {matches.map((m) => {
                const around = context(
                  book.editions[m.edition].blocks[m.block].text,
                  m,
                )
                return (
                  <li key={`${m.block}:${m.start}:${m.end}`}>
                    <button
                      type="button"
                      onClick={() => {
                        picked.current(m)
                        closed.current()
                      }}
                    >
                      {around.before}
                      <strong>{around.match}</strong>
                      {around.after}
                    </button>
                  </li>
                )
              })}
            </ol>
          </div>
        ))}
      </section>
    ))
  }, [results, chapters, book, all, picked, closed])
  if (!visible) return null

  const step = (by: 1 | -1) => {
    // Enter before the pause is over searches what is typed, not what was.
    const list = typed === query ? results : matchesOf(book, typed, searched)
    setQuery(typed)
    const next = stepFrom(list, found, by, here())
    if (next) onPick(next)
  }
  const counted =
    results.length > MOST_LISTED
      ? `${MOST_LISTED}+ matches`
      : results.length === 1
        ? '1 match'
        : `${results.length} matches`
  return (
    <div
      className="docked-panel"
      role="dialog"
      aria-label="Search"
      ref={panelRef}
      tabIndex={-1}
    >
      <div className="reader-panel-head">
        <strong className="reader-panel-title">Search</strong>
        <button type="button" className="reader-button" onClick={onClose}>
          Close
        </button>
      </div>
      <input
        ref={boxRef}
        type="search"
        className="reader-field book-search-box"
        aria-label="Search the book"
        value={typed}
        onChange={(event) => setTyped(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
            step(event.shiftKey ? -1 : 1)
          } else if (event.key === 'Escape') {
            // The box's own Escape would empty it; the reader's is to close.
            event.preventDefault()
            onClose()
          }
        }}
      />
      <label className="book-search-all">
        <input
          type="checkbox"
          checked={all}
          onChange={(event) => setAll(event.target.checked)}
        />
        All editions
      </label>
      {ready(query) && (
        <p className="book-search-count" role="status">
          {counted}
        </p>
      )}
      {rows}
    </div>
  )
}

function matchesOf(book: Book, query: string, editions: string[]): Match[] {
  return ready(query) ? searchBook(book, query, editions) : []
}

/** Matches by edition, in the order searched, then by the chapter each is in. */
function groups(
  matches: Match[],
  chapters: Record<string, Entry[]>,
): { edition: string; chapters: { entry?: Entry; matches: Match[] }[] }[] {
  const out: ReturnType<typeof groups> = []
  for (const m of matches) {
    let group = out.at(-1)
    if (group?.edition !== m.edition) {
      group = { edition: m.edition, chapters: [] }
      out.push(group)
    }
    const entries = chapters[m.edition] ?? []
    const entry = entries[entryAt(entries, m.block)]
    let run = group.chapters.at(-1)
    if (!run || run.entry !== entry) {
      run = { entry, matches: [] }
      group.chapters.push(run)
    }
    run.matches.push(m)
  }
  return out
}
