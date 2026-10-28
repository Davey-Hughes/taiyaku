import { act, useRef } from 'react'
import { MemoryRouter, type NavigateFunction, useNavigate } from 'react-router'
import { describe, expect, it } from 'vitest'
import { type LateAnswers, useLateAnswers } from '../src/configPages'
import { flush } from './support/async'
import { mount } from './support/mount'

interface Line {
  at: string
  focus?: boolean
  n?: number
  says: string
}

const TABLE = '/config/libraries'
const MANGA = `${TABLE}/Manga`
const SHELVES = `${TABLE}/Shelves`

/**
 * The hook under a router at `entry`, drawing its notices with the line as a
 * paragraph, and handing the hook and the router to the test. Made here
 * because `useComponentExportOnlyModules` refuses an unexported component at
 * module scope, and `noExportsInTest` refuses the export.
 */
function screenAt(entry: string) {
  const handed: { answers?: LateAnswers<Line>; go?: NavigateFunction } = {}
  function Screen() {
    const lineEl = useRef<HTMLParagraphElement>(null)
    const answers = useLateAnswers<Line>(TABLE, lineEl)
    handed.answers = answers
    handed.go = useNavigate()
    return (
      <div className="page">
        {answers.drawNotices(
          null,
          answers.shown !== undefined && (
            <p ref={lineEl} className="line" tabIndex={-1}>
              {answers.shown.says}
            </p>
          ),
        )}
      </div>
    )
  }
  const host = mount(
    <MemoryRouter initialEntries={[entry]}>
      <Screen />
    </MemoryRouter>,
  )
  const answers = () => {
    if (handed.answers === undefined) throw new Error('nothing drawn')
    return handed.answers
  }
  const go = (to: string) => flush(() => handed.go?.(to, { replace: true }))
  return { host, answers, go }
}

describe('useLateAnswers, between a rename and the commit that moves the person', () => {
  // The rename's transition has not moved the address yet, so the page
  // drawn is still the old one: each is read through the rename too.
  it('places an answer from beneath the renamed page as sent, at the line’s new page', () => {
    const { answers } = screenAt(`${MANGA}/dirs/11`)
    act(() => answers().follow(MANGA, SHELVES))
    const { sent, line } = answers().place(`${MANGA}/dirs/11`, {
      at: MANGA,
      says: 'Removed.',
    })
    expect(sent).toBe(true)
    expect(line.at).toBe(SHELVES)
    expect(line.focus).toBe(true)
  })

  it('says a refusal at the renamed page’s new address', async () => {
    const { host, answers, go } = screenAt(MANGA)
    act(() => {
      answers().follow(MANGA, SHELVES)
      answers().say('Not saved.')
    })
    await go(SHELVES)
    expect(
      [...host.querySelectorAll('.page > [role="alert"]')].map(
        (p) => p.textContent,
      ),
    ).toEqual(['Not saved.'])
  })
})

describe('useLateAnswers, once the person is at a renamed page’s old address again', () => {
  // Another library may have taken the old name since: what is sent from
  // there is that page's own.
  it('places an answer from there as sent from there', async () => {
    const { answers, go } = screenAt(`${MANGA}/dirs/11`)
    act(() => answers().follow(MANGA, SHELVES))
    await go(`${SHELVES}/dirs/11`)
    await go(`${MANGA}/dirs/20`)
    const { sent, line } = answers().place(`${MANGA}/dirs/20`, {
      at: MANGA,
      says: 'Removed.',
    })
    expect(sent).toBe(true)
    expect(line.at).toBe(MANGA)
  })
})
