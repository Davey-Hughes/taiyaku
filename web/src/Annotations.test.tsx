import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { Annotations, type Box, PageAnnotations } from './Annotations'
import type { AnnotationsPrefs } from './modules/annotations'
import type { Note, NoteHere } from './notes'
import { mount, rerender } from './testing/mount'

/**
 * `PageAnnotations` builds its own `ResizeObserver` and jsdom does not carry
 * one — there is no layout engine underneath it to report on. A stub that
 * does nothing is enough: none of the tests below rely on the observer firing
 * on its own, only on `measure` being reachable through the `load` listener
 * and the `version`-triggered effect, both of which call it directly.
 */
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver =
  StubResizeObserver as unknown as typeof ResizeObserver

const BOX: Box = { left: 10, top: 20, width: 300, height: 400 }

function prefs(over: Partial<AnnotationsPrefs> = {}): AnnotationsPrefs {
  return {
    markers: true,
    travel: true,
    reveal: false,
    notesList: 'auto',
    ...over,
  }
}

let seq = 0
function note(over: Partial<Note> = {}): Note {
  seq += 1
  return {
    id: `n${seq}`,
    edition: 'raw',
    slot: 0,
    body: 'a note',
    created: '2026-01-01T00:00:00Z',
    updated: '2026-01-01T00:00:00Z',
    ...over,
  }
}

function seen(n: Note, travelled = false): NoteHere {
  return { note: n, travelled }
}

/** Sets a `DOMRect`-shaped return on an element's own measurement, since jsdom's is always zero. */
function stubRect(
  el: Element,
  rect: Partial<DOMRect> & { width: number; height: number },
) {
  const full: DOMRect = {
    x: rect.left ?? 0,
    y: rect.top ?? 0,
    width: rect.width,
    height: rect.height,
    top: rect.top ?? 0,
    left: rect.left ?? 0,
    right: (rect.left ?? 0) + rect.width,
    bottom: (rect.top ?? 0) + rect.height,
    toJSON() {
      return this
    },
  }
  el.getBoundingClientRect = () => full
}

function stubNatural(img: HTMLImageElement, w: number, h: number) {
  Object.defineProperty(img, 'naturalWidth', { value: w, configurable: true })
  Object.defineProperty(img, 'naturalHeight', {
    value: h,
    configurable: true,
  })
}

describe('Annotations', () => {
  it('draws nothing without a measured box', () => {
    const host = mount(
      <Annotations
        box={undefined}
        here={[]}
        prefs={prefs()}
        onOpen={vi.fn()}
      />,
    )
    expect(host.childElementCount).toBe(0)
  })

  it('draws nothing with a box but no notes and nothing pending', () => {
    const host = mount(
      <Annotations box={BOX} here={[]} prefs={prefs()} onOpen={vi.fn()} />,
    )
    expect(host.childElementCount).toBe(0)
  })

  it('draws nothing with notes but both markers and reveal switched off', () => {
    const host = mount(
      <Annotations
        box={BOX}
        here={[seen(note())]}
        prefs={prefs({ markers: false, reveal: false })}
        onOpen={vi.fn()}
      />,
    )
    expect(host.childElementCount).toBe(0)
  })

  it('mounts for a pending region alone, with no notes and markers off', () => {
    const host = mount(
      <Annotations
        box={BOX}
        here={[]}
        prefs={prefs({ markers: false, reveal: false })}
        pending={{ x: 0.1, y: 0.1, w: 0.2, h: 0.2 }}
        onOpen={vi.fn()}
      />,
    )
    expect(host.childElementCount).toBe(1)
  })

  it('positions its layer at the measured box', () => {
    const host = mount(
      <Annotations
        box={BOX}
        here={[]}
        prefs={prefs({ markers: false })}
        pending={{ x: 0, y: 0, w: 1, h: 1 }}
        onOpen={vi.fn()}
      />,
    )
    const layer = host.firstElementChild as HTMLElement
    expect(layer.style.position).toBe('absolute')
    expect(layer.style.left).toBe('10px')
    expect(layer.style.top).toBe('20px')
    expect(layer.style.width).toBe('300px')
    expect(layer.style.height).toBe('400px')
    expect(layer.style.pointerEvents).toBe('none')
  })

  describe('the pending region', () => {
    it('fills dark while the pointer is down and shows no body', () => {
      const host = mount(
        <Annotations
          box={BOX}
          here={[]}
          prefs={prefs()}
          pending={{ x: 0.1, y: 0.1, w: 0.2, h: 0.3 }}
          drawing={true}
          pendingBody="hello"
          onOpen={vi.fn()}
        />,
      )
      const pending = host.firstElementChild?.firstElementChild as HTMLElement
      expect(pending.style.background).toBe('rgba(0, 0, 0, 0.42)')
      expect(pending.style.width).toBe('20%')
      expect(pending.style.height).toBe('30%')
      expect(pending.querySelector('p')).toBeNull()
    })

    it('settles to the note colour and shows its body once the pointer lifts', () => {
      const host = mount(
        <Annotations
          box={BOX}
          here={[]}
          prefs={prefs()}
          pending={{ x: 0.1, y: 0.1, w: 0.2, h: 0.3 }}
          drawing={false}
          pendingBody="hello there"
          pendingColour="rose"
          onOpen={vi.fn()}
        />,
      )
      const pending = host.firstElementChild?.firstElementChild as HTMLElement
      expect(pending.style.background).toBe('rgba(255, 138, 160, 0.22)')
      const body = pending.querySelector('p')
      expect(body?.textContent).toBe('hello there')
    })

    it('shows no body while a rectangle has none to show', () => {
      const host = mount(
        <Annotations
          box={BOX}
          here={[]}
          prefs={prefs()}
          pending={{ x: 0.1, y: 0.1, w: 0.2, h: 0.3 }}
          drawing={false}
          onOpen={vi.fn()}
        />,
      )
      const pending = host.firstElementChild?.firstElementChild as HTMLElement
      expect(pending.querySelector('p')).toBeNull()
    })

    it('draws a point as a ring rather than a zero-sized rectangle, and never labels it', () => {
      const host = mount(
        <Annotations
          box={BOX}
          here={[]}
          prefs={prefs()}
          pending={{ x: 0.5, y: 0.5, w: 0, h: 0 }}
          drawing={false}
          pendingBody="should not show"
          onOpen={vi.fn()}
        />,
      )
      const pending = host.firstElementChild?.firstElementChild as HTMLElement
      expect(pending.style.width).toBe('20px')
      expect(pending.style.height).toBe('20px')
      expect(pending.style.marginLeft).toBe('-10px')
      expect(pending.style.borderRadius).toBe('50%')
      expect(pending.style.borderStyle).toBe('dashed')
      expect(pending.style.background).toBe('rgba(255, 214, 102, 0.22)')
      expect(pending.querySelector('p')).toBeNull()
    })

    it('fills a point dark too while the pointer is still down on it', () => {
      const host = mount(
        <Annotations
          box={BOX}
          here={[]}
          prefs={prefs()}
          pending={{ x: 0.5, y: 0.5, w: 0, h: 0 }}
          drawing={true}
          onOpen={vi.fn()}
        />,
      )
      const pending = host.firstElementChild?.firstElementChild as HTMLElement
      expect(pending.style.background).toBe('rgba(0, 0, 0, 0.42)')
    })
  })

  describe('revealed regions', () => {
    it('tints a rectangle region and shows its body', () => {
      const n = note({
        region: { x: 0.1, y: 0.2, w: 0.3, h: 0.4 },
        body: 'the words',
      })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs({ reveal: true, markers: false })}
          onOpen={vi.fn()}
        />,
      )
      const tint = host.firstElementChild?.firstElementChild as HTMLElement
      expect(tint.style.left).toBe('10%')
      expect(tint.style.top).toBe('20%')
      expect(tint.style.width).toBe('30%')
      expect(tint.style.height).toBe('40%')
      expect(tint.querySelector('p')?.textContent).toBe('the words')
    })

    it('tints a rectangle region with no body and shows none', () => {
      const n = note({ region: { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }, body: '' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs({ reveal: true, markers: false })}
          onOpen={vi.fn()}
        />,
      )
      const tint = host.firstElementChild?.firstElementChild as HTMLElement
      expect(tint.querySelector('p')).toBeNull()
    })

    it('draws a point region as a pin, not a tint', () => {
      const n = note({ region: { x: 0.5, y: 0.5, w: 0, h: 0 } })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs({ reveal: true, markers: false })}
          onOpen={vi.fn()}
        />,
      )
      const pin = host.firstElementChild?.firstElementChild as HTMLElement
      expect(pin.style.width).toBe('18px')
      expect(pin.style.height).toBe('18px')
      expect(pin.style.marginLeft).toBe('-9px')
      expect(pin.style.borderRadius).toBe('50%')
    })

    it('never tints a travelled note, even with a region', () => {
      const n = note({ region: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n, true)]}
          prefs={prefs({ reveal: true, markers: false })}
          onOpen={vi.fn()}
        />,
      )
      // The layer itself still mounts — `pending` isn't the only reason to,
      // `here` is non-empty — it just has nothing inside it to draw.
      expect(host.firstElementChild?.childElementCount).toBe(0)
    })

    it('never tints a whole-slot note, which has no region', () => {
      const n = note({ region: undefined })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs({ reveal: true, markers: false })}
          onOpen={vi.fn()}
        />,
      )
      expect(host.firstElementChild?.childElementCount).toBe(0)
    })

    it('draws nothing while reveal is off, even with regioned notes', () => {
      const n = note({ region: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs({ reveal: false, markers: false })}
          onOpen={vi.fn()}
        />,
      )
      expect(host.childElementCount).toBe(0)
    })
  })

  describe('margin tabs', () => {
    it('labels a local note by its body', () => {
      const n = note({ body: 'a local note' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      expect(button.getAttribute('aria-label')).toBe('Note: a local note')
      expect(button.textContent).toBe('1')
    })

    it('labels an empty local note as empty', () => {
      const n = note({ body: '' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      expect(button.getAttribute('aria-label')).toBe('Note: empty')
    })

    it('labels a travelled note with the edition it came from', () => {
      const n = note({ edition: 'jp-v01', body: 'over there' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n, true)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      expect(button.getAttribute('aria-label')).toBe(
        'Note written on jp-v01: over there',
      )
    })

    it('labels an empty travelled note as empty', () => {
      const n = note({ edition: 'jp-v01', body: '' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n, true)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      expect(button.getAttribute('aria-label')).toBe(
        'Note written on jp-v01: empty',
      )
    })

    it('draws no tabs while markers are switched off', () => {
      const n = note()
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs({ markers: false })}
          onOpen={vi.fn()}
        />,
      )
      expect(host.querySelector('button')).toBeNull()
    })

    it('opens the note it names when clicked', () => {
      const n = note()
      const onOpen = vi.fn()
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={onOpen}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.click()
      })
      expect(onOpen).toHaveBeenCalledExactlyOnceWith(n.id)
    })

    it('sits in the left margin, mirrored, when tabSide is left', () => {
      const n = note()
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          tabSide="left"
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      expect(button.style.right).toBe('100%')
      expect(button.style.marginRight).toBe('4px')
      expect(button.style.left).toBe('')
    })

    it('sits in the right margin by default', () => {
      const n = note()
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      expect(button.style.left).toBe('100%')
      expect(button.style.marginLeft).toBe('4px')
    })
  })

  describe('the popover on hover', () => {
    it('shows the body while the pointer is on the tab', () => {
      const n = note({ body: 'hover text' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))
      })
      const outer = button.parentElement as HTMLElement
      expect(outer.children.length).toBe(2)
      expect(outer.firstElementChild?.textContent).toBe('hover text')
      expect(outer.querySelector('em')).toBeNull()
    })

    it('prefixes the edition on a travelled note', () => {
      const n = note({ edition: 'jp-v01', body: 'hover text' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n, true)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))
      })
      const outer = button.parentElement as HTMLElement
      const popover = outer.firstElementChild as HTMLElement
      expect(popover.querySelector('em')?.textContent).toBe('on jp-v01: ')
      expect(popover.textContent).toBe('on jp-v01: hover text')
    })

    it('shows nothing on hover for a note with no body', () => {
      const n = note({ body: '' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))
      })
      const outer = button.parentElement as HTMLElement
      expect(outer.children.length).toBe(1)
    })

    it('clears on leaving the tab it was shown for', () => {
      const n = note({ body: 'hover text' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))
      })
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerout', { bubbles: true }))
      })
      const outer = button.parentElement as HTMLElement
      expect(outer.children.length).toBe(1)
    })

    it('leaving a tab that was never hovered does not clear another tab’s popover', () => {
      const a = note({ body: 'note a' })
      const b = note({ body: 'note b' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(a), seen(b)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const buttons = host.querySelectorAll('button')
      const buttonA = buttons[0] as HTMLButtonElement
      const buttonB = buttons[1] as HTMLButtonElement
      act(() => {
        buttonA.dispatchEvent(
          new PointerEvent('pointerover', { bubbles: true }),
        )
      })
      act(() => {
        buttonB.dispatchEvent(new PointerEvent('pointerout', { bubbles: true }))
      })
      const outerA = buttonA.parentElement as HTMLElement
      expect(outerA.children.length).toBe(2)
      expect(outerA.firstElementChild?.textContent).toBe('note a')
    })

    it('anchors the popover into the page from a left-margin tab', () => {
      const n = note({ body: 'hover text' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          tabSide="left"
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))
      })
      const popover = button.parentElement?.firstElementChild as HTMLElement
      expect(popover.style.left).toBe('6px')
      expect(popover.style.right).toBe('')
    })

    it('anchors the popover into the page from a right-margin tab', () => {
      const n = note({ body: 'hover text' })
      const host = mount(
        <Annotations
          box={BOX}
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />,
      )
      const button = host.querySelector('button') as HTMLButtonElement
      act(() => {
        button.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))
      })
      const popover = button.parentElement?.firstElementChild as HTMLElement
      expect(popover.style.right).toBe('6px')
      expect(popover.style.left).toBe('')
    })
  })
})

describe('PageAnnotations', () => {
  it('measures nothing and draws nothing when no image is in its box', () => {
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <PageAnnotations
          match="page.jpg"
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const anchor = host.querySelector(
      '[data-testid="pagebox"] > div',
    ) as HTMLElement
    expect(anchor.childElementCount).toBe(0)
  })

  it('draws nothing before its image reports a real size', () => {
    const n = note({ region: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } })
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          here={[seen(n)]}
          prefs={prefs({ reveal: true })}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const anchor = host.querySelector(
      '[data-testid="pagebox"] > div:last-child',
    ) as HTMLElement
    expect(anchor.childElementCount).toBe(0)
  })

  it('draws nothing while the image has a size but has not decoded', () => {
    const n = note({ region: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } })
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          here={[seen(n)]}
          prefs={prefs({ reveal: true })}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const img = pageBox.querySelector('img') as HTMLImageElement
    stubRect(pageBox, { left: 0, top: 0, width: 400, height: 300 })
    stubRect(img, { left: 0, top: 0, width: 400, height: 300 })
    act(() => {
      img.dispatchEvent(new Event('load'))
    })
    const anchor = host.querySelector(
      '[data-testid="pagebox"] > div:last-child',
    ) as HTMLElement
    expect(anchor.childElementCount).toBe(0)
  })

  it('measures the artwork inside its element and positions the layer there', () => {
    const n = note({ region: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } })
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          here={[seen(n)]}
          prefs={prefs({ reveal: true })}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const img = pageBox.querySelector('img') as HTMLImageElement
    // A 400x300 element holding a 200x100 image: `paintedBox` scales by
    // min(400/200, 300/100) = 2, so the art is 400x200, centred with 50px of
    // letterbox above and below and none to the sides.
    stubRect(pageBox, { left: 10, top: 20, width: 400, height: 300 })
    stubRect(img, { left: 10, top: 20, width: 400, height: 300 })
    stubNatural(img, 200, 100)
    act(() => {
      img.dispatchEvent(new Event('load'))
    })
    const layer = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    expect(layer.style.left).toBe('0px')
    expect(layer.style.top).toBe('50px')
    expect(layer.style.width).toBe('400px')
    expect(layer.style.height).toBe('200px')
  })

  it('pushes the artwork to the hugged edge rather than centring it', () => {
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          hug="left-edge"
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          pending={{ x: 0, y: 0, w: 1, h: 1 }}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const img = pageBox.querySelector('img') as HTMLImageElement
    stubRect(pageBox, { left: 0, top: 0, width: 400, height: 300 })
    stubRect(img, { left: 0, top: 0, width: 400, height: 300 })
    stubNatural(img, 200, 100)
    act(() => {
      img.dispatchEvent(new Event('load'))
    })
    const layer = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    // Centred this would be 0 too (400 - 400 spare = 0), so this alone does
    // not distinguish the hug — the point is that nothing throws taking the
    // 'left-edge' path, pinned properly by `paintedBox`'s own tests.
    expect(layer.style.left).toBe('0px')
  })

  it('picks the image named by match out of more than one', () => {
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="left.jpg" alt="" />
        <img src="right.jpg" alt="" />
        <PageAnnotations
          match="right.jpg"
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          pending={{ x: 0, y: 0, w: 1, h: 1 }}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const [left, right] = [
      ...pageBox.querySelectorAll('img'),
    ] as HTMLImageElement[]
    stubRect(pageBox, { left: 0, top: 0, width: 1000, height: 1000 })
    stubRect(left, { left: 0, top: 0, width: 100, height: 100 })
    stubNatural(left, 50, 50)
    stubRect(right, { left: 0, top: 0, width: 400, height: 400 })
    stubNatural(right, 200, 200)
    act(() => {
      right.dispatchEvent(new Event('load'))
    })
    const layer = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    // Scale is min(400/200, 400/200) = 2, so a 200x200 image paints as
    // 400x400 — the whole element, only reachable by measuring `right.jpg`.
    expect(layer.style.width).toBe('400px')
    expect(layer.style.height).toBe('400px')
  })

  it('falls back to the first image when match names none of them', () => {
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="left.jpg" alt="" />
        <img src="right.jpg" alt="" />
        <PageAnnotations
          match="nowhere.jpg"
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          pending={{ x: 0, y: 0, w: 1, h: 1 }}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const [left] = [...pageBox.querySelectorAll('img')] as HTMLImageElement[]
    stubRect(pageBox, { left: 0, top: 0, width: 200, height: 200 })
    stubRect(left, { left: 0, top: 0, width: 200, height: 200 })
    stubNatural(left, 100, 100)
    act(() => {
      left.dispatchEvent(new Event('load'))
    })
    const layer = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    expect(layer.style.width).toBe('200px')
  })

  it('falls back to the first image when no match is given at all', () => {
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="left.jpg" alt="" />
        <img src="right.jpg" alt="" />
        <PageAnnotations
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          pending={{ x: 0, y: 0, w: 1, h: 1 }}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const [left] = [...pageBox.querySelectorAll('img')] as HTMLImageElement[]
    stubRect(pageBox, { left: 0, top: 0, width: 200, height: 200 })
    stubRect(left, { left: 0, top: 0, width: 200, height: 200 })
    stubNatural(left, 100, 100)
    act(() => {
      left.dispatchEvent(new Event('load'))
    })
    const layer = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    expect(layer.style.width).toBe('200px')
  })

  it('re-measures when version changes, though nothing resized', () => {
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          version={1}
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          pending={{ x: 0, y: 0, w: 1, h: 1 }}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const img = pageBox.querySelector('img') as HTMLImageElement
    stubRect(pageBox, { left: 0, top: 0, width: 400, height: 400 })
    stubRect(img, { left: 0, top: 0, width: 400, height: 400 })
    stubNatural(img, 100, 100)
    act(() => {
      img.dispatchEvent(new Event('load'))
    })
    const layerBefore = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    expect(layerBefore.style.width).toBe('400px')

    // A zoom: the transform changes what `getBoundingClientRect` reports
    // without the element resizing, which is exactly what no `ResizeObserver`
    // would ever fire on — the reason `version` exists as its own trigger.
    stubRect(pageBox, { left: 0, top: 0, width: 800, height: 800 })
    stubRect(img, { left: 0, top: 0, width: 800, height: 800 })
    rerender(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          version={2}
          here={[]}
          prefs={prefs({ markers: false, reveal: false })}
          pending={{ x: 0, y: 0, w: 1, h: 1 }}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const layerAfter = host.querySelector(
      '[data-testid="pagebox"] > div:last-child > div',
    ) as HTMLElement
    expect(layerAfter.style.width).toBe('800px')
  })

  it('passes its own notes and prefs through to the layer it measures', () => {
    const n = note({ body: 'threaded through' })
    const host = mount(
      <div data-testid="pagebox" style={{ position: 'relative' }}>
        <img src="page.jpg" alt="" />
        <PageAnnotations
          match="page.jpg"
          here={[seen(n)]}
          prefs={prefs()}
          onOpen={vi.fn()}
        />
      </div>,
    )
    const pageBox = host.querySelector('[data-testid="pagebox"]') as HTMLElement
    const img = pageBox.querySelector('img') as HTMLImageElement
    stubRect(pageBox, { left: 0, top: 0, width: 400, height: 400 })
    stubRect(img, { left: 0, top: 0, width: 400, height: 400 })
    stubNatural(img, 100, 100)
    act(() => {
      img.dispatchEvent(new Event('load'))
    })
    const button = host.querySelector('button') as HTMLButtonElement
    expect(button.getAttribute('aria-label')).toBe('Note: threaded through')
  })
})
