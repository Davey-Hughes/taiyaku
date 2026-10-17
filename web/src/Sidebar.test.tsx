import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { Sidebar } from './Sidebar'
import { mount } from './testing/mount'

const at = (ui: React.ReactNode) => mount(<MemoryRouter>{ui}</MemoryRouter>)

describe('the sidebar', () => {
  it('always offers a way to the library and to search', () => {
    const host = at(<Sidebar libraries={[]} />)
    expect(host.querySelector('a[href="/"]')).not.toBeNull()
    expect(host.querySelector('a[href="/search"]')).not.toBeNull()
  })

  it('lists every library it is given, each linking to its own page', () => {
    const host = at(
      <Sidebar
        libraries={[
          { id: 4, name: 'Manga', scanned: true },
          { id: 9, name: 'Comics', scanned: false },
        ]}
      />,
    )
    const links = [...host.querySelectorAll('.sidebar-libraries a')]
    expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
      ['Manga', '/?library=4'],
      ['Comics', '/?library=9'],
    ])
    expect(host.querySelector('h2')?.textContent).toBe('Libraries')
  })

  it('draws no libraries heading when there is no library', () => {
    const host = at(<Sidebar libraries={[]} />)
    expect(host.textContent).not.toContain('Libraries')
  })

  it('shows a reader no proposals link', () => {
    // This must fail if `Sidebar.tsx`'s `admin &&` guard is ever removed —
    // an unconditional render would put the link here regardless of `admin`.
    const host = at(<Sidebar libraries={[]} admin={false} />)
    expect(host.querySelector('a[href="/proposals"]')).toBeNull()
  })

  it('shows an admin the proposals link', () => {
    // The other half of the guard: a mutation that always hid the link (for
    // instance replacing the condition with `false &&`) would still pass the
    // test above, and only fails here.
    const host = at(<Sidebar libraries={[]} admin={true} />)
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
    const host = at(<Sidebar libraries={[]} admin={true} openProposals={7} />)
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '7',
    )
  })

  it('draws a zero rather than hiding an inbox somebody has emptied', () => {
    // `0` and "not read yet" are different facts and the falsy test that
    // conflates them is the easy mistake here: an emptied inbox is worth
    // saying, and a badge that vanished at zero would read as a failed fetch.
    const host = at(<Sidebar libraries={[]} admin={true} openProposals={0} />)
    expect(host.querySelector('a[href="/proposals"] .count')?.textContent).toBe(
      '0',
    )
  })

  it('draws no count at all until one has been read', () => {
    // Never fetched, still in flight and failed are one value, as
    // `StatusContext`'s own doc explains for the same three states — and
    // none of them is a number this sidebar may invent.
    const host = at(<Sidebar libraries={[]} admin={true} />)
    expect(host.querySelector('a[href="/proposals"] .count')).toBeNull()
    expect(host.querySelector('a[href="/proposals"]')?.textContent).toBe(
      'Proposals',
    )
  })

  it('hides the proposals link when no admin bit is given at all', () => {
    // The prop is optional (`gate.ts`'s "no gate above" reading), so the
    // default itself — not just an explicit `admin={false}` — must be the
    // safe one.
    const host = at(<Sidebar libraries={[]} />)
    expect(host.querySelector('a[href="/proposals"]')).toBeNull()
  })

  it('draws the configuration link for an admin and not for a reader', () => {
    // Spec C4 is admin-gated throughout, and the link takes the same `admin`
    // prop the proposals entry does rather than a second one. What an account
    // may *see* is the server's to enforce; this chooses only what to draw.
    const withAdmin = at(<Sidebar libraries={[]} admin />)
    expect(withAdmin.querySelector('a[href="/config"]')).not.toBeNull()

    const withoutAdmin = at(<Sidebar libraries={[]} />)
    expect(withoutAdmin.querySelector('a[href="/config"]')).toBeNull()
  })

  it('offers a reader the account screen, which the two entries above do not', () => {
    // O94. The account screen is where every account changes its own
    // password, so this link takes no `admin &&` — and copying the guard the
    // configuration link sits behind is the easy mistake, since the two are
    // neighbours. Both non-admin shapes: `admin={false}`, and the prop absent,
    // which is `gate.ts`'s "no account known".
    const asReader = at(<Sidebar libraries={[]} admin={false} />)
    expect(asReader.querySelector('a[href="/account"]')?.textContent).toBe(
      'Account',
    )

    const unstated = at(<Sidebar libraries={[]} />)
    expect(unstated.querySelector('a[href="/account"]')?.textContent).toBe(
      'Account',
    )
  })

  it('offers an admin the account screen too', () => {
    // The other half: a guard inverted to `!admin &&` passes the test above.
    const host = at(<Sidebar libraries={[]} admin />)
    expect(host.querySelector('a[href="/account"]')?.textContent).toBe(
      'Account',
    )
  })

  it('marks the screen it is on, and a library page its own library and not the library', () => {
    const on = (path: string) =>
      mount(
        <MemoryRouter initialEntries={[path]}>
          <Sidebar
            libraries={[
              { id: 4, name: 'Manga', scanned: true },
              { id: 9, name: 'Comics', scanned: true },
            ]}
            admin={true}
          />
        </MemoryRouter>,
      )
    const current = (host: HTMLElement) =>
      [...host.querySelectorAll('[aria-current="page"]')].map((a) =>
        a.getAttribute('href'),
      )
    expect(current(on('/'))).toEqual(['/'])
    expect(current(on('/?library=9'))).toEqual(['/?library=9'])
    expect(current(on('/search'))).toEqual(['/search'])
    expect(current(on('/search?library=9'))).toEqual(['/search'])
    expect(current(on('/config/libraries'))).toEqual(['/config'])
    expect(current(on('/series/3'))).toEqual([])
  })

  it('names the link with what its count counts — O164', () => {
    // The number alone ran into the word: Chromium named the link
    // "Proposals17". The label uses the inbox heading's own words.
    const host = at(<Sidebar libraries={[]} admin={true} openProposals={17} />)
    expect(
      host.querySelector('a[href="/proposals"]')?.getAttribute('aria-label'),
    ).toBe('Proposals, 17 open')
  })

  it('names an emptied inbox the same way — O164', () => {
    const host = at(<Sidebar libraries={[]} admin={true} openProposals={0} />)
    expect(
      host.querySelector('a[href="/proposals"]')?.getAttribute('aria-label'),
    ).toBe('Proposals, 0 open')
  })

  it('leaves the link named by its text until a count is read — O164', () => {
    const host = at(<Sidebar libraries={[]} admin={true} />)
    expect(
      host.querySelector('a[href="/proposals"]')?.hasAttribute('aria-label'),
    ).toBe(false)
  })
})
