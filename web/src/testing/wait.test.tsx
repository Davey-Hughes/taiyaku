import { describe, expect, it } from 'vitest'
import { watchDrawn } from './wait'

/** Lets a `MutationObserver`'s callback, queued by a change, run. */
const observed = () => new Promise((resolve) => setTimeout(resolve, 0))

// The two cases run in order: the first leaves its watch running, as a test
// that fails before `stop` does, and the second is the test after it.
describe('watchDrawn', () => {
  let reads = 0

  it('reads on each change to the document while it watches', async () => {
    watchDrawn(() => {
      reads += 1
      return ''
    })
    const before = reads
    document.body.append(document.createElement('p'))
    await observed()
    expect(reads).toBeGreaterThan(before)
  })

  it('stops watching when the test that began the watch ends without stop', async () => {
    expect(reads, 'the case before this one began a watch').toBeGreaterThan(0)
    const before = reads
    document.body.append(document.createElement('p'))
    await observed()
    expect(reads).toBe(before)
  })
})
