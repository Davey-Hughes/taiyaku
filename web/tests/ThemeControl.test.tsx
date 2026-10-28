import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { mount } from './support/mount'
import { fakeStorage } from './support/storage'
import { ThemeControl } from '../src/ThemeControl'

function radio(host: HTMLElement, label: string): HTMLInputElement {
  const row = [...host.querySelectorAll('.theme-control label')].find(
    (l) => l.textContent?.trim() === label,
  )
  const input = row?.querySelector('input[type="radio"]')
  if (!(input instanceof HTMLInputElement)) {
    throw new Error(`no radio labelled "${label}"`)
  }
  return input
}

describe('ThemeControl', () => {
  let storage: Storage

  beforeEach(() => {
    storage = fakeStorage()
    vi.stubGlobal('localStorage', storage)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    document.documentElement.removeAttribute('data-theme')
  })

  it('offers System, Paper and Ink, with System chosen until a reader picks', () => {
    const host = mount(<ThemeControl />)
    expect(
      [...host.querySelectorAll('.theme-control label')].map((l) =>
        l.textContent?.trim(),
      ),
    ).toEqual(['System', 'Paper', 'Ink'])
    expect(radio(host, 'System').checked).toBe(true)
  })

  it('starts from the stored choice', () => {
    storage.setItem('taiyaku.prefs.theme', '{"choice":"paper"}')
    const host = mount(<ThemeControl />)
    expect(radio(host, 'Paper').checked).toBe(true)
  })

  it('applies and remembers a choice the moment it is made', () => {
    const host = mount(<ThemeControl />)
    act(() => {
      radio(host, 'Ink').click()
    })
    expect(document.documentElement.getAttribute('data-theme')).toBe('ink')
    expect(storage.getItem('taiyaku.prefs.theme')).toBe('{"choice":"ink"}')
    expect(radio(host, 'Ink').checked).toBe(true)
  })

  it('hands the choice back to the OS when System is picked again', () => {
    storage.setItem('taiyaku.prefs.theme', '{"choice":"ink"}')
    document.documentElement.setAttribute('data-theme', 'ink')
    const host = mount(<ThemeControl />)
    act(() => {
      radio(host, 'System').click()
    })
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false)
    expect(storage.getItem('taiyaku.prefs.theme')).toBe('{"choice":"system"}')
  })
})
