import { describe, expect, it } from 'vitest'
import type { FetchedTruth } from './api'
import type { LabelBox } from './labelling'
import type { TruthDoc, TruthRegion } from './truth'
import {
  boxesToRegions,
  commitSave,
  editionsFromSidecar,
  frameOfImage,
  type ImageMeasure,
  openWork,
  pageRegions,
  pointOnPage,
  regionsToBoxes,
  saveTruth,
  truthWriteGate,
  type TruthPorts,
  visitedPages,
  withPage,
} from './truthWrite'

const emptyDoc = (): TruthDoc => ({ taiyaku_truth: 1, editions: {} })

const region = (over: Partial<TruthRegion> = {}): TruthRegion => ({
  x: 0.1,
  y: 0.2,
  w: 0.3,
  h: 0.4,
  axis: 'v',
  kind: 'dialogue',
  ignore: false,
  ...over,
})

const members = [{ sha256: 'aa' }]

// ---------------------------------------------------------------------------
// Absent vs. empty — the labelling progress tracker.

describe('withPage / pageRegions', () => {
  it('a page opened and left empty writes a present-but-empty key', () => {
    const written = withPage(emptyDoc(), 'raw-mono', members, 5, [])
    expect(pageRegions(written, 'raw-mono', 5)).toEqual([])
  })

  it('a page never opened has no key at all', () => {
    const written = withPage(emptyDoc(), 'raw-mono', members, 5, [])
    expect(pageRegions(written, 'raw-mono', 6)).toBeUndefined()
  })

  it('a page with real boxes keeps them, distinct from an opened-and-empty one', () => {
    const written = withPage(emptyDoc(), 'raw-mono', members, 7, [region()])
    expect(pageRegions(written, 'raw-mono', 7)).toEqual([region()])
  })

  it('touching one page leaves an already-present page untouched', () => {
    let doc = withPage(emptyDoc(), 'raw-mono', members, 3, [region()])
    doc = withPage(doc, 'raw-mono', members, 4, [])
    expect(pageRegions(doc, 'raw-mono', 3)).toEqual([region()])
    expect(pageRegions(doc, 'raw-mono', 4)).toEqual([])
  })

  it('touching a page leaves other editions of the same work untouched', () => {
    let doc = withPage(emptyDoc(), 'raw-mono', members, 3, [region()])
    doc = withPage(doc, 'en-mono', members, 3, [])
    expect(pageRegions(doc, 'raw-mono', 3)).toEqual([region()])
    expect(pageRegions(doc, 'en-mono', 3)).toEqual([])
  })

  it('stamps the edition with the sidecar members handed in, every time', () => {
    let doc = withPage(emptyDoc(), 'raw-mono', [{ sha256: 'old' }], 1, [])
    doc = withPage(doc, 'raw-mono', [{ sha256: 'new' }], 2, [])
    expect(doc.editions['raw-mono'].members).toEqual([{ sha256: 'new' }])
  })

  it('does not mutate the document handed in', () => {
    const before = emptyDoc()
    withPage(before, 'raw-mono', members, 1, [region()])
    expect(before.editions).toEqual({})
  })
})

describe('visitedPages', () => {
  it('lists every page index with a key, present or empty', () => {
    let doc = withPage(emptyDoc(), 'raw-mono', members, 3, [region()])
    doc = withPage(doc, 'raw-mono', members, 8, [])
    expect(visitedPages(doc, 'raw-mono')).toEqual(new Set([3, 8]))
  })

  it('is empty for an edition nobody has opened, and for no document at all', () => {
    expect(visitedPages(emptyDoc(), 'raw-mono')).toEqual(new Set())
    expect(visitedPages(undefined, 'raw-mono')).toEqual(new Set())
  })
})

// ---------------------------------------------------------------------------
// writeGate: refused, not silently dropped.

describe('truthWriteGate', () => {
  it('refuses out loud when there is a document but no validator', () => {
    expect(truthWriteGate(emptyDoc(), undefined)).toEqual({
      kind: 'refuse',
      notice: expect.stringContaining('These labels are not being saved'),
    })
  })

  it('writes against the validator it was given', () => {
    const doc = emptyDoc()
    expect(truthWriteGate(doc, 'v1')).toEqual({
      kind: 'write',
      etag: 'v1',
      doc,
    })
  })

  it('waits, silently, before anything has loaded', () => {
    expect(truthWriteGate(undefined, undefined)).toEqual({ kind: 'wait' })
  })
})

describe('saveTruth / commitSave', () => {
  it('a write whose response carries no ETag leaves the next write refused', async () => {
    // O35's shape, for truth: a PUT that lands but answers with no ETag —
    // an old server, or a proxy that stripped the header — must not let the
    // caller believe it can still write conditionally.
    const puts: { work: string; etag: string; doc: TruthDoc }[] = []
    const ports: Pick<TruthPorts, 'put'> = {
      put(work, etag, doc) {
        puts.push({ work, etag, doc })
        return Promise.resolve({ doc, etag: undefined })
      },
    }
    const doc = withPage(emptyDoc(), 'raw-mono', members, 0, [])
    const first = await saveTruth('gotoubun', 'v1', doc, ports)
    expect(first).toEqual({ kind: 'written', doc, etag: undefined })

    // The caller stores `first.etag` (undefined) and tries again — the gate
    // this whole path exists for.
    const second = truthWriteGate(
      doc,
      first.kind === 'written' ? first.etag : 'unreachable',
    )
    expect(second).toEqual({
      kind: 'refuse',
      notice: expect.stringContaining('These labels are not being saved'),
    })
    expect(puts).toHaveLength(1) // the refused attempt never reached `put`
  })

  it('commitSave refuses through the dispatch, and never calls put, when there is no validator', async () => {
    const puts: unknown[] = []
    const ports: Pick<TruthPorts, 'put'> = {
      put: (...args) => {
        puts.push(args)
        throw new Error('unreachable')
      },
    }
    const notices: (string | undefined)[] = []
    const etags: (string | undefined)[] = []
    await commitSave('gotoubun', emptyDoc(), undefined, ports, {
      setEtag: (e) => etags.push(e),
      setNotice: (n) => notices.push(n),
    })
    expect(puts).toEqual([])
    expect(notices).toEqual([
      expect.stringContaining('These labels are not being saved'),
    ])
    expect(etags).toEqual([])
  })

  it('commitSave stores the fresh etag and clears the notice on a landed write', async () => {
    const ports: Pick<TruthPorts, 'put'> = {
      put: (_work, _etag, doc) => Promise.resolve({ doc, etag: 'v2' }),
    }
    const notices: (string | undefined)[] = []
    const etags: (string | undefined)[] = []
    await commitSave('gotoubun', emptyDoc(), 'v1', ports, {
      setEtag: (e) => etags.push(e),
      setNotice: (n) => notices.push(n),
    })
    expect(etags).toEqual(['v2'])
    expect(notices).toEqual([undefined])
  })

  it('commitSave reports a server refusal through the notice, and does not touch the etag', async () => {
    const ports: Pick<TruthPorts, 'put'> = {
      put: () => Promise.reject(new Error('writing truth: 412 stale')),
    }
    const notices: (string | undefined)[] = []
    const etags: (string | undefined)[] = []
    await commitSave('gotoubun', emptyDoc(), 'v1', ports, {
      setEtag: (e) => etags.push(e),
      setNotice: (n) => notices.push(n),
    })
    expect(notices).toEqual(['writing truth: 412 stale'])
    expect(etags).toEqual([])
  })
})

// ---------------------------------------------------------------------------
// LabelBox <-> TruthRegion.

describe('regionsToBoxes / boxesToRegions', () => {
  it('round-trips a region through a box and back, content unchanged', () => {
    const regions = [
      region({ ignore: true }),
      region({ axis: null, kind: 'sfx' }),
    ]
    expect(boxesToRegions(regionsToBoxes(regions))).toEqual(regions)
  })

  it('defaults a missing ignore flag to false, matching the server default', () => {
    const bare: TruthRegion = {
      x: 0,
      y: 0,
      w: 0.1,
      h: 0.1,
      axis: null,
      kind: 'other',
    }
    expect(regionsToBoxes([bare])[0].ignore).toBe(false)
  })

  it('mints a distinct id per box, not shared across a page', () => {
    const [a, b] = regionsToBoxes([region(), region()])
    expect(a.id).not.toBe(b.id)
  })

  it('drops id and text on the way back to the wire — text is reserved and this editor never sets it', () => {
    const box: LabelBox = {
      id: 'x',
      x: 0,
      y: 0,
      w: 0.1,
      h: 0.1,
      kind: 'other',
      axis: null,
      ignore: false,
    }
    const [wire] = boxesToRegions([box])
    expect(wire).not.toHaveProperty('id')
    expect(wire).not.toHaveProperty('text')
  })
})

// ---------------------------------------------------------------------------
// The sidecar: which editions, how many pages, which member digests.

describe('editionsFromSidecar', () => {
  const sidecar = () =>
    JSON.stringify({
      hub: 'raw-mono',
      editions: {
        'raw-mono': {
          members: [
            { sha256: 'aa', pages: 20 },
            { sha256: 'bb', pages: 5 },
          ],
        },
        'en-mono': { members: [{ sha256: 'cc', pages: 25 }] },
      },
    })

  it('orders editions hub-first, the same rule App.tsx reads them by', () => {
    expect(editionsFromSidecar(sidecar()).editions).toEqual([
      'raw-mono',
      'en-mono',
    ])
  })

  it('sums each edition members own page counts', () => {
    const info = editionsFromSidecar(sidecar())
    expect(info.byEdition['raw-mono'].pageCount).toBe(25)
    expect(info.byEdition['en-mono'].pageCount).toBe(25)
  })

  it('carries each members digest, for withPage to stamp a fresh edition entry with', () => {
    const info = editionsFromSidecar(sidecar())
    expect(info.byEdition['raw-mono'].members).toEqual([
      { sha256: 'aa' },
      { sha256: 'bb' },
    ])
  })

  it('carries the work title, the one field that differs between the four works', () => {
    const titled = JSON.stringify({
      hub: 'raw-mono',
      work: { title: 'Go-Toubun no Hanayome v01', reading: 'rtl' },
      editions: { 'raw-mono': { members: [{ sha256: 'aa', pages: 3 }] } },
    })
    expect(editionsFromSidecar(titled).title).toBe('Go-Toubun no Hanayome v01')
  })

  it('parses a sidecar with no work block at all rather than throwing', () => {
    // `taiyaku-core`'s `Work::title` is a required `String`, so a sidecar this
    // server wrote always has one. This text arrives over the wire and is
    // parsed against a hand-written interface rather than a validated schema,
    // and reaching into an absent `work` would throw inside `openWork` and
    // take the edition and page pickers down with it — losing the whole
    // instrument over a cosmetic field.
    const info = editionsFromSidecar(sidecar())
    expect(info.title).toBeUndefined()
    expect(info.editions).toEqual(['raw-mono', 'en-mono'])
  })
})

describe('openWork', () => {
  it('reads the sidecar and the truth document, and reports both together', async () => {
    const sidecar = JSON.stringify({
      hub: 'raw-mono',
      editions: { 'raw-mono': { members: [{ sha256: 'aa', pages: 3 }] } },
    })
    const doc = withPage(emptyDoc(), 'raw-mono', members, 0, [])
    const ports: Pick<TruthPorts, 'sidecar' | 'get'> = {
      sidecar: () => Promise.resolve(sidecar),
      get: (): Promise<FetchedTruth> => Promise.resolve({ doc, etag: 'v1' }),
    }
    const opened = await openWork(ports, 'gotoubun')
    expect(opened.info.editions).toEqual(['raw-mono'])
    expect(opened.doc).toBe(doc)
    expect(opened.etag).toBe('v1')
  })
})

// ---------------------------------------------------------------------------
// Coordinates: against the image, not the element, when the two differ.

describe('pointOnPage', () => {
  const container = { left: 0, top: 0, width: 400, height: 400 }

  it('measures against the container when no image is on screen — the standalone editor', () => {
    expect(pointOnPage(container, undefined, 200, 100)).toEqual({
      x: 0.5,
      y: 0.25,
    })
  })

  it('measures against the painted image, not the element, when the two differ', () => {
    // A 200x100 natural image rendered to fill a 400x400 element: `paintedBox`
    // scales it to 400x200, letterboxed 100px top and bottom. The element's
    // own rect is the full 400x400 box.
    const image: ImageMeasure = {
      rect: { left: 0, top: 0, width: 400, height: 400 },
      naturalWidth: 200,
      naturalHeight: 100,
    }
    // y=100 sits at the very top edge of the painted art (which runs
    // y=100..300) — fraction 0.
    expect(pointOnPage(container, image, 200, 100)).toEqual({ x: 0.5, y: 0 })
    // The same point measured with no image at all (the fallback above) gives
    // the different, wrong-for-a-real-page answer — proving the two paths
    // actually disagree rather than coincidentally matching.
    expect(pointOnPage(container, undefined, 200, 100)).toEqual({
      x: 0.5,
      y: 0.25,
    })
  })

  it('measures against an image offset from its container', () => {
    const image: ImageMeasure = {
      rect: { left: 50, top: 0, width: 200, height: 200 },
      naturalWidth: 200,
      naturalHeight: 200,
    }
    expect(pointOnPage(container, image, 150, 100)).toEqual({ x: 0.5, y: 0.5 })
  })
})

describe('frameOfImage', () => {
  const container = { left: 10, top: 10, width: 400, height: 400 }

  it('fills the container when no image is on screen', () => {
    expect(frameOfImage(container, undefined)).toEqual({
      left: 0,
      top: 0,
      width: 400,
      height: 400,
    })
  })

  it('sits over the painted image, inset within its element, not the whole container', () => {
    const image: ImageMeasure = {
      rect: { left: 10, top: 10, width: 400, height: 400 },
      naturalWidth: 200,
      naturalHeight: 100,
    }
    expect(frameOfImage(container, image)).toEqual({
      left: 0,
      top: 100,
      width: 400,
      height: 200,
    })
  })
})
