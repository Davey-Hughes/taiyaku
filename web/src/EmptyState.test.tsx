import { describe, expect, it } from 'vitest'
import { EmptyState } from './EmptyState'
import { mount } from './testing/mount'

describe('the empty states', () => {
  it('tells a new deployment where a library is made, and what a scan will do', () => {
    const host = mount(<EmptyState kind="no-libraries" />)
    expect(host.querySelector('h2')?.textContent).toBe('No library yet')
    expect(host.textContent).toContain('Configuration → Libraries')
    expect(host.textContent).toContain(
      'taiyaku library scan --add /path/to/manga --library Manga',
    )
    expect(host.textContent).toContain('without writing anything back')
  })

  it('says a library with folders has not been read yet, which is not the same thing', () => {
    const host = mount(<EmptyState kind="not-scanned" />)
    expect(host.querySelector('h2')?.textContent).toBe('Not read yet')
    expect(host.textContent).toContain(
      'This library has folders, and nothing has scanned them.',
    )
    expect(host.textContent).toContain(
      'Scan library, on Configuration → Libraries, reads them',
    )
    expect(host.querySelector('pre')?.textContent).toBe('taiyaku library scan')
    expect(host.textContent).not.toContain('--add')
  })

  it('says a library it cannot show is no library here, and offers no scan', () => {
    const host = mount(<EmptyState kind="no-library" />)
    expect(host.querySelector('h2')?.textContent).toBe('No such library')
    expect(host.textContent).toContain('this account may not see it')
    expect(host.textContent).not.toContain('library scan')
  })

  it('tells an account shown no library that nothing is shared with it, and who can share one', () => {
    const host = mount(<EmptyState kind="no-grant" />)
    expect(host.querySelector('h2')?.textContent).toBe('No library shared')
    expect(host.querySelector('p')?.textContent).toBe(
      'No library has been shared with this account. An administrator can grant one in Configuration → Users.',
    )
    expect(host.textContent).not.toContain('library scan')
  })

  it('names what was searched, since FTS5 on CJK can match nothing surprisingly', () => {
    const host = mount(<EmptyState kind="no-matches" query="五等分の花嫁" />)
    expect(host.textContent).toContain('五等分の花嫁')
  })

  it('names the kind whose tab has just been answered out, and the control that undoes it', () => {
    const host = mount(
      <EmptyState kind="kind-answered" label="Attach Chapters" />,
    )
    expect(host.textContent).toContain(
      'No Attach Chapters proposal is waiting on a decision.',
    )
    expect(host.textContent).toContain('All shows the ones that are.')
  })

  it('still says something useful for a kind-answered state with no label to name', () => {
    // `Proposals.tsx` always has a label to pass, so this is the branch no
    // screen reaches — and a template dropped straight into the sentence
    // would render "No undefined proposal is waiting", which is the failure
    // `no-matches` already carries the same guard against.
    const host = mount(<EmptyState kind="kind-answered" />)
    expect(host.textContent).toContain(
      'No proposal of this kind is waiting on a decision.',
    )
    expect(host.textContent).not.toContain('undefined')
  })

  it('separates a cleared inbox from one whose weak proposals are hidden', () => {
    // The same split `no-libraries` and `not-scanned` are drawn on: the person
    // can act on exactly one of the two, and the acts are different. A
    // shared message would send somebody to a scan when the proposal they
    // want is already raised and merely below the threshold.
    const clear = mount(<EmptyState kind="inbox-clear" />)
    const hidden = mount(<EmptyState kind="inbox-weak-hidden" />)
    expect(clear.textContent).toContain('taiyaku library scan')
    expect(clear.textContent).not.toContain('Show weak proposals')
    expect(hidden.textContent).toContain('Show weak proposals')
    expect(hidden.textContent).not.toContain('taiyaku library scan')
  })

  it('says which decided state is empty, never that there is nothing left to answer (O72)', () => {
    // A person on the Dismissed filter is looking for a dismissal. "Nothing
    // to answer" is a sentence about the open inbox, and here it would read
    // as though the thing they came for had never existed — so each of the
    // three names the state it is about, and none borrows the inbox's words.
    // Two states, so a kind that printed one fixed word passes neither half.
    for (const decided of ['accepted', 'dismissed'] as const) {
      const none = mount(<EmptyState kind="decided-none" decided={decided} />)
      const hidden = mount(
        <EmptyState kind="decided-weak-hidden" decided={decided} />,
      )
      const kind = mount(
        <EmptyState
          kind="decided-kind-none"
          label="Attach Chapters"
          decided={decided}
        />,
      )
      expect(none.querySelector('h2')?.textContent).toBe(`Nothing ${decided}`)
      expect(none.textContent).toContain(
        `No proposal in this library has been ${decided}.`,
      )
      expect(hidden.textContent).toContain(
        `Nothing ${decided} is above the score threshold.`,
      )
      expect(hidden.textContent).toContain('Show weak proposals')
      expect(kind.textContent).toContain(
        `No Attach Chapters proposal has been ${decided}.`,
      )
      expect(kind.textContent).toContain('All shows every one that has.')
      for (const host of [none, hidden, kind]) {
        expect(host.textContent).not.toContain('Nothing to answer')
        expect(host.textContent).not.toContain('waiting on a decision')
        expect(host.textContent).not.toContain('taiyaku library scan')
      }
    }
    const unlabelled = mount(
      <EmptyState kind="decided-kind-none" decided="dismissed" />,
    )
    expect(unlabelled.textContent).toContain(
      'No proposal of this kind has been dismissed.',
    )
    expect(unlabelled.textContent).not.toContain('undefined')
  })

  it('renders something for every kind — none of them is a null return', () => {
    for (const kind of [
      'no-libraries',
      'no-grant',
      'no-library',
      'not-scanned',
      'no-matches',
      'inbox-clear',
      'inbox-weak-hidden',
      'kind-answered',
      'decided-none',
      'decided-weak-hidden',
      'decided-kind-none',
    ] as const) {
      expect(mount(<EmptyState kind={kind} />).textContent).not.toBe('')
    }
  })
})
