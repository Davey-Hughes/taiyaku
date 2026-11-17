import { SIDEBAR_LAYOUT } from '../../src/phoneLayout'

/** The least window height the sidebar is drawn at (`SIDEBAR_LAYOUT`). */
export const SIDEBAR_HEIGHT = Number(
  /\(height >= (\d+)px\)/.exec(SIDEBAR_LAYOUT)?.[1],
)

/**
 * Room above `main`'s content, so that what lies `depth` px down lies more
 * than `least` px down. A case that wants a window shorter than the page
 * above a control, in the sidebar's layout, keeps the window tall enough
 * for the sidebar and puts the control lower instead: a window under
 * `SIDEBAR_HEIGHT` would draw the phone layout.
 */
export function lowered(main: HTMLElement, depth: number, least: number) {
  if (depth > least) return
  const top = Number.parseFloat(getComputedStyle(main).paddingTop)
  main.style.paddingTop = `${top + Math.ceil(least - depth) + 1}px`
}
