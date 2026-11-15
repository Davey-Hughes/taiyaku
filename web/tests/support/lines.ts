/**
 * The width of `text` set on one line in `el`'s own font: a span inside
 * `el` inherits it, and is taken out again before anything reads `el`.
 */
export const oneLineIn = (el: Element, text: string) => {
  const probe = document.createElement('span')
  probe.style.position = 'absolute'
  probe.style.whiteSpace = 'pre'
  probe.style.visibility = 'hidden'
  probe.textContent = text
  el.append(probe)
  const width = probe.getBoundingClientRect().width
  probe.remove()
  return width
}

/** How wide a line `block` lays out is: its content box. */
export const lineOf = (block: Element) => {
  const style = getComputedStyle(block)
  return (
    block.clientWidth -
    parseFloat(style.paddingLeft) -
    parseFloat(style.paddingRight)
  )
}

/**
 * Every element and every text node in `tile` whose box ends past the
 * tile's right edge. Text is asked on its own: text that overflows its
 * block leaves the block's box where it was.
 */
export const pastTheEdgeOf = (tile: Element) => {
  const edge = tile.getBoundingClientRect().right + 0.5
  const past: string[] = []
  for (const el of tile.querySelectorAll('*')) {
    if (el.getBoundingClientRect().right > edge) {
      past.push(`<${el.tagName.toLowerCase()} class="${el.className}">`)
    }
  }
  const walker = document.createTreeWalker(tile, NodeFilter.SHOW_TEXT)
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const range = document.createRange()
    range.selectNodeContents(node)
    if (range.getBoundingClientRect().right > edge) {
      past.push(JSON.stringify(node.textContent?.slice(0, 40)))
    }
  }
  return past
}

/**
 * How many lines `text` takes set as `field` sets it, `width` px wide, by
 * default the field's content width. The probe goes in the field's label,
 * never in the field: a textarea's children are not laid out, so a probe
 * there would read 0.
 */
export const lineCount = (
  field: HTMLTextAreaElement,
  text: string,
  width = lineOf(field),
) => {
  const label = field.closest('label')
  if (label === null) throw new Error('the field has no label to probe in')
  const style = getComputedStyle(field)
  const probe = document.createElement('div')
  probe.style.cssText = `position: absolute; visibility: hidden; width: ${width}px; white-space: pre-wrap; overflow-wrap: break-word`
  probe.style.font = style.font
  probe.textContent = text
  label.append(probe)
  const height = probe.getBoundingClientRect().height
  probe.remove()
  return height / parseFloat(style.lineHeight)
}
