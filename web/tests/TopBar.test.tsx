import { MemoryRouter } from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import { mount, rerender } from './support/mount'
import {
  BOOK_LAYOUT_WORDS,
  BOOK_LAYOUTS,
  PAGE_LAYOUT_WORDS,
  PAGE_LAYOUTS,
  PAGE_LAYOUTS_COMPARED,
} from '../src/layouts'
import { TopBar, type TopBarProps } from '../src/TopBar'

const props = (over: Partial<TopBarProps> = {}): TopBarProps => ({
  title: 'Dandadan',
  editionLine: 'Vol. 1 · Raw, monochrome',
  layouts: PAGE_LAYOUTS,
  layoutWords: PAGE_LAYOUT_WORDS,
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

/** The layout group's buttons, each as its name's text, hover text and pressed state. */
const layoutButtons = (host: ParentNode) =>
  [...host.querySelectorAll('[aria-label="Layout"] button')].map((b) => [
    b.textContent,
    b.getAttribute('title'),
    b.getAttribute('aria-pressed'),
  ])

describe('TopBar', () => {
  it('leaves for the library', () => {
    const onLeave = vi.fn()
    const host = mount(<TopBar {...props({ onLeave })} />)
    button(host, 'Library').click()
    expect(onLeave).toHaveBeenCalledOnce()
  })

  it('names the way out by the word it is handed', () => {
    const onLeave = vi.fn()
    const host = mount(<TopBar {...props({ onLeave, leaveWord: 'Search' })} />)
    expect(host.querySelector('.reader-back')?.textContent).toBe('Search')
    button(host, 'Search').click()
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

  it('leads the edition line with the series, linked to its page', () => {
    const host = mount(
      <MemoryRouter>
        <TopBar {...props({ series: { id: 41, title: 'Dandadan' } })} />
      </MemoryRouter>,
    )
    const line = host.querySelector('.reader-title-line') as HTMLElement
    expect(
      [...line.children].map((el) => [
        el.className,
        el.textContent,
        el.getAttribute('href'),
      ]),
    ).toEqual([
      ['reader-title-series', 'Dandadan', '/series/41'],
      ['reader-title-edition', ' · Vol. 1 · Raw, monochrome', null],
    ])
  })

  it('links to no series where it is handed none', () => {
    const host = mount(<TopBar {...props()} />)
    const line = host.querySelector('.reader-title-line') as HTMLElement
    expect(
      [...line.children].map((el) => [el.className, el.textContent]),
    ).toEqual([['reader-title-edition', 'Vol. 1 · Raw, monochrome']])
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
      ['One page', 'Two pages', 'Scroll through the pages'].map((m) =>
        button(host, m).getAttribute('aria-pressed'),
      ),
    ).toEqual(['false', 'true', 'false'])
  })

  it('asks for the layout a button names', () => {
    const onMode = vi.fn()
    const host = mount(<TopBar {...props({ onMode })} />)
    button(host, 'Scroll through the pages').click()
    expect(onMode).toHaveBeenCalledWith('scroll')
  })

  it("names each of the page reader's layouts in words, its hover text the same words", () => {
    const host = mount(
      <TopBar
        editionLine="Raw"
        layouts={PAGE_LAYOUTS_COMPARED}
        layoutWords={PAGE_LAYOUT_WORDS}
        mode="compare"
        onMode={vi.fn()}
        settingsOpen={false}
        onSettings={vi.fn()}
        onLeave={vi.fn()}
      />,
    )
    expect(layoutButtons(host)).toEqual([
      ['One page', 'One page', 'false'],
      ['Two pages', 'Two pages', 'false'],
      ['Scroll through the pages', 'Scroll through the pages', 'false'],
      ['Compare editions', 'Compare editions', 'true'],
    ])
  })

  it("names each of the book reader's layouts and its paged switch in words, the switch's hover text saying whether it is on", () => {
    const bar = (paged: boolean) => (
      <TopBar
        editionLine="English"
        layouts={BOOK_LAYOUTS}
        layoutWords={BOOK_LAYOUT_WORDS}
        mode="scroll"
        onMode={vi.fn()}
        paged={paged}
        onPaged={vi.fn()}
        settingsOpen={false}
        onSettings={vi.fn()}
        onLeave={vi.fn()}
      />
    )
    const host = mount(bar(false))
    expect(layoutButtons(host)).toEqual([
      ['One edition', 'One edition', 'true'],
      ['Compare editions', 'Compare editions', 'false'],
      [
        'Each sentence over its translation',
        'Each sentence over its translation',
        'false',
      ],
      ['Page turning', 'Page turning: off', 'false'],
    ])
    // The name stays put as the switch turns on; only its pressed state and
    // its hover text say so.
    rerender(bar(true))
    expect(layoutButtons(host).at(-1)).toEqual([
      'Page turning',
      'Page turning: on',
      'true',
    ])
  })

  it('names the paged switch in words where it is alone in the group', () => {
    const host = mount(
      <TopBar
        {...props({ layouts: undefined, paged: false, onPaged: vi.fn() })}
      />,
    )
    expect(layoutButtons(host)).toEqual([
      ['Page turning', 'Page turning: off', 'false'],
    ])
  })

  it('shows ⚙ its name and its key as hover text', () => {
    const host = mount(<TopBar {...props()} />)
    expect(button(host, 'Settings').getAttribute('title')).toBe('Settings (s)')
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

  it('draws the layouts a reader offers, in its order, compare among them', () => {
    const onMode = vi.fn()
    const host = mount(
      <TopBar
        title="T"
        editionLine="English"
        layouts={['scroll', 'compare'] as const}
        layoutWords={BOOK_LAYOUT_WORDS}
        mode="scroll"
        onMode={onMode}
        settingsOpen={false}
        onSettings={vi.fn()}
        onLeave={vi.fn()}
      />,
    )
    const group = host.querySelector('[aria-label="Layout"]')
    expect(
      [...(group?.querySelectorAll('button') ?? [])].map((b) => b.textContent),
    ).toEqual(['One edition', 'Compare editions'])
    button(host, 'Compare editions').click()
    expect(onMode).toHaveBeenCalledWith('compare')
  })

  it('draws no layout group for a reader that has no layout', () => {
    const host = mount(<TopBar {...props({ layouts: undefined })} />)
    expect(host.querySelector('[aria-label="Layout"]')).toBeNull()
    expect(button(host, 'Library')).toBeDefined()
    expect(button(host, 'Settings')).toBeDefined()
  })
})
