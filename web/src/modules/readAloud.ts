import type { ReaderModule } from '../module'

export const READ_INTENT = 'read-aloud:toggle'

/** How fast a book is read aloud, 1 the voice's own pace. */
export interface ReadAloudPrefs {
  rate: number
}

/** The slowest and fastest a reading goes, whatever the setting says. */
export const RATES = [0.5, 2] as const

export const readAloudModule: ReaderModule<ReadAloudPrefs> = {
  id: 'read-aloud',
  label: 'Read aloud',
  defaults: { rate: 1 },
  keys: [
    {
      key: 'r',
      intent: READ_INTENT,
      label: 'read aloud from the sentence at the top, until r again',
    },
  ],
}
