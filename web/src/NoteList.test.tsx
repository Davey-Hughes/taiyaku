import { describe, expect, it } from 'vitest'
import type { Note, NoteHere } from './notes'
import { NoteList } from './NoteList'
import { noteColour } from './palette'
import { mount } from './testing/mount'

let counter = 0

const makeNote = (overrides: Partial<Note> = {}): Note => {
  counter += 1
  return {
    id: `note-${counter}`,
    edition: 'JP',
    slot: 0,
    body: 'a note',
    created: '2026-01-01T00:00:00Z',
    updated: '2026-01-01T00:00:00Z',
    ...overrides,
  }
}

const here = (overrides: Partial<Note> = {}, travelled = false): NoteHere => ({
  note: makeNote(overrides),
  travelled,
})

describe('NoteList', () => {
  it('renders nothing when there are no notes here', () => {
    const host = mount(
      <NoteList here={[]} placement="side" onOpen={() => {}} />,
    )
    expect(host.childElementCount).toBe(0)
  })

  it('numbers rows by position in `here`, not by any note field', () => {
    const rows = [here({ id: 'z' }), here({ id: 'a' }), here({ id: 'm' })]
    const host = mount(
      <NoteList here={rows} placement="side" onOpen={() => {}} />,
    )
    const numbers = [...host.querySelectorAll('button')].map(
      (b) => b.firstElementChild?.textContent,
    )
    expect(numbers).toEqual(['1', '2', '3'])
  })

  it('lists each note as a listitem inside a labelled list', () => {
    const host = mount(
      <NoteList here={[here()]} placement="side" onOpen={() => {}} />,
    )
    const list = host.firstElementChild as HTMLElement
    expect(list.getAttribute('role')).toBe('list')
    expect(list.getAttribute('aria-label')).toBe('Notes on this page')
    expect(host.querySelector('[role="listitem"]')).not.toBeNull()
  })

  it('positions itself in the margin for "side" placement', () => {
    const host = mount(
      <NoteList here={[here()]} placement="side" onOpen={() => {}} />,
    )
    const list = host.firstElementChild as HTMLElement
    expect(list.style.position).toBe('absolute')
    expect(list.style.width).toBe('240px')
  })

  it('gives up absolute positioning for "below" placement, sitting in flow instead', () => {
    const host = mount(
      <NoteList here={[here()]} placement="below" onOpen={() => {}} />,
    )
    const list = host.firstElementChild as HTMLElement
    expect(list.style.position).toBe('')
    expect(list.style.flex).toBe('0 0 auto')
  })

  it('marks a travelled note with the edition it came from', () => {
    const host = mount(
      <NoteList
        here={[here({ edition: 'FR' }, true)]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    expect(host.textContent).toContain('on FR ·')
  })

  it('says nothing about travel for a note that is native to this slot', () => {
    const host = mount(
      <NoteList
        here={[here({ edition: 'FR' }, false)]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    expect(host.textContent).not.toContain('on FR ·')
  })

  it('marks a point anchor, which has no area of its own to show', () => {
    const host = mount(
      <NoteList
        here={[here({ region: { x: 0.2, y: 0.3, w: 0, h: 0 } })]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    expect(host.textContent).toContain('point ·')
  })

  it('says nothing about a point for an area region or no region at all', () => {
    const host = mount(
      <NoteList
        here={[
          here({ region: { x: 0.2, y: 0.3, w: 0.1, h: 0.1 }, id: 'area' }),
          here({ id: 'whole-slot' }),
        ]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    expect(host.textContent).not.toContain('point ·')
  })

  it('shows an empty placeholder for a note with no body, not blank space', () => {
    const host = mount(
      <NoteList
        here={[here({ body: '' })]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    expect(host.textContent).toContain('empty')
  })

  it('shows the note’s own words when it has any, not the placeholder', () => {
    const host = mount(
      <NoteList
        here={[here({ body: 'watch the eyes here' })]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    expect(host.textContent).toContain('watch the eyes here')
    expect(host.textContent).not.toContain('empty')
  })

  it('colours the marker number by the note’s own colour name', () => {
    const host = mount(
      <NoteList
        here={[here({ color: 'rose' })]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    const badge = host.querySelector('button > span') as HTMLElement
    expect(badge.style.background).toBe(noteColour('rose').tab)
  })

  it('falls back to the default colour for a note that names none', () => {
    const host = mount(
      <NoteList
        here={[here({ color: undefined })]}
        placement="side"
        onOpen={() => {}}
      />,
    )
    const badge = host.querySelector('button > span') as HTMLElement
    expect(badge.style.background).toBe(noteColour(undefined).tab)
  })

  it('opens the note that was clicked, by id, not by position', () => {
    const opened: string[] = []
    const rows = [here({ id: 'first' }), here({ id: 'second' })]
    const host = mount(
      <NoteList
        here={rows}
        placement="side"
        onOpen={(id) => opened.push(id)}
      />,
    )
    const buttons = host.querySelectorAll('button')
    ;(buttons[1] as HTMLButtonElement).click()
    ;(buttons[0] as HTMLButtonElement).click()
    expect(opened).toEqual(['second', 'first'])
  })
})
