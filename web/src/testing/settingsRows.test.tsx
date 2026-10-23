import { describe, expect, it } from 'vitest'
import { Row, Section } from '../settingsLayout'
import { mount } from './mount'
import { expectRowsWired, helpOf, row, section } from './settingsRows'

describe('expectRowsWired', () => {
  it('passes on rows whose controls their labels label and their help describes', () => {
    const host = mount(
      <Section title="Scanning">
        <Row label="Exclude globs" help="One pattern a line.">
          {(ids) => (
            <textarea
              id={ids.control}
              name="exclude"
              aria-describedby={ids.help}
            />
          )}
        </Row>
        <Row label="Watch" help="Rescans on a change." check>
          {(ids) => (
            <input
              type="checkbox"
              id={ids.control}
              name="watch"
              aria-describedby={`${ids.help} watch-note`}
            />
          )}
        </Row>
        <Row label="Display name">
          {(ids) => <input id={ids.control} name="display" />}
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).not.toThrow()
  })

  it('passes over a row that holds no form control', () => {
    const host = mount(
      <Section title="Sessions">
        <Row label="Firefox · 10.0.0.2" help="Signed in 2026-10-01.">
          <button type="button">Revoke</button>
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).not.toThrow()
  })

  it('throws, naming the row, on a control its row’s label does not label', () => {
    const host = mount(
      <Section title="Profile">
        <Row label="Display name">
          <input name="display" />
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'the row "Display name": its input "display" is not labelled by the row\'s label',
    )
  })

  it('throws, naming the row, on a control its row’s help does not describe', () => {
    const host = mount(
      <Section title="Scanning">
        <Row label="Exclude globs" help="One pattern a line.">
          {(ids) => <textarea id={ids.control} name="exclude" />}
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'the row "Exclude globs": its textarea "exclude" is not described by the row\'s help',
    )
  })

  it('throws, naming the row, on a control a label of its own labels in other words', () => {
    const host = mount(
      <Section title="Profile">
        <Row label="Display name">
          <label>
            Your name <input name="display" />
          </label>
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'the row "Display name": its input "display" is not labelled by the row\'s label',
    )
  })

  it('throws, naming the row, on a control a label of its own labels besides its row’s label', () => {
    const host = mount(
      <Section title="Profile">
        <Row label="Display name">
          {(ids) => (
            <label>
              Your name <input id={ids.control} name="display" />
            </label>
          )}
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'the row "Display name": its input "display" has a label besides the row\'s',
    )
  })

  it('throws, naming the row, on a control whose aria-labelledby names it in place of its row’s label', () => {
    const host = mount(
      <Section title="Profile">
        <p id="other-words">Your name</p>
        <Row label="Display name">
          {(ids) => (
            <input
              id={ids.control}
              name="display"
              aria-labelledby="other-words"
            />
          )}
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'the row "Display name": its input "display" is named by its aria-labelledby, not the row\'s label',
    )
  })

  it('throws, naming the row, on a control described by something other than its row’s help', () => {
    const host = mount(
      <Section title="Scanning">
        <p id="other-note">Read when the directory is next scanned.</p>
        <Row label="Exclude globs" help="One pattern a line.">
          {(ids) => (
            <textarea
              id={ids.control}
              name="exclude"
              aria-describedby="other-note"
            />
          )}
        </Row>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'the row "Exclude globs": its textarea "exclude" is not described by the row\'s help',
    )
  })

  describe('a row whose radios are a group', () => {
    /**
     * Drawn by `Row`, which wires the group itself; a case that needs a
     * fault takes one attribute off it, or points it elsewhere, afterwards.
     * `#other-words` is a sentence in the same section that is not the row's.
     */
    const sees = (help?: string) => {
      const host = mount(
        <Section title="Libraries">
          <p id="other-words">Libraries this account may see</p>
          <Row label="Sees" help={help} group="radiogroup">
            <label>
              <input type="radio" name="sees" value="every" />
              <span>Every library</span>
            </label>
            <label>
              <input type="radio" name="sees" value="only" />
              <span>Only these</span>
            </label>
          </Row>
        </Section>,
      )
      const group = host.querySelector('[role="radiogroup"]')
      if (!group) throw new Error('no radiogroup')
      return { host, group }
    }

    it('passes on a group its row’s label names and its help describes, whose radios keep labels of their own', () => {
      const helped = sees('An administrator sees every library.')
      expect(() => expectRowsWired(helped.host)).not.toThrow()
      const bare = sees()
      expect(() => expectRowsWired(bare.host)).not.toThrow()
    })

    it('throws, naming the row, on radios in a row with no group to name them', () => {
      const host = mount(
        <Section title="Libraries">
          <Row label="Sees">
            <label>
              <input type="radio" name="sees" value="every" />
              <span>Every library</span>
            </label>
          </Row>
        </Section>,
      )
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its input "sees" is not labelled by the row\'s label',
      )
    })

    it('throws, naming the row, on a group its row’s label does not name', () => {
      const { host, group } = sees('An administrator sees every library.')
      group.removeAttribute('aria-labelledby')
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its radiogroup is not labelled by the row\'s label',
      )
    })

    it('throws, naming the row, on a group named by something other than its row’s label', () => {
      const { host, group } = sees('An administrator sees every library.')
      group.setAttribute('aria-labelledby', 'other-words')
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its radiogroup is not labelled by the row\'s label',
      )
    })

    it('throws, naming the row, on a group named by other words as well as its row’s label', () => {
      const { host, group } = sees('An administrator sees every library.')
      group.setAttribute(
        'aria-labelledby',
        `other-words ${group.getAttribute('aria-labelledby')}`,
      )
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its radiogroup is named by more than the row\'s label',
      )
    })

    it('throws, naming the row, on a group its row’s help does not describe', () => {
      const { host, group } = sees('An administrator sees every library.')
      group.removeAttribute('aria-describedby')
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its radiogroup is not described by the row\'s help',
      )
    })

    it('throws, naming the row, on a text box in a group’s box, since only its radios keep labels of their own', () => {
      const host = mount(
        <Section title="Libraries">
          <Row label="Sees" group="radiogroup">
            <label>
              <input type="radio" name="sees" value="every" />
              <span>Every library</span>
            </label>
            <input name="note" />
          </Row>
        </Section>,
      )
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its input "note" is not labelled by the row\'s label',
      )
    })

    it('throws, naming the row, on a group described by something other than its row’s help', () => {
      const { host, group } = sees('An administrator sees every library.')
      group.setAttribute('aria-describedby', 'other-words')
      expect(() => expectRowsWired(host)).toThrow(
        'the row "Sees": its radiogroup is not described by the row\'s help',
      )
    })
  })

  it('throws on a root with no rows, so a caller cannot check nothing', () => {
    const host = mount(
      <Section title="Directories">
        <p>No directories yet.</p>
      </Section>,
    )
    expect(() => expectRowsWired(host)).toThrow(
      'no .settings-row under the root',
    )
  })
})

describe('row, section and helpOf', () => {
  it('finds the one row whose label is the words given, not one that holds them, and its help', () => {
    const host = mount(
      <Section title="Role & permissions">
        <Row label="Can download" help="Save whole editions as files." check>
          {(ids) => (
            <input type="checkbox" id={ids.control} name="can_download" />
          )}
        </Row>
        <Row label="Can download all">
          {(ids) => <input id={ids.control} name="all" />}
        </Row>
      </Section>,
    )
    expect(row(host, 'Can download').querySelector('input')?.name).toBe(
      'can_download',
    )
    expect(helpOf(row(host, 'Can download'))?.textContent).toBe(
      'Save whole editions as files.',
    )
    expect(helpOf(row(host, 'Can download all'))).toBeNull()
  })

  it('throws on words no row’s label reads, and on words two rows’ labels share', () => {
    const host = mount(
      <Section title="Sessions & keys">
        <Row label="Firefox · 10.0.0.2">
          <button type="button">Revoke</button>
        </Row>
        <Row label="Firefox · 10.0.0.2">
          <button type="button">Revoke</button>
        </Row>
      </Section>,
    )
    expect(() => row(host, 'Safari')).toThrow('0 rows labelled Safari')
    expect(() => row(host, 'Firefox · 10.0.0.2')).toThrow(
      '2 rows labelled Firefox · 10.0.0.2',
    )
  })

  it('finds the one section whose heading is the words given, and throws on words no heading reads, and on words two headings share', () => {
    const host = mount(
      <>
        <Section title="Sessions & keys">
          <p className="held">keys</p>
        </Section>
        <Section title="Sessions">
          <p className="held">sessions</p>
        </Section>
      </>,
    )
    expect(section(host, 'Sessions').querySelector('.held')?.textContent).toBe(
      'sessions',
    )
    expect(() => section(host, 'Keys')).toThrow('0 sections titled Keys')
    const twice = mount(
      <>
        <Section title="Sessions">
          <p>this browser</p>
        </Section>
        <Section title="Sessions">
          <p>another</p>
        </Section>
      </>,
    )
    expect(() => section(twice, 'Sessions')).toThrow(
      '2 sections titled Sessions',
    )
  })
})
