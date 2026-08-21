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
})
