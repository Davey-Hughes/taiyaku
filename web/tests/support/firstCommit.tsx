import { useLayoutEffect } from 'react'

/**
 * Records `read()` in the commit that mounts it: drawn beside a view by the
 * render that mounts the view, it records what the view's first render
 * drew. A layout effect runs once its commit's DOM is in place and before
 * any of that commit's passive effects. A `rerender` inside `act` flushes
 * the passive effects, and the renders they cause, before it returns, so an
 * assertion after it cannot tell a body drawn in the first render from one
 * drawn an effect later; what this records can.
 */
export function FirstCommit({
  read,
  into,
}: {
  read: () => string
  into: { drawn?: string }
}) {
  useLayoutEffect(() => {
    into.drawn ??= read()
  })
  return null
}
