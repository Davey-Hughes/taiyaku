import { describe, expect, it } from 'vitest'
import {
  PHONE_LAYOUT,
  PHONE_LAYOUT_PARTS,
  SIDEBAR_LAYOUT,
} from '../../src/phoneLayout'
import { withoutComments } from '../support/css'

/**
 * O305: the phone layout turns on one condition, and every stylesheet that
 * asks the viewport's size asks it in those words, or in its complement's,
 * so that no rule turns at a size the scripts (`phoneLayout.ts`) do not
 * know. A stylesheet added under `styles/` is read without being named.
 */
const SHEETS = import.meta.glob<string>(['./index.css', './styles/*.css'], {
  base: '../../src',
  query: '?raw',
  import: 'default',
  eager: true,
})

/** The two queries that turn at a size of their own, each for its reason. */
const EXCEPTIONS = [
  // A Configuration table's row as two lines, at a width its columns need.
  './styles/admin.css: @media (width < 36rem)',
  // A book's rows as one column, at a width its columns need.
  './styles/book.css: @media (max-width: 40rem)',
]

/** Each `@media` that asks the viewport's width or height, by its file. */
const SIZE_QUERIES = Object.entries(SHEETS).flatMap(([file, css]) =>
  [...withoutComments(css).matchAll(/@media\s+([^{;]+)\{/g)]
    .map((m) => (m[1] ?? '').replace(/\s+/g, ' ').trim())
    .filter((prelude) => /\b(?:width|height)\b/.test(prelude))
    .map((prelude) => `${file}: @media ${prelude}`),
)

const asks = (query: string, condition: string) =>
  query.endsWith(`: @media ${condition}`)

describe('the stylesheets turn at the phone layout’s one condition', () => {
  it('reads index.css and every stylesheet under styles/', () => {
    const files = Object.keys(SHEETS)
    expect(files).toContain('./index.css')
    expect(files).toContain('./styles/shell.css')
    expect(files).toContain('./styles/library.css')
    for (const css of Object.values(SHEETS)) expect(css).not.toBe('')
  })

  it('writes every viewport-size query as the phone layout, its complement, or a named exception', () => {
    expect(
      SIZE_QUERIES.filter(
        (query) =>
          !asks(query, PHONE_LAYOUT) &&
          !asks(query, SIDEBAR_LAYOUT) &&
          !EXCEPTIONS.includes(query),
      ),
    ).toEqual([])
  })

  it('writes the phone layout and its complement', () => {
    expect(
      [PHONE_LAYOUT, SIDEBAR_LAYOUT].filter(
        (condition) => !SIZE_QUERIES.some((query) => asks(query, condition)),
      ),
    ).toEqual([])
  })

  it('finds each named exception once', () => {
    expect(
      SIZE_QUERIES.filter((query) => EXCEPTIONS.includes(query)).sort(),
    ).toEqual([...EXCEPTIONS].sort())
  })
})

/**
 * The sidebar's condition is written out rather than derived, as a
 * stylesheet writes it. Were one bound moved in the parts and not in it,
 * both layouts' rules, or neither's, would hold between the two, and every
 * case above would still pass.
 */
describe('the sidebar’s condition', () => {
  it('is each part of the phone layout’s, negated, all of them at once', () => {
    const negated = PHONE_LAYOUT_PARTS.map((part) =>
      part.replace(' < ', ' >= '),
    )
    // The fixture tells: each part is a bound the negation turned.
    expect(negated.filter((part, i) => part === PHONE_LAYOUT_PARTS[i])).toEqual(
      [],
    )
    expect(SIDEBAR_LAYOUT).toBe(negated.join(' and '))
  })
})
