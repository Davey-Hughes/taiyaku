/**
 * What a note's colour name looks like.
 *
 * A note stores a **name** (`amber`, `rose`) rather than pixels, so the file
 * beside the sidecar stays free of presentation — the same discipline that
 * keeps `taiyaku.json` a document about correspondence. This module is the one
 * place that turns a name into colour, which means a native shell, a printout
 * or a dark theme can answer differently without touching a single stored
 * note.
 *
 * Five names rather than a colour picker. A reader marking up a page wants to
 * tell one note from another at a glance, which needs colours that are
 * *distinguishable*, not colours that are arbitrary — and an arbitrary picker
 * reliably produces a pale yellow on a white page. Every entry here was chosen
 * to hold against both the white of a manga page and the black of its ink.
 */
export interface NoteColour {
  /** The margin tab, solid. */
  tab: string
  /** The same colour at tab strength, for a travelled note's outline. */
  edge: string
  /** The region wash, which artwork has to stay readable through. */
  tint: string
}

const PALETTE: Record<string, NoteColour> = {
  amber: {
    tab: 'rgba(255, 214, 102, 0.92)',
    edge: 'rgba(255, 214, 102, 1)',
    tint: 'rgba(255, 214, 102, 0.22)',
  },
  rose: {
    tab: 'rgba(255, 138, 160, 0.92)',
    edge: 'rgba(255, 138, 160, 1)',
    tint: 'rgba(255, 138, 160, 0.22)',
  },
  violet: {
    tab: 'rgba(190, 150, 255, 0.92)',
    edge: 'rgba(190, 150, 255, 1)',
    tint: 'rgba(190, 150, 255, 0.24)',
  },
  sky: {
    tab: 'rgba(120, 199, 255, 0.92)',
    edge: 'rgba(120, 199, 255, 1)',
    tint: 'rgba(120, 199, 255, 0.24)',
  },
  mint: {
    tab: 'rgba(126, 231, 179, 0.92)',
    edge: 'rgba(126, 231, 179, 1)',
    tint: 'rgba(126, 231, 179, 0.24)',
  },
}

/** The colour a note gets when it names none — every note written before colours existed. */
export const DEFAULT_COLOUR = 'amber'

/** Every name this build offers, in the order the picker shows them. */
export const COLOUR_NAMES = Object.keys(PALETTE)

/**
 * The pixels for a colour name.
 *
 * **An unknown name falls back to the default rather than refusing.** That is
 * the opposite of what this project does almost everywhere else, and it is
 * deliberate here: a note may name a colour written by a later build or by
 * hand, and the alternative to falling back is a note that does not draw at
 * all. Losing sight of a note is far worse than seeing it in the wrong colour,
 * and the server has already refused anything that is not a name.
 */
export function noteColour(name: string | undefined): NoteColour {
  return PALETTE[name ?? DEFAULT_COLOUR] ?? PALETTE[DEFAULT_COLOUR]
}

/**
 * The colour a new note should take, given the ones already on the page.
 *
 * The first name nobody has used, so notes on one page come out visibly
 * different without a reader choosing; once every colour is in use it cycles,
 * handing back the least-used rather than giving up and repeating the first.
 * Ties break in palette order, so the sequence is the same every time rather
 * than depending on which note happened to be created first.
 *
 * `undefined` counts as the default, because that is what an absent colour
 * means — a note written before colours existed occupies `amber`, and the next
 * new note beside it should not land on amber too.
 *
 * A name this build does not know counts as nothing: it cannot be one of ours,
 * and letting it consume a slot would leave a gap in the cycle for a colour
 * nobody can see.
 */
export function nextColour(used: (string | undefined)[]): string {
  const counts = new Map(COLOUR_NAMES.map((name) => [name, 0]))
  for (const name of used) {
    const key = name ?? DEFAULT_COLOUR
    if (counts.has(key)) counts.set(key, (counts.get(key) ?? 0) + 1)
  }
  let best = DEFAULT_COLOUR
  for (const name of COLOUR_NAMES) {
    if ((counts.get(name) ?? 0) < (counts.get(best) ?? 0)) best = name
  }
  return best
}
