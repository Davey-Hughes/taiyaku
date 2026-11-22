/**
 * A volume's `i` panel as drawn (O283), from `volumePanel.ts`'s state: the
 * Title form and its stamp section, the alignment detail and an
 * administrator's content match marks.
 */
import { type ReactNode, useEffect, useRef, useState } from 'react'
import { useGate } from './gate'
import { HeldButton } from './HeldButton'
import type {
  AlignmentCard,
  ContentMatch,
  EditionCard,
  KeptApart,
  WorkCard,
} from './library'
import { splitMatch, undoSplit } from './matches'
import { MetadataEditor } from './MetadataEditor'
import { type PatchResult, patchWork, type WorkEdit } from './metadata'
import { sentenceOf } from './message'
import { invalidateLibraryViews } from './pageReads'
import { Refusal } from './Refusal'
import type { Landing, PanelState } from './volumePanel'

/**
 * `patchWork`, then every Library view address marked stale: the screen's
 * own read is sent again at once, so the volume's title follows the save, and
 * the grid, home, series and search kept for a later arrival are read then.
 * Only once the save has answered, and succeeded: a read sent before would
 * answer with the title from before it.
 */
async function saveWork(slug: string, fields: WorkEdit): Promise<PatchResult> {
  const saved = await patchWork(slug, fields)
  invalidateLibraryViews()
  return saved
}

/**
 * One volume's alignment detail (§3's panel), read from the work alone, since
 * a verdict per row of a list would cost a read per row. A pair with no
 * verdict says "not aligned": a blank would say "not loaded". With no
 * alignment, `editionCount` (the tile's own) tells one edition, nothing to
 * pair, from two or more not aligned yet, a normal state: a derive aligns only
 * editions whose files the match pass scored at or above its bar, whether
 * joined by content or by name, and `taiyaku align` aligns others.
 */
function AlignmentPanel({
  editionCount,
  state,
}: {
  editionCount: number
  state: { alignments: AlignmentCard[] } | { error: string } | undefined
}) {
  if (state === undefined)
    return (
      <div className="alignment-panel">
        <p>Reading…</p>
      </div>
    )
  if ('error' in state)
    return (
      <div className="alignment-panel">
        <p role="alert">{state.error}</p>
      </div>
    )
  if (state.alignments.length === 0)
    return (
      <div className="alignment-panel">
        <p>
          {editionCount < 2
            ? 'This volume has one edition, so there is nothing to pair it with.'
            : 'No alignment recorded.'}
        </p>
      </div>
    )
  return (
    <div className="alignment-panel">
      <ul>
        {state.alignments.map((a) => (
          <li key={`${a.from}→${a.to}`}>
            <span className="pair">
              {a.from} → {a.to}
            </span>{' '}
            {a.verdict === null ? (
              <span className="verdict">not aligned</span>
            ) : (
              <>
                <span className="verdict">{a.verdict}</span>
                {a.paired !== null && a.slots !== null && (
                  <span className="paired">
                    {' '}
                    · {a.paired} of {a.slots}
                  </span>
                )}
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}

/** A volume number as the series screens write it: `v05`, `v05.5`. */
function volumeTag(volume: number): string {
  const text = String(volume)
  const dot = text.indexOf('.')
  const whole = dot < 0 ? text : text.slice(0, dot)
  return `v${whole.padStart(2, '0')}${dot < 0 ? '' : text.slice(dot)}`
}

/** A marked edition: one whose own file a pair joined to this work. */
type Marked = EditionCard & { matched: ContentMatch }

/** One Split as the panel draws it: its pairs, and each volume it keeps this one apart from, once. */
interface KeptSplit {
  split: number
  pairs: number[]
  slugs: string[]
  from: string[]
}

/**
 * `kept` by the Split that set each pair apart, in the order drawn: one line
 * and one Undo per Split, as Undo restores a whole Split.
 */
function bySplit(kept: KeptApart[]): KeptSplit[] {
  const splits: KeptSplit[] = []
  for (const k of kept) {
    let one = splits.find((s) => s.split === k.split)
    if (one === undefined) {
      one = { split: k.split, pairs: [], slugs: [], from: [] }
      splits.push(one)
    }
    one.pairs.push(k.id)
    if (!one.slugs.includes(k.slug)) {
      one.slugs.push(k.slug)
      one.from.push(
        k.volume === null ? k.series : `${k.series} ${volumeTag(k.volume)}`,
      )
    }
  }
  return splits
}

/** `names` as a sentence lists them: "a", "a and b", "a, b and c". */
function listed(names: string[]): string {
  if (names.length < 2) return names.join('')
  return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`
}

/**
 * A volume's content matches, an administrator's (spec §6): each edition
 * joined by content with its score and Split, and each Split that kept the
 * volume apart with one Undo. Split asks first, since it moves an edition off
 * the volume at once; Undo only lets the derive join every pair of that Split
 * again. Each press derives the whole store, so `onAnswer` reads the views
 * drawn and the volume again. One press out at a time, held by a ref too: the
 * hold is drawn only after a render, and two presses inside one would send
 * twice.
 */
function MatchMarks({
  slug,
  editions,
  kept,
  onAnswer,
  focusPanel,
}: {
  slug: string
  editions: EditionCard[]
  kept: KeptApart[]
  onAnswer: (to: Landing) => Promise<void>
  focusPanel: () => void
}) {
  /** The key of the edition whose Split is asking to be confirmed. */
  const [asking, setAsking] = useState<string | undefined>()
  /** The press out: `split:<key>` or `undo:<pair>`. */
  const [out, setOut] = useState<string | undefined>()
  const sending = useRef(false)
  /** Why the last press was refused. */
  const [failure, setFailure] = useState<string | undefined>()
  const root = useRef<HTMLDivElement>(null)
  /** Set as Cancel closes a question: that edition's Split… takes focus back. */
  const backTo = useRef<string | undefined>(undefined)
  useEffect(() => {
    const key = backTo.current
    if (asking !== undefined || key === undefined) return
    backTo.current = undefined
    const splits = root.current?.querySelectorAll<HTMLButtonElement>(
      'button[name="split"]',
    )
    ;[...(splits ?? [])].find((b) => b.dataset.key === key)?.focus()
  }, [asking])
  const marked = editions.filter((e): e is Marked => e.matched != null)
  if (marked.length === 0 && kept.length === 0) return null

  const send = (what: string, press: () => Promise<Landing>) => {
    if (sending.current) return
    sending.current = true
    setOut(what)
    setFailure(undefined)
    press()
      .then(
        (to) => {
          // The question goes as the Split answers, and with it the button
          // that had focus, which the panel holds until the read lands.
          const question = root.current?.querySelector('.split-confirm')
          if (question?.contains(document.activeElement)) focusPanel()
          setAsking(undefined)
          return onAnswer(to)
        },
        (err: unknown) => {
          setFailure(sentenceOf(err))
        },
      )
      .finally(() => {
        sending.current = false
        setOut(undefined)
      })
  }

  return (
    <div className="match-marks" ref={root}>
      <ul>
        {marked.map((e) => (
          <li key={e.key}>
            <div className="mark">
              <p>
                <span className="pair">{e.key}</span> matched by content ·{' '}
                {e.matched.score.toFixed(2)}
              </p>
              <HeldButton
                type="button"
                name="split"
                data-key={e.key}
                data-pair={e.matched.id}
                held={out !== undefined || asking === e.key}
                onClick={() => {
                  setFailure(undefined)
                  setAsking(e.key)
                }}
              >
                Split…
              </HeldButton>
            </div>
            {asking === e.key && (
              <div
                className="split-confirm"
                role="group"
                aria-label={`Split ${e.key} off this volume`}
              >
                <p>
                  Split {e.key} off this volume? Its file goes back to where its
                  own name puts it, unless it matches another volume.
                </p>
                <div className="split-actions">
                  <HeldButton
                    type="button"
                    className="confirm"
                    name="confirm-split"
                    held={out !== undefined}
                    onClick={() => {
                      send(`split:${e.key}`, () =>
                        splitMatch(e.matched.id, slug, e.key).then(
                          (answer): Landing => ({
                            name: 'undo-split',
                            pairs: answer.split,
                          }),
                        ),
                      )
                    }}
                  >
                    {out === `split:${e.key}` ? 'Splitting…' : `Split ${e.key}`}
                  </HeldButton>
                  <HeldButton
                    type="button"
                    name="cancel-split"
                    held={out !== undefined}
                    onClick={() => {
                      backTo.current = e.key
                      setAsking(undefined)
                    }}
                  >
                    Cancel
                  </HeldButton>
                </div>
              </div>
            )}
          </li>
        ))}
        {bySplit(kept).map((s) => (
          <li key={`split-${s.split}`}>
            <div className="mark">
              <p>kept apart from {listed(s.from)}</p>
              <HeldButton
                type="button"
                name="undo-split"
                data-pair={s.pairs[0]}
                held={out !== undefined}
                onClick={() => {
                  send(`undo:${s.split}`, () =>
                    undoSplit(s.pairs[0]).then(
                      (): Landing => ({ name: 'split', pairs: s.pairs }),
                    ),
                  )
                }}
              >
                {out === `undo:${s.split}` ? 'Undoing…' : 'Undo'}
              </HeldButton>
            </div>
          </li>
        ))}
      </ul>
      {failure !== undefined && <Refusal>{failure}</Refusal>}
    </div>
  )
}

/**
 * The open volume's panel, `state.box`: the Title form and its stamp
 * section where the account may edit, the alignment detail and the content
 * match marks, then the screen's `foot`. One block, so that a pointer's
 * reveal can ask whether focus is in it. `id` is for a screen that draws
 * the panel away from its `i`, which then names it in `aria-controls`;
 * `onFocusCapture` and `onMouseEnter` for one that draws it outside its
 * tile, which then hears there what a tile hears of its nested panel.
 */
export function VolumePanel({
  state,
  work: w,
  foot,
  id,
  onFocusCapture,
  onMouseEnter,
}: {
  state: PanelState
  work: WorkCard
  foot?: ReactNode
  id?: string
  onFocusCapture?: () => void
  onMouseEnter?: (event: React.MouseEvent<HTMLDivElement>) => void
}) {
  // **Admin, because every metadata PATCH route is** (`routes.rs`'s grant
  // table). Rendering the affordance to a reader would invite a correction
  // and then answer 403, which is worse than not offering it.
  const mayEdit = useGate()?.who.admin === true
  // **The column alone, never `admin || can_stamp`.** O86 folds `admin` into
  // every capability that widens what an account may read; this one decides
  // whether it may rewrite files taiyaku does not own, which an admin role
  // does not answer for (`Me` in `api.ts`, design doc §1.1), and the server
  // answers that write with 403 to an administrator without the column.
  // `mayEdit` already requires `admin`, so the section needs both, which is
  // narrower than either alone, by intent.
  const mayStamp = useGate()?.who.can_stamp === true
  /** The open panel's detail, once it has been read. */
  const known = state.detail[w.slug]
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: focus and the pointer arriving choose the panel's tile, as that tile's `li` hears them on the series page; the panel is no control itself.
    <div
      ref={state.box}
      id={id}
      className="tile-panel"
      onFocusCapture={onFocusCapture}
      onMouseEnter={onMouseEnter}
    >
      {mayEdit && (
        // **In the panel, not beside the title** — C3 §5's "contextual, from
        // the thing being corrected": the correction is prompted by looking at
        // a volume, and this is the surface a volume already opens. There is no
        // metadata screen to navigate to.
        //
        // A save reads the screen's works again (`saveWork`), so the tile's
        // title follows it; the field keeps the title as typed (O283).
        <MetadataEditor
          // Keyed by volume, as the editor takes its title once: a panel
          // that moves to another tile draws a fresh one.
          key={w.slug}
          ref={state.editor}
          slug={w.slug}
          title={w.title}
          onPatch={saveWork}
          canStamp={mayStamp}
        />
      )}
      {mayEdit ? (
        // One section for how the volume's editions relate: the alignment
        // lines, then the content match marks under them, set off by a rule
        // above and below. Not one row per edition: an alignment is a stored
        // pair of any two keys (`browse::alignments_of`), not each edition
        // against the hub. A reader, sent no marks, keeps the bare alignment
        // detail.
        <section className="edition-group" aria-label="Editions">
          <AlignmentPanel editionCount={w.editions.length} state={known} />
          {known !== undefined && !('error' in known) && (
            // Keyed by volume, so a press's confirmation or refusal never shows
            // under another volume's marks.
            <MatchMarks
              key={w.slug}
              slug={w.slug}
              editions={known.editions}
              kept={known.kept_apart}
              onAnswer={(to) => state.rereadAfterMatch(w.slug, to)}
              focusPanel={state.focusPanel}
            />
          )}
        </section>
      ) : (
        <AlignmentPanel editionCount={w.editions.length} state={known} />
      )}
      {foot}
    </div>
  )
}
