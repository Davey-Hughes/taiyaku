import { describe, expect, it } from 'vitest'
import { beginsWord, headAt, isKorean, wordAt } from '../src/wordAt'

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

  it('stops at a digit unless told to take digits in', () => {
    expect(wordAt('10년', 0)).toBeUndefined()
    expect(wordAt('10년', 2)).toEqual([2, 3])
    expect(wordAt('en 10años', 3)).toBeUndefined()
    expect(wordAt('en 10años', 5)).toEqual([5, 9])
    expect(wordAt('10년', 2, { digits: false })).toEqual([2, 3])
  })
})

describe('wordAt taking digits in', () => {
  const digits = { digits: true }

  it.each([
    ['the first digit of 10년', '그는 10년 동안', 3, [3, 6]],
    ['the second digit of 10년', '그는 10년 동안', 4, [3, 6]],
    ['the letter of 10년', '그는 10년 동안', 5, [3, 6]],
    ['the digit of 4번지에', '4번지에 살았다', 0, [0, 4]],
    ['the last letter of 4번지에', '4번지에 살았다', 3, [0, 4]],
    ['the digit of 11월이', '11월이 왔다', 1, [0, 4]],
    ['the particle of 11월이', '11월이 왔다', 3, [0, 4]],
    ['digits after the letters', '해리2 왔다', 2, [0, 3]],
    ['digits between letters', '가1나 왔다', 1, [0, 3]],
    ['full-width digits', '１０년', 0, [0, 3]],
    ['Arabic-Indic digits', '٣번', 0, [0, 2]],
    ['half of a digit past the basic plane', '\u{1D7CF}번', 1, [0, 3]],
  ])('takes in %s', (_name, text, at, run) => {
    expect(wordAt(text, at, digits)).toEqual(run)
  })

  it.each([
    ['a bare number', '서기 2000 년', 4],
    ['a bare number at the end of the text', '서기 2000', 6],
    ['a single digit', '7 번', 0],
    ['digits with a keycap’s marks on', '1\u{FE0F}\u{20E3}', 0],
  ])('finds no word in %s', (_name, text, at) => {
    expect(wordAt(text, at, digits)).toBeUndefined()
  })

  it('leaves a letter a space away from a number on its own', () => {
    expect(wordAt('2000 년', 5, digits)).toEqual([5, 6])
  })

  it('counts only decimal digits', () => {
    expect(wordAt('②번', 1, digits)).toEqual([1, 2])
    expect(wordAt('²번', 1, digits)).toEqual([1, 2])
    expect(wordAt('②번', 0, digits)).toBeUndefined()
  })

  it('still joins two letters over an apostrophe or hyphen, and no digit', () => {
    expect(wordAt('10년-해리', 0, digits)).toEqual([0, 6])
    expect(wordAt('해리-10년', 5, digits)).toEqual([3, 6])
    expect(wordAt('10-년', 3, digits)).toEqual([3, 4])
    expect(wordAt('10-년', 0, digits)).toBeUndefined()
  })
})

describe('isKorean', () => {
  it.each(['ko', 'KO', 'Ko', 'ko-KR', 'KO-kr', 'ko_KR', 'ko-Kore-KR'])(
    'takes %s',
    (tag) => {
      expect(isKorean(tag)).toBe(true)
    },
  )

  it.each([
    'kok',
    'kor',
    'k',
    'ja',
    'en-ko',
    'x-ko',
    '',
    '-ko',
    null,
    undefined,
  ])('refuses %s', (tag) => {
    expect(isKorean(tag)).toBe(false)
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
