/**
 * The screens someone sees on their first run — spec C1 §8.
 *
 * **Three of §8's five, and the other two are elsewhere by their own nature.**
 * "A series with no cover" is a per-tile placeholder rather than a screen, so it
 * lives in `Library.tsx` beside the `<img>` it replaces; "a work whose members
 * are missing from disk" needs a fact §5.5 leaves to spec C3 and is not built.
 *
 * **None of them is a `null` return.** "No roots configured" and "roots
 * configured, scan not yet run" are told apart by `/api/library/status` and by
 * nothing else: `/api/library/series` answers `{ series: [], unfiled: [] }` in
 * both states, which is correct of it and useless here. The difference is
 * between "you have no library" and "your library has not been read yet", and
 * the person seeing it can act on exactly one of those.
 */
export type EmptyKind = 'no-roots' | 'not-scanned' | 'no-matches'

export interface EmptyStateProps {
  kind: EmptyKind
  /** What was searched, for `no-matches`. */
  query?: string
}

export function EmptyState({ kind, query }: EmptyStateProps) {
  if (kind === 'no-roots') {
    return (
      <section className="empty">
        <h2>No library yet</h2>
        <p>
          Nothing has been added to this server. Point it at a directory of
          archives:
        </p>
        <pre>taiyaku library scan --add /path/to/manga</pre>
        <p>
          A scan reads what is on disk — series, volumes, editions and page
          counts — without writing anything back into the tree.
        </p>
      </section>
    )
  }
  if (kind === 'not-scanned') {
    return (
      <section className="empty">
        <h2>Not read yet</h2>
        <p>This library has roots, and nothing has scanned them.</p>
        <pre>taiyaku library scan</pre>
      </section>
    )
  }
  return (
    <section className="empty">
      <h2>Nothing here</h2>
      <p>
        {query === undefined || query === ''
          ? 'No series match this filter.'
          : `Nothing matched ${query}.`}
      </p>
    </section>
  )
}
