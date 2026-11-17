import { onTestFinished } from 'vitest'
import { cdp, userEvent } from 'vitest/browser'
import { PHONE_LAYOUT } from '../../src/phoneLayout'
import { RESEAT_MS } from '../../src/reseatTiming'
import { beforeUnmount } from './mount'

/** Lets `count` frames be drawn. */
export async function frames(count: number): Promise<void> {
  for (let i = 0; i < count; i += 1) {
    await new Promise(requestAnimationFrame)
  }
}

/**
 * Lets any motion `ms` long started by now end, a reseat unless told, and the
 * frames after its last one be drawn: a case about where a list comes to
 * rest reads it here, not while it is still moving.
 */
export async function afterMotion(ms: number = RESEAT_MS): Promise<void> {
  await new Promise((done) => setTimeout(done, ms))
  await frames(2)
}

/** `count` sizes evenly from `from` to `to`, rounded, `from` left out. */
export const ramp = (from: number, to: number, count: number) =>
  Array.from({ length: count }, (_, i) =>
    Math.round(from + ((to - from) * (i + 1)) / count),
  )

/** The box around the test's frame, which a drag sizes. */
export function frameBox(): HTMLElement {
  const box = window.frameElement?.parentElement
  if (!box) throw new Error('this file is not running in a frame')
  return box
}

/**
 * Drags the window through `sizes` as a drag's sizes arrive: each is set
 * on the test frame's box from the page around it, in a task between two
 * frames, so each frame lays a new size, and a step that leaves out a side
 * keeps it. `page.viewport` waits for a frame before it answers, so it
 * cannot pace a size a frame. Gives the size each frame was drawn at, from
 * the first to the one that drew the last.
 */
export async function dragged(
  sizes: readonly { width?: number; height?: number }[],
): Promise<{ width: number; height: number }[]> {
  const box = frameBox()
  const outer = window.parent
  const seen: { width: number; height: number }[] = []
  let done = false
  const watch = () => {
    if (done) return
    seen.push({ width: window.innerWidth, height: window.innerHeight })
    requestAnimationFrame(watch)
  }
  requestAnimationFrame(watch)
  await new Promise<void>((resolve) => {
    let i = 0
    const step = () => {
      if (i === sizes.length) return resolve()
      const { width, height } = sizes[i]
      i += 1
      outer.setTimeout(() => {
        if (width !== undefined) box.style.width = `${width}px`
        if (height !== undefined) box.style.height = `${height}px`
        requestAnimationFrame(step)
      }, 0)
    }
    requestAnimationFrame(step)
  })
  done = true
  return seen
}

/**
 * Runs `read` in each frame from here until the test's page is unmounted
 * (`beforeUnmount`), as drawn: in an observer made after the page's, so
 * after its lists and its Continue row are laid, of a box resized each
 * frame, so that it runs in every frame and not only those the page
 * resizes in.
 */
export function eachFrame(read: () => void): void {
  const ticker = document.createElement('div')
  ticker.style.cssText =
    'position: fixed; left: 0; top: 0; width: 1px; height: 1px; visibility: hidden; pointer-events: none'
  document.body.append(ticker)
  const observer = new ResizeObserver(read)
  observer.observe(ticker)
  let on = true
  const tick = () => {
    if (!on) return
    ticker.style.width = ticker.style.width === '1px' ? '2px' : '1px'
    requestAnimationFrame(tick)
  }
  requestAnimationFrame(tick)
  beforeUnmount(() => {
    on = false
    observer.disconnect()
    ticker.remove()
  })
}

/** Whether `animation` is a script's Web Animation, not a CSS one. */
export const scripted = (animation: Animation) =>
  !(animation instanceof CSSTransition || animation instanceof CSSAnimation)

/**
 * How many Web Animations run anywhere in `tile`, whichever element they
 * move: a glide is one, and a corner control's CSS fade, which a pointer
 * over the tile starts, is not.
 */
export const scriptedIn = (tile: Element) =>
  tile.getAnimations({ subtree: true }).filter(scripted).length

/** `screen.orientation`'s types as the DevTools session spells them. */
const SESSION_ORIENTATION = {
  'portrait-primary': 'portraitPrimary',
  'portrait-secondary': 'portraitSecondary',
  'landscape-primary': 'landscapePrimary',
  'landscape-secondary': 'landscapeSecondary',
} as const

/**
 * Turns the screen the page reports (`screen.orientation`) over the
 * DevTools session, at the page's size and screen as they are, and puts it
 * back once the test ends. The override is the page's, and stays, for the
 * reason `Cover.browser.test.tsx`'s `atDensity` gives; this frame's view,
 * and its `(orientation: …)`, follow the frame's box instead.
 */
export async function screenTurns() {
  const { cssLayoutViewport } = await cdp().send('Page.getLayoutMetrics')
  const as = (type: OrientationType, angle: number) =>
    cdp().send('Emulation.setDeviceMetricsOverride', {
      width: cssLayoutViewport.clientWidth,
      height: cssLayoutViewport.clientHeight,
      screenWidth: screen.width,
      screenHeight: screen.height,
      deviceScaleFactor: devicePixelRatio,
      mobile: false,
      screenOrientation: { type: SESSION_ORIENTATION[type], angle },
    })
  const { type, angle } = screen.orientation
  onTestFinished(async () => {
    await as(type, angle)
  })
  return (to: 'portrait-primary' | 'landscape-primary') =>
    as(to, to === 'portrait-primary' ? 0 : 90)
}

/** Asks for reduced motion where `on`, and for no preference where not. */
export const reducedMotion = (on: boolean) =>
  cdp().send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-reduced-motion', value: on ? 'reduce' : '' }],
  })

function mainOf(host: HTMLElement): HTMLElement {
  const main = host.querySelector('main')
  if (main === null) throw new Error('no main')
  return main
}

/** The element that scrolls: `<main>` beside the sidebar, the document in the phone layout. */
export const scrollerOf = (host: HTMLElement): Element =>
  matchMedia(PHONE_LAYOUT).matches
    ? (document.scrollingElement ?? document.documentElement)
    : mainOf(host)

/** Turns the mouse's wheel over the page until `y` px down, and waits for it to rest. */
export async function wheeledTo(host: HTMLElement, y: number) {
  const scroller = scrollerOf(host)
  for (let turn = 0; scroller.scrollTop < y - 1 && turn < 20; turn++) {
    await userEvent.wheel(mainOf(host), {
      delta: { y: Math.ceil(y - scroller.scrollTop) },
    })
    let still = 0
    let was = scroller.scrollTop
    while (still < 3) {
      await frames(1)
      still = scroller.scrollTop === was ? still + 1 : 0
      was = scroller.scrollTop
    }
  }
}
