import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'lookup:toggle'

/**
 * Whether a tap on a word in a book looks it up rather than showing the bars,
 * kept in this browser and off until turned on; and whether the characters a
 * look-up names are told of with what the earlier books of their series said
 * of them. The settings panel labels a field from its key, so the key is
 * named for the label it is to read as.
 */
export const lookupModule: ReaderModule<{
  on: boolean
  includeEarlierBooks: boolean
}> = {
  id: 'lookup',
  label: 'Look up words',
  defaults: { on: false, includeEarlierBooks: true },
  keys: [
    {
      key: 'l',
      intent: TOGGLE_INTENT,
      label: 'look up a word with a tap on it, until l again',
    },
  ],
}
