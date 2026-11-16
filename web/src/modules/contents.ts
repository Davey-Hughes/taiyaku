import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'contents:toggle'

/** The book reader's contents panel, a key and nothing to tune. */
export const contentsModule: ReaderModule<Record<string, never>> = {
  id: 'contents',
  label: 'Contents',
  defaults: {},
  keys: [
    {
      key: 't',
      intent: TOGGLE_INTENT,
      label: 'the contents, to go to a chapter',
    },
  ],
}
