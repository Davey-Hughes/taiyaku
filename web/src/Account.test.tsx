import { act } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Account } from './Account'
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
})
