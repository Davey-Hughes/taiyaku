// Shared by the component tests that hold a control's `aria-label` to its
// label's words (O175) and by `fieldNames.browser.test.tsx`, which needs the
// same reading of what a label's words are.

/** One label, the words it shows and the control it wraps. */
export interface Field {
  /** The label's own words, as the markup writes them. */
  written: string
  /**
   * The element that draws those words: the label's `<span>`, or the label
   * itself where the words are a bare text node, as on the account screen.
   */
  words: Element
  /** The one control the label wraps. */
  control: Element
}

/**
 * What each label says and which control it says it about.
 *
 * The words are the `<span>`'s where the label has one, so a hint beside the
 * control is never part of them, and the label's own text nodes where it has
 * none. Throws on a label wrapping no control or more than one, so a caller
 * cannot compare nothing.
 */
export function fields(labels: Iterable<Element>): Field[] {
  return [...labels].map((label) => {
    const controls = label.querySelectorAll('input, select, textarea')
    if (controls.length !== 1) {
      throw new Error(
        `a label wraps ${controls.length} controls: ${label.textContent}`,
      )
    }
    const span = label.querySelector(':scope > span')
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
