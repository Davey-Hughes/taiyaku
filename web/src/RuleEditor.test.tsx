import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import * as curation from './curation'
import { RuleEditor } from './RuleEditor'
import * as rulesApi from './rules'
import type { Impact, Preview, Rule, Saved } from './rules'
import { mount } from './testing/mount'

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

const previewBody = (over: Partial<Preview> = {}): Preview => ({
  impact: impact(),
  rows: [],
  rows_truncated: false,
  conflicts: [],
  ...over,
})

/**
 * A whole `DeriveReport`, the same fixture shape `RemoveRoot.test.tsx` and
 * `RootRow.test.tsx` already use — `{} as Derived` is enough only until a
 * component reads one of the twenty-nine fields, and this one now does.
 */
const derived = (over: Partial<config.Derived> = {}): config.Derived => ({
  series: 2,
  works: 71,
  editions: 71,
  members: 71,
  unattached_chapter_dirs: 0,
  unreadable: 0,
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
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
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

  it('draws the server’s sentence when a preview is refused, and stops saying it is working', async () => {
    // `busy` is what draws "Previewing…" — a catch that forgot to clear it
    // would leave that message on screen forever after the one failure a
    // person is most likely to hit here: a pattern the store's regex engine
    // refuses. Checked as an absence, not just a presence: the message alone
    // would still pass with the screen stuck saying it is working.
    vi.mocked(rulesApi.previewRules).mockRejectedValueOnce(
      new Error('/api/library/rules/preview: 400 pattern does not compile'),
    )
    const host = await render()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'pattern does not compile',
    )
    expect(host.querySelector('.busy')).toBeNull()
    // I2: a failed preview must not leave Save actionable — there is no
    // preview at all here (this is the very first one, rejected), so Save
    // has no number of its own to state and must stay disabled.
    expect(host.querySelector<HTMLButtonElement>('button.save')?.disabled).toBe(
      true,
    )
  })

  it('invalidates the standing preview when a later one fails, so Save cannot act on a number that describes a different list', async () => {
    // I2's own discriminating case: the *first* preview (on mount) succeeds,
    // so `preview`/`predicted.current` hold a real, non-empty value before
    // the failure this test causes — unlike the test above, where there was
    // never a successful preview to leave stale in the first place.
    const host = await render()
    expect(host.querySelector<HTMLButtonElement>('button.save')?.disabled).toBe(
      false,
    )
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
    const button = host.querySelector<HTMLButtonElement>('button.save')
    expect(button?.disabled).toBe(true)
    expect(button?.textContent).not.toContain('71')
  })

  it('leaves Save clickable again after a refusal, rather than stuck on "Saving…"', async () => {
    // `saving` is what disables the button and swaps its label — a catch that
    // forgot to clear it would leave a refused save looking like one still in
    // flight, with no way to try again short of a reload.
    vi.mocked(rulesApi.saveRules).mockRejectedValue(
      new Error(
        '/api/library/rules: 400 rule 9999: no such rule exists to keep',
      ),
    )
    const host = await render()
    await clickText(host, 'Save')
    const button = host.querySelector<HTMLButtonElement>('button.save')
    expect(button?.disabled).toBe(false)
    expect(button?.textContent).not.toContain('Saving')
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

  it('disables Save while a preview is in flight, and re-enables it once the preview lands', async () => {
    // Save compares the answer it gets back against `predicted.current` — the
    // *last landed* preview's impact. A click that lands while a preview is
    // still in flight would send the just-committed rules against a stale
    // impact and draw the "something changed the library" warning for a race
    // the client caused, not the server. This file's own top comment.
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
    expect(host.querySelector<HTMLButtonElement>('button.save')?.disabled).toBe(
      true,
    )

    await act(async () => {
      resolvePreview(previewBody())
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector<HTMLButtonElement>('button.save')?.disabled).toBe(
      false,
    )
  })

  it('re-previews the saved list once a save lands, and disables Save until that lands too', async () => {
    // I1's own discriminating case. Without `save` re-previewing on landing,
    // this second `previewRules` mock is never consumed at all — the button
    // stays enabled the instant `saveRules` resolves and keeps stating '71',
    // the pre-save number, forever after the library has already been
    // re-homed to match it.
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
    // stay disabled rather than re-enable against the number that preview is
    // about to replace.
    expect(host.querySelector<HTMLButtonElement>('button.save')?.disabled).toBe(
      true,
    )
    await act(async () => {
      resolveFollowUp(
        previewBody({ impact: impact({ works_rehomed: 0, series_merged: 0 }) }),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    const settled = host.querySelector<HTMLButtonElement>('button.save')
    expect(settled?.disabled).toBe(false)
    expect(settled?.textContent).toContain('0')
    expect(settled?.textContent).not.toContain('71')
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
    // Let the save's own follow-up preview (I1) land and re-enable Save
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
    // `doImport` fires its own `runPreview` right after landing, and that
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

  it('reorders from the Conflicts tab’s own button, the same as a drag would', async () => {
    // `PreviewTabs` carries its own `onReorder`, wired separately from
    // `RuleList`'s drag-and-drop — this is the only path that exercises it.
    vi.mocked(rulesApi.previewRules).mockResolvedValue(
      previewBody({
        conflicts: [
          {
            field: 'series',
            rel_path: 'a/b',
            first: 0,
            wins: 1,
            others: 0,
            examples: 3,
          },
        ],
      }),
    )
    const host = await render()
    await clickText(host, 'Conflicts')
    vi.mocked(rulesApi.previewRules).mockClear()
    await clickText(host, 'Move rule 2 above rule 1')
    const sent = vi.mocked(rulesApi.previewRules).mock.calls[0][0]
    expect(sent.map((r) => r.pattern)).toEqual(['Colour/.*', 'Manga/.*'])
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
