import { readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, it } from 'vitest'

// The vitest projects look only in `tests/`, so a test file under `src/`
// would never run and nothing else would say so.
const SRC = fileURLToPath(new URL('../src', import.meta.url))

it('keeps every test file out of src', () => {
  const strays = readdirSync(SRC, { recursive: true, encoding: 'utf8' })
    .filter((name) => /\.test\.tsx?$/.test(name))
    // Only files count: vitest saves a failed browser test's screenshots in
    // `__screenshots__/<test file name>/` beside the test file, and an
    // ignored one of those left under `src/` is a directory, not a test.
    .filter((name) => statSync(join(SRC, name)).isFile())
    .sort()
  expect(strays).toEqual([])
})
