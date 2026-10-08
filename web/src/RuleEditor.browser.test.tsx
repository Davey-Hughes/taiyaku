import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives: the rules
// under test live in `styles/admin.css`, which reaches the app through
// `index.css`.
import './index.css'
import * as config from './config'
import * as curation from './curation'
import { RuleEditor } from './RuleEditor'
import * as rulesApi from './rules'
import { emptyDerive } from './testing/derive'
import { press } from './testing/input'
import { mount } from './testing/mount'

/**
 * The report a save's re-derive draws on the Rules screen. `ScanReport`'s
 * section headings are styled by class for the Libraries screen, and this
 * screen draws the same report, so what it makes of that class is held here.
 */

vi.mock('./rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
}))
vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  listRoots: vi.fn(),
}))
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  listProposals: vi.fn(),
}))

const rule: rulesApi.Rule = {
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
}

const impact: rulesApi.Impact = {
  series_before: 8,
  series_after: 2,
  series_merged: 6,
  works_rehomed: 71,
  works_orphaned: 0,
  proposals_reopened: 0,
  proposals_raised: 0,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(rulesApi.listRules).mockResolvedValue({
    rules: [rule],
    complaints: [],
  })
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  })
  vi.mocked(rulesApi.previewRules).mockResolvedValue({
    impact,
    rows: [],
    rows_truncated: false,
    overrides: [],
  })
  vi.mocked(rulesApi.saveRules).mockResolvedValue({
    impact,
    rules: [rule],
    reopened_proposals: [],
    complaints: [],
    derive: { ...emptyDerive, lost_positions: 2 },
  })
})

describe('the Rules screen’s save report', () => {
  it.each([390, 1280])(
    'draws the report’s heading at the body size, as a label for what follows it, at %ipx',
    async (width) => {
      await page.viewport(width, 844)
      const host = await act(async () => {
        const mounted = mount(
          <div className="library">
            <main>
              <RuleEditor />
            </main>
          </div>,
        )
        await Promise.resolve()
        await Promise.resolve()
        return mounted
      })
      await press(host.querySelector('button.save') as Element)
      const details = host.querySelector(
        'details.save-derive',
      ) as HTMLDetailsElement
      details.open = true
      await document.fonts.ready

      const heading = details.querySelector('.report-heading') as HTMLElement
      expect(heading.textContent).toBe('What derivation made of it')
      const style = getComputedStyle(heading)
      // The body's size: never louder than the summary that opened it.
      expect(style.fontSize).toBe(
        getComputedStyle(details.querySelector('summary') as Element).fontSize,
      )
      expect(style.fontSize).toBe('16px')
      expect(style.fontWeight).toBe('600')
      // Its face is the screen's headings', not the body's.
      expect(style.fontFamily).toBe(
        getComputedStyle(host.querySelector('.rule-editor > h2') as Element)
          .fontFamily,
      )
      // And it stands clear of the summary above it and the list below.
      expect(
        heading.getBoundingClientRect().top -
          (
            details.querySelector('summary') as HTMLElement
          ).getBoundingClientRect().bottom,
      ).toBeGreaterThanOrEqual(16)
    },
  )
})
