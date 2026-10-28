import { type Locator, page } from 'vitest/browser'

/** The button among `scope`'s whose words are `words`; throws where none is. */
export function buttonSaying(scope: ParentNode, words: string): HTMLElement {
  const found = [...scope.querySelectorAll('button')].find(
    (button) => button.textContent === words,
  )
  if (found === undefined) throw new Error(`no button says ${words}`)
  return found
}

/**
 * `el`, the `role` it has, found again by its role and its words, for
 * `userEvent` to act on. Not `el` itself: @vitest/browser 4.1.10 finds an
 * element it is handed by a name it builds from CSS generated content,
 * alternative text and all, so a path's breaks (`PathWithBreaks`,
 * `content: "\200b" / ""`) put `​" / "` in a directory's name, and an action
 * on the element finds nothing and times out. Chromium's own name, which
 * `getByRole` asks for, is the words.
 */
export function byRole(el: Element, role: 'cell' | 'link'): Locator {
  return page.getByRole(role, { name: el.textContent ?? '', exact: true })
}
