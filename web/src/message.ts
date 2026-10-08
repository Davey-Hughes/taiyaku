/**
 * The string to show a person when something thrown has to be read.
 *
 * **One module rather than a line in each screen**, on `bytes.ts`'s argument —
 * a second formatter is one that drifts — and here there were forty. Four
 * files had already grown a private copy of it (`Labeller.tsx`,
 * `RuleEditor.tsx`, `notesWrite.ts`, `truthWrite.ts`), three of them under
 * this name and one under `message`, which is the drift arriving.
 *
 * It is also 30 of the 203 uncovered branches the web coverage gate was
 * counting on 2026-09-12: the `String(err)` arm is reached only by a throw
 * that is not an `Error`, and almost nothing throws one of those, so at forty
 * sites the idiom was a standing subtraction from the aggregate that no test
 * anybody would write could pay back. Here it is one branch, tested in
 * `message.test.ts`. See `docs/ci.md`, "The coverage gates".
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
 * The body of a thrown `path: status body`, trimmed, or `undefined` for a
 * failure that is not a response. The one place the two readers below find
 * it, so that they cannot disagree about what a body is.
 */
function bodyOf(message: string): string | undefined {
  const found = /^\S+: \d{3} ([\s\S]*)$/.exec(message)
  return found === null ? undefined : found[1].trim()
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
  const body = bodyOf(message)
  if (body === undefined || body === '') return message
  if (!body.startsWith('{')) return isPlainSentence(body) ? body : message
  const error = objectOf(body)?.error
  return typeof error === 'string' && error.trim() !== '' ? error : message
}

/**
 * The volumes a refused move lists beside its sentence, or none. The
 * sentence names the first few; this is all of them. Read from the same body
 * as `sentenceOf`'s, and answering none for a failure that is not a
 * response, a body that is not an object, or a `volumes` that is not a list
 * of strings.
 */
export function volumesOf(err: unknown): string[] {
  const volumes = objectOf(bodyOf(messageOf(err)))?.volumes
  return Array.isArray(volumes) && volumes.every((v) => typeof v === 'string')
    ? volumes
    : []
}
