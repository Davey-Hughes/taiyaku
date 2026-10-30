/**
 * What a failed load left to say. `App`'s load effect records one in its
 * `catch`.
 *
 * `not-found` is `api.ts`'s `WorkNotFound`, the sidecar read's 404, and
 * `work` is the slug that error carries. `thrown` is anything else, and
 * `message` is what `message.ts`'s `messageOf` reads from it, which may be
 * `''`.
 */
export type LoadFailure =
  | { kind: 'not-found'; work: string }
  | { kind: 'thrown'; message: string }

export interface ReaderFailedProps {
  /** What the load's `catch` recorded. */
  failure: LoadFailure
  /** `App`'s own `onLeave`: the exit the top bar's `Library` button takes. */
  onLeave: () => void
}

/**
 * What the reader draws when its work never loads — O176.
 *
 * Without this screen `App` would draw nothing in this state:
 * `if (!session) return null` holds for good, which would leave `Routes.tsx`'s
 * black `.reader-route` with no message, no heading, no landmark and no way
 * back but the browser's own. A typed `/read/…` for a slug the library does
 * not hold gets here, and so does a server that does not answer.
 *
 * **One failure is said in this screen's own words, and every other is shown
 * as it was thrown** — either way in a `<p role="alert">`, which is where
 * `Library.tsx` and `Series.tsx` put a failed read of theirs. The one is
 * `not-found` (O179): the alert reads "The server found no work named" and
 * then the slug, in a `<code>`, with no quotation mark and no backtick round
 * it. The thrown message is not on screen in that case, and so neither is the
 * response's body.
 *
 * **It says what the server found, and not what the library holds**, because
 * a 404 here is three different facts under one body. `routes.rs`'s
 * `get_sidecar` answers it for a slug the library does not hold; for a work
 * the account's grant hides; and for a work the library does hold and this
 * server process has not loaded — `ServeState`'s `works` is a snapshot taken
 * at startup, so a volume a scan has just derived is in the library's own
 * answers and is a 404 to the reader.
 *
 * For `thrown` the alert is the message — for a read the server refused,
 * `api.ts`'s sentence with the status and the body, and for anything else
 * whatever the thrower said, `startup.ts`'s own throw included. The heading
 * above the alert is on screen rather than hidden the way `ReaderHeading`'s
 * is, because nothing else here says the same words.
 *
 * **A `<main className="reader">` of its own**, not a child of `App`'s: that
 * one is below the guard this is returned from. The class is what gives it
 * the black ground, `color-scheme: dark` and so the Ink half of every token,
 * and `.reader :focus-visible`'s ring (`styles/reader.css`). O163's one
 * landmark and one heading hold here as they do once a work has loaded.
 *
 * **The button is the only way out that this screen has to supply.** `App`'s
 * keydown listener subscribes once there is a session, so `L` is not bound
 * here and `Tab` is the browser's own, which is what reaches the button.
 * There is no retry beside it: `Routes.tsx` keys `App` by slug, so nothing
 * short of a remount would run the load again, and reloading is the retry.
 * Reloading by the browser's own control: `styles/reader.css`'s
 * `overscroll-behavior` rule is keyed on `.reader-route` and `.reader`, so it
 * holds on this screen as it does under a page.
 *
 * **The tab's title is left alone.** O173 decided it says the app's name
 * until a work has loaded "and for good if it never does"; that is `App`'s
 * `useDocumentTitle(undefined)`, and this sets nothing over it.
 *
 * Geometry inline and colour by class, as `Gap.tsx` does for its own
 * sentence on the black. `minHeight` rather than `height`, because the
 * message is as long as the response it quotes: a box that may grow, not one
 * that centres what it cannot hold.
 */
export function ReaderFailed({ failure, onLeave }: ReaderFailedProps) {
  return (
    <main
      className="reader"
      style={{
        boxSizing: 'border-box',
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 16,
        minHeight: '100%',
        padding: 24,
        textAlign: 'center',
      }}
    >
      <h1 className="reader-failed-title">This work could not be opened</h1>
      <p
        role="alert"
        className="reader-muted"
        style={{ margin: 0, overflowWrap: 'anywhere' }}
      >
        {failure.kind === 'not-found' ? (
          <>
            The server found no work named <code>{failure.work}</code>.
          </>
        ) : (
          failure.message
        )}
      </p>
      <button type="button" className="reader-button" onClick={onLeave}>
        Back to the library
      </button>
    </main>
  )
}
