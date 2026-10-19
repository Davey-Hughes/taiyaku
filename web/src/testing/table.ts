// What the Configuration screens' tables draw, read as a person reads it.

/** What a table cell says, without the label only the narrow layout shows. */
export const cellValue = (cell: Element) =>
  [...cell.childNodes]
    .filter((node) => !(node instanceof Element && node.matches('.cell-label')))
    .map((node) => node.textContent)
    .join('')
