import { act, createRef, type ReactNode } from 'react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { PageHeader, RemovalSection, Row, Section } from './settingsLayout'
import { mount, rerender } from './testing/mount'

/** Mounted under a router, since a back link is a router `Link`. */
function draw(ui: ReactNode): HTMLElement {
  return mount(<MemoryRouter>{ui}</MemoryRouter>)
}

/** The one element `selector` finds under `root`, or a throw naming it. */
function one<E extends Element = HTMLElement>(
  root: ParentNode,
  selector: string,
): E {
  const found = root.querySelectorAll<E>(selector)
  if (found.length !== 1) {
    throw new Error(`${found.length} elements match ${selector}`)
  }
  return found[0]
}

const HEADINGS = 'h1, h2, h3, h4, h5, h6'

describe('PageHeader', () => {
  it('links its back link to the way up, in the words given', () => {
    const host = draw(
      <PageHeader back={{ to: '/config/users', label: 'Users' }} title="Ann" />,
    )
    const link = one<HTMLAnchorElement>(host, 'p.back > a')
    expect(link.getAttribute('href')).toBe('/config/users')
    expect(link.textContent).toBe('‹ Users')
  })

  it('draws a header with no way up outside a router', () => {
    const host = mount(<PageHeader title="Backup" />)
    expect(one(host, '.page-title').textContent).toBe('Backup')
  })

  it('titles the page at h2 when given no level', () => {
    const host = draw(<PageHeader title="Backup" />)
    const heading = one(host, HEADINGS)
    expect(heading.tagName).toBe('H2')
    expect(heading.textContent).toBe('Backup')
    expect(heading.className).toBe('page-title')
  })

  it('titles the page at the level it is given, in the same class', () => {
    const host = draw(<PageHeader title="Account" level={1} />)
    const heading = one(host, HEADINGS)
    expect(heading.tagName).toBe('H1')
    expect(heading.className).toBe('page-title')
  })

  it('draws the description under the title, in a block that may hold a list', () => {
    const host = draw(
      <PageHeader
        title="Manga"
        description={
          <dl>
            <dt>Series</dt>
            <dd>12</dd>
          </dl>
        }
      />,
    )
    const description = one(host, '.page-description')
    expect(description.tagName).toBe('DIV')
    expect(description.querySelector('dd')?.textContent).toBe('12')
    expect(
      one(host, '.page-header-text > .page-title').nextElementSibling,
    ).toBe(description)
  })

  it('draws the actions it is given in its actions slot, beside the title', () => {
    const host = draw(
      <PageHeader
        title="Manga"
        actions={<button type="button">Scan library</button>}
      />,
    )
    const actions = one(host, '.page-header-main > .page-actions')
    expect(one(actions, 'button').textContent).toBe('Scan library')
  })

  it('makes the title focusable, out of the tab order, where a titleRef is given', () => {
    const titleRef = createRef<HTMLHeadingElement>()
    const host = draw(<PageHeader title="Ann" titleRef={titleRef} />)
    const heading = one(host, HEADINGS)
    expect(titleRef.current).toBe(heading)
    expect(heading.getAttribute('tabindex')).toBe('-1')
    act(() => heading.focus())
    expect(document.activeElement).toBe(heading)
  })

  it('leaves the title unfocusable where no titleRef is given', () => {
    const host = draw(<PageHeader title="Bob" />)
    expect(one(host, HEADINGS).hasAttribute('tabindex')).toBe(false)
  })

  it('keeps the title in its place, inert, with the editor after it and then the description, while the title’s editor is given', () => {
    const host = draw(
      <PageHeader
        title="Manga"
        titleEditor={<input aria-label="Name" defaultValue="Manga" />}
        description="1858 works in 2 directories"
      />,
    )
    const heading = one(host, HEADINGS)
    expect(heading.textContent).toBe('Manga')
    expect(heading.hasAttribute('inert')).toBe(true)
    const text = one(host, '.page-header-main > .page-header-text')
    expect([...text.children]).toEqual([
      heading,
      one(host, 'input[aria-label="Name"]'),
      one(host, '.page-description'),
    ])
  })

  it('draws the editor’s answer under its line while the title is edited and there is one, and keeps no slot for it otherwise', () => {
    const header = (editing: boolean, answer?: string) => (
      <PageHeader
        title="Manga"
        titleEditor={editing ? <input aria-label="Name" /> : undefined}
        description="1858 works in 2 directories"
        actions={<button type="button">Rename…</button>}
        editorActions={<button type="button">Save</button>}
        editorAnswer={answer === undefined ? undefined : <p>{answer}</p>}
      />
    )
    const host = draw(header(false))
    const slot = () => host.querySelector('.page-header > .page-header-answer')
    expect(slot()).toBeNull()
    act(() => rerender(<MemoryRouter>{header(true)}</MemoryRouter>))
    expect(slot()).toBeNull()
    act(() => rerender(<MemoryRouter>{header(true, 'taken')}</MemoryRouter>))
    // After the title's line and the actions, so it is beneath both.
    expect(
      one(host, '.page-header > .page-header-main').nextElementSibling,
    ).toBe(slot())
    expect(slot()?.textContent).toBe('taken')
    expect(one(host, '.page-description').textContent).toBe(
      '1858 works in 2 directories',
    )
    act(() => rerender(<MemoryRouter>{header(false, 'taken')}</MemoryRouter>))
    expect(slot()).toBeNull()
  })

  it('draws its actions and the editor’s in one slot, the set not in use inert', () => {
    const header = (editing: boolean) => (
      <PageHeader
        title="Manga"
        titleEditor={editing ? <input aria-label="Name" /> : undefined}
        actions={<button type="button">Rename…</button>}
        editorActions={<button type="button">Save</button>}
      />
    )
    const host = draw(header(false))
    const sets = () =>
      [
        ...one(
          host,
          '.page-header-main > .page-actions.page-actions-swap',
        ).querySelectorAll(':scope > .page-actions-set'),
      ].map((set) => [set.textContent, set.hasAttribute('inert')])
    expect(one(host, HEADINGS).hasAttribute('inert')).toBe(false)
    expect(sets()).toEqual([
      ['Rename…', false],
      ['Save', true],
    ])
    act(() => rerender(<MemoryRouter>{header(true)}</MemoryRouter>))
    expect(sets()).toEqual([
      ['Rename…', true],
      ['Save', false],
    ])
  })
})

describe('Section', () => {
  it('heads its section at h3 when given no level', () => {
    const host = draw(
      <Section title="Sessions">
        <p>rows</p>
      </Section>,
    )
    const heading = one(host, HEADINGS)
    expect(heading.tagName).toBe('H3')
    expect(heading.textContent).toBe('Sessions')
    expect(heading.className).toBe('settings-section-title')
    // Only an untitled box is set close under its page's header.
    expect(one(host, 'section').classList).not.toContain('settings-untitled')
  })

  it('heads its section at the level it is given, in the same class', () => {
    const host = draw(
      <Section title="Theme" level={2}>
        <p>rows</p>
      </Section>,
    )
    const heading = one(host, HEADINGS)
    expect(heading.tagName).toBe('H2')
    expect(heading.className).toBe('settings-section-title')
  })

  it('draws its description beside its heading, outside the box', () => {
    const host = draw(
      <Section title="Sessions" description="Where this account is signed in.">
        <p>rows</p>
      </Section>,
    )
    const description = one(host, '.settings-section-description')
    expect(description.textContent).toBe('Where this account is signed in.')
    expect(description.closest('.settings-box')).toBeNull()
    expect(description.parentElement).toBe(one(host, '.settings-section-head'))
  })

  it('holds its children inside its box', () => {
    const host = draw(
      <Section title="Sessions">
        <p className="held">rows</p>
      </Section>,
    )
    expect(one(host, '.held').parentElement).toBe(
      one(host, 'section > .settings-box'),
    )
  })

  it('draws no heading where it is given no title, and marks the box untitled', () => {
    const host = draw(
      <Section>
        <p className="held">rows</p>
      </Section>,
    )
    expect(host.querySelectorAll(HEADINGS)).toHaveLength(0)
    expect(host.querySelector('.settings-section-head')).toBeNull()
    expect(one(host, '.settings-box > .held').textContent).toBe('rows')
    // `settings.css` sets an untitled box close under its page's header by
    // this class.
    expect(one(host, 'section').classList).toContain('settings-untitled')
  })

  it('takes no description without a title, which it would drop unseen', () => {
    // Held by `pnpm run typecheck`, not by this run: the directive fails
    // the typecheck the day the element below compiles.
    // @ts-expect-error A description needs a title to sit under.
    const untitled = <Section description="Signed in here.">rows</Section>
    expect(untitled.type).toBe(Section)
    const unset = (
      // @ts-expect-error An undefined title is no title, and drops it too.
      <Section title={undefined} description="Signed in here.">
        rows
      </Section>
    )
    expect(unset.type).toBe(Section)
  })

  it('takes no null title, which would draw an empty heading', () => {
    // Held by `pnpm run typecheck`, as the case above is.
    // @ts-expect-error A titled section's title is something to read.
    const empty = <Section title={null}>rows</Section>
    expect(empty.type).toBe(Section)
  })

  it('gives its heading the titleId as its id, focusable out of the tab order, where one is given', () => {
    const host = draw(
      <Section title="taiyaku" level={2} titleId="taiyaku">
        <p>rows</p>
      </Section>,
    )
    const heading = one(host, HEADINGS)
    expect(heading.id).toBe('taiyaku')
    expect(heading.getAttribute('tabindex')).toBe('-1')
    act(() => heading.focus())
    expect(document.activeElement).toBe(heading)
  })

  it('gives its heading no id and leaves it unfocusable where no titleId is given', () => {
    const host = draw(
      <Section title="Sessions">
        <p>rows</p>
      </Section>,
    )
    const heading = one(host, HEADINGS)
    expect(heading.hasAttribute('id')).toBe(false)
    expect(heading.hasAttribute('tabindex')).toBe(false)
  })

  it('draws its actions on its heading’s line, outside the heading and outside the box', () => {
    const host = draw(
      <Section
        title="taiyaku"
        level={2}
        titleId="taiyaku"
        actions={<a href="/about/licence">Back to top</a>}
      >
        <p>rows</p>
      </Section>,
    )
    const heading = one(host, HEADINGS)
    const actions = one(host, '.settings-section-actions')
    expect({
      heading: [heading.textContent, heading.id],
      inHead: actions.parentElement === one(host, '.settings-section-head'),
      inHeading: heading.contains(actions),
      inBox: actions.closest('.settings-box') !== null,
      holds: one(actions, 'a').textContent,
    }).toEqual({
      heading: ['taiyaku', 'taiyaku'],
      inHead: true,
      inHeading: false,
      inBox: false,
      holds: 'Back to top',
    })
  })

  it('draws no actions where it is given none', () => {
    const host = draw(
      <Section title="Sessions">
        <p>rows</p>
      </Section>,
    )
    expect(host.querySelector('.settings-section-actions')).toBeNull()
  })

  it('takes no actions without a title, which would have no heading line to sit on', () => {
    // Held by `pnpm run typecheck`, as the cases above are.
    const untitled = (
      // @ts-expect-error An untitled box has no heading line to hold actions.
      <Section actions={<button type="button">Go</button>}>rows</Section>
    )
    expect(untitled.type).toBe(Section)
  })

  it('takes no titleId without a title, which would have no heading to name', () => {
    // Held by `pnpm run typecheck`, as the cases above are.
    // @ts-expect-error A titleId names a heading, and an untitled box has none.
    const untitled = <Section titleId="taiyaku">rows</Section>
    expect(untitled.type).toBe(Section)
  })
})

describe('Row', () => {
  it('ties its label to its control by for', () => {
    const host = draw(
      <Row label="Display name">{(ids) => <input id={ids.control} />}</Row>,
    )
    const input = one<HTMLInputElement>(host, 'input')
    const label = one<HTMLLabelElement>(host, 'label.settings-row-label')
    expect(label.htmlFor).not.toBe('')
    expect(label.htmlFor).toBe(input.id)
    expect([...(input.labels ?? [])]).toEqual([label])
    // A row of one control is no group: `expectRowsWired` passes over the
    // controls inside a group, so a stray role would hide this one from it.
    expect(one(host, '.settings-row-control').hasAttribute('role')).toBe(false)
  })

  it('toggles a check row’s checkbox by a click on its label', () => {
    const host = draw(
      <Row label="Show the strip" check>
        {(ids) => <input type="checkbox" id={ids.control} />}
      </Row>,
    )
    expect(one(host, '.settings-row').classList).toContain('settings-row-check')
    const box = one<HTMLInputElement>(host, 'input[type="checkbox"]')
    expect(box.checked).toBe(false)
    act(() => one(host, 'label').click())
    expect(box.checked).toBe(true)
  })

  it('names its help in the control’s aria-describedby', () => {
    const host = draw(
      <Row label="Exclude globs" help="One pattern a line.">
        {(ids) => <textarea id={ids.control} aria-describedby={ids.help} />}
      </Row>,
    )
    const help = one(host, '.settings-row-help')
    expect(help.textContent).toBe('One pattern a line.')
    expect(help.id).not.toBe('')
    expect(one(host, 'textarea').getAttribute('aria-describedby')).toBe(help.id)
  })

  it('gives a row with no help no aria-describedby', () => {
    const host = draw(
      <Row label="Display name">
        {(ids) => <input id={ids.control} aria-describedby={ids.help} />}
      </Row>,
    )
    expect(host.querySelector('.settings-row-help')).toBeNull()
    expect(one(host, 'input').hasAttribute('aria-describedby')).toBe(false)
  })

  it('names its radio group by its label and describes it by its help, and leaves each radio its own label', () => {
    const host = draw(
      <Row
        label="Sees"
        help="An administrator sees every library."
        group="radiogroup"
      >
        <label>
          <input type="radio" name="sees" value="every" />
          <span>Every library</span>
        </label>
        <label>
          <input type="radio" name="sees" value="only" />
          <span>Only these</span>
        </label>
      </Row>,
    )
    const group = one(host, '[role="radiogroup"]')
    const label = one(host, '.settings-row-label')
    const help = one(host, '.settings-row-help')
    // A `div`: a `<label>` names one control, and here there are two.
    expect(label.tagName).toBe('DIV')
    expect(label.id).not.toBe('')
    expect(group.getAttribute('aria-labelledby')).toBe(label.id)
    expect(help.id).not.toBe('')
    expect(group.getAttribute('aria-describedby')).toBe(help.id)
    const radios = [
      ...group.querySelectorAll<HTMLInputElement>('input[type="radio"]'),
    ]
    expect(
      radios.map((r) => [...(r.labels ?? [])].map((l) => l.textContent)),
    ).toEqual([['Every library'], ['Only these']])
  })

  it('gives a group with no help no aria-describedby', () => {
    const host = draw(
      <Row label="Sees" group="radiogroup">
        <label>
          <input type="radio" name="sees" value="every" />
          <span>Every library</span>
        </label>
      </Row>,
    )
    const group = one(host, '[role="radiogroup"]')
    expect(host.querySelector('.settings-row-help')).toBeNull()
    expect(group.getAttribute('aria-labelledby')).toBe(
      one(host, '.settings-row-label').id,
    )
    expect(group.hasAttribute('aria-describedby')).toBe(false)
  })
})

describe('RemovalSection', () => {
  it('draws its box with the danger edge, and a confirm inside that box', () => {
    const host = draw(
      <RemovalSection title="Removal">
        <button type="button">Remove…</button>
        <div className="remove-user">
          <button type="button">Remove Ann</button>
        </div>
      </RemovalSection>,
    )
    // `settings.css` draws the danger edge on exactly this selector.
    const box = one(host, '.settings-removal .settings-box')
    expect(one(box, '.remove-user').parentElement).toBe(box)
  })
})
