import { act } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Note } from '../src/notes'
import { type BookNotes, useBookNotes } from '../src/useBookNotes'
import { mount, rerender } from './support/mount'

/**
 * `useBookNotes`, mounted on its own, for what the book reader cannot reach:
 * the reader offers no change before the notes are read, and switches work
 * only by remounting.
 */

const note = (body: string): Note => ({
  id: body,
  edition: 'en',
  slot: 0,
  body,
  created: '2026-11-15T10:00:00Z',
  updated: '2026-11-15T10:00:00Z',
})

/** Answers each work's read when the test says, and counts the writes. */
function notesServer() {
  const answers = new Map<string, (res: Response) => void>()
  const fetch = vi.fn(
    (url: string, init?: RequestInit) =>
      new Promise<Response>((resolve) => {
        const work = decodeURIComponent(url.split('/')[3] ?? '')
        if (init?.method === 'PUT') {
          resolve(new Response(String(init.body), { headers: { etag: '"2"' } }))
        } else answers.set(work, resolve)
      }),
  )
  vi.stubGlobal('fetch', fetch)
  const answer = async (work: string, res: Response) => {
    await act(async () => {
      answers.get(work)?.(res)
      await new Promise((resolve) => setTimeout(resolve, 0))
    })
  }
  const read = (work: string, notes: Note[]) =>
    answer(
      work,
      new Response(JSON.stringify({ work, notes }), {
        headers: { etag: '"1"' },
      }),
    )
  const puts = () =>
    fetch.mock.calls.filter(([, init]) => init?.method === 'PUT').length
  return { answer, read, puts }
}

function probe(work: string) {
  let last: BookNotes | undefined
  function Probe({ work }: { work: string }) {
    last = useBookNotes(work)
    return null
  }
  mount(<Probe work={work} />)
  return {
    last: () => last as BookNotes,
    switchTo: (to: string) => rerender(<Probe work={to} />),
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('useBookNotes', () => {
  it('writes nothing for a change made before the notes are read', async () => {
    const server = notesServer()
    const hook = probe('a')
    expect(hook.last().ready).toBe(false)
    act(() => hook.last().change([note('early')]))
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 750))
    })
    expect(server.puts()).toBe(0)
  })

  it('keeps a switched-to work’s notes over the late answer for the one before', async () => {
    const server = notesServer()
    const hook = probe('a')
    hook.switchTo('b')
    await server.read('b', [note('of b')])
    await server.read('a', [note('of a')])
    expect(hook.last().notes.map((n) => n.body)).toEqual(['of b'])
  })

  it('says nothing of a read that failed for a work switched away from', async () => {
    const server = notesServer()
    const hook = probe('a')
    hook.switchTo('b')
    await server.answer('a', new Response('gone', { status: 500 }))
    expect(hook.last().notice).toBeUndefined()
  })
})
