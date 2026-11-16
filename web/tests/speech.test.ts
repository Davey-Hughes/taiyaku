import { afterEach, describe, expect, it, vi } from 'vitest'
import { hasVoice, speak } from '../src/speech'

/** A speech engine that speaks nothing, and says so as a browser would. */
function engine(voices: { lang: string; name: string }[] = []) {
  const spoken: SpeechSynthesisUtterance[] = []
  const synth = {
    getVoices: () => voices,
    speak: (u: SpeechSynthesisUtterance) => spoken.push(u),
    cancel: vi.fn(),
  }
  class Utterance {
    lang = ''
    rate = 1
    voice: unknown = null
    onstart?: () => void
    onend?: () => void
    onerror?: () => void
    readonly text: string
    constructor(text: string) {
      this.text = text
    }
  }
  vi.stubGlobal('speechSynthesis', synth)
  vi.stubGlobal('SpeechSynthesisUtterance', Utterance)
  /** The newest utterance starting and then finishing. */
  const finish = () => {
    const u = spoken.at(-1)
    u?.onstart?.(new Event('start') as SpeechSynthesisEvent)
    u?.onend?.(new Event('end') as SpeechSynthesisEvent)
  }
  return { spoken, synth, finish }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('reading aloud', () => {
  it('speaks each sentence in turn from the one given, in its language and pace', () => {
    const { spoken, finish } = engine([{ lang: 'ru-RU', name: 'Milena' }])
    const started: number[] = []
    const onEnd = vi.fn()
    speak(['Один.', 'Два.', 'Три.'], 1, 'ru', 0.8, {
      onSentence: (i) => started.push(i),
      onEnd,
    })
    expect(spoken.map((u) => [u.text, u.lang, u.rate])).toEqual([
      ['Два.', 'ru', 0.8],
    ])
    expect((spoken[0].voice as { name: string } | null)?.name).toBe('Milena')
    finish()
    finish()
    expect(started).toEqual([1, 2])
    expect(spoken.map((u) => u.text)).toEqual(['Два.', 'Три.'])
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('stops between sentences, once, cancelling what is being said', () => {
    const { spoken, synth, finish } = engine()
    const onEnd = vi.fn()
    const started: number[] = []
    const stop = speak(['A.', 'B.'], 0, 'en', 1, {
      onSentence: (i) => started.push(i),
      onEnd,
    })
    stop()
    stop()
    finish()
    expect(synth.cancel).toHaveBeenCalledTimes(1)
    expect(onEnd).toHaveBeenCalledTimes(1)
    expect(started).toEqual([])
    expect(spoken).toHaveLength(1)
  })

  it('ends at once where the browser has no speech, and on an error', () => {
    const onEnd = vi.fn()
    speak(['A.'], 0, 'en', 1, { onSentence: vi.fn(), onEnd })
    expect(onEnd).toHaveBeenCalledTimes(1)

    const { spoken } = engine()
    const failed = vi.fn()
    speak(['A.', 'B.'], 0, 'en', 1, { onSentence: vi.fn(), onEnd: failed })
    spoken[0].onerror?.(new Event('error') as SpeechSynthesisErrorEvent)
    expect(failed).toHaveBeenCalledTimes(1)
    expect(spoken).toHaveLength(1)
  })

  it('has no voice, and says it cannot speak, where the browser has no speech', () => {
    expect(hasVoice('en')).toBe(false)
    const onEnd = vi.fn()
    const onFail = vi.fn()
    speak(['A.'], 0, 'en', 1, { onSentence: vi.fn(), onEnd, onFail })
    expect(onFail).toHaveBeenCalledWith('synthesis-unavailable')
    expect(onEnd).toHaveBeenCalledTimes(1)
  })

  it('says why the browser could not speak, though not for a stop it was told', () => {
    const { spoken } = engine()
    const onFail = vi.fn()
    speak(['A.'], 0, 'ja', 1, { onSentence: vi.fn(), onEnd: vi.fn(), onFail })
    const failed = new Event('error') as SpeechSynthesisErrorEvent
    Object.defineProperty(failed, 'error', { value: 'synthesis-failed' })
    spoken[0].onerror?.(failed)
    expect(onFail).toHaveBeenCalledWith('synthesis-failed')

    const quiet = vi.fn()
    speak(['A.'], 0, 'ja', 1, {
      onSentence: vi.fn(),
      onEnd: vi.fn(),
      onFail: quiet,
    })
    const cut = new Event('error') as SpeechSynthesisErrorEvent
    Object.defineProperty(cut, 'error', { value: 'interrupted' })
    spoken[1].onerror?.(cut)
    expect(quiet).not.toHaveBeenCalled()
  })

  it('knows a voice by its language alone, and not before the voices are listed', () => {
    engine([{ lang: 'ja_JP', name: 'Kyoko' }])
    expect(hasVoice('ja')).toBe(true)
    expect(hasVoice('ko-KR')).toBe(false)
    engine([])
    expect(hasVoice('ja')).toBeUndefined()
  })
})
