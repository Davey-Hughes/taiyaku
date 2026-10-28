// Shared by the component tests that hold a control's `aria-label` to its
// label's words (O175) and by `fieldNames.browser.test.tsx`, which needs the
// same reading of what a label's words are.

/** What a label can be the label of here: a control a person sets. */
const CONTROLS = 'input, select, textarea'

/** One label, the words it shows and the control it labels. */
export interface Field {
  /** The label's own words, as the markup writes them. */
  written: string
  /**
   * The element that draws those words: the label's `<span>`, or the label
   * itself where the words are a bare text node, as on the account screen
   * and in a settings row's `for` label.
   */
  words: Element
  /** The one control the label wraps, or the one its `for` names. */
  control: Element
}

/**
 * What each label says and which control it says it about.
 *
 * A label with a `for`, as a settings row draws, is the label of the control
 * that id names, and holds nothing but its words. A label without one is the
 * label of the control it wraps; its words are the `<span>`'s where it has
 * one, so a hint beside the control is never part of them, and its own text
 * nodes where it has none. Throws on a `for` that names no control, and on a
 * label wrapping no control or more than one, so a caller cannot compare
 * nothing.
 */
export function fields(labels: Iterable<Element>): Field[] {
  return [...labels].map((label) => {
    const span = label.querySelector(':scope > span')
    const id = label.getAttribute('for')
    if (id !== null) {
      const control = label.ownerDocument.getElementById(id)
      if (!control?.matches(CONTROLS)) {
        throw new Error(
          `a label is for "${id}", which is no control: ${label.textContent}`,
        )
      }
      return {
        written: (span ?? label).textContent ?? '',
        words: span ?? label,
        control,
      }
    }
    const controls = label.querySelectorAll(CONTROLS)
    if (controls.length !== 1) {
      throw new Error(
        `a label wraps ${controls.length} controls: ${label.textContent}`,
      )
    }
    const written =
      span?.textContent ??
      [...label.childNodes]
        .filter((node) => node.nodeType === Node.TEXT_NODE)
        .map((node) => node.textContent)
        .join('')
        .trim()
    return { written, words: span ?? label, control: controls[0] }
  })
}
