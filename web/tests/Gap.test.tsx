import { describe, expect, it } from 'vitest'
import { Gap } from '../src/Gap'
import { mount } from './support/mount'

const from = { edition: 'JP', page: 12 }

describe('Gap', () => {
  it('names the missing edition and the page it is missing from', () => {
    const host = mount(
      <Gap reading="ltr" edition="US" from={from} onTake={() => {}} />,
    )
    const text = host.textContent ?? ''
    expect(text).toContain('No US page for JP page 12.')
  })

  it('says nothing is nearby either when both offers are absent', () => {
    const host = mount(
      <Gap reading="ltr" edition="US" from={from} onTake={() => {}} />,
    )
    expect(host.textContent).toContain('No paired page nearby in US either.')
    expect(host.querySelectorAll('button')).toHaveLength(0)
  })

  it('omits the verdict sentence when the pairing has none', () => {
    const host = mount(
      <Gap reading="ltr" edition="US" from={from} onTake={() => {}} />,
    )
    expect(host.textContent).not.toContain('sound overall')
    expect(host.textContent).not.toContain('fragmented')
    expect(host.textContent).not.toContain('refused')
  })

  it.each([
    [
      'sound' as const,
      'This pairing is sound overall — this is one isolated gap, not a sign of trouble.',
    ],
    [
      'fragmented' as const,
      'This pairing is fragmented: matches are sparse across the whole volume, so gaps like this one are expected.',
    ],
    [
      'refused' as const,
      'This pairing was refused: these editions were not found to correspond at all.',
    ],
  ])('renders the %s verdict as its own sentence', (verdict, sentence) => {
    const host = mount(
      <Gap
        reading="ltr"
        edition="US"
        from={from}
        verdict={verdict}
        onTake={() => {}}
      />,
    )
    expect(host.textContent).toContain(sentence)
  })

  it('offers only the side that has a move, in an ltr work: before on the left', () => {
    const host = mount(
      <Gap
        reading="ltr"
        edition="US"
        from={from}
        before={{ slot: 3, page: 4 }}
        onTake={() => {}}
      />,
    )
    const buttons = host.querySelectorAll('button')
    expect(buttons).toHaveLength(1)
    expect(buttons[0]?.textContent).toBe('← US page 4')
  })

  it('offers only the side that has a move, in an ltr work: after on the right', () => {
    const host = mount(
      <Gap
        reading="ltr"
        edition="US"
        from={from}
        after={{ slot: 7, page: 9 }}
        onTake={() => {}}
      />,
    )
    const buttons = host.querySelectorAll('button')
    expect(buttons).toHaveLength(1)
    expect(buttons[0]?.textContent).toBe('US page 9 →')
  })

  it('points the arrows the other way, and reverses the row, in an rtl work', () => {
    const host = mount(
      <Gap
        reading="rtl"
        edition="US"
        from={from}
        before={{ slot: 3, page: 4 }}
        after={{ slot: 7, page: 9 }}
        onTake={() => {}}
      />,
    )
    const row = host.querySelector('button')?.parentElement as HTMLElement
    expect(row.style.flexDirection).toBe('row-reverse')
    const buttons = host.querySelectorAll('button')
    expect(buttons[0]?.textContent).toBe('→ US page 4')
    expect(buttons[1]?.textContent).toBe('US page 9 ←')
  })

  it('lays the row out forward, not reversed, in an ltr work', () => {
    const host = mount(
      <Gap
        reading="ltr"
        edition="US"
        from={from}
        before={{ slot: 3, page: 4 }}
        after={{ slot: 7, page: 9 }}
        onTake={() => {}}
      />,
    )
    const row = host.querySelector('button')?.parentElement as HTMLElement
    expect(row.style.flexDirection).toBe('row')
  })

  it('hands the offer’s authority-space slot to onTake, not its display page', () => {
    const taken: number[] = []
    const host = mount(
      <Gap
        reading="ltr"
        edition="US"
        from={from}
        before={{ slot: 3, page: 4 }}
        after={{ slot: 7, page: 9 }}
        onTake={(slot) => taken.push(slot)}
      />,
    )
    const buttons = host.querySelectorAll('button')
    ;(buttons[0] as HTMLButtonElement).click()
    ;(buttons[1] as HTMLButtonElement).click()
    expect(taken).toEqual([3, 7])
  })
})
