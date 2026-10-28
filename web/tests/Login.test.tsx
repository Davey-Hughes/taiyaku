import { act } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Login } from '../src/Login'
import { mount } from './support/mount'

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

  it('carries the class base.css scopes the theme’s controls to', () => {
    const host = mount(<Login onSignedIn={noop} />)
    expect(host.firstElementChild?.classList.contains('login')).toBe(true)
  })

  it('draws its heading’s wordmark as words, not as a link as the sidebar’s is', () => {
    const host = mount(<Login onSignedIn={noop} />)
    const heading = host.querySelector('h1')
    expect(heading?.querySelector('.wordmark')).not.toBeNull()
    expect(heading?.querySelector('a')).toBeNull()
    expect(heading?.closest('a')).toBeNull()
  })

  /**
   * The heading is the wordmark, and the 対訳 seal inside it is drawn for the
   * eye only — `aria-hidden` — so the heading's accessible name is `taiyaku`
   * alone. No testing-library here, so the name is computed as it comes out
   * for this markup: the heading's text, less every `aria-hidden` descendant.
   */
  it('names its heading taiyaku, not the seal beside it', () => {
    const host = mount(<Login onSignedIn={noop} />)
    const heading = host.querySelector('h1')
    if (!heading) throw new Error('no heading')
    expect(heading.textContent).toContain('対訳')

    // Neither attribute present: an `aria-label` here would give the heading
    // that value as its accessible name outright, and the text-content
    // computation below would never run at all — so this rules out the
    // heading being named "対訳" by a label rather than by its content.
    expect(heading.hasAttribute('aria-label')).toBe(false)
    expect(heading.hasAttribute('aria-labelledby')).toBe(false)

    const named = heading.cloneNode(true) as HTMLElement
    for (const hidden of named.querySelectorAll('[aria-hidden="true"]')) {
      hidden.remove()
    }
    expect(named.textContent?.trim()).toBe('taiyaku')
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

  /**
   * **An account name is compared byte for byte**, so a keyboard that helps
   * is a keyboard that locks the reader out. `user.name` is `TEXT NOT NULL
   * UNIQUE` with no `COLLATE NOCASE`, and `account::by_name`'s
   * `WHERE name = ?1` matches under SQLite's BINARY default — verified
   * against a real library, where `local` returns 1 row and `Local` returns
   * 0. iOS capitalises the first letter of an empty text field by default,
   * so every iPhone typed a name that could not match, on the one screen
   * there is no way past. Reported from a device pass, 2026-09-23.
   */
  it('stops a phone keyboard rewriting the account name', () => {
    const host = mount(<Login onSignedIn={noop} />)
    const { name, password } = fields(host)
    expect(name.getAttribute('autocapitalize')).toBe('none')
    expect(name.getAttribute('autocorrect')).toBe('off')
    // The attribute, not the property: jsdom does not reflect `spellcheck`.
    expect(name.getAttribute('spellcheck')).toBe('false')
    // Not on the password: it is never shown, a manager fills it, and iOS
    // does not autocapitalise a `type="password"` field in the first place.
    expect(password.getAttribute('autocapitalize')).toBeNull()
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

  it('is a main landmark with one h1, kept at the h2 size it was drawn at — O163', () => {
    const host = mount(<Login onSignedIn={noop} />)
    expect(host.firstElementChild?.tagName).toBe('MAIN')
    const h1s = [...host.querySelectorAll('h1')]
    expect(h1s).toHaveLength(1)
    expect(host.querySelector('h2')).toBeNull()
    // `base.css` sizes an `h1` a step up; the heading keeps the `h2` step.
    expect(h1s[0].style.fontSize).toBe('var(--text-h2)')
  })

  it('names its tab Sign in — O173', () => {
    document.title = 'stale'
    mount(<Login onSignedIn={noop} />)
    expect(document.title).toBe('Sign in — taiyaku')
  })
})
