import { describe, expect, it } from 'vitest'
import { messageOf, sentenceOf, volumesOf } from './message'

describe('messageOf', () => {
  it('reads an Error by its message rather than its toString', () => {
    // `String(new Error('nope'))` is `'Error: nope'`. The prefix is the thing
    // this arm exists to drop: the screens that print this put it in a
    // failure notice a person reads, not in a log.
    expect(messageOf(new Error('nope'))).toBe('nope')
  })

  it('reads a subclass the same way', () => {
    expect(messageOf(new TypeError('wrong shape'))).toBe('wrong shape')
  })

  it('stringifies anything else, which is the arm forty call sites never reached', () => {
    // Nothing in this codebase throws a non-Error deliberately; the arm is
    // there because `unknown` is what a `catch` binds and a thrown string is
    // what a dependency can hand back. Pinned once here rather than being
    // thirty uncovered branches spread over twenty-one files.
    expect(messageOf('a bare string')).toBe('a bare string')
    expect(messageOf(undefined)).toBe('undefined')
    expect(messageOf({ code: 500 })).toBe('[object Object]')
  })
})

describe('sentenceOf', () => {
  it("answers the refusal's own sentence where the body carries one", () => {
    const thrown = new Error(
      '/api/library/libraries/3: 409 {"error":"`Manga` is the only library `sam` is granted."}',
    )
    expect(sentenceOf(thrown)).toBe(
      '`Manga` is the only library `sam` is granted.',
    )
  })

  it('answers the sentence from a body that spans lines', () => {
    const thrown = new Error(
      '/api/library/libraries/3: 409 {\n  "error": "taken"\n}',
    )
    expect(sentenceOf(thrown)).toBe('taken')
  })

  it('keeps the whole message where there is no sentence to take', () => {
    for (const message of [
      '/api/library/libraries: 500 {"detail":"no error key"}',
      '/api/library/libraries: 500 {"error":7}',
      '/api/library/libraries: 502 {not json}',
      'Failed to fetch',
    ]) {
      expect(sentenceOf(new Error(message))).toBe(message)
    }
  })

  it('answers a plain-text body as that text, which is how some routes refuse', () => {
    expect(
      sentenceOf(
        new Error(
          "/api/library/libraries: 403 this route is an administrator's",
        ),
      ),
    ).toBe("this route is an administrator's")
    expect(
      sentenceOf(
        new Error('/api/library/roots/3: 400 the path does not exist\n'),
      ),
    ).toBe('the path does not exist')
  })

  it('keeps the path and status where the body is empty or only space', () => {
    for (const message of [
      '/api/library/libraries: 403 ',
      '/api/library/libraries: 403  \n',
    ]) {
      expect(sentenceOf(new Error(message))).toBe(message)
    }
  })

  it('keeps the whole message for a JSON body that has no sentence in it', () => {
    const message = '/api/library/libraries: 403 {"detail":"nope"}'
    expect(sentenceOf(new Error(message))).toBe(message)
  })

  it('keeps the whole message for a body that is not one short plain sentence', () => {
    const bodies: Record<string, string> = {
      'a page of markup': '<html><body>502 Bad Gateway</body></html>',
      'a page over several lines': 'Bad gateway\r\nnginx/1.27.2',
      'a JSON array': '["a","b"]',
      'a JSON string': '"a json string"',
      'a bare number': '0',
      'a status line': '502 Bad Gateway',
      'a long line': 'panicked at crates/x.rs:1 '.repeat(20).trim(),
    }
    for (const [what, body] of Object.entries(bodies)) {
      const message = `/api/library/libraries: 500 ${body}`
      expect(sentenceOf(new Error(message)), what).toBe(message)
    }
  })

  it('answers a text of exactly the bound as the sentence, and keeps the message for one character more', () => {
    const at = 'a'.repeat(200)
    expect(sentenceOf(new Error(`/api/library/libraries: 400 ${at}`))).toBe(at)
    const over = `${at}a`
    const message = `/api/library/libraries: 400 ${over}`
    expect(sentenceOf(new Error(message))).toBe(message)
  })

  it('keeps the whole message for a text that begins with a number', () => {
    const message = '/api/library/libraries: 500 3 folders could not be read'
    expect(sentenceOf(new Error(message))).toBe(message)
  })

  it('answers a short one-line text as the sentence', () => {
    expect(sentenceOf(new Error('/api/library/libraries: 404 Not Found'))).toBe(
      'Not Found',
    )
    expect(
      sentenceOf(
        new Error('/api/library/libraries: 400 library 7 is not there'),
      ),
    ).toBe('library 7 is not there')
  })

  it('keeps the whole message for an error key that holds no sentence', () => {
    for (const body of ['{"error": ""}', '{"error": "  "}']) {
      const message = `/api/library/libraries: 400 ${body}`
      expect(sentenceOf(new Error(message))).toBe(message)
    }
  })

  it('reads a thrown value that is not an Error as messageOf does', () => {
    expect(sentenceOf('plain')).toBe('plain')
  })
})

describe('volumesOf', () => {
  it('answers the volumes a refusal lists', () => {
    const thrown = new Error(
      '/api/library/roots/3/move: 409 {"error":"cannot be moved","volumes":["A v01 (raw)","B v02 (raw)"]}',
    )
    expect(volumesOf(thrown)).toEqual(['A v01 (raw)', 'B v02 (raw)'])
  })

  it('reads a body with whitespace around it, as sentenceOf does', () => {
    const thrown = new Error(
      '/api/library/roots/3/move: 409 \n {"error":"x","volumes":["A v01 (raw)"]}\n',
    )
    expect(sentenceOf(thrown)).toBe('x')
    expect(volumesOf(thrown)).toEqual(['A v01 (raw)'])
  })

  it('answers none where there is no list, or it is not one of strings', () => {
    for (const message of [
      '/api/library/roots/3/move: 409 {"error":"a scan is running","volumes":[]}',
      '/api/library/roots/3/move: 409 {"error":"x"}',
      '/api/library/roots/3/move: 409 {"error":"x","volumes":[1,2]}',
      '/api/library/roots/3/move: 500 plain text',
      'Failed to fetch',
    ]) {
      expect(volumesOf(new Error(message))).toEqual([])
    }
  })
})
