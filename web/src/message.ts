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
