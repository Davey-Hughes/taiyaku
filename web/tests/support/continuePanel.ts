// Shared by the browser tests of the Continue row's panel (O417). Each file
// keeps its own `vi.mock` of `src/library` and answers its own reads.
import type { Me } from '../../src/api'
import { byHand, clickAt, pointAt, until } from './byHand'

/** An administrator who may download: the panel draws all it can. */
export const ADMIN: Me = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

export function need<T extends Element = HTMLElement>(
  scope: ParentNode,
  selector: string,
): T {
  const found = scope.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/** The row's cards, in its order. */
export const cardsIn = (host: Element) => [
  ...host.querySelectorAll<HTMLElement>('section.continue > ul > li'),
]

/** A card's own link: its cover, its title and its volume. */
export const linkOf = (card: Element) => need(card, ':scope > a')

/** A card's own `i`. */
export const iOf = (card: Element) =>
  need<HTMLButtonElement>(card, ':scope > .panel-open')

export const rect = (el: Element) => el.getBoundingClientRect()

/**
 * The page area's left and right: `main`'s content box, its right at the
 * whole pixel `clientWidth` is rounded to, so only for a page area a whole
 * number of pixels wide, as every case that reads it draws. Measured, never
 * taken from the window: the gutter the page keeps while a panel can open
 * (`library.css`) takes its room even where scrollbars are hidden.
 */
export function pageArea(host: Element) {
  const main = need(host, '.library > main')
  const style = getComputedStyle(main)
  const left = rect(main).left + main.clientLeft
  return {
    left: left + Number.parseFloat(style.paddingLeft),
    right: left + main.clientWidth - Number.parseFloat(style.paddingRight),
  }
}

/** Whether `card`'s panel is open and drawn whole. */
export const openFor = (host: Element, card: Element) =>
  iOf(card).getAttribute('aria-expanded') === 'true' &&
  host.querySelector('section.continue > .tile-panel .match-marks') !== null

/**
 * Opens `card`'s panel as a hand does, the pointer onto the card and then a
 * press on its `i`, and waits until it is drawn whole: it is laid as it
 * mounts.
 */
export async function open(host: HTMLElement, card: HTMLElement) {
  await byHand(async () => {
    await pointAt(linkOf(card))
    await clickAt(iOf(card))
    await until('the panel', () => openFor(host, card))
  })
  return need(host, 'section.continue > .tile-panel')
}

/** The panel drawn under the row, where one is. */
export const panelIn = (host: Element) =>
  host.querySelector<HTMLElement>('section.continue > .tile-panel')

/**
 * The title the open panel's Title field holds: whose panel it is, where
 * no `i` is drawn to say so.
 */
export const titleIn = (host: Element) =>
  panelIn(host)?.querySelector<HTMLTextAreaElement>("textarea[name='title']")
    ?.value
