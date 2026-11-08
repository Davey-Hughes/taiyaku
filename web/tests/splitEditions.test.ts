import { afterEach, describe, expect, it, vi } from 'vitest'
import { listSplitEditions, type SplitEdition } from '../src/splitEditions'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the split-editions client', () => {
  it('asks for the list and hands back the answer', async () => {
    const edition: SplitEdition = {
      work: 'blame',
      title: 'Blame!',
      edition: 'en',
      pages: 400,
      members: [
        {
          path: 'Blame! v01.cbz',
          pages: 200,
          directories: [{ root: 3, path: '/media/a', libraries: [1] }],
        },
        {
          path: 'Blame! v02.cbz',
          pages: 200,
          directories: [{ root: 4, path: '/media/b', libraries: [2] }],
        },
      ],
      libraries: [
        { id: 1, name: 'A', members: 1, pages: 200 },
        { id: 2, name: 'B', members: 1, pages: 200 },
      ],
    }
    const fetchMock = vi
      .fn()
      .mockImplementation(() => json({ editions: [edition] }))
    vi.stubGlobal('fetch', fetchMock)
    const answer = await listSplitEditions()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/split-editions')
    const init = fetchMock.mock.calls[0]?.[1] as RequestInit | undefined
    expect(init?.method ?? 'GET').toBe('GET')
    expect(answer).toEqual({ editions: [edition] })
  })
})
