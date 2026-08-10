import { act } from 'react'
import { describe, expect, it } from 'vitest'
import { NotePanel, type NotePanelProps } from './NotePanel'
import type { Note, NotesDoc, ReviewEntry } from './notes'
import { COLOUR_NAMES, DEFAULT_COLOUR } from './palette'
import { mount, rerender } from './testing/mount'

/**
 * `NotePanel` in `jsdom`: what it renders for a draft and for the review
 * list, and what it does with the events a reader triggers — clicks, and
 * typing in the body textarea. `notes.ts` (99.13%) already covers the data
 * this component is handed; nothing here re-derives `notesHere`,
 * `reviewList` or `nextColour` — only what `NotePanel` itself does with the
 * result.
 */

function note(overrides: Partial<Note> = {}): Note {
  return {
    id: 'n1',
    edition: 'en-mono',
    slot: 3,
    body: 'hello',
    created: '2026-08-01T00:00:00Z',
    updated: '2026-08-01T00:00:00Z',
    ...overrides,
  }
}

function entry(overrides: Partial<ReviewEntry> = {}): ReviewEntry {
  return { note: note(), here: 3, ...overrides }
}

function baseProps(overrides: Partial<NotePanelProps> = {}): NotePanelProps {
  return {
    mode: 'edit',
    draft: note(),
    entries: [],
    doc: { work: 'gotoubun-v01', notes: [] },
    work: 'gotoubun-v01',
    viewing: 'en-mono',
    busy: false,
    notice: undefined,
    onBody: () => {},
    onColour: () => {},
    onDelete: () => {},
    onClose: () => {},
    canUndo: false,
    canRedo: false,
    onUndo: () => {},
    onRedo: () => {},
    onGo: () => {},
    ...overrides,
  }
}

/** Sets a textarea's value through the native setter, the way a real
 * keystroke does and a plain `el.value = …` assignment does not — React
 * tracks the previous value on the wrapped property, so only the native
 * setter followed by a dispatched `input` event reaches `onChange`. */
function typeInto(el: HTMLTextAreaElement, value: string): void {
  const setter = Object.getOwnPropertyDescriptor(
    window.HTMLTextAreaElement.prototype,
    'value',
  )?.set
  setter?.call(el, value)
  el.dispatchEvent(new Event('input', { bubbles: true }))
}

/** Every `<button>` that is one of the review list's rows, rather than a
 * header control — the header buttons all carry a `title` or are one of the
 * two fixed labels a row's composite text can never equal. */
function entryRows(host: HTMLElement): HTMLButtonElement[] {
  return [...host.querySelectorAll('button')].filter(
    (b) =>
      !b.hasAttribute('title') &&
      b.textContent !== 'Export' &&
      b.textContent !== 'Close',
  ) as HTMLButtonElement[]
}

describe('NotePanel, closed', () => {
  it('renders nothing when mode is undefined', () => {
    const host = mount(
      <NotePanel {...baseProps({ mode: undefined, draft: undefined })} />,
    )
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(host.textContent).toBe('')
  })
})

describe('NotePanel, editing', () => {
  it('renders the dialog shell but no editor when there is no draft to show', () => {
    const host = mount(<NotePanel {...baseProps({ draft: undefined })} />)
    const dialog = host.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-label')).toBe('Note')
    expect(host.querySelector('textarea')).toBeNull()
  })

  it('labels a region note by its edition and slot', () => {
    const host = mount(
      <NotePanel
        {...baseProps({
          draft: note({
            edition: 'raw',
            slot: 7,
            region: { x: 0, y: 0, w: 1, h: 1 },
          }),
        })}
      />,
    )
    expect(host.querySelector('p')?.textContent).toBe('raw slot 7 · region')
  })

  it('labels a whole-page note distinctly from a region', () => {
    const host = mount(
      <NotePanel {...baseProps({ draft: note({ region: undefined }) })} />,
    )
    expect(host.querySelector('p')?.textContent).toBe(
      'en-mono slot 3 · whole page',
    )
  })

  it('shows the body, focuses it, and puts the caret at the end', () => {
    const host = mount(
      <NotePanel {...baseProps({ draft: note({ body: 'hello there' }) })} />,
    )
    const ta = host.querySelector('textarea') as HTMLTextAreaElement
    expect(ta.value).toBe('hello there')
    expect(ta.getAttribute('aria-label')).toBe('Note body')
    expect(document.activeElement).toBe(ta)
    expect(ta.selectionStart).toBe('hello there'.length)
    expect(ta.selectionEnd).toBe('hello there'.length)
  })

  it('refocuses the body when a different note opens', () => {
    const host = mount(
      <NotePanel {...baseProps({ draft: note({ id: 'a', body: 'first' }) })} />,
    )
    const away = document.createElement('button')
    document.body.append(away)
    away.focus()
    expect(document.activeElement).toBe(away)

    rerender(
      <NotePanel
        {...baseProps({ draft: note({ id: 'b', body: 'second' }) })}
      />,
    )
    const ta = host.querySelector('textarea') as HTMLTextAreaElement
    expect(document.activeElement).toBe(ta)
    expect(ta.selectionStart).toBe('second'.length)
    away.remove()
  })

  it('reports the body as the reader types', () => {
    const seen: string[] = []
    const host = mount(
      <NotePanel {...baseProps({ onBody: (b) => seen.push(b) })} />,
    )
    const ta = host.querySelector('textarea') as HTMLTextAreaElement
    act(() => typeInto(ta, 'a new line'))
    expect(seen).toEqual(['a new line'])
  })

  it('renders one swatch per known colour, ringing the current one', () => {
    const host = mount(
      <NotePanel {...baseProps({ draft: note({ color: 'rose' }) })} />,
    )
    const swatches = [
      ...host.querySelectorAll('[role="group"] button'),
    ] as HTMLButtonElement[]
    expect(swatches).toHaveLength(COLOUR_NAMES.length)
    const rose = swatches.find((b) => b.getAttribute('aria-label') === 'rose')
    const mint = swatches.find((b) => b.getAttribute('aria-label') === 'mint')
    expect(rose?.getAttribute('aria-pressed')).toBe('true')
    expect(mint?.getAttribute('aria-pressed')).toBe('false')
  })

  it('falls back to the default colour ring when a note names none', () => {
    const host = mount(
      <NotePanel {...baseProps({ draft: note({ color: undefined }) })} />,
    )
    const swatches = [
      ...host.querySelectorAll('[role="group"] button'),
    ] as HTMLButtonElement[]
    const amber = swatches.find(
      (b) => b.getAttribute('aria-label') === DEFAULT_COLOUR,
    )
    expect(amber?.getAttribute('aria-pressed')).toBe('true')
  })

  it('reports a click on a colour swatch', () => {
    const seen: string[] = []
    const host = mount(
      <NotePanel
        {...baseProps({
          draft: note({ color: 'rose' }),
          onColour: (c) => seen.push(c),
        })}
      />,
    )
    const swatches = [
      ...host.querySelectorAll('[role="group"] button'),
    ] as HTMLButtonElement[]
    const mint = swatches.find((b) => b.getAttribute('aria-label') === 'mint')
    act(() => mint?.click())
    expect(seen).toEqual(['mint'])
  })

  it('deletes the open draft by id', () => {
    const seen: string[] = []
    const host = mount(
      <NotePanel
        {...baseProps({
          draft: note({ id: 'n42' }),
          onDelete: (id) => seen.push(id),
        })}
      />,
    )
    const del = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Delete',
    ) as HTMLButtonElement
    act(() => del.click())
    expect(seen).toEqual(['n42'])
  })

  it('disables every exit while a write is in flight', () => {
    const host = mount(<NotePanel {...baseProps({ busy: true })} />)
    const swatch = host.querySelector(
      '[role="group"] button',
    ) as HTMLButtonElement
    const del = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Delete',
    ) as HTMLButtonElement
    expect(swatch.disabled).toBe(true)
    expect(del.disabled).toBe(true)
    expect(host.textContent).toContain('saving…')
  })

  it('reads as saved, not busy, once a write has settled', () => {
    const host = mount(<NotePanel {...baseProps({ busy: false })} />)
    const swatch = host.querySelector(
      '[role="group"] button',
    ) as HTMLButtonElement
    expect(swatch.disabled).toBe(false)
    expect(host.textContent).toContain('saved · esc or click away when done')
  })

  it('shows a notice banner when there is one', () => {
    const host = mount(
      <NotePanel {...baseProps({ notice: 'could not save' })} />,
    )
    expect(host.textContent).toContain('could not save')
  })

  it('shows no notice banner when there is none', () => {
    const host = mount(<NotePanel {...baseProps({ notice: undefined })} />)
    // Only the slot label's `<p>` renders for a draft; a notice would add a
    // second.
    expect(host.querySelectorAll('p')).toHaveLength(1)
  })
})

describe('NotePanel, reviewing', () => {
  it('pluralises the note count for zero', () => {
    const host = mount(
      <NotePanel
        {...baseProps({ mode: 'review', draft: undefined, entries: [] })}
      />,
    )
    expect(host.querySelector('strong')?.textContent).toBe('0 notes')
  })

  it('keeps the note count singular for exactly one', () => {
    const host = mount(
      <NotePanel
        {...baseProps({ mode: 'review', draft: undefined, entries: [entry()] })}
      />,
    )
    expect(host.querySelector('strong')?.textContent).toBe('1 note')
  })

  it('pluralises the note count above one', () => {
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          entries: [entry(), entry({ note: note({ id: 'n2' }) })],
        })}
      />,
    )
    expect(host.querySelector('strong')?.textContent).toBe('2 notes')
  })

  it('shows the empty-state hint when there is nothing written yet', () => {
    const host = mount(
      <NotePanel
        {...baseProps({ mode: 'review', draft: undefined, entries: [] })}
      />,
    )
    expect(host.textContent).toContain('Nothing written yet')
  })

  it('hides the empty-state hint once a note exists', () => {
    const host = mount(
      <NotePanel
        {...baseProps({ mode: 'review', draft: undefined, entries: [entry()] })}
      />,
    )
    expect(host.textContent).not.toContain('Nothing written yet')
  })

  it('shows a notice banner in review mode too', () => {
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          notice: 'saved 3 notes',
        })}
      />,
    )
    expect(host.textContent).toContain('saved 3 notes')
  })

  it('enables undo and redo exactly when history allows and nothing is busy', () => {
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          canUndo: true,
          canRedo: true,
        })}
      />,
    )
    expect(
      (host.querySelector('[title="Undo (z)"]') as HTMLButtonElement).disabled,
    ).toBe(false)
    expect(
      (host.querySelector('[title="Redo (Z)"]') as HTMLButtonElement).disabled,
    ).toBe(false)
  })

  it('disables undo and redo when there is nothing to step to', () => {
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          canUndo: false,
          canRedo: false,
        })}
      />,
    )
    expect(
      (host.querySelector('[title="Undo (z)"]') as HTMLButtonElement).disabled,
    ).toBe(true)
    expect(
      (host.querySelector('[title="Redo (Z)"]') as HTMLButtonElement).disabled,
    ).toBe(true)
  })

  it('disables undo and redo while a write is in flight even if the history allows it', () => {
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          canUndo: true,
          canRedo: true,
          busy: true,
        })}
      />,
    )
    expect(
      (host.querySelector('[title="Undo (z)"]') as HTMLButtonElement).disabled,
    ).toBe(true)
    expect(
      (host.querySelector('[title="Redo (Z)"]') as HTMLButtonElement).disabled,
    ).toBe(true)
  })

  it('calls onUndo and onRedo when their buttons are clicked', () => {
    const calls: string[] = []
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          canUndo: true,
          canRedo: true,
          onUndo: () => calls.push('undo'),
          onRedo: () => calls.push('redo'),
        })}
      />,
    )
    act(() =>
      (host.querySelector('[title="Undo (z)"]') as HTMLButtonElement).click(),
    )
    act(() =>
      (host.querySelector('[title="Redo (Z)"]') as HTMLButtonElement).click(),
    )
    expect(calls).toEqual(['undo', 'redo'])
  })

  it('closes on click', () => {
    const seen: string[] = []
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          onClose: () => seen.push('closed'),
        })}
      />,
    )
    const btn = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Close',
    ) as HTMLButtonElement
    act(() => btn.click())
    expect(seen).toEqual(['closed'])
  })

  it('disables export when there is nothing to export', () => {
    const host = mount(
      <NotePanel
        {...baseProps({ mode: 'review', draft: undefined, entries: [] })}
      />,
    )
    const btn = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Export',
    ) as HTMLButtonElement
    expect(btn.disabled).toBe(true)
  })

  it('exports exactly the document the server holds, named for the work', () => {
    const doc: NotesDoc = { work: 'gotoubun-v01', notes: [note()] }
    let capturedHref = ''
    let capturedDownload = ''
    const originalClick = HTMLAnchorElement.prototype.click
    HTMLAnchorElement.prototype.click = function click(
      this: HTMLAnchorElement,
    ) {
      capturedHref = this.href
      capturedDownload = this.download
    }
    const revoked: string[] = []
    const originalRevoke = URL.revokeObjectURL
    URL.revokeObjectURL = (u: string) => {
      revoked.push(u)
    }

    try {
      const host = mount(
        <NotePanel
          {...baseProps({
            mode: 'review',
            draft: undefined,
            entries: [entry()],
            doc,
          })}
        />,
      )
      const btn = [...host.querySelectorAll('button')].find(
        (b) => b.textContent === 'Export',
      ) as HTMLButtonElement
      expect(btn.disabled).toBe(false)
      act(() => btn.click())
    } finally {
      HTMLAnchorElement.prototype.click = originalClick
      URL.revokeObjectURL = originalRevoke
    }

    expect(capturedDownload).toBe('gotoubun-v01.notes.json')
    expect(capturedHref).toMatch(/^blob:/)
    expect(revoked).toEqual([capturedHref])
  })

  it('marks a note unreachable from the edition being read, and disables its row', () => {
    const e = entry({
      here: undefined,
      note: note({ edition: 'raw', slot: 5 }),
    })
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          entries: [e],
          viewing: 'en-mono',
        })}
      />,
    )
    const [row] = entryRows(host)
    expect(row?.disabled).toBe(true)
    expect(row?.textContent).toContain('raw slot 5')
    expect(row?.textContent).toContain('not reachable from en-mono')
  })

  it('names where a travelled note lands here, when it is reachable', () => {
    const e = entry({
      here: 7,
      note: note({ edition: 'raw', slot: 5 }),
    })
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          entries: [e],
          viewing: 'en-mono',
        })}
      />,
    )
    const [row] = entryRows(host)
    expect(row?.disabled).toBe(false)
    expect(row?.textContent).toContain('raw slot 5')
    expect(row?.textContent).toContain('en-mono slot 7')
  })

  it('adds no extra location text for a note already on the edition being read', () => {
    const e = entry({ here: 3, note: note({ edition: 'en-mono', slot: 3 }) })
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          entries: [e],
          viewing: 'en-mono',
        })}
      />,
    )
    const [row] = entryRows(host)
    expect(row?.disabled).toBe(false)
    expect(row?.textContent).not.toContain('not reachable')
    expect((row?.textContent?.match(/slot/g) ?? []).length).toBe(1)
  })

  it('shows a note body, or a marker for an empty one', () => {
    const withBody = entry({ note: note({ body: 'remember this' }) })
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          entries: [withBody],
        })}
      />,
    )
    const [row] = entryRows(host)
    expect(row?.querySelector('em')).toBeNull()
    expect(row?.textContent).toContain('remember this')
  })

  it('marks an empty note body rather than leaving it blank', () => {
    const empty = entry({ note: note({ body: '' }) })
    const host = mount(
      <NotePanel
        {...baseProps({ mode: 'review', draft: undefined, entries: [empty] })}
      />,
    )
    const [row] = entryRows(host)
    expect(row?.querySelector('em')?.textContent).toBe('empty')
  })

  it('takes the reader to a note when its row is clicked', () => {
    const seen: ReviewEntry[] = []
    const e = entry()
    const host = mount(
      <NotePanel
        {...baseProps({
          mode: 'review',
          draft: undefined,
          entries: [e],
          onGo: (x) => seen.push(x),
        })}
      />,
    )
    act(() => entryRows(host)[0]?.click())
    expect(seen).toEqual([e])
  })
})
