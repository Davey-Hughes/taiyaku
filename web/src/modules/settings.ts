import type { ReaderModule } from '../module'

/** `settingsModule`'s own declared intent string — read here by `App.tsx` rather than duplicated as a literal. */
export const TOGGLE_INTENT = 'settings:toggle'

/** The panel's key, which ⚙'s hover text names: one declaration, so the two cannot disagree. */
export const TOGGLE_KEY = 's'

/**
 * The settings panel's own on/off key, registered exactly the way every
 * reading behaviour claims a key — through `ReaderModule.keys`, dispatched
 * by `keys.ts`'s `intentOf` as `{ kind: 'module', intent: TOGGLE_INTENT }` —
 * rather than as a second hard-coded case beside `Tab`'s `{ kind: 'chrome'
 * }` in `keys.ts` itself. `Tab` already opens the chrome; this is a
 * deliberately separate key so the two can be shown independently, and
 * routing it through the contract is what makes the startup collision check
 * (`module.ts`'s `collisions`/`duplicateIds`) cover it the same as every
 * other module's key, rather than adding a parallel mechanism nothing
 * checks.
 *
 * `defaults: {}` — this module tunes nothing about itself. `settingsPanel.ts`'s
 * `fieldsOf` reports zero fields for an empty slice, so `Settings.tsx`'s
 * generated section for this entry renders no controls and is skipped
 * outright (see `Settings.tsx`), rather than showing an empty "Settings"
 * section inside the settings panel itself.
 */
export const settingsModule: ReaderModule<Record<string, never>> = {
  id: 'settings',
  label: 'Settings',
  defaults: {},
  keys: [
    {
      key: TOGGLE_KEY,
      intent: TOGGLE_INTENT,
      label: 'open the settings panel',
    },
  ],
}
