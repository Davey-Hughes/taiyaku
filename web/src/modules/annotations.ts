import type { ReaderModule } from '../module'

export interface AnnotationsPrefs {
  /** Draw a margin tab per note on the page being read. */
  markers: boolean
  /** Show notes written on other editions, resolved through the alignment. */
  travel: boolean
  /**
   * Show every annotated region on this page — the box and the words in it.
   *
   * One switch, not two. The words briefly had their own (`labels`, `t`), on
   * the argument that a tint and a text block draw different amounts over the
   * artwork; in use that was a distinction without a decision, because the
   * margin tabs already say *where* the notes are without drawing on the page
   * at all. So what this answers is the only question left: show me what they
   * say, or get out of the way.
   */
  reveal: boolean
  /**
   * Where the list of this page's notes goes while they are revealed:
   * `auto`, `side` or `below`.
   *
   * `auto` answers by the width of the window — beside the page where there is
   * margin for it, under the page where there is not. A reader whose screen
   * `auto` guesses wrong on can say so rather than living with it, which is
   * what makes this the module contract's first declared-options setting.
   */
  notesList: string
}

/** `n` — start a note on the slot being read. */
export const NEW_INTENT = 'annotations:new'
/** `r` — show the annotated regions and what they say, for as long as it is switched on. */
export const REVEAL_INTENT = 'annotations:reveal'
/** `a` — the review list: every note on the work, including the unreachable ones. */
export const REVIEW_INTENT = 'annotations:review'
/** `z` — step back through what has been written, and `Z` to step forward again. */
export const UNDO_INTENT = 'annotations:undo'
export const REDO_INTENT = 'annotations:redo'

/**
 * Notes on the page, and the three gestures they need.
 *
 * `travel` defaults on because a note written on the raw being readable while
 * reading the English is the entire feature — the one thing no other reader
 * can offer, and the sidecar earning its keep a second time. `reveal` defaults
 * off: always-tinted regions are intrusive on a busy page, which is why the
 * reveal is a key rather than a permanent state.
 *
 * Five keys where every other module claims at most one, because these are
 * distinct acts rather than settings — writing, looking, finding, and stepping
 * back and forth through what has been written — each something a reader does
 * mid-read without wanting the settings panel. Tunability stays in `defaults`,
 * as everywhere else.
 *
 * `z`/`Z` rather than the usual `ctrl+z`: `intentOf` yields any key carrying a
 * modifier, deliberately, so that a reader's own browser and OS shortcuts
 * always win — which means `ctrl+z` never reaches this reader at all, and
 * leaves the textarea's own undo working while a note is being typed.
 *
 * The one that draws over the page is off by default: the margin tabs already
 * say where the notes are, so a reader who wants the page itself gets it, and
 * `r` is there for the moment they want to read what they wrote.
 */
export const annotationsModule: ReaderModule<AnnotationsPrefs> = {
  id: 'annotations',
  label: 'Annotations',
  defaults: { markers: true, travel: true, reveal: false, notesList: 'auto' },
  options: { notesList: ['auto', 'side', 'below'] },
  keys: [
    { key: 'n', intent: NEW_INTENT, label: 'write a note on this page' },
    {
      key: 'r',
      intent: REVEAL_INTENT,
      label: 'tint the regions notes are anchored to',
    },
    { key: 'a', intent: REVIEW_INTENT, label: 'list every note on this work' },
    { key: 'z', intent: UNDO_INTENT, label: 'undo the last note edit' },
    { key: 'Z', intent: REDO_INTENT, label: 'redo the last undone note edit' },
  ],
}

/**
 * The readout's note count: how many are on this page, out of the work's own.
 *
 * Nothing at all where the work has no notes — a reader who has never
 * annotated anything should not carry a permanent `no notes here of 0`, the
 * same reasoning `healthReadout` uses to say nothing about a sound pair with a
 * counterpart. Once some exist, a page with none says `no notes here` rather
 * than `0 notes here`, because a zero in a row of numbers reads like a figure
 * that failed to compute rather than a fact.
 */
export function noteCountLine(here: number, total: number): string | undefined {
  if (total === 0) return undefined
  const count = here === 0 ? 'no notes' : `${here} note${here === 1 ? '' : 's'}`
  return `${count} here of ${total}`
}
