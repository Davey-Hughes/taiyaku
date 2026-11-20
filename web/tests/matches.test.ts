import { afterEach, describe, expect, it, vi } from 'vitest'
import { splitMatch, undoSplit } from '../src/matches'
import { emptyDerive } from './support/derive'

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })

/** Stubs `fetch` to answer every request with `answer`, and keeps each. */
function serve(answer: () => Response) {
  const sent: { url: string; init: RequestInit | undefined }[] = []
  vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) => {
    sent.push({ url: String(input), init })
    return Promise.resolve(answer())
  })
  return sent
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the content match client', () => {
  it('splits by POST to the pair’s address, the body naming the work and the edition pressed', async () => {
    const sent = serve(() => json({ split: [7, 8], derive: emptyDerive }))
    await expect(splitMatch(7, 'ruridragon-v02', 'ja')).resolves.toEqual({
      split: [7, 8],
      derive: emptyDerive,
    })
    expect(sent.map(({ url, init }) => [url, init?.method])).toEqual([
      ['/api/library/matches/7/split', 'POST'],
    ])
    expect(JSON.parse(String(sent[0].init?.body))).toEqual({
      work: 'ruridragon-v02',
      edition: 'ja',
    })
  })

  it('undoes by DELETE of the same address, with no body', async () => {
    const sent = serve(() => json({ derive: emptyDerive }))
    await expect(undoSplit(9)).resolves.toEqual({ derive: emptyDerive })
    expect(sent.map(({ url, init }) => [url, init?.method])).toEqual([
      ['/api/library/matches/9/split', 'DELETE'],
    ])
    expect(sent[0].init?.body).toBeUndefined()
  })

  it('carries a refusal’s status and body into what it throws', async () => {
    const refusal = {
      error:
        'a scan or another library job is running; split again once it has finished',
    }
    serve(() => json(refusal, 409))
    await expect(splitMatch(7, 'ruridragon-v02', 'ja')).rejects.toThrow(
      `/api/library/matches/7/split: 409 ${JSON.stringify(refusal)}`,
    )
    await expect(undoSplit(9)).rejects.toThrow(
      `/api/library/matches/9/split: 409 ${JSON.stringify(refusal)}`,
    )
  })
})
