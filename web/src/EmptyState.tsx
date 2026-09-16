/**
 * The screens someone sees on their first run — spec C1 §8.
 *
 * **Three of §8's five, and the other two are elsewhere by their own nature.**
 * "A series with no cover" is a per-tile placeholder rather than a screen, so it
 * lives in `Library.tsx` beside the `<img>` it replaces; "a work whose members
 * are missing from disk" needed a fact §5.5 left to spec C3, and C3 built it —
 * `browse::missing_by_edition`, `EditionCard.missing`, and `library.ts`'s
 * `unreadable`. It still draws no screen of its own here, though: the
 * treatment lives per-work, withholding the reader link and the download
 * wherever a `WorkCard` renders, rather than as a sixth `EmptyKind` in this
 * catalogue.
 *
 * **None of them is a `null` return.** "No roots configured" and "roots
 * configured, scan not yet run" are told apart by `/api/library/status` and by
 * nothing else: `/api/library/series` answers `{ series: [], unfiled: [] }` in
 * both states, which is correct of it and useless here. The difference is
 * between "you have no library" and "your library has not been read yet", and
 * the person seeing it can act on exactly one of those.
 *
 * **Three more for the proposals inbox — spec C3 §3 — split on that same
 * rule.** An inbox that draws nothing is three different situations, and the
 * person seeing it can act on exactly one of each: `inbox-clear` is "there is
 * nothing to answer", `inbox-weak-hidden` is "there may be something and the
 * score threshold is holding it back", and `kind-answered` is "this one kind
 * is answered out and the others are still waiting". Collapsing the second
 * into the first would tell somebody their work was finished while the
 * control that would show them otherwise sat unpressed above the message.
 *
 * **And three for the inbox's other two states (O72), on the same split.**
 * With `accepted` or `dismissed` selected an empty list is again three
 * situations: nothing has been decided that way (`decided-none`), something
 * may have been and the threshold is hiding it (`decided-weak-hidden`), or
 * this one kind holds none of them while others do (`decided-kind-none`).
 * They are kinds of their own rather than the three above with a flag,
 * because every sentence above is about proposals *waiting on a decision*,
 * and a decided list saying "nothing to answer" would be telling somebody
 * looking for a dismissal that there was nothing left to decide.
 *
 * **None of them draws a control of its own.** Every recovery these six
 * name — `Show weak proposals`, the `All` tab — is already on screen above
 * the list, drawn by `Proposals.tsx`, so this names it in words rather than
 * offering a second button that does the same thing from a different place.
 * The two library states point at a shell command for the same reason: the
 * act that fixes them is not on the page at all.
 */
export type EmptyKind =
  | 'no-roots'
  | 'not-scanned'
  | 'no-matches'
  | 'inbox-clear'
  | 'inbox-weak-hidden'
  | 'kind-answered'
  | 'decided-none'
  | 'decided-weak-hidden'
  | 'decided-kind-none'

export interface EmptyStateProps {
  kind: EmptyKind
  /** What was searched, for `no-matches`. */
  query?: string
  /**
   * Which kind's tab is selected, as `kindLabel` writes it, for
   * `kind-answered` and `decided-kind-none`.
   */
  label?: string
  /** Which decided state the inbox is showing, for the three `decided-*` kinds. */
  decided?: 'accepted' | 'dismissed'
}

export function EmptyState({ kind, query, label, decided }: EmptyStateProps) {
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
  if (kind === 'inbox-clear') {
    return (
      <section className="empty">
        <h2>Nothing to answer</h2>
        <p>
          Every proposal this library has raised has been accepted or dismissed.
          A scan raises new ones:
        </p>
        <pre>taiyaku library scan</pre>
      </section>
    )
  }
  if (kind === 'inbox-weak-hidden') {
    return (
      <section className="empty">
        <h2>Nothing to answer</h2>
        <p>
          Nothing is open above the score threshold. Show weak proposals reveals
          the ones below it — a weak proposal is hidden, never dropped.
        </p>
      </section>
    )
  }
  if (kind === 'kind-answered') {
    return (
      <section className="empty">
        <h2>Nothing here</h2>
        <p>
          {label === undefined || label === ''
            ? 'No proposal of this kind is waiting on a decision.'
            : `No ${label} proposal is waiting on a decision.`}{' '}
          All shows the ones that are.
        </p>
      </section>
    )
  }
  if (kind === 'decided-none') {
    return (
      <section className="empty">
        <h2>Nothing {decided}</h2>
        <p>No proposal in this library has been {decided}.</p>
      </section>
    )
  }
  if (kind === 'decided-weak-hidden') {
    return (
      <section className="empty">
        <h2>Nothing {decided}</h2>
        <p>
          Nothing {decided} is above the score threshold. Show weak proposals
          reveals the ones below it.
        </p>
      </section>
    )
  }
  if (kind === 'decided-kind-none') {
    return (
      <section className="empty">
        <h2>Nothing here</h2>
        <p>
          {label === undefined || label === ''
            ? `No proposal of this kind has been ${decided}.`
            : `No ${label} proposal has been ${decided}.`}{' '}
          All shows every one that has.
        </p>
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
