import { describe, expect, it } from 'vitest'
import { messageOf, reasonOf, refusalOf, sentenceOf } from '../src/message'

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
    // what a dependency can hand back.
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

  it('answers the sentence from a body with space before and after it', () => {
    const thrown = new Error(
      '/api/library/roots/3/move: 409 \n {"error":"x","volumes":[]}\n',
    )
    expect(sentenceOf(thrown)).toBe('x')
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
    const message =
      '/api/library/libraries: 500 3 directories could not be read'
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

/** A proxy's own page for an upstream that did not answer. */
const PROXY_PAGE =
  '/api/keys: 502 <html>\r\n<head><title>502 Bad Gateway</title></head>\r\n</html>'

describe('reasonOf', () => {
  it('answers the status alone for a response whose body is a page of markup, empty, or no sentence', () => {
    for (const [message, said] of [
      [PROXY_PAGE, 'the server answered 502'],
      ['/api/keys: 500 ', 'the server answered 500'],
      ['/api/keys/1: 404 \n ', 'the server answered 404'],
      ['/api/password: 400 {"error": ""}', 'the server answered 400'],
      ['/api/session: 503 [1, 2]', 'the server answered 503'],
    ] as const) {
      expect(reasonOf(new Error(message))).toBe(said)
    }
  })

  it('answers the sentence a body carries, and a failure that is not a response as it was thrown', () => {
    for (const [message, said] of [
      ['/api/keys/1: 404 no such key', 'no such key'],
      ['/api/session: 403 {"error":"wrong origin"}', 'wrong origin'],
      [
        'That current password did not match.',
        'That current password did not match.',
      ],
      ['Failed to fetch', 'Failed to fetch'],
    ] as const) {
      expect(reasonOf(new Error(message))).toBe(said)
    }
  })
})

describe('refusalOf', () => {
  it('says what was not done before the status, for a response whose body carries no sentence', () => {
    expect(refusalOf(new Error(PROXY_PAGE), 'The key was not made')).toBe(
      'The key was not made: the server answered 502',
    )
  })

  it('answers the sentence alone where the body carries one, and a failure that is not a response as it was thrown', () => {
    for (const [message, said] of [
      [
        '/api/keys: 400 a key needs a name, so the right one can be revoked later',
        'a key needs a name, so the right one can be revoked later',
      ],
      [
        'That current password did not match.',
        'That current password did not match.',
      ],
    ] as const) {
      expect(refusalOf(new Error(message), 'The key was not made')).toBe(said)
    }
  })
})
