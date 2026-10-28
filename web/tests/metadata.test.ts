import { describe, expect, it, vi } from 'vitest'
import {
  patchEdition,
  patchMember,
  patchSeries,
  patchWork,
  reasons,
  type PatchResult,
} from '../src/metadata'

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })

describe('patchWork', () => {
  it('sends only the fields it was given', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      json({
        taiyaku_library: 1,
        updated: 't',
        title: 'New',
        persisted: true,
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const result = await patchWork('w', { title: 'New' })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/works/w',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ title: 'New' }),
      }),
    )
    expect(result.persisted).toBe(true)
  })

  it('encodes a slug that needs it', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        json({ taiyaku_library: 1, updated: 't', persisted: true }),
      )
    vi.stubGlobal('fetch', fetchMock)

    await patchWork('a/b', { title: 'New' })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/works/a%2Fb',
      expect.anything(),
    )
  })

  it('reports a document that could not be written to disk', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        json({
          taiyaku_library: 1,
          updated: 't',
          title: 'New',
          persisted: false,
        }),
      ),
    )

    const result = await patchWork('w', { title: 'New' })

    expect(result.persisted).toBe(false)
  })

  // The brief's own illustrative tests never exercise `why` at all — both
  // cases below are added because the shipped server's `why` is not the
  // always-an-array shape the brief assumed.
  it('leaves `why` absent on the ordinary path, rather than defaulting it to an empty array', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        json({
          taiyaku_library: 1,
          updated: 't',
          title: 'New',
          persisted: true,
        }),
      ),
    )

    const result = await patchWork('w', { title: 'New' })

    expect(result.why).toBeUndefined()
    expect('why' in result).toBe(false)
    expect(reasons(result)).toEqual([])
  })

  it('carries every reason a fanned-out edit could not persist', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        json({
          taiyaku_library: 1,
          updated: 't',
          persisted: false,
          why: [
            'writing /roots/a/.taiyaku/x.library.json: read-only file system (os error 30)',
          ],
        }),
      ),
    )

    const result = await patchWork('w', { title: 'New' })

    expect(result.persisted).toBe(false)
    expect(reasons(result)).toEqual([
      'writing /roots/a/.taiyaku/x.library.json: read-only file system (os error 30)',
    ])
  })

  // A hard failure (409/404/403) is a bare string, never JSON — unlike the
  // 200 responses above. A client that called `res.json()` on this body
  // would throw a confusing "Unexpected token" instead of the server's own
  // sentence, which is exactly the failure `sendJson` (and this test) rules
  // out. Not in the brief, whose illustrative tests only ever mock a 200.
  it('propagates a 409 refusal as the server’s own bare-string body, not JSON', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('a series edit named no title to attach', {
          status: 409,
          headers: { 'content-type': 'text/plain; charset=utf-8' },
        }),
      ),
    )

    await expect(patchWork('w', { title: 'New' })).rejects.toThrow(
      '/api/library/works/w: 409 a series edit named no title to attach',
    )
  })

  it('propagates a 404 for a slug naming nothing', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('no such work, edition, member or series', {
          status: 404,
        }),
      ),
    )

    await expect(patchWork('nonexistent', { title: 'New' })).rejects.toThrow(
      '/api/library/works/nonexistent: 404 no such work, edition, member or series',
    )
  })

  // No route in `crates/taiyaku-serve/src/metadata.rs` ever answers 204 — every
  // branch answers a document or a bare-string refusal — but `sendJson` is
  // shared with routes that do, so this guard exists and is worth proving
  // fires rather than left as an unreachable throw nothing exercises.
  it('refuses to answer with no document at all, if the server ever sent an empty 204', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 204 })),
    )

    await expect(patchWork('w', { title: 'New' })).rejects.toThrow(
      'expected a JSON body, got an empty 204',
    )
  })

  it('propagates a 403 for a non-admin', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('editing library metadata is an administrator’s', {
          status: 403,
        }),
      ),
    )

    await expect(patchWork('w', { title: 'New' })).rejects.toThrow(
      /^\/api\/library\/works\/w: 403/,
    )
  })
})

describe('patchSeries', () => {
  it('sends only the fields it was given, to the series id in the path', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        json({ taiyaku_library: 1, updated: 't', persisted: true }),
      )
    vi.stubGlobal('fetch', fetchMock)

    const result = await patchSeries(7, { tags: ['seinen'] })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/series/7',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ tags: ['seinen'] }),
      }),
    )
    expect(result.persisted).toBe(true)
  })
})

describe('patchEdition', () => {
  it('sends only the fields it was given, to the edition id in the path', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        json({ taiyaku_library: 1, updated: 't', persisted: true }),
      )
    vi.stubGlobal('fetch', fetchMock)

    const result = await patchEdition(3, { label: 'Perfect Edition' })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/editions/3',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ label: 'Perfect Edition' }),
      }),
    )
    expect(result.persisted).toBe(true)
  })
})

describe('patchMember', () => {
  it('sends only `chapter`, never a `chapter_source` the caller did not set', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        json({ taiyaku_library: 1, updated: 't', persisted: true }),
      )
    vi.stubGlobal('fetch', fetchMock)

    const result = await patchMember(11, { chapter: 4.5 })

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/members/11',
      expect.objectContaining({
        method: 'PATCH',
        body: JSON.stringify({ chapter: 4.5 }),
      }),
    )
    expect(result.persisted).toBe(true)
  })

  it('sends `{}` rather than a `chapter: undefined` key when asked to change nothing', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(
        json({ taiyaku_library: 1, updated: 't', persisted: true }),
      )
    vi.stubGlobal('fetch', fetchMock)

    await patchMember(11, {})

    expect(fetchMock).toHaveBeenCalledWith(
      '/api/library/members/11',
      expect.objectContaining({ method: 'PATCH', body: JSON.stringify({}) }),
    )
  })
})

describe('reasons', () => {
  it('defaults an absent `why` to an empty array', () => {
    const result: PatchResult = {
      taiyaku_library: 1,
      updated: 't',
      persisted: true,
    }
    expect(reasons(result)).toEqual([])
  })

  it('passes a present `why` through unchanged', () => {
    const result: PatchResult = {
      taiyaku_library: 1,
      updated: 't',
      persisted: false,
      why: ['one reason', 'another reason'],
    }
    expect(reasons(result)).toEqual(['one reason', 'another reason'])
  })
})
