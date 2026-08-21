import { describe, expect, it } from 'vitest'
import { EmptyState } from './EmptyState'
import { mount } from './testing/mount'

describe('the empty states', () => {
  it('tells a new deployment how to add a root and what a scan will do', () => {
    const host = mount(<EmptyState kind="no-roots" />)
    expect(host.textContent).toContain('library scan --add')
  })

  it('says a configured library has not been read yet, which is not the same thing', () => {
    const host = mount(<EmptyState kind="not-scanned" />)
    expect(host.textContent).toContain('library scan')
    expect(host.textContent).not.toContain('--add')
  })

  it('names what was searched, since FTS5 on CJK can match nothing surprisingly', () => {
    const host = mount(<EmptyState kind="no-matches" query="五等分の花嫁" />)
    expect(host.textContent).toContain('五等分の花嫁')
  })

  it('renders something for every kind — none of them is a null return', () => {
    for (const kind of ['no-roots', 'not-scanned', 'no-matches'] as const) {
      expect(mount(<EmptyState kind={kind} />).textContent).not.toBe('')
    }
  })
})
