import { describe, expect, it } from 'vitest'
import type { Alignment } from '../session'
import {
  chapterLine,
  chapterReadout,
  healthLine,
  healthReadout,
  nextTurns,
  pagesPerMinute,
  positionLine,
  readoutRow,
  readoutsModule,
  timeLine,
} from './readouts'

describe('positionLine', () => {
  it('shows a 1-based slot, the page, and a percentage', () => {
    expect(positionLine({ slot: 87, total: 195, page: 84 })).toBe('slot 88 / 195 · p. 84 · 45%')
  })

  it('reads 100% on the last slot, not 99%', () => {
    expect(positionLine({ slot: 194, total: 195, page: 190 })).toContain('100%')
  })

  it('reads 1% rather than 0% on the very first slot', () => {
    // Zero-based slot 0 of 195 is the first page read, not zero progress —
    // the percentage is derived from the 1-based slot, matching the count
    // printed right next to it, not from the raw zero-based index.
    expect(positionLine({ slot: 0, total: 195, page: 1 })).toContain('1%')
  })
})

describe('chapterLine', () => {
  it('names the chapter and its position', () => {
    expect(chapterLine({ member: 2, of: 7, path: 'x - c002.cbz' })).toBe('c002 · 3 of 7')
  })

  it('says the work has no chapter edition rather than showing nothing', () => {
    expect(chapterLine(undefined)).toBe('no chapter edition')
  })

  it('extracts the chapter code from a real fan-scan filename, dots and all', () => {
    // The corpus's own en-fan member path: a group tag, a title, and the
    // chapter code, dash-separated — and c005.5's own dot must survive
    // stripping only the trailing .cbz, not the first dot it meets.
    expect(
      chapterLine({
        member: 6,
        of: 7,
        path: 'en-fan/Go-Toubun no Hanayome - Fan Translated - c005.5.cbz',
      }),
    ).toBe('c005.5 · 7 of 7')
  })
})

describe('healthLine', () => {
  it('stays quiet on a sound pair with a counterpart', () => {
    expect(healthLine('sound', true)).toBeUndefined()
  })

  it('says so on a fragmented pair', () => {
    expect(healthLine('fragmented', true)).toBe('fragmented pair')
  })

  it('reports an unknown verdict as unknown, never as sound', () => {
    expect(healthLine(undefined, true)).toBe('verdict unknown')
  })

  it('names a sound pair explicitly once this exact slot has no counterpart', () => {
    // `sound` describes the whole pairing, not every slot in it — Gap.tsx's
    // own sentence says as much ("this is one isolated gap, not a sign of
    // trouble"). A slot-level absence is still worth a word even when the
    // pairing overall is fine.
    expect(healthLine('sound', false)).toBe('sound pair · no counterpart here')
  })

  it('appends the same counterpart note to a non-sound verdict', () => {
    expect(healthLine('refused', false)).toBe('refused pair · no counterpart here')
  })
})

describe('timeLine', () => {
  it('labels the remaining figure as an estimate', () => {
    expect(timeLine({ elapsedMs: 18 * 60_000, pagesPerMin: 4, slotsLeft: 88 })).toMatch(/~/)
  })

  it('omits the estimate rather than dividing by zero before any pages are turned', () => {
    expect(timeLine({ elapsedMs: 1000, pagesPerMin: 0, slotsLeft: 88 })).not.toMatch(/~/)
  })

  it('never reads to the minute on the elapsed side either — under a minute reads as <1m', () => {
    expect(timeLine({ elapsedMs: 1000, pagesPerMin: 0, slotsLeft: 88 })).toBe('<1m read')
  })

  it('reports the projected minutes remaining at the given pace', () => {
    expect(timeLine({ elapsedMs: 18 * 60_000, pagesPerMin: 4, slotsLeft: 88 })).toBe('18m read · ~22m left')
  })
})

describe('pagesPerMinute', () => {
  it('is zero with fewer than two turns — a rate needs two points', () => {
    expect(pagesPerMinute([], 0)).toBe(0)
    expect(pagesPerMinute([1_000], 1_000)).toBe(0)
  })

  it('divides the turn count by the span between the oldest and now', () => {
    // Three turns spread across 30s of the most recent window is 2 intervals
    // in 30s, i.e. 4 per minute.
    expect(pagesPerMinute([0, 15_000, 30_000], 30_000)).toBe(4)
  })

  it('is zero rather than infinite when every turn shares one timestamp', () => {
    expect(pagesPerMinute([5_000, 5_000], 5_000)).toBe(0)
  })
})

describe('nextTurns', () => {
  it('skips the first call — session load is not a page turn', () => {
    // The bug a browser caught: recording session-load itself as a turn
    // makes the mount-to-loaded interval look like a page turn, producing a
    // reading-time estimate before a single key has been pressed.
    expect(nextTurns([], 1_000, true)).toEqual([])
  })

  it('appends on a subsequent call', () => {
    expect(nextTurns([1_000], 2_000, false)).toEqual([1_000, 2_000])
  })

  it('caps at the ten most recent turns', () => {
    const turns = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
    expect(nextTurns(turns, 10, false)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
  })
})

describe('readoutRow', () => {
  it('joins in a fixed order regardless of which fields are set', () => {
    expect(readoutRow({ health: 'fragmented pair', position: 'slot 1 / 2 · p. 1 · 50%' })).toBe(
      'slot 1 / 2 · p. 1 · 50% · fragmented pair',
    )
  })

  it('skips an absent field rather than leaving an empty segment', () => {
    expect(readoutRow({ position: 'slot 1 / 2 · p. 1 · 50%', chapter: 'no chapter edition' })).toBe(
      'slot 1 / 2 · p. 1 · 50% · no chapter edition',
    )
  })

  it('orders health second, ahead of time and chapter, so a clipped row loses those first', () => {
    // Chrome.tsx clips from the end of this joined string on a narrow
    // viewport — health is the one segment this project cannot afford to
    // lose first (see this function's own doc), so it sits right after
    // position rather than trailing behind time and chapter.
    expect(
      readoutRow({
        position: 'slot 1 / 2 · p. 1 · 50%',
        time: '18m read · ~22m left',
        chapter: 'c002 · 3 of 7',
        health: 'fragmented pair',
      }),
    ).toBe('slot 1 / 2 · p. 1 · 50% · fragmented pair · 18m read · ~22m left · c002 · 3 of 7')
  })

  it('is the empty string with nothing enabled', () => {
    expect(readoutRow({})).toBe('')
  })
})

describe('chapterReadout', () => {
  const sound: Alignment = { from: 'hub', to: 'mono', quality: { verdict: 'sound' } }
  const fragmented: Alignment = { from: 'hub', to: 'fan', quality: { verdict: 'fragmented' } }

  it('marks the chapter line when a hub-touching alignment is not sound', () => {
    expect(chapterReadout({ member: 0, of: 7, path: 'c000.cbz' }, [sound, fragmented], 'hub')).toBe(
      '~c000 · 1 of 7',
    )
  })

  it('leaves the chapter line unmarked when every hub-touching alignment is sound', () => {
    expect(chapterReadout({ member: 0, of: 7, path: 'c000.cbz' }, [sound], 'hub')).toBe('c000 · 1 of 7')
  })

  it('never marks the no-chapter-edition case — there is nothing derived to caveat', () => {
    expect(chapterReadout(undefined, [fragmented], 'hub')).toBe('no chapter edition')
  })
})

describe('healthReadout', () => {
  const alignments: Alignment[] = [
    { from: 'raw-mono', to: 'en-mono', quality: { verdict: 'sound' } },
    { from: 'raw-mono', to: 'en-fan', quality: { verdict: 'fragmented' } },
  ]

  it('is silent while viewing the hub itself — there is no pair to judge', () => {
    expect(healthReadout(alignments, 'raw-mono', 'raw-mono')).toBeUndefined()
  })

  it('is silent on a sound edge', () => {
    expect(healthReadout(alignments, 'raw-mono', 'en-mono')).toBeUndefined()
  })

  it('names a fragmented edge', () => {
    expect(healthReadout(alignments, 'raw-mono', 'en-fan')).toBe('fragmented pair')
  })

  it('reports unknown for a spoke-to-spoke pair with no stored edge', () => {
    expect(healthReadout(alignments, 'en-mono', 'en-fan')).toBe('verdict unknown')
  })
})

describe('readoutsModule', () => {
  it('defaults every readout on — nothing is hidden until a reader hides it', () => {
    expect(readoutsModule.defaults).toEqual({ position: true, time: true, chapter: true, health: true })
  })

  it('claims no keys — every readout is toggled in settings, not mid-read', () => {
    expect(readoutsModule.keys ?? []).toEqual([])
  })
})
