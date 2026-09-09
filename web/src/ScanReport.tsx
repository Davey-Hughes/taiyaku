/**
 * A scan's two reports, rendered whole — spec C4 §3.
 *
 * **Two sections, because the server already sends two objects.** The walk
 * and the derivation are two different passes over the library, and keeping
 * them apart is what lets this draw "what the walk found" and "what
 * derivation made of it" rather than making a reader work out which of
 * forty-one keys came from which pass. `finished_scan_json`'s own doc
 * comment makes the same argument from the other side.
 *
 * **Every field is drawn, including the collections that are empty this
 * time.** The route destructures both structs with no `..` so a field cannot
 * go missing on the way out; undoing that at the last step by hiding an
 * empty row would make "nothing unreadable this time" and "this field does
 * not exist in this version of the API" look identical — exactly the
 * omission C4 §3 writes rules against.
 *
 * **The confession comes first.** A scan that quietly ignored 40 EPUBs and
 * said "done" is the failure this project keeps writing rules against, and
 * burying it under the achievement is the readable version of the same
 * thing. So the fields naming what did *not* happen lead each section and
 * the counts of what did follow them.
 *
 * `data-field` on every row is what the test counts. A count rather than a
 * list of names: the failure being guarded is a field silently dropped, and
 * a test that enumerates the fields it expects has the same hole it is
 * checking for.
 */
import type { Declined, Derived, Orphan, ScanOutcome, Walk } from './config'
import { scanFailed } from './config'

/** One count, with the key the test counts on. */
function Count({
  field,
  label,
  n,
}: {
  field: string
  label: string
  n: number
}) {
  return (
    <div className="count" data-field={field}>
      <dt>{label}</dt>
      <dd>{n}</dd>
    </div>
  )
}

/**
 * One list of paths, drawn as its length and then its members.
 *
 * The length is drawn even when it is zero, which is the whole point: an
 * empty list is a fact this report is making, not a section to skip.
 */
function Paths({
  field,
  label,
  paths,
}: {
  field: string
  label: string
  paths: string[]
}) {
  return (
    <div className="paths" data-field={field}>
      <dt>{label}</dt>
      <dd>
        {paths.length}
        {paths.length > 0 && (
          <ul>
            {paths.map((path, index) => (
              // `index` alongside `path`: `Excluder::build` can emit two
              // byte-identical complaints (two identical bad lines), and
              // `bad_exclude_patterns` is exactly the field that renders
              // through this component — a plain `key={path}` collides for
              // that case, silently dropping one row from React's own
              // reconciliation rather than the list this component drew.
              <li key={`${index}:${path}`}>
                <code>{path}</code>
              </li>
            ))}
          </ul>
        )}
      </dd>
    </div>
  )
}

/** The orphaned documents, each naming the work that absorbed its own. */
function Orphans({ orphans }: { orphans: Orphan[] }) {
  return (
    <div className="paths" data-field="orphan_documents">
      <dt>Orphaned documents</dt>
      <dd>
        {orphans.length}
        {orphans.length > 0 && (
          <ul>
            {orphans.map((orphan) => (
              <li key={orphan.path}>
                <code>{orphan.path}</code>
                {orphan.title !== null && <> — {orphan.title}</>}
                {orphan.absorbed_into !== null && (
                  <> · absorbed into {orphan.absorbed_into.title}</>
                )}
              </li>
            ))}
          </ul>
        )}
      </dd>
    </div>
  )
}

/** The documents a root no longer keeps — declined, not orphaned. */
function Declines({ declined }: { declined: Declined[] }) {
  return (
    <div className="paths" data-field="declined_documents">
      <dt>Documents this root no longer keeps</dt>
      <dd>
        {declined.length}
        {declined.length > 0 && (
          <ul>
            {declined.map((one) => (
              <li key={one.path}>
                <code>{one.path}</code>
                {one.title !== null && <> — {one.title}</>}
              </li>
            ))}
          </ul>
        )}
      </dd>
    </div>
  )
}

function WalkSection({ walk }: { walk: Walk }) {
  return (
    <section className="walk">
      <h3>What the walk found</h3>
      {walk.refused !== null && (
        <p role="alert" className="refused">
          This scan was refused and nothing was applied: {walk.refused}
        </p>
      )}
      <dl>
        <Paths
          field="unreadable_dirs"
          label="Directories it could not read"
          paths={walk.unreadable_dirs}
        />
        <Paths
          field="excluded_dirs"
          label="Directories excluded"
          paths={walk.excluded_dirs}
        />
        <Paths
          field="bad_exclude_patterns"
          label="Unusable exclude patterns"
          paths={walk.bad_exclude_patterns}
        />
        <Count
          field="excluded_files"
          label="Files excluded"
          n={walk.excluded_files}
        />
        <Count
          field="forgotten_by_exclusion"
          label="Forgotten because excluded"
          n={walk.forgotten_by_exclusion}
        />
        <div className="count" data-field="refused">
          <dt>Refused</dt>
          <dd>{walk.refused === null ? 'no' : 'yes'}</dd>
        </div>
        <Count field="dirs" label="Directories" n={walk.dirs} />
        <Count field="files" label="Files" n={walk.files} />
        <Count field="added" label="Added" n={walk.added} />
        <Count field="updated" label="Updated" n={walk.updated} />
        <Count field="gone" label="Gone" n={walk.gone} />
        <div className="count" data-field="root">
          <dt>Root</dt>
          <dd>
            <code>{walk.root}</code>
          </dd>
        </div>
      </dl>
    </section>
  )
}

/**
 * What derivation made of a pass, on its own.
 *
 * **Exported, because a root removal re-derives too.** `RemovalAnswer` carries
 * a whole `DeriveReport` — the server's own whole-branch review added the key
 * so that "nothing a removal's re-derive found ever reached a caller" would
 * stop being true — and `RemoveRoot` rendering only its three counts put that
 * straight back. One component, two callers, rather than a second rendering of
 * twenty-nine fields that could drift from this one.
 */
export function DeriveSection({ derive }: { derive: Derived }) {
  return (
    <section className="derived">
      <h3>What derivation made of it</h3>
      <dl>
        {/* What it did not do, first. */}
        <Count field="unreadable" label="Unreadable" n={derive.unreadable} />
        <Count field="suppressed" label="Suppressed" n={derive.suppressed} />
        <Count
          field="unrecognised_kinds"
          label="Unrecognised kinds"
          n={derive.unrecognised_kinds}
        />
        <Count
          field="unattached_chapter_dirs"
          label="Unattached chapter directories"
          n={derive.unattached_chapter_dirs}
        />
        <Count
          field="lost_positions"
          label="Reading positions lost"
          n={derive.lost_positions}
        />
        <Count
          field="discarded_rows"
          label="Rows discarded"
          n={derive.discarded_rows}
        />
        <Count
          field="series_holding_no_work"
          label="Series holding no work"
          n={derive.series_holding_no_work}
        />
        <Count
          field="root_removed_mid_derive"
          label="Roots removed mid-derive"
          n={derive.root_removed_mid_derive}
        />
        <Count
          field="moved_truth_collisions"
          label="Truth-page collisions"
          n={derive.moved_truth_collisions}
        />
        <Paths
          field="unfoldable"
          label="Could not fold"
          paths={derive.unfoldable}
        />
        <Paths
          field="unreadable_hand"
          label="Unreadable documents"
          paths={derive.unreadable_hand}
        />
        <Paths
          field="unwritable_documents"
          label="Documents it could not write"
          paths={derive.unwritable_documents}
        />
        <Paths
          field="stale_documents"
          label="Stale documents"
          paths={derive.stale_documents}
        />
        <Orphans orphans={derive.orphan_documents} />
        <Declines declined={derive.declined_documents} />

        {/* Then what it did. */}
        <Count field="series" label="Series" n={derive.series} />
        <Count field="works" label="Works" n={derive.works} />
        <Count field="editions" label="Editions" n={derive.editions} />
        <Count field="members" label="Members" n={derive.members} />
        <Count field="merged" label="Merged" n={derive.merged} />
        <Count field="rekeyed" label="Rekeyed" n={derive.rekeyed} />
        <Count
          field="kept_editions"
          label="Editions kept"
          n={derive.kept_editions}
        />
        <Count
          field="emptied_works"
          label="Works emptied"
          n={derive.emptied_works}
        />
        <Count
          field="unfolded_works"
          label="Works unfolded"
          n={derive.unfolded_works}
        />
        <Count
          field="repaired_documents"
          label="Documents repaired"
          n={derive.repaired_documents}
        />
        <Count
          field="moved_positions"
          label="Reading positions moved"
          n={derive.moved_positions}
        />
        <Count field="moved_notes" label="Notes moved" n={derive.moved_notes} />
        <Count
          field="moved_truth_pages"
          label="Truth pages moved"
          n={derive.moved_truth_pages}
        />
        <Count field="raised" label="Proposals raised" n={derive.raised} />
      </dl>
    </section>
  )
}

export function ScanReport({ outcome }: { outcome: ScanOutcome }) {
  if (scanFailed(outcome)) {
    return (
      <div className="scan-report">
        <p role="alert">{outcome.error}</p>
      </div>
    )
  }
  return (
    <div className="scan-report">
      <WalkSection walk={outcome.scan} />
      <DeriveSection derive={outcome.derive} />
      <p className="note">{outcome.note}</p>
    </div>
  )
}
