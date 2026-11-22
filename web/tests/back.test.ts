import { describe, expect, it } from 'vitest'
import { destination } from '../src/back'

/**
 * What each address is called by a way back to it: the word its button
 * shows, and the phrase a failed load's sentence ends with. An address the
 * table does not name is "Back", with no phrase.
 */
describe('destination', () => {
  it.each([
    ['/?q=kaguya', 'Search', 'the search'],
    ['/?library=2&q=kaguya&sort=added', 'Search', 'the search'],
    ['/', 'Library', 'the library'],
    ['/?q=', 'Library', 'the library'],
    ['/?library=2&show=unread&sort=added', 'Library', 'the library'],
    ['/series/7', 'Series', 'the series'],
    ['/series/7?library=2', 'Series', 'the series'],
    ['/read/kaguya-v01', 'Book', 'the book'],
    ['/book/harry-potter-1', 'Book', 'the book'],
    ['/proposals', 'Proposals', 'Proposals'],
    ['/config', 'Configuration', 'Configuration'],
    ['/config/libraries/3', 'Configuration', 'Configuration'],
    ['/account', 'Account', 'Account'],
    ['/about', 'About', 'About'],
    ['/about/licence', 'About', 'About'],
    ['/search?q=kaguya', 'Back', ''],
    ['/configure', 'Back', ''],
    ['/aboutness', 'Back', ''],
    ['/accounts', 'Back', ''],
    ['/proposals/3', 'Back', ''],
    ['/series', 'Back', ''],
  ])('calls %s %s, %s', (path, word, phrase) => {
    expect(destination(path)).toEqual({ word, phrase })
  })
})
