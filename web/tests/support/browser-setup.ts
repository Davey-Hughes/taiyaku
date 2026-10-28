/**
 * Tells React it is running inside a test that uses `act`, and empties the
 * read store (`reads.ts`) after each test.
 *
 * **The flag changes what React reports, not what `act` does.** Without it,
 * `act` still flushes the updates and effects it wraps before it returns —
 * measured in the `jsdom` project on React 19.2.8, where the flag is read only
 * by the code choosing which warning to print. Unset, React logs "The current
 * testing environment is not configured to support act(...)" for an update
 * scheduled inside an `act`, and nothing for one scheduled outside it.
 * Set, the first goes quiet and the second is reported: "An update to X inside
 * a test was not wrapped in act(...)", which says a state change landed where
 * no `act` was waiting for it.
 *
 * Set for both projects that render, `browser` and `jsdom`; the `node` project
 * renders nothing. Until O75 it was set for `browser` alone, while this doc
 * accounted for two projects of three and claimed that without it `act` does
 * not flush effects — which was never measured, and is not so.
 *
 * A warning fails nothing, so it helps only if the output is read. Vitest 4.1
 * prints a passing test's console output under its `default` reporter, and
 * picks its `agent` reporter instead, which hides it, when it detects an agent
 * running it — pass `--reporter=default` to see these.
 */
import { afterEach } from 'vitest'
import { emptyReads } from '../../src/reads'

declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean
}

globalThis.IS_REACT_ACT_ENVIRONMENT = true

// The read store is module-level, so it outlives a test's unmount; emptied
// after every test, so one case's answers never reach the next.
afterEach(() => {
  emptyReads()
})
