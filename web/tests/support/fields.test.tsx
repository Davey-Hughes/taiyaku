import { describe, expect, it } from 'vitest'
import { Row, Section } from '../../src/settingsLayout'
import { fields } from './fields'
import { mount } from './mount'

describe('fields', () => {
  it('reads a label tied to its control by `for` as well as one that wraps it', () => {
    const host = mount(
      <Section title="Profile">
        <Row label="Display name" help="Shown beside your notes.">
          {(ids) => <input id={ids.control} name="display" />}
        </Row>
        <label className="field">
          <span>Edition key</span>
          <input name="edition" />
          <small>Two letters or a word.</small>
        </label>
      </Section>,
    )
    const read = fields(host.querySelectorAll('label'))
    expect(
      read.map(({ written, words, control }) => [
        written,
        words.tagName,
        control.getAttribute('name'),
      ]),
    ).toEqual([
      // The `for` label's own words, without the help beside it.
      ['Display name', 'LABEL', 'display'],
      ['Edition key', 'SPAN', 'edition'],
    ])
  })

  it.each([
    ['names nothing', () => <label htmlFor="nowhere">Display name</label>],
    [
      'names an element that is not a control',
      () => (
        <>
          <label htmlFor="nowhere">Display name</label>
          <div id="nowhere">Display name</div>
        </>
      ),
    ],
  ])('throws on a label whose `for` %s', (_, ui) => {
    const host = mount(ui())
    expect(() => fields(host.querySelectorAll('label'))).toThrow(
      'a label is for "nowhere", which is no control: Display name',
    )
  })
})
