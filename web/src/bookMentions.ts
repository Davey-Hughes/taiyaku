/**
 * The other places the text on screen names the character a look-up's card
 * is for, marked while the card shows, and the word looked up itself. The
 * mentions are found here, in the reader, by the names the card gives that
 * are that character's alone, so some are missed: one by surname, or by a
 * title. Marked with the CSS Custom Highlight API, which paints over the
 * words where they lie: a span round each would break lines anew, and split
 * the text nodes that selections, notes and the reader's place count by.
 */
import type { Block, CharacterCard } from './book'
import { rangeOf } from './bookNotes'
import { topBlock } from './bookPages'
import type { TextRange } from './notes'

/** The highlights' names, which `::highlight()` in `book.css` styles. */
const MENTION = 'book-mention'
const LOOKED_UP = 'book-lookup-word'

/**
 * A single word all in lower case, which a book's log can give as a name
 * ("boy"): the common noun, which the text uses of anyone.
 */
const COMMON_NOUN = /^\p{Ll}+$/u

/** What a word is made of. */
const WORD = '[\\p{L}\\p{M}\\p{N}]'

/**
 * Whether a name's first character must begin a word: in every script that
 * sets words apart, Hangul among them, where a name found inside a longer
 * word is that word and often another person's name. Not kana or Han, which
 * write a name straight after the word before.
 */
const OPENS =
  /^(?![\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}])[\p{L}\p{N}]$/u

/**
 * Whether a name's last character must end a word. Not Hangul, kana or Han:
 * a particle is written onto a name there.
 */
const CLOSES =
  /^(?![\p{Script=Hangul}\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}])[\p{L}\p{N}]$/u

/** The source of what finds `name`, in its own case, as the name it is. */
function finds(name: string): string {
  const letters = Array.from(name)
  const words = name
    .split(/\s+/u)
    .map((word) => word.replace(/[\\^$.*+?()[\]{}|/]/g, '\\$&'))
    // A line the book breaks inside a name is still the name.
    .join('\\s+')
  const opens = OPENS.test(letters[0]) ? `(?<!${WORD})` : ''
  const closes = CLOSES.test(letters.at(-1) ?? '') ? `(?!${WORD})` : ''
  return `${opens}${words}${closes}`
}

/** Whether `text` has `name` in it, as `finds` finds it. */
const holds = (text: string, name: string) =>
  new RegExp(finds(name), 'u').test(text)

/**
 * The names that are the first of `cards`' character's alone. A mark on
 * someone else is worse than a mention missed, so what a family shares is
 * left out: the surname, and for a character with a given name a title with
 * it, unless that title and surname are the shown name and the very text
 * looked up, which then says who is meant. With a given name: it, and the
 * shown name and aliases that carry it; with none: the shown name and
 * aliases that carry all of it. None where the look-up named several by
 * neither the first one's shown name nor its given name.
 */
function ownNames(cards: CharacterCard[]): string[] {
  const card = cards.at(0)
  if (!card) return []
  const shown = card.name.text.trim()
  const given = card.name.given?.trim() ?? ''
  const surname = card.name.surname?.trim() ?? ''
  const aliases = card.aliases.map((alias) => alias.text.trim())
  const { part, text: matched } = card.matched
  // As the server sent the two, which is how it would say they are the same.
  // Matched whole: by a part, the server still answers the name the part is
  // of, which is not the text looked up.
  const lookedUpShown = part === 'whole' && matched === card.name.text
  const several = cards.length > 1 && !lookedUpShown
  if (given === '') {
    if (several || shown === '' || shown === surname) return []
    return [shown, ...aliases.filter((alias) => holds(alias, shown))]
  }
  if (several && (part === 'surname' || !holds(matched, given))) return []
  // A shown name with no given name in it is still this character's alone
  // ("Old Wickhand"), unless what it has is the surname ("Mrs. Vell"),
  // which is theirs alone only as the text looked up, and never bare.
  const own = (name: string) =>
    holds(name, given) ||
    (name === shown &&
      name !== surname &&
      (lookedUpShown || !(surname !== '' && holds(name, surname))))
  return [
    given,
    // As one run, where the shown name is neither.
    ...(surname === '' ? [] : [`${given} ${surname}`]),
    ...[shown, ...aliases].filter(own),
  ]
}

/**
 * What finds, in the text, the names `ownNames` gives for `cards`. In a
 * script that spaces its words a name is a whole word, in its own case; in
 * Hangul it begins one, and a particle may follow. The longest name is tried
 * first, so the full name is one mention and not its two halves. None where
 * there is no name to look for.
 */
export function mentionPattern(cards: CharacterCard[]): RegExp | undefined {
  const names = ownNames(cards).filter(
    (name) => name !== '' && !COMMON_NOUN.test(name),
  )
  if (names.length === 0) return undefined
  const pieces = [...new Set(names)]
    .sort((a, b) => b.length - a.length)
    .map(finds)
  return new RegExp(pieces.join('|'), 'gu')
}

/** Where `text` has a name `names` finds, leftmost first, none overlapping. */
export function mentionsIn(text: string, names: RegExp): TextRange[] {
  return Array.from(text.matchAll(names), (found) => ({
    start: found.index,
    end: found.index + found[0].length,
  }))
}

/**
 * The blocks of `text` on its screen and within a screen of it either way,
 * in order. The whole book is drawn, so only these are searched; the screen
 * beside is taken in so that a page turned, or a line scrolled in, shows its
 * marks already there.
 */
export function blocksNear(text: HTMLElement): HTMLElement[] {
  const top = text.querySelector<HTMLElement>(
    `:scope > [data-block="${topBlock(text)}"]`,
  )
  if (!top) return []
  const screen = text.getBoundingClientRect()
  const near = (block: Element) => {
    const box = block.getBoundingClientRect()
    return (
      box.right > screen.left - screen.width &&
      box.left < screen.right + screen.width &&
      box.bottom > screen.top - screen.height &&
      box.top < screen.bottom + screen.height
    )
  }
  // A figure between two blocks is passed over, as it names no block.
  const run = (from: Element | null, way: 'next' | 'previous') => {
    const out: HTMLElement[] = []
    for (let at = from; at; at = at[`${way}ElementSibling`]) {
      if (!(at instanceof HTMLElement) || at.dataset.block === undefined) {
        continue
      }
      if (!near(at)) break
      out.push(at)
    }
    return out
  }
  return [
    ...run(top.previousElementSibling, 'previous').reverse(),
    ...run(top, 'next'),
  ]
}

/**
 * What of the mention `found` in `text` lies outside `lit`, less the space
 * between the two: the first name before a surname looked up alone, which
 * the light cannot run back over, is still a mention of its character.
 */
function outside(text: string, found: TextRange, lit: TextRange): TextRange[] {
  if (found.start >= lit.end || found.end <= lit.start) return [found]
  const before = text.slice(found.start, Math.max(found.start, lit.start))
  const after = text.slice(Math.min(found.end, lit.end), found.end)
  return [
    { start: found.start, end: found.start + before.trimEnd().length },
    { start: found.end - after.trimStart().length, end: found.end },
  ].filter((part) => part.start < part.end)
}

/**
 * A DOM range over each name `names` finds in `near`, blocks of an edition
 * whose blocks are `blocks`. Not the words `lit` covers, which the look-up
 * has lit already as the one asked about. A compared row draws no block on
 * its own, and so has none.
 */
export function mentionRanges(
  near: HTMLElement[],
  blocks: Block[],
  names: RegExp,
  lit?: { block: number; range: TextRange },
): Range[] {
  const out: Range[] = []
  for (const drawn of near) {
    const block = Number(drawn.dataset.block)
    const own = drawn.querySelector(':scope > [data-text]')
    const text = blocks[block]?.text
    if (!own || text === undefined) continue
    for (const found of mentionsIn(text, names)) {
      const parts =
        lit?.block === block ? outside(text, found, lit.range) : [found]
      for (const part of parts) {
        const range = rangeOf(own, part)
        if (range) out.push(range)
      }
    }
  }
  return out
}

/** Sets the highlight `name` to `ranges`, or with none, removes it. Nothing where the browser has no custom highlights. */
function mark(name: string, ranges: Range[]) {
  if (typeof CSS === 'undefined' || typeof Highlight === 'undefined') return
  const marks = CSS.highlights
  if (!marks) return
  if (ranges.length === 0) marks.delete(name)
  else marks.set(name, new Highlight(...ranges))
}

/**
 * Marks `ranges` as mentions, in place of any marked before; none clears
 * them. The card is whole without them, where the browser has no custom
 * highlights.
 */
export function markMentions(ranges: Range[]) {
  mark(MENTION, ranges)
}

/**
 * Marks `range` as the word the look-up is on, or with none, no word. The
 * selection lights that word too, and is the reader's to take: a press in
 * the look-up's panel moves it there. This is what keeps the word lit while
 * the panel is open.
 */
export function markLookedUp(range: Range | undefined) {
  mark(LOOKED_UP, range ? [range] : [])
}
