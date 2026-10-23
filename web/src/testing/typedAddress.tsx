import { act } from 'react'
import { BrowserRouter, Route, Routes } from 'react-router'
import { onTestFinished } from 'vitest'
import { type Locator, page, userEvent } from 'vitest/browser'
import type { Me } from '../api'
import { Config } from '../Config'
import { GateContext } from '../gate'
import { mount } from './mount'

async function settle() {
  await act(async () => {
    await new Promise((r) => setTimeout(r, 120))
  })
}

/**
 * Configuration under the router the app runs, `BrowserRouter`, at `width`,
 * signed in as `who`, with the frame put at `address` first, as a typed
 * address or a reload puts it; the frame's own address is put back when the
 * test ends.
 */
export async function openedAt(
  address: string,
  who: Me,
  width = 1280,
): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
  })
  history.replaceState(null, '', address)
  await page.viewport(width, 800)
  const host = mount(
    <BrowserRouter>
      <GateContext value={{ who, signOut: () => undefined }}>
        <Routes>
          <Route path="/config/*" element={<Config />} />
        </Routes>
      </GateContext>
    </BrowserRouter>,
  )
  await settle()
  return host
}

/** A real mouse, inside `act`, and what it started given time to land. */
export const click = async (el: Element | Locator) => {
  await act(async () => {
    await userEvent.click(el)
  })
  await settle()
}

/** The link in `scope` whose text is `text`. */
export function linkIn(scope: Element, text: string): HTMLAnchorElement {
  const found = [...scope.querySelectorAll('a')].find(
    (a) => a.textContent === text,
  )
  if (found === undefined) throw new Error(`no link "${text}"`)
  return found
}
