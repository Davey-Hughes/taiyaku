import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { FieldControl } from '../src/settingsFields'
import type { Field } from '../src/settingsPanel'
import { typeInto } from './support/input'
import { mount } from './support/mount'
import { MS_SCALE } from './support/sliderScale'

const noStyle = {}

const DELAY: Field = {
  key: 'delay',
  kind: 'slider',
  scale: MS_SCALE,
  initial: 100,
}

/** `DELAY` drawn at `value`, and its range. */
function slider(value: unknown, onChange: (raw: unknown) => void = () => {}) {
  const host = mount(
    <FieldControl field={DELAY} value={value} onChange={onChange} />,
  )
  const range = host.querySelector<HTMLInputElement>('input[type="range"]')
  if (!range) throw new Error('no range drawn')
  return { host, range }
}

function doubleClick(el: Element) {
  act(() => {
    el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))
  })
}

describe('FieldControl', () => {
  it('renders a checkbox for a boolean and reports the new value', () => {
    const onChange = vi.fn()
    const host = mount(
      <FieldControl
        field={{ key: 'continueStrip', kind: 'boolean' }}
        value={true}
        onChange={onChange}
        rowStyle={noStyle}
      />,
    )
    const box = host.querySelector('input[type="checkbox"]') as HTMLInputElement
    expect(box.checked).toBe(true)
    act(() => {
      box.click()
    })
    expect(onChange).toHaveBeenCalledWith(false)
  })

  it('renders a select for a string with options, humanised', () => {
    const host = mount(
      <FieldControl
        field={{
          key: 'panel',
          kind: 'select',
          options: ['off', 'on-select', 'onSelect'],
        }}
        value={'off'}
        onChange={() => {}}
        rowStyle={noStyle}
      />,
    )
    // `humanize` splits camelCase and lowercases; it does not touch a
    // hyphen. `'on-select'` alone would prove nothing — it is a no-op for
    // `humanize` (`humanize('on-select') === 'on-select'`), so a render that
    // never called `humanize` on its options at all would pass this
    // assertion just as well. `'onSelect'` is the discriminating case:
    // `humanize('onSelect') === 'on select'`, which only a render that
    // actually humanizes its options can produce.
    expect(
      [...host.querySelectorAll('option')].map((o) => o.textContent),
    ).toEqual(['off', 'on-select', 'on select'])
  })

  it('shows a select’s values by their declared words in the panel’s case, each option keeping its value', () => {
    const field: Field = {
      key: 'mode',
      kind: 'select',
      options: ['single', 'two', 'scroll'],
      words: { single: 'One page', two: 'Two pages' },
    }
    const shown = (sentenceCase: boolean) => {
      const host = mount(
        <FieldControl
          field={field}
          value="two"
          onChange={() => {}}
          sentenceCase={sentenceCase}
        />,
      )
      return [...host.querySelectorAll('option')].map((o) => [
        o.value,
        o.textContent,
        o.selected,
      ])
    }
    // The reader's panel writes its words in lowercase, View's as sentences.
    expect(shown(false)).toEqual([
      ['single', 'one page', false],
      ['two', 'two pages', true],
      ['scroll', 'scroll', false],
    ])
    expect(shown(true)).toEqual([
      ['single', 'One page', false],
      ['two', 'Two pages', true],
      ['scroll', 'Scroll', false],
    ])
  })

  it('summarises an object rather than inventing a control for it', () => {
    const host = mount(
      <FieldControl
        field={{ key: 'breaks', kind: 'object' }}
        value={{ a: 1, b: 2 }}
        onChange={() => {}}
        rowStyle={noStyle}
      />,
    )
    expect(host.textContent).toContain('2')
    expect(host.querySelector('input')).toBeNull()
  })

  it('takes its row styling from its caller', () => {
    // Deliberately not `'6px 0'` — that is `Settings.tsx`'s own module-scope
    // `rowStyle` value. A `settingsFields.tsx` holding a same-valued constant
    // of its own and never wiring the prop through would still satisfy an
    // assertion phrased against `'6px 0'`, so this uses a value that appears
    // nowhere else, which only the prop path can produce.
    const host = mount(
      <FieldControl
        field={{ key: 'x', kind: 'boolean' }}
        value={false}
        onChange={() => {}}
        rowStyle={{ padding: '3px 9px' }}
      />,
    )
    expect((host.firstElementChild as HTMLElement).style.padding).toBe(
      '3px 9px',
    )
    // A slider's row is its name's, above the track: on the outer `div` the
    // style would set name, value and track side by side.
    const drawn = mount(
      <FieldControl
        field={DELAY}
        value={100}
        onChange={() => {}}
        rowStyle={{ padding: '3px 9px' }}
      />,
    )
    const head = drawn.querySelector<HTMLElement>('.field-slider-head')
    expect(head?.style.padding).toBe('3px 9px')
    expect(drawn.querySelector('.field-slider')?.hasAttribute('style')).toBe(
      false,
    )
  })

  it('draws a slider as a range over positions, its value and unit on the name’s row', () => {
    const { host, range } = slider(120)
    expect([
      range.min,
      range.max,
      range.value,
      range.getAttribute('aria-valuetext'),
    ]).toEqual(['0', '10', '7', '120 milliseconds'])
    const outer = host.firstElementChild as HTMLElement
    expect(outer.className).toBe('field-slider')
    expect([...outer.children].map((child) => child.className)).toEqual([
      'field-slider-head',
      'field-slider-track',
    ])
    const [head, track] = outer.children
    const label = head.querySelector('label')
    expect(label?.textContent).toBe('delay')
    expect(range.id).not.toBe('')
    expect(label?.htmlFor).toBe(range.id)
    expect(head.querySelector('output')?.textContent).toBe('120 ms')
    expect(range.parentElement).toBe(track)
  })

  it('reports a position as its value, and its default on a double-click', async () => {
    const onChange = vi.fn()
    const { range } = slider(120, onChange)
    await typeInto(range, '2')
    expect(onChange).toHaveBeenLastCalledWith(70)
    doubleClick(range)
    expect(onChange).toHaveBeenLastCalledWith(100)
    expect(onChange).toHaveBeenCalledTimes(2)
  })

  it('draws a value off the scale at the nearest position, and one that is no number at its default', () => {
    const drawn = (value: unknown) => {
      const { host, range } = slider(value)
      return [host.querySelector('output')?.textContent, range.value]
    }
    expect(drawn(123)).toEqual(['120 ms', '7'])
    expect(drawn('x')).toEqual(['100 ms', '5'])
    expect(drawn(Number.POSITIVE_INFINITY)).toEqual(['100 ms', '5'])
  })
})
