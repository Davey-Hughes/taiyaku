import { act } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { forgetAbout } from './about'
import { Account } from './Account'
import { ABOUT } from './testing/about'
import { clipboardContext } from './testing/clipboard'
import { fields } from './testing/fields'
import { mount, rerender } from './testing/mount'

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

describe('Account', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    // `about()` holds an answer for the page's life, and the servers here
    // that answer every GET with a list would hand the next test that.
    forgetAbout()
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
   * says — so on a paper `--bg` the "Password changed" confirmation would be
   * `--success`'s dark green on white. The check runs after a change, with
   * the notice on screen.
   *
   * **The sweep is for any `style` attribute, not only one naming a colour.**
   * Until ⑤c `LOOKS.page` was `{}` beside a panel half made of
   * `CSSProperties`, and the layout-only objects the two placements shared —
   * `rowStyle`, the headings, the small print — reached this page too, so a
   * colour filter was the only thing that could be asserted. The panel half
   * is class names now and those objects are gone, which leaves the page
   * carrying no inline style at all; Task 6 fills `LOOKS.page`, and this is
   * what says it did so with classes.
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
    expect(host.querySelector('[role="dialog"]')).toBeNull()
    expect(buttonSaying(host, 'Close')).toBeUndefined()
    // The key list loaded: its one row's Revoke is the only one drawn.
    expect(buttonSaying(host, 'Revoke')).toBeDefined()

    const current = host.querySelector<HTMLInputElement>(
      'input[autocomplete="current-password"]',
    )
    const next = host.querySelector<HTMLInputElement>(
      'input[autocomplete="new-password"]',
    )
    if (!current || !next) throw new Error('missing password fields')
    type(current, 'old')
    type(next, 'new')
    await act(async () => {
      current.form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    const notice = [...screen.querySelectorAll('p')].find((p) =>
      (p.textContent ?? '').startsWith('Password changed'),
    )
    expect(notice).toBeDefined()

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
  })

  /**
   * **Every key in `LOOKS.page`, pinned the way `LOOKS.panel` is below.**
   * Deleting `section: 'account-card'` from `LOOKS.page` leaves `/account`
   * with no cards at all, and every other gate stays green: the stylesheet
   * test in `admin-account.css.test.ts` reads `admin.css`, not the component,
   * so the rule still exists whether or not anything wears it;
   * `Account.browser.test.tsx`'s page case measures the key-name box, not the
   * card; and neither the no-inline-style case above nor the secret case
   * looks at it. `heading`, `wide`, `signOut`, `failure` and `note` sit in the
   * same position — nothing else in this file names them for the page. This
   * pins all six, following the panel's shape below rather than a different
   * one.
   */
  it('paints every part of the page with admin.css’s classes', async () => {
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

    // `look.section`, `look.heading`: one of each per section. `.account >`,
    // not a bare `section`: the page wrapper itself is a `<section
    // className="account">`, and a bare query counts it as a fourth.
    expect(
      [...host.querySelectorAll('.account > section')].map(
        (el) => el.className,
      ),
    ).toEqual(['account-card', 'account-card', 'account-card'])
    expect([...host.querySelectorAll('h2')].map((el) => el.className)).toEqual([
      'account-heading',
      'account-heading',
      'account-heading',
    ])

    // `look.wide` and `look.signOut`, on the two section verbs — pinned
    // separately, as the panel does below, because they are two keys even
    // though both hold `'account-wide'` today.
    expect(buttonSaying(host, 'Change password')?.className).toBe(
      'account-wide',
    )
    expect(buttonSaying(host, 'Sign out')?.className).toBe('account-wide')

    // `look.note`, on the confirmation a password change leaves behind.
    await act(async () => {
      host
        .querySelector('form')
        ?.dispatchEvent(
          new Event('submit', { bubbles: true, cancelable: true }),
        )
      await Promise.resolve()
    })
    await settle()
    const notice = [...host.querySelectorAll('p')].find((el) =>
      (el.textContent ?? '').startsWith('Password changed'),
    )
    expect(notice?.className).toBe('account-note')

    // `look.failure`. A revocation the server refuses, so the alert appears
    // beside the confirmation rather than in place of it.
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

    // **`look.meta` and `look.row`, the two keys the first pass left out.**
    // `meta` is not cosmetic: `.why` is what buys `--muted` at
    // `--text-meta`, and without it these four paragraphs render `--text` at
    // 16 px — which is the 4.18:1 on Paper that ⑤c exists to fix. Deleting
    // the key restores the defect with every other gate green, because
    // `admin-account.css.test.ts` reads the stylesheet and the rule is
    // still there; only the class stops being applied.
    expect([...host.querySelectorAll('.why')].map((el) => el.tagName)).toEqual([
      'P',
      'SPAN',
      'SPAN',
    ])

    // `look.row` on the create form and on each key and session row. Asserted
    // by count rather than per element: the three are one key in the map and
    // a mutation takes all of them at once.
    expect(host.querySelectorAll('.account-row')).toHaveLength(3)
  })

  /**
   * **Every key in `LOOKS.panel`, pinned where the browser file cannot reach
   * it.** `Account.browser.test.tsx` measures the panel's own box — corners,
   * shadow, width, body size, the scrim — and sweeps the rendered tree for
   * inline style. What it does not distinguish is *which* class carries a
   * part: an `<input>` reads 14px from `.reader-dialog-panel :where(input,
   * select) { font: inherit }` whether or not `look.field` is on it, so
   * dropping `field` from the map entirely leaves that file green. And the
   * secret, the failure and the confirmation are drawn only after an action,
   * so no fixture that merely mounts the panel ever sees them.
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

    // `look.field` on all three boxes, `look.wide`/`look.signOut` on the two
    // section verbs, `look.button` on the row-level ones.
    expect([...host.querySelectorAll('input')].map((el) => el.className)) //
      .toEqual([
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
    expect(secret?.querySelector('p')?.className).toBe('reader-meta')

    // `look.note`, on the confirmation a password change leaves behind.
    await act(async () => {
      host
        .querySelector('form')
        ?.dispatchEvent(
          new Event('submit', { bubbles: true, cancelable: true }),
        )
      await Promise.resolve()
    })
    await settle()
    const notice = [...host.querySelectorAll('p')].find((el) =>
      (el.textContent ?? '').startsWith('Password changed'),
    )
    expect(notice?.className).toBe('reader-success')

    // `look.failure`. A revocation the server refuses, so the alert appears
    // beside the confirmation rather than in place of it.
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

    expect(host.textContent).toContain('tyk_the-only-copy')
    expect(host.textContent).toMatch(/cannot be shown again/)
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
        'POST /api/keys': () => new Response(null, { status: 400 }),
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
    const [current, next, keyName] = inputs
    if (!current || !next || !keyName) throw new Error('missing fields')
    type(current, 'the old one')
    type(next, 'the new one')
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
    expect(host.textContent).toContain('phone')
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
    const [current, , keyName] = inputs
    if (!current || !keyName) throw new Error('missing fields')
    type(current, 'my password')

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
    expect(reopened[0]?.value).toBe('')
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

  // Both placements. Only the page's labels are `.field`s, and the panel's
  // boxes carry the same `aria-label` all the same. One `it()` each:
  // `testing/mount.ts` holds one root.
  for (const placement of ['page', 'panel'] as const) {
    it(`gives each password box its label’s words as its aria-label, as a ${placement} — O175`, async () => {
      vi.stubGlobal(
        'fetch',
        server({
          '/api/keys': () => json([]),
          '/api/sessions': () => json([]),
        }),
      )
      const host = mount(
        placement === 'page' ? (
          <Account placement="page" onSignedOut={noop} />
        ) : (
          <Account visible onClose={noop} onSignedOut={noop} />
        ),
      )
      await settle()
      // Current and New — the screen's only two labels. Counted, because a
      // loop over nothing passes.
      const labelled = fields(host.querySelectorAll('label'))
      expect(labelled).toHaveLength(2)
      for (const { written, control } of labelled) {
        expect(control.getAttribute('type'), written).toBe('password')
        expect(control.getAttribute('aria-label'), written).toBe(written)
      }
    })
  }

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

    // One `it()` per placement: `testing/mount.ts` holds one root.
    for (const placement of ['page', 'panel'] as const) {
      it(`draws it straight after the OPDS line, as a ${placement}`, async () => {
        vi.stubGlobal('fetch', answered())
        const host = placed(placement)
        await settle()
        const next = opdsLine(host).nextElementSibling
        expect(next?.className).toBe('catalog-address')
        expect(next?.querySelector('code')?.textContent).toBe(
          `${location.origin}${CATALOG}`,
        )
        expect(host.querySelectorAll('code')).toHaveLength(1)
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

      it(`draws the OPDS line with no address when /api/about does not answer, as a ${placement}`, async () => {
        clipboardContext(true, { writeText: vi.fn() })
        const fetchMock = server(lists)
        vi.stubGlobal('fetch', fetchMock)
        const host = placed(placement)
        await settle()
        // Asked, and answered 404: not a screen that never asked.
        expect(fetchMock).toHaveBeenCalledWith('/api/about')
        expect(opdsLine(host).nextElementSibling?.tagName).toBe('FORM')
        expect(host.querySelector('.catalog-address')).toBeNull()
        expect(host.querySelector('code')).toBeNull()
        expect(buttonSaying(host, 'Copy address')).toBeUndefined()
        expect(host.querySelector('[role="alert"]')).toBeNull()
      })
    }
  })
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
