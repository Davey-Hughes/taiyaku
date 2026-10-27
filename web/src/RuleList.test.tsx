import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import type { Proposal } from './curation'
import { RuleList } from './RuleList'
import type { Rule } from './rules'
import { holdOf } from './testing/held'
import { mount, rerender } from './testing/mount'
import { holdTheClock, pass, WAIT_DRAWN_AFTER_MS } from './testing/wait'

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga/.*',
  match_on: 'path',
  set_series: 'Series',
  set_edition_key: null,
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const proposal = (over: Partial<Proposal> = {}): Proposal => ({
  id: 10,
  kind: 'same-series',
  subject_id: 1,
  target_id: 2,
  score: 0.9,
  reason: 'two spellings of one title',
  state: 'accepted',
  created_rule_id: 2,
  ...over,
})

// A single fake shared across a dragstart/drop pair, so the drop genuinely
// reads what the dragstart wrote. Two independent stubs — one for each
// event — would let `onDragStart` write anything, or nothing, and the test
// would still pass on whatever the drop stub hardcoded.
function dataTransfer() {
  const store = new Map<string, string>()
  return {
    setData: (key: string, value: string) => void store.set(key, value),
    getData: (key: string) => store.get(key) ?? '',
    effectAllowed: '',
  }
}

function render(props: Partial<Parameters<typeof RuleList>[0]> = {}) {
  const handlers = {
    onSelect: vi.fn(),
    onToggle: vi.fn(),
    onReorder: vi.fn(),
    onAdd: vi.fn(),
    onDelete: vi.fn(),
  }
  let host: HTMLElement | undefined
  act(() => {
    host = mount(
      <RuleList
        rules={[rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })]}
        selected={0}
        proposals={[]}
        overrideCount={0}
        frozen={false}
        {...handlers}
        {...props}
      />,
    )
  })
  if (!host) throw new Error('nothing mounted')
  return { host, ...handlers }
}

describe('RuleList', () => {
  it('draws one row per rule, in list order, numbered from one', () => {
    const { host } = render()
    const rows = host.querySelectorAll('.rule-row')
    expect(rows).toHaveLength(2)
    expect(rows[0].textContent).toContain('1')
    expect(rows[1].textContent).toContain('2')
  })

  it('marks the selected rule on its button, which is what takes focus — O156', () => {
    // The mark belongs on the element that takes focus: the row's select
    // button, not the `<li>` round it.
    const { host } = render({ selected: 1 })
    const marks = [...host.querySelectorAll('.rule-row')].map((row) => [
      row.getAttribute('aria-current'),
      row.querySelector('button.select')?.getAttribute('aria-current') ?? null,
    ])
    expect(marks).toEqual([
      [null, null],
      [null, 'true'],
    ])
  })

  it('selects by index, never by rule id', () => {
    // Two unsaved rules both carry `id: 0`; a list keyed on id could not tell
    // them apart, and `Decided`/`Override` name a position for the same reason.
    const { host, onSelect } = render({
      rules: [rule({ id: 0 }), rule({ id: 0, pattern: 'Second/.*' })],
    })
    act(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
    })
    expect(onSelect).toHaveBeenCalledWith(1)
  })

  it('toggles enable from the row', () => {
    const { host, onToggle } = render()
    act(() => {
      host
        .querySelectorAll<HTMLInputElement>(
          '.rule-row input[type="checkbox"]',
        )[1]
        .click()
    })
    expect(onToggle).toHaveBeenCalledWith(1)
  })

  it('wraps the enable box in a wordless label of its own — O157', () => {
    // The label is what `admin.css`'s coarse block grows to 44×44. It adds a
    // target, not a name: the box is still named by its own `aria-label`.
    const { host, onToggle } = render()
    const boxes = [
      ...host.querySelectorAll<HTMLInputElement>(
        '.rule-row input[type="checkbox"]',
      ),
    ]
    expect(boxes.length).toBeGreaterThan(1)
    for (const [i, box] of boxes.entries()) {
      const label = box.parentElement
      expect(label?.tagName, `row ${i + 1}`).toBe('LABEL')
      expect(label?.className, `row ${i + 1}`).toBe('enable')
      expect(label?.textContent, `row ${i + 1}`).toBe('')
      expect(box.getAttribute('aria-label')).toBe(`rule ${i + 1} enabled`)
    }
    // And a press on the label, outside the box, is a press on the box.
    act(() => {
      host
        .querySelectorAll<HTMLLabelElement>('.rule-row label.enable')[1]
        .click()
    })
    expect(onToggle).toHaveBeenCalledWith(1)
  })

  it('labels a freshly added rule "new rule", not a blank button', () => {
    // blankRule() sets all four set_* fields to null but pattern to '' —
    // the one non-nullable field of that set. A `??` chain alone stops at
    // '' rather than falling through, so this is the normal state of every
    // newly added rule, not an edge case.
    const { host } = render({
      rules: [
        rule({
          id: 0,
          set_series: null,
          set_edition_key: null,
          set_kind: null,
          set_language: null,
          pattern: '',
        }),
      ],
    })
    // `.rule-name`, not the whole button: `.select` now also holds a
    // `.mechanism` span, so its own textContent is the two concatenated.
    const name = host.querySelector<HTMLElement>('.rule-row .rule-name')
    expect(name?.textContent).toBe('new rule')
  })

  it('names a builtin by its note rather than calling it a new rule', () => {
    const { host } = render({
      rules: [
        rule({
          kind: 'builtin-volume',
          match_on: 'name',
          pattern: '',
          set_series: null,
          note: 'volume from the name',
        }),
      ],
    })
    const row = host.querySelector('.rule-row')
    // `.rule-name`, not `.select`: the button now holds two spans, and
    // `.select`'s own textContent is their concatenation.
    expect(row?.querySelector('.rule-name')?.textContent).toBe(
      'volume from the name',
    )
    expect(row?.querySelector('.mechanism')?.textContent).toBe(
      'builtin on name → volume',
    )
  })

  it('says which rules are off', () => {
    const { host } = render({
      rules: [rule(), rule({ id: 2, ordinal: 2, enabled: false })],
    })
    expect(host.querySelectorAll('.rule-row')[1].textContent).toContain('off')
  })

  it('reorders on drop, from the dragged index to the dropped one', () => {
    const { host, onReorder } = render()
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    const dt = dataTransfer()
    act(() => {
      rows[1].dispatchEvent(
        Object.assign(new Event('dragstart', { bubbles: true }), {
          dataTransfer: dt,
        }),
      )
      rows[0].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: dt,
          preventDefault: () => {},
        }),
      )
    })
    expect(onReorder).toHaveBeenCalledWith(1, 0)
  })

  it('ignores a drop that carries no dragged row', () => {
    // **`Number('')` is `0`, not `NaN`** — so a `Number.isNaN` guard alone
    // reads a drop that began outside this list (a file from the desktop, a
    // foreign drag source) as "row 0 was dragged" and silently reorders the
    // list under someone who never dragged a rule. The row this drops on is
    // deliberately **not** row 0: landing on row 0 makes the separate
    // `from === index` check swallow the bug and the test pass for the wrong
    // reason, which is exactly how it stayed hidden.
    const { host, onReorder } = render()
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    act(() => {
      rows[1].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: dataTransfer(),
          preventDefault: () => {},
        }),
      )
    })
    expect(onReorder).not.toHaveBeenCalled()
  })

  it('badges a rule an accepted proposal wrote', () => {
    // C4 §1 requires those marked. The mark comes from the proposals list's
    // own `created_rule_id`, which needs no server change.
    const { host } = render({ proposals: [proposal({ created_rule_id: 2 })] })
    const rows = host.querySelectorAll('.rule-row')
    expect(rows[0].querySelector('.from-proposal')).toBeNull()
    expect(rows[1].querySelector('.from-proposal')).not.toBeNull()
  })

  it('warns before deleting a rule a proposal wrote, naming the proposal', () => {
    const { host, onDelete } = render({
      proposals: [proposal({ id: 42, created_rule_id: 2 })],
    })
    act(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[1].click()
    })
    expect(onDelete).not.toHaveBeenCalled()
    expect(host.textContent).toContain('42')
    act(() => {
      host.querySelector<HTMLElement>('button.confirm-delete')?.click()
    })
    expect(onDelete).toHaveBeenCalledWith(1)
  })

  it('keeps the rule when the confirm dialog is dismissed', () => {
    // The whole point of asking first is that backing out is possible. A
    // "Keep it" that quietly deleted anyway — or that left the dialog open
    // behind it — would make the confirmation theatre rather than a choice.
    const { host, onDelete } = render({
      proposals: [proposal({ id: 42, created_rule_id: 2 })],
    })
    act(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[1].click()
    })
    expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()
    act(() => {
      ;[...host.querySelectorAll<HTMLElement>('.confirm button')]
        .find((b) => b.textContent === 'Keep it')
        ?.click()
    })
    expect(host.querySelector('[role="alertdialog"]')).toBeNull()
    expect(onDelete).not.toHaveBeenCalled()
  })

  it('closes the confirm dialog when the list changes under it', () => {
    // M3: the dialog holds `{index, proposal}` while the rest of the list
    // stays interactive. Addressing a rule by position is right for this
    // list (this file's own doc comment says why an id cannot do the job),
    // but *holding* a position open across a mutation is not — a different,
    // lower-indexed rule being deleted while this dialog is open shifts the
    // array under `confirming.index`, and "Delete and reopen" would then
    // delete whatever slid into that slot and reopen a proposal nobody was
    // warned about. Closing on every list change is simpler, and correct,
    // whether or not the rule the dialog was about even still exists.
    const onDelete = vi.fn()
    const rules = [
      rule({ id: 1 }),
      rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' }),
      rule({ id: 3, ordinal: 3, pattern: 'Third/.*' }),
    ]
    const proposals = [proposal({ id: 42, created_rule_id: 3 })]
    let host: HTMLElement | undefined
    act(() => {
      host = mount(
        <RuleList
          rules={rules}
          selected={undefined}
          proposals={proposals}
          overrideCount={0}
          frozen={false}
          onSelect={vi.fn()}
          onToggle={vi.fn()}
          onReorder={vi.fn()}
          onAdd={vi.fn()}
          onDelete={onDelete}
        />,
      )
    })
    if (!host) throw new Error('nothing mounted')
    act(() => {
      host?.querySelectorAll<HTMLElement>('.rule-row button.delete')[2].click()
    })
    expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()

    // A different delete, at a lower index, lands while the dialog is open.
    rerender(
      <RuleList
        rules={[
          rule({ id: 2, ordinal: 1, pattern: 'Colour/.*' }),
          rule({ id: 3, ordinal: 2, pattern: 'Third/.*' }),
        ]}
        selected={undefined}
        proposals={proposals}
        frozen={false}
        overrideCount={0}
        onSelect={vi.fn()}
        onToggle={vi.fn()}
        onReorder={vi.fn()}
        onAdd={vi.fn()}
        onDelete={onDelete}
      />,
    )
    expect(host.querySelector('[role="alertdialog"]')).toBeNull()
  })

  it('marks dragover cancelable, or a real browser never lets the drop land', () => {
    // A browser refuses to fire `drop` on a target whose `dragover` did not
    // call `preventDefault()` — jsdom fires it regardless, which is exactly
    // why this checks the call itself rather than the reorder it enables.
    // Silently dropping this line would not fail a single existing test; it
    // would just make every reorder inert in production while the whole
    // suite stayed green.
    const { host } = render()
    const row = host.querySelectorAll<HTMLElement>('.rule-row')[0]
    const event = new Event('dragover', { bubbles: true, cancelable: true })
    act(() => {
      row.dispatchEvent(event)
    })
    expect(event.defaultPrevented).toBe(true)
  })

  it('deletes a rule no proposal wrote without a confirmation', () => {
    const { host, onDelete } = render()
    act(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[0].click()
    })
    expect(onDelete).toHaveBeenCalledWith(0)
    expect(host.querySelector('[role="alertdialog"]')).toBeNull()
  })

  it('adds a rule', () => {
    const { host, onAdd } = render()
    act(() => {
      host.querySelector<HTMLElement>('button.add-rule')?.click()
    })
    expect(onAdd).toHaveBeenCalled()
  })

  /**
   * The five gestures O96 freezes, read **per element**, as how many times a
   * press on each reached its handler and how each is drawn. A whole-page
   * check cannot tell "the row is frozen" from "the page rendered nothing".
   * Each is pressed before the hold is drawn, so it is the control's own
   * handler that turns the press down, and read once the delay has passed,
   * when a hold is drawn.
   */
  const gestures = (mounted: ReturnType<typeof render>) => {
    const { host } = mounted
    const toggle = host.querySelector(
      '.rule-row input[type="checkbox"]',
    ) as HTMLInputElement
    const select = host.querySelector(
      '.rule-row button.select',
    ) as HTMLButtonElement
    const remove = host.querySelector(
      '.rule-row button.delete',
    ) as HTMLButtonElement
    const add = host.querySelector('button.add-rule') as HTMLButtonElement
    act(() => {
      for (const control of [toggle, select, remove, add]) control.click()
    })
    pass(WAIT_DRAWN_AFTER_MS)
    return {
      draggable: host
        .querySelector<HTMLElement>('.rule-row')
        ?.getAttribute('draggable'),
      toggle: [mounted.onToggle.mock.calls.length, holdOf(toggle)],
      select: [mounted.onSelect.mock.calls.length, holdOf(select)],
      remove: [mounted.onDelete.mock.calls.length, holdOf(remove)],
      add: [mounted.onAdd.mock.calls.length, holdOf(add)],
    }
  }

  it('freezes every gesture while a save is in flight, and only while one is — O96', () => {
    // Both halves, because the positive alone passes against a component that
    // holds its controls always — which is the mechanical mis-fix this item
    // warns about, and it would freeze the list permanently.
    holdTheClock()
    expect(gestures(render({ frozen: true }))).toEqual({
      draggable: 'false',
      toggle: [0, 'held'],
      select: [0, 'held'],
      remove: [0, 'held'],
      add: [0, 'held'],
    })
    expect(gestures(render({ frozen: false }))).toEqual({
      draggable: 'true',
      toggle: [1, 'free'],
      select: [1, 'free'],
      remove: [1, 'free'],
      add: [1, 'free'],
    })
  })

  it('freezes the confirm dialog’s Delete and reopen too — O96, whole-branch review M2', () => {
    // The dialog can only be open across a save if it was opened before the
    // save started, because the Delete that opens it is itself held — so
    // the *same* `rules` array is passed to both renders, or `RuleList`'s own
    // M3 effect closes the dialog and this asserts nothing.
    //
    // It is the one frozen control the `gestures` helper above cannot reach,
    // and the one that reopens a proposal.
    const rules = [
      rule({ id: 1 }),
      rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' }),
    ]
    const proposals = [proposal({ id: 42, created_rule_id: 2 })]
    const props = {
      rules,
      selected: undefined,
      proposals,
      overrideCount: 0,
      onSelect: vi.fn(),
      onToggle: vi.fn(),
      onReorder: vi.fn(),
      onAdd: vi.fn(),
      onDelete: vi.fn(),
    }
    holdTheClock()
    let host: HTMLElement | undefined
    act(() => {
      host = mount(<RuleList {...props} frozen={false} />)
    })
    if (!host) throw new Error('nothing mounted')
    act(() => {
      host?.querySelectorAll<HTMLElement>('.rule-row button.delete')[1].click()
    })
    expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()
    const confirm = () =>
      host?.querySelector('button.confirm-delete') as HTMLButtonElement
    expect(holdOf(confirm())).toBe('free')

    act(() => {
      rerender(<RuleList {...props} frozen={true} />)
    })
    // Pressed before the hold is drawn: its handler turns it down, so it
    // deletes nothing and the dialog stays open.
    act(() => {
      confirm().click()
    })
    expect(props.onDelete).not.toHaveBeenCalled()
    expect(host.querySelector('[role="alertdialog"]')).not.toBeNull()
    pass(WAIT_DRAWN_AFTER_MS)
    expect(holdOf(confirm())).toBe('held')
  })

  it('refuses a drop that lands while a save is in flight — O96', () => {
    // `draggable={!frozen}` cannot cancel a drag that was already in progress
    // when the save started; flipping the attribute under it does nothing to
    // the drop that ends it. jsdom dispatches both events regardless of the
    // attribute, which is exactly what lets this assert the **handler's**
    // guard rather than the attribute's — remove the `if (frozen) return`
    // from `onDrop` and this fails while the test above still passes.
    const { host, onReorder } = render({ frozen: true })
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    const dt = dataTransfer()
    act(() => {
      rows[1].dispatchEvent(
        Object.assign(new Event('dragstart', { bubbles: true }), {
          dataTransfer: dt,
        }),
      )
      rows[0].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: dt,
          preventDefault: () => {},
        }),
      )
    })
    expect(onReorder).not.toHaveBeenCalled()
  })

  it('draws the rule count, and says nothing about cost below the threshold', () => {
    const { host } = render()
    expect(host.textContent).toContain('2 rules')
    expect(host.textContent).not.toContain('expensive')
  })

  it('says a preview is getting expensive past forty rules', () => {
    const many = Array.from({ length: 41 }, (_, i) =>
      rule({ id: i + 1, ordinal: i + 1 }),
    )
    const { host } = render({ rules: many })
    expect(host.textContent).toContain('expensive')
  })

  // Pins the boundary itself, not the behaviour on either side of it: forty
  // rules draws no warning. Without this, a mutant turning the `>` at
  // RuleList.tsx into `>=` survives the whole suite — the 2-rule and
  // 41-rule cases can't tell `>` from `>=` apart, only the 40th rule can.
  // Forty is a guess anchored to a measured curve's knee (10 extra rules 47
  // ms, 40 -> 401 ms, 80 -> 1.34 s, 160 -> 5.01 s, in a debug build on a
  // one-work fixture), not a measurement of this screen; the preview route
  // itself refuses past 200.
  it('draws no cost warning at exactly forty rules', () => {
    const forty = Array.from({ length: 40 }, (_, i) =>
      rule({ id: i + 1, ordinal: i + 1 }),
    )
    const { host } = render({ rules: forty })
    expect(host.textContent).not.toContain('expensive')
  })

  it('draws the override count as a badge', () => {
    const { host } = render({ overrideCount: 2 })
    expect(host.querySelector('.override-badge')?.textContent).toContain(
      '2 overrides',
    )
  })

  it('separates the rule count from the override badge', () => {
    const { host } = render({ overrideCount: 1 })
    const count = host.querySelector('.rule-count')
    // Exact text, not `toContain`: the defect this pins is a *missing*
    // separator, and `toContain('1 override')` passes against `4 rules1
    // override` just as happily as against the correct string.
    expect(count?.textContent).toBe('2 rules · 1 override')
  })

  it('pluralises the override badge and keeps the separator', () => {
    const { host } = render({ overrideCount: 3 })
    expect(host.querySelector('.rule-count')?.textContent).toBe(
      '2 rules · 3 overrides',
    )
  })

  it('says nothing about overrides when there are none', () => {
    const { host } = render({ overrideCount: 0 })
    expect(host.querySelector('.rule-count')?.textContent).toBe('2 rules')
  })

  it('gives the row button an accessible name that is not two strings run together', () => {
    const { host } = render({
      rules: [
        rule({
          kind: 'builtin-volume',
          match_on: 'name',
          pattern: '',
          set_series: null,
          note: 'volume from the name',
        }),
      ],
    })
    const button = host.querySelector('.select')
    expect(button?.getAttribute('aria-label')).toBe(
      'volume from the name — builtin on name → volume',
    )
    // WCAG 2.5.3, Label in Name: whatever a person sees on the control has to
    // be inside what a screen reader announces for it, or a voice-control user
    // cannot ask for it by the name they can read.
    const visible = button?.querySelector('.rule-name')?.textContent ?? ''
    expect(visible).not.toBe('')
    expect(button?.getAttribute('aria-label')).toContain(visible)
  })
})
