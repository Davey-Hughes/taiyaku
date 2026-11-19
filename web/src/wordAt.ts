/**
 * The word a tap lands in, for a language whose words a space or punctuation
 * ends, and the run of text a Japanese tap sends the dictionary to find its
 * longest word in. Offsets are UTF-16, as a block's are.
 */

const LETTER = /^[\p{L}\p{M}]/u

/** What joins two runs of letters into one word: l'homme, l’homme, well-read. */
const JOINERS = new Set(["'", '’', '-'])

/** Whether the character starting at UTF-16 offset `i` is a letter or a mark. */
const letterAt = (text: string, i: number) =>
  i >= 0 && i < text.length && LETTER.test(text.slice(i, i + 2))

/** Where the character holding UTF-16 offset `i` starts, so half a pair counts as the pair. */
const charStart = (text: string, i: number) =>
  i > 0 &&
  /[\uDC00-\uDFFF]/.test(text[i]) &&
  /[\uD800-\uDBFF]/.test(text[i - 1])
    ? i - 1
    : i

/** Where the character before the one starting at `i` starts. */
const before = (text: string, i: number) => charStart(text, i - 1)

/** Where the character after the one starting at `i` starts. */
const after = (text: string, i: number) =>
  i + ((text.codePointAt(i) ?? 0) > 0xffff ? 2 : 1)

/**
 * The run of letters and marks around offset `at`, an apostrophe or hyphen
 * between two letters kept in it; none where `at` is on a space or
 * punctuation.
 */
export function wordAt(text: string, at: number): [number, number] | undefined {
  const from = charStart(text, at)
  if (!letterAt(text, from)) return undefined
  let start = from
  for (;;) {
    const prev = before(text, start)
    if (letterAt(text, prev)) start = prev
    else if (JOINERS.has(text[prev]) && letterAt(text, before(text, prev))) {
      start = before(text, prev)
    } else break
  }
  let end = after(text, from)
  for (;;) {
    if (letterAt(text, end)) end = after(text, end)
    else if (JOINERS.has(text[end]) && letterAt(text, end + 1)) {
      end = after(text, end + 1)
    } else break
  }
  return [start, end]
}

/** Up to `most` characters of `text` from offset `at`, counted in code points as the server caps them. */
export function headAt(text: string, at: number, most = 16): string {
  return Array.from(text.slice(at, at + most * 2))
    .slice(0, most)
    .join('')
}

/**
 * What the server's longest-word search goes on through, `ends_word` in the
 * dictionary crate: what Rust calls alphanumeric, the Alphabetic property and
 * the numbers, then the diacritics, the kana voicing marks and the variation
 * selectors. The two must agree, or a tap opens a look-up the server cannot
 * answer.
 */
const WORD_GOES_ON =
  /^(?:[\p{Alphabetic}\p{N}]|[\u0300-\u036F]|\u3099|\u309A|[\uFE00-\uFE0F]|[\u{E0100}-\u{E01EF}])/u

/**
 * Whether a dictionary word can begin at offset `at`: on a character the
 * server's longest-word search goes on through. A tap on any other, a space,
 * punctuation, a symbol or a format character such as a zero-width space,
 * keeps its usual meaning.
 */
export function beginsWord(text: string, at: number): boolean {
  return WORD_GOES_ON.test(text.slice(at, at + 2))
}
