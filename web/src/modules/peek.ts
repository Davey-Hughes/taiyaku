import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'peek:toggle'

/** The book reader's peek, a key and nothing to tune. */
export const peekModule: ReaderModule<Record<string, never>> = {
  id: 'peek',
  label: 'Peek',
  defaults: {},
  keys: [
    {
      key: 'p',
      intent: TOGGLE_INTENT,
      label:
        'peek at the next edition under the paragraph at the top; double-click a paragraph for that one',
    },
  ],
}
