import { act, useState } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Root } from '../src/config'
import { RuleForm } from '../src/RuleForm'
import type { Rule } from '../src/rules'
import { fields } from './support/fields'
import { mount } from './support/mount'

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga \\(RAW\\)/.*',
  match_on: 'path',
  set_series: null,
  set_edition_key: 'raw',
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const roots: Root[] = [
  {
    id: 1,
    path: '/srv/manga',
    edition_key: 'en',
    default_kind: 'manga',
    default_language: 'en',
    exclude_globs: null,
    scan_interval_secs: 86400,
    keeps_documents: false,
    last_scan_at: null,
    last_scan_status: null,
  },
]

function render(props: Partial<Parameters<typeof RuleForm>[0]> = {}) {
  const onCommit = vi.fn()
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <RuleForm rule={rule()} roots={roots} onCommit={onCommit} {...props} />,
    )
  })
  if (!host) throw new Error('nothing mounted')
  return { host, onCommit }
}

function typeInto(input: HTMLInputElement, value: string) {
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

/** Choose an option the way a person operating the select does. */
function selectValue(select: HTMLSelectElement, value: string) {
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set
    setter?.call(select, value)
    select.dispatchEvent(new Event('change', { bubbles: true }))
  })
}

/**
 * The real mount site in miniature: `selected` is state this harness owns,
 * and the `key` below is a composite key — index *and* id — not a bare id.
 *
 * **A bare id cannot do this job.** Every unsaved rule carries `id: 0`, the
 * store's sentinel, so two unsaved rules are indistinguishable by id alone;
 * a `key` (or an internal compare) built from `rule.id` would see `0 !== 0`
 * fail on every switch between them and never remount. Only the index in the
 * key forces React to replace the instance.
 *
 * Declared inside a function rather than at module scope, for the reason
 * `Gate.test.tsx`'s `probe` gives: `style/useComponentExportOnlyModules`
 * refuses an unexported component at module scope, and
 * `suspicious/noExportsInTest` refuses the export that would satisfy it.
 */
function harness(props: {
  rules: Rule[]
  roots: Root[]
  onCommit: (rule: Rule) => void
}) {
  function Harness() {
    const [selected, setSelected] = useState(0)
    return (
      <div>
        <button type="button" onClick={() => setSelected(1)}>
          pick second
        </button>
        <RuleForm
          key={`${selected}:${props.rules[selected].id}`}
          rule={props.rules[selected]}
          roots={props.roots}
          onCommit={props.onCommit}
        />
      </div>
    )
  }
  return <Harness />
}

function renderHarness(rules: Rule[]) {
  const onCommit = vi.fn()
  let host: HTMLElement | undefined
  act(() => {
    host = mount(harness({ rules, roots, onCommit }))
  })
  if (!host) throw new Error('nothing mounted')
  return { host, onCommit }
}

describe('RuleForm', () => {
  it('draws the rule’s current values', () => {
    const { host } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    )
    expect(pattern?.value).toBe('Manga \\(RAW\\)/.*')
    expect(
      host.querySelector<HTMLSelectElement>('select[name="kind"]')?.value,
    ).toBe('regex')
    expect(
      host.querySelector<HTMLInputElement>('input[name="set_edition_key"]')
        ?.value,
    ).toBe('raw')
  })

  it('offers every kind and match_on the store parses, and no others', () => {
    const { host } = render()
    const kinds = [
      ...host.querySelectorAll<HTMLOptionElement>('select[name="kind"] option'),
    ].map((o) => o.value)
    expect(kinds).toEqual([
      'prefix',
      'regex',
      'builtin-volume',
      'builtin-chapter',
    ])
    const matches = [
      ...host.querySelectorAll<HTMLOptionElement>(
        'select[name="match_on"] option',
      ),
    ].map((o) => o.value)
    expect(matches).toEqual(['path', 'name', 'parent'])
  })

  it('scopes to every root, or to one named root', () => {
    const { host } = render()
    const scope = host.querySelector<HTMLSelectElement>(
      'select[name="root_id"]',
    )
    const options = [...(scope?.options ?? [])].map((o) => o.value)
    expect(options).toEqual(['', '1'])
    expect(scope?.value).toBe('')
  })

  it('fires no commit while typing', () => {
    // The screen's central cost decision: a preview is the slow call, and a
    // half-typed regex is a rule that matches nothing. The build design's §5.3.
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'M')
    typeInto(pattern, 'Ma')
    typeInto(pattern, 'Man')
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('commits the whole edited rule on blur', () => {
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'Colour/.*')
    act(() => {
      pattern.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    })
    expect(onCommit).toHaveBeenCalledTimes(1)
    // The whole rule, not a spot check of three fields — `enabled`, `ordinal`,
    // `kind`, `match_on`, `root_id` and `note` all have to survive the trip
    // unchanged, the way `rules.test.ts`'s `carries every field across the
    // trip` pins the same shape for the export/import converter.
    expect(onCommit.mock.calls[0][0]).toEqual({
      ...rule(),
      pattern: 'Colour/.*',
    })
  })

  it('commits on Enter', () => {
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'Colour/.*')
    act(() => {
      pattern.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
      )
    })
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit.mock.calls[0][0].pattern).toBe('Colour/.*')
  })

  it('does not commit a blur that changed nothing', () => {
    // A click through the form is not five derivations.
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    act(() => {
      pattern.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    })
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('reads an emptied text field as null, not as an empty string', () => {
    // The columns are nullable and the store reads absent-or-null as "no
    // value". An empty string is a value, and it would set the field to "".
    const { host, onCommit } = render()
    const edition = host.querySelector<HTMLInputElement>(
      'input[name="set_edition_key"]',
    ) as HTMLInputElement
    typeInto(edition, '')
    act(() => {
      edition.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    })
    expect(onCommit.mock.calls[0][0].set_edition_key).toBe(null)
  })

  /**
   * The redraw is the composite `key`'s doing, not any compare this component
   * runs internally, and proving that requires driving the real mechanism
   * rather than an `onCommit` mock's `rerender`.
   *
   * **Both rules below carry `id: 0`.** That is the case a bare id compare
   * could never handle — every unsaved rule shares the sentinel, so switching
   * between two of them would leave `rule.id !== seenId` false forever under
   * an internal fallback, and the draft would never reset. It is also
   * precisely the shape the rule editor's own design produces: a person can
   * have two brand-new, unsaved rules open one after another before either is
   * ever written. The composite key — index and id together — is what
   * actually resets this form, and this test fails without it.
   */
  it('redraws to the second rule via the composite key, which a bare id cannot do for two unsaved rules', () => {
    const rules = [
      rule({ id: 0, pattern: 'First/.*' }),
      rule({ id: 0, pattern: 'Second/.*' }),
    ]
    const { host } = renderHarness(rules)
    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('First/.*')

    act(() => {
      ;(host.querySelector('button') as HTMLButtonElement).click()
    })

    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('Second/.*')
  })

  it('commits a kind change immediately, with the rest of the rule intact', () => {
    const { host, onCommit } = render()
    const kind = host.querySelector<HTMLSelectElement>(
      'select[name="kind"]',
    ) as HTMLSelectElement
    selectValue(kind, 'prefix')
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit.mock.calls[0][0]).toEqual({ ...rule(), kind: 'prefix' })
  })

  it('commits a match_on change immediately, like its two siblings', () => {
    // **The third select, tested because the other two are.** `kind` and
    // `root_id` each carry a conversion worth pinning — a vocabulary and a
    // string-to-number — and `match_on` carries neither, which is exactly why
    // it is the one that would be left out and the one whose handler could
    // quietly diverge from theirs. Three near-identical handlers with two
    // tests is how the untested one stops matching.
    const { host, onCommit } = render()
    const matchOn = host.querySelector<HTMLSelectElement>(
      'select[name="match_on"]',
    ) as HTMLSelectElement
    selectValue(matchOn, 'parent')
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit.mock.calls[0][0]).toEqual({ ...rule(), match_on: 'parent' })
  })

  it('commits root_id as a number, and as null for "every root"', () => {
    const { host, onCommit } = render()
    const scope = host.querySelector<HTMLSelectElement>(
      'select[name="root_id"]',
    ) as HTMLSelectElement

    selectValue(scope, '1')
    expect(onCommit).toHaveBeenCalledTimes(1)
    expect(onCommit.mock.calls[0][0].root_id).toBe(1)
    expect(typeof onCommit.mock.calls[0][0].root_id).toBe('number')

    selectValue(scope, '')
    expect(onCommit).toHaveBeenCalledTimes(2)
    expect(onCommit.mock.calls[1][0].root_id).toBe(null)
  })

  /**
   * **The second commit, the select's, resends the pattern, and that field
   * is not stale.** `commit()` never resets `draft` after a blur, so the
   * select's `onChange` — a closure from the same render the typing already
   * produced — closes over a `draft` that already holds the typed pattern.
   * It resends that field, but it is the current value, identical to what the
   * blur just committed. Two `onCommit` calls for two gestures is the owner's
   * ruling for the select handlers; this pins that the resent field is at
   * least correct, so the redundancy is a duplicate preview, not a data bug.
   */
  it('resends the just-committed pattern when a select changes right after a blur, but not a stale one', () => {
    const { host, onCommit } = render()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    typeInto(pattern, 'Colour/.*')
    act(() => {
      pattern.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    })
    const kind = host.querySelector<HTMLSelectElement>(
      'select[name="kind"]',
    ) as HTMLSelectElement
    selectValue(kind, 'prefix')

    expect(onCommit).toHaveBeenCalledTimes(2)
    expect(onCommit.mock.calls[0][0]).toEqual({
      ...rule(),
      pattern: 'Colour/.*',
    })
    expect(onCommit.mock.calls[1][0]).toEqual({
      ...rule(),
      pattern: 'Colour/.*',
      kind: 'prefix',
    })
  })

  it('gives each control its label’s words as its aria-label — O175', () => {
    const { host } = render()
    // Three selects and six text boxes. Counted, because a loop over nothing
    // passes.
    const labelled = fields(host.querySelectorAll('.rule-form label.field'))
    expect(labelled).toHaveLength(9)
    for (const { written, control } of labelled) {
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })

  it('gives the sets group its legend’s words as its aria-label — O182', () => {
    // The legend is drawn in capitals, and Chromium's tree named the group in
    // them.
    const { host } = render()
    const group = host.querySelector('fieldset.rule-sets')
    const legend = group?.querySelector(':scope > legend')
    expect(legend?.textContent).toBe('sets')
    expect(group?.getAttribute('aria-label')).toBe(legend?.textContent)
  })
})
