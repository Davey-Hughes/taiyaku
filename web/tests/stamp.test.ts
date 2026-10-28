import { afterEach, describe, expect, it, vi } from 'vitest'
import { type Archive, performStamp, stampPlan } from '../src/stamp'

/**
 * A fresh `Response` per call, not one shared instance — `Response.json()`
 * consumes its body stream, so a second read of the same instance throws
 * "Body is unusable". `users.test.ts`, `config.test.ts` and `library.test.ts`
 * carry the same note for the same reason.
 */
const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

const EMPTY = { archives: [], total_bytes: 0, skipped_formats: [] }

const ONE: Archive = {
  member_id: 7,
  edition_key: 'e',
  path: '/lib/v01.cbz',
  bytes: 40,
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('stampPlan', () => {
  it('reads the plan for one work', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        json({ archives: [ONE], total_bytes: 40, skipped_formats: ['rar'] }),
      )
    vi.stubGlobal('fetch', fetchMock)

    const plan = await stampPlan('w')

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/works/w/stamp-comicinfo',
    )
    expect(plan.archives).toHaveLength(1)
    expect(plan.archives[0]?.member_id).toBe(7)
    expect(plan.skipped_formats).toEqual(['rar'])
  })

  it('percent-encodes a slug, rather than letting it reshape the path', async () => {
    const fetchMock = vi.fn().mockImplementation(() => json(EMPTY))
    vi.stubGlobal('fetch', fetchMock)

    await stampPlan('a/b')

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/works/a%2Fb/stamp-comicinfo',
    )
  })

  it('carries an empty plan as an answer, not as a failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(() => json(EMPTY)),
    )

    await expect(stampPlan('w')).resolves.toMatchObject({ archives: [] })
  })
})

describe('performStamp', () => {
  it('sends back the archives it was given, unaltered', async () => {
    const fetchMock = vi.fn().mockImplementation(() =>
      json({
        rewritten: 1,
        content_key: 'k2',
        document_moved: true,
        sidecar_rewritten: true,
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const done = await performStamp('w', [ONE])

    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    // **The exact array, byte for byte.** `stamp::apply` compares the body to
    // a freshly computed plan field for field and answers 409 on any
    // difference, so a client that reordered or trimmed a field would be
    // refused every time.
    expect(init.body).toBe(JSON.stringify({ archives: [ONE] }))
    expect(done.content_key).toBe('k2')
  })

  it('refuses an empty 204 where an outcome was expected', async () => {
    // The POST answers a body on every success path, so this is the guard for
    // a server that stops doing so — without it `performStamp` would resolve
    // `undefined` and the screen would render "Rewrote undefined archives".
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 204 })),
    )
    await expect(performStamp('w', [])).rejects.toThrow(/empty 204/)
  })

  it("surfaces a 409 with the server's own sentence", async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(
        () =>
          new Response('a scan or another library job is currently running', {
            status: 409,
          }),
      ),
    )

    await expect(performStamp('w', [])).rejects.toThrow(
      /a scan or another library job/,
    )
  })
})
