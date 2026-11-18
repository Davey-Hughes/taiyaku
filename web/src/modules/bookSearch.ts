import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'book-search:toggle'
export const NEXT_INTENT = 'book-search:next'
export const PREVIOUS_INTENT = 'book-search:previous'

/** The book reader's find in a book: a key to open it, two to step, nothing to tune. */
export const bookSearchModule: ReaderModule<Record<string, never>> = {
  id: 'book-search',
  label: 'Search',
  defaults: {},
  keys: [
    { key: '/', intent: TOGGLE_INTENT, label: 'search the book' },
    { key: ']', intent: NEXT_INTENT, label: 'the next match of the search' },
    {
      key: '[',
      intent: PREVIOUS_INTENT,
      label: 'the match of the search before',
    },
  ],
}
