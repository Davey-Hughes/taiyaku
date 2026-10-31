/**
 * The sign-in gate, against a real `fetch`.
 *
 * **Nothing here mocks `api.ts`**, and that is the point. `Library.test.tsx`
 * mocks `../src/library` wholesale, so no test in it ever reaches `api.ts`'s
 * `request` and none of them can see a 401 at all. These tests stub `fetch`
 * and let the real `getJson`, `Unauthenticated` and `whenSignedOut` run, so
 * what they pin is the path a browser actually takes.
 */
import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { getJson, type Me } from '../src/api'
import { Gate } from '../src/Gate'
import { useGate } from '../src/gate'
import { entryOf, read, readSet } from '../src/reads'
import { lastLibraryAddress, useKeptScroll } from '../src/scrollKeeper'
import { mount } from './support/mount'

const ADMIN: Me = {
  user_id: 7,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

/**
 * A server that holds one session, so signing in changes what `/api/me`
 * answers — the whole of what "signing in reveals the library" means.
 */
function server(who: Me | null = null) {
  let session = who
  const calls: string[] = []
  const handler = vi.fn(async (input: string, init?: RequestInit) => {
    const url = String(input)
    calls.push(url)
    if (url === '/api/session' && init?.method === 'POST') {
      session = ADMIN
      return new Response(null, { status: 204 })
    }
    if (session === null) return new Response('', { status: 401 })
    return new Response(JSON.stringify(session), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  })
  vi.stubGlobal('fetch', handler)
  return {
    calls,
    /** End the session server-side, the way another browser revoking it does. */
    revoke: () => {
      session = null
    },
  }
}

/** Let the gate's `me()` and everything it starts finish. */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

/**
 * What the gate guards: reads the context, and can make a call that 401s.
 *
 * Declared inside a function rather than at module scope, which is where two of
 * this project's lint rules meet: `style/useComponentExportOnlyModules` (on by
 * name — `biome.jsonc` says why) refuses an unexported component at module
 * scope, and `suspicious/noExportsInTest` refuses the export that would satisfy
 * it. A component declared inside a function is outside both, which is the
 * shape `Routes.test.tsx`'s `Nav` already uses.
 */
function probe() {
  function Guarded() {
    const gate = useGate()
    return (
      <div>
        <p data-testid="who">
          {gate === undefined
            ? 'no gate'
            : `${gate.who.user_id}/${gate.who.admin}`}
        </p>
        <button
          type="button"
          data-testid="sign-out"
          onClick={() => gate?.signOut()}
        >
          sign out
        </button>
        <button
          type="button"
          data-testid="call"
          onClick={() => {
            // The rejection is swallowed here exactly as every write in `App`
            // swallows its own — which is the case `whenSignedOut` exists for.
            void getJson('/api/library/status').catch(() => undefined)
          }}
        >
          call
        </button>
      </div>
    )
  }
  return <Guarded />
}

const show = async () => {
  const host = mount(<Gate>{probe()}</Gate>)
  await settle()
  return host
}

/**
 * A Library view's scroll keeper, drawn at `/?show=multi` in a router of its
 * own beside the gate, which makes that the last Library view's address.
 * Declared inside a function for the reason `probe` gives.
 */
function keepALibraryView() {
  function Kept() {
    useKeptScroll(true, { library: true })
    return null
  }
  mount(
    <MemoryRouter initialEntries={['/?show=multi']}>
      <Kept />
    </MemoryRouter>,
  )
}

const form = (host: HTMLElement) => host.querySelector('form')
const guarded = (host: HTMLElement) =>
  host.querySelector('[data-testid="who"]')?.textContent

/** Type into an input the way a browser does — `Login.test.tsx`'s note. */
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

const click = async (host: HTMLElement, testid: string) => {
  await act(async () => {
    host
      .querySelector<HTMLButtonElement>(`[data-testid="${testid}"]`)
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await Promise.resolve()
  })
  await settle()
}

describe('the sign-in gate', () => {
  beforeEach(() => {
    vi.unstubAllGlobals()
  })

  it('draws nothing at all until me() has answered', async () => {
    server(ADMIN)
    const host = mount(<Gate>{probe()}</Gate>)
    // Synchronous, before `settle()`. Rendering the children first and
    // replacing them once the answer arrives would flash a library at every
    // signed-out visitor; rendering the login screen first would flash one at
    // every signed-in one.
    expect(host.innerHTML).toBe('')
    await settle()
    expect(guarded(host)).toBe('7/true')
  })

  /**
   * Spec C1 §5: "a 401 routes to the login screen rather than throwing", and
   * §10's test list: "401 routes to login rather than rendering an empty grid".
   */
  it('shows a signed-out visitor the sign-in screen, not what it guards', async () => {
    server(null)
    const host = await show()
    expect(form(host), 'the sign-in form').not.toBeNull()
    // The negative half: what the gate guards must not be mounted at all — a
    // screen that renders and then reports "not signed in" in a banner is the
    // failure this test pins.
    expect(host.querySelector('[data-testid="who"]')).toBeNull()
  })

  it('reveals what it guards on a successful sign-in, without a reload', async () => {
    const { calls } = server(null)
    const host = await show()
    const inputs = [...host.querySelectorAll('input')]
    const [name, password] = inputs as HTMLInputElement[]
    if (!name || !password) throw new Error('the form is missing its fields')
    type(name, 'davey')
    type(password, 'a password')
    await act(async () => {
      form(host)?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()

    expect(form(host), 'the form has gone').toBeNull()
    expect(guarded(host)).toBe('7/true')
    // Asked again rather than assumed: `POST /api/session` answers 204 with no
    // body, so the account and its capabilities can only come from a second
    // `me()`. Two of them — the mount's and the sign-in's.
    expect(calls.filter((c) => c === '/api/me')).toHaveLength(2)
  })

  /**
   * A session that dies mid-read, noticed through a call whose own caller
   * swallowed the rejection — `api.ts`'s `whenSignedOut`, which is a
   * module-level slot and so can only be registered in one place. That place is
   * the gate.
   */
  it('lands on the sign-in screen when a 401 arrives later, from any call', async () => {
    const { revoke } = server(ADMIN)
    const host = await show()
    expect(guarded(host)).toBe('7/true')

    revoke()
    await click(host, 'call')

    expect(form(host), 'the sign-in form').not.toBeNull()
    expect(host.querySelector('[data-testid="who"]')).toBeNull()
  })

  it('signs out from the context, and back in again on the same page load', async () => {
    server(ADMIN)
    const host = await show()
    await click(host, 'sign-out')
    expect(form(host), 'the sign-in form').not.toBeNull()

    // The session is still good server-side — signing out here is the account
    // panel's button, and `logout()` is its own call. So the form's submit puts
    // the same account straight back.
    await act(async () => {
      form(host)?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    expect(guarded(host)).toBe('7/true')
  })

  /** Spec §3.9: what the store keeps was read for the account that read it. */
  it('empties the read store on signing out', async () => {
    server(ADMIN)
    const host = await show()
    const kept = read('/api/kept', () => Promise.resolve('an answer'))
    await readSet([kept])
    expect(entryOf(kept)?.answer).toBe('an answer')

    await click(host, 'sign-out')
    expect(form(host), 'the sign-in form').not.toBeNull()
    expect(entryOf(kept)).toBeUndefined()
  })

  it('empties the read store on signing in', async () => {
    server(ADMIN)
    const host = await show()
    await click(host, 'sign-out')
    const kept = read('/api/kept', () => Promise.resolve('an answer'))
    await readSet([kept])
    expect(entryOf(kept)?.answer).toBe('an answer')

    await act(async () => {
      form(host)?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    expect(guarded(host)).toBe('7/true')
    expect(entryOf(kept)).toBeUndefined()
  })

  /** The scroll keeper's address and scrolls were the account's too. */
  it('forgets the last Library view on signing out', async () => {
    server(ADMIN)
    const host = await show()
    keepALibraryView()
    expect(lastLibraryAddress()).toBe('/?show=multi')

    await click(host, 'sign-out')
    expect(form(host), 'the sign-in form').not.toBeNull()
    expect(lastLibraryAddress()).toBeUndefined()
  })

  it('forgets the last Library view on signing in', async () => {
    server(ADMIN)
    const host = await show()
    await click(host, 'sign-out')
    keepALibraryView()
    expect(lastLibraryAddress()).toBe('/?show=multi')

    await act(async () => {
      form(host)?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await Promise.resolve()
    })
    await settle()
    expect(guarded(host)).toBe('7/true')
    expect(lastLibraryAddress()).toBeUndefined()
  })

  it('logs a failure that is not a 401, and still offers the sign-in screen', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    vi.stubGlobal(
      'fetch',
      vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
    )
    const host = await show()
    // A server that is down and a server that wants a password look the same
    // from a login screen, so the difference goes where a developer can see it.
    expect(logged).toHaveBeenCalledWith(expect.any(TypeError))
    expect(form(host)).not.toBeNull()
    logged.mockRestore()
  })

  it('answers useGate with undefined where no gate is mounted above', async () => {
    server(ADMIN)
    // The case every screen's own test is in: mounted alone, with nothing
    // above. It has to be safe rather than a crash, and it has to read as "no
    // account known" rather than as an account.
    const host = mount(probe())
    await settle()
    expect(guarded(host)).toBe('no gate')
  })
})
