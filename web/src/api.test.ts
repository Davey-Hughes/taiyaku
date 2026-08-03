import { afterEach, describe, expect, it, vi } from 'vitest'
import { getProgress, getSidecar, listWorks, pageUrl, putProgress, thumbUrl } from './api'

describe('url building', () => {
  it('addresses a page by flat index', () => {
    expect(pageUrl('go-toubun-v01', 'en-mono', 47))
      .toBe('/api/works/go-toubun-v01/en-mono/pages/47')
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

  it('listWorks returns the parsed array of work ids', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(['taiyaku']), { status: 200 }),
    )
    vi.stubGlobal('fetch', fetchMock)

    await expect(listWorks()).resolves.toEqual(['taiyaku'])
    expect(fetchMock).toHaveBeenCalledWith('/api/works')
  })

  it('listWorks rejects with the server body on a non-2xx', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response('no works configured', { status: 500 })),
    )

    await expect(listWorks()).rejects.toThrow(/no works configured/)
  })

  it('getSidecar returns the sidecar verbatim as text, unparsed', async () => {
    const body = '{"editions":[]}'
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body, { status: 200 })))

    await expect(getSidecar('taiyaku')).resolves.toBe(body)
  })

  it('getProgress resolves to undefined on a 204 (nothing recorded)', async () => {
    // Response.ok is true for 204, so this only stays correct if the 204
    // check runs before the !res.ok check — reordering those two lines would
    // route a "nothing recorded" response into res.json() on an empty body.
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 204 })))

    await expect(getProgress('taiyaku')).resolves.toBeUndefined()
  })

  it('getProgress resolves to the parsed position on a 200', async () => {
    const progress = { edition: 'en-mono', slot: 12 }
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(JSON.stringify(progress), { status: 200 })),
    )

    await expect(getProgress('taiyaku')).resolves.toEqual(progress)
  })

  it('getProgress rejects on a non-2xx, carrying the server\'s own wording', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response('unknown work "taiyaku"', { status: 404 })),
    )

    await expect(getProgress('taiyaku')).rejects.toThrow(/unknown work "taiyaku"/)
  })

  it('putProgress resolves on success and sends the position as JSON', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response(null, { status: 204 }))
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
      vi.fn().mockResolvedValue(new Response('slot 12 is out of range for en-mono', {
        status: 400,
      })),
    )

    await expect(putProgress('taiyaku', { edition: 'en-mono', slot: 12 }))
      .rejects.toThrow(/slot 12 is out of range for en-mono/)
  })
})
