/**
 * Tells React it is running inside a test that uses `act`.
 *
 * Without it React logs "The current testing environment is not configured to
 * support act(...)" and, more importantly, `act` does not actually flush
 * effects synchronously — so a test can pass for the wrong reason, observing a
 * state React had not committed yet. Set for the `browser` project only; the
 * `node` project renders nothing.
 */
declare global {
  // eslint-disable-next-line no-var
  var IS_REACT_ACT_ENVIRONMENT: boolean
}

globalThis.IS_REACT_ACT_ENVIRONMENT = true

export {}
