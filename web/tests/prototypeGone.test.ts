import { readdirSync } from 'node:fs'
import { basename } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import indexHtml from '../index.html?raw'

const PUBLIC = fileURLToPath(new URL('../public', import.meta.url))

const SOURCES = import.meta.glob<string>('../src/**/*.{ts,tsx}', {
  query: '?raw',
  import: 'default',
  eager: true,
})

describe('the prototype is gone', () => {
  it('keeps no prototype file in web/public', () => {
    const names = readdirSync(PUBLIC, { recursive: true, encoding: 'utf8' })
    expect(names).toContain('theme.js')
    expect(names.filter((name) => basename(name).startsWith('proto'))).toEqual(
      [],
    )
  })

  it('sends no /proto- request from web/src', () => {
    expect(Object.keys(SOURCES)).toContain('../src/phoneBar.ts')
    expect(Object.keys(SOURCES)).toContain('../src/Shell.tsx')
    const sending = Object.entries(SOURCES)
      .filter(([, text]) => text.includes('/proto-'))
      .map(([name]) => name)
    expect(sending).toEqual([])
  })

  it('loads no prototype script from index.html', () => {
    const sources = [
      ...indexHtml.matchAll(/<script\b[^>]*\ssrc="([^"]*)"/gi),
    ].map((match) => match[1])
    expect(sources).toContain('/theme.js')
    expect(sources.filter((src) => /(?:^|\/)proto/i.test(src))).toEqual([])
  })
})
