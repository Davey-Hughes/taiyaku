/**
 * Where the reader puts focus — spec 2026-09-26 §3.1 and §3.3.
 *
 * The DOM half of the ring: `ring.ts` decides which stop is next from counts,
 * and this reads the stops off the page and moves focus onto the one it
 * names. The page marks its regions with attributes rather than this module
 * knowing components: `data-reader-page` on the page area, `data-ring="top"`
 * on the top bar and the correction banner, `data-ring="bottom"` on the
 * bottom bar, `data-ring="controls"` on the note list.
 */
import { type RefObject, useEffect, useRef } from 'react'
import type { RingSizes, RingStop } from './ring'

/** The page area — the ring's page stop, and where focus goes when nothing else will have it. */
export const PAGE_SELECTOR = '[data-reader-page]'

/** What `Tab` can stop on, before `tabIndex` and `disabled` are asked. */
const TABBABLE = [
  'button',
  'a[href]',
  'input',
  'select',
  'textarea',
  '[tabindex]',
]

/** `TABBABLE`, each under `scope`, as one selector list. */
const under = (scope: string) => TABBABLE.map((t) => `${scope} ${t}`).join(', ')

/**
 * The elements `Tab` stops on, in document order. One `querySelectorAll`
 * over a selector list, so an element two selectors match is listed once.
 */
export function tabbablesIn(
  scope: ParentNode,
  selector = TABBABLE.join(', '),
): HTMLElement[] {
  return [...scope.querySelectorAll<HTMLElement>(selector)].filter(
    (el) => el.tabIndex >= 0 && !(el as HTMLButtonElement).disabled,
  )
}

/** Whether the bars are on the page. Read from the DOM so it is right straight after a `flushSync`. */
export function barsMounted(root: ParentNode): boolean {
  return root.querySelector('[data-ring="bottom"]') !== null
}

/** The bars' stops: the top edge's first, then the bottom bar's. */
export function barStops(root: ParentNode): HTMLElement[] {
  return [
    ...tabbablesIn(root, under('[data-ring="top"]')),
    ...tabbablesIn(root, under('[data-ring="bottom"]')),
  ]
}

/** What the page itself offers: controls inside the page area, then the note list. */
export function controlStops(root: ParentNode): HTMLElement[] {
  return tabbablesIn(
    root,
    `${under(PAGE_SELECTOR)}, ${under('[data-ring="controls"]')}`,
  )
}

export function ringSizes(root: ParentNode): RingSizes {
  return {
    bars: barsMounted(root) ? barStops(root).length : undefined,
    controls: controlStops(root).length,
  }
}

/** Which stop `from` is. Anything that is neither a bar stop nor a control counts as the page. */
export function ringAt(root: ParentNode, from: EventTarget | null): RingStop {
  const el = from as HTMLElement | null
  const bar = barStops(root).indexOf(el as HTMLElement)
  if (bar >= 0) return { region: 'bars', index: bar }
  const control = controlStops(root).indexOf(el as HTMLElement)
  if (control >= 0) return { region: 'controls', index: control }
  return { region: 'page', index: 0 }
}

/**
 * Move focus onto `to`. The page area is focused without scrolling, which in
 * scroll mode would move the column.
 *
 * **Falls back to the page area if the stop will not take focus.** A bar or
 * control stop can be disconnected or otherwise refuse it — the key's default
 * is already prevented by the time this runs, so a `Tab` that stuck would
 * leave focus nowhere the reader could find it (`PAGE_SELECTOR`'s doc).
 */
export function focusStop(root: ParentNode, to: RingStop): void {
  if (to.region === 'page') {
    root
      .querySelector<HTMLElement>(PAGE_SELECTOR)
      ?.focus({ preventScroll: true })
    return
  }
  const stops = to.region === 'bars' ? barStops(root) : controlStops(root)
  const stop = stops.at(to.index)
  stop?.focus()
  if (document.activeElement !== stop) {
    root
      .querySelector<HTMLElement>(PAGE_SELECTOR)
      ?.focus({ preventScroll: true })
  }
}

/**
 * A reader panel's focus — spec 2026-09-26 §3.3, O161.
 *
 * On opening, focus moves onto the panel itself, so a screen reader names the
 * panel rather than the button that opened it (the audit heard ⚙'s own
 * *"settings, expanded"*); a panel that wants focus further in — the note
 * editor's textarea — moves it there from its own, later effect. On closing,
 * focus goes back to what held it when the panel opened, or to the page if
 * that has gone or was `<body>`. While open, `Tab` wraps at either end
 * rather than leaving.
 *
 * **Call it before any effect of the panel's own that moves focus.** Effects
 * in one component run in declaration order, and this one records what held
 * focus before the panel opened.
 */
export function usePanelFocus(
  ref: RefObject<HTMLElement | null>,
  open: boolean,
): void {
  const opener = useRef<HTMLElement | null>(null)
  useEffect(() => {
    if (!open) return
    const panel = ref.current
    if (!panel) return
    const before = document.activeElement
    opener.current =
      before instanceof HTMLElement &&
      before !== document.body &&
      !panel.contains(before)
        ? before
        : null
    panel.focus({ preventScroll: true })
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Tab' || event.ctrlKey || event.metaKey || event.altKey)
        return
      const stops = tabbablesIn(panel)
      const first = stops[0]
      const last = stops.at(-1)
      if (!first || !last) {
        event.preventDefault()
        return
      }
      const active = document.activeElement
      const atEnd = event.shiftKey
        ? active === first || active === panel
        : active === last
      if (!atEnd) return
      event.preventDefault()
      ;(event.shiftKey ? last : first).focus()
    }
    panel.addEventListener('keydown', onKeyDown)
    return () => {
      panel.removeEventListener('keydown', onKeyDown)
      // Only a focus the panel held — or dropped to `<body>` as it unmounted.
      // A reader who has since put focus somewhere else keeps it.
      const active = document.activeElement
      if (active && active !== document.body && !panel.contains(active)) return
      const back = opener.current?.isConnected
        ? opener.current
        : document.querySelector<HTMLElement>(PAGE_SELECTOR)
      back?.focus({ preventScroll: true })
    }
  }, [open, ref])
}
