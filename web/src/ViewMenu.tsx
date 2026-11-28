/**
 * A page's View button: the browse preferences that act on this page, beside
 * its heading, so each control is drawn where its effect is seen.
 *
 * **Generated, not written**: one control per key from `settingsPanel.ts`'s
 * `fieldsOf`, which infers each control from the preference's runtime type,
 * narrowed to the keys the page names and in the page's order. Another
 * preference needs no edit here, only a page that names it. The state is
 * `Shell.tsx`'s, reached through `BrowseControlsContext`, so a change made on
 * one page is the one every page reads and stores. Reset restores every
 * browse preference, not only the ones this page shows: one Reset that left
 * the other page's choices behind would not be a reset.
 *
 * **A disclosure, as More is** (`AccountMenu.tsx`, `popover.ts`): the panel
 * holds plain controls, so neither a dialog's moved focus nor a popup's
 * announcement is owed. It follows the button in the document, drawn below
 * it, so Tab from the button enters it; Escape, a press or focus outside,
 * and any navigation close it.
 */
import { useContext } from 'react'
import {
  BrowseControlsContext,
  type BrowsePrefs,
  browseModule,
  useBrowsePrefs,
} from './browseModule'
import { usePopover } from './popover'
import { FieldControl } from './settingsFields'
import { fieldsOf } from './settingsPanel'

/**
 * A click on a checkbox's row, off the box, clicks the box once and cancels
 * the label's own activation. Firefox drops that activation once a press has
 * moved a few pixels, which would leave a slightly sloppy press on a row's
 * words doing nothing there; cancelling it keeps Chromium, which still
 * passes it on, from toggling the box a second time. The box takes the
 * focus that activation gave it, so Space toggles the row just pressed.
 */
function sendRowClick(event: React.MouseEvent<HTMLDivElement>) {
  const target = event.target as Element
  const box = target
    .closest('label')
    ?.querySelector<HTMLInputElement>(':scope > input[type="checkbox"]')
  if (!box || target === box) return
  event.preventDefault()
  box.focus({ preventScroll: true })
  box.click()
}

export function ViewMenu({ keys }: { keys: readonly (keyof BrowsePrefs)[] }) {
  const prefs = useBrowsePrefs()
  const controls = useContext(BrowseControlsContext)
  const { open, toggle, wrap, button, panelId } = usePopover()
  if (!controls) return null
  const fields = fieldsOf(
    browseModule.defaults,
    browseModule.options,
    browseModule.sliders,
    browseModule.optionWords,
  )
  const shown = keys.flatMap((key) => fields.filter((f) => f.key === key))
  return (
    <div className="view-menu" ref={wrap}>
      <button
        ref={button}
        type="button"
        className="view-button"
        aria-expanded={open}
        aria-controls={panelId}
        onClick={toggle}
      >
        View
        <svg viewBox="0 0 12 12" aria-hidden="true">
          <path d="M3 4.5l3 3 3-3" />
        </svg>
      </button>
      {/* biome-ignore lint/a11y/noStaticElementInteractions lint/a11y/useKeyWithClickEvents: the rows are labels and checkboxes with their own keyboard; this only re-sends a label's click to its box. */}
      <div
        id={panelId}
        className="view-panel"
        hidden={!open}
        onClick={sendRowClick}
      >
        {shown.map((field) => (
          <FieldControl
            key={field.key}
            field={field}
            value={prefs[field.key as keyof BrowsePrefs]}
            sentenceCase
            onChange={(raw) => {
              controls.change(field.key, raw)
            }}
          />
        ))}
        <button type="button" className="view-reset" onClick={controls.reset}>
          Reset
        </button>
      </div>
    </div>
  )
}
