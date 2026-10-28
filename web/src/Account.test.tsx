import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { About } from './About'
import { Account } from './Account'
import type { Me } from './api'
import { GateContext } from './gate'
import { ABOUT } from './testing/about'
import { deferred, flush } from './testing/async'
import { clipboardContext } from './testing/clipboard'
import { fields } from './testing/fields'
import { holdOf } from './testing/held'
import { mount, rerender } from './testing/mount'
import { expectRowsWired, helpOf, row, section } from './testing/settingsRows'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './testing/wait'
import { Users } from './Users'
import { VersionLabel } from './VersionLabel'

const noop = () => {}

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

/**
 * A `fetch` that answers each path from a table, so a test states what the
 * server holds rather than the order the component asks in — which is an
 * implementation detail these tests should not pin.
 */
const server = (routes: Record<string, () => Response>) =>
  vi.fn((input: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    const key = `${method} ${input}`
    const answer = routes[key] ?? routes[input]
    return Promise.resolve(
      answer ? answer() : new Response(null, { status: 404 }),
    )
  })

const settle = async () => {
  await act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** Type into an input the way a browser does, so React's state follows. */
const type = (input: HTMLInputElement, value: string) => {
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(input, value)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

const buttonSaying = (host: HTMLElement, text: string) =>
  [...host.querySelectorAll('button')].find((b) =>
    (b.textContent ?? '').includes(text),
  )

/** The page placement's screen, the `<section>` it is drawn in. */
const pageScreen = (host: HTMLElement): HTMLElement => {
  const screen = host.querySelector<HTMLElement>(
    'section.account[aria-label="Account"]',
  )
  if (!screen) throw new Error('no account page drawn')
  return screen
}

/** Each settings row's label words under `root`, in order. */
const rowLabels = (root: ParentNode) =>
  [...root.querySelectorAll('.settings-row')].map(
    (each) =>
      each.querySelector(':scope > .settings-row-text > .settings-row-label')
        ?.textContent,
  )

/** The screen in either placement, the panel open. */
const mountAs = (placement: 'page' | 'panel') =>
  mount(
    placement === 'page' ? (
      <Account placement="page" onSignedOut={noop} />
    ) : (
      <Account visible onClose={noop} onSignedOut={noop} />
    ),
  )

/**
 * Submit `form`, and let what it sent be answered, and the listing that
 * follows: a macrotask, which runs once every promise before it has settled.
 */
const submitted = async (form: Element | null) => {
  if (!form) throw new Error('no form to submit')
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })
}

const KEY_FORM = 'form:has(input[placeholder])'
const PASSWORD_FORM = 'form:has(input[type="password"])'

/** API keys' visually hidden polite region, which says a key was made. */
const keyRegion = (host: HTMLElement) => {
  const region = host.querySelector(`${KEY_FORM} > [aria-live="polite"]`)
  if (!region) throw new Error('no region saying a key was made')
  return region
}

/** The region that says what a press of the secret's box did, if drawn. */
const copyRegion = (host: HTMLElement) =>
  host.querySelector(`${KEY_FORM} > [role="status"]`)

/**
 * Password's visually hidden polite region, beside its form, which says a
 * change worked.
 */
const passwordRegion = (host: HTMLElement) => {
  const region = host.querySelector(`${PASSWORD_FORM} + [aria-live="polite"]`)
  if (!region) throw new Error('no region saying the password was changed')
  return region
}

const CAPTION = 'Copy this now — it is not stored and cannot be shown again.'

type Key = { id: number; name: string }

/**
 * A server whose keys are `before` until a create adds `made`, and which
 * revokes any of them; `revoked` holds each id it was asked to, in order.
 */
function listedKeys(before: Key[], made: Key[]) {
  const row = ({ id, name }: Key) => ({
    id,
    name,
    created: 'then',
    last_used: null,
  })
  let listed = before.map(row)
  const revoked: number[] = []
  const revokes = [...before, ...made].map(({ id }) => [
    `DELETE /api/keys/${id}`,
    () => {
      revoked.push(id)
      listed = listed.filter((key) => key.id !== id)
      return new Response(null, { status: 204 })
    },
  ])
  return {
    revoked,
    server: server({
      '/api/keys': () => json(listed),
      '/api/sessions': () => json([]),
      'POST /api/keys': () => {
        listed = [...listed, ...made.map(row)]
        return json({ secret: 'tyk_the-only-copy' })
      },
      ...Object.fromEntries(revokes),
    }),
  }
}

/**
 * A server that makes `phone` (id 1, secret `tyk_first`) on the first
 * create and `tablet` (id 2, `tyk_second`) on the second, and answers each
 * listing with the keys it holds when asked. `hold` keeps one request's
 * answer until the case settles it; `holdKeyRead` keeps the nth listing of
 * the keys, answering it with the keys it held when it was asked.
 */
function madeInTurn() {
  const MADE = [
    { id: 1, name: 'phone', secret: 'tyk_first' },
    { id: 2, name: 'tablet', secret: 'tyk_second' },
  ]
  let listed: { id: number; name: string; created: string; last_used: null }[] =
    []
  let creates = 0
  let keyReads = 0
  const revoked: number[] = []
  const held = new Map<string, Promise<Response>>()
  const heldReads = new Map<number, Promise<void>>()
  const fetch = vi.fn((input: string, init?: RequestInit) => {
    const asked = `${init?.method ?? 'GET'} ${input}`
    if (asked === 'POST /api/keys') {
      const made = MADE[creates]
      creates += 1
      if (!made) throw new Error('a third key was asked for')
      listed = [
        ...listed,
        { id: made.id, name: made.name, created: 'then', last_used: null },
      ]
      return Promise.resolve(json({ secret: made.secret }))
    }
    const deleted = /^DELETE \/api\/keys\/(\d+)$/.exec(asked)
    if (deleted) {
      const id = Number(deleted[1])
      const answer = held.get(asked) ?? Promise.resolve(new Response(null))
      return answer.then((response) => {
        revoked.push(id)
        listed = listed.filter((key) => key.id !== id)
        return response
      })
    }
    if (asked === 'GET /api/keys') {
      keyReads += 1
      const now = json(listed)
      return (heldReads.get(keyReads) ?? Promise.resolve()).then(() => now)
    }
    if (asked === 'GET /api/sessions') return Promise.resolve(json([]))
    return Promise.resolve(new Response(null, { status: 404 }))
  })
  return {
    fetch,
    revoked,
    hold: (asked: string, answer: Promise<Response>) => {
      held.set(asked, answer)
    },
    holdKeyRead: (nth: number, until: Promise<void>) => {
      heldReads.set(nth, until)
    },
  }
}

/**
 * A server that changes the password, signing out every session but this
 * one, and answers each listing of the sessions with the ones it holds when
 * asked. `holdChange` keeps the change's answer until the case settles it;
 * `holdSessionRead` keeps the nth listing of the sessions.
 */
function signsOthersOut() {
  const session = (id: string, agent: string, current: boolean) => ({
    id,
    created: 'then',
    last_seen: 'today',
    user_agent: agent,
    ip: null,
    current,
  })
  let listed = [session('aa', 'Firefox', true), session('bb', 'Safari', false)]
  let changes = 0
  let sessionReads = 0
  let heldChange: Promise<Response> | undefined
  const heldReads = new Map<number, Promise<void>>()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      if (asked === 'PUT /api/password') {
        changes += 1
        const answer =
          heldChange ?? Promise.resolve(new Response(null, { status: 204 }))
        return answer.then((response) => {
          if (response.ok) listed = listed.filter((each) => each.current)
          return response
        })
      }
      if (asked === 'GET /api/sessions') {
        sessionReads += 1
        const now = json(listed)
        return (heldReads.get(sessionReads) ?? Promise.resolve()).then(
          () => now,
        )
      }
      if (asked === 'GET /api/keys') return Promise.resolve(json([]))
      return Promise.resolve(new Response(null, { status: 404 }))
    }),
  )
  return {
    changes: () => changes,
    holdChange: (answer: Promise<Response>) => {
      heldChange = answer
    },
    holdSessionRead: (nth: number, until: Promise<void>) => {
      heldReads.set(nth, until)
    },
  }
}

/** The alert above the sections, if drawn. */
const alertIn = (host: HTMLElement) =>
  host.querySelector('[role="alert"]')?.textContent

/**
 * Whether a list holds a row for `name`, ending in its Revoke. Not the
 * screen's words: the OPDS line names Panels and Chunky, and the hidden
 * line a create says names its key.
 */
const rowListed = (host: HTMLElement, name: string) =>
  [...host.querySelectorAll('li, .reader-section-head')].some(
    (row) =>
      (row.textContent ?? '').includes(name) &&
      row.querySelector('button')?.textContent === 'Revoke',
  )

/** Whether Signed in lists a session from Safari, which a change signs out. */
const listsSafari = (host: HTMLElement) =>
  [...host.querySelectorAll('li, .reader-section-head')].some((row) =>
    (row.textContent ?? '').includes('Safari'),
  )

/** Press Revoke on the key named `name`, and let what follows land. */
async function revoke(host: HTMLElement, name: string) {
  const button = [...host.querySelectorAll('li, .reader-section-head')]
    .filter((row) => row.querySelector('button')?.textContent === 'Revoke')
    .find((row) => (row.textContent ?? '').includes(name))
    ?.querySelector('button')
  if (!button) throw new Error(`no Revoke on ${name}`)
  await act(async () => {
    button.click()
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })
}
/** What the server answers a Create sent with no name (`session.rs`). */
const NEEDS_A_NAME = 'a key needs a name, so the right one can be revoked later'
const KEY_MADE =
  'Key made: phone. Copy it now — it is not stored and cannot be shown again.'
const KEY_MADE_UNNAMED =
  'Key made. Copy it now — it is not stored and cannot be shown again.'
const PASSWORD_CHANGED =
  'Password changed. Every other signed-in browser has been signed out; API keys still work.'

describe('Account', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders nothing when not visible, and asks for nothing', () => {
    const fetchMock = server({})
    vi.stubGlobal('fetch', fetchMock)
    const host = mount(
      <Account visible={false} onClose={noop} onSignedOut={noop} />,
    )
    expect(host.firstElementChild).toBeNull()
    expect(fetchMock).not.toHaveBeenCalled()
  })

  /**
   * **The same screen at `/account`, drawn as a page** — O94. Inside `Shell`'s
   * `<main>` there is no reader under it to cover and nothing to close: the
   * sidebar beside it is how a person leaves. And the panel's look does not
   * come with it. `.library` follows the viewer's colour scheme, while the
   * panel's classes are the reader's and resolve dark whatever the theme
   * says. The check runs after a change, with what the change drew on
   * screen.
   *
   * **The sweep is for any `style` attribute, not only one naming a colour**:
   * the page is drawn by classes alone, the settings components' and
   * `admin.css`'s, so an inline style of any kind is one no stylesheet
   * answers for.
   *
   * **The page's shape is the settings pages'**: a header, then a section
   * per part; nothing a form says is drawn between them.
   */
  it('draws as a page with no overlay, no Close and no inline style of its own', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () =>
          json([{ id: 1, name: 'Panels', created: 'then', last_used: null }]),
        '/api/sessions': () => json([]),
        'PUT /api/password': () => new Response(null, { status: 204 }),
      }),
    )
    const host = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()

    const screen = host.firstElementChild
    if (!(screen instanceof HTMLElement)) throw new Error('nothing drawn')
    expect(screen.tagName).toBe('SECTION')
    expect(screen.getAttribute('aria-label')).toBe('Account')
    expect(
      [...screen.children].map((el) => [el.tagName, el.className]),
    ).toEqual([
      ['DIV', 'page-header'],
      ['SECTION', 'settings-section'],
      ['SECTION', 'settings-section'],
      ['SECTION', 'settings-section'],
    ])
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(buttonSaying(host, 'Close')).toBeUndefined()
    // The key list loaded: its one row's Revoke is the only one drawn.
    expect(buttonSaying(host, 'Revoke')).toBeDefined()

    const current = host.querySelector<HTMLInputElement>(
      'input[autocomplete="current-password"]',
    )
    const [next, confirm] = host.querySelectorAll<HTMLInputElement>(
      'input[autocomplete="new-password"]',
    )
    if (!current || !next || !confirm) {
      throw new Error('missing password fields')
    }
    type(current, 'old')
    type(next, 'new')
    type(confirm, 'new')
    await act(async () => {
      current.form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    expect(passwordRegion(screen).textContent).toBe(PASSWORD_CHANGED)

    // Read off the `style` attribute React wrote, element by element.
    const painted = [...screen.querySelectorAll('[style]')].map(
      (el) => `${el.tagName}: ${el.getAttribute('style')}`,
    )
    expect(painted).toEqual([])
  })

  /**
   * **`class="secret secret"`, which is what the page rendered.** The call
   * site writes the literal `secret` for both placements and appends
   * `look.secret` after it; `LOOKS.page` set that to `'secret'` as well, so
   * the one box on this screen drawn from a template literal carried the
   * same class twice. Collapsing the literal instead is the fix that looks
   * right and is not: `.secret` is what `admin.css` paints the page's copy
   * with *and* half of what the panel's box carries, so the panel would lose
   * it — which is what the panel's own class assertion below states.
   *
   * A key has to be minted for the box to exist at all, so this cannot ride
   * along on the page case above: one `mount` per `it()`.
   */
  it('draws the page’s secret box with one class, not the same one twice', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
      }),
    )
    const host = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()

    const create = host.querySelector('form:has(input[placeholder])')
    await act(async () => {
      create?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()

    const secret = host.querySelector('.secret')
    expect(secret?.textContent).toContain('tyk_the-only-copy')
    expect(secret?.className).toBe('secret')
    // `look.meta` on its caption, which buys the quiet text `.why` draws.
    expect(secret?.querySelector('.secret-caption')?.className).toBe(
      'copy-words secret-caption why',
    )
    // In the API keys box, straight under the form that made it.
    const keys = section(pageScreen(host), 'API keys')
    expect(secret?.parentElement).toBe(
      keys.querySelector(':scope > .settings-box'),
    )
    expect(secret?.previousElementSibling).toBe(create)
  })

  /**
   * **Every part of the page, drawn by the settings components or by
   * `LOOKS.page`.** Each section is a `Section` at h2, its heading outside
   * its box; each setting, key and session is a `Row`, its facts as the
   * row's help, where `settings.css` draws them quiet; each verb sits where
   * the settings pages put it, and takes no class, since `base.css` draws
   * every button there. What the components do not draw — the failure
   * above the sections — keeps `LOOKS.page`'s class, which nothing else in
   * this file names for the page. Per element, so that a fact drawn in the
   * wrong section, or in no row, fails here.
   */
  it('draws every part of the page with the settings components, and its alerts with admin.css’s classes', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () =>
          json([{ id: 7, name: 'phone', created: 'then', last_used: null }]),
        '/api/sessions': () =>
          json([
            {
              id: 'aa',
              created: 'then',
              last_seen: 'today',
              user_agent: 'Firefox',
              ip: null,
              current: true,
            },
          ]),
        'PUT /api/password': () => new Response(null, { status: 204 }),
        'DELETE /api/keys/7': () => new Response(null, { status: 500 }),
      }),
    )
    const host = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()
    const screen = pageScreen(host)

    // A `Section` per part, at h2 under the page's h1.
    expect(
      [...screen.querySelectorAll(':scope > section')].map((el) => [
        el.className,
        el.querySelector(
          ':scope > .settings-section-head > h2.settings-section-title',
        )?.textContent,
      ]),
    ).toEqual([
      ['settings-section', 'Password'],
      ['settings-section', 'API keys'],
      ['settings-section', 'Signed in'],
    ])
    const password = section(screen, 'Password')
    const keys = section(screen, 'API keys')
    const signedIn = section(screen, 'Signed in')

    // Each section's rows, by their labels.
    expect(rowLabels(password)).toEqual([
      'Current password',
      'New password',
      'Confirm password',
    ])
    expect(rowLabels(keys)).toEqual(['New key', 'phone'])
    expect(rowLabels(signedIn)).toEqual(['Firefox (this one)'])

    // A key's and a session's facts are their row's help, and each row ends
    // in its Revoke.
    const key = row(keys, 'phone')
    expect(helpOf(key)?.textContent).toBe('never used')
    expect(
      key.querySelector(':scope > .settings-row-control > button')?.textContent,
    ).toBe('Revoke')
    const session = row(signedIn, 'Firefox (this one)')
    expect(helpOf(session)?.textContent).toBe('last seen today')
    expect(
      session.querySelector(':scope > .settings-row-control > button')
        ?.textContent,
    ).toBe('Revoke')

    // Create beside the box it sends, in the New key row.
    expect(
      row(keys, 'New key').querySelector(
        ':scope > .settings-row-control > button',
      )?.textContent,
    ).toBe('Create')

    // Change password at the foot of its form, Sign out at the foot of its
    // box, each the last thing there.
    const foot = (root: ParentNode) =>
      root.querySelector(':scope > .settings-actions:last-child > button')
    expect(
      foot(password.querySelector(':scope > .settings-box > form') ?? password)
        ?.textContent,
    ).toBe('Change password')
    expect(
      foot(signedIn.querySelector(':scope > .settings-box') ?? signedIn)
        ?.textContent,
    ).toBe('Sign out')

    // No button on the page carries a class of its own.
    expect(
      [...screen.querySelectorAll('button')]
        .filter((b) => b.className !== '')
        .map((b) => `${b.textContent}: ${b.className}`),
    ).toEqual([])

    // The confirmation a password change leaves behind is said and not
    // drawn, from Password's visually hidden region, which takes no class
    // of `LOOKS`.
    await submitted(host.querySelector(PASSWORD_FORM))
    const region = passwordRegion(host)
    expect([region.textContent, region.className]).toEqual([
      PASSWORD_CHANGED,
      'visually-hidden',
    ])

    // `look.failure`, on a revocation the server refuses.
    const revoke = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Revoke' && !b.disabled,
    )
    await act(async () => {
      revoke?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.className).toBe(
      'account-failure',
    )
  })

  /**
   * **Every key in `LOOKS.panel`, pinned where the browser file cannot reach
   * it.** `Account.browser.test.tsx` measures the panel's own box — corners,
   * shadow, width, body size, the scrim — and sweeps the rendered tree for
   * inline style. What it does not distinguish is *which* class carries a
   * part: an `<input>` reads 14px from `.reader-dialog-panel :where(input,
   * select) { font: inherit }` whether or not `look.field` is on it, so
   * dropping `field` from the map entirely leaves that file green. And the
   * secret and the failure are drawn only after an action, so no fixture
   * that merely mounts the panel ever sees them.
   *
   * A class name is worth asserting here because that file proves the same
   * classes resolve, inside `.reader`, to the box ③ draws. Neither half
   * stands alone.
   */
  it('paints every part of the panel with the reader’s classes', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () =>
          json([{ id: 7, name: 'phone', created: 'then', last_used: null }]),
        '/api/sessions': () =>
          json([
            {
              id: 'aa',
              created: 'then',
              last_seen: 'today',
              user_agent: 'Firefox',
              ip: null,
              current: true,
            },
          ]),
        'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        'PUT /api/password': () => new Response(null, { status: 204 }),
        'DELETE /api/keys/7': () => new Response(null, { status: 500 }),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    // The frame, which the panel branch names literally rather than through
    // `look`, because that branch *is* the panel.
    expect(host.firstElementChild?.className).toBe('reader-dialog')
    expect(host.querySelector('[role="dialog"]')?.className).toBe(
      'reader-dialog-panel',
    )
    expect(host.querySelector('[role="dialog"] > div')?.className).toBe(
      'reader-panel-head',
    )
    expect(host.querySelector('h2')?.className).toBe('reader-panel-title')
    expect(buttonSaying(host, 'Close')?.className).toBe('reader-button')

    // `look.section`, `look.heading`, one of each per section.
    expect([...host.querySelectorAll('section')].map((el) => el.className)) //
      .toEqual(['reader-section', 'reader-section', 'reader-section'])
    expect([...host.querySelectorAll('h3')].map((el) => el.className)).toEqual([
      'reader-panel-heading',
      'reader-panel-heading',
      'reader-panel-heading',
    ])

    // `look.field` on all four boxes, Confirm's as its other
    // fields', `look.wide`/`look.signOut` on the two section verbs,
    // `look.button` on the row-level ones.
    expect([...host.querySelectorAll('input')].map((el) => el.className)) //
      .toEqual([
        'reader-field reader-field-block',
        'reader-field reader-field-block',
        'reader-field reader-field-block',
        'reader-field reader-field-block',
      ])
    expect(buttonSaying(host, 'Change password')?.className).toBe(
      'reader-button reader-button-wide',
    )
    expect(buttonSaying(host, 'Sign out')?.className).toBe(
      'reader-button reader-button-wide',
    )
    expect(buttonSaying(host, 'Create')?.className).toBe('reader-button')
    expect(
      [...host.querySelectorAll('button')]
        .filter((b) => b.textContent === 'Revoke')
        .map((b) => b.className),
    ).toEqual(['reader-button', 'reader-button'])

    // `look.row`: the create form, the key row, the session row — the three
    // that carried `rowStyle`. Both class names, per row: the modifier is
    // what carries the 8 px that inline style had, and `.reader-section-head`
    // alone — ③'s row, which no panel of ③'s ever stacked — has no margin.
    expect(
      [...host.querySelectorAll('.reader-section-head')].map(
        (el) => el.className,
      ),
    ).toEqual([
      'reader-section-head reader-section-head-stacked',
      'reader-section-head reader-section-head-stacked',
      'reader-section-head reader-section-head-stacked',
    ])
    // `look.meta`: the OPDS blurb, and the small print beside each row.
    expect(host.querySelectorAll('span.reader-meta')).toHaveLength(2)
    expect(host.querySelectorAll('p.reader-meta')).toHaveLength(1)

    // The secret, drawn only after a key is minted. Both classes: `.secret`
    // is what `admin.css` paints the page's copy with, and dropping either
    // half of the template literal is a different panel.
    const create = host.querySelector('form:has(input[placeholder])')
    await act(async () => {
      create?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    const secret = host.querySelector('.secret')
    expect(secret?.textContent).toContain('tyk_the-only-copy')
    expect(secret?.className).toBe('secret reader-secret')
    // The fourth `look.meta` site, and the one the count above cannot see:
    // that count runs before this submit, when no secret is on screen, so
    // dropping `look.meta` from the caption turned nothing red.
    expect(secret?.querySelector('.secret-caption')?.className).toBe(
      'copy-words secret-caption reader-meta',
    )

    // The confirmation a password change leaves behind is said and not
    // drawn, from Password's visually hidden region, which takes no class
    // of `LOOKS`.
    await submitted(host.querySelector(PASSWORD_FORM))
    const region = passwordRegion(host)
    expect([region.textContent, region.className]).toEqual([
      PASSWORD_CHANGED,
      'visually-hidden',
    ])

    // `look.failure`, on a revocation the server refuses.
    const revoke = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Revoke' && !b.disabled,
    )
    await act(async () => {
      revoke?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.className).toBe(
      'reader-failure',
    )
  })

  it('lists the account’s keys and sessions', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () =>
          json([
            { id: 1, name: 'phone', created: 'then', last_used: null },
            { id: 2, name: 'laptop', created: 'then', last_used: 'today' },
          ]),
        '/api/sessions': () =>
          json([
            {
              id: 'aa',
              created: 'then',
              last_seen: 'today',
              user_agent: 'Firefox',
              ip: null,
              current: true,
            },
          ]),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    expect(host.textContent).toContain('phone')
    expect(host.textContent).toContain('never used')
    expect(host.textContent).toContain('laptop')
    expect(host.textContent).toContain('Firefox')
    expect(host.textContent).toContain('(this one)')
  })

  /**
   * **The one property the whole key design rests on.** Only the hash is
   * stored, so the secret cannot be answered a second time — and a screen that
   * showed it without saying so would leave a reader to discover that by losing
   * one.
   */
  it('shows a new key once and says it cannot be shown again', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const form = host.querySelector('form:has(input[placeholder])')
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()

    // Read off the box: the line the polite region says a key was made in
    // says "cannot be shown again" as well.
    const secret = host.querySelector('.secret')
    expect(secret?.textContent).toContain('tyk_the-only-copy')
    expect(secret?.textContent).toMatch(/cannot be shown again/)
  })

  /**
   * The session doing the asking cannot be revoked from this list: pressing it
   * would sign the reader out through a control labelled as if it affected some
   * other browser. Signing out is its own button, which says so.
   */
  it('will not let the current session revoke itself from the list', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () =>
          json([
            {
              id: 'aa',
              created: 'then',
              last_seen: 'today',
              user_agent: 'Firefox',
              ip: null,
              current: true,
            },
            {
              id: 'bb',
              created: 'then',
              last_seen: 'yesterday',
              user_agent: 'Safari',
              ip: null,
              current: false,
            },
          ]),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const revokes = [...host.querySelectorAll('button')].filter(
      (b) => b.textContent === 'Revoke',
    )
    expect(revokes).toHaveLength(2)
    expect(revokes[0]?.disabled).toBe(true)
    expect(revokes[1]?.disabled).toBe(false)
    expect(buttonSaying(host, 'Sign out')).toBeDefined()
  })

  it('reports upward after signing out', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'DELETE /api/session': () => new Response(null, { status: 204 }),
      }),
    )
    const onSignedOut = vi.fn()
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={onSignedOut} />,
    )
    await settle()

    await act(async () => {
      buttonSaying(host, 'Sign out')?.click()
      await Promise.resolve()
    })
    await settle()
    expect(onSignedOut).toHaveBeenCalledOnce()
  })

  /**
   * A wrong current password is a message rather than a silent no-op, and the
   * success line says what else the change did — every other browser signed
   * out, API keys untouched — because neither is visible from here.
   */
  it('says what a password change did, and what it did not', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'PUT /api/password': () => new Response(null, { status: 204 }),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const form = host.querySelector('form')
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()

    expect(host.textContent).toMatch(/signed out/)
    expect(host.textContent).toMatch(/API keys still work/)
  })

  // Both placements say the one confirmation, from Password's own visually
  // hidden region beside its form, as API keys says a key was made, so that
  // saying it moves nothing (the user's ruling of 2026-10-25). One `it()`
  // each: `testing/mount.ts` holds one root.
  for (const placement of ['page', 'panel'] as const) {
    it(`says a password change worked in a visually hidden polite region in Password, drawn empty before it, and from no region above the sections, as a ${placement}`, async () => {
      // A region drawn together with what it says is often not read at all.
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'PUT /api/password': () => new Response(null, { status: 204 }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const region = passwordRegion(host)
      expect(region.textContent).toBe('')
      expect(region.classList.contains('visually-hidden')).toBe(true)
      expect(
        region.closest('section')?.querySelector('h2, h3')?.textContent,
      ).toBe('Password')

      await submitted(host.querySelector(PASSWORD_FORM))
      expect(passwordRegion(host)).toBe(region)
      expect(region.textContent).toBe(PASSWORD_CHANGED)
      const screen = host.querySelector('section.account, [role="dialog"]')
      expect(screen?.querySelectorAll(':scope > [aria-live]')).toHaveLength(0)
    })
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`says a second password change again, in a node of its own, as a ${placement}`, async () => {
      // A live region speaks its text changing: the same words left in the
      // same node have not changed, and are not spoken again.
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'PUT /api/password': () => new Response(null, { status: 204 }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const region = passwordRegion(host)
      await submitted(host.querySelector(PASSWORD_FORM))
      const first = region.firstChild
      expect([first?.textContent, region.textContent]).toEqual([
        PASSWORD_CHANGED,
        PASSWORD_CHANGED,
      ])

      await submitted(host.querySelector(PASSWORD_FORM))
      expect(passwordRegion(host)).toBe(region)
      expect(region.textContent).toBe(PASSWORD_CHANGED)
      expect(region.firstChild).not.toBeNull()
      expect(region.firstChild).not.toBe(first)
    })
  }

  // The user's ruling of 2026-10-25: the sessions a change signed out leave
  // Signed in in the render that says it worked, so the page moves once.
  for (const placement of ['page', 'panel'] as const) {
    it(`says Password changed in the render that draws the Signed in list after the change, and holds Change password until then, as a ${placement}`, async () => {
      const sessions = signsOthersOut()
      const listing = deferred<void>()
      // The mount's listing is the first; the change's is the second.
      sessions.holdSessionRead(2, listing.promise)
      const host = mountAs(placement)
      await settle()
      const region = passwordRegion(host)
      expect(listsSafari(host)).toBe(true)

      await submitted(host.querySelector(PASSWORD_FORM))
      expect(sessions.changes()).toBe(1)
      expect([region.textContent, listsSafari(host)]).toEqual(['', true])
      await submitted(host.querySelector(PASSWORD_FORM))
      expect(sessions.changes()).toBe(1)

      await act(async () => {
        listing.resolve()
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })
      expect([region.textContent, listsSafari(host)]).toEqual([
        PASSWORD_CHANGED,
        false,
      ])
      await submitted(host.querySelector(PASSWORD_FORM))
      expect(sessions.changes()).toBe(2)
    })
  }

  // A change answered after the panel closed is not this panel's to answer:
  // the close emptied its boxes, and what is typed after it is not undone.
  // The sessions it signed out are true all the same, and the reopen's
  // listing was read before they were.
  it('does not say Password changed, nor empty what is typed, in a panel opened again while the change was out, and lists the sessions it signed out no longer', async () => {
    const sessions = signsOthersOut()
    const answer = deferred<Response>()
    sessions.holdChange(answer.promise)
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await submitted(host.querySelector(PASSWORD_FORM))
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(listsSafari(host)).toBe(true)
    const current = host.querySelector<HTMLInputElement>(
      'input[autocomplete="current-password"]',
    )
    if (!current) throw new Error('no Current box')
    type(current, 'typed since')

    await act(async () => {
      answer.resolve(new Response(null, { status: 204 }))
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    expect(sessions.changes()).toBe(1)
    expect([passwordRegion(host).textContent, current.value]).toEqual([
      '',
      'typed since',
    ])
    expect(listsSafari(host)).toBe(false)
  })

  it('does not say Password changed in a panel opened again after a close that came between the change’s answer and its listing', async () => {
    const sessions = signsOthersOut()
    const listing = deferred<void>()
    // The mount's listing is the first; the change's is the second.
    sessions.holdSessionRead(2, listing.promise)
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await submitted(host.querySelector(PASSWORD_FORM))
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    await act(async () => {
      listing.resolve()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(sessions.changes()).toBe(1)
    expect(passwordRegion(host).textContent).toBe('')
  })

  it('does not say a failed revocation of a key or a session, nor a failed sign-out, in a panel opened again after a close that came while they were out', async () => {
    const answer = deferred<void>()
    const session = (id: string, agent: string, current: boolean) => ({
      id,
      created: 'then',
      last_seen: 'today',
      user_agent: agent,
      ip: null,
      current,
    })
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const asked = `${init?.method ?? 'GET'} ${input}`
        if (asked.startsWith('DELETE ')) {
          return answer.promise.then(() => new Response(null, { status: 500 }))
        }
        if (asked === 'GET /api/keys') {
          return Promise.resolve(
            json([{ id: 1, name: 'phone', created: 'then', last_used: null }]),
          )
        }
        if (asked === 'GET /api/sessions') {
          return Promise.resolve(
            json([
              session('aa', 'Firefox', true),
              session('bb', 'Safari', false),
            ]),
          )
        }
        return Promise.resolve(new Response(null, { status: 404 }))
      }),
    )
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    const fetch = vi.mocked(globalThis.fetch)
    const revokeOn = (name: string) =>
      [...host.querySelectorAll('.reader-section-head')]
        .find((row) => (row.textContent ?? '').includes(name))
        ?.querySelector('button')
    const presses = [
      revokeOn('phone'),
      revokeOn('Safari'),
      buttonSaying(host, 'Sign out'),
    ]
    for (const button of presses) {
      if (!button) throw new Error('a button to press is missing')
      act(() => button.click())
    }
    expect(
      fetch.mock.calls
        .filter(([, init]) => init?.method === 'DELETE')
        .map(([path]) => path),
    ).toEqual(['/api/keys/1', '/api/sessions/bb', '/api/session'])

    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    await act(async () => {
      answer.resolve()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  // O228, and the user's rulings of 2026-10-25: a made key is said by its
  // name, never by its secret, in a visually hidden polite region in API
  // keys beside the secret's box, so the line moves nothing on the page;
  // Password's region says "Password changed" alone.
  for (const placement of ['page', 'panel'] as const) {
    it(`says a key was made, by its name and not its secret, in a polite region in API keys and not in Password’s, as a ${placement}`, async () => {
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const region = keyRegion(host)
      expect(region.textContent).toBe('')
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')

      await submitted(host.querySelector(KEY_FORM))
      // The screen holds the secret, in its box, and the region does not.
      expect(host.querySelector('.secret')?.textContent).toContain(
        'tyk_the-only-copy',
      )
      expect(keyRegion(host)).toBe(region)
      expect(region.classList.contains('visually-hidden')).toBe(true)
      expect(region.textContent).toBe(KEY_MADE)
      expect(region.textContent).not.toContain('tyk_the-only-copy')
      expect(passwordRegion(host).textContent).toBe('')
    })
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`names the key in its secret's caption, in the caption's own words, and the line saying it was made as before, as a ${placement}`, async () => {
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, ' tablet ')

      await submitted(host.querySelector(KEY_FORM))
      expect(host.querySelector('.secret-caption')?.textContent).toBe(
        'Key “tablet”: copy this now — it is not stored and cannot be shown again.',
      )
      expect(keyRegion(host).textContent).toBe(
        'Key made: tablet. Copy it now — it is not stored and cannot be shown again.',
      )
    })
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`says a key made with no name was made, naming none, as a ${placement}`, async () => {
      // `create_key` (`session.rs`) refuses a blank name, so this is a server
      // that lets one through, and the line still reads whole. A name of
      // spaces is none, as that check reads it.
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const region = keyRegion(host)
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')

      await submitted(host.querySelector(KEY_FORM))
      expect(region.textContent).toBe(KEY_MADE_UNNAMED)
      expect(host.querySelector('.secret-caption')?.textContent).toBe(CAPTION)

      type(keyName, '   ')
      await submitted(host.querySelector(KEY_FORM))
      expect(region.textContent).toBe(KEY_MADE_UNNAMED)
      expect(host.querySelector('.secret-caption')?.textContent).toBe(CAPTION)
    })
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`says a second key made with the same name again, in a node of its own, as a ${placement}`, async () => {
      // A live region speaks its text changing: the same words left in the
      // same node have not changed, and are not spoken again.
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const region = keyRegion(host)
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      const first = region.firstChild
      expect([first?.textContent, region.textContent]).toEqual([
        KEY_MADE,
        KEY_MADE,
      ])

      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect(keyRegion(host)).toBe(region)
      expect(region.textContent).toBe(KEY_MADE)
      expect(region.firstChild).not.toBeNull()
      expect(region.firstChild).not.toBe(first)
    })
  }

  it('takes a made key’s line away when the panel closes, as it takes Password changed', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        'PUT /api/password': () => new Response(null, { status: 204 }),
      }),
    )
    const host = mountAs('panel')
    await settle()
    // Closed, nothing is drawn, so each is read again after reopening.
    const reopen = async () => {
      rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
      await settle()
      rerender(<Account visible onClose={noop} onSignedOut={noop} />)
      await settle()
    }
    const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
    if (!keyName) throw new Error('no key name box')
    type(keyName, 'phone')
    await submitted(host.querySelector(KEY_FORM))
    expect(keyRegion(host).textContent).toBe(KEY_MADE)
    await reopen()
    expect(keyRegion(host).textContent).toBe('')

    await submitted(host.querySelector(PASSWORD_FORM))
    expect(passwordRegion(host).textContent).toBe(PASSWORD_CHANGED)
    await reopen()
    expect(passwordRegion(host).textContent).toBe('')
  })

  for (const placement of ['page', 'panel'] as const) {
    it(`keeps Password changed through a key made, and takes a key’s line away as the next key is asked for, so that none stands beside a refusal, as a ${placement}`, async () => {
      let creates = 0
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () =>
            ++creates === 1
              ? json({ secret: 'tyk_the-only-copy' })
              : new Response(NEEDS_A_NAME, { status: 400 }),
          'PUT /api/password': () => new Response(null, { status: 204 }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const note = passwordRegion(host)
      const region = keyRegion(host)
      const alert = () => host.querySelector('[role="alert"]')?.textContent
      await submitted(host.querySelector(PASSWORD_FORM))
      expect(note.textContent).toBe(PASSWORD_CHANGED)

      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect([note.textContent, region.textContent]).toEqual([
        PASSWORD_CHANGED,
        KEY_MADE,
      ])

      // A key the server refuses: the last key's line went as it was sent.
      await submitted(host.querySelector(KEY_FORM))
      expect([note.textContent, region.textContent, alert()]).toEqual([
        PASSWORD_CHANGED,
        '',
        NEEDS_A_NAME,
      ])
    })
  }

  // The user's ruling of 2026-10-25: a revoked key's secret no longer works,
  // so its box goes; another key's revocation leaves a key that still does.
  for (const placement of ['page', 'panel'] as const) {
    it(`takes the secret’s box and its line away when the key it shows is revoked, and leaves both when another key is, as a ${placement}`, async () => {
      const keys = listedKeys(
        [{ id: 1, name: 'Panels' }],
        [{ id: 2, name: 'phone' }],
      )
      vi.stubGlobal('fetch', keys.server)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect(host.querySelector('.secret')?.textContent).toContain(
        'tyk_the-only-copy',
      )

      await revoke(host, 'Panels')
      expect(keys.revoked).toEqual([1])
      expect(host.querySelector('.secret')?.textContent).toContain(
        'tyk_the-only-copy',
      )
      expect(keyRegion(host).textContent).toBe(KEY_MADE)

      await revoke(host, 'phone')
      expect(keys.revoked).toEqual([1, 2])
      expect(host.querySelector('.secret')).toBeNull()
      expect(keyRegion(host).textContent).toBe('')
    })
  }

  it('leaves the secret’s box on every revoke when the listing after the create cannot say which key it is', async () => {
    // Another browser made a key while this one's was made, so two keys are
    // new to the listing that follows, and either may be the one shown.
    const keys = listedKeys(
      [{ id: 1, name: 'Panels' }],
      [
        { id: 2, name: 'phone' },
        { id: 3, name: 'tablet' },
      ],
    )
    vi.stubGlobal('fetch', keys.server)
    const host = mountAs('page')
    await settle()
    await submitted(host.querySelector(KEY_FORM))
    await revoke(host, 'tablet')
    await revoke(host, 'phone')
    expect(keys.revoked).toEqual([3, 2])
    expect(host.querySelector('.secret')?.textContent).toContain(
      'tyk_the-only-copy',
    )
  })

  it('leaves a newer key’s box when the revoke of the key shown before it answers after that key is made', async () => {
    // Revoke is not held while a key is asked for, so the shown key's
    // revocation can answer once the next key's box is drawn, and that box
    // holds a secret nothing can show again.
    const keys = madeInTurn()
    const revoked = deferred<Response>()
    keys.hold('DELETE /api/keys/1', revoked.promise)
    vi.stubGlobal('fetch', keys.fetch)
    const host = mountAs('page')
    await settle()
    const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
    if (!keyName) throw new Error('no key name box')
    type(keyName, 'phone')
    await submitted(host.querySelector(KEY_FORM))
    expect(host.querySelector('.secret')?.textContent).toContain('tyk_first')

    await revoke(host, 'phone')
    type(keyName, 'tablet')
    await submitted(host.querySelector(KEY_FORM))
    expect(host.querySelector('.secret')?.textContent).toContain('tyk_second')

    await act(async () => {
      revoked.resolve(new Response(null, { status: 204 }))
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    expect(host.querySelector('.secret')?.textContent ?? '').toContain(
      'tyk_second',
    )
    expect(keyRegion(host).textContent).toBe(
      'Key made: tablet. Copy it now — it is not stored and cannot be shown again.',
    )
  })

  // The listing that names a made key is its own create's: the box is drawn
  // with it, and Create is held until then, so no later key can be asked
  // for while it is out and take the box's place.
  it('draws a made key’s box only with the listing after its create, and holds Create until then', async () => {
    const keys = madeInTurn()
    const firstListing = deferred<void>()
    // The mount's listing is the first; the first create's is the second.
    keys.holdKeyRead(2, firstListing.promise)
    vi.stubGlobal('fetch', keys.fetch)
    const host = mountAs('page')
    await settle()
    const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
    if (!keyName) throw new Error('no key name box')
    type(keyName, 'phone')
    await submitted(host.querySelector(KEY_FORM))
    // The create has answered and its listing has not.
    expect(host.querySelector('.secret')).toBeNull()
    type(keyName, 'tablet')
    await submitted(host.querySelector(KEY_FORM))
    const creates = () =>
      keys.fetch.mock.calls.filter(([, init]) => init?.method === 'POST')
    expect(creates()).toHaveLength(1)

    await act(async () => {
      firstListing.resolve()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    expect(host.querySelector('.secret')?.textContent).toContain('tyk_first')
    await revoke(host, 'phone')
    expect(keys.revoked).toEqual([1])
    expect(host.querySelector('.secret')).toBeNull()
  })

  // The user's ruling of 2026-10-25: a press of the secret's box copies it,
  // through `useCopy`, as Copy address copies the catalog's address.
  for (const placement of ['page', 'panel'] as const) {
    it(`draws the secret’s box as a button named by its caption, which copies exactly the secret and says Copied, as a ${placement}`, async () => {
      const writeText = vi.fn(() => Promise.resolve())
      clipboardContext(true, { writeText })
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const said = copyRegion(host)
      expect(said?.textContent).toBe('')
      await submitted(host.querySelector(KEY_FORM))

      const box = host.querySelector<HTMLButtonElement>('.secret')
      if (!box) throw new Error('no secret box')
      expect([box.tagName, box.type, box.className]).toEqual([
        'BUTTON',
        'button',
        placement === 'page' ? 'secret' : 'secret reader-secret',
      ])
      // Named by its caption, which says what a press does, and so not by
      // the secret it holds.
      const name = document.getElementById(
        box.getAttribute('aria-labelledby') ?? '',
      )
      expect(name?.textContent).toBe(CAPTION)
      expect(box.contains(name)).toBe(true)
      expect(box.hasAttribute('aria-label')).toBe(false)

      await act(async () => {
        box.click()
      })
      await settle()
      expect(writeText.mock.calls).toEqual([['tyk_the-only-copy']])
      expect(copyRegion(host)).toBe(said)
      expect(said?.textContent).toBe('Copied')
    })
  }

  it.each([
    ['outside a secure context', false, { writeText: vi.fn() }],
    ['without a clipboard', true, undefined],
  ] as const)(
    'draws the secret as text to be selected by hand, and no copy, %s',
    async (_, secure, clipboard) => {
      clipboardContext(secure, clipboard)
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
        }),
      )
      const host = mountAs('page')
      await settle()
      await submitted(host.querySelector(KEY_FORM))
      const box = host.querySelector('.secret')
      expect(box?.tagName).toBe('DIV')
      expect(box?.textContent).toBe(`tyk_the-only-copy${CAPTION}`)
      expect(copyRegion(host)).toBeNull()
    },
  )

  it('says Not copied for a refused write, and draws the secret as text, selected for copying by hand, with focus on its box, until the next key', async () => {
    clipboardContext(true, {
      writeText: vi.fn(() => Promise.reject(new Error('denied'))),
    })
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
      }),
    )
    const host = mountAs('page')
    await settle()
    await submitted(host.querySelector(KEY_FORM))
    const button = host.querySelector<HTMLButtonElement>('button.secret')
    if (!button) throw new Error('no secret box to press')
    act(() => button.focus())
    await act(async () => {
      button.click()
    })
    await settle()

    expect(copyRegion(host)?.textContent).toBe('Not copied')
    const box = host.querySelector<HTMLElement>('.secret')
    expect(box?.tagName).toBe('DIV')
    expect(document.activeElement).toBe(box)
    expect(window.getSelection()?.toString()).toBe('tyk_the-only-copy')

    await submitted(host.querySelector(KEY_FORM))
    expect(host.querySelector('.secret')?.tagName).toBe('BUTTON')
  })

  it('shows why a password change was refused', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'PUT /api/password': () => new Response(null, { status: 403 }),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const form = host.querySelector('form')
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()

    expect(host.querySelector('[role="alert"]')?.textContent).toMatch(
      /current password did not match/,
    )
  })

  /** Revoking a key re-reads the list, so the row goes without a reload. */
  it('revokes a key and reflects it', async () => {
    let keys = [{ id: 7, name: 'phone', created: 'then', last_used: null }]
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (method === 'DELETE' && input === '/api/keys/7') {
        keys = []
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      if (input === '/api/keys') return Promise.resolve(json(keys))
      if (input === '/api/sessions') return Promise.resolve(json([]))
      return Promise.resolve(new Response(null, { status: 404 }))
    })
    vi.stubGlobal('fetch', fetchMock)

    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()
    expect(host.textContent).toContain('phone')

    await act(async () => {
      buttonSaying(host, 'Revoke')?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.textContent).not.toContain('phone')
  })

  it('revokes another session and reflects it', async () => {
    let sessions = [
      {
        id: 'bb',
        created: 'then',
        last_seen: 'yesterday',
        user_agent: 'Safari',
        ip: null,
        current: false,
      },
    ]
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (method === 'DELETE' && input === '/api/sessions/bb') {
        sessions = []
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      if (input === '/api/keys') return Promise.resolve(json([]))
      if (input === '/api/sessions') return Promise.resolve(json(sessions))
      return Promise.resolve(new Response(null, { status: 404 }))
    })
    vi.stubGlobal('fetch', fetchMock)

    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()
    expect(host.textContent).toContain('Safari')

    await act(async () => {
      buttonSaying(host, 'Revoke')?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.textContent).not.toContain('Safari')
  })

  it('closes on its own button', async () => {
    vi.stubGlobal(
      'fetch',
      server({ '/api/keys': () => json([]), '/api/sessions': () => json([]) }),
    )
    const onClose = vi.fn()
    const host = mount(
      <Account visible={true} onClose={onClose} onSignedOut={noop} />,
    )
    await settle()
    buttonSaying(host, 'Close')?.click()
    expect(onClose).toHaveBeenCalledOnce()
  })

  /** A key creation that fails says so rather than showing a blank secret. */
  it('reports a refused key creation', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'POST /api/keys': () => new Response(NEEDS_A_NAME, { status: 400 }),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const form = host.querySelector('form:has(input[placeholder])')
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')).not.toBeNull()
  })

  /**
   * A listing that fails leaves the panel usable rather than throwing into an
   * unmounted promise — a reader whose keys did not load can still change a
   * password.
   */
  it('survives a listing that fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 500 })),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()
    expect(host.querySelector('[role="dialog"]')).not.toBeNull()
  })

  /**
   * **A list whose first read fails is drawn as nothing, and the rest of the
   * screen is drawn as ever** (spec 2026-10-27 §4.6 as corrected, and §5):
   * a reader whose keys could not be read can still change a password and
   * end a session. Nothing says the failure: the screen says none of its
   * lists' failures (`Account.tsx`), and before a first good listing there
   * is none to keep on screen in the list's place.
   */
  for (const placement of ['page', 'panel'] as const) {
    for (const failed of ['keys', 'sessions'] as const) {
      it(`draws the whole screen, with no row for the ${failed} and no alert, when the ${failed}’ first read fails, as a ${placement}`, async () => {
        const keyRows = [
          { id: 1, name: 'Panels', created: 'then', last_used: null },
        ]
        const sessionRows = [
          {
            id: 'aa',
            created: 'then',
            last_seen: 'today',
            user_agent: 'Firefox',
            ip: null,
            current: true,
          },
        ]
        const refused = () => new Response(null, { status: 500 })
        vi.stubGlobal(
          'fetch',
          server({
            '/api/keys': failed === 'keys' ? refused : () => json(keyRows),
            '/api/sessions':
              failed === 'sessions' ? refused : () => json(sessionRows),
            '/api/about': () => json(ABOUT),
          }),
        )
        const host = mountAs(placement)
        await settle()
        await flush()
        const passwordForm = host.querySelector(PASSWORD_FORM)
        expect(
          [...(passwordForm?.querySelectorAll('input[type="password"]') ?? [])]
            .length,
        ).toBe(3)
        expect(buttonSaying(host, 'Change password')).toBeDefined()
        expect(host.querySelector(`${KEY_FORM} input[placeholder]`)).not.toBe(
          null,
        )
        expect(buttonSaying(host, 'Create')).toBeDefined()
        expect(buttonSaying(host, 'Sign out')).toBeDefined()
        expect([
          rowListed(host, 'Panels'),
          [...host.querySelectorAll('li, .reader-section-head')].some(
            (row) =>
              (row.textContent ?? '').includes('Firefox') &&
              row.querySelector('button')?.textContent === 'Revoke',
          ),
        ]).toEqual([failed !== 'keys', failed !== 'sessions'])
        expect(host.querySelector('[role="alert"]')).toBeNull()
      })
    }
  }

  /**
   * Every field carries what was typed into the call, which the form-submitting
   * tests above cannot show: they submit empty forms, so a component that
   * dropped its inputs on the floor would pass all of them.
   */
  it('sends what was typed into each field', async () => {
    const fetchMock = vi.fn((input: string, init?: RequestInit) => {
      if ((init?.method ?? 'GET') !== 'GET')
        return Promise.resolve(new Response(null, { status: 204 }))
      if (input === '/api/keys') return Promise.resolve(json([]))
      return Promise.resolve(json([]))
    })
    vi.stubGlobal('fetch', fetchMock)

    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const inputs = [...host.querySelectorAll('input')] as HTMLInputElement[]
    const [current, next, confirm, keyName] = inputs
    if (!current || !next || !confirm || !keyName) {
      throw new Error('missing fields')
    }
    type(current, 'the old one')
    type(next, 'the new one')
    type(confirm, 'the new one')
    type(keyName, 'tablet')

    const forms = [...host.querySelectorAll('form')]
    for (const form of forms) {
      await act(async () => {
        form.dispatchEvent(
          new Event('submit', { bubbles: true, cancelable: true }),
        )
        await Promise.resolve()
      })
    }
    await settle()

    expect(fetchMock).toHaveBeenCalledWith('/api/password', {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ current: 'the old one', next: 'the new one' }),
    })
    expect(fetchMock).toHaveBeenCalledWith('/api/keys', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'tablet' }),
    })
  })

  /**
   * A revoke that fails leaves the row on screen rather than removing it
   * optimistically — the key is still live, and a list that had already
   * forgotten it would be lying about what the server holds.
   */
  it('keeps a row when its revoke fails', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'DELETE')
          return Promise.resolve(new Response(null, { status: 500 }))
        if (input === '/api/keys')
          return Promise.resolve(
            json([{ id: 7, name: 'phone', created: 'then', last_used: null }]),
          )
        return Promise.resolve(json([]))
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    await act(async () => {
      buttonSaying(host, 'Revoke')?.click()
      await Promise.resolve()
    })
    await settle()
    expect(rowListed(host, 'phone')).toBe(true)
  })

  /** Signing out that fails does not tell `App` the reader is signed out. */
  it('does not report a sign-out that failed', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((_input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'DELETE')
          return Promise.resolve(new Response(null, { status: 500 }))
        return Promise.resolve(json([]))
      }),
    )
    const onSignedOut = vi.fn()
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={onSignedOut} />,
    )
    await settle()

    await act(async () => {
      buttonSaying(host, 'Sign out')?.click()
      await Promise.resolve()
    })
    await settle()
    expect(onSignedOut).not.toHaveBeenCalled()
  })

  /**
   * **The one-time secret does not survive closing the panel.**
   *
   * `App` renders `<Account>` unconditionally, so `return null` hides it
   * without unmounting and React keeps every hook's state. The secret was
   * therefore redisplayed to anyone who reopened the panel — for the rest of
   * the page's life — while the copy beside it said it "cannot be shown
   * again". Found by the whole-branch review, 2026-08-18.
   */
  it('forgets the shown secret and the typed passwords when it closes', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const inputs = [...host.querySelectorAll('input')] as HTMLInputElement[]
    const [current, next, confirm, keyName] = inputs
    if (!current || !next || !confirm || !keyName)
      throw new Error('missing fields')
    type(current, 'my password')
    type(next, 'my next password')
    type(confirm, 'my next password')

    const form = host.querySelector('form:has(input[placeholder])')
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    expect(host.textContent).toContain('tyk_the-only-copy')

    // Close, then reopen the same instance — which is what `App` does.
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    rerender(<Account visible={true} onClose={noop} onSignedOut={noop} />)
    await settle()

    expect(host.textContent).not.toContain('tyk_the-only-copy')
    const reopened = [...host.querySelectorAll('input')] as HTMLInputElement[]
    expect(reopened.slice(0, 3).map((box) => box.value)).toEqual(['', '', ''])
  })

  /**
   * Neither form may be submitted twice while its first request is in flight.
   * A double-click on Create minted two permanent API keys and showed only the
   * second secret — the first existed, was never displayed, and could be found
   * only by listing keys and guessing which of two identically-named rows it
   * was. Found by the second whole-branch review.
   */
  it('will not submit either form twice while one is in flight', async () => {
    let creates = 0
    let release: (() => void) | undefined
    const held = new Promise<void>((resolve) => {
      release = resolve
    })
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'POST' && input === '/api/keys') {
          creates += 1
          return held.then(() => json({ secret: 'tyk_once' }))
        }
        return Promise.resolve(json([]))
      }),
    )

    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const form = host.querySelector('form:has(input[placeholder])')
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    // The second submit, while the first is still unresolved.
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    expect(creates).toBe(1)

    release?.()
    await settle()
    // And once it lands, the form works again.
    expect(host.textContent).toContain('tyk_once')
  })

  /** A sign-out that failed is said, not swallowed. */
  it('reports a sign-out that did not work', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((_input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'DELETE')
          return Promise.resolve(new Response(null, { status: 500 }))
        return Promise.resolve(json([]))
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()
    await act(async () => {
      buttonSaying(host, 'Sign out')?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')).not.toBeNull()
  })

  /**
   * **A revocation that did not happen must not look like one that did.**
   *
   * Both Revoke buttons discarded every failure and skipped the refresh, so the
   * row stayed on screen unchanged with no alert — and a reader revoking the key
   * for a tablet they had just lost believed the credential was dead when it
   * still worked. Found by the third whole-branch review.
   */
  it('says when a revoke failed rather than leaving the row looking revoked', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'DELETE')
          return Promise.resolve(new Response(null, { status: 403 }))
        if (input === '/api/keys')
          return Promise.resolve(
            json([
              { id: 7, name: 'lost tablet', created: 'then', last_used: null },
            ]),
          )
        return Promise.resolve(
          json([
            {
              id: 'bb',
              created: 'then',
              last_seen: 'yesterday',
              user_agent: 'Safari',
              ip: null,
              current: false,
            },
          ]),
        )
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const revokes = () =>
      [...host.querySelectorAll('button')].filter(
        (b) => b.textContent === 'Revoke',
      )
    await act(async () => {
      revokes()[0]?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toMatch(
      /was not revoked/,
    )
    expect(host.textContent).toContain('lost tablet')

    // The session button reports its own failure in its own words.
    await act(async () => {
      revokes()[1]?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toMatch(
      /was not ended/,
    )
  })

  /**
   * **The clause naming the gesture used to be on the `Error` arm alone.**
   * Both Revoke handlers read
   * `why instanceof Error ? `That key was not revoked: ${why.message}` : String(why)`,
   * so a rejection that was not an `Error` — a cancelled request, a proxy
   * closing the socket — printed the bare reason with no clause saying what had
   * failed, under a row still sitting on screen. That is the same
   * indistinguishability the case above this one exists to prevent, reached by
   * the other arm. O103 collapsed the ternary into `messageOf` and the prefix
   * became unconditional; this is what holds it there. The non-`Error` case
   * further down drives the POST and PUT forms only and never reaches a DELETE.
   */
  it('still names the gesture when a revoke rejects with something that is not an Error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'DELETE')
          return Promise.reject('the network went away')
        if (input === '/api/keys')
          return Promise.resolve(
            json([
              { id: 7, name: 'lost tablet', created: 'then', last_used: null },
            ]),
          )
        return Promise.resolve(
          json([
            {
              id: 'bb',
              created: 'then',
              last_seen: 'yesterday',
              user_agent: 'Safari',
              ip: null,
              current: false,
            },
          ]),
        )
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    const revokes = () =>
      [...host.querySelectorAll('button')].filter(
        (b) => b.textContent === 'Revoke',
      )
    await act(async () => {
      revokes()[0]?.click()
      await Promise.resolve()
    })
    await settle()
    // Both halves asserted: the reason *and* the clause that says what it was
    // the reason for. A match on the reason alone passes against the defect.
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'That key was not revoked: the network went away',
    )

    await act(async () => {
      revokes()[1]?.click()
      await Promise.resolve()
    })
    await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'That session was not ended: the network went away',
    )
  })

  /**
   * A rejection that is not an `Error` is still shown. Every handler here
   * routes its `catch` through `message.ts`'s `messageOf`, whose `String(err)`
   * arm is the one nothing else exercises — a `fetch` that rejects with a
   * string, or a bare object, would otherwise render as nothing at all. The
   * arm is pinned directly in `message.test.ts`; this case is what proves it
   * reaches the screen from a real handler, which a unit test cannot.
   */
  it('shows a rejection that is not an Error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn((_input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'POST')
          return Promise.reject('the network went away')
        if ((init?.method ?? 'GET') === 'PUT')
          return Promise.reject('the network went away')
        return Promise.resolve(json([]))
      }),
    )
    const host = mount(
      <Account visible={true} onClose={noop} onSignedOut={noop} />,
    )
    await settle()

    for (const form of [...host.querySelectorAll('form')]) {
      await act(async () => {
        form.dispatchEvent(
          new Event('submit', { bubbles: true, cancelable: true }),
        )
        await Promise.resolve()
      })
      await settle()
      expect(host.querySelector('[role="alert"]')?.textContent).toContain(
        'the network went away',
      )
    }
  })

  it('heads the page with an h1 and its sections with h2s — O163', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
      }),
    )
    const host = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()
    expect(
      [...host.querySelectorAll('h1, h2, h3')].map((h) => [
        h.tagName,
        h.textContent,
      ]),
    ).toEqual([
      ['H1', 'Account'],
      ['H2', 'Password'],
      ['H2', 'API keys'],
      ['H2', 'Signed in'],
    ])
  })

  // With a key and a session, so the report-only rows are on the page too,
  // and are passed over as rows holding no form control.
  it('wires each of the page’s boxes to its row’s label, as Row wires one', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () =>
          json([{ id: 7, name: 'phone', created: 'then', last_used: null }]),
        '/api/sessions': () =>
          json([
            {
              id: 'aa',
              created: 'then',
              last_seen: 'today',
              user_agent: 'Firefox',
              ip: null,
              current: true,
            },
          ]),
      }),
    )
    const host = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()
    const screen = pageScreen(host)
    expect(screen.querySelectorAll('.settings-row')).toHaveLength(6)
    expectRowsWired(screen)
  })

  // The page's four boxes, each a row's control: the three passwords and the
  // key's name, which its label names now and not its placeholder.
  it('gives each box its label’s words as its aria-label, as a page — O175', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
      }),
    )
    const host = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()
    expect(
      fields(host.querySelectorAll('label')).map(({ written, control }) => [
        written,
        control.getAttribute('aria-label'),
        control.getAttribute('type'),
      ]),
    ).toEqual([
      ['Current password', 'Current password', 'password'],
      ['New password', 'New password', 'password'],
      ['Confirm password', 'Confirm password', 'password'],
      ['New key', 'New key', null],
    ])
  })

  // The panel's three labels, drawn as written, whose boxes carry the same
  // `aria-label` all the same: Current, New and Confirm, the panel's short
  // words (the user's ruling of 2026-10-25), where the page says Confirm
  // password.
  it('gives each password box its label’s words as its aria-label, as a panel — O175', async () => {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
      }),
    )
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    // The screen's only three labels. Counted, because a loop over nothing
    // passes.
    const labelled = fields(host.querySelectorAll('label'))
    expect(labelled.map(({ written }) => written)).toEqual([
      'Current',
      'New',
      'Confirm',
    ])
    for (const { written, control } of labelled) {
      expect(control.getAttribute('type'), written).toBe('password')
      expect(control.getAttribute('aria-label'), written).toBe(written)
    }
  })

  /**
   * **The catalog's address, beside the OPDS line, in both placements**, as
   * `/api/about` answers it. Another version than today's, so an address the
   * screen wrote for itself cannot pass for the one it was handed.
   */
  describe('the catalog’s address', () => {
    const CATALOG = '/opds/v1.3/catalog'
    const lists = {
      '/api/keys': () => json([]),
      '/api/sessions': () => json([]),
    }
    const answered = () =>
      server({
        ...lists,
        '/api/about': () => json({ ...ABOUT, catalog: CATALOG }),
      })

    const placed = (placement: 'page' | 'panel') =>
      mount(
        placement === 'page' ? (
          <Account placement="page" onSignedOut={noop} />
        ) : (
          <Account visible onClose={noop} onSignedOut={noop} />
        ),
      )

    /**
     * Whether `row`'s one button is the unseen Copy address that keeps a
     * button's room: hidden from assistive technology, out of the Tab
     * order and taking no press (`CatalogAddress`'s `keepsButtonRoom`),
     * and of the class `placement` draws its buttons with, so that it
     * takes the room one of them does.
     */
    const roomKeeper = (
      row: Element | null | undefined,
      placement: 'page' | 'panel',
    ) => {
      const buttons = [...(row?.querySelectorAll('button') ?? [])]
      return (
        buttons.length === 1 &&
        buttons[0]?.getAttribute('aria-hidden') === 'true' &&
        buttons[0]?.tabIndex === -1 &&
        buttons[0]?.disabled === true &&
        buttons[0]?.className === (placement === 'panel' ? 'reader-button' : '')
      )
    }

    /** The OPDS line: the paragraph under "API keys" that starts so. */
    const opdsLine = (host: HTMLElement) => {
      const section = [...host.querySelectorAll('section')].find(
        (el) => el.querySelector('h2, h3')?.textContent === 'API keys',
      )
      const line = [...(section?.querySelectorAll('p') ?? [])].find((p) =>
        (p.textContent ?? '').startsWith('For OPDS readers'),
      )
      if (!line) throw new Error('no OPDS line under "API keys"')
      return line
    }

    /**
     * What is drawn straight after the OPDS line. In the panel that is the
     * line's next sibling. On the page the line is the API keys section's
     * description, above its box, so what follows it is the box's first
     * part.
     */
    const afterLine = (host: HTMLElement, placement: 'page' | 'panel') => {
      const line = opdsLine(host)
      if (placement === 'panel') return line.nextElementSibling
      expect(line.className).toBe('settings-section-description')
      return (
        line.closest('section')?.querySelector(':scope > .settings-box')
          ?.firstElementChild ?? null
      )
    }

    // One `it()` per placement: `testing/mount.ts` holds one root.
    for (const placement of ['page', 'panel'] as const) {
      it(`draws it straight after the OPDS line, and the form after it, as a ${placement}`, async () => {
        vi.stubGlobal('fetch', answered())
        const host = placed(placement)
        await settle()
        const next = afterLine(host, placement)
        expect(next?.className).toBe('catalog-address')
        expect(next?.querySelector('code')?.textContent).toBe(
          `${location.origin}${CATALOG}`,
        )
        expect(host.querySelectorAll('code')).toHaveLength(1)
        expect(next?.nextElementSibling?.tagName).toBe('FORM')
      })

      it(`draws Copy address as it draws the row’s own buttons, as a ${placement}`, async () => {
        clipboardContext(true, { writeText: vi.fn() })
        vi.stubGlobal('fetch', answered())
        const host = placed(placement)
        await settle()
        const copy = buttonSaying(host, 'Copy address')
        expect(copy?.className).toBe(
          placement === 'panel' ? 'reader-button' : '',
        )
        expect(copy?.className).toBe(buttonSaying(host, 'Create')?.className)
      })

      it(`says in the address’s row that the catalog’s address could not be read, with no Copy address but an unseen one keeping its room, and no alert, when /api/about does not answer, as a ${placement}`, async () => {
        clipboardContext(true, { writeText: vi.fn() })
        const fetchMock = server(lists)
        vi.stubGlobal('fetch', fetchMock)
        const host = placed(placement)
        await settle()
        // Asked, and answered 404: not a screen that never asked.
        expect(fetchMock).toHaveBeenCalledWith('/api/about')
        const next = afterLine(host, placement)
        expect(next?.className).toBe('catalog-address')
        expect(next?.firstElementChild?.textContent).toBe(
          'The catalog’s address could not be read.',
        )
        expect(next?.nextElementSibling?.tagName).toBe('FORM')
        expect(host.querySelector('code')).toBeNull()
        expect(roomKeeper(next, placement)).toBe(true)
        expect(host.querySelector('[role="alert"]')).toBeNull()
      })

      it(`draws no body, and so no address’s row, while /api/about is being read, as a ${placement}`, async () => {
        clipboardContext(true, { writeText: vi.fn() })
        const listing = server(lists)
        vi.stubGlobal(
          'fetch',
          vi.fn((input: string, init?: RequestInit) =>
            input === '/api/about'
              ? new Promise<Response>(() => {})
              : listing(input, init),
          ),
        )
        const host = placed(placement)
        await settle()
        await flush()
        expect(host.querySelector('.catalog-address')).toBeNull()
        expect(host.querySelector('form')).toBeNull()
      })
    }
  })
})

/**
 * What a create or a revocation leaves behind when its answer lands late:
 * the panel closed meanwhile, a slow list of sessions, a name typed while
 * the key list is read, and an alert from an earlier revocation.
 */
describe('Account’s keys, answered late', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  // The defect the close effect's comment names, reached through a create
  // whose listing lands once the panel has closed.
  it('does not draw a key made before the panel closed when the panel opens again, though its listing landed while it was closed', async () => {
    const keys = madeInTurn()
    const listing = deferred<void>()
    // The mount's listing is the first; the create's is the second.
    keys.holdKeyRead(2, listing.promise)
    vi.stubGlobal('fetch', keys.fetch)
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
    if (!keyName) throw new Error('no key name box')
    type(keyName, 'phone')
    await submitted(host.querySelector(KEY_FORM))

    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    await act(async () => {
      listing.resolve()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(host.querySelector('.secret')).toBeNull()
    expect(host.textContent).not.toContain('tyk_first')
    expect(keyRegion(host).textContent).toBe('')
    // Create is free again: what waited on that listing ran as the panel
    // opened, though the listing landed while nothing drew it.
    const again = host.querySelector<HTMLInputElement>('input[placeholder]')
    if (!again) throw new Error('no key name box')
    type(again, 'tablet')
    await submitted(host.querySelector(KEY_FORM))
    expect(host.querySelector('.secret')?.textContent).toContain('tyk_second')
  })

  for (const placement of ['page', 'panel'] as const) {
    it(`draws a made key’s box with the key list alone, and frees Create, while the list of sessions has not answered, as a ${placement}`, async () => {
      const keys = madeInTurn()
      // The screen's first read answers, so its body is drawn; every listing
      // of the sessions after it is left unanswered.
      let sessionReads = 0
      vi.stubGlobal(
        'fetch',
        vi.fn((input: string, init?: RequestInit) => {
          if (input !== '/api/sessions') return keys.fetch(input, init)
          sessionReads += 1
          return sessionReads === 1
            ? Promise.resolve(json([]))
            : new Promise<Response>(() => {})
        }),
      )
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect(host.querySelector('.secret')?.textContent).toContain('tyk_first')
      expect(rowListed(host, 'phone')).toBe(true)
      type(keyName, 'tablet')
      await submitted(host.querySelector(KEY_FORM))
      expect(host.querySelector('.secret')?.textContent).toContain('tyk_second')
    })

    it(`keeps a name typed in the key’s box while the listing after a create is read, and empties a box still holding the name sent, as a ${placement}`, async () => {
      const keys = madeInTurn()
      const first = deferred<void>()
      const second = deferred<void>()
      keys.holdKeyRead(2, first.promise)
      keys.holdKeyRead(3, second.promise)
      vi.stubGlobal('fetch', keys.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      const land = (listing: { resolve: () => void }) =>
        act(async () => {
          listing.resolve()
          await new Promise((resolve) => {
            setTimeout(resolve, 0)
          })
        })

      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      type(keyName, 'tablet')
      await land(first)
      expect(host.querySelector('.secret')?.textContent).toContain('tyk_first')
      expect(keyName.value).toBe('tablet')

      await submitted(host.querySelector(KEY_FORM))
      await land(second)
      expect(host.querySelector('.secret')?.textContent).toContain('tyk_second')
      expect(keyName.value).toBe('')
    })

    it(`takes away the alert of a revocation that failed with the list change of the next one that works, of a key or of a session, as a ${placement}`, async () => {
      const listings: Array<() => void> = []
      let keyRows = [
        { id: 1, name: 'Panels', created: 'then', last_used: null },
        { id: 2, name: 'Chunky', created: 'then', last_used: null },
      ]
      let sessionRows = [
        {
          id: 'aa',
          created: 'then',
          last_seen: 'today',
          user_agent: 'Firefox',
          ip: null,
          current: true,
        },
        {
          id: 'bb',
          created: 'then',
          last_seen: 'then',
          user_agent: 'Safari',
          ip: null,
          current: false,
        },
        {
          id: 'cc',
          created: 'then',
          last_seen: 'then',
          user_agent: 'Chrome',
          ip: null,
          current: false,
        },
      ]
      // Each listing after the mount's waits for the case to let it land.
      let reads = 0
      const later = <T,>(rows: () => T) => {
        reads += 1
        if (reads <= 2) return Promise.resolve(json(rows()))
        return new Promise<Response>((resolve) => {
          listings.push(() => resolve(json(rows())))
        })
      }
      vi.stubGlobal(
        'fetch',
        vi.fn((input: string, init?: RequestInit) => {
          const asked = `${init?.method ?? 'GET'} ${input}`
          if (asked === 'DELETE /api/keys/1') {
            return Promise.resolve(new Response(null, { status: 500 }))
          }
          if (asked === 'DELETE /api/keys/2') {
            keyRows = keyRows.filter((key) => key.id !== 2)
            return Promise.resolve(new Response(null, { status: 204 }))
          }
          if (asked === 'DELETE /api/sessions/bb') {
            return Promise.resolve(new Response(null, { status: 500 }))
          }
          if (asked === 'DELETE /api/sessions/cc') {
            sessionRows = sessionRows.filter((session) => session.id !== 'cc')
            return Promise.resolve(new Response(null, { status: 204 }))
          }
          if (input === '/api/keys') return later(() => keyRows)
          if (input === '/api/sessions') return later(() => sessionRows)
          return Promise.resolve(new Response(null, { status: 404 }))
        }),
      )
      const host = mountAs(placement)
      await settle()
      const alert = () => host.querySelector('[role="alert"]')?.textContent
      // A list's row for `name`, ending in its Revoke. Not the screen's
      // words: the OPDS line names Chunky as well.
      const listed = (name: string) =>
        [...host.querySelectorAll('li, .reader-section-head')].some(
          (row) =>
            (row.textContent ?? '').includes(name) &&
            row.querySelector('button')?.textContent === 'Revoke',
        )
      const landAll = () =>
        act(async () => {
          for (const land of listings.splice(0)) land()
          await new Promise((resolve) => {
            setTimeout(resolve, 0)
          })
        })

      await revoke(host, 'Panels')
      expect(alert()).toMatch(/That key was not revoked/)
      await revoke(host, 'Chunky')
      // Answered, and its listing not yet landed: the alert and the row stay.
      expect([alert(), listed('Chunky')]).toEqual([
        expect.stringMatching(/That key was not revoked/),
        true,
      ])
      await landAll()
      expect([alert(), listed('Chunky')]).toEqual([undefined, false])

      const sessionRevoke = (agent: string) => {
        const button = [...host.querySelectorAll('li, .reader-section-head')]
          .filter((row) => (row.textContent ?? '').includes(agent))
          .map((row) => row.querySelector('button'))
          .find((b) => b?.textContent === 'Revoke')
        if (!button) throw new Error(`no Revoke for ${agent}`)
        return button
      }
      const press = (agent: string) =>
        act(async () => {
          sessionRevoke(agent).click()
          await new Promise((resolve) => {
            setTimeout(resolve, 0)
          })
        })
      await press('Safari')
      expect(alert()).toMatch(/That session was not ended/)
      await press('Chrome')
      expect([alert(), listed('Chrome')]).toEqual([
        expect.stringMatching(/That session was not ended/),
        true,
      ])
      await landAll()
      expect([alert(), listed('Chrome')]).toEqual([undefined, false])
    })
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`keeps the alert of a revocation or a sign-out that failed after the press of a revocation that worked, through the list change of the one that worked, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()

      await revoke(host, 'Chunky')
      await revoke(host, 'Panels')
      expect(alertIn(host)).toMatch(/That key was not revoked/)
      await revokes.land()
      expect([
        alertIn(host),
        rowListed(host, 'Panels'),
        rowListed(host, 'Chunky'),
      ]).toEqual([
        expect.stringMatching(/That key was not revoked/),
        true,
        false,
      ])

      await revoke(host, 'Chrome')
      await revoke(host, 'Safari')
      expect(alertIn(host)).toMatch(/That session was not ended/)
      await revokes.land()
      expect([
        alertIn(host),
        rowListed(host, 'Safari'),
        rowListed(host, 'Chrome'),
      ]).toEqual([
        expect.stringMatching(/That session was not ended/),
        true,
        false,
      ])

      await revoke(host, 'Edge')
      const signOut = buttonSaying(host, 'Sign out')
      if (!signOut) throw new Error('no Sign out')
      await act(async () => {
        signOut.click()
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })
      const signOutFailure = alertIn(host)
      expect(signOutFailure).not.toMatch(/That session was not ended/)
      await revokes.land()
      expect([alertIn(host), rowListed(host, 'Edge')]).toEqual([
        signOutFailure,
        false,
      ])
    })
  }

  // The lists a revocation pressed before the close made stale are read
  // before a panel opened again draws its body, so what that revocation
  // does alongside them has run before anything in the panel can fail.
  it('waits, in a panel opened again, for the lists a revocation pressed before the close made stale, and keeps the alert of a revocation that fails after them', async () => {
    const revokes = failsSomeRevokes()
    vi.stubGlobal('fetch', revokes.fetch)
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await revoke(host, 'Chunky')
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(rowListed(host, 'Panels')).toBe(false)
    await revokes.land()
    expect([rowListed(host, 'Panels'), rowListed(host, 'Chunky')]).toEqual([
      true,
      false,
    ])

    await revoke(host, 'Panels')
    await revokes.land()
    expect([alertIn(host), rowListed(host, 'Panels')]).toEqual([
      expect.stringMatching(/That key was not revoked/),
      true,
    ])
  })

  for (const placement of ['page', 'panel'] as const) {
    it(`drops a listing of the keys that lands after a later one, so that a revoked key is not listed again, as a ${placement}`, async () => {
      const keys = madeInTurn()
      const tabletListing = deferred<void>()
      // The mount's listing, the two creates', then the revocations'.
      keys.holdKeyRead(4, tabletListing.promise)
      vi.stubGlobal('fetch', keys.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      type(keyName, 'tablet')
      await submitted(host.querySelector(KEY_FORM))

      // Asked once tablet was revoked, and so still holding phone.
      await revoke(host, 'tablet')
      await revoke(host, 'phone')
      // Phone's listing, the later one, has landed and run what tablet's
      // revocation does alongside its list: tablet's box has gone with it.
      expect([
        rowListed(host, 'phone'),
        rowListed(host, 'tablet'),
        host.querySelector('.secret'),
      ]).toEqual([false, false, null])
      await act(async () => {
        tabletListing.resolve()
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })
      // Tablet's own listing, landing after, is not drawn.
      expect([
        rowListed(host, 'phone'),
        rowListed(host, 'tablet'),
        host.querySelector('.secret'),
      ]).toEqual([false, false, null])
    })

    it(`drops a listing of the keys that lands after a later one, so that a made key’s box keeps its row, as a ${placement}`, async () => {
      const keys = madeInTurn()
      const returnListing = deferred<void>()
      // The first visit's listing is the first; the return's, the second.
      keys.holdKeyRead(2, returnListing.promise)
      // `/api/about` answered, so the return has every answer kept and
      // draws them at once rather than waiting on its reads.
      vi.stubGlobal(
        'fetch',
        vi.fn((input: string, init?: RequestInit) =>
          input === '/api/about'
            ? Promise.resolve(json(ABOUT))
            : keys.fetch(input, init),
        ),
      )
      const host = mountAs(placement)
      await settle()
      rerender(
        placement === 'page' ? (
          <div />
        ) : (
          <Account visible={false} onClose={noop} onSignedOut={noop} />
        ),
      )
      await settle()
      // The return draws the lists kept at once, and reads them again.
      rerender(
        placement === 'page' ? (
          <Account placement="page" onSignedOut={noop} />
        ) : (
          <Account visible onClose={noop} onSignedOut={noop} />
        ),
      )
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect([
        host.querySelector('.secret')?.textContent,
        rowListed(host, 'phone'),
      ]).toEqual([expect.stringContaining('tyk_first'), true])

      // Asked as the screen was drawn again, before phone was made.
      await act(async () => {
        returnListing.resolve()
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })
      expect([
        host.querySelector('.secret')?.textContent,
        rowListed(host, 'phone'),
      ]).toEqual([expect.stringContaining('tyk_first'), true])
    })

    it(`draws a made key's box, with its secret, though the listing after its create fails, as a ${placement}`, async () => {
      let reads = 0
      vi.stubGlobal(
        'fetch',
        server({
          'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
          'GET /api/keys': () => {
            reads += 1
            return reads === 1 ? json([]) : new Response(null, { status: 500 })
          },
          '/api/sessions': () => json([]),
        }),
      )
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect(reads).toBe(2)
      expect([
        host.querySelector('.secret')?.textContent,
        keyRegion(host).textContent,
      ]).toEqual([expect.stringContaining('tyk_the-only-copy'), KEY_MADE])
    })

    it(`takes away, with a later listing that lands first, the alert a revocation's own listing would have taken away after it, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')

      await revoke(host, 'Panels')
      await revoke(host, 'Chunky')
      // Asked after Chunky's, so it no longer holds Chunky.
      type(keyName, 'Kobo')
      await submitted(host.querySelector(KEY_FORM))
      expect(alertIn(host)).toMatch(/That key was not revoked/)
      await revokes.landLast(2)
      const drawn = () => [
        alertIn(host),
        rowListed(host, 'Chunky'),
        rowListed(host, 'Kobo'),
        host.querySelector('.secret')?.textContent,
      ]
      const once = [undefined, false, true, expect.stringContaining('tyk_kobo')]
      expect(drawn()).toEqual(once)
      await revokes.land()
      expect(drawn()).toEqual(once)
    })

    it(`draws a made key's box with a later listing that lands first and lists it, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')

      type(keyName, 'Kobo')
      await submitted(host.querySelector(KEY_FORM))
      // Asked after Kobo's, so it lists Kobo too.
      await revoke(host, 'Chunky')
      await revokes.landLast(2)
      const drawn = () => [
        rowListed(host, 'Chunky'),
        rowListed(host, 'Kobo'),
        host.querySelector('.secret')?.textContent,
        keyRegion(host).textContent,
      ]
      const once = [
        false,
        true,
        expect.stringContaining('tyk_kobo'),
        'Key made: Kobo. Copy it now — it is not stored and cannot be shown again.',
      ]
      expect(drawn()).toEqual(once)
      await revokes.land()
      expect(drawn()).toEqual(once)
    })

    it(`runs a made key's box with its own listing, not with an earlier one that lands first and is dropped, so that the box comes with its row and goes when its key is revoked, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')

      await revoke(host, 'Chunky')
      type(keyName, 'Kobo')
      await submitted(host.querySelector(KEY_FORM))
      const drawn = () => [
        rowListed(host, 'Chunky'),
        rowListed(host, 'Kobo'),
        host.querySelector('.secret')?.textContent,
      ]
      // Chunky's two listings, the keys' asked before Kobo was made, so
      // without Kobo; dropped, since Kobo's were asked after them.
      await revokes.landFirst(2)
      expect(drawn()).toEqual([true, false, undefined])
      await revokes.land()
      expect(drawn()).toEqual([
        false,
        true,
        expect.stringContaining('tyk_kobo'),
      ])

      await revoke(host, 'Kobo')
      await revokes.land()
      expect(drawn()).toEqual([false, false, undefined])
    })

    it(`draws a made key's box, with its secret, when a later listing of the keys fails first, and drops its own listing landing after it, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')

      type(keyName, 'Kobo')
      await submitted(host.querySelector(KEY_FORM))
      await revoke(host, 'Chunky')
      const drawn = () => [
        rowListed(host, 'Chunky'),
        rowListed(host, 'Kobo'),
        host.querySelector('.secret')?.textContent,
      ]
      expect(drawn()).toEqual([true, false, undefined])
      // Chunky's listing of the keys, asked after Kobo's, fails first: the
      // newest read of the keys has failed, so the list kept is drawn and
      // the box with it, its key unknown.
      await revokes.failLast(1)
      const once = [true, false, expect.stringContaining('tyk_kobo')]
      expect(drawn()).toEqual(once)
      await revokes.land()
      expect(drawn()).toEqual(once)
    })

    it(`drops a listing of the sessions that lands after a later one, so that an ended session is not listed again, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()

      // Chrome's listing is asked while Edge is still signed in.
      await revoke(host, 'Chrome')
      await revoke(host, 'Edge')
      await revokes.landLast(2)
      expect([rowListed(host, 'Chrome'), rowListed(host, 'Edge')]).toEqual([
        false,
        false,
      ])
      await revokes.land()
      expect([rowListed(host, 'Chrome'), rowListed(host, 'Edge')]).toEqual([
        false,
        false,
      ])
    })
  }
})

/**
 * A server holding the keys Panels (1) and Chunky (2), and the sessions
 * Firefox (this one), Safari, Chrome and Edge. Revoking Panels or Safari
 * fails, as does signing out; revoking Chunky, Chrome or Edge works, and a
 * create makes Kobo (3), whose secret is `tyk_kobo`, and revoking Kobo
 * works. The mount's two listings answer at once; each listing after them
 * waits until `land` answers every one out so far, each with the rows the
 * server held when it was asked, or until `landFirst` or `landLast`
 * answers some of them, or `failLast` answers them with a failure.
 */
function failsSomeRevokes() {
  const key = (id: number, name: string) => ({
    id,
    name,
    created: 'then',
    last_used: null,
  })
  const session = (id: string, agent: string, current: boolean) => ({
    id,
    created: 'then',
    last_seen: 'then',
    user_agent: agent,
    ip: null,
    current,
  })
  let keyRows = [key(1, 'Panels'), key(2, 'Chunky')]
  let sessionRows = [
    session('aa', 'Firefox', true),
    session('bb', 'Safari', false),
    session('cc', 'Chrome', false),
    session('dd', 'Edge', false),
  ]
  const out: Array<(failed: boolean) => void> = []
  let reads = 0
  const listing = (rows: unknown) => {
    reads += 1
    const now = json(rows)
    if (reads <= 2) return Promise.resolve(now)
    return new Promise<Response>((resolve) => {
      out.push((failed) =>
        resolve(failed ? new Response(null, { status: 500 }) : now),
      )
    })
  }
  const fetch = vi.fn((input: string, init?: RequestInit) => {
    const asked = `${init?.method ?? 'GET'} ${input}`
    if (
      asked === 'DELETE /api/keys/1' ||
      asked === 'DELETE /api/sessions/bb' ||
      asked === 'DELETE /api/session'
    )
      return Promise.resolve(new Response(null, { status: 500 }))
    const revokedKey = /^DELETE \/api\/keys\/(2|3)$/.exec(asked)
    if (revokedKey) {
      keyRows = keyRows.filter((each) => each.id !== Number(revokedKey[1]))
      return Promise.resolve(new Response(null, { status: 204 }))
    }
    if (asked === 'POST /api/keys') {
      keyRows = [...keyRows, key(3, 'Kobo')]
      return Promise.resolve(json({ secret: 'tyk_kobo' }))
    }
    const ended = /^DELETE \/api\/sessions\/(cc|dd)$/.exec(asked)
    if (ended) {
      sessionRows = sessionRows.filter((each) => each.id !== ended[1])
      return Promise.resolve(new Response(null, { status: 204 }))
    }
    if (asked === 'GET /api/keys') return listing(keyRows)
    if (asked === 'GET /api/sessions') return listing(sessionRows)
    return Promise.resolve(new Response(null, { status: 404 }))
  })
  const answered = (
    answers: Array<(failed: boolean) => void>,
    failed = false,
  ) =>
    act(async () => {
      for (const answer of answers) answer(failed)
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
  return {
    fetch,
    land: () => answered(out.splice(0)),
    /** Answers the `count` listings asked for first, and no other. */
    landFirst: (count: number) => answered(out.splice(0, count)),
    /** Answers the `count` listings asked for last, and no other. */
    landLast: (count: number) => answered(out.splice(-count)),
    /** Answers the `count` listings asked for last with a 500. */
    failLast: (count: number) => answered(out.splice(-count), true),
  }
}

/**
 * Where focus goes when a Revoke's row goes with focus on it, which would
 * drop it to the page, and out of the panel's dialog: to the nearest row
 * left in the same list, the next one, else the one before, passing over
 * the current session's, whose Revoke is disabled; with no row to go to,
 * to the New key box, or to Signed in's heading. Focus is compared with
 * `toBe`: `toEqual` reads two elements of the same markup, such as two
 * Revokes, as equal.
 */
describe('Account’s focus after a Revoke', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  /** The Revoke in the row of a list that names `name`. */
  const revokeOf = (host: HTMLElement, name: string) => {
    const button = [...host.querySelectorAll('li, .reader-section-head')]
      .filter((row) => (row.textContent ?? '').includes(name))
      .map((row) => row.querySelector('button'))
      .find((each) => each?.textContent === 'Revoke')
    if (!button) throw new Error(`no Revoke for ${name}`)
    return button
  }

  /** Focus a row's Revoke, press it, and let its listing land. */
  const pressRevoke = async (host: HTMLElement, name: string) => {
    const button = revokeOf(host, name)
    act(() => button.focus())
    await act(async () => {
      button.click()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
  }

  /** The heading of the section titled `title`, in either placement. */
  const headingOf = (host: HTMLElement, title: string) =>
    [...host.querySelectorAll('h2, h3')].find(
      (heading) => heading.textContent === title,
    )

  /** Focus is still in the panel's dialog, where there is one. */
  const inDialog = (host: HTMLElement) =>
    host.querySelector('[role="dialog"]')?.contains(document.activeElement) ??
    true

  /**
   * A server whose sessions are Safari, this one (Firefox) and Chrome, in
   * that order, and which ends any but this one at once.
   */
  function endsSessions() {
    const session = (id: string, agent: string, current: boolean) => ({
      id,
      created: 'then',
      last_seen: 'then',
      user_agent: agent,
      ip: null,
      current,
    })
    let listed = [
      session('bb', 'Safari', false),
      session('aa', 'Firefox', true),
      session('cc', 'Chrome', false),
    ]
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const ended = /^\/api\/sessions\/(\w+)$/.exec(input)
        if (init?.method === 'DELETE' && ended) {
          listed = listed.filter((each) => each.id !== ended[1])
          return Promise.resolve(new Response(null, { status: 204 }))
        }
        if (input === '/api/sessions') return Promise.resolve(json(listed))
        return Promise.resolve(json([]))
      }),
    )
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`gives focus to the next key’s Revoke, else the one before’s, and with no key left to the New key box, as a ${placement}`, async () => {
      const keys = listedKeys(
        [
          { id: 1, name: 'Panels' },
          { id: 2, name: 'Chunky' },
          { id: 3, name: 'Tablet' },
        ],
        [],
      )
      vi.stubGlobal('fetch', keys.server)
      const host = mountAs(placement)
      await settle()

      await pressRevoke(host, 'Chunky')
      expect(document.activeElement).toBe(revokeOf(host, 'Tablet'))
      expect(inDialog(host)).toBe(true)
      await pressRevoke(host, 'Tablet')
      expect(document.activeElement).toBe(revokeOf(host, 'Panels'))
      expect(inDialog(host)).toBe(true)
      await pressRevoke(host, 'Panels')
      expect(document.activeElement).toBe(
        host.querySelector('input[placeholder]'),
      )
      expect(inDialog(host)).toBe(true)
      expect(keys.revoked).toEqual([2, 3, 1])
    })

    it(`gives focus to the next session’s Revoke, passing over this one’s, and with none left to Signed in’s heading, as a ${placement}`, async () => {
      endsSessions()
      const host = mountAs(placement)
      await settle()

      await pressRevoke(host, 'Safari')
      expect(document.activeElement).toBe(revokeOf(host, 'Chrome'))
      expect(inDialog(host)).toBe(true)
      await pressRevoke(host, 'Chrome')
      const heading = headingOf(host, 'Signed in')
      expect(document.activeElement).toBe(heading)
      expect(inDialog(host)).toBe(true)
      expect(heading?.getAttribute('tabindex')).toBe('-1')
    })

    it(`passes over a row that goes in the same listing as the one whose Revoke had focus, as a ${placement}`, async () => {
      const keys = madeInTurn()
      const tabletListing = deferred<void>()
      // The mount's listing, the two creates', then the revocations'.
      keys.holdKeyRead(4, tabletListing.promise)
      vi.stubGlobal('fetch', keys.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      type(keyName, 'tablet')
      await submitted(host.querySelector(KEY_FORM))

      // Phone's listing is asked once both are revoked, so both rows go.
      await pressRevoke(host, 'tablet')
      await pressRevoke(host, 'phone')
      expect([rowListed(host, 'phone'), rowListed(host, 'tablet')]).toEqual([
        false,
        false,
      ])
      expect(document.activeElement).toBe(keyName)
    })

    it(`leaves focus where it was put when a Revoke’s row goes once focus has left it, as a ${placement}`, async () => {
      const revokes = failsSomeRevokes()
      vi.stubGlobal('fetch', revokes.fetch)
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')

      await pressRevoke(host, 'Chrome')
      act(() => keyName.focus())
      await revokes.land()
      expect(rowListed(host, 'Chrome')).toBe(false)
      expect(document.activeElement).toBe(keyName)
    })
  }
})

/**
 * A refusal of Create or of Change password (the user's ruling of
 * 2026-10-25) is said in a callout drawn straight after the row it answers
 * (the user's rulings of 2026-10-26), the New key row's or Confirm
 * password's, not above the sections, where it moved the page; it goes
 * when a box of its form is edited, when focus leaves the form, or as the
 * next attempt is sent. One `it()` per placement.
 */
describe('Account’s refusals of Create and Change password', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  /** Create and Change password are refused while `refusing` holds. */
  function refusals() {
    const holds = {
      refusing: true,
      held: undefined as Promise<void> | undefined,
    }
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const asked = `${init?.method ?? 'GET'} ${input}`
        const answer = (status: number, body: string | null) =>
          (holds.held ?? Promise.resolve()).then(() =>
            holds.refusing
              ? new Response(body, { status })
              : new Response(null, { status: 204 }),
          )
        if (asked === 'POST /api/keys') return answer(400, NEEDS_A_NAME)
        if (asked === 'PUT /api/password') return answer(403, null)
        return Promise.resolve(json([]))
      }),
    )
    return holds
  }

  /**
   * The callout a refusal of `form` is said in, if drawn: among the
   * callouts drawn straight after the row it answers, the New key row (on
   * the page, the row in its form; in the panel, the form), or Confirm
   * password's row (in the panel, its label).
   */
  const calloutOf = (host: HTMLElement, form: 'key' | 'password') => {
    const row =
      form === 'key'
        ? (host.querySelector(`${KEY_FORM} > .settings-row`) ??
          host.querySelector(KEY_FORM))
        : host
            .querySelector(`${PASSWORD_FORM} input[aria-label^="Confirm"]`)
            ?.closest('.settings-row, label')
    for (
      let after = row?.nextElementSibling;
      after?.matches('.callout');
      after = after.nextElementSibling
    ) {
      if (after.querySelector(':scope > [role="alert"]') !== null) return after
    }
    return null
  }
  const refusalIn = (host: HTMLElement, form: 'key' | 'password') =>
    calloutOf(host, form)?.querySelector(':scope > [role="alert"]')?.textContent

  for (const placement of ['page', 'panel'] as const) {
    it(`says a refused Create in an alert in a callout drawn after the New key row, and nothing above the sections, as a ${placement}`, async () => {
      refusals()
      const host = mountAs(placement)
      await settle()
      await submitted(host.querySelector(KEY_FORM))
      expect(refusalIn(host, 'key')).toBe(NEEDS_A_NAME)
      expect(
        [...host.querySelectorAll('[role="alert"]')].map((alert) =>
          alert.parentElement?.matches('.callout'),
        ),
      ).toEqual([true])
    })

    it(`says a refused Change password in an alert in a callout drawn after Confirm password’s row, and nothing above the sections, as a ${placement}`, async () => {
      refusals()
      const host = mountAs(placement)
      await settle()
      await submitted(host.querySelector(PASSWORD_FORM))
      expect(refusalIn(host, 'password')).toMatch(
        /current password did not match/,
      )
      expect(
        [...host.querySelectorAll('[role="alert"]')].map((alert) =>
          alert.parentElement?.matches('.callout'),
        ),
      ).toEqual([true])
    })

    it(`takes a refused Create’s callout away when the key’s name is edited, when focus leaves its form and not when it moves inside it, and as the next key is asked for, as a ${placement}`, async () => {
      const holds = refusals()
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      const outside = host.querySelector<HTMLInputElement>(
        'input[type="password"]',
      )
      if (!keyName || !outside) throw new Error('a box is missing')
      const create = buttonSaying(host, 'Create') as HTMLButtonElement

      await submitted(host.querySelector(KEY_FORM))
      type(keyName, 'phone')
      expect(calloutOf(host, 'key')).toBeNull()

      act(() => keyName.focus())
      await submitted(host.querySelector(KEY_FORM))
      act(() => create.focus())
      expect(refusalIn(host, 'key')).toBe(NEEDS_A_NAME)
      act(() => outside.focus())
      expect(calloutOf(host, 'key')).toBeNull()

      await submitted(host.querySelector(KEY_FORM))
      expect(refusalIn(host, 'key')).toBe(NEEDS_A_NAME)
      holds.held = new Promise(() => {})
      await submitted(host.querySelector(KEY_FORM))
      expect(calloutOf(host, 'key')).toBeNull()
    })

    it(`takes a refused Change password’s callout away when any of its boxes is edited, when focus leaves its form and not when it moves inside it, and as the next change is sent, as a ${placement}`, async () => {
      const holds = refusals()
      const host = mountAs(placement)
      await settle()
      const form = host.querySelector(PASSWORD_FORM)
      const boxes = [...(form?.querySelectorAll('input') ?? [])]
      const outside = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (boxes.length !== 3 || !outside) throw new Error('a box is missing')

      const [current, next, confirm] = boxes
      const editTakesItAway = async (box: HTMLInputElement, typed: string) => {
        await submitted(form)
        expect(refusalIn(host, 'password')).toMatch(/did not match/)
        type(box, typed)
        expect(calloutOf(host, 'password')).toBeNull()
      }
      await editTakesItAway(current, 'old')
      await editTakesItAway(next, 'new')
      type(confirm, 'new')
      await editTakesItAway(confirm, 'newer')
      type(next, 'newer')

      act(() => current.focus())
      await submitted(form)
      act(() => confirm.focus())
      expect(refusalIn(host, 'password')).toMatch(/did not match/)
      act(() => outside.focus())
      expect(calloutOf(host, 'password')).toBeNull()

      await submitted(form)
      expect(refusalIn(host, 'password')).toMatch(/did not match/)
      holds.held = new Promise(() => {})
      await submitted(form)
      expect(calloutOf(host, 'password')).toBeNull()
    })
  }

  it('keeps a refused Create’s callout while the window loses focus, and takes it away as focus goes to the page', async () => {
    refusals()
    const host = mountAs('page')
    await settle()
    const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
    if (!keyName) throw new Error('no key name box')
    act(() => keyName.focus())
    await submitted(host.querySelector(KEY_FORM))
    expect(refusalIn(host, 'key')).toBe(NEEDS_A_NAME)
    // Focus leaving for nowhere: the window's, where the document has no
    // focus, and the page's, where it keeps it.
    const blur = (documentHasFocus: boolean) => {
      const hasFocus = vi
        .spyOn(document, 'hasFocus')
        .mockReturnValue(documentHasFocus)
      act(() => {
        keyName.dispatchEvent(
          new FocusEvent('focusout', { bubbles: true, relatedTarget: null }),
        )
      })
      hasFocus.mockRestore()
    }
    blur(false)
    expect(refusalIn(host, 'key')).toBe(NEEDS_A_NAME)
    blur(true)
    expect(calloutOf(host, 'key')).toBeNull()
  })

  it('takes both refusals’ callouts away when the panel closes', async () => {
    refusals()
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await submitted(host.querySelector(KEY_FORM))
    await submitted(host.querySelector(PASSWORD_FORM))
    expect([refusalIn(host, 'key'), refusalIn(host, 'password')]).toEqual([
      NEEDS_A_NAME,
      expect.stringMatching(/did not match/),
    ])
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect([calloutOf(host, 'key'), calloutOf(host, 'password')]).toEqual([
      null,
      null,
    ])
  })

  it('does not say a refusal of Change password in a panel opened again after a close that came while it was out', async () => {
    const holds = refusals()
    const answer = deferred<void>()
    holds.held = answer.promise
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await submitted(host.querySelector(PASSWORD_FORM))
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    await act(async () => {
      answer.resolve()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(calloutOf(host, 'password')).toBeNull()
  })

  /**
   * A server listing key 7, tablet, and session bb, from Safari, that
   * answers this screen's five writes from `writes`.
   */
  function answersWrites(writes: Record<string, () => Response>) {
    vi.stubGlobal(
      'fetch',
      vi.fn((input: string, init?: RequestInit) => {
        const asked = `${init?.method ?? 'GET'} ${input}`
        const answers: Record<string, () => Response> = {
          ...writes,
          'GET /api/keys': () =>
            json([{ id: 7, name: 'tablet', created: 'then', last_used: null }]),
          'GET /api/sessions': () =>
            json([
              {
                id: 'bb',
                created: 'then',
                last_seen: 'then',
                user_agent: 'Safari',
                ip: null,
                current: false,
              },
            ]),
        }
        return Promise.resolve(
          answers[asked]?.() ?? new Response(null, { status: 404 }),
        )
      }),
    )
  }

  /** The five writes: Create, Change password, the two Revokes, Sign out. */
  const WRITES = [
    'POST /api/keys',
    'PUT /api/password',
    'DELETE /api/keys/7',
    'DELETE /api/sessions/bb',
    'DELETE /api/session',
  ]

  /**
   * A server that refuses, in its own words, a Create with no name, a
   * Change password to an empty password, the revocation of key 7 and of
   * session bb, which it does not hold, and a sign-out from another origin.
   */
  function refusesInWords() {
    answersWrites({
      'POST /api/keys': () => new Response(NEEDS_A_NAME, { status: 400 }),
      'PUT /api/password': () =>
        new Response(
          'an empty password would lock the account rather than open it',
          { status: 400 },
        ),
      'DELETE /api/keys/7': () => new Response('no such key', { status: 404 }),
      'DELETE /api/sessions/bb': () =>
        new Response('no such session', { status: 404 }),
      'DELETE /api/session': () =>
        new Response(
          JSON.stringify({
            error:
              "this request's Origin does not match this server; a session can only be opened or closed from the reader's own origin",
          }),
          { status: 403, headers: { 'content-type': 'application/json' } },
        ),
    })
  }

  /** A proxy's own page for an upstream that did not answer. */
  const PROXY_PAGE =
    '<html>\r\n<head><title>502 Bad Gateway</title></head>\r\n<body>\r\n<center><h1>502 Bad Gateway</h1></center>\r\n</body>\r\n</html>\r\n'

  /** A server behind a proxy that answers each write with its page. */
  function failsBehindAProxy() {
    answersWrites(
      Object.fromEntries(
        WRITES.map((write) => [
          write,
          () =>
            new Response(PROXY_PAGE, {
              status: 502,
              headers: { 'content-type': 'text/html' },
            }),
        ]),
      ),
    )
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`says a refused Create and a refused Change password in the server’s words, with no address or status, as a ${placement}`, async () => {
      refusesInWords()
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, '   ')
      await submitted(host.querySelector(KEY_FORM))
      expect(refusalIn(host, 'key')).toBe(NEEDS_A_NAME)

      const current = host.querySelector<HTMLInputElement>(
        'input[autocomplete="current-password"]',
      )
      if (!current) throw new Error('no current password box')
      type(current, 'the old one')
      await submitted(host.querySelector(PASSWORD_FORM))
      expect(refusalIn(host, 'password')).toBe(
        'an empty password would lock the account rather than open it',
      )
    })

    it(`says a failed revocation of a key or a session, and a failed sign-out, by what was not done and in the server’s words, as a ${placement}`, async () => {
      refusesInWords()
      const host = mountAs(placement)
      await settle()
      await revoke(host, 'tablet')
      expect(alertIn(host)).toBe('That key was not revoked: no such key')
      await revoke(host, 'Safari')
      expect(alertIn(host)).toBe('That session was not ended: no such session')
      const signOut = buttonSaying(host, 'Sign out')
      if (!signOut) throw new Error('no Sign out')
      await act(async () => {
        signOut.click()
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })
      expect(alertIn(host)).toBe(
        "You were not signed out: this request's Origin does not match this server; a session can only be opened or closed from the reader's own origin",
      )
    })
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`says a write answered with a page of markup by what was not done and the status, and never the page, as a ${placement}`, async () => {
      failsBehindAProxy()
      const host = mountAs(placement)
      await settle()
      const keyName = host.querySelector<HTMLInputElement>('input[placeholder]')
      if (!keyName) throw new Error('no key name box')
      type(keyName, 'phone')
      await submitted(host.querySelector(KEY_FORM))
      expect(refusalIn(host, 'key')).toBe(
        'The key was not made: the server answered 502',
      )

      const current = host.querySelector<HTMLInputElement>(
        'input[autocomplete="current-password"]',
      )
      if (!current) throw new Error('no current password box')
      type(current, 'the old one')
      await submitted(host.querySelector(PASSWORD_FORM))
      expect(refusalIn(host, 'password')).toBe(
        'The password was not changed: the server answered 502',
      )

      await revoke(host, 'tablet')
      expect(alertIn(host)).toBe(
        'That key was not revoked: the server answered 502',
      )
      await revoke(host, 'Safari')
      expect(alertIn(host)).toBe(
        'That session was not ended: the server answered 502',
      )
      const signOut = buttonSaying(host, 'Sign out')
      if (!signOut) throw new Error('no Sign out')
      await act(async () => {
        signOut.click()
        await new Promise((resolve) => {
          setTimeout(resolve, 0)
        })
      })
      expect(alertIn(host)).toBe(
        'You were not signed out: the server answered 502',
      )
    })
  }

  it('does not say a refusal of Create in a panel opened again after a close that came while it was out', async () => {
    const holds = refusals()
    const answer = deferred<void>()
    holds.held = answer.promise
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await submitted(host.querySelector(KEY_FORM))
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    await act(async () => {
      answer.resolve()
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    expect(calloutOf(host, 'key')).toBeNull()
  })
})

/**
 * Confirm password (the user's rulings of 2026-10-25): Change password is
 * held while New password and Confirm password differ, and a callout laid
 * over the form at Confirm password says so whenever the confirm is neither
 * empty nor the start of the new one, wherever focus is, and after a press
 * of the held button (the user's ruling of 2026-10-26). One `it()` per
 * placement: `testing/mount.ts` holds one root.
 */
describe('Account’s Confirm password', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  /** A server that answers a password change, and counts each one sent. */
  function changes() {
    vi.stubGlobal(
      'fetch',
      server({
        '/api/keys': () => json([]),
        '/api/sessions': () => json([]),
        'PUT /api/password': () => new Response(null, { status: 204 }),
      }),
    )
    const fetch = vi.mocked(globalThis.fetch)
    return {
      sent: () =>
        fetch.mock.calls
          .filter(([, init]) => init?.method === 'PUT')
          .map(([, init]) => JSON.parse(String(init?.body))),
    }
  }

  /** The password form's three boxes and Change password, in each placement. */
  async function passwordForm(placement: 'page' | 'panel') {
    const host = mountAs(placement)
    await settle()
    const form = host.querySelector<HTMLFormElement>(PASSWORD_FORM)
    if (!form) throw new Error('no password form')
    const [current, next, confirm] = form.querySelectorAll('input')
    const change = buttonSaying(host, 'Change password')
    if (!current || !next || !confirm || !change) {
      throw new Error('the password form is missing a part')
    }
    return { host, form, current, next, confirm, change }
  }

  /**
   * The status region in the callout drawn straight after Confirm
   * password's row, or its label in the panel.
   */
  const lineAfter = (confirm: HTMLInputElement) => {
    const callout = confirm.closest('.settings-row, label')?.nextElementSibling
    expect(callout?.className).toBe('callout')
    const region = callout?.querySelector(':scope > [role="status"]')
    if (!(region instanceof HTMLElement)) throw new Error('no status region')
    return region
  }

  for (const placement of ['page', 'panel'] as const) {
    it(`holds Change password, by aria-disabled, while New password and Confirm password differ, and frees it when they match, as a ${placement}`, async () => {
      changes()
      const { next, confirm, change } = await passwordForm(placement)
      expect(holdOf(change)).toBe('free')
      type(next, 'hunter22')
      expect(holdOf(change)).toBe('held')
      type(confirm, 'hunter2')
      expect(holdOf(change)).toBe('held')
      type(confirm, 'hunter22')
      expect(holdOf(change)).toBe('free')
      type(confirm, 'hunter222')
      expect(holdOf(change)).toBe('held')
      type(next, 'hunter222')
      expect(holdOf(change)).toBe('free')
    })

    it(`says the two do not match for a press of Change password while they differ, the confirm empty or the start of the password, until a box is edited, and sends nothing, as a ${placement}`, async () => {
      const sends = changes()
      const { form, current, next, confirm, change } =
        await passwordForm(placement)
      const region = lineAfter(confirm)
      const press = () =>
        act(async () => {
          change.click()
          await new Promise((resolve) => {
            setTimeout(resolve, 0)
          })
        })
      type(current, 'the old one')
      type(next, 'hunter22')
      expect(region.textContent).toBe('')
      await press()
      expect([
        region.textContent,
        confirm.getAttribute('aria-invalid'),
        confirm.getAttribute('aria-describedby'),
      ]).toEqual(['The two passwords do not match.', 'true', region.id])

      type(confirm, 'hunter')
      expect(region.textContent).toBe('')
      await press()
      expect(region.textContent).toBe('The two passwords do not match.')
      type(confirm, 'hunter2')
      expect(region.textContent).toBe('')
      // Back to what the press found: an edit ended what it said.
      type(confirm, 'hunter')
      expect(region.textContent).toBe('')

      // A submit that reaches the form another way says it too.
      await submitted(form)
      expect(region.textContent).toBe('The two passwords do not match.')
      expect(sends.sent()).toEqual([])
    })

    it(`says the two do not match in a callout under Confirm password’s row, in a status region drawn empty before it, for a typo and a longer confirm and not for the start of the password, as a ${placement}`, async () => {
      changes()
      const { next, confirm } = await passwordForm(placement)
      const region = lineAfter(confirm)
      expect(region.textContent).toBe('')
      type(next, 'hunter22')
      act(() => confirm.focus())
      const said = (typed: string) => {
        type(confirm, typed)
        expect(lineAfter(confirm)).toBe(region)
        return region.textContent
      }
      expect(said('hun')).toBe('')
      expect(said('hunx')).toBe('The two passwords do not match.')
      expect(said('hunter22')).toBe('')
      expect(said('hunter222')).toBe('The two passwords do not match.')
      expect(said('')).toBe('')
    })

    it(`says the two do not match while they differ, wherever focus is, as a ${placement}`, async () => {
      changes()
      const { current, next, confirm } = await passwordForm(placement)
      const region = lineAfter(confirm)
      // Filled while focus is on none of them, as a password manager fills.
      type(next, 'hunter22')
      type(confirm, 'hunted')
      expect(region.textContent).toBe('The two passwords do not match.')
      for (const box of [confirm, next, current]) {
        act(() => box.focus())
        expect(region.textContent, box.getAttribute('aria-label') ?? '').toBe(
          'The two passwords do not match.',
        )
      }
      act(() => current.blur())
      expect(region.textContent).toBe('The two passwords do not match.')
      type(confirm, 'hunter')
      expect(region.textContent).toBe('')
    })

    it(`names the line as Confirm password’s description, and marks the box invalid, only while the line is said, as a ${placement}`, async () => {
      changes()
      const { next, confirm } = await passwordForm(placement)
      const region = lineAfter(confirm)
      act(() => confirm.focus())
      const marks = () => [
        confirm.getAttribute('aria-invalid'),
        confirm.getAttribute('aria-describedby'),
      ]
      type(next, 'hunter22')
      type(confirm, 'hunter')
      expect(marks()).toEqual([null, null])
      type(confirm, 'hunted')
      expect(region.id).not.toBe('')
      expect(marks()).toEqual(['true', region.id])
      type(confirm, 'hunter22')
      expect(marks()).toEqual([null, null])
    })

    it(`sends nothing for a submit while the two differ, and sends the new password once they match, as a ${placement}`, async () => {
      const sends = changes()
      const { form, current, next, confirm } = await passwordForm(placement)
      type(current, 'the old one')
      type(next, 'hunter22')
      type(confirm, 'hunter2')
      await submitted(form)
      type(confirm, 'hunter23')
      await submitted(form)
      expect(sends.sent()).toEqual([])
      type(confirm, 'hunter22')
      await submitted(form)
      expect(sends.sent()).toEqual([
        { current: 'the old one', next: 'hunter22' },
      ])
    })

    it(`keeps focus on Change password when a change to the boxes holds it, as a ${placement}`, async () => {
      changes()
      const { next, confirm, change } = await passwordForm(placement)
      type(next, 'hunter22')
      type(confirm, 'hunter22')
      act(() => change.focus())
      expect(document.activeElement).toBe(change)
      // A box filled while Change password has focus, as a password
      // manager may fill one.
      type(confirm, 'hunter2')
      expect(holdOf(change)).toBe('held')
      expect(document.activeElement).toBe(change)
    })

    it(`empties New password and Confirm password, with Current password, once a change works, as a ${placement}`, async () => {
      const sends = changes()
      const { form, current, next, confirm } = await passwordForm(placement)
      type(current, 'the old one')
      type(next, 'hunter22')
      type(confirm, 'hunter22')
      await submitted(form)
      expect(sends.sent()).toHaveLength(1)
      expect([current.value, next.value, confirm.value]).toEqual(['', '', ''])
    })
  }
})

/**
 * What the screen reads before it draws its body: the keys, the sessions
 * and `/api/about`, as one set (spec 2026-10-27 §5). The body is drawn once
 * all three are in, in one step, so a list still being read is never drawn
 * as an empty one; the page draws Loading… in its place after 200 ms and
 * the panel draws no wait line; a panel opened again draws the lists kept
 * at once; and one `/api/about` is sent for the screen and the build label
 * beside it, and for About and the label.
 */
describe('Account’s reads', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  const KEY_ROWS = [{ id: 1, name: 'Panels', created: 'then', last_used: null }]
  const SESSION_ROWS = [
    {
      id: 'aa',
      created: 'then',
      last_seen: 'today',
      user_agent: 'Firefox',
      ip: null,
      current: true,
    },
  ]

  /**
   * A server whose first listing of each list waits for the case to answer
   * it, and whose later ones answer at once; `/api/about` answers at once.
   */
  function heldFirstListings() {
    const keys = deferred<void>()
    const sessions = deferred<void>()
    const asked = { keys: 0, sessions: 0 }
    const fetch = vi.fn((input: string) => {
      if (input === '/api/keys') {
        asked.keys += 1
        const now = json(KEY_ROWS)
        return asked.keys === 1 ? keys.promise.then(() => now) : now
      }
      if (input === '/api/sessions') {
        asked.sessions += 1
        const now = json(SESSION_ROWS)
        return asked.sessions === 1 ? sessions.promise.then(() => now) : now
      }
      if (input === '/api/about') return Promise.resolve(json(ABOUT))
      return Promise.resolve(new Response(null, { status: 404 }))
    })
    vi.stubGlobal('fetch', fetch)
    return { keys, sessions, fetch }
  }

  /** The rows each list draws, by the name each row starts with. */
  const lists = (host: HTMLElement) =>
    JSON.stringify({
      keys: rowListed(host, 'Panels'),
      sessions: listsBrowser(host, 'Firefox'),
      body: sectionTitles(host),
    })

  /** Whether Signed in lists a session from `browser`, with its Revoke. */
  const listsBrowser = (host: HTMLElement, browser: string) =>
    [...host.querySelectorAll('li, .reader-section-head')].some(
      (row) =>
        (row.textContent ?? '').includes(browser) &&
        row.querySelector('button')?.textContent === 'Revoke',
    )

  /** The title of each section drawn, in either placement. */
  const sectionTitles = (host: HTMLElement) =>
    [
      ...host.querySelectorAll(
        '.settings-section-title, .reader-panel-heading',
      ),
    ].map((title) => title.textContent)

  const NOTHING = JSON.stringify({ keys: false, sessions: false, body: [] })
  const EVERYTHING = JSON.stringify({
    keys: true,
    sessions: true,
    body: ['Password', 'API keys', 'Signed in'],
  })

  for (const placement of ['page', 'panel'] as const) {
    for (const first of ['keys', 'sessions'] as const) {
      it(`draws no body while either list is unread, the ${first} landing first, and both lists in one step once both land, as a ${placement}`, async () => {
        const held = heldFirstListings()
        const host = mountAs(placement)
        await settle()
        const drawn = watchDrawn(() => lists(host))
        await flush(() => {
          held[first].resolve()
        })
        expect(lists(host)).toBe(NOTHING)
        await flush(() => {
          held[first === 'keys' ? 'sessions' : 'keys'].resolve()
        })
        drawn.stop()
        expect([...drawn.seen]).toEqual([NOTHING, EVERYTHING])
      })
    }
  }

  it('heads the page at once, and draws Loading… in its body’s place from 200 ms until its reads land', async () => {
    holdTheClock()
    const held = heldFirstListings()
    const host = mountAs('page')
    await settle()
    const page = pageScreen(host)
    const parts = () =>
      [...page.children].map((part) =>
        part.matches('[role="status"]') ? part.textContent : part.className,
      )
    expect(page.querySelector('.page-title')?.textContent).toBe('Account')
    expect(parts()).toEqual(['page-header'])
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(parts()).toEqual(['page-header'])
    pass(1)
    expect(parts()).toEqual(['page-header', 'Loading…'])
    await flush(() => {
      held.keys.resolve()
      held.sessions.resolve()
    })
    expect(lists(host)).toBe(EVERYTHING)
    expect(parts()).not.toContain('Loading…')
  })

  it('draws no Loading… in the panel, whose head is drawn at once', async () => {
    holdTheClock()
    heldFirstListings()
    const host = mountAs('panel')
    await settle()
    const panel = host.querySelector('[role="dialog"]')
    if (!panel) throw new Error('no panel drawn')
    /** Each part of the panel's head, and each status in the panel, by its words. */
    const drawn = () =>
      JSON.stringify({
        head: [...panel.querySelectorAll('.reader-panel-head > *')].map(
          (part) => part.textContent,
        ),
        statuses: [...panel.querySelectorAll('[role="status"]')].map(
          (status) => status.textContent,
        ),
      })
    const headAlone = JSON.stringify({
      head: ['Account', 'Close'],
      statuses: [],
    })
    expect(drawn()).toBe(headAlone)
    expect(lists(host)).toBe(NOTHING)
    pass(WAIT_DRAWN_AFTER_MS)
    expect(drawn()).toBe(headAlone)
    expect(lists(host)).toBe(NOTHING)
  })

  it('draws its lists at once when the panel is opened again, and reads them again', async () => {
    const held = heldFirstListings()
    const host = mount(<Account visible onClose={noop} onSignedOut={noop} />)
    await settle()
    await flush(() => {
      held.keys.resolve()
      held.sessions.resolve()
    })
    rerender(<Account visible={false} onClose={noop} onSignedOut={noop} />)
    await settle()
    const asked = held.fetch.mock.calls.length
    held.fetch.mockImplementation(() => new Promise<Response>(() => {}))

    rerender(<Account visible onClose={noop} onSignedOut={noop} />)
    expect(lists(host)).toBe(EVERYTHING)
    await settle()
    expect(
      held.fetch.mock.calls.slice(asked).map(([address]) => address),
    ).toEqual(['/api/keys', '/api/sessions', '/api/about'])
  })

  it('sends one /api/about between the page and the build label drawn beside it', async () => {
    const fetchMock = server({
      '/api/keys': () => json([]),
      '/api/sessions': () => json([]),
      '/api/about': () => json(ABOUT),
    })
    vi.stubGlobal('fetch', fetchMock)
    const host = mount(
      <MemoryRouter>
        <VersionLabel />
        <Account placement="page" onSignedOut={noop} />
      </MemoryRouter>,
    )
    await settle()
    await flush()
    expect(host.querySelector('.version-label a')?.textContent).toBe('5ee5159')
    expect(afterOpdsLine(host)?.querySelector('code')?.textContent).toBe(
      `${location.origin}${ABOUT.catalog}`,
    )
    expect(
      fetchMock.mock.calls.filter(([address]) => address === '/api/about'),
    ).toHaveLength(1)
  })

  it('sends one /api/about between About and the build label drawn beside it', async () => {
    const fetchMock = server({ '/api/about': () => json(ABOUT) })
    vi.stubGlobal('fetch', fetchMock)
    const host = mount(
      <MemoryRouter>
        <VersionLabel />
        <About />
      </MemoryRouter>,
    )
    await settle()
    await flush()
    expect(host.querySelector('.version-label a')?.textContent).toBe('5ee5159')
    expect(host.querySelector('.about-facts')).not.toBeNull()
    expect(
      fetchMock.mock.calls.filter(([address]) => address === '/api/about'),
    ).toHaveLength(1)
  })

  /** The catalog's address, the page's first row under API keys. */
  const afterOpdsLine = (host: HTMLElement) =>
    host.querySelector('.account .catalog-address')
})

/**
 * **The page placement holds no colour and no `opacity`, and that is the half
 * of this screen a stylesheet cannot be asked about.** `/account`'s quiet text
 * was four inline `opacity` dims — 0.8, 0.85 and two at 0.6 — and the pair at
 * 0.6 computed **4.18:1 on Paper** at 16 px, under AA's 4.5 for normal text;
 * the other two clear it at 7.88 and 9.27. Computed on 2026-09-24 from
 * `tokens.css`'s `--text` blended over `--bg`, which is what a composited
 * `opacity` resolves to. `admin.css`'s own `.proposal-detail` comment has
 * banned opacity dimming on these screens since C3, and ⑤b spent a PR
 * emptying the last nine from the stylesheet — nine is the count, and
 * `admin.css` holds none today — but a `style=` prop is somewhere no rule in
 * that stylesheet can reach, which is why this assertion is over the
 * component's source rather than over the CSS.
 */
describe('Account.tsx carries no inline style', () => {
  /**
   * **Not vacuous, stated first.** The two assertions in the case below are
   * text searches over this same `?raw` read, and an empty string satisfies
   * each of them — a stubbed import would leave both measuring nothing
   * rather than the component's source. `admin-account.css.test.ts`'s
   * `reads admin.css at all` is the same guard for its own `?raw` import,
   * for the same reason; this is `Account.tsx`'s. Probed on 2026-09-24 under
   * `jsdom`, with a control line, before writing this: unlike `admin.css?raw`
   * (stubbed to `''` by vitest's default `css: false`), a `.tsx?raw` import
   * is not a CSS import and arrives whole — this case is a robustness guard
   * against that ever changing, not a fix for something failing today.
   */
  it('reads Account.tsx at all', async () => {
    const source = await import('./Account.tsx?raw').then((m) => m.default)
    expect(source).toContain('export function Account(')
  })

  it('has no `style=` prop and no `opacity`', async () => {
    const source = await import('./Account.tsx?raw').then((m) => m.default)
    const code = source.replace(/\/\*[\s\S]*?\*\/|\/\/[^\n]*/g, ' ')
    expect(code.match(/style=\{/g) ?? []).toEqual([])
    expect(code.match(/\bopacity\b/g) ?? []).toEqual([])
  })
})

/**
 * An administrator's own row in the users list carries their keys and
 * sessions, so each write here changes what Users draws. It marks that list
 * stale, so Users arriving afterwards waits for it rather than drawing the
 * keys and sessions from before the write.
 */
describe('Account, and the users list Users draws', () => {
  const ADMIN: Me = {
    user_id: 1,
    admin: true,
    can_download: true,
    can_stream: true,
    can_stamp: false,
  }
  const me = {
    id: 1,
    name: 'davey',
    role: 'admin',
    can_download: true,
    can_stream: true,
    can_stamp: false,
    can_log_in: true,
    sessions: [],
    keys: [],
    every_library: true,
    libraries: [],
  }
  const session = (id: string, agent: string, current: boolean) => ({
    id,
    created: 'then',
    last_seen: 'today',
    user_agent: agent,
    ip: null,
    current,
  })

  /**
   * A server that answers each write these cases make on `/account`, and
   * the two lists Users reads; `holdUsers` keeps every later read of the
   * users list until the case lets it answer.
   */
  function accountsServer() {
    const routes: Record<string, () => Response> = {
      'GET /api/keys': () =>
        json([{ id: 7, name: 'phone', created: 'then', last_used: null }]),
      'GET /api/sessions': () =>
        json([session('aa', 'Firefox', true), session('bb', 'Safari', false)]),
      'GET /api/about': () => json(ABOUT),
      'POST /api/keys': () => json({ secret: 'tyk_the-only-copy' }),
      'DELETE /api/keys/7': () => new Response(null, { status: 204 }),
      'DELETE /api/sessions/bb': () => new Response(null, { status: 204 }),
      'PUT /api/password': () => new Response(null, { status: 204 }),
      'GET /api/library/users': () => json({ users: [me] }),
      'GET /api/library/libraries': () => json({ libraries: [] }),
    }
    const held: (() => void)[] = []
    let holding = false
    const fetch = vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      const answer = routes[asked]
      if (answer === undefined)
        return Promise.resolve(new Response(null, { status: 404 }))
      if (holding && asked === 'GET /api/library/users')
        return new Promise<Response>((resolve) => {
          held.push(() => resolve(answer()))
        })
      return Promise.resolve(answer())
    })
    return {
      fetch,
      holdUsers: () => {
        holding = true
      },
      letUsersAnswer: () => {
        for (const one of held.splice(0)) one()
      },
      usersReads: () =>
        fetch.mock.calls.filter(([address]) => address === '/api/library/users')
          .length,
    }
  }

  /** Users drawn in a root of its own, as a Configuration tab. */
  async function usersDrawn() {
    const host = mount(
      <MemoryRouter initialEntries={['/config/users']}>
        <GateContext value={{ who: ADMIN, signOut: noop }}>
          <Routes>
            <Route path="/config/users/*" element={<Users />} />
          </Routes>
        </GateContext>
      </MemoryRouter>,
    )
    await settle()
    return host
  }

  /** Whether the table's rows are drawn. */
  const tableDrawn = (host: HTMLElement) =>
    host.querySelector('.user-table tbody tr') !== null

  /**
   * Users drawn and left, so the store keeps what it read; then `/account`,
   * where `write` is done; then Users again with the users list's read held.
   * What Users draws on arrival, its wait line once the wait has held, the
   * reads of the list the return sent, and whether the table is drawn once
   * the read lands.
   */
  async function usersAfter(write: (host: HTMLElement) => Promise<void>) {
    const answers = accountsServer()
    vi.stubGlobal('fetch', answers.fetch)
    const before = await usersDrawn()
    await flush()
    expect(tableDrawn(before)).toBe(true)
    rerender(<div />)
    const account = mount(<Account placement="page" onSignedOut={noop} />)
    await settle()
    await flush()
    expect(rowListed(account, 'phone')).toBe(true)
    await write(account)
    rerender(<div />)
    holdTheClock()
    answers.holdUsers()
    const readsBefore = answers.usersReads()
    const host = await usersDrawn()
    const onArrival = tableDrawn(host)
    pass(WAIT_DRAWN_AFTER_MS)
    const wait =
      host.querySelector('.user-table > p[role="status"]')?.textContent ?? null
    const reads = answers.usersReads() - readsBefore
    await flush(() => answers.letUsersAnswer())
    return { onArrival, wait, reads, landed: tableDrawn(host) }
  }

  it.each([
    [
      'a key is made',
      async (host: HTMLElement) => {
        await submitted(host.querySelector(KEY_FORM))
        expect(host.querySelector('.secret')?.textContent).toContain(
          'tyk_the-only-copy',
        )
      },
    ],
    [
      'a key is revoked',
      async (host: HTMLElement) => {
        await revoke(host, 'phone')
        expect(alertIn(host)).toBeUndefined()
      },
    ],
    [
      'a session is revoked',
      async (host: HTMLElement) => {
        await revoke(host, 'Safari')
        expect(alertIn(host)).toBeUndefined()
      },
    ],
    [
      'the password is changed',
      async (host: HTMLElement) => {
        await submitted(host.querySelector(PASSWORD_FORM))
        expect(passwordRegion(host).textContent).toBe(PASSWORD_CHANGED)
      },
    ],
  ] as const)(
    'makes Users wait for the users list on a return after %s',
    async (_, write) => {
      expect(await usersAfter(write)).toEqual({
        onArrival: false,
        wait: 'Loading…',
        reads: 1,
        landed: true,
      })
    },
  )

  it('draws Users at once on a return after a visit to /account that wrote nothing', async () => {
    expect(await usersAfter(async () => {})).toEqual({
      onArrival: true,
      wait: null,
      reads: 1,
      landed: true,
    })
  })
})
