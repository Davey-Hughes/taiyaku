import { act } from 'react'
import { userEvent } from '@vitest/browser/context'
import { describe, expect, it } from 'vitest'
// The real stylesheet, which the entry points import and a mounted component
// otherwise never sees. Without it `getComputedStyle` reports the browser's
// defaults and the visibility assertions below would pass against a box that
// has no styling at all.
import '../src/index.css'
import { Labeller } from '../src/Labeller'
import { mount } from './support/mount'

const ALPHA = /rgba?\([^)]*?([\d.]+)\s*\)$/

/** The alpha of a computed colour; `1` for an `rgb()` with no alpha at all. */
function alphaOf(colour: string): number {
  const m = colour.match(ALPHA)
  return m && colour.startsWith('rgba') ? Number(m[1]) : 1
}

// A box nobody can see is a box nobody can check. The first version of this
// styling was a 1px hairline at `opacity: 0.15`, which a labelling session
// reported as "impossible to see" — invisible over halftone artwork rather
// than merely quiet. Colour is a matter of taste and is not asserted here;
// *being visible at all* is not.
describe('a resting box is visible without being touched', () => {
  it('has a filled interior and a border thick enough to read on line art', () => {
    const host = mount(
      <Labeller
        initialBoxes={[
          {
            id: 'a',
            x: 0.1,
            y: 0.1,
            w: 0.3,
            h: 0.3,
            kind: 'dialogue',
            axis: 'v',
            ignore: false,
          },
        ]}
      />,
    )
    const drawn = host.querySelector('.region-box') as HTMLElement
    const style = getComputedStyle(drawn)
    expect(alphaOf(style.backgroundColor)).toBeGreaterThan(0.05)
    expect(Number.parseFloat(style.borderTopWidth)).toBeGreaterThanOrEqual(2)
    // Not faded out as a whole: the fill carries the quietness now, so that
    // the page stays readable with every box on it drawn.
    expect(Number(style.opacity)).toBe(1)
  })
})

// This is what the `browser` project exists for: `jsdom` has no layout
// engine, `getBoundingClientRect` returns zeroes there, and a box drawn at
// the wrong place is not observable without one — the argument
// `vite.config.ts`'s own comment already makes.
describe('the page area is not text', () => {
  it('refuses a text selection, and lets the pointer past the image', () => {
    // A drag across artwork is a box being drawn; the browser has no way to
    // know that and ran a text selection under the pointer, painting the page
    // blue on every box. The `<img>` compounded it by being natively
    // draggable, so a press on the artwork also started a drag-and-drop.
    //
    // Asserted as computed style in a real browser rather than by dragging and
    // reading `window.getSelection()`: `user-select` is what governs the
    // behaviour, and a synthetic pointer sequence produces no native selection
    // to observe in the first place — a test built that way would pass with
    // the rule deleted.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    expect(getComputedStyle(page).userSelect).toBe('none')
  })
})

describe('selecting a box by pointer', () => {
  it('selects on a real click, driven through the browser rather than dispatched', async () => {
    // The regression this exists for: capturing the pointer on `pointerdown`
    // retargets the compatibility mouse events — `click` among them — to the
    // capturing element, so every click on a box was delivered to the page
    // instead. Nothing could be selected by pointer at all; `Tab` was the only
    // way in, because it reads the cursor ref rather than a click.
    //
    // `userEvent` rather than `dispatchEvent`, and this is the whole test:
    // retargeting is done by the browser's own dispatch, so a synthetic click
    // lands on whatever element it is called on and passes whatever the
    // capture did. An earlier version of this test dispatched one and proved
    // nothing — putting the bug back left it green.
    const host = mount(
      <Labeller
        initialBoxes={[
          {
            id: 'a',
            x: 0.2,
            y: 0.2,
            w: 0.4,
            h: 0.4,
            kind: 'dialogue',
            axis: 'v',
            ignore: false,
          },
        ]}
      />,
    )
    const drawn = host.querySelector('.region-box') as HTMLElement
    expect(drawn.dataset.selected).toBe('false')
    await userEvent.click(drawn)
    expect(
      (host.querySelector('.region-box') as HTMLElement).dataset.selected,
      'a click on a box did not select it',
    ).toBe('true')
  })

  it('does not take the pointer for a press that never became a drag', async () => {
    // The mechanism, at the level the bug actually lived: a press with a
    // pixel or two of jitter must not capture, because capturing is what
    // costs the click.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    const x = box.left + box.width / 2
    const y = box.top + box.height / 2
    await act(async () => {
      page.dispatchEvent(pointer('pointerdown', x, y))
      page.dispatchEvent(pointer('pointermove', x + 2, y + 2))
    })
    expect(page.hasPointerCapture(1)).toBe(false)
    // And nothing was previewed, because a jitter is not a box.
    expect(host.querySelector('[data-testid="region-preview"]')).toBeNull()
  })

  it('creates no box from a press that wobbled but never dragged', async () => {
    // `addBox` refuses a *zero* extent, and a wobble is not zero: a click on a
    // box to select it used to leave a two-pixel box behind wherever the hand
    // moved. Specks like that enter the ground truth as labels like any other,
    // and `score` counts a prediction that misses one as spurious.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    const x = box.left + box.width / 2
    const y = box.top + box.height / 2
    await act(async () => {
      page.dispatchEvent(pointer('pointerdown', x, y))
      page.dispatchEvent(pointer('pointermove', x + 2, y + 1))
      page.dispatchEvent(pointer('pointerup', x + 2, y + 1))
    })
    expect(host.querySelectorAll('.region-box')).toHaveLength(0)
  })
})

describe('the box you are drawing, while you draw it', () => {
  it('tracks the pointer mid-drag and lands where the preview last was', async () => {
    // The preview and the committed box come from one function, so this is
    // the wiring: that something is drawn before the button comes up, that it
    // follows the pointer, and that the box replacing it is in the same place.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    const from = {
      x: box.left + box.width * 0.3,
      y: box.top + box.height * 0.3,
    }
    const to = { x: box.left + box.width * 0.6, y: box.top + box.height * 0.6 }

    expect(host.querySelector('[data-testid="region-preview"]')).toBeNull()
    await act(async () => {
      page.dispatchEvent(pointer('pointerdown', from.x, from.y))
      page.dispatchEvent(pointer('pointermove', to.x, to.y))
    })
    const preview = host.querySelector(
      '[data-testid="region-preview"]',
    ) as HTMLElement
    expect(preview, 'nothing was drawn during the drag').not.toBeNull()
    const at = preview.style.left
    const wide = preview.style.width

    await act(async () => {
      page.dispatchEvent(pointer('pointerup', to.x, to.y))
    })
    expect(host.querySelector('[data-testid="region-preview"]')).toBeNull()
    const drawn = host.querySelector('.region-box') as HTMLElement
    // The committed box sits exactly where the preview last did.
    expect(drawn.style.left).toBe(at)
    expect(drawn.style.width).toBe(wide)
  })

  it('previews an upward drag the way it will be stored, not as the pointer delta', async () => {
    // Dragging up and left gives a negative extent. `normaliseRect` flips it;
    // a preview built from the raw delta would draw a rectangle that is not
    // the box about to be created.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    await act(async () => {
      page.dispatchEvent(
        pointer(
          'pointerdown',
          box.left + box.width * 0.6,
          box.top + box.height * 0.6,
        ),
      )
      page.dispatchEvent(
        pointer(
          'pointermove',
          box.left + box.width * 0.3,
          box.top + box.height * 0.3,
        ),
      )
    })
    const preview = host.querySelector(
      '[data-testid="region-preview"]',
    ) as HTMLElement
    expect(Number.parseFloat(preview.style.width)).toBeGreaterThan(0)
    expect(Number.parseFloat(preview.style.height)).toBeGreaterThan(0)
    // The near edge is where the pointer ended up, not where it started.
    expect(Number.parseFloat(preview.style.left)).toBeCloseTo(30, 0)
  })

  it('clears the preview when the system cancels the drag', async () => {
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    await act(async () => {
      page.dispatchEvent(pointer('pointerdown', box.left + 10, box.top + 10))
      page.dispatchEvent(pointer('pointermove', box.left + 80, box.top + 80))
    })
    expect(host.querySelector('[data-testid="region-preview"]')).not.toBeNull()
    await act(async () => {
      page.dispatchEvent(pointer('pointercancel', box.left + 80, box.top + 80))
    })
    // A cancelled gesture sends no `pointerup`, so without the handler the
    // preview would sit there describing a box that never gets made.
    expect(host.querySelector('[data-testid="region-preview"]')).toBeNull()
    expect(host.querySelectorAll('.region-box')).toHaveLength(0)
  })
})

function pointer(type: string, x: number, y: number): PointerEvent {
  return new PointerEvent(type, {
    clientX: x,
    clientY: y,
    bubbles: true,
    pointerId: 1,
  })
}

describe('drag to draw, with a layout engine', () => {
  it('lands the box where the pointer went, in page fractions', async () => {
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    // A quarter of the way in, to half way across and down.
    await drag(
      page,
      box.left + box.width * 0.25,
      box.top + box.height * 0.25,
      box.left + box.width * 0.5,
      box.top + box.height * 0.5,
    )
    const drawn = host.querySelector('.region-box') as HTMLElement
    expect(Number.parseFloat(drawn.style.left)).toBeCloseTo(25, 0)
    expect(Number.parseFloat(drawn.style.width)).toBeCloseTo(25, 0)
  })

  it('measures against the page element, not the window', async () => {
    // The page is letterboxed inside the viewport. Converting against the
    // window puts every box off by the letterbox margin, which looks correct
    // at one aspect ratio and wrong at every other.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    expect(box.left).toBeGreaterThan(0)
    await drag(
      page,
      box.left,
      box.top,
      box.left + box.width,
      box.top + box.height,
    )
    const drawn = host.querySelector('.region-box') as HTMLElement
    expect(Number.parseFloat(drawn.style.left)).toBeCloseTo(0, 0)
    expect(Number.parseFloat(drawn.style.width)).toBeCloseTo(100, 0)
  })
})

async function drag(
  el: Element,
  x0: number,
  y0: number,
  x1: number,
  y1: number,
) {
  act(() => {
    el.dispatchEvent(
      new PointerEvent('pointerdown', {
        clientX: x0,
        clientY: y0,
        bubbles: true,
      }),
    )
  })
  act(() => {
    el.dispatchEvent(
      new PointerEvent('pointermove', {
        clientX: x1,
        clientY: y1,
        bubbles: true,
      }),
    )
  })
  act(() => {
    el.dispatchEvent(
      new PointerEvent('pointerup', {
        clientX: x1,
        clientY: y1,
        bubbles: true,
      }),
    )
  })
}
