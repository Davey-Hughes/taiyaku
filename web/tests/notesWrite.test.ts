/**
 * The ten cases of the extraction map's §4.1a.
 *
 * Every one of them is an assertion about *which arguments the ports received*
 * or *which outcome came back* — no DOM, no component, no timers. That is the
 * whole reason `saveNotes` takes ports rather than living in a hook: four of
 * these cases are assertions on an argument or a call count, and there is no
 * way to make one of those from outside a mounted component.
 *
 * Cases 2, 3, 4 and 9 were written against the *correct* semantics and watched
 * failing against a faithful port of the save path `App.tsx` had. The other
 * six are controls that must pass on both sides of the fix: the fix changes
 * the validator the retry writes against, and nothing else.
 */
import { describe, expect, it } from 'vitest'
import type { FetchedNotes } from '../src/api'
import { mergeOver, type Note, type NotesDoc } from '../src/notes'
import { saveNotes, writeGate, type NotesPorts } from '../src/notesWrite'

const WORK = 'gotoubun'
const T = '2026-08-09T10:00:00.000Z'

const note = (id: string, body = id): Note => ({
  id,
  edition: 'jp',
  slot: 3,
  body,
  created: T,
  updated: T,
})

const KEPT_ON_TOP =
  'These notes changed on another device; yours were kept on top.'

/**
 * The server's own sentence, from `taiyaku-serve`'s `routes.rs` `put_notes`,
 * wrapped the way `api.ts` throws it.
 *
 * Verbatim rather than invented: the match in `saveNotes` is by substring
 * because `api.ts` only promises the status is somewhere in the message, so a
 * made-up sentence would leave case 8 testing nothing.
 */
const stale = (tag: string) =>
  new Error(
    `writing notes: 412 these notes have changed since you read them; ` +
      `their ETag is now "${tag}". Re-read the notes and write again.`,
  )

interface PutCall {
  work: string
  etag: string
  doc: NotesDoc
}

/** A port pair that records every call and answers from the two callbacks. */
function scripted(script: {
  put?: (call: PutCall, attempt: number) => Promise<FetchedNotes>
  get?: (work: string, attempt: number) => Promise<FetchedNotes>
}) {
  const puts: PutCall[] = []
  const gets: string[] = []
  const ports: NotesPorts = {
    put(work, etag, doc) {
      const call = { work, etag, doc }
      puts.push(call)
      if (!script.put) throw new Error('unscripted put')
      return script.put(call, puts.length)
    },
    get(work) {
      gets.push(work)
      if (!script.get) throw new Error('unscripted get')
      return script.get(work, gets.length)
    },
  }
  return { puts, gets, ports }
}

/**
 * A stand-in for the notes routes: one document, one validator, and a `put`
 * that refuses any `If-Match` but the current one.
 *
 * A conditional server rather than a queue of canned answers, because the
 * whole question here is which validator the second write carries — a `put`
 * that resolved whatever it was sent would answer "the retry landed" for an
 * implementation that sent the refused validator again, which is precisely the
 * defect these cases exist to catch.
 *
 * `elsewhere` is the other device: it replaces the document and mints a new
 * validator, which is the only way to reach the 412 path at all.
 */
function server(notes: Note[], work = WORK) {
  let doc: NotesDoc = { work, notes }
  let tag = 'v1'
  let minted = 0
  const puts: PutCall[] = []
  const gets: { work: string; etag: string | undefined }[] = []
  return {
    puts,
    gets,
    get etag() {
      return tag
    },
    get doc() {
      return doc
    },
    elsewhere(next: Note[]) {
      doc = { work, notes: next }
      tag = `elsewhere-${++minted}`
    },
    ports: {
      put(sentWork, ifMatch, sent) {
        puts.push({ work: sentWork, etag: ifMatch, doc: sent })
        if (ifMatch !== tag) return Promise.reject(stale(tag))
        doc = sent
        tag = `mine-${++minted}`
        return Promise.resolve({ doc, etag: tag })
      },
      get(asked) {
        gets.push({ work: asked, etag: tag })
        return Promise.resolve({ doc, etag: tag })
      },
    } satisfies NotesPorts,
  }
}

/**
 * `writeGate`, and the silence it used to keep.
 *
 * Same discipline as the ten above and for the same reason: the refusal case
 * was written first, watched failing against a faithful port of today's
 * `if (!notesDoc || notesEtag === undefined) return`, and only then fixed.
 *
 * The reader-facing sentence is spelled out here rather than imported. It is
 * the only thing anybody sees when their notes stop reaching the server, so a
 * change to its wording should have to be made twice, on purpose.
 */
const NO_VALIDATOR =
  'These notes are not being saved: this device has no current copy of them ' +
  'to write against. Reload the page.'

describe('writeGate', () => {
  it('writes against the validator it was given', () => {
    const doc: NotesDoc = { work: WORK, notes: [note('a')] }
    expect(writeGate(doc, 'v1')).toEqual({ kind: 'write', etag: 'v1', doc })
  })

  // The defect. Watched failing against the faithful port, which answered
  // `wait` here and so wrote nothing, said nothing, and let the reader keep
  // typing into a session that could no longer save.
  //
  // Two things arrive at this state and neither is exotic: a load whose notes
  // read failed (`startup.ts`'s `Opened.notesEtag`, absent), and a `PUT` whose
  // response carried no `ETag` (`api.ts`'s `putNotes`, `?? undefined`, which
  // `App.tsx` stores). After either, every later write is refused — which is
  // right, since this reader never writes without a validator (§4.1a, case 9).
  // Refusing *silently* is what is not.
  it('refuses out loud when there is a document but no validator', () => {
    expect(writeGate({ work: WORK, notes: [] }, undefined)).toEqual({
      kind: 'refuse',
      notice: NO_VALIDATOR,
    })
  })

  // A control, passing on both sides of the fix. Before the load has answered
  // there is no document, nothing has been asked for, and a notice would be
  // the reader being told about a failure that has not happened.
  it('waits, silently, before anything has loaded', () => {
    expect(writeGate(undefined, undefined)).toEqual({ kind: 'wait' })
    expect(writeGate(undefined, 'v1')).toEqual({ kind: 'wait' })
  })
})

describe('saveNotes', () => {
  // 1
  it('writes, and does not re-read, when the validator is current', async () => {
    const s = server([note('a')])
    const out = await saveNotes(
      WORK,
      s.etag,
      s.doc,
      [note('a'), note('b')],
      s.ports,
    )

    expect(out).toEqual({
      kind: 'written',
      doc: { work: WORK, notes: [note('a'), note('b')] },
      etag: s.etag,
    })
    expect(s.gets).toEqual([])
  })

  // 2 — the defect. Watched failing against the faithful port.
  it('retries against the validator the re-read returned', async () => {
    const s = server([note('a')])
    const mine = s.etag
    const doc = s.doc
    s.elsewhere([note('a'), note('c')])

    await saveNotes(WORK, mine, doc, [note('a'), note('b')], s.ports)

    expect(s.puts).toHaveLength(2)
    expect(s.puts[0].etag).toBe(mine)
    expect(s.puts[1].etag).toBe(s.gets[0].etag)
  })

  // 3 — the defect. Watched failing against the faithful port.
  it('refuses when the retry is refused too, and says why', async () => {
    const second = stale('v3')
    const s = scripted({
      put: (_call, attempt) =>
        Promise.reject(attempt === 1 ? stale('v2') : second),
      get: () =>
        Promise.resolve({
          doc: { work: WORK, notes: [note('c')] },
          etag: 'v2',
        }),
    })

    const out = await saveNotes(
      WORK,
      'v1',
      { work: WORK, notes: [] },
      [],
      s.ports,
    )

    expect(out).toEqual({ kind: 'refused', notice: second.message })
  })

  // 4 — the defect. Watched failing against the faithful port.
  it('reports the merge as landed only with the write that landed it', async () => {
    const s = server([note('a')])
    const mine = s.etag
    const doc = s.doc
    s.elsewhere([note('a'), note('c')])

    const out = await saveNotes(
      WORK,
      mine,
      doc,
      [note('a'), note('b')],
      s.ports,
    )

    expect(out).toEqual({
      kind: 'merged',
      doc: { work: WORK, notes: [note('a'), note('c'), note('b')] },
      etag: s.etag,
      notice: KEPT_ON_TOP,
    })
    // The second write's validator, not the re-read's — the two differ, which
    // is what makes the assertion above mean something.
    expect(s.etag).not.toBe(s.gets[0].etag)
  })

  // 5 — a control. Today's `console.error(rereadErr)` path, pinned.
  it('shows the original refusal when the re-read itself fails', async () => {
    const first = stale('v2')
    const offline = new Error('reading notes: 503 the server is away')
    const s = scripted({
      put: () => Promise.reject(first),
      get: () => Promise.reject(offline),
    })

    const out = await saveNotes(
      WORK,
      'v1',
      { work: WORK, notes: [] },
      [],
      s.ports,
    )

    expect(out).toEqual({
      kind: 'refused',
      notice: first.message,
      error: offline,
    })
  })

  // 6 — a control. "Retried once and once only" (`saveNotes`'s doc), pinned.
  it('retries once and once only', async () => {
    const s = scripted({
      put: (_call, attempt) => Promise.reject(stale(`v${attempt + 1}`)),
      get: () =>
        Promise.resolve({ doc: { work: WORK, notes: [] }, etag: 'v2' }),
    })

    await saveNotes(WORK, 'v1', { work: WORK, notes: [] }, [], s.ports)

    expect(s.puts).toHaveLength(2)
    expect(s.gets).toHaveLength(1)
  })

  // 7 — a control. The fix changes the validator, not the merge rule.
  it('sends the other device’s notes with this reader’s laid over them', async () => {
    const s = server([note('a', 'first')])
    const mine = s.etag
    const doc = s.doc
    const theirs = [note('a', 'theirs'), note('only-theirs')]
    const ours = [note('a', 'ours'), note('only-ours')]
    s.elsewhere(theirs)

    await saveNotes(WORK, mine, doc, ours, s.ports)

    expect(s.puts[1].doc.notes).toEqual(mergeOver(theirs, ours))
    expect(s.puts[1].doc.notes).toEqual([
      note('a', 'ours'),
      note('only-theirs'),
      note('only-ours'),
    ])
  })

  // 8 — a control. Anything that is not a stale validator is not retried.
  it('does not re-read for a failure that is not a stale validator', async () => {
    const broken = new Error('writing notes: 500 the disk is full')
    const s = scripted({ put: () => Promise.reject(broken) })

    const out = await saveNotes(
      WORK,
      'v1',
      { work: WORK, notes: [] },
      [],
      s.ports,
    )

    expect(out).toEqual({ kind: 'refused', notice: broken.message })
    expect(s.gets).toEqual([])
  })

  // 9 — the defect, in a state today's code cannot reach. Watched failing.
  //
  // A server old enough to answer the notes route without an `ETag` leaves
  // nothing to write conditionally against, and this reader never writes
  // without one. Refusing is what the load path already does when the notes
  // read fails; an unconditional write would be the only place in the reader
  // that guessed.
  it('refuses rather than writing without a validator to write against', async () => {
    const first = stale('v2')
    const s = scripted({
      put: () => Promise.reject(first),
      get: () =>
        Promise.resolve({
          doc: { work: WORK, notes: [note('c')] },
          etag: undefined,
        }),
    })

    const out = await saveNotes(
      WORK,
      'v1',
      { work: WORK, notes: [] },
      [],
      s.ports,
    )

    expect(out).toEqual({ kind: 'refused', notice: first.message })
    expect(s.puts).toHaveLength(1)
  })

  // 10 — a control. `doc` supplies the body's `work` and nothing else; the
  // notes sent are the argument, never `doc.notes`. The easiest thing in the
  // port to get wrong by tidying two parameters into one.
  it('takes the work from the document and the notes from the argument', async () => {
    const s = server([note('a')], 'in-the-body')
    const mine = s.etag
    s.elsewhere([note('a'), note('c')])

    await saveNotes(
      'in-the-url',
      mine,
      { work: 'in-the-body', notes: [note('never-sent')] },
      [note('a'), note('b')],
      s.ports,
    )

    expect(s.puts[0]).toEqual({
      work: 'in-the-url',
      etag: mine,
      doc: { work: 'in-the-body', notes: [note('a'), note('b')] },
    })
    expect(s.puts[1].doc).toEqual({
      work: 'in-the-body',
      notes: [note('a'), note('c'), note('b')],
    })
  })

  // Beyond §4.1a's ten, and added because the branch was otherwise the one
  // uncovered thing in this module. `api.ts` only ever throws an `Error`, so
  // this is defensive — but a port is a seam and the thing on the other side
  // of one is not always well behaved. What matters is that the reader is
  // shown something they can read rather than `[object Object]`.
  it('shows a refusal that was not thrown as an Error', async () => {
    const s = scripted({ put: () => Promise.reject('the tab went away') })

    const out = await saveNotes(
      WORK,
      'v1',
      { work: WORK, notes: [] },
      [],
      s.ports,
    )

    expect(out).toEqual({ kind: 'refused', notice: 'the tab went away' })
  })
})
