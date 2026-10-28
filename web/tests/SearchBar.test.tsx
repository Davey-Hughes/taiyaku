import { act } from 'react'
import { type Navigator, Router, type To } from 'react-router'
import { describe, expect, it } from 'vitest'
import { SearchBar } from '../src/SearchBar'
import { SEARCH_BOX_ID } from '../src/searchHits'
import { mount, rerender } from './support/mount'

/** A location as the router commits it: the query string and the state. */
interface Entry {
  search: string
  state: unknown
}

/**
 * The box under a router whose URL moves only when the test says. The real
 * router commits each URL in a transition, and a transition's commit can
 * land after the box has taken the next key: React runs the passive effects
 * of a transition's commit in a task of their own, and a key handled in
 * between finds the URL one keystroke behind. A test cannot time that
 * window, so it stages the state it leaves. `asked` is every location the
 * box has asked for, state and all, for a test to commit when it chooses.
 */
function staged() {
  const asked: Entry[] = []
  const searchOf = (to: To) => (typeof to === 'string' ? to : (to.search ?? ''))
  const navigator: Navigator = {
    createHref: searchOf,
    go: () => {},
    push: () => {},
    replace: (to: To, state?: unknown) => {
      asked.push({ search: searchOf(to), state })
    },
  }
  const at = ({ search, state }: Entry) => (
    <Router location={{ pathname: '/', search, state }} navigator={navigator}>
      <SearchBar placeholder="Search every library" />
    </Router>
  )
  const host = mount(at({ search: '', state: null }))
  const boxNow = () => {
    const box = host.querySelector<HTMLInputElement>(`#${SEARCH_BOX_ID}`)
    if (!box) throw new Error('no search box')
    return box
  }
  return {
    get box() {
      return boxNow()
    },
    asked,
    /** The location the router commits, now; a box anew after `leave`. */
    commit: (entry: Entry) => rerender(at(entry)),
    /** Unmount the box, as leaving the page does. */
    leave: () => rerender(<p>gone</p>),
    /**
     * A key, as the box sees it: the native setter, because React tracks a
     * controlled input's value and a plain assignment fires no `onChange`.
     */
    type: (value: string) =>
      act(() => {
        const box = boxNow()
        Object.getOwnPropertyDescriptor(
          HTMLInputElement.prototype,
          'value',
        )?.set?.call(box, value)
        box.dispatchEvent(new Event('input', { bubbles: true }))
      }),
  }
}

/** The location the box asked for `n`th, which the test then commits. */
function askedFor(bar: ReturnType<typeof staged>, n: number): Entry {
  const entry = bar.asked[n]
  if (!entry) throw new Error(`the box asked for no location ${n}`)
  return entry
}

describe('SearchBar', () => {
  it('keeps its own text when the URL of an earlier key commits after a later one — O223', () => {
    const bar = staged()
    bar.type('a')
    bar.type('ab')
    expect(bar.asked.map((e) => e.search)).toEqual(['?q=a', '?q=ab'])
    bar.commit(askedFor(bar, 0))
    expect(bar.box.value).toBe('ab')
    bar.commit(askedFor(bar, 1))
    expect(bar.box.value).toBe('ab')
  })

  it('takes a URL it did not ask for, once its own have landed', () => {
    const bar = staged()
    bar.type('a')
    bar.commit(askedFor(bar, 0))
    bar.commit({ search: '?q=other', state: null })
    expect(bar.box.value).toBe('other')
  })

  it('takes a URL it did not ask for after asking twice for the same text', () => {
    // "a", deleted, and "a" again before the router commits: it commits the
    // last, and a link to the page with no query follows.
    const bar = staged()
    bar.type('a')
    bar.type('')
    bar.type('a')
    expect(bar.asked.map((e) => e.search)).toEqual(['?q=a', '', '?q=a'])
    bar.commit(askedFor(bar, 2))
    expect(bar.box.value).toBe('a')
    bar.commit({ search: '', state: null })
    expect(bar.box.value).toBe('')
  })

  it('takes an older entry Back restores, though the box once asked for it', () => {
    const bar = staged()
    bar.type('a')
    bar.commit(askedFor(bar, 0))
    bar.type('ab')
    bar.commit(askedFor(bar, 1))
    // Back to the entry the first key left, its state and all.
    bar.commit(askedFor(bar, 0))
    expect(bar.box.value).toBe('a')
  })

  it('takes an entry Back restores whose own URL a link overtook', () => {
    // A key, then a link followed before the key's URL committed: the
    // router commits the link's, and Back then finds the key's entry.
    const bar = staged()
    bar.type('a')
    bar.commit({ search: '?q=x', state: null })
    expect(bar.box.value).toBe('x')
    bar.commit(askedFor(bar, 0))
    expect(bar.box.value).toBe('a')
  })

  it('takes an entry a box mounted before this one left', () => {
    const bar = staged()
    bar.type('zzz')
    const left = askedFor(bar, 0)
    bar.commit(left)
    bar.leave()
    // Back on the page, a new box, which asks for a URL of its own.
    bar.commit({ search: '', state: null })
    bar.type('a')
    // Back to the entry the first box left, before the new one's lands.
    bar.commit(left)
    expect(bar.box.value).toBe('zzz')
  })
})
