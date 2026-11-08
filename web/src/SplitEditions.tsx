/**
 * The split editions screen: each edition a library shows some of the files
 * of and not all, what an account granted one of those libraries alone sees
 * of it, and where each file is held (spec §3.8).
 *
 * **What a library sees is the route's count, not one made here.** A file
 * held under directories of two libraries counts in both, so the counts are
 * the server's and the screen only puts them beside the edition's totals.
 *
 * **A directory links to its page under one library holding it**, the
 * first the edition lists (spec decision 29), whose name the address
 * carries: the route gives a directory's libraries as ids, so the name is
 * found among the edition's libraries. A directory several libraries hold
 * is one link, not one per library: links of one path to different pages
 * could not be told apart.
 */
import { Link } from 'react-router'
import { dirPath } from './configAddress'
import { JOINING_DOT } from './joiningDot'
import { messageOf } from './message'
import { SPLIT_EDITIONS } from './pageReads'
import { PageHeader, PageLoading, Section } from './settingsLayout'
import type {
  MemberDirectory,
  SplitEdition,
  SplitLibrary,
} from './splitEditions'
import { usePage } from './usePage'

const pageCount = (pages: number) =>
  `${pages} ${pages === 1 ? 'page' : 'pages'}`

/** What an account granted `library` alone sees of `edition`. */
function sees(library: SplitLibrary, edition: SplitEdition): string {
  const files = edition.members.length
  return library.members === files
    ? `${library.name} sees every file`
    : `${library.name} sees ${library.members} of ${files} files, ${library.pages} of ${edition.pages} pages`
}

/**
 * `dir`'s path, as a link to its page under the first library the edition
 * lists that holds it. A directory none of whose libraries the edition
 * lists has no page to name, so its path is drawn alone.
 */
function DirLink({
  dir,
  edition,
}: {
  dir: MemberDirectory
  edition: SplitEdition
}) {
  const library = edition.libraries.find((one) =>
    dir.libraries.includes(one.id),
  )
  const path = <code>{dir.path}</code>
  return library === undefined ? (
    path
  ) : (
    <Link to={dirPath(library.name, dir.root)}>{path}</Link>
  )
}

export function SplitEditions() {
  const {
    ready,
    answers: [list],
    failures: [listFailure],
    loadingDrawn,
  } = usePage<[{ editions: SplitEdition[] }]>([SPLIT_EDITIONS])
  const editions = ready ? list?.editions : undefined
  const failure = listFailure === undefined ? undefined : messageOf(listFailure)

  return (
    <section className="split-editions">
      <PageHeader
        title="Split editions"
        description={
          <>
            An edition is split when a library shows some of its files and not
            all of them. An account granted only some of those libraries sees
            only their files, with the pages renumbered. An account granted
            every library listed sees every file a directory holds, and an
            account that sees every library, as an administrator does, sees the
            whole edition.
          </>
        }
      />
      {failure !== undefined && <p role="alert">{failure}</p>}
      {editions === undefined && <PageLoading drawn={loadingDrawn} />}

      {editions !== undefined && (
        <>
          <Section>
            {editions.length === 0 ? (
              <p className="empty">No edition is split between libraries.</p>
            ) : (
              <ul className="settings-group">
                {editions.map((edition) => (
                  <li key={`${edition.work}\n${edition.edition}`}>
                    <h3 className="split-edition-title">
                      {edition.title}
                      {JOINING_DOT}
                      <code>{edition.edition}</code>
                    </h3>
                    <ul className="split-edition-sees">
                      {edition.libraries.map((library) => (
                        <li key={library.id}>{sees(library, edition)}</li>
                      ))}
                    </ul>
                    <ul className="split-edition-files">
                      {edition.members.map((member) => (
                        <li key={member.path}>
                          <p className="split-edition-file">
                            <code>{member.path}</code>
                            {JOINING_DOT}
                            {pageCount(member.pages)}
                          </p>
                          <ul className="split-edition-dirs">
                            {member.directories.map((dir) => (
                              <li key={dir.root}>
                                <DirLink dir={dir} edition={edition} />
                              </li>
                            ))}
                          </ul>
                        </li>
                      ))}
                    </ul>
                  </li>
                ))}
              </ul>
            )}
          </Section>
          {editions.length > 0 && (
            <p className="split-edition-how">
              To split one, give one of its directories its own edition key (in
              the directory&rsquo;s settings, or with a rule), then scan that
              directory.
            </p>
          )}
        </>
      )}
    </section>
  )
}
