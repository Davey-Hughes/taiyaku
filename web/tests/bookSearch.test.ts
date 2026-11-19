import { describe, expect, it, vi } from 'vitest'
import type { Block, Book, Edition } from '../src/book'
import {
  context,
  fold,
  isJapanese,
  ready,
  searchBook,
  stepFrom,
} from '../src/bookSearch'

const block = (text: string): Block => ({ text, heading: false, sentences: [] })
const edition = (texts: string[], language: string | null = 'en'): Edition => ({
  label: 'E',
  language,
  blocks: texts.map(block),
})
const bookOf = (editions: Record<string, Edition>): Book => ({
  title: 'T',
  hub: Object.keys(editions)[0],
  editions,
  alignments: [],
})

describe('fold', () => {
  it('composes a half-width kana with its voicing mark', () => {
    expect(fold('ｶﾞ').text).toBe('ガ')
    expect(fold('ｶﾞ').to).toEqual([0, 2])
  })
  it('keeps each folded character mapped to its original offset', () => {
    const f = fold('㈱')
    expect(f.text).toBe('(株)')
    expect(f.to).toEqual([0, 0, 0, 1])
  })
  it('lowers ASCII and leaves a base before a combining mark to compose', () => {
    expect(fold('AbC').text).toBe('abc')
    expect(fold('e\u0301x')).toEqual({ text: 'éx', to: [0, 2, 3] })
  })
})

describe('searchBook', () => {
  const find = (text: string, query: string) =>
    searchBook(bookOf({ en: edition([text]) }), query, ['en'])

  it('finds a full-width query in half-width text and back', () => {
    expect(find('ガイド', 'ｶﾞ')).toEqual([
      { edition: 'en', block: 0, start: 0, end: 1 },
    ])
    expect(find('ｶﾞｲﾄﾞ', 'ガ')).toEqual([
      { edition: 'en', block: 0, start: 0, end: 2 },
    ])
  })
  it('ignores case', () => {
    expect(find('Harry', 'harry')).toHaveLength(1)
  })
  it('folds case where lower case alone would not: ß and SS, ς and Σ, İ and I', () => {
    const at = (start: number, end: number) => [
      { edition: 'en', block: 0, start, end },
    ]
    expect(find('Straße', 'strasse')).toEqual(at(0, 6))
    expect(find('STRASSE', 'straße')).toEqual(at(0, 7))
    expect(find('STRAẞE', 'strasse')).toEqual(at(0, 6))
    expect(find('Straße', 'as')).toEqual(at(3, 5))
    expect(find('ΟΔΟΣ', 'οδος')).toEqual(at(0, 4))
    expect(find('οδος', 'ΟΔΟΣ')).toEqual(at(0, 4))
    expect(find('İstanbul', 'istanbul')).toEqual(at(0, 8))
    expect(find('İSTANBUL', 'istanbul')).toEqual(at(0, 8))
    expect(find('ISTANBUL', 'İstanbul')).toEqual(at(0, 8))
    expect(find('I\u{307}stanbul', 'istanbul')).toEqual(at(0, 9))
  })
  it('takes dotless ı as i, so a Turkish capital I finds it and dotted and dotless are one letter', () => {
    const at = (start: number, end: number) => [
      { edition: 'en', block: 0, start, end },
    ]
    expect(find('kırmızı', 'KIRMIZI')).toEqual(at(0, 7))
    expect(find('KIRMIZI', 'kırmızı')).toEqual(at(0, 7))
    expect(find('kirmizi', 'kırmızı')).toEqual(at(0, 7))
  })
  it('looks past a variation selector between two characters, lighting it with the one before', () => {
    const at = (start: number, end: number) => [
      { edition: 'en', block: 0, start, end },
    ]
    expect(find('葛\u{E0100}城', '葛城')).toEqual(at(0, 4))
    expect(find('葛\u{FE00}城', '葛城')).toEqual(at(0, 3))
    expect(find('葛\u{E0100}城', '城')).toEqual(at(3, 4))
    expect(find('葛城', '葛\u{E0100}城')).toEqual(at(0, 2))
  })
  it('finds overlapping occurrences', () => {
    expect(find('aaa', 'aa').map((m) => m.start)).toEqual([0, 1])
  })
  it('never spans two blocks', () => {
    const b = bookOf({ en: edition(['ab', 'cd']) })
    expect(searchBook(b, 'bc', ['en'])).toEqual([])
  })
  it('covers a whole original character when only part of its fold matches', () => {
    expect(find('a㈱b', '(')).toEqual([
      { edition: 'en', block: 0, start: 1, end: 2 },
    ])
  })
  it('orders by edition, then block, then start', () => {
    const b = bookOf({ en: edition(['x', 'xx']), ko: edition(['x']) })
    expect(
      searchBook(b, 'x', ['ko', 'en']).map((m) => [
        m.edition,
        m.block,
        m.start,
      ]),
    ).toEqual([
      ['ko', 0, 0],
      ['en', 0, 0],
      ['en', 1, 0],
      ['en', 1, 1],
    ])
  })
  it('counts an astral character as the two code units it is in the text', () => {
    expect(find('𝟏a', 'a')).toEqual([
      { edition: 'en', block: 0, start: 2, end: 3 },
    ])
    expect(find('𝟏', '1')).toEqual([
      { edition: 'en', block: 0, start: 0, end: 2 },
    ])
  })
  it('lists a match once where folded characters share an original one', () => {
    expect(find('……', '..').map((m) => [m.start, m.end])).toEqual([
      [0, 1],
      [0, 2],
      [1, 2],
    ])
  })
  it('finds nothing for an empty query', () => {
    expect(find('abc', '')).toEqual([])
  })

  it('folds a block once, however many searches read it', () => {
    const b = bookOf({ en: edition(['Ünïcödé façade', 'naïve café']) })
    const normalize = vi.spyOn(String.prototype, 'normalize')
    searchBook(b, 'ca', ['en'])
    const once = normalize.mock.calls.length
    searchBook(b, 'fa', ['en'])
    expect(once).toBeGreaterThan(0)
    expect(normalize.mock.calls.length).toBe(once)
    normalize.mockRestore()
  })

  it('searches a large book quickly', () => {
    const line = 'the quick brown fox jumps over the lazy dog, then thinks. '
    const text = line.repeat(4).slice(0, 220)
    const blocks = Array.from({ length: 3000 }, (_, i) => `${i} ${text}`)
    const b = bookOf({
      a: edition(blocks),
      b: edition(blocks),
      c: edition(blocks),
    })
    const all = ['a', 'b', 'c']
    const times: number[] = []
    for (let i = 0; i < 3; i++) {
      const t = performance.now()
      const found = searchBook(b, 'th', all)
      times.push(performance.now() - t)
      expect(found.length).toBeGreaterThan(0)
    }
    times.sort((x, y) => x - y)
    expect(times[1]).toBeLessThan(100)
  })

  it('searches a large book quickly the first time, before any block is folded', () => {
    const line = 'the quick brown fox jumps over the lazy dog, then thinks. '
    const text = line.repeat(4).slice(0, 220)
    const blocks = () => Array.from({ length: 3000 }, (_, i) => `${i} ${text}`)
    const b = bookOf({
      a: edition(blocks()),
      b: edition(blocks()),
      c: edition(blocks()),
    })
    const t = performance.now()
    const found = searchBook(b, 'th', ['a', 'b', 'c'])
    expect(performance.now() - t).toBeLessThan(300)
    expect(found.length).toBeGreaterThan(0)
  })
})

describe('ready', () => {
  it('wants two characters, or one of a CJK script', () => {
    expect(ready('a')).toBe(false)
    expect(ready('ab')).toBe(true)
    expect(ready('1')).toBe(false)
    expect(ready('12')).toBe(true)
    expect(ready('猫')).toBe(true)
    expect(ready('𠮷')).toBe(true)
    expect(ready('々')).toBe(true)
    expect(ready('ね')).toBe(true)
    expect(ready('ｶ')).toBe(true)
    expect(ready('한')).toBe(true)
    expect(ready('ㄱ')).toBe(true)
    expect(ready('ㄅ')).toBe(true)
    expect(ready('')).toBe(false)
  })
  it('counts no white space', () => {
    expect(ready('  ')).toBe(false)
    expect(ready('a ')).toBe(false)
    expect(ready(' a')).toBe(false)
    expect(ready('\u{3000}')).toBe(false)
    expect(ready('\u{3000}猫')).toBe(true)
    expect(ready('a b')).toBe(true)
  })
  it('takes no single mark of punctuation for a kana', () => {
    expect(ready('・')).toBe(false)
    expect(ready('「')).toBe(false)
    expect(ready('ー')).toBe(true)
    expect(ready('ヶ')).toBe(true)
  })
  it('counts a character outside the BMP as one, and a letter number as a letter', () => {
    expect(ready('😀')).toBe(false)
    expect(ready('😀😀')).toBe(true)
    expect(ready('〇')).toBe(true)
  })
  it('waits on one character as typed, whatever its folded form runs to', () => {
    expect(ready('ß')).toBe(false)
    expect(ready('ﬁ')).toBe(false)
    expect(ready('ßa')).toBe(true)
    expect(ready('e\u{301}')).toBe(false)
  })
})

describe('stepFrom', () => {
  const at = (block: number) => ({ edition: 'en', block, start: 0, end: 1 })
  const all = [at(0), at(1), at(2)]
  const m = (block: number, start: number) => ({
    edition: 'en',
    block,
    start,
    end: start + 1,
  })
  const listed = [m(0, 5), m(1, 0), m(1, 8), m(3, 2)]
  const here = (block: number, at: number) => ({
    edition: 'en',
    block,
    at,
    order: ['en'],
  })
  it('steps on from the match given, round the ends', () => {
    expect(stepFrom(all, at(1), 1)).toEqual(at(2))
    expect(stepFrom(all, at(2), 1)).toEqual(at(0))
    expect(stepFrom(all, at(0), -1)).toEqual(at(2))
  })
  it('starts from either end with no match given, or one not in the list and no place to start from', () => {
    expect(stepFrom(all, undefined, 1)).toEqual(at(0))
    expect(stepFrom(all, undefined, -1)).toEqual(at(2))
    expect(stepFrom(all, at(9), 1)).toEqual(at(0))
  })
  it('steps from the reader’s place where no match has been gone to', () => {
    expect(stepFrom(listed, undefined, 1, here(1, 4))).toEqual(m(1, 8))
    expect(stepFrom(listed, undefined, -1, here(1, 4))).toEqual(m(1, 0))
    // A match at the place is on, not back.
    expect(stepFrom(listed, undefined, 1, here(1, 0))).toEqual(m(1, 0))
    // Round the ends.
    expect(stepFrom(listed, undefined, 1, here(4, 0))).toEqual(m(0, 5))
    expect(stepFrom(listed, undefined, -1, here(0, 0))).toEqual(m(3, 2))
  })
  it('steps from the reader’s place where the match given is not in the list', () => {
    expect(stepFrom(listed, m(9, 0), 1, here(1, 4))).toEqual(m(1, 8))
    expect(stepFrom(listed, m(9, 0), -1, here(1, 4))).toEqual(m(1, 0))
    expect(stepFrom(listed, m(9, 0), 1, here(2, 0))).toEqual(m(3, 2))
    expect(stepFrom(listed, m(9, 0), -1, here(2, 0))).toEqual(m(1, 8))
    // A match at the place is on, not back: it is what the screen starts with.
    expect(stepFrom(listed, m(9, 0), 1, here(1, 0))).toEqual(m(1, 0))
    expect(stepFrom(listed, m(9, 0), -1, here(1, 0))).toEqual(m(0, 5))
    // Round the ends.
    expect(stepFrom(listed, m(9, 0), 1, here(4, 0))).toEqual(m(0, 5))
    expect(stepFrom(listed, m(9, 0), -1, here(0, 0))).toEqual(m(3, 2))
    // A match listed is stepped on from, wherever the reader is.
    expect(stepFrom(listed, m(1, 0), 1, here(3, 9))).toEqual(m(1, 8))
  })
  it('steps from the reader’s place across editions in the order they are listed', () => {
    const m = (edition: string, block: number) => ({
      edition,
      block,
      start: 0,
      end: 1,
    })
    const listed = [m('en', 1), m('en', 4), m('fr', 1), m('fr', 3)]
    const order = ['en', 'ja', 'fr']
    const here = (edition: string, block: number) => ({
      edition,
      block,
      at: 0,
      order,
    })
    const gone = m('en', 9)
    expect(stepFrom(listed, gone, 1, here('fr', 2))).toEqual(m('fr', 3))
    expect(stepFrom(listed, gone, -1, here('fr', 2))).toEqual(m('fr', 1))
    expect(stepFrom(listed, gone, 1, here('en', 5))).toEqual(m('fr', 1))
    expect(stepFrom(listed, gone, -1, here('fr', 0))).toEqual(m('en', 4))
    expect(stepFrom(listed, gone, 1, here('ja', 0))).toEqual(m('fr', 1))
    expect(stepFrom(listed, gone, -1, here('ja', 0))).toEqual(m('en', 4))
    expect(stepFrom(listed, gone, 1, here('fr', 4))).toEqual(m('en', 1))
  })
  it('steps past a match sharing its start but not its end', () => {
    const matches = searchBook(bookOf({ en: edition(['……']) }), '..', ['en'])
    const seen = [matches[0]]
    for (let i = 1; i < matches.length; i++) {
      seen.push(stepFrom(matches, seen.at(-1), 1)!)
    }
    expect(seen).toEqual(matches)
  })
  it('has nothing to step to in no matches', () => {
    expect(stepFrom([], undefined, 1)).toBeUndefined()
  })
})

describe('isJapanese', () => {
  it('reads the language tag by its first subtag, ja or jpn', () => {
    const tagged = (language: string) => isJapanese(edition([], language))
    expect(tagged('JA-jp')).toBe(true)
    expect(tagged('ja')).toBe(true)
    expect(tagged('jpn')).toBe(true)
    expect(tagged('JPN-JP')).toBe(true)
    expect(tagged('ja_JP')).toBe(true)
    expect(tagged('jav')).toBe(false)
    expect(tagged('jam')).toBe(false)
    expect(tagged('en')).toBe(false)
  })
  it('judges by the script where there is no tag, or und, or an empty one', () => {
    for (const language of [null, 'und', 'UND-Jpan', '']) {
      expect(isJapanese(edition(['これは猫です。', 'Hi'], language))).toBe(true)
      expect(isJapanese(edition(['It is a cat.', 'これ'], language))).toBe(
        false,
      )
      expect(isJapanese(edition([], language))).toBe(false)
    }
  })
  it('takes text with no kana for not Japanese, as Chinese is', () => {
    expect(isJapanese(edition(['我是一隻貓。', '名字還沒有。'], null))).toBe(
      false,
    )
    expect(isJapanese(edition(['吾輩は猫である。'], null))).toBe(true)
  })
})

describe('context', () => {
  const m = { edition: 'en', block: 0, start: 4, end: 6 }
  it('clips at the block ends', () => {
    expect(context('0123456789', m)).toEqual({
      before: '0123',
      match: '45',
      after: '6789',
    })
  })
  it('never cuts a surrogate pair at the clip', () => {
    const text = `${'𝟏'.repeat(3)}ab${'𝟏'.repeat(3)}`
    const at = { edition: 'en', block: 0, start: 6, end: 8 }
    const c = context(text, at, 3)
    expect(c).toEqual({ before: '𝟏', match: 'ab', after: '𝟏' })
  })
  it('clips to the distance asked', () => {
    expect(context('0123456789', m, 2)).toEqual({
      before: '23',
      match: '45',
      after: '67',
    })
  })
})
