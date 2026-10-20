/**
 * `/about/licence`: taiyaku's licence and then each font's, every one above
 * its full text, as `licenceTexts.ts` bundles them from their files, under a
 * list of contents that links to each.
 *
 * **The texts arrive by `import()`**, so they are a chunk this page loads
 * and the entry chunk every page loads does not carry them. Until the chunk
 * arrives the page draws only its way up and its title; a chunk that fails
 * to load is said the way a failed request is on About.
 *
 * **The contents are drawn with the cards, not before them.** Drawn while
 * the chunk loads they would move nothing, since the cards come in below
 * them, but a chunk that fails would leave them leading to cards that are
 * never drawn.
 *
 * **Each contents link is a plain `<a>` to its card's fragment**, which the
 * router leaves to the browser: following it scrolls the card's heading to
 * the top of the view and focuses it, and the fragment stays in the address
 * to be shared. **Where the browser cannot put the view where the address
 * says, the page does**: on a load with a fragment, whose card is not drawn
 * until the texts arrive, and on Back and Forward (the effect below says
 * why).
 *
 * Each text is drawn as written, its line breaks and indents kept, and wraps
 * where the page is narrower than its lines rather than widening the page.
 */
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import {
  type Location,
  Link,
  useLocation,
  useNavigationType,
} from 'react-router'
import { messageOf } from './message'
import { FONT_LICENCE, TAIYAKU_LICENCE } from './spdx'

type Texts = typeof import('./licenceTexts')

const OFL = `the SIL Open Font License, version 1.1 (${FONT_LICENCE})`

/**
 * The licensed works, in the order the page draws them. `id` is the card
 * heading's, and so the fragment that leads to it: written here rather than
 * made per mount, so a link to a card says the same on every load.
 */
const WORKS: readonly {
  id: string
  name: string
  licence: string
  terms: string
  text: keyof Texts
}[] = [
  {
    id: 'taiyaku',
    name: 'taiyaku',
    licence: TAIYAKU_LICENCE,
    terms: `taiyaku is licensed under the GNU General Public License, version 3 or any later version (${TAIYAKU_LICENCE}).`,
    text: 'taiyaku',
  },
  {
    id: 'shippori-mincho-b1',
    name: 'Shippori Mincho B1',
    licence: FONT_LICENCE,
    terms: `The face titles are set in, licensed under ${OFL}.`,
    text: 'shipporiMincho',
  },
  {
    id: 'zen-kaku-gothic-new',
    name: 'Zen Kaku Gothic New',
    licence: FONT_LICENCE,
    terms: `The face the interface is set in, licensed under ${OFL}.`,
    text: 'zenKakuGothicNew',
  },
  {
    id: 'taiyaku-marks',
    name: 'taiyaku marks',
    licence: FONT_LICENCE,
    terms: `Seven characters cut from Zen Kaku Gothic New, licensed under ${OFL}.`,
    text: 'marks',
  },
]

/**
 * One licensed work: its name, its licence, and that licence's full text.
 * Its heading can take focus, which a link to it gives, and is not a stop
 * for Tab.
 */
function Licensed({
  id,
  name,
  terms,
  text,
}: {
  id: string
  name: string
  terms: string
  text: string
}) {
  return (
    <section className="about-card">
      <h2 className="about-heading" id={id} tabIndex={-1}>
        {name}
      </h2>
      <p>{terms}</p>
      <pre className="licence-text">{text}</pre>
    </section>
  )
}

export function Licence() {
  const [texts, setTexts] = useState<Texts | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const location = useLocation()
  const navigation = useNavigationType()
  const page = useRef<HTMLElement>(null)
  /** How many times the history has moved while the page is drawn. */
  const [pops, setPops] = useState(0)
  /**
   * The location, and the count of the history's moves, the view was last
   * put in place for, once the cards were drawn.
   */
  const placed = useRef<{ location: Location; pops: number } | undefined>(
    undefined,
  )

  // No `cancelled` flag, unlike About's request: there is one load per mount
  // and nothing to order, and React drops a state update made after unmount.
  useEffect(() => {
    import('./licenceTexts').then(setTexts, (err: unknown) => {
      setFailure(messageOf(err))
    })
  }, [])

  // The browser's own scroll restoration is off for the page's entries: on
  // Back and Forward it would put a scroll back after the effect below has
  // put the view in place. The setting is an entry's, and an entry made from
  // the current one, for a fragment or by a link's push, starts with it. So
  // it is turned off when the page arrives, which every fragment followed
  // here inherits, and back on when the page goes, for the entry the app has
  // moved to, which a link's push would otherwise have left off.
  useEffect(() => {
    history.scrollRestoration = 'manual'
    return () => {
      history.scrollRestoration = 'auto'
    }
  }, [])

  // Counts the history's moves, for the effect below, which says why. The
  // new count must not be drawn beside the location the history has left,
  // which the effect would land, and two things keep it apart. This listener
  // is added from a passive effect, after the layout effect `BrowserRouter`
  // listens from, so the router hears each `popstate` first. And React 19
  // draws a transition begun in a `popstate`, as the router's update is,
  // at the microtask checkpoint after the router's listener, before this
  // one runs; an ordinary transition would wait behind this listener's
  // urgent update. "Back under React's own scheduler" in
  // `Shell.browser.test.tsx` checks the order.
  useEffect(() => {
    const popped = () => {
      setPops((count) => count + 1)
    }
    window.addEventListener('popstate', popped)
    return () => {
      window.removeEventListener('popstate', popped)
    }
  }, [])

  // Puts the view where the address says when the cards are first drawn, and
  // on every later move of the history (`POP`): Back, Forward, and a followed
  // contents link, which the browser has already landed. On a load the
  // fragment's card is not drawn yet when the browser looks for it. On Back
  // and Forward the browser would put back only the document's scroll, which
  // at a desktop width leaves `<main>`, the part that scrolls there, where it
  // was, and at a phone's width puts back the view the entry was left at, not
  // the card the entry names.
  //
  // A fragment that names a card lands on that card and focuses its heading,
  // as following its link does. Any other address, on a change, puts the
  // page's top in view and leaves focus where it is. Keyed on the location
  // object and on the count of the history's moves: the router makes a new
  // location only when one of its fields or the navigation's type changes,
  // and every entry the browser makes for a fragment has the `key` "default"
  // and a null state, so a move between two entries of one fragment changes
  // the count alone. A move that changes the location as well can run the
  // effect once for each, and the second lands where the first did. A layout
  // effect, so the view is not painted in its old place first. Scrolled first
  // and then focused without a scroll of its own, so the heading lands where
  // its `scroll-margin-top` puts it. `scrollIntoView` is optional-chained
  // because jsdom has none.
  useLayoutEffect(() => {
    if (texts === undefined) return
    const last = placed.current
    if (last?.location === location && last.pops === pops) return
    const first = last === undefined
    placed.current = { location, pops }
    if (!first && navigation !== 'POP') return
    const work = WORKS.find(({ id }) => `#${id}` === location.hash)
    if (work !== undefined) {
      const heading = page.current?.querySelector<HTMLElement>(`#${work.id}`)
      heading?.scrollIntoView?.({ block: 'start' })
      heading?.focus({ preventScroll: true })
    } else if (!first) {
      page.current?.scrollIntoView?.({ block: 'start' })
    }
  }, [texts, location, navigation, pops])

  return (
    <section className="about licence" aria-label="Licence" ref={page}>
      <p className="back">
        <Link to="/about">‹ About</Link>
      </p>
      <h1>Licence</h1>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {texts !== undefined && (
        <>
          <nav className="licence-contents" aria-label="Contents">
            <ul>
              {WORKS.map((work) => (
                <li key={work.id}>
                  <a href={`#${work.id}`}>{`${work.name} (${work.licence})`}</a>
                </li>
              ))}
            </ul>
          </nav>
          {WORKS.map((work) => (
            <Licensed
              key={work.id}
              id={work.id}
              name={work.name}
              terms={work.terms}
              text={texts[work.text]}
            />
          ))}
        </>
      )}
    </section>
  )
}
