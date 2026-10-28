/**
 * The read store's rules (spec §3), each against a client call the test
 * answers by hand, so the order answers land in is the test's to choose.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  emptyReads,
  entryOf,
  invalidate,
  markStale,
  read,
  readNow,
  readSet,
  subscribe,
} from '../src/reads'
import { address } from './support/reads'

const subscriptions: (() => void)[] = []

/** A listener on `path`, which makes it drawn; taken off after the test. */
function listen(path: string) {
  const listener = vi.fn()
  subscriptions.push(subscribe(path, listener))
  return listener
}

/**
 * A listener on `path` the test takes off itself; taken off after the test
 * too, so a test that fails first leaves nothing drawn for the next.
 */
function listenUntilOff(path: string, listener: () => void = vi.fn()) {
  const off = subscribe(path, listener)
  subscriptions.push(off)
  return off
}

/** Lets every request already settled run its handlers. */
const landed = () => new Promise((resolve) => setTimeout(resolve, 0))

afterEach(() => {
  for (const off of subscriptions.splice(0)) off()
  emptyReads()
})

describe('the read store', () => {
  it('joins a request already out, and starts one once it is applied (§3.2)', async () => {
    const a = address<{ n: number }>('/api/a')
    const first = readSet([a.read])
    const second = readSet([a.read])
    expect(a.fetch).toHaveBeenCalledTimes(1)

    a.requests[0].resolve({ n: 1 })
    await Promise.all([first, second])
    expect(entryOf(a.read)?.answer).toEqual({ n: 1 })

    void readSet([a.read])
    expect(a.fetch).toHaveBeenCalledTimes(2)
  })

  it('starts a second request on invalidate while one is out (§3.2)', async () => {
    const a = address<string>('/api/a')
    listen('/api/a')
    const out = readSet([a.read])
    const again = invalidate(a.read)
    expect(a.fetch).toHaveBeenCalledTimes(2)

    a.requests[0].resolve('before the write')
    a.requests[1].resolve('after the write')
    await Promise.all([out, again])
    expect(entryOf(a.read)?.answer).toBe('after the write')
  })

  it('drops an older request’s answer that lands after the newer one’s (§3.3)', async () => {
    const a = address<string>('/api/a')
    listen('/api/a')
    const older = readSet([a.read])
    const newer = invalidate(a.read)

    a.requests[1].resolve('newer')
    await newer
    expect(entryOf(a.read)?.answer).toBe('newer')

    a.requests[0].resolve('older')
    await older
    expect(entryOf(a.read)?.answer).toBe('newer')
  })

  it('drops an older request’s answer that lands first, and keeps the newer one’s (§3.3)', async () => {
    const a = address<string>('/api/a')
    listen('/api/a')
    const first = readSet([a.read])
    a.requests[0].resolve('first')
    await first

    const older = readSet([a.read])
    const newer = invalidate(a.read)
    a.requests[1].resolve('older')
    await older
    // The older request may predate the write: what is kept is still the
    // answer from before, marked stale, until the newer one lands.
    expect(entryOf(a.read)).toMatchObject({ answer: 'first', stale: true })

    a.requests[2].resolve('newer')
    await newer
    expect(entryOf(a.read)).toMatchObject({ answer: 'newer', stale: false })
  })

  it('leaves the entry and the answer the same objects for the same text, and tells no one (§3.4)', async () => {
    const a = address<{ n: number }>('/api/a')
    const heard = listen('/api/a')
    const first = readSet([a.read])
    a.requests[0].resolve({ n: 1 })
    await first
    const entry = entryOf(a.read)
    const answer = entry?.answer
    expect(heard).toHaveBeenCalledTimes(1)

    const same = readSet([a.read])
    a.requests[1].resolve({ n: 1 })
    await same
    expect(entryOf(a.read)).toBe(entry)
    expect(entryOf(a.read)?.answer).toBe(answer)
    expect(heard).toHaveBeenCalledTimes(1)

    // The control: a different text is a new entry, and is heard.
    const changed = readSet([a.read])
    a.requests[2].resolve({ n: 2 })
    await changed
    expect(entryOf(a.read)).not.toBe(entry)
    expect(entryOf(a.read)?.answer).toEqual({ n: 2 })
    expect(heard).toHaveBeenCalledTimes(2)
  })

  it('keeps the answer beside a failure, and the next good answer clears it (§3.5)', async () => {
    const a = address<{ n: number }>('/api/a')
    const refused = new Error('the first read failed')
    const first = readSet([a.read])
    a.requests[0].reject(refused)
    await first
    expect(entryOf(a.read)).toEqual({
      answer: undefined,
      failure: refused,
      stale: false,
    })

    const second = readSet([a.read])
    a.requests[1].resolve({ n: 1 })
    await second
    const answer = entryOf(a.read)?.answer
    expect(entryOf(a.read)).toEqual({
      answer: { n: 1 },
      failure: undefined,
      stale: false,
    })

    const down = new Error('the server is down')
    const third = readSet([a.read])
    a.requests[2].reject(down)
    await third
    expect(entryOf(a.read)?.answer).toBe(answer)
    expect(entryOf(a.read)?.failure).toBe(down)

    const fourth = readSet([a.read])
    a.requests[3].resolve({ n: 1 })
    await fourth
    expect(entryOf(a.read)?.answer).toBe(answer)
    expect(entryOf(a.read)?.failure).toBeUndefined()
  })

  it('re-reads on invalidate what something draws, only marks the rest, and reads a marked one when asked (§3.6)', async () => {
    const drawn = address<string>('/api/drawn')
    const hidden = address<string>('/api/hidden')
    listen('/api/drawn')
    const both = readSet([drawn.read, hidden.read])
    drawn.requests[0].resolve('one')
    hidden.requests[0].resolve('one')
    await both

    const again = invalidate(drawn.read, hidden.read)
    expect(drawn.fetch).toHaveBeenCalledTimes(2)
    expect(hidden.fetch).toHaveBeenCalledTimes(1)
    expect(entryOf(drawn.read)?.stale).toBe(true)
    expect(entryOf(hidden.read)?.stale).toBe(true)

    drawn.requests[1].resolve('two')
    await again
    expect(entryOf(drawn.read)).toMatchObject({ answer: 'two', stale: false })
    expect(entryOf(hidden.read)).toMatchObject({ answer: 'one', stale: true })

    const asked = readSet([hidden.read])
    expect(hidden.fetch).toHaveBeenCalledTimes(2)
    hidden.requests[1].resolve('one')
    await asked
    expect(entryOf(hidden.read)).toMatchObject({ answer: 'one', stale: false })
  })

  it('clears stale when a re-read fails, keeping the answer beside the failure', async () => {
    const a = address<string>('/api/a')
    listen('/api/a')
    const first = readSet([a.read])
    a.requests[0].resolve('one')
    await first

    const again = invalidate(a.read)
    expect(entryOf(a.read)?.stale).toBe(true)
    const down = new Error('the server is down')
    a.requests[1].reject(down)
    await again
    expect(entryOf(a.read)).toEqual({
      answer: 'one',
      failure: down,
      stale: false,
    })
  })

  it('neither joins nor keeps a request out for an address invalidated while nothing draws it (§3.2)', async () => {
    const a = address<string>('/api/a')
    const out = readSet([a.read])
    void invalidate(a.read)
    const asked = readSet([a.read])
    expect(a.fetch).toHaveBeenCalledTimes(2)

    a.requests[0].resolve('before the write')
    await out
    expect(entryOf(a.read)).toBeUndefined()

    a.requests[1].resolve('after the write')
    await asked
    expect(entryOf(a.read)?.answer).toBe('after the write')
  })

  it('counts a listener taken off as drawing nothing, and does not call it', async () => {
    const a = address<string>('/api/a')
    const heard = vi.fn()
    const off = subscribe('/api/a', heard)
    const first = readSet([a.read])
    a.requests[0].resolve('one')
    await first
    expect(heard).toHaveBeenCalledTimes(1)

    off()
    const marked = invalidate(a.read)
    expect(a.fetch).toHaveBeenCalledTimes(1)
    await marked
    const asked = readSet([a.read])
    a.requests[1].resolve('two')
    await asked
    expect(entryOf(a.read)?.answer).toBe('two')
    expect(heard).toHaveBeenCalledTimes(1)
  })

  describe('markStale (§9’s call 9)', () => {
    it('sends nothing and keeps nothing for an address with nothing kept, though something draws it', async () => {
      const a = address<string>('/api/a')
      listen('/api/a')
      markStale(a.read)
      await landed()
      expect(a.fetch).not.toHaveBeenCalled()
      expect(entryOf(a.read)).toBeUndefined()
    })

    it('marks a kept address nothing draws stale, drops an answer to a request from before the mark, and the next read waits for its own', async () => {
      const a = address<string>('/api/a')
      const first = readSet([a.read])
      a.requests[0].resolve('one')
      await first
      const before = readSet([a.read])
      markStale(a.read)
      expect(entryOf(a.read)).toMatchObject({ answer: 'one', stale: true })

      a.requests[1].resolve('before the mark')
      await before
      expect(entryOf(a.read)).toMatchObject({ answer: 'one', stale: true })

      const arrival = readSet([a.read])
      expect(a.fetch).toHaveBeenCalledTimes(3)
      expect(entryOf(a.read)?.stale).toBe(true)
      a.requests[2].resolve('after the mark')
      await arrival
      expect(entryOf(a.read)).toMatchObject({
        answer: 'after the mark',
        stale: false,
      })
    })

    it('drops a request out for an address with nothing kept and nothing drawing it, which the next read does not join', async () => {
      const a = address<string>('/api/a')
      const before = readSet([a.read])
      markStale(a.read)
      const arrival = readSet([a.read])
      expect(a.fetch).toHaveBeenCalledTimes(2)

      a.requests[0].resolve('before the mark')
      await before
      expect(entryOf(a.read)).toBeUndefined()
      a.requests[1].resolve('after the mark')
      await arrival
      expect(entryOf(a.read)?.answer).toBe('after the mark')
    })

    it('marks a drawn address stale and tells no one: no listener is called, nothing is sent, the drawn answer stays, and a read once it is left waits for its own', async () => {
      const a = address<{ n: number }>('/api/a')
      const heard = vi.fn()
      const off = listenUntilOff('/api/a', heard)
      const first = readSet([a.read])
      a.requests[0].resolve({ n: 1 })
      await first
      expect(heard).toHaveBeenCalledTimes(1)
      const drawnAnswer = entryOf(a.read)?.answer

      markStale(a.read)
      await landed()
      expect(heard).toHaveBeenCalledTimes(1)
      expect(a.fetch).toHaveBeenCalledTimes(1)
      expect(entryOf(a.read)?.answer).toBe(drawnAnswer)
      expect(entryOf(a.read)?.stale).toBe(true)

      off()
      const arrival = readSet([a.read])
      expect(a.fetch).toHaveBeenCalledTimes(2)
      expect(entryOf(a.read)?.stale).toBe(true)
      a.requests[1].resolve({ n: 2 })
      await arrival
      expect(entryOf(a.read)).toMatchObject({ answer: { n: 2 }, stale: false })
      expect(heard).toHaveBeenCalledTimes(1)
    })

    it('answers the page drawing an address whose read was out at the mark, with an entry it can draw', async () => {
      // A page waiting on its arrival's read, as a return does to a stale
      // entry. `usePage` draws a waiting page only from an entry that is not
      // stale, so an answer kept stale here would hold the page for good.
      const a = address<string>('/api/a')
      const first = readSet([a.read])
      a.requests[0].resolve('one')
      await first
      await invalidate(a.read)
      const heard = listen('/api/a')
      const out = readSet([a.read])

      markStale(a.read)
      a.requests[1].resolve('before the mark')
      await out
      expect(heard).toHaveBeenCalledTimes(1)
      expect(entryOf(a.read)).toEqual({
        answer: 'before the mark',
        failure: undefined,
        stale: false,
      })
    })

    it('joins no request out at the mark for a drawn address once its reader has gone, and drops its answer landing with nothing drawing the address', async () => {
      const a = address<string>('/api/a')
      const off = listenUntilOff('/api/a')
      const first = readSet([a.read])
      a.requests[0].resolve('one')
      await first

      const polled = readNow(a.read)
      markStale(a.read)
      off()
      const arrival = readSet([a.read])
      expect(a.fetch).toHaveBeenCalledTimes(3)
      a.requests[1].resolve('before the mark')
      await expect(polled).resolves.toBe('before the mark')
      expect(entryOf(a.read)).toMatchObject({ answer: 'one', stale: true })
      a.requests[2].resolve('after the mark')
      await arrival
      expect(entryOf(a.read)).toMatchObject({
        answer: 'after the mark',
        stale: false,
      })

      // Out at a mark while drawn, and landing once nothing draws it.
      const back = listenUntilOff('/api/a')
      const again = readNow(a.read)
      markStale(a.read)
      back()
      a.requests[3].resolve('before the second mark')
      await expect(again).resolves.toBe('before the second mark')
      expect(entryOf(a.read)).toMatchObject({
        answer: 'after the mark',
        stale: true,
      })
    })

    it.each([
      { outcome: 'an answer', lands: 'answered' as const },
      { outcome: 'a failure', lands: 'failed' as const },
    ])(
      'marks stale, once nothing draws the address, the entry a drawn page took from $outcome to a request out at the mark, so the next arrival waits for its own read',
      async ({ lands }) => {
        const a = address<string>('/api/a')
        const first = readSet([a.read])
        a.requests[0].resolve('one')
        await first
        const off = listenUntilOff('/api/a')
        const out = readSet([a.read])

        markStale(a.read)
        if (lands === 'answered') a.requests[1].resolve('before the mark')
        else a.requests[1].reject(new Error('the server is down'))
        await out
        const kept = lands === 'answered' ? 'before the mark' : 'one'
        expect(entryOf(a.read)).toMatchObject({ answer: kept, stale: false })

        off()
        expect(entryOf(a.read)).toMatchObject({ answer: kept, stale: true })
        const arrival = readSet([a.read])
        expect(a.fetch).toHaveBeenCalledTimes(3)
        expect(entryOf(a.read)?.stale).toBe(true)
        a.requests[2].resolve('after the mark')
        await arrival
        expect(entryOf(a.read)).toMatchObject({
          answer: 'after the mark',
          stale: false,
        })
      },
    )

    it('leaves an answer to a request sent after the mark as it is once nothing draws the address, though one from before had landed while it was drawn', async () => {
      const a = address<string>('/api/a')
      const first = readSet([a.read])
      a.requests[0].resolve('one')
      await first
      const off = listenUntilOff('/api/a')
      const before = readNow(a.read)
      markStale(a.read)
      a.requests[1].resolve('before the mark')
      await before

      const after = readNow(a.read)
      a.requests[2].resolve('after the mark')
      await after
      off()
      expect(entryOf(a.read)).toMatchObject({
        answer: 'after the mark',
        stale: false,
      })

      // Marked again with nothing out: only the mark itself makes it stale.
      const back = listenUntilOff('/api/a')
      markStale(a.read)
      const polled = readNow(a.read)
      a.requests[3].resolve('after the second mark')
      await polled
      back()
      expect(entryOf(a.read)).toMatchObject({
        answer: 'after the second mark',
        stale: false,
      })
    })
  })

  it('applies a set of three when its last answer lands, telling each listener once (§3.8)', async () => {
    const one = address<string>('/api/1')
    const two = address<string>('/api/2')
    const three = address<string>('/api/3')
    const heard = [listen('/api/1'), listen('/api/2'), listen('/api/3')]
    // One function listening to all three, as a page drawn from three reads
    // does: it is told once for the set, not once per address.
    const page = vi.fn()
    for (const path of ['/api/1', '/api/2', '/api/3']) {
      subscriptions.push(subscribe(path, page))
    }

    const set = readSet([one.read, two.read, three.read])
    two.requests[0].resolve('2')
    await landed()
    three.requests[0].resolve('3')
    await landed()
    expect(entryOf(two.read)).toBeUndefined()
    expect(entryOf(three.read)).toBeUndefined()
    for (const listener of heard) expect(listener).not.toHaveBeenCalled()
    expect(page).not.toHaveBeenCalled()

    one.requests[0].resolve('1')
    await set
    expect(entryOf(one.read)?.answer).toBe('1')
    expect(entryOf(two.read)?.answer).toBe('2')
    expect(entryOf(three.read)?.answer).toBe('3')
    for (const listener of heard) expect(listener).toHaveBeenCalledTimes(1)
    expect(page).toHaveBeenCalledTimes(1)
  })

  it('changes nothing and tells no one again for a joined request another set has applied', async () => {
    const x = address<string>('/api/x')
    const y = address<string>('/api/y')
    const heardX = listen('/api/x')
    const heardY = listen('/api/y')
    const alone = readSet([x.read])
    const pair = readSet([x.read, y.read])
    expect(x.fetch).toHaveBeenCalledTimes(1)

    const down = new Error('the server is down')
    x.requests[0].reject(down)
    await alone
    const entry = entryOf(x.read)
    expect(entry?.failure).toBe(down)
    expect(heardX).toHaveBeenCalledTimes(1)

    y.requests[0].resolve('y')
    await pair
    expect(entryOf(x.read)).toBe(entry)
    expect(heardX).toHaveBeenCalledTimes(1)
    expect(heardY).toHaveBeenCalledTimes(1)
  })

  it('drops every entry on emptyReads, and an answer in flight across it (§3.9)', async () => {
    const a = address<string>('/api/a')
    const heard = listen('/api/a')
    const first = readSet([a.read])
    a.requests[0].resolve('kept')
    await first
    expect(heard).toHaveBeenCalledTimes(1)

    const across = readSet([a.read])
    emptyReads()
    expect(entryOf(a.read)).toBeUndefined()
    expect(heard).toHaveBeenCalledTimes(2)
    a.requests[1].resolve('from before the emptying')
    await across
    expect(entryOf(a.read)).toBeUndefined()

    // Nor is a request started before the emptying joined after it.
    const before = readSet([a.read])
    emptyReads()
    const after = readSet([a.read])
    expect(a.fetch).toHaveBeenCalledTimes(4)
    a.requests[2].resolve('from before the emptying')
    a.requests[3].resolve('after')
    await Promise.all([before, after])
    expect(entryOf(a.read)?.answer).toBe('after')
  })

  it('resolves readNow with its answer and keeps it, and a readSet joins it', async () => {
    const a = address<string>('/api/a')
    const heard = listen('/api/a')
    const poll = readNow(a.read)
    const set = readSet([a.read])
    expect(a.fetch).toHaveBeenCalledTimes(1)

    a.requests[0].resolve('one')
    await expect(poll).resolves.toBe('one')
    await set
    expect(entryOf(a.read)?.answer).toBe('one')
    expect(heard).toHaveBeenCalledTimes(1)
  })

  it('rejects readNow with its failure, kept beside the answer', async () => {
    const a = address<string>('/api/a')
    const first = readNow(a.read)
    a.requests[0].resolve('one')
    await first

    const down = new Error('the server is down')
    const poll = readNow(a.read)
    a.requests[1].reject(down)
    await expect(poll).rejects.toBe(down)
    expect(entryOf(a.read)).toMatchObject({ answer: 'one', failure: down })
  })

  it('starts a new request for every readNow, and keeps only the newest one’s answer', async () => {
    const a = address<string>('/api/a')
    const older = readNow(a.read)
    const newer = readNow(a.read)
    expect(a.fetch).toHaveBeenCalledTimes(2)

    a.requests[1].resolve('newer')
    await newer
    a.requests[0].resolve('older')
    await expect(older).resolves.toBe('older')
    expect(entryOf(a.read)?.answer).toBe('newer')
  })

  it('keeps a readNow’s answer over that of the readSet request it overtook, landing later', async () => {
    const a = address<string>('/api/a')
    const set = readSet([a.read])
    const poll = readNow(a.read)
    expect(a.fetch).toHaveBeenCalledTimes(2)

    a.requests[1].resolve('polled')
    await expect(poll).resolves.toBe('polled')
    a.requests[0].resolve('from the set')
    await set
    expect(entryOf(a.read)?.answer).toBe('polled')
  })

  it('keeps the same entry for a second invalidate of an entry already stale', async () => {
    // `useSyncExternalStore` compares entries by identity: a new object for
    // a mark already made would redraw a page for nothing.
    const a = address<string>('/api/a')
    const first = readSet([a.read])
    a.requests[0].resolve('one')
    await first

    await invalidate(a.read)
    const marked = entryOf(a.read)
    expect(marked?.stale).toBe(true)
    await invalidate(a.read)
    expect(entryOf(a.read)).toBe(marked)
  })

  it('fails a read whose client call throws rather than rejecting, as any other failure', async () => {
    const thrown = new Error('thrown before any promise')
    const a = read<string>('/api/a', () => {
      throw thrown
    })
    await readSet([a])
    expect(entryOf(a)).toEqual({
      answer: undefined,
      failure: thrown,
      stale: false,
    })
    await expect(readNow(a)).rejects.toBe(thrown)
  })
})
