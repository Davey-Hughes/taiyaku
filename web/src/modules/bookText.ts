import type { ReaderModule } from '../module'

/**
 * How a book's text is set: its size in pixels, the space between its lines
 * as a multiple of that size, the column's width in ems, the typeface, whether
 * lines are justified and hyphenated, whether a paragraph opens indented
 * rather than spaced from the last, whether a ruby's reading is drawn over
 * its base, the page's colours, and whether a Japanese edition is set
 * vertically, top to bottom and right to left.
 */
export interface BookTextPrefs {
  textSize: number
  lineSpacing: number
  columnWidth: number
  typeface: string
  justify: boolean
  indent: boolean
  furigana: boolean
  page: string
  vertical: boolean
}

export const LARGER_INTENT = 'book-text:larger'
export const SMALLER_INTENT = 'book-text:smaller'
export const USUAL_INTENT = 'book-text:usual'

/** The intents that set the text's size. */
export const TEXT_SIZE_INTENTS: ReadonlySet<string> = new Set([
  LARGER_INTENT,
  SMALLER_INTENT,
  USUAL_INTENT,
])

/** How far one press moves the text size, and the sizes it stays between, in pixels. */
const STEP = 2
const SMALLEST = 12
const LARGEST = 40

export const bookTextModule: ReaderModule<BookTextPrefs> = {
  id: 'book-text',
  label: 'Text',
  keys: [
    { key: '=', intent: LARGER_INTENT, label: 'larger text' },
    { key: '+', intent: LARGER_INTENT, label: 'larger text' },
    { key: '-', intent: SMALLER_INTENT, label: 'smaller text' },
    { key: '0', intent: USUAL_INTENT, label: 'text at its usual size' },
  ],
  defaults: {
    textSize: 18,
    lineSpacing: 1.6,
    columnWidth: 34,
    typeface: 'serif',
    justify: true,
    indent: true,
    furigana: true,
    page: 'dark',
    vertical: false,
  },
  options: {
    typeface: ['serif', 'sans'],
    page: ['dark', 'paper'],
  },
}

/** `prefs` with the text size `intent` asks for, or unchanged for any other intent. */
export function resized(prefs: BookTextPrefs, intent: string): BookTextPrefs {
  if (intent === LARGER_INTENT) return stepped(prefs, 1)
  if (intent === SMALLER_INTENT) return stepped(prefs, -1)
  if (intent === USUAL_INTENT) {
    return { ...prefs, textSize: bookTextModule.defaults.textSize }
  }
  return prefs
}

/** `prefs` with the text `steps` steps larger, or smaller for a negative count. */
export function stepped(prefs: BookTextPrefs, steps: number): BookTextPrefs {
  const to = prefs.textSize + steps * STEP
  return { ...prefs, textSize: Math.min(LARGEST, Math.max(SMALLEST, to)) }
}
