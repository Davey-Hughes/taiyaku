import { describe, expect, it, vi } from 'vitest'
import { mount, rerender } from './testing/mount'
import { TopBar, type TopBarProps } from './TopBar'

const props = (over: Partial<TopBarProps> = {}): TopBarProps => ({
  title: 'Dandadan',
  editionLine: 'Vol. 1 · Raw, monochrome',
  mode: 'single',
  onMode: vi.fn(),
  settingsOpen: false,
  onSettings: vi.fn(),
  onLeave: vi.fn(),
  ...over,
})

/** A button by its text — the word an icon button carries, visually hidden. */
const button = (host: ParentNode, text: string) =>
  [...host.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

describe('TopBar', () => {
  it('leaves for the library', () => {
    const onLeave = vi.fn()
    const host = mount(<TopBar {...props({ onLeave })} />)
    button(host, 'Library').click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('names the work and the edition on screen', () => {
    const host = mount(<TopBar {...props()} />)
    expect(host.querySelector('.reader-title-work')?.textContent).toBe(
      'Dandadan',
    )
    expect(host.querySelector('.reader-title-edition')?.textContent).toBe(
      'Vol. 1 · Raw, monochrome',
    )
  })

  it('shows the edition line alone until the title arrives', () => {
    const host = mount(
      <TopBar {...props({ title: undefined, editionLine: 'raw' })} />,
    )
    expect(host.querySelector('.reader-title-work')).toBeNull()
    expect(host.querySelector('.reader-title')?.textContent).toBe('raw')
  })

  it('marks the layout on screen pressed, and the other two not', () => {
    const host = mount(<TopBar {...props({ mode: 'two' })} />)
    expect(
      ['single', 'two', 'scroll'].map((m) =>
        button(host, m).getAttribute('aria-pressed'),
      ),
    ).toEqual(['false', 'true', 'false'])
  })

  it('asks for the layout a button names', () => {
    const onMode = vi.fn()
    const host = mount(<TopBar {...props({ onMode })} />)
    button(host, 'scroll').click()
    expect(onMode).toHaveBeenCalledWith('scroll')
  })

  it('opens settings, and says whether they are open', () => {
    const onSettings = vi.fn()
    const host = mount(<TopBar {...props({ onSettings })} />)
    expect(button(host, 'Settings').getAttribute('aria-expanded')).toBe('false')
    button(host, 'Settings').click()
    expect(onSettings).toHaveBeenCalledOnce()
    rerender(<TopBar {...props({ onSettings, settingsOpen: true })} />)
    expect(button(host, 'Settings').getAttribute('aria-expanded')).toBe('true')
  })

  it("docks at the reader's top edge, its height padded by the top safe area, over the page", () => {
    const host = mount(<TopBar {...props()} />)
    const bar = host.firstElementChild as HTMLElement
    expect(bar.classList.contains('reader-top-bar')).toBe(true)
    expect(bar.style.position).toBe('absolute')
    expect(bar.style.top).toBe('0px')
    expect(bar.style.height).toBe(
      'calc(var(--reader-top-bar) + var(--safe-top))',
    )
    expect(Number(bar.style.zIndex)).toBe(3)
  })

  it('takes the fade it is handed', () => {
    const host = mount(<TopBar {...props({ style: { opacity: 0 } })} />)
    expect((host.firstElementChild as HTMLElement).style.opacity).toBe('0')
  })

  it('keeps its five icons out of the accessibility tree', () => {
    const host = mount(<TopBar {...props()} />)
    const icons = [...host.querySelectorAll('svg')]
    expect(icons).toHaveLength(5)
    for (const icon of icons)
      expect(icon.getAttribute('aria-hidden')).toBe('true')
  })
})
