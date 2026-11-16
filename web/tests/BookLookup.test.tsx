import { describe, expect, it, vi } from 'vitest'
import { LookupPanel } from '../src/BookLookup'
import { mount } from './support/mount'

/** The panel for `hablaron`, as a lookup stands at `looked`. */
const shown = (looked: Parameters<typeof LookupPanel>[0]['looked']) =>
  mount(
    <LookupPanel
      word="hablaron"
      looked={looked}
      onSave={vi.fn()}
      onClose={vi.fn()}
    />,
  )

describe('the look-up panel', () => {
  it('says it is looking, and what went wrong where it failed', () => {
    expect(shown({ kind: 'looking' }).textContent).toContain('Looking it up…')
    expect(shown({ kind: 'failed', why: 'Error: 500' }).textContent).toContain(
      'Error: 500',
    )
  })

  it('says when the dictionary holds no such word', () => {
    expect(shown({ kind: 'found', entries: [] }).textContent).toContain(
      'Not in the dictionary.',
    )
  })

  it('names the form a headword was found by, and no part of speech it lacks', () => {
    const host = shown({
      kind: 'found',
      entries: [
        { word: 'hablar', pos: 'verb', glosses: ['to speak'], via: 'hablaron' },
        { word: 'Hablaron', pos: '', glosses: ['a name'], via: null },
      ],
    })
    const heads = [...host.querySelectorAll('.book-lookup-entry > p')].map(
      (p) => p.textContent,
    )
    expect(heads).toEqual(['hablar verb · from hablaron', 'Hablaron '])
    expect(host.querySelectorAll('.book-lookup-entry em')).toHaveLength(1)
  })
})
