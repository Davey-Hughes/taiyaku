/**
 * What `main.tsx` actually renders — the one test in this suite that mounts the
 * app rather than a piece of it.
 *
 * **`Gate.test.tsx` cannot cover this and neither can `Routes.test.tsx`.** Each
 * mounts what it is testing directly, so both would go on passing with the gate
 * removed from `main.tsx` entirely — which is the exact regression that shipped:
 * `main.tsx` swapped `<App/>` for `<AppRoutes/>`, `/` became the library, and
 * every route lost the sign-in path that had lived inside `App`. A signed-out
 * visitor got "not signed in" in an alert banner and stood there. The
 * composition is the defect surface, so the composition is what this file
 * executes: `await import('./main')` against a stubbed `fetch`, with only the
 * reader replaced.
 *
 * `./App` is mocked for the reason `Routes.test.tsx` mocks it: `App`
 * value-imports `resolver.ts`, which value-imports `taiyaku-wasm` from a
 * gitignored directory, so a test that let it load would pass here and break CI
 * and every fresh clone. Nothing else is mocked — `Routes.tsx`, `Gate.tsx`,
 * `Library.tsx`, `library.ts` and `api.ts` all run for real, which is what makes
 * the 401 below the same 401 a browser gets.
 */
import { act } from 'react'
import { afterEach, expect, it, vi } from 'vitest'

vi.mock('./App', () => ({ default: () => <div data-testid="reader" /> }))

afterEach(() => {
  vi.unstubAllGlobals()
  document.body.innerHTML = ''
})

it('puts the sign-in screen in front of a signed-out visitor at /', async () => {
  // Every call 401s, including the three `Library` would make. `Response` is
  // real here, so `api.ts`'s `request` sees a genuine `res.status === 401` and
  // raises its own `Unauthenticated` through the real `whenSignedOut` hook.
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue(new Response('', { status: 401 })),
  )
  const root = document.createElement('div')
  root.id = 'root'
  document.body.append(root)

  // `main.tsx` renders on import; there is nothing to call. `act` is what
  // flushes the effects it starts, and the awaited macrotask is what lets
  // `me()` answer — the same shape `Gate.test.tsx`'s `settle` uses.
  await act(async () => {
    await import('./main')
    await new Promise((resolve) => setTimeout(resolve, 0))
  })

  expect(root.querySelector('form'), 'the sign-in form').not.toBeNull()
  expect(
    root.querySelector('input[autocomplete="current-password"]'),
  ).not.toBeNull()
  // The three negatives, and they are the regression rather than decoration:
  // the library must not be mounted, its failure banner must not be what a
  // signed-out visitor is left looking at, and the reader must not be either.
  expect(root.querySelector('.library')).toBeNull()
  expect(root.querySelector('[role="alert"]')).toBeNull()
  expect(root.querySelector('[data-testid="reader"]')).toBeNull()
})
