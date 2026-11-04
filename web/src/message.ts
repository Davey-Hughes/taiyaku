/**
 * The string to show a person when something thrown has to be read.
 *
 * **One module rather than a line in each screen**, on `bytes.ts`'s argument —
 * a second formatter is one that drifts. It is also one branch for coverage:
 * the `String(err)` arm is reached only by a throw that is not an `Error`, and
 * almost nothing throws one of those, so a copy at each site would be a
 * standing subtraction from the aggregate that no test anybody would write
 * could pay back. Here it is one branch, tested in `message.test.ts`. See
 * `docs/ci.md`, "The coverage gates".
 */
export const messageOf = (err: unknown): string =>
  err instanceof Error ? err.message : String(err)

/** The most characters a plain-text body may have and be drawn as a sentence. */
const MAX_PLAIN_SENTENCE = 200

/**
 * Whether a plain-text body is drawn as a sentence: one line of at most the
 * bound, not starting with `<`, `[` or `"`, and not starting with a token of
 * digits alone. A body that fails any of these keeps the whole message, so
 * that a status or a count is read beside the path it came from.
 */
function isPlainSentence(body: string): boolean {
  return (
    body.length <= MAX_PLAIN_SENTENCE &&
    !/[\r\n]/.test(body) &&
    !/^[<["]/.test(body) &&
    !/^\d+(\s|$)/.test(body)
  )
}

/**
 * The status and the body, trimmed, of a thrown `path: status body`, or
 * `undefined` for a failure that is not a response. The one place the
 * readers below find them, so that they cannot disagree about what a body
 * is.
 */
function answerOf(
  message: string,
): { status: number; body: string } | undefined {
  const found = /^\S+: (\d{3}) ([\s\S]*)$/.exec(message)
  return found === null
    ? undefined
    : { status: Number(found[1]), body: found[2].trim() }
}

/** A body that is a JSON object, parsed, or `undefined`. */
function objectOf(
  body: string | undefined,
): Record<string, unknown> | undefined {
  if (body === undefined || !body.startsWith('{')) return undefined
  try {
    const parsed: unknown = JSON.parse(body)
    return typeof parsed === 'object' && parsed !== null
      ? (parsed as Record<string, unknown>)
      : undefined
  } catch {
    return undefined
  }
}

/**
 * A refusal's own sentence, where the server sent one.
 *
 * `api.ts` throws `path: status body`. The library and user administration
 * routes answer a refusal as `{"error": "<sentence>"}`; others, such as the
 * rules routes and the refusal a reader gets from an administrator's route,
 * answer plain text. This answers the sentence for either: the `error` string
 * of a JSON object where it is not empty, and a plain-text body that
 * `isPlainSentence` accepts. It answers `messageOf`'s whole string for
 * everything else: a body that is empty, a JSON object with no non-empty
 * `error` string, text that is not JSON after starting with `{`, a plain-text
 * body `isPlainSentence` refuses, or a failure that is not a response at all.
 *
 * A caller that needs the status reads it from `messageOf`, with
 * `config.ts`'s `statusOf`.
 */
export function sentenceOf(err: unknown): string {
  const message = messageOf(err)
  return ownSentenceOf(answerOf(message)?.body) ?? message
}

/** The sentence a body carries, as `sentenceOf` reads it, or `undefined`. */
function ownSentenceOf(body: string | undefined): string | undefined {
  if (body === undefined || body === '') return undefined
  if (!body.startsWith('{')) return isPlainSentence(body) ? body : undefined
  const error = objectOf(body)?.error
  return typeof error === 'string' && error.trim() !== '' ? error : undefined
}

/**
 * The status of a response whose body carries no sentence, or `undefined`
 * for one that carries one and for a failure that is not a response. Such
 * a body is a proxy's page of markup, an empty body, or anything else
 * `sentenceOf` answers the whole message for.
 */
function statusWithoutSentence(err: unknown): number | undefined {
  const answer = answerOf(messageOf(err))
  return answer === undefined || ownSentenceOf(answer.body) !== undefined
    ? undefined
    : answer.status
}

/**
 * Why a write failed, for a screen that says it after what was not done:
 * the server's own sentence, as `sentenceOf` reads it; for a response that
 * carries none, its status alone, since the body is markup or nothing and
 * not words to read; and a failure that is not a response as it was
 * thrown.
 */
export function reasonOf(err: unknown): string {
  const status = statusWithoutSentence(err)
  return status === undefined
    ? sentenceOf(err)
    : `the server answered ${status}`
}

/**
 * A refused write, for a screen that says it on its own: the server's own
 * sentence, which says what is wrong; for a response that carries none,
 * `notDone` and then `reasonOf`'s status, so that the person reads what
 * failed; and a failure that is not a response as it was thrown.
 */
export function refusalOf(err: unknown, notDone: string): string {
  return statusWithoutSentence(err) === undefined
    ? reasonOf(err)
    : `${notDone}: ${reasonOf(err)}`
}
