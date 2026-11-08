// What the Configuration screens' tables draw, read as a person reads it.

/**
 * What a table cell says, without the label only the narrow layout shows,
 * and without words it holds unseen and unread (`aria-hidden`).
 */
export const cellValue = (cell: Element) =>
  [...cell.childNodes]
    .filter(
      (node) =>
        !(
          node instanceof Element &&
          node.matches('.cell-label, [aria-hidden="true"]')
        ),
    )
    .map((node) => node.textContent)
    .join('')
