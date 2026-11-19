import { describe, expect, it } from 'vitest'
import { beginsWord, headAt, wordAt } from '../src/wordAt'

describe('wordAt', () => {
  it('finds the run of letters a character is in', () => {
    expect(wordAt('¿Hablaron mucho?', 3)).toEqual([1, 9])
  })

  it('keeps an apostrophe or hyphen between letters in the word', () => {
    expect(wordAt("l'homme est", 0)).toEqual([0, 7])
    expect(wordAt('l’homme est', 5)).toEqual([0, 7])
    expect(wordAt('a well-read man', 3)).toEqual([2, 11])
  })

  it('leaves a closing quotation mark out of the word before it', () => {
    expect(wordAt('‘Yes’ she said', 1)).toEqual([1, 4])
  })

  it('leaves an opening quotation mark out of the word after it', () => {
    expect(wordAt("he said 'yes", 9)).toEqual([9, 12])
  })

  it('finds no word at whitespace or punctuation', () => {
    expect(wordAt('a  b', 1)).toBeUndefined()
    expect(wordAt('¿Hablaron mucho?', 0)).toBeUndefined()
    expect(wordAt("l'homme", 1)).toBeUndefined()
    expect(wordAt('ab', 2)).toBeUndefined()
  })

  it('keeps a combining accent in the word', () => {
    expect(wordAt('говори́т', 2)).toEqual([0, 'говори́т'.length])
  })

  it('takes a letter past the basic plane whole, from either half', () => {
    expect(wordAt('a 𠮷野 b', 2)).toEqual([2, 5])
    expect(wordAt('a 𠮷野 b', 3)).toEqual([2, 5])
  })
})

describe('beginsWord', () => {
  it.each([
    ['a kanji', '猫', 0],
    ['a kanji past the basic plane', '\u{20BB7}野', 0],
    ['the kana length mark', 'ー', 0],
    ['the iteration mark', '々', 0],
    ['a digit', '1', 0],
    ['a number that is no digit', '〇', 0],
    ['a diacritic', '\u0301', 0],
    ['the kana voicing mark', '\u3099', 0],
    ['the kana semi-voicing mark', '\u309A', 0],
    ['a variation selector', '\uFE0F', 0],
    ['an ideographic variation selector', '\u{E0100}', 0],
    ['a mark with the Alphabetic property', '\u0E31', 0],
  ])('takes %s', (_name, text, at) => {
    expect(beginsWord(text, at)).toBe(true)
  })

  it.each([
    ['a zero-width space', '\u200B猫', 0],
    ['a zero-width joiner', '\u200D猫', 0],
    ['a star', '★猫', 0],
    ['an ideographic space', '吾輩は　猫', 3],
    ['a space', 'a b', 1],
    ['a full stop', '猫。', 1],
    ['an enclosing mark', '\u20DD', 0],
    ['a tone mark no word goes on through', '\u302A', 0],
    ['a Cyrillic titlo', '\u0483', 0],
    ['nothing, past the end', '猫', 1],
  ])('refuses %s', (_name, text, at) => {
    expect(beginsWord(text, at)).toBe(false)
  })
})

describe('headAt', () => {
  it('takes characters from the offset, never cutting a pair in two', () => {
    expect(headAt('𠮷あいう', 0, 2)).toBe('𠮷あ')
  })

  it('takes 16 characters by default, or what is left', () => {
    expect(headAt('あ'.repeat(20), 2)).toBe('あ'.repeat(16))
    expect(headAt('あいう', 1)).toBe('いう')
  })
})
