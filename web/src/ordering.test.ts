import { describe, expect, it } from 'vitest'
import { createOrdering } from './ordering'

describe('createOrdering', () => {
  it('issues strictly increasing numbers, never reusing one', () => {
    const order = createOrdering()
    expect([order.next(), order.next(), order.next()]).toEqual([1, 2, 3])
  })

  it('draws the first attempt, however slowly it settles', () => {
    const order = createOrdering()
    const shown: string[] = []
    expect(order.inOrder(order.next(), 'first.jpg', (v) => shown.push(v))).toBe(
      true,
    )
    expect(shown).toEqual(['first.jpg'])
  })

  it('draws attempts that arrive in order', () => {
    const order = createOrdering()
    const shown: string[] = []
    for (const page of ['a', 'b', 'c']) {
      order.inOrder(order.next(), page, (v) => shown.push(v))
    }
    expect(shown).toEqual(['a', 'b', 'c'])
  })

  // The hazard this exists for: `createDecodeQueue` runs several decodes at
  // once and they settle in whatever order the browser finishes them. Without
  // this, a flick landing on a late page could be overwritten by an earlier
  // one arriving afterwards.
  it('drops a late arrival that a newer one has already overtaken', () => {
    const order = createOrdering()
    const shown: string[] = []
    const early = order.next()
    const late = order.next()

    order.inOrder(late, 'page-29', (v) => shown.push(v))
    const drew = order.inOrder(early, 'page-21', (v) => shown.push(v))

    expect(drew).toBe(false)
    expect(shown).toEqual(['page-29'])
  })

  it('keeps dropping older arrivals after one has been overtaken', () => {
    const order = createOrdering()
    const shown: number[] = []
    const attempts = [order.next(), order.next(), order.next(), order.next()]
    // Settling order: newest first, then the three it overtook.
    order.inOrder(attempts[3] as number, 4, (v) => shown.push(v))
    for (const a of [attempts[0], attempts[1], attempts[2]]) {
      order.inOrder(a as number, 0, (v) => shown.push(v))
    }
    expect(shown).toEqual([4])
  })

  it('draws a newer arrival after an older one, which is the ordinary case', () => {
    const order = createOrdering()
    const shown: number[] = []
    const first = order.next()
    const second = order.next()
    order.inOrder(first, 1, (v) => shown.push(v))
    expect(order.inOrder(second, 2, (v) => shown.push(v))).toBe(true)
    expect(shown).toEqual([1, 2])
  })

  it('does not call show at all for a dropped attempt, so no work is done for it', () => {
    const order = createOrdering()
    let calls = 0
    order.inOrder(order.next(), 'newer', () => {
      calls += 1
    })
    const stale = 0
    order.inOrder(stale, 'older', () => {
      calls += 1
    })
    expect(calls).toBe(1)
  })

  it('keeps two orderings independent, so one cell cannot silence the other', () => {
    // `two` mode builds one per cell; a shared counter would let the left
    // cell's progress drop the right cell's frames.
    const left = createOrdering()
    const right = createOrdering()
    const drawn: string[] = []
    left.inOrder(left.next(), 'L1', (v) => drawn.push(v))
    left.inOrder(left.next(), 'L2', (v) => drawn.push(v))
    expect(right.inOrder(right.next(), 'R1', (v) => drawn.push(v))).toBe(true)
    expect(drawn).toEqual(['L1', 'L2', 'R1'])
  })
})
