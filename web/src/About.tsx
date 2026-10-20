/**
 * `/about`: which build is running, where its source is, the OPDS catalog's
 * address, and each format taiyaku writes with its version and a sentence
 * saying what it is, all as `GET /api/about` answers them (`about.ts`);
 * and taiyaku's licence, a link to the licence page (`Licence.tsx`).
 *
 * **The licence's card is drawn once the request has settled, answered or
 * failed, and last.** It uses nothing the route says, so a failure keeps
 * it; it waits for the answer so that it is not drawn alone at the top of
 * the page and then pushed down by the cards the answer brings. While the
 * request is in flight the page is its title alone.
 *
 * **The commit links to the repository's page for it only when there is
 * one.** A build that carries no commit says "unknown build" as plain text:
 * a link to `<repository>/commit/` with nothing after it leads nowhere.
 *
 * **Each format's sentence is drawn under its name, in the name's cell**,
 * as text rather than a tooltip, so it is there on a touch screen and read
 * out with the name by a screen reader.
 *
 * The OPDS catalog's address, and whether it can be copied, are
 * `CatalogAddress`'s.
 */
import { useEffect, useState } from 'react'
import { Link } from 'react-router'
import { type About as AboutAnswer, about, shortCommit } from './about'
import { CatalogAddress } from './CatalogAddress'
import { messageOf } from './message'
import { FONT_LICENCE, TAIYAKU_LICENCE } from './spdx'

export function About() {
  const [answer, setAnswer] = useState<AboutAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()

  useEffect(() => {
    let cancelled = false
    about()
      .then((next) => {
        if (!cancelled) setAnswer(next)
      })
      .catch((err: unknown) => {
        if (!cancelled) setFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <section className="about" aria-label="About">
      <h1>About</h1>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {answer !== undefined && (
        <>
          <section className="about-card">
            <h2 className="about-heading">This build</h2>
            <dl className="about-facts">
              <div>
                <dt>Commit</dt>
                <dd>
                  {answer.commit === null ? (
                    'unknown build'
                  ) : (
                    <a href={`${answer.repository}/commit/${answer.commit}`}>
                      {shortCommit(answer.commit)}
                    </a>
                  )}
                </dd>
              </div>
              <div>
                <dt>Source</dt>
                <dd>
                  <a href={answer.repository}>{answer.repository}</a>
                </dd>
              </div>
              <div>
                <dt>OPDS catalog</dt>
                <dd>
                  <CatalogAddress catalog={answer.catalog} />
                </dd>
              </div>
            </dl>
          </section>
          <section className="about-card">
            <h2 className="about-heading">Formats</h2>
            <table className="config-table">
              <thead>
                <tr>
                  <th scope="col">Format</th>
                  <th scope="col">Version</th>
                </tr>
              </thead>
              <tbody>
                {answer.formats.map((format) => (
                  <tr key={format.name}>
                    <td className="lead">
                      <span className="about-format-name">{format.name}</span>
                      <span className="about-format-description">
                        {format.description}
                      </span>
                    </td>
                    <td>
                      {format.version === null ? (
                        'unversioned'
                      ) : (
                        <>
                          <span className="cell-label">Version </span>
                          {format.version}
                        </>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        </>
      )}
      {(answer !== undefined || failure !== undefined) && (
        <section className="about-card">
          <h2 className="about-heading">Licence</h2>
          <p className="about-licence">
            taiyaku is <Link to="/about/licence">{TAIYAKU_LICENCE}</Link>, and
            its fonts are {FONT_LICENCE}.
          </p>
        </section>
      )}
    </section>
  )
}
