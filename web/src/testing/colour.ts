/** A computed `rgb()` or `rgba()` as its channels, 0–255, and its alpha. */
export const channels = (colour: string) => {
  const [r, g, b, a = 1] = (colour.match(/[\d.]+/g) ?? []).map(Number)
  return { r, g, b, a }
}
export type Channels = ReturnType<typeof channels>

/** WCAG 2's relative luminance. */
export const luminance = ({ r, g, b }: Channels) => {
  const linear = (c: number) => {
    const s = c / 255
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/** WCAG 2's contrast ratio between two opaque colours. */
export const contrast = (one: Channels, other: Channels) => {
  const [high, low] = [luminance(one), luminance(other)].sort((x, y) => y - x)
  return (high + 0.05) / (low + 0.05)
}

/**
 * A colour token as the browser resolves it for an element inside `within`,
 * in whichever theme and scheme are in force there.
 */
export function tokenColour(within: Element, token: string): string {
  const probe = document.createElement('span')
  probe.style.color = `var(${token})`
  within.append(probe)
  const colour = getComputedStyle(probe).color
  probe.remove()
  return colour
}
