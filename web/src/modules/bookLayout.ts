import { BOOK_LAYOUT_WORDS, BOOK_LAYOUTS, type BookLayout } from '../layouts'
import type { ReaderModule } from '../module'

/**
 * How a book is laid out, kept as the page reader's `layout` is: one edition,
 * two compared, or one interlinear, either scrolled or turned page by page;
 * and whether an interlinear translation waits for a tap, to test oneself.
 */
export interface BookLayoutPrefs {
  mode: BookLayout
  paged: boolean
  hideTranslation: boolean
}

export const bookLayoutModule: ReaderModule<BookLayoutPrefs> = {
  id: 'book-layout',
  label: 'Layout',
  defaults: { mode: 'scroll', paged: false, hideTranslation: false },
  options: { mode: [...BOOK_LAYOUTS] },
  optionWords: { mode: BOOK_LAYOUT_WORDS },
}
