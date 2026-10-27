/**
 * `/about`: which build is running, where its source is, the OPDS catalog's
 * address, and each format taiyaku writes with its version and a sentence
 * saying what it is, all as `GET /api/about` answers them (`about.ts`);
 * and the licences, taiyaku's and its fonts', each a link to that licence
 * on the licence page (`Licence.tsx`).
 *
 * **The Licence section is drawn once the request has settled, answered or
 * failed, and last.** It uses nothing the route says, so a failure keeps
 * it; it waits for the answer so that it is not drawn alone at the top of
 * the page and then pushed down by the sections the answer brings. While
 * the request is in flight the page is its header alone.
 *
 * **The build's facts stay a `dl`**, not settings rows: a screen reader
 * reads each as a term and its definition, and there is nothing to set.
 *
 * **The commit links to the repository's page for it only when there is
 * one.** A build that carries no commit says "unknown build" as plain text:
 * a link to `<repository>/commit/` with nothing after it leads nowhere. A
 * build whose tracked files differed from its commit says " with uncommitted
 * changes" after the link, in words where the sidebar's label has a `*`;
 * the link still leads to the commit the changes were made on.
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
import { reasonOf } from './message'
import { PageHeader, Section } from './settingsLayout'
import {
  FONT_LICENCE,
  FONTS_FRAGMENT,
  TAIYAKU_FRAGMENT,
  TAIYAKU_LICENCE,
} from './spdx'

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
        if (!cancelled) {
          setFailure(`This build’s details were not loaded: ${reasonOf(err)}`)
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <section className="about" aria-label="About">
      <PageHeader level={1} title="About" />
      {failure !== undefined && <p role="alert">{failure}</p>}
      {answer !== undefined && (
        <>
          <Section level={2} title="This build">
            {/* A group (`settings.css`): each fact is a row of the box, with
                the box's line between one row and the next. */}
            <dl className="about-facts settings-group">
              <div>
                <dt>Commit</dt>
                <dd>
                  {answer.commit === null ? (
                    'unknown build'
                  ) : (
                    <>
                      <a href={`${answer.repository}/commit/${answer.commit}`}>
                        {shortCommit(answer.commit)}
                      </a>
                      {answer.dirty && ' with uncommitted changes'}
                    </>
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
          </Section>
          <Section level={2} title="Formats">
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
          </Section>
        </>
      )}
      {(answer !== undefined || failure !== undefined) && (
        <Section level={2} title="Licence">
          {/* Each link lands on its licence by the licence page's own
              fragments, the fonts' on the first of the three fonts. A
              sentence each, each a row of the box, so the two links are
              never on lines close enough for a finger's targets to meet. */}
          <p className="about-licence">
            taiyaku is{' '}
            <Link to={`/about/licence#${TAIYAKU_FRAGMENT}`}>
              {TAIYAKU_LICENCE}
            </Link>
            .
          </p>
          <p className="about-licence">
            Its fonts are{' '}
            <Link to={`/about/licence#${FONTS_FRAGMENT}`}>{FONT_LICENCE}</Link>.
          </p>
        </Section>
      )}
    </section>
  )
}
