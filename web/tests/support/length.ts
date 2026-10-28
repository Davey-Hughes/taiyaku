/**
 * `token`, a custom property holding a length, in px as `inside` resolves
 * it: the width of a probe drawn there that wide. Zero where `inside`
 * defines no such property, rather than the probe's full width.
 */
export function lengthOf(inside: Element, token: string) {
  const probe = document.createElement('div')
  probe.style.width = `var(${token}, 0px)`
  inside.append(probe)
  const width = probe.getBoundingClientRect().width
  probe.remove()
  return width
}
