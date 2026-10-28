import { describe, expect, it } from 'vitest'
import { WHOLE, type Rect } from '../../src/viewport'
import { registry } from '../../src/modules/registry'
import {
  anchorIn,
  centreOf,
  panStep,
  swapMapKind,
  wheelStep,
  zoomable,
  zoomModule,
  zoomStep,
} from '../../src/modules/zoom'

const prefs = { ...zoomModule.defaults }
/** A rect at 2x, offset so nothing below can pass by symmetry alone. */
const zoomed: Rect = { x: 0.2, y: 0.3, w: 0.5, h: 0.5 }
const box = { left: 100, top: 50, width: 400, height: 400 }

describe('zoomModule', () => {
  it('is registered', () => {
    // `Registry.ids` is a property, not a method — see `module.ts`'s own
    // interface and `annotations.test.ts`'s `expect(registry.ids).toContain`.
    expect(registry.ids).toContain('zoom')
  })

  it("claims three keys, none of them another module's", () => {
    const mine = new Set((zoomModule.keys ?? []).map((k) => k.key))
    expect(mine).toEqual(new Set(['=', '-', '0']))
    const others = registry
      .all()
      .filter((m) => m.id !== 'zoom')
      .flatMap((m) => m.keys ?? [])
      .map((k) => k.key)
    for (const key of others) expect(mine.has(key)).toBe(false)
  })

  // `intentOf` answers ' ', arrows, Tab, 'c' and 1-9 before it reaches any
  // module, so a module claiming one of those would declare a binding that
  // never fires rather than collide loudly.
  it('claims nothing intentOf answers before modules', () => {
    const reserved = new Set([
      ' ',
      'ArrowRight',
      'ArrowLeft',
      'Tab',
      'c',
      ...'123456789',
    ])
    for (const { key } of zoomModule.keys ?? [])
      expect(reserved.has(key)).toBe(false)
  })

  it('offers onSwap as a choice rather than free text', () => {
    expect(zoomModule.options?.onSwap).toEqual(['content', 'reset'])
  })

  it('starts enabled, at fit-whole magnification', () => {
    expect(zoomModule.defaults.enabled).toBe(true)
    expect(zoomModule.defaults.onSwap).toBe('content')
    expect(zoomModule.defaults.maxScale).toBeGreaterThan(1)
    expect(zoomModule.defaults.step).toBeGreaterThan(1)
  })
})

describe('centreOf', () => {
  it('is the middle of the rect, not the middle of the group', () => {
    expect(centreOf(zoomed)).toEqual({ x: 0.45, y: 0.55 })
    expect(centreOf(WHOLE)).toEqual({ x: 0.5, y: 0.5 })
  })
})

describe('anchorIn', () => {
  // The pointer's own fractions of the box, mapped back through the rect on
  // screen: at 2x the pointer has to reach half as far into the group as it
  // reached into the page area.
  it('maps a client point through the rect currently drawn', () => {
    expect(anchorIn(WHOLE, box, { x: 300, y: 250 })).toEqual({ x: 0.5, y: 0.5 })
    expect(anchorIn(zoomed, box, { x: 300, y: 250 })).toEqual({
      x: 0.2 + 0.25,
      y: 0.3 + 0.25,
    })
  })

  it('takes the box origin off, so a page area that is not at 0,0 still anchors', () => {
    expect(anchorIn(WHOLE, box, { x: 100, y: 50 })).toEqual({ x: 0, y: 0 })
    expect(anchorIn(WHOLE, box, { x: 500, y: 450 })).toEqual({ x: 1, y: 1 })
  })

  // An element mid-teardown, not a viewport. Dividing by it puts `NaN` into
  // the rect and the page vanishes with no error anywhere.
  it('refuses a zero-sized box rather than dividing by it', () => {
    expect(
      anchorIn(WHOLE, { ...box, width: 0 }, { x: 1, y: 1 }),
    ).toBeUndefined()
    expect(
      anchorIn(WHOLE, { ...box, height: 0 }, { x: 1, y: 1 }),
    ).toBeUndefined()
    expect(
      anchorIn(WHOLE, { ...box, width: -5 }, { x: 1, y: 1 }),
    ).toBeUndefined()
  })
})

describe('zoomable', () => {
  it('needs the module on and something drawn — both, and only these', () => {
    expect(zoomable(prefs, true)).toBe(true)
    expect(zoomable(prefs, false)).toBe(false)
    expect(zoomable({ ...prefs, enabled: false }, true)).toBe(false)
    expect(zoomable({ ...prefs, enabled: false }, false)).toBe(false)
  })
})

describe('zoomStep', () => {
  it('zooms in about the rect centre, keeping it there', () => {
    const next = zoomStep(WHOLE, 'in', prefs)
    expect(next.w).toBeCloseTo(1 / prefs.step, 10)
    expect(centreOf(next).x).toBeCloseTo(0.5, 10)
    expect(centreOf(next).y).toBeCloseTo(0.5, 10)
  })

  it('zooms out about the rect centre, and stops at the whole page', () => {
    const inOnce = zoomStep(WHOLE, 'in', prefs)
    const back = zoomStep(inOnce, 'out', prefs)
    expect(back.w).toBeCloseTo(1, 10)
    // Already at fit-whole, `out` has nowhere to go.
    expect(zoomStep(WHOLE, 'out', prefs)).toEqual(WHOLE)
  })

  // `0` at fit-whole must be `Object.is`-equal to the state already held, or
  // React commits a re-render for a press that changed nothing.
  it('hands back WHOLE itself, not a copy', () => {
    expect(zoomStep(zoomed, 'whole', prefs)).toBe(WHOLE)
    expect(zoomStep(WHOLE, 'whole', prefs)).toBe(WHOLE)
  })

  // Not redundant with `zoomable` at the call site: that predicate asks a
  // second question as well, so a caller with nothing to say about `drawn`
  // still must not get a zoomed rect out of a switched-off module.
  it('returns the same rect while the module is off, for all three steps', () => {
    const off = { ...prefs, enabled: false }
    for (const which of ['in', 'out', 'whole'] as const) {
      expect(zoomStep(zoomed, which, off)).toBe(zoomed)
    }
  })

  it('honours maxScale rather than the step alone', () => {
    const tight = { ...prefs, maxScale: 2, step: 100 }
    expect(zoomStep(WHOLE, 'in', tight).w).toBeCloseTo(0.5, 10)
  })
})

describe('wheelStep', () => {
  const notch = { deltaY: -120, deltaMode: 0, ctrlKey: false }

  it('zooms in on a notch up and out on a notch down', () => {
    const at = { x: 300, y: 250 }
    const inward = wheelStep(WHOLE, notch, at, box, prefs)
    const outward = wheelStep(zoomed, { ...notch, deltaY: 120 }, at, box, prefs)
    expect(inward?.w).toBeLessThan(1)
    expect(outward?.w).toBeGreaterThan(zoomed.w)
  })

  // The whole point of anchoring: the page under the pointer stays under it.
  it('anchors on the pointer, not on the centre', () => {
    const corner = { x: 100, y: 50 }
    const next = wheelStep(WHOLE, notch, corner, box, prefs)
    expect(next?.x).toBeCloseTo(0, 10)
    expect(next?.y).toBeCloseTo(0, 10)
  })

  it('refuses an unusable box rather than putting NaN in the rect', () => {
    expect(
      wheelStep(WHOLE, notch, { x: 1, y: 1 }, { ...box, width: 0 }, prefs),
    ).toBeUndefined()
  })

  // `wheelBy`'s own rule, reached through here: a gesture that cannot be
  // interpreted is a factor of exactly one, which leaves the rect alone.
  it('leaves the rect alone for a wheel with no delta', () => {
    const still = wheelStep(
      zoomed,
      { deltaY: 0, deltaMode: 0, ctrlKey: false },
      { x: 300, y: 250 },
      box,
      prefs,
    )
    expect(still).toEqual(zoomed)
  })

  // A trackpad pinch arrives as a `wheel` with `ctrlKey` set, and is scaled
  // differently — the reason `wheelBy` takes the whole event shape.
  it('reads a pinch through wheelBy rather than as an ordinary notch', () => {
    const at = { x: 300, y: 250 }
    const pinch = wheelStep(
      WHOLE,
      { deltaY: -120, deltaMode: 0, ctrlKey: true },
      at,
      box,
      prefs,
    )
    const wheel = wheelStep(WHOLE, notch, at, box, prefs)
    expect(pinch?.w).not.toBeCloseTo(wheel?.w ?? 0, 6)
  })
})

describe('panStep', () => {
  const size = { width: 400, height: 400 }

  // A drag moves the page under the viewport, so the rect moves the other
  // way: dragging right shows what was to the left.
  it('moves the rect against the drag, scaled by the rect', () => {
    const next = panStep(zoomed, size, { x: 200, y: 200 }, { x: 240, y: 200 })
    // 40px of a 400px box is a tenth of the box, and the rect is showing half
    // the group, so the rect moves 0.05 — the other way.
    expect(next.x).toBeCloseTo(0.15, 10)
    expect(next.y).toBeCloseTo(0.3, 10)
  })

  // The bail-out this exists for: at fit-whole there is nowhere to pan, and a
  // fresh equal rect per pointer move re-renders the reader for nothing.
  it('hands back the same rect object where the pan had nowhere to go', () => {
    expect(panStep(WHOLE, size, { x: 200, y: 200 }, { x: 260, y: 240 })).toBe(
      WHOLE,
    )
  })

  it('hands back the same rect object when the pointer did not move', () => {
    expect(panStep(zoomed, size, { x: 200, y: 200 }, { x: 200, y: 200 })).toBe(
      zoomed,
    )
  })

  // Clamped, so a drag that runs past the edge stops at it rather than
  // sailing off and leaving the reader looking at background.
  it('clamps at the edge instead of sailing past it', () => {
    const next = panStep(zoomed, size, { x: 200, y: 200 }, { x: 2000, y: 2000 })
    expect(next.x).toBe(0)
    expect(next.y).toBe(0)
    expect(next.w).toBe(zoomed.w)
  })
})

describe('swapMapKind', () => {
  // The measured default. `'content'` means "map through whichever derived
  // transform wins", and `mapRect`'s corpus measurement says that is
  // `'pillarbox'` — not the `'letterbox'` this line held until 2026-08-08.
  it('maps content through pillarbox', () => {
    expect(swapMapKind({ ...prefs, onSwap: 'content' })).toBe('pillarbox')
  })

  it('throws the view away when the reader asks it to', () => {
    expect(swapMapKind({ ...prefs, onSwap: 'reset' })).toBe('reset')
  })
})
