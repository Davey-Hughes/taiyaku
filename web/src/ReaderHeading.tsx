import { useDocumentTitle } from './title'

/**
 * The reader's one heading, for a screen reader alone, and its tab title
 * (O163, O173). Always mounted with the reader, so it is there with the bars
 * down, when the top bar that shows the same words is not.
 *
 * **A component rather than a hook in `App`**, because the name needs the
 * edition being shown, which `App` computes only below its `if (!session)
 * return null` — where no hook may be called.
 */
export function ReaderHeading({ name }: { name: string }) {
  useDocumentTitle(name)
  return <h1 className="visually-hidden">{name}</h1>
}
