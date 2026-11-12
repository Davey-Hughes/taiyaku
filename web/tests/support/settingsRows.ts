// Shared by the settings pages' tests, which find a section or a row by
// its words, and hold every form row on their page to the wiring
// `settingsLayout.tsx`'s `Row` hands its control.

/** What `Row` treats as a form control: a labelable thing a person sets. */
const CONTROLS = 'input:not([type="hidden"]), select, textarea'

/** The box `Row` makes of a control that is several controls (`group`). */
const GROUPS = '[role="radiogroup"]'

/** What a group holds that keeps a label of its own. */
const CHOICES = 'input[type="radio"]'

const LABEL = ':scope > .settings-row-text > .settings-row-label'
const HELP = ':scope > .settings-row-text > .settings-row-help'

/**
 * The one `Section` under `root` whose heading is `title`, word for word.
 * Throws on none or several, so a caller never reads the wrong box.
 */
export function section(root: ParentNode, title: string): HTMLElement {
  const found = [
    ...root.querySelectorAll<HTMLElement>('section.settings-section'),
  ].filter(
    (s) =>
      s.querySelector(
        ':scope > .settings-section-head > .settings-section-title',
      )?.textContent === title,
  )
  if (found.length !== 1) {
    throw new Error(`${found.length} sections titled ${title}`)
  }
  return found[0]
}

/**
 * The one `Row` under `root` whose label is `label`, word for word, so
 * "Can download" is not "Can download all". Throws on none or several.
 */
export function row(root: ParentNode, label: string): HTMLElement {
  const found = [...root.querySelectorAll<HTMLElement>('.settings-row')].filter(
    (r) => r.querySelector(LABEL)?.textContent === label,
  )
  if (found.length !== 1) {
    throw new Error(`${found.length} rows labelled ${label}`)
  }
  return found[0]
}

/** The help beside a row's label, or `null` where the row has none. */
export function helpOf(settingsRow: Element): HTMLElement | null {
  return settingsRow.querySelector<HTMLElement>(HELP)
}

/** Words that say only what kind of control a control is. */
const KINDS = new Set([
  'box',
  'combobox',
  'field',
  'input',
  'list',
  'number',
  'select',
  'spinbutton',
  'text',
  'textarea',
  'textbox',
])

/**
 * Whether `name` says only what kind of control `control` is, its type, its
 * tag or one of `KINDS`, and so nothing of what it sets.
 */
function kindOnly(
  control: HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement,
  name: string,
): boolean {
  const said = name.trim().toLowerCase()
  return (
    KINDS.has(said) ||
    said === control.type ||
    said === control.tagName.toLowerCase()
  )
}

/** The ids an `aria-labelledby` or `aria-describedby` names. */
function ids(element: Element, attribute: string): string[] {
  return element.getAttribute(attribute)?.split(/\s+/) ?? []
}

/**
 * Throws unless every `.settings-row` under `root` that holds a form control
 * is wired as `Row` wires one: each control is labelled by the row's own
 * label and by no other, and has no `aria-labelledby`, which would name it
 * in the label's place, so a click on the words reaches it and its name is
 * those words; and where the row has help, the control's `aria-describedby`
 * names the help, so the help is read with it. An `aria-label` is left to
 * the O175 tests, which hold it to the label's words.
 *
 * A row whose control is a group, radios each with a label of their own,
 * is held to the same two things through the group: the group's
 * `aria-labelledby` names the row's label and nothing else, and the help
 * describes it. The radios inside keep their own labels, which no rule here
 * reads; any other control in the group's box has rules of its own below.
 *
 * A row holding no form control, a session or a key with its Revoke, is
 * passed over. Every fault is named, by its row's label, in the one throw.
 * A root with no rows at all throws too, so a caller cannot check nothing.
 */
export function expectRowsWired(root: HTMLElement): void {
  const rows = [...root.querySelectorAll('.settings-row')]
  if (rows.length === 0) {
    throw new Error('no .settings-row under the root, so nothing was checked')
  }
  const faults: string[] = []
  for (const each of rows) {
    const label = each.querySelector(LABEL)
    const help = helpOf(each)
    const name = `the row "${label?.textContent?.trim() ?? '(no label)'}"`
    const groups = [...each.querySelectorAll(GROUPS)]
    for (const group of groups) {
      const which = `its ${group.getAttribute('role')}`
      const names = ids(group, 'aria-labelledby')
      if (label === null || label.id === '' || !names.includes(label.id)) {
        faults.push(`${name}: ${which} is not labelled by the row's label`)
      } else if (names.length > 1) {
        faults.push(`${name}: ${which} is named by more than the row's label`)
      }
      if (
        help !== null &&
        (help.id === '' || !ids(group, 'aria-describedby').includes(help.id))
      ) {
        faults.push(`${name}: ${which} is not described by the row's help`)
      }
    }
    const controls = [
      ...each.querySelectorAll<
        HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement
      >(CONTROLS),
    ].filter(
      (control) =>
        !(
          control.matches(CHOICES) &&
          groups.some((group) => group.contains(control))
        ),
    )
    for (const control of controls) {
      const which = `its ${control.tagName.toLowerCase()}${
        control.name === '' ? '' : ` "${control.name}"`
      }`
      const labels = [...(control.labels ?? [])]
      if (groups.some((group) => group.contains(control))) {
        // As Every's count is in the Scan schedule's: the row's label names
        // the group, so it names the control by an `aria-label` that says
        // what it sets, with no `<label>`, which would tie a radio's words
        // to it, and no `aria-labelledby`, which would outrank it. The help
        // describes it as it describes a row's control.
        const own = control.getAttribute('aria-label')
        if (own === null || own.trim() === '') {
          faults.push(`${name}: ${which} has no aria-label of its own`)
        } else if (kindOnly(control, own)) {
          faults.push(
            `${name}: ${which} is named "${own}", which says what it is and not what it sets`,
          )
        }
        if (labels.length > 0) {
          faults.push(`${name}: ${which} has a <label> of its own`)
        }
        if (control.hasAttribute('aria-labelledby')) {
          faults.push(
            `${name}: ${which} is named by its aria-labelledby, not an aria-label of its own`,
          )
        }
      } else {
        if (!labels.some((l) => l === label)) {
          faults.push(`${name}: ${which} is not labelled by the row's label`)
        } else if (labels.length > 1) {
          faults.push(`${name}: ${which} has a label besides the row's`)
        }
        if (control.hasAttribute('aria-labelledby')) {
          faults.push(
            `${name}: ${which} is named by its aria-labelledby, not the row's label`,
          )
        }
      }
      if (
        help !== null &&
        (help.id === '' || !ids(control, 'aria-describedby').includes(help.id))
      ) {
        faults.push(`${name}: ${which} is not described by the row's help`)
      }
    }
  }
  if (faults.length > 0) {
    throw new Error(`settings rows not wired:\n${faults.join('\n')}`)
  }
}
