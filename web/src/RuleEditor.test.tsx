import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import * as curation from './curation'
import { RuleEditor } from './RuleEditor'
import * as rulesApi from './rules'
import type { Impact, Override, Preview, Rule, Saved } from './rules'
import { deferred, flush } from './testing/async'
import { holdOf } from './testing/held'
import { mount } from './testing/mount'
import {
  holdTheClock,
  pass,
  sentenceDrawn,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './testing/wait'

vi.mock('./rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
  importRules: vi.fn(),
}))
vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
}))
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))

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

const impact = (over: Partial<Impact> = {}): Impact => ({
  series_before: 8,
  series_after: 2,
  series_merged: 6,
  works_rehomed: 71,
  works_orphaned: 0,
  proposals_reopened: 0,
  proposals_raised: 0,
  ...over,
})

const anOverride = (over: Partial<Override> = {}): Override => ({
  field: 'series',
  rel_path: 'a/b',
  first: 0,
  wins: 1,
  first_value: 'A',
  wins_value: 'B',
  others: 0,
  examples: 3,
  ...over,
})

const previewBody = (over: Partial<Preview> = {}): Preview => ({
  impact: impact(),
  rows: [],
  rows_truncated: false,
  overrides: [],
  ...over,
})

/**
 * A whole `DeriveReport`, the same fixture shape `RemoveRoot.test.tsx` and
 * `RootRow.test.tsx` already use — `{} as Derived` is enough only until a
 * component reads one of the thirty-one fields, and this one now does.
 */
const derived = (over: Partial<config.Derived> = {}): config.Derived => ({
  series: 2,
  works: 71,
  editions: 71,
  members: 71,
  unattached_chapter_dirs: 0,
  unreadable: {
    total: 0,
    refused_format: 0,
    by_format: {},
    uncounted: [],
    vanished: [],
    no_pages: 0,
  },
  suppressed: 0,
  rekeyed: 0,
  kept_editions: 0,
  emptied_works: 0,
  unfolded_works: 0,
  unfoldable: [],
  unreadable_hand: [],
  repaired_documents: 0,
  unwritable_documents: [],
  orphan_documents: [],
  declined_documents: [],
  stale_documents: [],
  moved_positions: 0,
  lost_positions: 0,
  moved_notes: 0,
  dropped_notes: 0,
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
  uncarried_alignments: 0,
  series_holding_no_work: 0,
  merged: 0,
  unrecognised_kinds: 0,
  raised: 0,
  root_removed_mid_derive: 0,
  ...over,
})

const savedBody = (over: Partial<Saved> = {}): Saved => ({
  impact: impact(),
  rules: [rule()],
  reopened_proposals: [],
  complaints: [],
  derive: derived(),
  ...over,
})

async function render() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<RuleEditor />)
    await Promise.resolve()
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const clickText = async (host: HTMLElement, text: string) => {
  await act(async () => {
    ;[...host.querySelectorAll<HTMLElement>('button')]
      .find((b) => b.textContent?.includes(text))
      ?.click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

const saveButton = (host: HTMLElement) =>
  host.querySelector('button.save') as HTMLButtonElement
const importButton = (host: HTMLElement) =>
  host.querySelector('.import button') as HTMLButtonElement

/**
 * Press Save, and say how many saves the press sent. A held Save is
 * `aria-disabled`, which leaves the press to reach it, so a hold is read as
 * a press that sends nothing; one made before the hold is drawn is turned
 * down by `save`'s own guard.
 */
const savesSentByPress = async (host: HTMLElement) => {
  const before = vi.mocked(rulesApi.saveRules).mock.calls.length
  await act(async () => {
    saveButton(host).click()
    await Promise.resolve()
  })
  return vi.mocked(rulesApi.saveRules).mock.calls.length - before
}

/**
 * Press Import, and say how many file pickers the press opened. The picker
 * is the browser's, so what is counted is the hidden input's `click`.
 */
const pickersOpenedByPress = async (host: HTMLElement) => {
  const input = host.querySelector('input[type="file"]') as HTMLInputElement
  const picker = vi.spyOn(input, 'click').mockImplementation(() => {})
  await act(async () => {
    importButton(host).click()
    await Promise.resolve()
  })
  const opened = picker.mock.calls.length
  picker.mockRestore()
  return opened
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(rulesApi.listRules).mockResolvedValue({
    rules: [rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })],
    complaints: [],
  })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
  vi.mocked(rulesApi.previewRules).mockResolvedValue(previewBody())
  vi.mocked(rulesApi.saveRules).mockResolvedValue(savedBody())
})

describe('RuleEditor', () => {
  it('draws the rules it loaded', async () => {
    const host = await render()
    expect(host.querySelectorAll('.rule-row')).toHaveLength(2)
  })

  it('puts the preview outside the right pane, so it spans the width', async () => {
    const host = await render()
    const preview = host.querySelector('.preview')
    expect(preview).not.toBeNull()
    // The point of the move: Provenance is file + one column per metadata
    // field, and inside the panes' grid it is capped at whatever the list
    // column leaves at every width.
    expect(host.querySelector('.editor-panes .preview')).toBeNull()
    const box = preview?.closest('.settings-section')
    expect(box?.querySelector('.settings-section-title')?.textContent).toBe(
      'Preview',
    )
    expect(box?.parentElement?.className).toContain('rule-editor')
  })

  it('puts the save’s re-derive report after the boxes, in none of them', async () => {
    // Its heading is h3, beside Preview's: inside the Preview box it would
    // be drawn as a part of the preview while its heading said otherwise.
    const host = await render()
    await clickText(host, 'Save')
    const report = host.querySelector('details.save-derive')
    expect(report).not.toBeNull()
    expect(report?.closest('.settings-section')).toBeNull()
    expect(report?.parentElement?.className).toContain('rule-editor')
  })

  it('previews on selecting a different rule', async () => {
    // `rows` is scoped to `selected`, so changing which rule is open is the
    // only way to populate Matched files and Provenance for it — and it is one
    // gesture, not a stream. The build design's §5.3.
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][1]).toBe(1)
  })

  it('does not preview again when the already-selected rule is clicked', async () => {
    // M1: the design runs a preview on selecting a *different* rule — the
    // already-open one has nothing left for a preview to answer, so paying
    // another ~281 ms derivation (and clearing the "Saved:" line) for the
    // same click is wasted work rather than a feature.
    const host = await render()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[0].click()
      await Promise.resolve()
    })
    vi.mocked(rulesApi.previewRules).mockClear()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[0].click()
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).not.toHaveBeenCalled()
  })

  it('previews on an enable toggle', async () => {
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await act(async () => {
      host
        .querySelectorAll<HTMLInputElement>(
          '.rule-row input[type="checkbox"]',
        )[0]
        .click()
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][0][0].enabled).toBe(
      false,
    )
  })

  it('previews on an add', async () => {
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await clickText(host, '+ Add')
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][0]).toHaveLength(3)
  })

  it('previews on a delete, sending the shortened list', async () => {
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[0].click()
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][0]).toHaveLength(1)
  })

  it('fires no preview while typing in a rule field, and one on blur', async () => {
    // The screen's central cost decision, asserted rather than described —
    // spec C4 §7's own named test.
    const host = await render()
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[0].click()
      await Promise.resolve()
    })
    vi.mocked(rulesApi.previewRules).mockClear()

    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    for (const value of ['M', 'Ma', 'Man', 'Manga']) {
      act(() => {
        setter?.call(pattern, value)
        pattern.dispatchEvent(new Event('input', { bubbles: true }))
      })
    }
    expect(rulesApi.previewRules).not.toHaveBeenCalled()

    await act(async () => {
      pattern.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
    })
    expect(rulesApi.previewRules).toHaveBeenCalledTimes(1)
  })

  it('renumbers ordinals when a reorder changes the preview', async () => {
    // Reordering two rules changes the preview — spec C4 §7. Layering is
    // positional, and the server reads `ordinal`, so an array that moved
    // without renumbering would preview identically.
    const host = await render()
    vi.mocked(rulesApi.previewRules).mockClear()
    const rows = host.querySelectorAll<HTMLElement>('.rule-row')
    await act(async () => {
      rows[1].dispatchEvent(
        Object.assign(new Event('dragstart', { bubbles: true }), {
          dataTransfer: { setData: vi.fn(), effectAllowed: '' },
        }),
      )
      rows[0].dispatchEvent(
        Object.assign(new Event('drop', { bubbles: true }), {
          dataTransfer: { getData: () => '1' },
        }),
      )
      await Promise.resolve()
    })
    const sent = vi.mocked(rulesApi.previewRules).mock.calls[0][0]
    expect(sent.map((r) => r.pattern)).toEqual(['Colour/.*', 'Manga/.*'])
    expect(sent.map((r) => r.ordinal)).toEqual([1, 2])
  })

  it('states what Save will do, from the live preview', async () => {
    const host = await render()
    expect(host.querySelector('.save')?.textContent).toContain('71')
  })

  it('sends the whole list to the save route and states what it did', async () => {
    // Asserted on `.saved` specifically, not `host.textContent` — the Save
    // button already contains '71' before the click (it states the live
    // preview's own number), so a `host.textContent` assertion would still
    // pass even if the "Saved:" paragraph never rendered at all.
    const host = await render()
    await clickText(host, 'Save')
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    expect(vi.mocked(rulesApi.saveRules).mock.calls[0][0]).toHaveLength(2)
    expect(host.querySelector('.saved')?.textContent).toContain('71')
  })

  it('states the proposals a save reopened, by id', async () => {
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({
        reopened_proposals: [4, 9],
        impact: impact({ proposals_reopened: 2 }),
      }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.textContent).toContain('4')
    expect(host.textContent).toContain('9')
  })

  it('warns when the save’s impact differs from the preview it was made from', async () => {
    // Nothing should change between them on a single-writer library — and the
    // screen's entire promise is that the preview is true, so the cheapest
    // place to notice it is false is the save that just disproved it. §4.2.
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({ impact: impact({ works_rehomed: 4 }) }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'differs',
    )
  })

  it('draws the server’s own sentence when a save is refused', async () => {
    vi.mocked(rulesApi.saveRules).mockRejectedValue(
      new Error(
        '/api/library/rules: 400 rule 9999: no such rule exists to keep',
      ),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'no such rule exists to keep',
    )
  })

  it('draws the server’s own sentence when a non-admin loads it', async () => {
    // No client-side admin guard: `/config/rules` is reachable by URL and the
    // screen renders the 403's message. The build design's §2.
    vi.mocked(rulesApi.listRules).mockRejectedValue(
      new Error('/api/library/rules: 403 only an administrator may do this'),
    )
    const host = await render()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'only an administrator',
    )
  })

  it('draws what the save’s own re-derive found, not only the rehome count', async () => {
    // Whole-branch review, I5/context note: `Impact` describes the
    // database's own structure and nothing past it, so a save that could not
    // write a hand document used to answer `200` with a rehome count and
    // silence about `<root>/.taiyaku/`. `DeriveSection` is `ScanReport.tsx`'s
    // own renderer for this exact struct, reused here rather than a second,
    // narrower vocabulary.
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({
        derive: derived({
          unwritable_documents: ['/srv/manga/.taiyaku/locked.json'],
          orphan_documents: [
            { path: '/srv/manga/a.epub', title: 'A', absorbed_into: null },
          ],
        }),
      }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.textContent).toContain('/srv/manga/.taiyaku/locked.json')
    expect(
      host.querySelector('[data-field="orphan_documents"]')?.textContent,
    ).toContain('/srv/manga/a.epub')
  })

  it('reports a complaint a save answered', async () => {
    vi.mocked(rulesApi.saveRules).mockResolvedValue(
      savedBody({ complaints: ['rule 2: pattern will not compile'] }),
    )
    const host = await render()
    await clickText(host, 'Save')
    expect(host.textContent).toContain('will not compile')
  })

  it('draws import as destructive and says what it reopened', async () => {
    vi.mocked(rulesApi.importRules).mockResolvedValue({
      rules: [rule()],
      complaints: [],
      reopened_proposals: [7],
    })
    const host = await render()
    expect(
      [...host.querySelectorAll('button')].some((b) =>
        b.textContent?.includes('Import'),
      ),
    ).toBe(true)
    expect(host.textContent).toContain('replaces')
  })

  it('says the library is not yet derived from an import, and names Save as what applies it', async () => {
    // I3: `POST .../rules/import` writes the rule table and derives nothing
    // — the CLI prints this warning for the identical route
    // (`library.rs`'s `library rules --import`), and this screen used to say
    // nothing at all, leaving the library's structure silently still the old
    // rules' the moment the import appeared to succeed. No proposals
    // reopened here, on purpose: that half of the old message must not be
    // the only thing this notice ever said.
    vi.mocked(rulesApi.importRules).mockResolvedValue({
      rules: [rule()],
      complaints: [],
      reopened_proposals: [],
    })
    const host = await render()
    const input = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!input) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(input, 'files', { value: [file], configurable: true })
    await act(async () => {
      input.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    const notice = host.querySelector('[role="alert"]')?.textContent ?? ''
    expect(notice).toContain('not been re-derived')
    expect(notice).toContain('Save')
  })

  /** How the Preview heading's line draws "Previewing…". */
  const previewingOn = (host: HTMLElement) => {
    const head = host
      .querySelector('.preview')
      ?.closest('.settings-section')
      ?.querySelector(':scope > .settings-section-head')
    if (!head) throw new Error('no Preview heading')
    return sentenceDrawn(head, 'Previewing…')
  }

  it('draws Previewing… on the Preview heading’s line once the first preview has lasted, laid out unseen until then', async () => {
    holdTheClock()
    const held = deferred<Preview>()
    vi.mocked(rulesApi.previewRules).mockReturnValueOnce(held.promise)
    const host = await render()
    expect(previewingOn(host)).toBe('unseen')
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(previewingOn(host)).toBe('unseen')
    pass(1)
    expect(previewingOn(host)).toBe('drawn')
    await flush(() => held.resolve(previewBody()))
    expect(previewingOn(host)).toBe('unseen')
  })

  it('says it is working rather than drawing a stale answer', async () => {
    // A preview has landed and another is out: the impact on screen is the
    // last one's, so the line says a newer one is coming.
    holdTheClock()
    const host = await render()
    const held = deferred<Preview>()
    vi.mocked(rulesApi.previewRules).mockReturnValueOnce(held.promise)
    await flush(() => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
    })
    expect(host.querySelector('.impact')).not.toBeNull()
    pass(WAIT_DRAWN_AFTER_MS)
    expect(previewingOn(host)).toBe('drawn')
  })

  it('draws the server’s sentence when a preview is refused, and stops saying it is working', async () => {
    // `busy` is what draws "Previewing…" — a catch that forgot to clear it
    // would leave that message on screen forever after the one failure a
    // person is most likely to hit here: a pattern the store's regex engine
    // refuses. Checked as an absence, not just a presence: the message alone
    // would still pass with the screen stuck saying it is working. Read once
    // the wait's delay has passed, since "Previewing…" is drawn only then.
    holdTheClock()
    vi.mocked(rulesApi.previewRules).mockRejectedValueOnce(
      new Error('/api/library/rules/preview: 400 pattern does not compile'),
    )
    const host = await render()
    // No preview to save against is not a wait, so Save is drawn held at
    // once: no time has passed on the held clock.
    expect(holdOf(saveButton(host))).toBe('held')
    pass(WAIT_DRAWN_AFTER_MS)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'pattern does not compile',
    )
    expect(previewingOn(host)).toBe('unseen')
    // I2: a failed preview must not leave Save actionable — there is no
    // preview at all here (this is the very first one, rejected), so Save
    // has no number of its own to state and must stay held.
    expect(await savesSentByPress(host)).toBe(0)
    expect(holdOf(saveButton(host))).toBe('held')
  })

  it('invalidates the standing preview when a later one fails, so Save cannot act on a number that describes a different list', async () => {
    // I2's own discriminating case: the *first* preview (on mount) succeeds,
    // so `preview`/`predicted.current` hold a real, non-empty value before
    // the failure this test causes — unlike the test above, where there was
    // never a successful preview to leave stale in the first place.
    const host = await render()
    expect(holdOf(saveButton(host))).toBe('free')
    vi.mocked(rulesApi.previewRules).mockRejectedValueOnce(
      new Error(
        '/api/library/rules/preview: 400 rule 9999: no such rule exists to keep',
      ),
    )
    await act(async () => {
      host
        .querySelectorAll<HTMLInputElement>(
          '.rule-row input[type="checkbox"]',
        )[0]
        .click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(await savesSentByPress(host)).toBe(0)
    expect(holdOf(saveButton(host))).toBe('held')
    expect(saveButton(host).textContent).not.toContain('71')
  })

  it('leaves Save pressable again after a refusal, rather than stuck on "Saving…"', async () => {
    // `saving` is what holds the button and swaps its label — a catch that
    // forgot to clear it would leave a refused save looking like one still in
    // flight, with no way to try again short of a reload. Read once the
    // wait's delay has passed, since a stuck "Saving…" is drawn only then.
    holdTheClock()
    vi.mocked(rulesApi.saveRules).mockRejectedValue(
      new Error(
        '/api/library/rules: 400 rule 9999: no such rule exists to keep',
      ),
    )
    const host = await render()
    await clickText(host, 'Save')
    pass(WAIT_DRAWN_AFTER_MS)
    expect(holdOf(saveButton(host))).toBe('free')
    expect(saveButton(host).textContent).not.toContain('Saving')
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('refuses a second Save while the first is still in flight, rather than racing two PUTs', async () => {
    // `savingNow` is a ref precisely so this guard is checked synchronously,
    // ahead of the state update that disables the button — two clicks landing
    // before React re-renders would otherwise both pass the guard and fire
    // two PUTs racing each other over the same rows.
    let resolveSave: (value: Saved) => void = () => {}
    vi.mocked(rulesApi.saveRules).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveSave = resolve
        }),
    )
    const host = await render()
    const button = host.querySelector<HTMLButtonElement>('button.save')
    if (!button) throw new Error('no save button')
    await act(async () => {
      button.click()
      button.click()
      await Promise.resolve()
    })
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    await act(async () => {
      resolveSave(savedBody())
      await Promise.resolve()
      await Promise.resolve()
    })
  })

  /**
   * Holds a save open and returns its resolvers. The list this fixture loads
   * has an **unsaved second rule** (`id: 0`) that comes back with a minted
   * `id: 7` — a fixture returning the ids it sent cannot fail against the
   * merge taking the edit's own id, which is the defect that reopens
   * proposals for nothing.
   */
  const heldSave = async () => {
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule(), rule({ id: 0, ordinal: 2, pattern: 'Colour/.*' })],
      complaints: [],
    })
    let resolveSave: (value: Saved) => void = () => {}
    let rejectSave: (why: unknown) => void = () => {}
    vi.mocked(rulesApi.saveRules).mockImplementation(
      () =>
        new Promise((resolve, reject) => {
          resolveSave = resolve
          rejectSave = reject
        }),
    )
    const host = await render()
    // Open rule 2 — the unsaved one — so the form's commit lands on it.
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
      await Promise.resolve()
    })
    const button = host.querySelector<HTMLButtonElement>('button.save')
    if (!button) throw new Error('no save button')
    await act(async () => {
      button.click()
      await Promise.resolve()
    })
    // Loud rather than silent: the button is held on `busy`, and a click
    // that never fired would otherwise make every test below assert the
    // no-save-in-flight path while claiming to assert the other one.
    expect(rulesApi.saveRules).toHaveBeenCalledTimes(1)
    // `land` and `fail` read the `let`s above at call time, which is after
    // the executor assigned them.
    return {
      host,
      land: (answer: Saved) => resolveSave(answer),
      fail: (why: unknown) => rejectSave(why),
    }
  }

  /** The answer `heldSave`'s list comes back as: rule 2's `id: 0` is minted 7. */
  const heldSaveAnswer = () =>
    savedBody({
      rules: [rule(), rule({ id: 7, ordinal: 2, pattern: 'Colour/.*' })],
    })

  /** Blur a changed `pattern` field, which is what commits it. */
  const editPatternAndBlur = async (host: HTMLElement, value: string) => {
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    ) as HTMLInputElement
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    act(() => {
      setter?.call(pattern, value)
      pattern.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await act(async () => {
      pattern.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
      await Promise.resolve()
    })
  }

  it('keeps a form commit made while a save was in flight, on the server’s own row — O96', async () => {
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    // The edit survived the save that would have overwritten it...
    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('Edited/.*')
    // ...and the *server's* minted id came with it. Asserted through the next
    // preview's own argument, which is the list `rules` now holds — reading it
    // off the DOM is impossible, and that is precisely why taking the edit's
    // `id: 0` forward would ship unnoticed.
    const sent = vi.mocked(rulesApi.previewRules).mock.calls.at(-1)?.[0]
    expect(sent?.[1]).toMatchObject({ id: 7, pattern: 'Edited/.*', ordinal: 2 })
  })

  it('says the held edit is applied and unsaved, and the notice outlives the follow-up preview — O96', async () => {
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    // The follow-up preview has landed by now, and its success path calls
    // `setFailure(undefined)` — a notice stored in `failure` would be gone.
    const notice = host.querySelector('.held-edit')
    expect(notice?.textContent).toContain('rule 2')
    expect(notice?.textContent).toContain('unsaved')
    // The save's own report is still standing next to it, which is the whole
    // reason the notice exists rather than `commit`'s `setSaved(undefined)`.
    expect(host.querySelector('.saved')).not.toBeNull()
  })

  it('replaces the list with the server’s answer when nothing was edited during the save — O96', async () => {
    // The unchanged path. Without this, "keep the edit" could be implemented
    // as "never replace the list" and every other test here would still pass,
    // while unsaved rules kept `id: 0` forever.
    const { host, land } = await heldSave()
    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.held-edit')).toBeNull()
    const sent = vi.mocked(rulesApi.previewRules).mock.calls.at(-1)?.[0]
    expect(sent?.[1]).toMatchObject({ id: 7, pattern: 'Colour/.*' })
  })

  it('holds Import while a save is in flight — O96, whole-branch review C1', async () => {
    // Import is the one discarded gesture whose loss outlives the session:
    // `POST .../rules/import` writes the rule table, so an import landing
    // inside the save window leaves the store holding the imported rules and
    // the screen showing the list `setRules` puts back over them.
    //
    // `async`/`await` throughout, never `act(...).then(...)`: a rejection
    // chained off `act`'s thenable is swallowed, and this test passed while
    // printing its own AssertionError until that was fixed.
    holdTheClock()
    const { host, land } = await heldSave()
    // Pressed before the hold is drawn, so it is the handler that turns it
    // down: no picker, and no refusal said for a gesture that did nothing.
    expect(await pickersOpenedByPress(host)).toBe(0)
    expect(host.querySelector('p.refused')).toBeNull()
    pass(WAIT_DRAWN_AFTER_MS)
    expect(importButton(host).textContent).toBe('Import')
    expect(holdOf(importButton(host))).toBe('held')

    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The control, without which the assertions above pass against a button
    // that is held always.
    expect(holdOf(importButton(host))).toBe('free')
    expect(await pickersOpenedByPress(host)).toBe(1)
  })

  type ImportAnswer = {
    rules: Rule[]
    complaints: string[]
    reopened_proposals: number[]
  }

  /**
   * Holds an import open and returns its resolver — the mirror of `heldSave`.
   * The answer carries **different rules from the ones sent**, which is what
   * an import actually does: it mints fresh ids and replaces the list, so a
   * fixture echoing the sent rules back could not fail against an edit being
   * held across it rather than discarded.
   */
  const heldImport = async (openRule?: number) => {
    let resolveImport: (value: ImportAnswer) => void = () => {}
    let rejectImport: (reason: Error) => void = () => {}
    vi.mocked(rulesApi.importRules).mockImplementation(
      () =>
        new Promise((resolve, reject) => {
          resolveImport = resolve
          rejectImport = reject
        }),
    )
    const host = await render()
    // **Selected before the import starts, never during it.** The list is
    // frozen for the import's duration, so `button.select` is held and a
    // click inside the window opens nothing — which is correct behaviour and
    // makes "edit a rule mid-import" reachable only from a rule already open.
    if (openRule !== undefined) {
      await act(async () => {
        host
          .querySelectorAll<HTMLElement>('.rule-row button.select')
          [openRule].click()
        await Promise.resolve()
        await Promise.resolve()
      })
    }
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // Loud rather than silent, for `heldSave`'s own reason: an import that
    // never fired would make every test below assert the no-write-in-flight
    // path while claiming to assert the other one.
    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
    return {
      host,
      land: (a: ImportAnswer) => resolveImport(a),
      // A refused document is the ordinary failure on this route, so the
      // window's notices have to be readable against it and not only against
      // a landing.
      fail: (reason: Error) => rejectImport(reason),
    }
  }

  /**
   * What `heldImport`'s import answers with. `set_series` is what a rule row
   * draws — never `pattern` — so these two strings are how a test tells the
   * imported list from the loaded one on screen.
   */
  const importAnswer = (): ImportAnswer => ({
    rules: [
      rule({ id: 11, set_series: 'Imported A' }),
      rule({ id: 12, ordinal: 2, set_series: 'Imported B' }),
    ],
    complaints: [],
    reopened_proposals: [],
  })

  it('holds Save while an import is in flight, and frees it when the import lands — O97', async () => {
    holdTheClock()
    const { host, land } = await heldImport()
    // Turned down by `save`'s own guard before the hold is drawn, sending
    // nothing and saying nothing.
    expect(await savesSentByPress(host)).toBe(0)
    expect(host.querySelector('p.refused')).toBeNull()
    pass(WAIT_DRAWN_AFTER_MS)
    expect(holdOf(saveButton(host))).toBe('held')

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The control, without which the assertion above passes against a button
    // held for some other reason. Both halves are non-trivial: the notice
    // proves the resolver delivered, and the row proves the list it delivered
    // is the one on screen.
    const notice = [...host.querySelectorAll('.import p')]
      .map((n) => n.textContent ?? '')
      .find((t) => t.includes('imported rules are stored'))
    expect(notice).toBeDefined()
    expect(
      [...host.querySelectorAll('.rule-row')].some((r) =>
        r.textContent?.includes('Imported B'),
      ),
    ).toBe(true)
  })

  it('says Importing… and holds Import while an import is in flight, so a second pick cannot race the first — O97', async () => {
    holdTheClock()
    const { host, land } = await heldImport()
    expect(await pickersOpenedByPress(host)).toBe(0)
    expect(host.querySelector('p.refused')).toBeNull()
    pass(WAIT_DRAWN_AFTER_MS)
    expect([
      importButton(host).textContent,
      holdOf(importButton(host)),
    ]).toEqual(['Importing…', 'held'])

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect([
      importButton(host).textContent,
      holdOf(importButton(host)),
    ]).toEqual(['Import', 'free'])
    expect(await pickersOpenedByPress(host)).toBe(1)
  })

  it('freezes the rule list and the override options while an import is in flight — O97', async () => {
    // The Overrides row needs a preview that *has* one, and its own tab open:
    // with the default empty `overrides` those two buttons never render, so a
    // test naming them while asserting only on `.rule-row` would pass against
    // `PreviewTabs` never being frozen at all. `PreviewTabs.test.tsx` mounts
    // only with `frozen={false}`, so this is the one place that covers it.
    holdTheClock()
    vi.mocked(rulesApi.previewRules).mockResolvedValue(
      previewBody({ overrides: [anOverride()] }),
    )
    const { host, land } = await heldImport()
    await act(async () => {
      ;[...host.querySelectorAll<HTMLElement>('.preview-tabs button')]
        .find((b) => b.textContent?.startsWith('Overrides'))
        ?.click()
      await Promise.resolve()
    })
    // Per control, not per page: a page-level assertion cannot tell these
    // apart from the Save button it sits beside.
    const reorder = () =>
      host.querySelector('button.reorder') as HTMLButtonElement
    const edit = () =>
      host.querySelector('button.edit-winner') as HTMLButtonElement
    const toggle = () =>
      host.querySelector('.rule-row input[type="checkbox"]') as HTMLInputElement
    const select = () =>
      host.querySelector('.rule-row button.select') as HTMLButtonElement
    // Each would start a preview if it acted, and none does: pressed before
    // the hold is drawn, each is turned down by its own handler.
    vi.mocked(rulesApi.previewRules).mockClear()
    for (const control of [reorder(), edit(), toggle(), select()]) {
      await act(async () => {
        control.click()
        await Promise.resolve()
      })
    }
    expect(rulesApi.previewRules).not.toHaveBeenCalled()
    expect(toggle().checked).toBe(true)
    pass(WAIT_DRAWN_AFTER_MS)
    expect([reorder(), edit(), toggle(), select()].map(holdOf)).toEqual([
      'held',
      'held',
      'held',
      'held',
    ])
    expect(host.querySelector<HTMLElement>('.rule-row')?.draggable).toBe(false)

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect([reorder(), toggle()].map(holdOf)).toEqual(['free', 'free'])
    expect(host.querySelector<HTMLElement>('.rule-row')?.draggable).toBe(true)
    await act(async () => {
      toggle().click()
      await Promise.resolve()
    })
    expect(vi.mocked(rulesApi.previewRules).mock.calls.at(-1)?.[0][0]).toEqual(
      expect.objectContaining({ id: 11, enabled: false }),
    )
  })

  it('clears the gate when an import fails, so one bad file does not hold both writers for the session — O97', async () => {
    // The `.catch` half of taking the gate. A refused document is the
    // ordinary failure here, not an exotic one, so a leak on this path is
    // reached by the first person who picks the wrong file — and it is
    // invisible: the screen shows the server's sentence and nothing says
    // why neither button works any more.
    let rejectImport: (why: unknown) => void = () => {}
    vi.mocked(rulesApi.importRules).mockImplementation(
      () =>
        new Promise((_resolve, reject) => {
          rejectImport = reject
        }),
    )
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The control: the gate is held while the import is in flight.
    expect(await savesSentByPress(host)).toBe(0)

    await act(async () => {
      rejectImport(new Error('rules/import: not a rule document'))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // Import first: a save it sent would hold Import again.
    expect(await pickersOpenedByPress(host)).toBe(1)
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('holds Save in the same tick the import gate is taken, so a click sends nothing and says nothing — O97, closed further by O101', async () => {
    // **The ref guard is not what stands here.** `save`'s
    // `writer.current !== undefined` check exists because a hold taken from
    // state lands on a render while the ref is read synchronously — and the
    // gate taken inside `file.text()`'s `.then`, one microtask after the
    // change event returned, left a click landing in between to reach `save`
    // with Save still free. `beginImport` takes the gate inside the discrete
    // change event itself, which React flushes synchronously, so `frozen` and
    // `writer.current` change together and there is no tick in which only one
    // of them is standing.
    //
    // So the click is made at once, with no `await` to hide behind, and turned
    // down by `saveHeld`: it sends nothing, and the ref guard's refusal is not
    // said. Move the gate back after the read and the click sends a save,
    // which is what makes it O101's own regression guard.
    let resolveImport: (value: ImportAnswer) => void = () => {}
    vi.mocked(rulesApi.importRules).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveImport = resolve
        }),
    )
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const save = saveButton(host)
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      save.click()
      await Promise.resolve()
    })

    // Loud rather than silent, the same reason `heldImport` says so: an
    // import that never fired would leave Save held for some other reason
    // and this test would assert nothing.
    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
    expect(rulesApi.saveRules).not.toHaveBeenCalled()
    expect(host.querySelector('p.refused')).toBeNull()

    await act(async () => {
      resolveImport(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(holdOf(save)).toBe('free')
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('discards a form commit made during an import rather than holding it, and says which rule — O97', async () => {
    // Holding works for a save because `answer.rules` is positionally aligned
    // to the list that was sent. An import's answer is aligned to nothing: it
    // replaces every rule and mints fresh ids, so index 1 of the answer is not
    // the rule the person was editing and applying the edit there would
    // overwrite an unrelated rule with it. Discarding is forced, not chosen.
    const { host, land } = await heldImport(1)
    await editPatternAndBlur(host, 'Edited/.*')

    expect(host.querySelector('p.refused')?.textContent).toContain('rule 2')
    // The edit reached nothing: the list on screen is still the loaded one.
    expect(
      [...host.querySelectorAll('.rule-row')].some((r) =>
        r.textContent?.includes('Imported B'),
      ),
    ).toBe(false)

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The imported list is what is on screen, with no trace of the edit.
    expect(
      [...host.querySelectorAll('.rule-row')].some((r) =>
        r.textContent?.includes('Imported B'),
      ),
    ).toBe(true)
  })

  it('still holds a form commit made during a save, which this branch must not cost — O96, kept', async () => {
    // The regression guard: task 4 changes the code that decides between held
    // and discarded, and O96's whole claim lives on the other branch of it.
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('p.refused')).toBeNull()
    const pattern = host.querySelector<HTMLInputElement>(
      'input[name="pattern"]',
    )
    expect(pattern?.value).toBe('Edited/.*')
  })

  it('clears the gate when the document is not JSON, which throws before any promise exists — O97', async () => {
    // **`importRules` is not `async`.** Its body is
    // `expectBody(sendJson(..., documentOf(doc)))`, and the parse is
    // evaluated as an *argument* — synchronously, inside the call. A file
    // that is not valid JSON therefore throws out of `importRules` itself
    // rather than returning a rejected promise, so `.then`/`.catch` are never
    // attached and the `.catch` that clears the gate never runs. Picking the
    // wrong file is the most ordinary thing a person can do here, and the
    // symptom is that Save and Import are dead for the rest of the session
    // with only the import's failure on screen to explain it.
    //
    // The mock throws synchronously for exactly that reason: it is modelling
    // what `importRules` throws for such a file, not inventing a failure
    // mode.
    vi.mocked(rulesApi.importRules).mockImplementation(() => {
      throw new Error('the file is not a rules document')
    })
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const file = new File(['<html>'], 'rules.json', {
      type: 'application/json',
    })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    // Import first: a save it sent would hold Import again.
    expect(await pickersOpenedByPress(host)).toBe(1)
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('refuses an import that arrives while a save is in flight, and writes nothing — O96, whole-branch review C1', async () => {
    // The *second* guard, for the reason the drag has two: a file picker
    // opened before the save started is still open and still delivers. The
    // assertion that matters is `importRules` never being called — the button
    // being held cannot cover this, and refusing after the write would be
    // no refusal at all.
    const { host, land } = await heldSave()
    vi.mocked(rulesApi.importRules).mockClear()

    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(rulesApi.importRules).not.toHaveBeenCalled()
    // In `refused` since O100, not in `importNotice`: a message about a
    // gesture that did not happen cannot live in the slot owned by the one
    // that did. Here it survived there by luck — nothing in `save` writes
    // `importNotice` — and it moved with the second-import case, which did not.
    const notice = host.querySelector('p.refused')?.textContent ?? ''
    expect(notice).toContain('not applied')
    expect(notice).toContain('nothing was imported')
    // Set while the save is still running, so "now the save has finished"
    // was false for as long as it was on screen — O100's second half.
    expect(notice).toContain('once the save has finished')

    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
  })

  /**
   * Delivers a file to the hidden input — the gesture, not the button. Used
   * for the pick that lands *inside* a window, which is the only way to reach
   * `beginImport`'s own guard: the button is held by then, and a picker
   * opened before the window started still delivers.
   */
  const deliverFile = async (
    host: HTMLElement,
    file: { text: () => Promise<string> },
  ) => {
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    Object.defineProperty(fileInput, 'files', {
      value: [file],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
  }

  /**
   * A file whose `text()` is held open, so the window between picking a file
   * and its bytes arriving can be inspected. A real `File` resolves on the
   * next microtask and that window is unobservable; a large one takes as long
   * as the read, which is what O101 is about.
   */
  const heldReadFile = () => {
    let deliver: (text: string) => void = () => {}
    return {
      file: {
        text: () =>
          new Promise<string>((resolve) => {
            deliver = resolve
          }),
      },
      arrive: async (text: string) => {
        await act(async () => {
          deliver(text)
          await Promise.resolve()
          await Promise.resolve()
          await Promise.resolve()
        })
      },
    }
  }

  it('keeps the refusal of a second import on screen after the first import lands — O100', async () => {
    // Both of `beginImport`'s refusals used to go to `importNotice`, and the
    // running import's own success path overwrites that state — so the
    // sentence saying the second pick was refused is replaced by one saying
    // an import worked, and the person is left believing the file they picked
    // second is the one in the store. It is not; the first document is. This
    // is the failure the build design's §5 created `refused` to avoid,
    // reached in the function that reasoning was written for, and found by
    // driving a real library with the POST held open at the transport.
    const { host, land } = await heldImport()
    vi.mocked(rulesApi.importRules).mockClear()

    await deliverFile(
      host,
      new File(['[]'], 'second.json', { type: 'application/json' }),
    )
    expect(rulesApi.importRules).not.toHaveBeenCalled()
    const refused = host.querySelector('p.refused')?.textContent ?? ''
    expect(refused).toContain('imported twice')
    // **The clause, not just the sentence.** This one is set while the first
    // import is still running, so "now the first has finished" was false for
    // the width of the window — the same tense defect as the discard notice
    // below, and the reason to assert the half that was wrong rather than a
    // substring that skips it.
    expect(refused).toContain('once the first has finished')

    await act(async () => {
      land(importAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The half that fails against the defect: the refusal outlives the answer
    // that would otherwise erase it, beside the import's own sentence.
    expect(host.querySelector('p.refused')?.textContent).toContain(
      'imported twice',
    )
    expect(
      [...host.querySelectorAll('.import p')]
        .map((n) => n.textContent ?? '')
        .find((t) => t.includes('imported rules are stored')),
    ).toBeDefined()
  })

  it('tells a discarded edit what the import will do, not what it has already done — O100', async () => {
    // The sentence ended "The imported rules are on screen now" and is set
    // while the import is still in flight, so for the width of the window it
    // describes a list that is not on screen yet. At two milliseconds nobody
    // reads it; held open at the transport it is the only thing on screen,
    // which is how it was found.
    const { host, fail } = await heldImport(1)
    await editPatternAndBlur(host, 'Edited/.*')

    const refused = host.querySelector('p.refused')?.textContent ?? ''
    expect(refused).toContain('rule 2')
    expect(refused).toContain('once the import has finished')
    expect(refused).not.toContain('on screen now')

    // **And it has to stay true when the import does not land.** An earlier
    // wording promised the imported rules would replace the list; a refused
    // document is the ordinary failure here, so that promise was broken by
    // the commonest ending this route has. The list is still the loaded one
    // and the notice must not say otherwise.
    await act(async () => {
      fail(new Error('rules/import: not a rule document'))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('p.refused')?.textContent).toBe(refused)
    expect(
      [...host.querySelectorAll('.rule-row')].some((r) =>
        r.textContent?.includes('Imported'),
      ),
    ).toBe(false)
  })

  it('takes the writer gate before the file is read, so Import says Importing… while the bytes are still arriving — O101', async () => {
    // The gate was taken inside `doImport`, which ran in `file.text()`'s
    // `.then` — so between picking a file and its bytes arriving, Import was
    // enabled and `writer.current` was free. On a small document that window
    // is one microtask; on a large one it is however long the read takes, and
    // nothing on screen said the gesture had started.
    holdTheClock()
    vi.mocked(rulesApi.importRules).mockResolvedValue(importAnswer())
    const held = heldReadFile()
    const host = await render()
    await deliverFile(host, held.file)

    // Pressed before the hold is drawn, so it is the handlers that turn them
    // down.
    expect(await pickersOpenedByPress(host)).toBe(0)
    expect(await savesSentByPress(host)).toBe(0)
    pass(WAIT_DRAWN_AFTER_MS)
    expect([
      importButton(host).textContent,
      holdOf(importButton(host)),
    ]).toEqual(['Importing…', 'held'])
    expect(holdOf(saveButton(host))).toBe('held')
    // Not called yet: the gate is held for a read that has not finished, which
    // is the whole claim.
    expect(rulesApi.importRules).not.toHaveBeenCalled()

    await held.arrive('[]')
    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
    expect(holdOf(importButton(host))).toBe('free')
    expect(await pickersOpenedByPress(host)).toBe(1)
  })

  it('refuses a second pick made while the first file is still being read — O101', async () => {
    // The consequence of the window above, and the route by which O100 is
    // reachable without a synthetic event: pick a large file, pick a small one
    // while it is still being read, and the small one's import is running when
    // the large one's finally fires.
    vi.mocked(rulesApi.importRules).mockResolvedValue(importAnswer())
    const held = heldReadFile()
    const host = await render()
    await deliverFile(host, held.file)
    await deliverFile(
      host,
      new File(['[]'], 'second.json', { type: 'application/json' }),
    )

    expect(rulesApi.importRules).not.toHaveBeenCalled()
    expect(host.querySelector('p.refused')?.textContent).toContain(
      'imported twice',
    )

    await held.arrive('[]')
    // One write, not two: the first pick's, the one that took the gate.
    expect(rulesApi.importRules).toHaveBeenCalledTimes(1)
  })

  it('clears the writer gate when the file cannot be read — O101', async () => {
    // The gate is taken before the read, so the read's own failure has to
    // release it. This is the third instance on this screen of a gate taken
    // and never released, after the failed import and the document that is
    // not JSON, and it is new because the gate moved.
    const host = await render()
    const fileInput = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!fileInput) throw new Error('no file input')
    Object.defineProperty(fileInput, 'files', {
      value: [{ text: () => Promise.reject(new Error('could not be read')) }],
      configurable: true,
    })
    await act(async () => {
      fileInput.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(host.querySelector('p[role="alert"]')?.textContent).toContain(
      'could not be read',
    )
    // Import first: a save it sent would hold Import again.
    expect(await pickersOpenedByPress(host)).toBe(1)
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('clears the held-edit notice when a later gesture changes the list — O96, whole-branch review M3', async () => {
    // The notice names a rule by *position* and claims it is the list on
    // screen. Delete a lower-indexed rule and both halves are false. This is
    // `RuleList`'s own M3 for its confirm dialog, for the same reason.
    const { host, land } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')
    await act(async () => {
      land(heldSaveAnswer())
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.held-edit')).not.toBeNull()

    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[0].click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.held-edit')).toBeNull()
  })

  it('keeps a held edit when the save fails, and states the failure where the preview cannot wipe it — O96', async () => {
    // Spec §5.1. `commit` fires a preview whose success path clears
    // `failure`, so the save's own reason goes to the notice slot on this
    // path — otherwise the 409 a scan causes flashes and vanishes.
    const { host, fail } = await heldSave()
    await editPatternAndBlur(host, 'Edited/.*')

    await act(async () => {
      fail(new Error('a scan is currently running'))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value,
    ).toBe('Edited/.*')
    // Said as what was not done and why, then what became of the edit.
    expect(host.querySelector('.held-edit')?.textContent).toBe(
      'The rules were not saved: a scan is currently running. Your edit to ' +
        'rule 2 arrived while the save was running — it is applied to the ' +
        'list on screen and is unsaved.',
    )
  })

  it('holds Save while a preview is in flight, and frees it once the preview lands', async () => {
    // Save compares the answer it gets back against `predicted.current` — the
    // *last landed* preview's impact. A click that lands while a preview is
    // still in flight would send the just-committed rules against a stale
    // impact and draw the "something changed the library" warning for a race
    // the client caused, not the server. This file's own top comment.
    holdTheClock()
    const host = await render()
    let resolvePreview: (value: Preview) => void = () => {}
    vi.mocked(rulesApi.previewRules).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolvePreview = resolve
        }),
    )
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
    })
    // Pressed before the hold is drawn, so it is `save`'s guard that turns
    // it down.
    expect(await savesSentByPress(host)).toBe(0)
    pass(WAIT_DRAWN_AFTER_MS)
    expect(holdOf(saveButton(host))).toBe('held')

    await act(async () => {
      resolvePreview(previewBody())
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(holdOf(saveButton(host))).toBe('free')
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('re-previews the saved list once a save lands, and holds Save until that lands too', async () => {
    // I1's own discriminating case. Without `save` re-previewing on landing,
    // this second `previewRules` mock is never consumed at all — the button
    // is free the instant `saveRules` resolves and keeps stating '71', the
    // pre-save number, forever after the library has already been re-homed
    // to match it.
    holdTheClock()
    let resolveFollowUp: (value: Preview) => void = () => {}
    vi.mocked(rulesApi.previewRules)
      .mockResolvedValueOnce(previewBody()) // the initial mount preview
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFollowUp = resolve
          }),
      )
    const host = await render()
    const button = host.querySelector<HTMLButtonElement>('button.save')
    await act(async () => {
      button?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The save itself landed — but its own re-preview has not, so Save must
    // stay held rather than free against the number that preview is about to
    // replace.
    expect(await savesSentByPress(host)).toBe(0)
    pass(WAIT_DRAWN_AFTER_MS)
    expect(holdOf(saveButton(host))).toBe('held')
    await act(async () => {
      resolveFollowUp(
        previewBody({ impact: impact({ works_rehomed: 0, series_merged: 0 }) }),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(holdOf(saveButton(host))).toBe('free')
    expect(saveButton(host).textContent).toContain('0')
    expect(saveButton(host).textContent).not.toContain('71')
    expect(await savesSentByPress(host)).toBe(1)
  })

  it('carries the id the server actually assigned on a second save, not the freshly-added rule’s 0', async () => {
    // Weak-test fix: `save()`'s `setRules(answer.rules)` is what replaces a
    // freshly-added rule's `id: 0` with the row the store actually created.
    // Delete that line and every other test in this file still passes; only
    // this one notices, because without it a second save re-sends `id: 0`
    // and the store reads that as "insert a new row" rather than "keep the
    // one just created" — a duplicated rule and an orphaned
    // `created_rule_id`.
    const withRealId = [
      rule(),
      rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' }),
      rule({
        id: 42,
        ordinal: 3,
        pattern: '',
        set_series: null,
        set_edition_key: null,
        set_kind: null,
        set_language: null,
      }),
    ]
    vi.mocked(rulesApi.saveRules)
      .mockResolvedValueOnce(savedBody({ rules: withRealId }))
      .mockResolvedValueOnce(savedBody({ rules: withRealId }))
    const host = await render()
    await clickText(host, '+ Add')
    await clickText(host, 'Save')
    // Let the save's own follow-up preview land and free Save
    // before the second click — the same allowance the import follow-up
    // preview test below gives it, and for the identical reason: it is a
    // second async hop off the back of the first, not one the initial
    // `act` block's two ticks are guaranteed to cover.
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    await clickText(host, 'Save')
    const secondSend = vi.mocked(rulesApi.saveRules).mock.calls[1]?.[0]
    expect(secondSend?.map((r) => r.id)).not.toContain(0)
    expect(secondSend?.map((r) => r.id)).toContain(42)
  })

  it('does not let a failed proposals refresh undo a save that already succeeded', async () => {
    // The refresh only re-reads which proposals are now marked "from a
    // proposal" — a badge concern, not the save's own result. A refresh that
    // failed offline must not turn an already-successful save into an
    // unhandled rejection; the save's own message stays on screen either way.
    const host = await render()
    vi.mocked(curation.listProposals).mockRejectedValueOnce(
      new Error('offline'),
    )
    await clickText(host, 'Save')
    expect(host.querySelector('.saved')?.textContent).toContain('71')
  })

  it('draws the server’s own sentence when an imported document is refused', async () => {
    vi.mocked(rulesApi.importRules).mockRejectedValue(
      new Error(
        '/api/library/rules/import: 400 rule 2: pattern will not compile',
      ),
    )
    const host = await render()
    const input = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!input) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(input, 'files', { value: [file], configurable: true })
    await act(async () => {
      input.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'pattern will not compile',
    )
  })

  it('draws a failure when the selected file cannot be read, rather than importing nothing silently', async () => {
    // A separate failure path from the one above: this file never reaches
    // `importRules` at all, because reading it is what failed.
    const host = await render()
    const input = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!input) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    file.text = () => Promise.reject(new Error('the file could not be read'))
    Object.defineProperty(input, 'files', { value: [file], configurable: true })
    await act(async () => {
      input.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'the file could not be read',
    )
    expect(rulesApi.importRules).not.toHaveBeenCalled()
  })

  it('replaces the rules from an imported file, and names the proposals it reopened', async () => {
    vi.mocked(rulesApi.importRules).mockResolvedValue({
      rules: [rule({ id: 5, pattern: 'Imported/.*', set_series: null })],
      complaints: [],
      reopened_proposals: [7, 12],
    })
    const host = await render()
    // Exercises the visible button's own handler, not only the hidden input
    // it triggers.
    await clickText(host, 'Import')
    const input = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!input) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(input, 'files', { value: [file], configurable: true })
    await act(async () => {
      input.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelectorAll('.rule-row')).toHaveLength(1)
    expect(
      host.querySelector<HTMLElement>('.rule-row button.select')?.textContent,
    ).toContain('Imported')
    expect(host.textContent).toContain('7')
    expect(host.textContent).toContain('12')
  })

  it('keeps naming the proposals an import reopened after its own follow-up preview resolves', async () => {
    // `sendImport` fires its own `runPreview` right after landing, and that
    // preview's success path unconditionally clears `failure` — so a notice
    // stored there was wiped a preview-cycle later, almost always before a
    // person could read it, by the very call the import itself triggered.
    // The follow-up preview is left to resolve normally here (the default
    // mock resolves it), rather than held open, because that resolution is
    // exactly the moment the old code lost the message — an assertion made
    // before it would pass against the broken version too.
    vi.mocked(rulesApi.importRules).mockResolvedValue({
      rules: [rule({ id: 5, pattern: 'Imported/.*', set_series: null })],
      complaints: [],
      reopened_proposals: [7, 12],
    })
    const host = await render()
    const input = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!input) throw new Error('no file input')
    const file = new File(['[]'], 'rules.json', { type: 'application/json' })
    Object.defineProperty(input, 'files', { value: [file], configurable: true })
    await act(async () => {
      input.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    // Give the follow-up preview — already in flight from the dispatch above
    // — every remaining chance to land before the assertion.
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('7')
    expect(host.textContent).toContain('12')
  })

  it('reorders from the Overrides tab’s own button, the same as a drag would', async () => {
    // `PreviewTabs` carries its own `onReorder`, wired separately from
    // `RuleList`'s drag-and-drop — this is the only path that exercises it.
    vi.mocked(rulesApi.previewRules).mockResolvedValue(
      previewBody({
        overrides: [anOverride()],
      }),
    )
    const host = await render()
    await clickText(host, 'Overrides')
    vi.mocked(rulesApi.previewRules).mockClear()
    await clickText(host, 'Move rule 2 above rule 1')
    const sent = vi.mocked(rulesApi.previewRules).mock.calls[0][0]
    expect(sent.map((r) => r.pattern)).toEqual(['Colour/.*', 'Manga/.*'])
  })

  it('opens the winning rule in the form from an override row', async () => {
    // **O95's second option, and the only path that exercises it.** "Edit
    // rule 2" is how a person narrows the rule that won, which no button can
    // do for them — and it must select the *winner*, since selecting the
    // loser would put the wrong regex in front of them. Selecting a rule is
    // also a preview trigger, so the request that follows is the proof it
    // actually selected one.
    vi.mocked(rulesApi.previewRules).mockResolvedValue(
      previewBody({ overrides: [anOverride()] }),
    )
    const host = await render()
    await clickText(host, 'Overrides')
    vi.mocked(rulesApi.previewRules).mockClear()
    await clickText(host, 'Edit rule 2')
    expect(vi.mocked(rulesApi.previewRules).mock.calls[0][1]).toBe(1)
  })

  it('draws a failure instead of exporting when a rule scopes to a root this library does not have', async () => {
    // `exportDoc` runs inline in the Export link's own `href`, on every
    // render — a root deleted out from under a still-scoped rule has to show
    // the reason rather than crash the whole screen building an href nobody
    // asked to see. It previously did crash: an unguarded `setFailure` here
    // fired again on every render it caused, and this rule's mismatch never
    // stops being true, so React's render-phase-update budget ran out and
    // the whole screen died with "Too many re-renders" instead of drawing
    // this message. Fixed by comparing against the current failure before
    // setting it. This checks the settled state rather than the instant
    // right after mount, since the message keeps re-asserting itself on
    // every later render too, for as long as the mismatch stands.
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule({ root_id: 99 })],
      complaints: [],
    })
    const host = await render()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('99')
    expect(host.querySelector('a.export')?.getAttribute('href')).toBe(
      'data:application/json,',
    )
  })

  it('exports the current rules as a data: URI, decodable back to the document', async () => {
    const host = await render()
    const href = host.querySelector('a.export')?.getAttribute('href') ?? ''
    const decoded = decodeURIComponent(
      href.replace('data:application/json,', ''),
    )
    expect(JSON.parse(decoded)).toEqual(
      rulesApi.toDocument(
        [rule(), rule({ id: 2, ordinal: 2, pattern: 'Colour/.*' })],
        [],
      ),
    )
  })

  describe('its waits, drawn only once they have lasted', () => {
    const firstSelect = (host: HTMLElement) =>
      host.querySelector('.rule-row button.select') as HTMLButtonElement
    /** Save's words, and how Save, Import and the list are drawn. */
    const whileSaving = (host: HTMLElement) => [
      saveButton(host).textContent,
      holdOf(saveButton(host)),
      holdOf(importButton(host)),
      holdOf(firstSelect(host)),
    ]
    /** Import's words, and how Import, Save and the list are drawn. */
    const whileImporting = (host: HTMLElement) => [
      importButton(host).textContent,
      holdOf(importButton(host)),
      holdOf(saveButton(host)),
      holdOf(firstSelect(host)),
    ]

    it('draws no wait and holds nothing for a save answered inside the wait’s delay', async () => {
      holdTheClock()
      const sent = deferred<Saved>()
      vi.mocked(rulesApi.saveRules).mockReturnValue(sent.promise)
      const host = await render()
      const drawn = watchDrawn(() => JSON.stringify(whileSaving(host)))
      await clickText(host, 'Save')
      pass(WAIT_DRAWN_AFTER_MS - 1)
      // The save's answer, and the preview of what it saved after it.
      await act(async () => {
        sent.resolve(savedBody())
        await Promise.resolve()
        await Promise.resolve()
        await Promise.resolve()
        await Promise.resolve()
      })
      pass(WAIT_DRAWN_AFTER_MS)
      drawn.stop()
      expect(host.querySelector('.saved')).not.toBeNull()
      expect([...drawn.seen]).toEqual([
        JSON.stringify(['Save — apply to 71 works', 'free', 'free', 'free']),
      ])
    })

    it('draws Saving… and holds Save, Import and the list from the wait’s delay', async () => {
      holdTheClock()
      vi.mocked(rulesApi.saveRules).mockReturnValue(new Promise(() => {}))
      const host = await render()
      await clickText(host, 'Save')
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(whileSaving(host)).toEqual([
        'Save — apply to 71 works',
        'free',
        'free',
        'free',
      ])
      pass(1)
      expect(whileSaving(host)).toEqual(['Saving…', 'held', 'held', 'held'])
    })

    it('draws Importing… and holds Import, Save and the list from the wait’s delay', async () => {
      holdTheClock()
      const { host } = await heldImport()
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(whileImporting(host)).toEqual(['Import', 'free', 'free', 'free'])
      pass(1)
      expect(whileImporting(host)).toEqual([
        'Importing…',
        'held',
        'held',
        'held',
      ])
    })

    it('keeps the box of the wider of its two words on Save and on Import, and names each by the words drawn', async () => {
      // The words not drawn are laid unseen in the button's cell
      // (`WaitWords`), from an attribute, so they are not in its name.
      const host = await render()
      const unseen = (button: HTMLButtonElement) =>
        button
          .querySelector('[aria-hidden="true"]')
          ?.getAttribute('data-unseen')
      for (const [button, words, wait] of [
        [saveButton(host), 'Save — apply to 71 works', 'Saving…'],
        [importButton(host), 'Import', 'Importing…'],
      ] as const) {
        expect(button.classList.contains('keeps-wait-width')).toBe(true)
        expect([button.textContent, unseen(button)]).toEqual([words, wait])
      }
    })
  })

  describe('the delete/selection index arithmetic', () => {
    const threeRules = () => [
      rule({ id: 1, ordinal: 1, pattern: 'A/.*' }),
      rule({ id: 2, ordinal: 2, pattern: 'B/.*' }),
      rule({ id: 3, ordinal: 3, pattern: 'C/.*' }),
    ]

    const openPattern = (host: HTMLElement) =>
      host.querySelector<HTMLInputElement>('input[name="pattern"]')?.value

    async function selectAndDelete(selectedIndex: number, deleteIndex: number) {
      vi.mocked(rulesApi.listRules).mockResolvedValue({
        rules: threeRules(),
        complaints: [],
      })
      const host = await render()
      await act(async () => {
        host
          .querySelectorAll<HTMLElement>('.rule-row button.select')
          [selectedIndex].click()
        await Promise.resolve()
      })
      await act(async () => {
        host
          .querySelectorAll<HTMLElement>('.rule-row button.delete')
          [deleteIndex].click()
        await Promise.resolve()
      })
      return host
    }

    it('shifts the open rule down by one when the deleted rule sat before it', async () => {
      const host = await selectAndDelete(2, 0) // C open, delete A
      expect(openPattern(host)).toBe('C/.*')
    })

    it('closes the form when the deleted rule was the one open', async () => {
      const host = await selectAndDelete(1, 1) // B open, delete B
      expect(host.querySelector('input[name="pattern"]')).toBeNull()
      expect(host.textContent).toContain('Pick a rule')
    })

    it('leaves the open rule’s index alone when the deleted rule sat after it', async () => {
      // The trap: a fix that decremented on every delete, rather than only
      // when the deleted row sat at-or-before the open one, would still pass
      // the "before" case above — selected-1 either way — and only this case
      // catches it. A later row disappearing does not move an earlier one.
      const host = await selectAndDelete(0, 2) // A open, delete C
      expect(openPattern(host)).toBe('A/.*')
    })
  })
})

/**
 * **`runPreview`'s two `seq !== latest.current` guards, which no test had
 * reached** — 116 calls went through the first of them and not one was stale.
 *
 * Every edit fires a preview and a preview is the slow call on this screen
 * (~281 ms is the figure the design quotes), so two are routinely outstanding
 * and the server need not answer them in order. A stale answer landing last is
 * a Save button stating a count derived for a rule list nobody is looking at.
 */
describe('RuleEditor, a preview overtaken by a newer one', () => {
  /** Select rule 2 against a held preview, then rule 1 against a prompt one. */
  const overtaken = async (first: Promise<Preview>) => {
    const host = await render()
    vi.mocked(rulesApi.previewRules)
      .mockReturnValueOnce(first)
      .mockReturnValueOnce(
        Promise.resolve(previewBody({ impact: impact({ works_rehomed: 12 }) })),
      )
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[1].click()
      await Promise.resolve()
    })
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.select')[0].click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.save')?.textContent).toContain('12')
    return host
  }

  it('does not draw the older answer over the newer one', async () => {
    const slow = deferred<Preview>()
    const host = await overtaken(slow.promise)
    await flush(() =>
      slow.resolve(previewBody({ impact: impact({ works_rehomed: 99 }) })),
    )
    const save = host.querySelector('.save')?.textContent ?? ''
    expect(save).toContain('12')
    expect(save).not.toContain('99')
  })

  it("does not let the older answer's failure invalidate the newer one", async () => {
    // The catch clears `preview` as well as setting the banner — I2's rule,
    // and the reason this guard is not merely cosmetic: a stale rejection
    // without it holds Save over a preview that succeeded afterwards.
    const slow = deferred<Preview>()
    const host = await overtaken(slow.promise)
    await flush(() => slow.reject(new Error('/api/library/rules/preview: 500')))
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('.save')?.textContent).toContain('12')
    expect(holdOf(saveButton(host))).toBe('free')
    expect(await savesSentByPress(host)).toBe(1)
  })
})

/**
 * Each failure on the screen is said as a sentence: what was not done, then
 * why, in the server's own words where its answer carries some, by its
 * status where the answer is a page of markup or nothing, and as thrown
 * where nothing answered. Never as an address and a status with a body.
 */
describe('RuleEditor’s failures, each said as what was not done and why', () => {
  const MARKUP = '<html><body><h1>502 Bad Gateway</h1></body></html>'

  /** Hands the hidden input `file`, as the browser's picker does. */
  const pick = async (
    host: HTMLElement,
    file: { text: () => Promise<string> },
  ) => {
    const input = host.querySelector<HTMLInputElement>('input[type="file"]')
    if (!input) throw new Error('no file input')
    Object.defineProperty(input, 'files', { value: [file], configurable: true })
    await act(async () => {
      input.dispatchEvent(new Event('change', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
      await Promise.resolve()
    })
  }
  const aDocument = { text: () => Promise.resolve('[]') }
  const alerts = (host: HTMLElement) =>
    [...host.querySelectorAll('[role="alert"]')].map((p) => p.textContent)

  it.each([
    [
      'a load refused in a sentence',
      async () => {
        vi.mocked(rulesApi.listRules).mockRejectedValue(
          new Error('/api/library/rules: 403 this route is an administrator’s'),
        )
        return render()
      },
      'The rules were not loaded: this route is an administrator’s',
    ],
    [
      'a load answered with markup',
      async () => {
        vi.mocked(rulesApi.listRules).mockRejectedValue(
          new Error(`/api/library/rules: 502 ${MARKUP}`),
        )
        return render()
      },
      'The rules were not loaded: the server answered 502',
    ],
    [
      'a preview refused',
      async () => {
        vi.mocked(rulesApi.previewRules).mockRejectedValueOnce(
          new Error('/api/library/rules/preview: 400 pattern does not compile'),
        )
        return render()
      },
      'The rules were not previewed: pattern does not compile',
    ],
    [
      'a save refused',
      async () => {
        vi.mocked(rulesApi.saveRules).mockRejectedValue(
          new Error(
            '/api/library/rules: 400 rule 9999: no such rule exists to keep',
          ),
        )
        const host = await render()
        await clickText(host, 'Save')
        return host
      },
      'The rules were not saved: rule 9999: no such rule exists to keep',
    ],
    [
      'a save answered with markup',
      async () => {
        vi.mocked(rulesApi.saveRules).mockRejectedValue(
          new Error(`/api/library/rules: 502 ${MARKUP}`),
        )
        const host = await render()
        await clickText(host, 'Save')
        return host
      },
      'The rules were not saved: the server answered 502',
    ],
    [
      'an import refused',
      async () => {
        vi.mocked(rulesApi.importRules).mockRejectedValue(
          new Error(
            '/api/library/rules/import: 400 rule 2: pattern will not compile',
          ),
        )
        const host = await render()
        await pick(host, aDocument)
        return host
      },
      'The rules were not imported: rule 2: pattern will not compile',
    ],
    [
      'an import answered with markup',
      async () => {
        vi.mocked(rulesApi.importRules).mockRejectedValue(
          new Error(`/api/library/rules/import: 502 ${MARKUP}`),
        )
        const host = await render()
        await pick(host, aDocument)
        return host
      },
      'The rules were not imported: the server answered 502',
    ],
    [
      'a file that cannot be read',
      async () => {
        const host = await render()
        await pick(host, {
          text: () => Promise.reject(new Error('the file could not be read')),
        })
        return host
      },
      'The rules were not imported: the file could not be read',
    ],
    [
      'a file that is not JSON',
      async () => {
        // `importRules` itself, which turns the file down before it sends.
        const { importRules } =
          await vi.importActual<typeof import('./rules')>('./rules')
        vi.mocked(rulesApi.importRules).mockImplementationOnce(importRules)
        const host = await render()
        await pick(host, {
          text: () =>
            Promise.resolve(
              '<!doctype html>\n<html><body><h1>Rules</h1></body></html>',
            ),
        })
        return host
      },
      'The rules were not imported: the file is not a rules document',
    ],
    [
      'an export of a rule scoped to a root the library does not have',
      async () => {
        vi.mocked(rulesApi.listRules).mockResolvedValue({
          rules: [rule({ root_id: 99 })],
          complaints: [],
        })
        return render()
      },
      'The rules cannot be exported: rule 1 names root 99, which this library does not have',
    ],
  ] as const)('says %s', async (_, failing, said) => {
    const host = await failing()
    expect(alerts(host)).toEqual([said])
  })
})
