import { describe, expect, it } from 'vitest'
import type { WorkDetail } from '../src/library'
import { readerName, titleLines } from '../src/readerTitle'

const detail = (over: Partial<WorkDetail> = {}): WorkDetail => ({
  id: 1,
  slug: 'dandadan-v01',
  title: 'Dandadan',
  volume: 1,
  aligned: true,
  pages: 207,
  editions: [
    {
      key: 'raw',
      label: 'Raw, monochrome',
      members: 1,
      pages: 207,
      missing: 0,
      cover: '/api/works/dandadan-v01/raw/pages/0/thumb?v=c0ffee-0-1',
    },
    {
      key: 'en',
      label: 'English official',
      members: 1,
      pages: 200,
      missing: 0,
      cover: '/api/works/dandadan-v01/en/pages/0/thumb?v=c0ffee-0-1',
    },
  ],
  opens: 'raw',
  alignments: [],
  ...over,
})

describe('titleLines', () => {
  it('names the edition by its key alone until the library answers', () => {
    expect(titleLines(undefined, 'raw')).toEqual({ editionLine: 'raw' })
  })

  it('gives the work title, and the volume with the label of the edition on screen', () => {
    expect(titleLines(detail(), 'en')).toEqual({
      title: 'Dandadan',
      editionLine: 'Vol. 1 · English official',
    })
  })

  it('gives the label alone for a work with no volume', () => {
    expect(titleLines(detail({ volume: null }), 'raw').editionLine).toBe(
      'Raw, monochrome',
    )
  })

  it('keeps a fractional volume as it is', () => {
    expect(titleLines(detail({ volume: 10.5 }), 'raw').editionLine).toBe(
      'Vol. 10.5 · Raw, monochrome',
    )
  })

  it('falls back to the key for an edition the library did not list', () => {
    expect(titleLines(detail(), 'en-fan').editionLine).toBe('Vol. 1 · en-fan')
  })
})

describe('readerName — O163, O173', () => {
  it('joins the work and its edition line, as the top bar draws them', () => {
    expect(
      readerName({
        title: 'Dandadan',
        editionLine: 'Vol. 1 · Raw, monochrome',
      }),
    ).toBe('Dandadan · Vol. 1 · Raw, monochrome')
  })

  it('says the edition line alone before the work’s detail arrives', () => {
    expect(readerName({ editionLine: 'raw' })).toBe('raw')
  })
})
