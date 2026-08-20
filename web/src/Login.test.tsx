import { act } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Login } from './Login'
import { mount } from './testing/mount'

const noop = () => {}

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

const fields = (host: HTMLElement) => {
  const inputs = [...host.querySelectorAll('input')] as HTMLInputElement[]
  const [name, password] = inputs
  if (!name || !password) throw new Error('the form is missing its fields')
  return { name, password }
}

const submit = async (host: HTMLElement) => {
  const form = host.querySelector('form')
  if (!form) throw new Error('no form')
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
    await Promise.resolve()
  })
}

describe('Login', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('sends what was typed and reports success upward', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    const onSignedIn = vi.fn()

    const host = mount(<Login onSignedIn={onSignedIn} />)
    const { name, password } = fields(host)
    type(name, 'davey')
    type(password, 'a password')
    await submit(host)

    expect(fetchMock).toHaveBeenCalledWith('/api/session', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'davey', password: 'a password' }),
    })
    expect(onSignedIn).toHaveBeenCalledOnce()
  })

  /**
   * A wrong password is shown here rather than routed on. Routing to the login
   * screen *from* the login screen is a loop that looks to a reader like
   * pressing the button did nothing at all.
   */
  it('shows why a sign-in failed and stays put', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 401 })),
    )
    const onSignedIn = vi.fn()

    const host = mount(<Login onSignedIn={onSignedIn} />)
    await submit(host)

    const alert = host.querySelector('[role="alert"]')
    expect(alert?.textContent).toMatch(/did not match/)
    expect(onSignedIn).not.toHaveBeenCalled()
    expect(host.querySelector('form')).not.toBeNull()
  })

  it('distinguishes being rate-limited from being wrong', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 429 })),
    )
    const host = mount(<Login onSignedIn={noop} />)
    await submit(host)
    expect(host.querySelector('[role="alert"]')?.textContent).toMatch(
      /Too many attempts/,
    )
  })

  /**
   * The password field is a real password field. Not a style point: a browser's
   * password manager offers to save what it recognises, and `autoComplete`
   * decides whether it offers the right thing.
   */
  it('marks its fields so a password manager can fill them', () => {
    const host = mount(<Login onSignedIn={noop} />)
    const { name, password } = fields(host)
    expect(password.type).toBe('password')
    expect(name.autocomplete).toBe('username')
    expect(password.autocomplete).toBe('current-password')
  })

  /** The previous failure clears on submit, not on the next keystroke. */
  it('keeps a failure on screen while the reader retypes', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 401 })),
    )
    const host = mount(<Login onSignedIn={noop} />)
    await submit(host)
    expect(host.querySelector('[role="alert"]')).not.toBeNull()

    type(fields(host).password, 'trying again')
    expect(host.querySelector('[role="alert"]')).not.toBeNull()
  })
})
