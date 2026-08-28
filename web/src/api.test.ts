import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  Unauthenticated,
  getJson,
  getNotes,
  getProgress,
  getSidecar,
  getTruth,
  changePassword,
  createKey,
  listKeys,
  listSessions,
  listWorks,
  login,
  logout,
  me,
  pageUrl,
  patchAlignment,
  revokeKey,
  revokeSession,
  whenSignedOut,
  putNotes,
  putProgress,
  putTruth,
  thumbUrl,
} from './api'

describe('url building', () => {
  it('addresses a page by flat index', () => {
    expect(pageUrl('go-toubun-v01', 'en-mono', 47)).toBe(
      '/api/works/go-toubun-v01/en-mono/pages/47',
    )
  })

  it('escapes an edition key that needs it', () => {
    // validate_key's alphabet permits `-`, and a key reaches the URL as a
    // path segment. Encoding rather than trusting the key is the cheap half
    // of not caring what the alphabet becomes later.
    expect(pageUrl('w', 'a b', 0)).toBe('/api/works/w/a%20b/pages/0')
  })

  it('appends /thumb for a tile', () => {
    expect(thumbUrl('w', 'raw', 3)).toBe('/api/works/w/raw/pages/3/thumb')
  })
})

// Mocks fetch directly rather than pulling in jsdom or MSW: these are plain
// functions wrapping the Fetch API, and a real DOM buys nothing here.
describe('network calls', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('listWorks returns the parsed array of work objects', async () => {
    const listed = [
      {
        slug: 'taiyaku',
        title: 'Taiyaku',
        series: null,
        volume: null,
        editions: ['raw'],
        cover: '/api/works/taiyaku/raw/pages/0/thumb',
      },
    ]
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify(listed), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(listWorks()).resolves.toEqual(listed)
    expect(fetchMock).toHaveBeenCalledWith('/api/works')
  })

  it('listWorks rejects with the server body on a non-2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          new Response('no works configured', { status: 500 }),
        ),
    )

    await expect(listWorks()).rejects.toThrow(/no works configured/)
  })

  it("getJson rejects with the status and the server's own sentence, not the status alone", async () => {
    // Every read in `library.ts` and `curation.ts` comes through `getJson`,
    // and this threw `path: status` with the body read off the wire and
    // dropped. The proposals inbox is where that cost the most: `Routes.tsx`
    // puts no admin guard on `/proposals`, so a reader who types the URL
    // reaches `listProposals`, and `curation.rs::admin_or_forbidden` answers
    // 403 with the sentence below — the only part of the refusal a person
    // can act on.
    //
    // The whole message is asserted rather than a substring of it: a version
    // that dropped the status, or the path, or appended the body to the
    // wrong one of them, passes `toThrow(/administrator/)`.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response("the proposals inbox is an administrator's", {
          status: 403,
        }),
      ),
    )

    await expect(getJson('/api/library/proposals')).rejects.toThrow(
      "/api/library/proposals: 403 the proposals inbox is an administrator's",
    )
  })

  it('getSidecar returns the sidecar verbatim as text, unparsed', async () => {
    const body = '{"editions":[]}'
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(body, { status: 200 })),
    )

    await expect(getSidecar('taiyaku')).resolves.toBe(body)
  })

  it('getProgress resolves to undefined on a 204 (nothing recorded)', async () => {
    // Response.ok is true for 204, so this only stays correct if the 204
    // check runs before the !res.ok check — reordering those two lines would
    // route a "nothing recorded" response into res.json() on an empty body.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 204 })),
    )

    await expect(getProgress('taiyaku')).resolves.toBeUndefined()
  })

  it('getProgress resolves to the position parsed out of a single-element set on a 200', async () => {
    const progress = { edition: 'en-mono', slot: 12 }
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            positions: [{ ...progress, updated: '2026-08-27 09:00:00' }],
          }),
          { status: 200 },
        ),
      ),
    )

    await expect(getProgress('taiyaku')).resolves.toEqual(progress)
  })

  it('getProgress takes the newest position out of the set the server answers', async () => {
    // The fixture is built so the right answer ('raw'/7, the head of the
    // newest-first list) is neither the last element (which would catch a
    // fold that took the tail instead of the head) nor the alphabetically
    // first edition (which would catch an implementation that re-sorted by
    // name instead of trusting server order): 'full-color' < 'monochrome' <
    // 'raw' alphabetically, and 'monochrome' is last in the array. A
    // fixture where the right and a plausible wrong answer coincide proves
    // nothing.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            positions: [
              { edition: 'raw', slot: 7, updated: '2026-08-27 10:00:00' },
              {
                edition: 'full-color',
                slot: 33,
                updated: '2026-08-27 09:24:41',
              },
              {
                edition: 'monochrome',
                slot: 2,
                updated: '2026-08-27 08:00:00',
              },
            ],
          }),
          { status: 200 },
        ),
      ),
    )

    await expect(getProgress('taiyaku')).resolves.toEqual({
      edition: 'raw',
      slot: 7,
    })
  })

  it("getProgress rejects on a non-2xx, carrying the server's own wording", async () => {
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(
          new Response('unknown work "taiyaku"', { status: 404 }),
        ),
    )

    await expect(getProgress('taiyaku')).rejects.toThrow(
      /unknown work "taiyaku"/,
    )
  })

  it('putProgress resolves on success and sends the position as JSON', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)

    const progress = { edition: 'en-mono', slot: 12 }
    await expect(putProgress('taiyaku', progress)).resolves.toBeUndefined()

    expect(fetchMock).toHaveBeenCalledWith('/api/works/taiyaku/progress', {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(progress),
      keepalive: true,
    })
  })

  it('putProgress rejects with the server body on a non-2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('slot 12 is out of range for en-mono', {
          status: 400,
        }),
      ),
    )

    await expect(
      putProgress('taiyaku', { edition: 'en-mono', slot: 12 }),
    ).rejects.toThrow(/slot 12 is out of range for en-mono/)
  })

  describe('patchAlignment', () => {
    it('sends the validator it was given and returns the fresh one', async () => {
      const calls: RequestInit[] = []
      vi.stubGlobal('fetch', (_url: string, init: RequestInit) => {
        calls.push(init)
        return Promise.resolve(
          new Response('{"taiyaku":1}', {
            status: 200,
            headers: { etag: '"new"' },
          }),
        )
      })

      const fresh = await patchAlignment('w', '"old"', {
        from: 'raw',
        to: 'en',
        a: '3',
        b: '5',
      })

      expect(new Headers(calls[0].headers).get('if-match')).toBe('"old"')
      expect(calls[0].method).toBe('PATCH')
      // The server refuses an unknown field with a 400, so the wire shape has
      // to be exact — asserted directly rather than resting on
      // `JSON.stringify` being a passthrough of what was passed in.
      expect(JSON.parse(calls[0].body as string)).toEqual({
        from: 'raw',
        to: 'en',
        a: '3',
        b: '5',
      })
      expect(fresh.etag).toBe('"new"')
      expect(fresh.json).toBe('{"taiyaku":1}')
    })

    /** A 412 is a real answer a reader has to be told about, not a network error. */
    it('reports a stale validator by name', async () => {
      vi.stubGlobal('fetch', () =>
        Promise.resolve(
          new Response('this work has changed since you read it', {
            status: 412,
          }),
        ),
      )

      await expect(
        patchAlignment('w', '"old"', { from: 'raw', to: 'en', a: '3', b: '5' }),
      ).rejects.toThrow(/changed since you read it/)
    })
  })

  describe('notes', () => {
    // A work nobody has annotated answers with an empty document rather than a
    // 204, unlike progress — so there is no "nothing recorded" case here, and
    // the tag is always present for the first write to be conditional on.
    it('reads the document and the validator a write has to carry', async () => {
      const urls: string[] = []
      vi.stubGlobal('fetch', (url: string) => {
        urls.push(url)
        return Promise.resolve(
          new Response('{"work":"w","notes":[]}', {
            status: 200,
            headers: { etag: '"n1"' },
          }),
        )
      })

      const fetched = await getNotes('w')

      expect(urls[0]).toBe('/api/works/w/notes')
      expect(fetched.etag).toBe('"n1"')
      expect(fetched.doc).toEqual({ work: 'w', notes: [] })
    })

    it('writes conditionally, and sends the document verbatim', async () => {
      const calls: RequestInit[] = []
      vi.stubGlobal('fetch', (_url: string, init: RequestInit) => {
        calls.push(init)
        return Promise.resolve(
          new Response('{"work":"w","notes":[]}', {
            status: 200,
            headers: { etag: '"n2"' },
          }),
        )
      })

      const doc = {
        work: 'w',
        notes: [
          {
            id: '01J8',
            edition: 'raw',
            slot: 3,
            body: 'why is this page here',
            created: '2026-08-05T10:00:00Z',
            updated: '2026-08-05T10:00:00Z',
          },
        ],
      }
      const fresh = await putNotes('w', '"n1"', doc)

      expect(calls[0].method).toBe('PUT')
      expect(new Headers(calls[0].headers).get('if-match')).toBe('"n1"')
      // The server refuses an unknown field with a 400, so the wire shape has
      // to be exact — the same reasoning `patchAlignment`'s test states.
      expect(JSON.parse(calls[0].body as string)).toEqual(doc)
      expect(fresh.etag).toBe('"n2"')
    })

    /** The lost-update answer, which the reader is told about rather than retried through. */
    it('reports a stale validator by name', async () => {
      vi.stubGlobal('fetch', () =>
        Promise.resolve(
          new Response('these notes have changed since you read them', {
            status: 412,
          }),
        ),
      )

      await expect(
        putNotes('w', '"old"', { work: 'w', notes: [] }),
      ).rejects.toThrow(/changed since you read them/)
    })
  })

  describe('truth', () => {
    // A work nobody has labelled answers with an empty document rather than
    // a 204, the same rule `notes` follows above and for the same reason:
    // the first write needs a validator to be conditional on.
    it('reads the document and the validator a write has to carry', async () => {
      const urls: string[] = []
      vi.stubGlobal('fetch', (url: string) => {
        urls.push(url)
        return Promise.resolve(
          new Response('{"taiyaku_truth":1,"editions":{}}', {
            status: 200,
            headers: { etag: '"t1"' },
          }),
        )
      })

      const fetched = await getTruth('w')

      expect(urls[0]).toBe('/api/works/w/truth')
      expect(fetched.etag).toBe('"t1"')
      expect(fetched.doc).toEqual({ taiyaku_truth: 1, editions: {} })
    })

    it('writes conditionally, and sends the document verbatim', async () => {
      const calls: RequestInit[] = []
      vi.stubGlobal('fetch', (_url: string, init: RequestInit) => {
        calls.push(init)
        return Promise.resolve(
          new Response('{"taiyaku_truth":1,"editions":{}}', {
            status: 200,
            headers: { etag: '"t2"' },
          }),
        )
      })

      const doc = {
        taiyaku_truth: 1,
        editions: {
          'raw-mono': {
            members: [{ sha256: 'aa' }],
            pages: { '0': [] },
          },
        },
      }
      const fresh = await putTruth('w', '"t1"', doc)

      expect(calls[0].method).toBe('PUT')
      expect(new Headers(calls[0].headers).get('if-match')).toBe('"t1"')
      expect(JSON.parse(calls[0].body as string)).toEqual(doc)
      expect(fresh.etag).toBe('"t2"')
    })

    /** O35: a labelling session is hours of work nowhere else, so the lost-update answer is reported rather than retried through. */
    it('reports a stale validator by name', async () => {
      vi.stubGlobal('fetch', () =>
        Promise.resolve(
          new Response('this document has changed since you read it', {
            status: 412,
          }),
        ),
      )

      await expect(
        putTruth('w', '"old"', { taiyaku_truth: 1, editions: {} }),
      ).rejects.toThrow(/changed since you read it/)
    })

    it('reports the etag as undefined when the response carries none — O35 leaves this to writeGate, not to this function', async () => {
      vi.stubGlobal(
        'fetch',
        vi
          .fn()
          .mockResolvedValue(
            new Response('{"taiyaku_truth":1,"editions":{}}', { status: 200 }),
          ),
      )

      const fresh = await getTruth('w')
      expect(fresh.etag).toBeUndefined()
    })
  })
})

describe('being signed out', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  /**
   * Eight call sites each throw on a non-ok response, and a 401 is not an
   * error the reader can show — it is a state change. Handled in one place so
   * a ninth call site cannot forget, and asserted over every one of them so
   * "handled in one place" is a fact rather than an intention.
   */
  it('raises Unauthenticated from every guarded call', async () => {
    globalThis.fetch = vi
      .fn()
      .mockResolvedValue(new Response('', { status: 401 })) as typeof fetch

    const calls: Array<[string, () => Promise<unknown>]> = [
      ['listWorks', () => listWorks()],
      ['getSidecar', () => getSidecar('t')],
      ['getProgress', () => getProgress('t')],
      ['putProgress', () => putProgress('t', { edition: 'raw', slot: 0 })],
      ['getNotes', () => getNotes('t')],
      ['getTruth', () => getTruth('t')],
      ['me', () => me()],
    ]
    for (const [name, call] of calls) {
      await expect(call(), name).rejects.toBeInstanceOf(Unauthenticated)
    }
  })

  it('leaves every other failure as an ordinary error', async () => {
    globalThis.fetch = vi
      .fn()
      .mockResolvedValue(new Response('nope', { status: 500 })) as typeof fetch
    const failure = await listWorks().catch((e: unknown) => e)
    expect(failure).toBeInstanceOf(Error)
    expect(failure).not.toBeInstanceOf(Unauthenticated)
  })
})

describe('signing in', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('posts the credentials as JSON', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    globalThis.fetch = fetchMock as typeof fetch

    await login('davey', 'a password')

    expect(fetchMock).toHaveBeenCalledWith('/api/session', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'davey', password: 'a password' }),
    })
  })

  /**
   * A 401 here is the ordinary answer to a wrong password rather than a
   * signed-out state to route on, so it stays an error with words a reader can
   * act on — routing to the login screen from the login screen would be a
   * loop that looks like nothing happening.
   */
  it('says what went wrong rather than routing away', async () => {
    for (const [status, expected] of [
      [401, /did not match/],
      [429, /Too many attempts/],
      [403, /origin/i],
    ] as const) {
      globalThis.fetch = vi
        .fn()
        .mockResolvedValue(new Response('', { status })) as typeof fetch
      const failure = await login('davey', 'x').catch((e: unknown) => e)
      expect(failure).toBeInstanceOf(Error)
      expect(failure).not.toBeInstanceOf(Unauthenticated)
      expect((failure as Error).message).toMatch(expected)
    }
  })

  it('signs out with a DELETE', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    globalThis.fetch = fetchMock as typeof fetch
    await logout()
    expect(fetchMock).toHaveBeenCalledWith('/api/session', { method: 'DELETE' })
  })
})

describe('the account calls', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('lists and creates keys, and returns the secret once', async () => {
    const listed = [{ id: 1, name: 'phone', created: 'then', last_used: null }]
    vi.stubGlobal(
      'fetch',
      vi.fn((_input: string, init?: RequestInit) => {
        if ((init?.method ?? 'GET') === 'POST')
          return Promise.resolve(
            new Response(JSON.stringify({ secret: 'tyk_once' }), {
              status: 200,
            }),
          )
        return Promise.resolve(
          new Response(JSON.stringify(listed), { status: 200 }),
        )
      }),
    )
    await expect(listKeys()).resolves.toEqual(listed)
    await expect(createKey('phone')).resolves.toBe('tyk_once')
  })

  it('revokes a key and a session', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await revokeKey(3)
    await revokeSession('abc')
    expect(fetchMock).toHaveBeenCalledWith('/api/keys/3', { method: 'DELETE' })
    expect(fetchMock).toHaveBeenCalledWith('/api/sessions/abc', {
      method: 'DELETE',
    })
  })

  it('lists sessions', async () => {
    const rows = [
      {
        id: 'aa',
        created: 'then',
        last_seen: 'now',
        user_agent: null,
        ip: null,
        current: true,
      },
    ]
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(new Response(JSON.stringify(rows), { status: 200 })),
    )
    await expect(listSessions()).resolves.toEqual(rows)
  })

  /**
   * A refused password change says which half was wrong. A 403 here means the
   * *current* password did not match — not that the session is invalid, which
   * is what a 401 would mean and what the guard answers instead.
   */
  it('distinguishes a wrong current password from any other failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 403 })),
    )
    await expect(changePassword('wrong', 'next')).rejects.toThrow(
      /current password did not match/,
    )

    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 500 })),
    )
    await expect(changePassword('a', 'b')).rejects.toThrow(
      /changing the password/,
    )

    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal('fetch', fetchMock)
    await changePassword('a', 'b')
    expect(fetchMock).toHaveBeenCalledWith('/api/password', {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ current: 'a', next: 'b' }),
    })
  })

  it('surfaces a failure from each listing rather than answering empty', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 500 })),
    )
    await expect(listKeys()).rejects.toThrow(/listing keys/)
    await expect(listSessions()).rejects.toThrow(/listing sessions/)
    await expect(createKey('x')).rejects.toThrow(/creating a key/)
    await expect(revokeKey(1)).rejects.toThrow(/revoking key/)
    await expect(revokeSession('x')).rejects.toThrow(/revoking a session/)
    await expect(me()).rejects.toThrow(/reading the account/)
    await expect(logout()).rejects.toThrow(/signing out/)
  })
})

describe('being signed out is routed once, from one place', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    whenSignedOut(() => undefined)
  })

  /**
   * The failure this exists to prevent: a session revoked from another browser
   * while the reader keeps turning pages. Every write's `.catch` swallows the
   * rejection deliberately — a failed position write is not something to show —
   * so the hook has to fire regardless of who catches what.
   */
  it('fires even when the caller swallows the rejection', async () => {
    globalThis.fetch = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 401 })) as typeof fetch
    const signedOut = vi.fn()
    whenSignedOut(signedOut)

    await putProgress('t', { edition: 'raw', slot: 3 }).catch(() => undefined)
    expect(signedOut).toHaveBeenCalled()
  })

  it('fires for every call routed through the wrapper, not just the reads', async () => {
    globalThis.fetch = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 401 })) as typeof fetch

    const calls: Array<[string, () => Promise<unknown>]> = [
      ['listWorks', () => listWorks()],
      ['getSidecar', () => getSidecar('t')],
      ['getProgress', () => getProgress('t')],
      ['putProgress', () => putProgress('t', { edition: 'raw', slot: 0 })],
      ['getNotes', () => getNotes('t')],
      ['putNotes', () => putNotes('t', 'tag', { work: 't', notes: [] })],
      ['getTruth', () => getTruth('t')],
      [
        'putTruth',
        () => putTruth('t', 'tag', { taiyaku_truth: 1, editions: {} }),
      ],
      [
        'patchAlignment',
        () =>
          patchAlignment('t', 'tag', { from: 'a', to: 'b', a: '0', b: '0' }),
      ],
      ['me', () => me()],
      ['listKeys', () => listKeys()],
      ['createKey', () => createKey('x')],
      ['revokeKey', () => revokeKey(1)],
      ['listSessions', () => listSessions()],
      ['revokeSession', () => revokeSession('x')],
      ['changePassword', () => changePassword('a', 'b')],
    ]

    for (const [name, call] of calls) {
      const signedOut = vi.fn()
      whenSignedOut(signedOut)
      await call().catch(() => undefined)
      expect(signedOut, `${name} did not route its 401`).toHaveBeenCalled()
    }
  })
})
