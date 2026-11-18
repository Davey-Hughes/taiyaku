import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import { cdp } from 'vitest/browser'
import { AccountPage } from '../src/AccountPage'
import * as config from '../src/config'
import * as curation from '../src/curation'
import { HeldButton } from '../src/HeldButton'
import { WAIT_DRAWN_AFTER_MS } from '../src/hooks'
// **Required**: the rules under test reach the app through `index.css`.
import '../src/index.css'
import { RuleEditor } from '../src/RuleEditor'
import * as rulesApi from '../src/rules'
import { holdOf } from './support/held'
import { typeInto } from './support/input'
import { mount } from './support/mount'
import * as usersApi from '../src/users'

vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  patchUser: vi.fn(),
  removalCost: vi.fn(),
  removeUser: vi.fn(),
}))
vi.mock('../src/rules', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/rules')>()),
  listRules: vi.fn(),
  previewRules: vi.fn(),
  saveRules: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  listRoots: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))

/**
 * O201: a held button is drawn unlike a live one, by `base.css`'s own rule,
 * and so is one a class rule colours: `.confirm` sets a border colour and a
 * text colour, and neither is what says "held". The word keeps its colour.
 */
describe('a held button', () => {
  it('is drawn on the page’s ground with a dashed border, its word unchanged', () => {
    const host = mount(
      <>
        <div className="library">
          <button type="button" name="live">
            Live
          </button>
          <button type="button" name="held" disabled>
            Held
          </button>
          <div className="remove-library">
            <button type="button" className="confirm" name="live-confirm">
              Remove
            </button>
            <button
              type="button"
              className="confirm"
              name="held-confirm"
              disabled
            >
              Remove
            </button>
          </div>
        </div>
        <div className="login">
          <button type="button" name="login-live">
            Live
          </button>
          <button type="button" name="login-held" disabled>
            Held
          </button>
        </div>
      </>,
    )
    const style = (name: string) =>
      getComputedStyle(host.querySelector(`button[name="${name}"]`) as Element)
    const page = getComputedStyle(document.body).backgroundColor

    for (const [live, held] of [
      ['live', 'held'],
      ['live-confirm', 'held-confirm'],
      ['login-live', 'login-held'],
    ]) {
      expect(style(live).borderTopStyle).toBe('solid')
      expect(style(held).borderTopStyle).toBe('dashed')
      expect(style(held).backgroundColor).toBe(page)
      expect(style(held).backgroundColor).not.toBe(style(live).backgroundColor)
      expect(style(held).color).toBe(style(live).color)
    }
  })
})

/**
 * A control held by `aria-disabled`, so that it keeps focus, is drawn as one
 * held by `disabled`: each kind beside its `disabled` twin, on the page, in a
 * removal's block and in Move…'s dialog, whose rules give a held button the
 * dialog's own ground. Under forced colours the browser draws a disabled
 * control's words and edge in `GrayText`, which a held one has from
 * `base.css`, a confirm's class colour notwithstanding.
 */
describe('a control held by aria-disabled', () => {
  const LOOK = [
    'border-top-style',
    'border-top-color',
    'color',
    'background-color',
    'opacity',
    'cursor',
  ]
  const look = (el: Element) => {
    const style = getComputedStyle(el)
    return Object.fromEntries(LOOK.map((p) => [p, style.getPropertyValue(p)]))
  }

  async function pairs() {
    const host = await act(async () =>
      mount(
        <div className="library">
          <div data-pair="button">
            <button type="button" disabled>
              Scan library
            </button>
            <HeldButton type="button" held>
              Scan library
            </HeldButton>
          </div>
          <div className="remove-library" data-pair="removal confirm">
            <button type="button" className="confirm keeps-wait-width" disabled>
              Remove Manga
            </button>
            <HeldButton type="button" className="confirm keeps-wait-width" held>
              Remove Manga
            </HeldButton>
          </div>
          <dialog open className="move-root settings-dialog">
            <div data-pair="dialog select">
              <select defaultValue="1" disabled>
                <option value="1">Manga</option>
              </select>
              <select defaultValue="1" aria-disabled>
                <option value="1">Manga</option>
              </select>
            </div>
            <div data-pair="dialog text box">
              <input type="text" defaultValue="Comics" disabled />
              <input type="text" defaultValue="Comics" readOnly aria-disabled />
            </div>
            <div data-pair="dialog button">
              <button type="button" disabled>
                Cancel
              </button>
              <HeldButton type="button" held>
                Cancel
              </HeldButton>
            </div>
            <div className="settings-actions" data-pair="dialog confirm">
              <button
                type="button"
                className="confirm keeps-wait-width"
                disabled
              >
                Move
              </button>
              <HeldButton
                type="button"
                className="confirm keeps-wait-width"
                held
              >
                Move
              </HeldButton>
            </div>
          </dialog>
        </div>,
      ),
    )
    return [...host.querySelectorAll<HTMLElement>('[data-pair]')].map(
      (pair) => {
        const [disabled, held] = [...pair.children]
        return { kind: pair.dataset.pair as string, disabled, held }
      },
    )
  }

  const expectDrawnAlike = async (when: string) => {
    const all = await pairs()
    expect(all).toHaveLength(6)
    for (const { kind, disabled, held } of all) {
      expect(holdOf(disabled), kind).toBe('disabled')
      expect(holdOf(held), kind).toBe('held')
      expect.soft(look(held), `${kind}, ${when}`).toEqual(look(disabled))
    }
    return all
  }

  /**
   * Every control a write holds on an account's page, drawn held on the page
   * itself: a removal is out, with the question that lifts the limit open
   * and a password typed twice. Each is paired with a copy of itself put beside
   * it and held by `disabled`, so each kind is read in its own place.
   */
  async function accountPairs() {
    const davey: usersApi.User = {
      id: 1,
      name: 'davey',
      role: 'admin',
      can_download: true,
      can_stream: true,
      can_stamp: true,
      can_log_in: true,
      wrong_passwords: 0,
      wrong_password_alarm: false,
      sessions: [],
      keys: [],
      every_library: true,
      libraries: [],
    }
    const sam: usersApi.User = {
      ...davey,
      id: 2,
      name: 'sam',
      role: 'user',
      sessions: [
        {
          id: 'aaa',
          created: '2026-09-01 10:00:00',
          last_seen: '2026-09-06 09:00:00',
          user_agent: 'Firefox',
          ip: '10.0.0.2',
          current: false,
        },
      ],
      keys: [
        {
          id: 9,
          name: 'the tablet',
          created: '2026-08-01 10:00:00',
          last_used: null,
        },
      ],
      every_library: false,
      libraries: [],
    }
    vi.mocked(usersApi.removalCost).mockResolvedValue({
      removed: {
        name: 'sam',
        positions: 0,
        notes: 0,
        sessions: 1,
        keys: 1,
        grants: 0,
      },
      dry_run: true,
    })
    vi.mocked(usersApi.removeUser).mockReturnValue(new Promise(() => {}))
    const host = await act(async () =>
      mount(
        <div className="library">
          <MemoryRouter>
            <AccountPage
              user={sam}
              users={[davey, sam]}
              libraries={[
                {
                  id: 70,
                  name: 'Manga',
                  works: 0,
                  books: 0,
                  match_by_content: true,
                  dirs: [],
                },
              ]}
              actingAs={1}
              onChanged={() => undefined}
            />
          </MemoryRouter>
        </div>,
      ),
    )
    const at = <T extends HTMLElement>(selector: string) =>
      host.querySelector(selector) as T
    await act(async () => {
      at('input[value="every"]').click()
    })
    await typeInto(at<HTMLInputElement>('input[name="password"]'), 'hunter22')
    await typeInto(
      at<HTMLInputElement>('input[name="confirm-password"]'),
      'hunter22',
    )
    await act(async () => {
      at('button[name="remove"]').click()
    })
    await act(async () => {
      at('button[name="confirm-remove"]').click()
      await new Promise((r) => setTimeout(r, WAIT_DRAWN_AFTER_MS + 100))
    })
    ;(document.activeElement as HTMLElement | null)?.blur()
    const pairs = [
      'select[name="role"]',
      'input[name="password"]',
      'input[name="confirm-password"]',
      'button[name="reset"]',
      'button[name="lift-limit"]',
      'button[name="keep-limit"]',
      'button[name="revoke-session-aaa"]',
      'button[name="revoke-key-9"]',
      'button[name="remove"]',
      'button[name="confirm-remove"]',
      'button[name="cancel-remove"]',
    ].map((kind) => {
      const held = at(kind)
      const disabled = held.cloneNode(true) as HTMLButtonElement
      disabled.removeAttribute('aria-disabled')
      disabled.removeAttribute('readonly')
      disabled.disabled = true
      held.after(disabled)
      return { kind, disabled, held }
    })
    onTestFinished(() => {
      for (const { disabled } of pairs) disabled.remove()
    })
    return pairs
  }

  const expectAccountDrawnAlike = async (when: string) => {
    const all = await accountPairs()
    for (const { kind, disabled, held } of all) {
      expect(holdOf(disabled), kind).toBe('disabled')
      expect(holdOf(held), kind).toBe('held')
      expect.soft(look(held), `${kind}, ${when}`).toEqual(look(disabled))
    }
    return all
  }

  /**
   * Every button a save holds on the Rules screen, drawn held on the screen
   * itself: a save is out, with the Overrides tab open on a row and the
   * confirm of a delete that reopens a proposal open. Each is paired with a
   * copy of itself put beside it and held by `disabled`. The list's enable
   * box is not among them: held by `aria-disabled`, a box draws nothing
   * (O159, `base.css`).
   */
  async function rulesPairs() {
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
    vi.mocked(rulesApi.listRules).mockResolvedValue({
      rules: [rule, { ...rule, id: 2, ordinal: 2, pattern: 'Colour/.*' }],
      complaints: [],
    })
    vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
    vi.mocked(curation.listProposals).mockResolvedValue({
      proposals: [
        {
          id: 42,
          kind: 'same-series',
          subject_id: 1,
          target_id: 2,
          score: 0.9,
          reason: 'two spellings of one title',
          state: 'accepted',
          created_rule_id: 2,
          subject_title: 'Death Note',
          target_title: 'Death Note (Colour)',
        },
      ],
      counts: {},
    })
    vi.mocked(rulesApi.previewRules).mockResolvedValue({
      impact: {
        series_before: 8,
        series_after: 2,
        series_merged: 6,
        works_rehomed: 71,
        works_orphaned: 0,
        proposals_reopened: 0,
        proposals_raised: 0,
      },
      rows: [],
      rows_truncated: false,
      overrides: [
        {
          field: 'series',
          rel_path: 'a/b',
          first: 0,
          wins: 1,
          first_value: 'A',
          wins_value: 'B',
          others: 0,
          examples: 3,
        },
      ],
    })
    vi.mocked(rulesApi.saveRules).mockReturnValue(new Promise(() => {}))
    const host = await act(async () =>
      mount(
        <div className="library">
          <main>
            <div className="config">
              <RuleEditor />
            </div>
          </main>
        </div>,
      ),
    )
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50))
    })
    const at = (selector: string) =>
      host.querySelector(selector) as HTMLButtonElement
    await act(async () => {
      ;[...host.querySelectorAll<HTMLElement>('.preview-tabs button')]
        .find((b) => b.textContent?.startsWith('Overrides'))
        ?.click()
    })
    await act(async () => {
      host.querySelectorAll<HTMLElement>('.rule-row button.delete')[1].click()
    })
    await act(async () => {
      at('button.save').click()
      await new Promise((r) => setTimeout(r, WAIT_DRAWN_AFTER_MS + 100))
    })
    ;(document.activeElement as HTMLElement | null)?.blur()
    const pairs = [
      'button.save',
      '.import button',
      '.rule-row button.select',
      '.rule-row button.delete',
      'button.confirm-delete',
      'button.add-rule',
      'button.reorder',
      'button.edit-winner',
    ].map((kind) => {
      const held = at(kind)
      const disabled = held.cloneNode(true) as HTMLButtonElement
      disabled.removeAttribute('aria-disabled')
      disabled.disabled = true
      held.after(disabled)
      return { kind, disabled, held }
    })
    onTestFinished(() => {
      for (const { disabled } of pairs) disabled.remove()
    })
    return pairs
  }

  const expectRulesDrawnAlike = async (when: string) => {
    const all = await rulesPairs()
    for (const { kind, disabled, held } of all) {
      expect(holdOf(disabled), kind).toBe('disabled')
      expect(holdOf(held), kind).toBe('held')
      expect.soft(look(held), `${kind}, ${when}`).toEqual(look(disabled))
    }
    return all
  }

  for (const theme of ['ink', 'paper']) {
    it(`is drawn as one held by disabled on the Rules screen, in ${theme}`, async () => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      await expectRulesDrawnAlike(theme)
    })
  }

  it('is drawn as one held by disabled on the Rules screen under forced colours, its words and edge in GrayText', async () => {
    const force = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'forced-colors', value }],
      })
    onTestFinished(async () => {
      await force('')
    })
    await force('active')
    expect(matchMedia('(forced-colors: active)').matches).toBe(true)
    const all = await expectRulesDrawnAlike('under forced colours')
    const probe = document.createElement('div')
    probe.style.color = 'GrayText'
    document.body.append(probe)
    const gray = getComputedStyle(probe).color
    probe.remove()
    for (const { kind, held } of all) {
      const { color, borderTopColor } = getComputedStyle(held)
      expect.soft([color, borderTopColor], kind).toEqual([gray, gray])
    }
  })

  for (const theme of ['ink', 'paper']) {
    it(`is drawn as one held by disabled on an account’s page, in ${theme}`, async () => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      await expectAccountDrawnAlike(theme)
    })
  }

  for (const theme of ['ink', 'paper']) {
    it(`is drawn as one held by disabled, in ${theme}`, async () => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      await expectDrawnAlike(theme)
    })
  }

  // Set over the DevTools session, as a system set to them has the browser
  // draw, and cleared after: the setting is the page's, not this frame's.
  it('is drawn as one held by disabled under forced colours, its words and edge in GrayText', async () => {
    const force = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'forced-colors', value }],
      })
    onTestFinished(async () => {
      await force('')
    })
    await force('active')
    // THE STATE THIS TEST NAMES: this page is told the system forces colours.
    expect(matchMedia('(forced-colors: active)').matches).toBe(true)
    const all = await expectDrawnAlike('under forced colours')
    const probe = document.createElement('div')
    probe.style.color = 'GrayText'
    document.body.append(probe)
    const gray = getComputedStyle(probe).color
    probe.remove()
    for (const { kind, held } of all) {
      expect.soft(getComputedStyle(held).color, kind).toBe(gray)
    }
  })

  it('is drawn as one held by disabled on an account’s page under forced colours, its words and edge in GrayText', async () => {
    const force = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'forced-colors', value }],
      })
    onTestFinished(async () => {
      await force('')
    })
    await force('active')
    expect(matchMedia('(forced-colors: active)').matches).toBe(true)
    const all = await expectAccountDrawnAlike('under forced colours')
    const probe = document.createElement('div')
    probe.style.color = 'GrayText'
    document.body.append(probe)
    const gray = getComputedStyle(probe).color
    probe.remove()
    for (const { kind, held } of all) {
      const { color, borderTopColor } = getComputedStyle(held)
      expect.soft([color, borderTopColor], kind).toEqual([gray, gray])
    }
  })
})
