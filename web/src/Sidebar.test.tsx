import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { Sidebar } from './Sidebar'
import { mount } from './testing/mount'

const at = (ui: React.ReactNode) => mount(<MemoryRouter>{ui}</MemoryRouter>)

describe('the sidebar', () => {
  it('always offers a way to the library and to search', () => {
    const host = at(<Sidebar roots={[]} showRoots={false} />)
    expect(host.querySelector('a[href="/"]')).not.toBeNull()
    expect(host.querySelector('a[href="/search"]')).not.toBeNull()
  })

  it('hides the roots section when it is switched off', () => {
    const host = at(
      <Sidebar roots={[{ id: 1, name: 'Manga' }]} showRoots={false} />,
    )
    expect(host.textContent).not.toContain('Manga')
  })

  it('filters the grid by root, and never by a path only the server knows', () => {
    const host = at(
      <Sidebar roots={[{ id: 4, name: 'Manga' }]} showRoots={true} />,
    )
    const link = host.querySelector('a[href="/?root=4"]')
    expect(link?.textContent).toBe('Manga')
  })

  it('shows a reader no proposals link', () => {
    // This must fail if `Sidebar.tsx`'s `admin &&` guard is ever removed —
    // an unconditional render would put the link here regardless of `admin`.
    const host = at(<Sidebar roots={[]} showRoots={false} admin={false} />)
    expect(host.querySelector('a[href="/proposals"]')).toBeNull()
  })

  it('shows an admin the proposals link', () => {
    // The other half of the guard: a mutation that always hid the link (for
    // instance replacing the condition with `false &&`) would still pass the
    // test above, and only fails here.
    const host = at(<Sidebar roots={[]} showRoots={false} admin={true} />)
    expect(host.querySelector('a[href="/proposals"]')?.textContent).toBe(
      'Proposals',
    )
  })

  it('carries the open count when it has been read', () => {
    // C3 §6 and the build design's file-structure table both ask for the
    // count; the entry shipped without one. The number is asserted through
    // its own element rather than through the link's `textContent`, so a
    // count rendered in the wrong place — or the label rendered twice —
    // cannot pass as `'Proposals7'`.
    const host = at(
      <Sidebar roots={[]} showRoots={false} admin={true} openProposals={7} />,
    )
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '7',
    )
  })

  it('draws a zero rather than hiding an inbox somebody has emptied', () => {
    // `0` and "not read yet" are different facts and the falsy test that
    // conflates them is the easy mistake here: an emptied inbox is worth
    // saying, and a badge that vanished at zero would read as a failed fetch.
    const host = at(
      <Sidebar roots={[]} showRoots={false} admin={true} openProposals={0} />,
    )
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '0',
    )
  })

  it('draws no count at all until one has been read', () => {
    // Never fetched, still in flight and failed are one value, as
    // `StatusContext`'s own doc explains for the same three states — and
    // none of them is a number this sidebar may invent.
    const host = at(<Sidebar roots={[]} showRoots={false} admin={true} />)
    expect(host.querySelector('a[href="/proposals"] .count')).toBeNull()
    expect(host.querySelector('a[href="/proposals"]')?.textContent).toBe(
      'Proposals',
    )
  })

  it('hides the proposals link when no admin bit is given at all', () => {
    // The prop is optional (`gate.ts`'s "no gate above" reading), so the
    // default itself — not just an explicit `admin={false}` — must be the
    // safe one.
    const host = at(<Sidebar roots={[]} showRoots={false} />)
    expect(host.querySelector('a[href="/proposals"]')).toBeNull()
  })
})
