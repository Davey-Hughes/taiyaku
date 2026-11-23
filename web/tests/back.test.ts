import { describe, expect, it } from 'vitest'
import { countsAsInput, destination, rewritesAtAnswer } from '../src/back'

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

/**
 * Which events count as the input after which a reader puts its series page
 * under itself (O431): the ones the HTML spec counts as activating the page.
 */
describe('countsAsInput', () => {
  const active = { isActive: true }
  const idle = { isActive: false }
  it.each([
    [{ type: 'keydown', key: 'ArrowRight' }, true],
    [{ type: 'keydown', key: 'Escape' }, false],
    [{ type: 'pointerdown', pointerType: 'mouse' }, true],
    [{ type: 'pointerdown', pointerType: 'touch' }, false],
    [{ type: 'pointerup', pointerType: 'touch' }, true],
    [{ type: 'pointerup', pointerType: 'mouse' }, false],
    [{ type: 'touchend' }, true],
  ])('counts %o: %s', (event, counts) => {
    expect([
      countsAsInput(event, undefined),
      countsAsInput(event, active),
      countsAsInput(event, idle),
    ]).toEqual([counts, counts, false])
  })
})

/**
 * Whether a reader puts its series page under itself as its answer lands,
 * not at the first input: the click that opened it still fresh, and the
 * browser not Firefox, on any system, while Firefox on an iPhone is WebKit.
 */
describe('rewritesAtAnswer', () => {
  const CHROME =
    'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36'
  const FIREFOX =
    'Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0'
  const FIREFOX_ANDROID =
    'Mozilla/5.0 (Android 15; Mobile; rv:145.0) Gecko/145.0 Firefox/145.0'
  const FIREFOX_IPHONE =
    'Mozilla/5.0 (iPhone; CPU iPhone OS 18_7 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/145.0 Mobile/15E148 Safari/605.1.15'
  const active = { isActive: true }
  it.each([
    ['fresh, in Chrome', true, active, CHROME, 400],
    ['fresh, in Firefox', false, active, FIREFOX, 400],
    ['fresh, in Firefox on Android', false, active, FIREFOX_ANDROID, 400],
    ['fresh, in Firefox on an iPhone', true, active, FIREFOX_IPHONE, 400],
    ['not active', false, { isActive: false }, CHROME, 400],
    ['with no userActivation', false, undefined, CHROME, 400],
    ['drawn 5 s ago', false, active, CHROME, 5000],
    ['drawn just under 5 s ago', true, active, CHROME, 4999],
  ])('%s: %s', (_, at, activation, userAgent, drawnFor) => {
    expect(rewritesAtAnswer(activation, userAgent, drawnFor)).toBe(at)
  })
})
