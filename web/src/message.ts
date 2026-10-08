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
  const found = /^\S+: \d{3} ([\s\S]*)$/.exec(message)
  if (found === null) return message
  const body = found[1].trim()
  if (body === '') return message
  if (!body.startsWith('{')) return isPlainSentence(body) ? body : message
  try {
    const parsed: unknown = JSON.parse(body)
    if (
      typeof parsed === 'object' &&
      parsed !== null &&
      'error' in parsed &&
      typeof parsed.error === 'string' &&
      parsed.error.trim() !== ''
    ) {
      return parsed.error
    }
  } catch {
    // Not JSON after all, so the whole message stands.
  }
  return message
}
