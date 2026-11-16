import type { ReaderModule } from '../module'

export const NEW_INTENT = 'book-notes:new'
export const LIST_INTENT = 'book-notes:list'
export const UNDO_INTENT = 'book-notes:undo'
export const REDO_INTENT = 'book-notes:redo'
export const PIN_INTENT = 'book-pins:pick'

/** The book reader's notes, keys and nothing to tune, as the page reader's `n`, `a`, `z` and `Z`. */
export const bookNotesModule: ReaderModule<Record<string, never>> = {
  id: 'book-notes',
  label: 'Notes',
  defaults: {},
  keys: [
    {
      key: 'n',
      intent: NEW_INTENT,
      label: 'write a note on the paragraph at the top',
    },
    { key: 'a', intent: LIST_INTENT, label: 'list every note on this book' },
    { key: 'z', intent: UNDO_INTENT, label: 'undo the last note edit' },
    { key: 'Z', intent: REDO_INTENT, label: 'redo the last undone note edit' },
  ],
}

/** Pinning a passage of two editions compared to its match, an administrator's. */
export const bookPinsModule: ReaderModule<Record<string, never>> = {
  id: 'book-pins',
  label: 'Pins',
  defaults: {},
  keys: [
    {
      key: 'c',
      intent: PIN_INTENT,
      label:
        'compared, pin a paragraph on the left to its match on the right (an administrator)',
    },
  ],
}
