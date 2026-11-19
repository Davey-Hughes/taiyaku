import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'lookup:toggle'

/**
 * Whether a tap on a word in a book looks it up rather than showing the bars,
 * kept in this browser and off until turned on.
 */
export const lookupModule: ReaderModule<{ on: boolean }> = {
  id: 'lookup',
  label: 'Look up words',
  defaults: { on: false },
  keys: [
    {
      key: 'l',
      intent: TOGGLE_INTENT,
      label: 'look up a word with a tap on it, until l again',
    },
  ],
}
