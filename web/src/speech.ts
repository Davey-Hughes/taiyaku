/**
 * Reading sentences aloud with the browser's own voices: nothing leaves the
 * device, and a language the device has no voice for is said to have none
 * rather than read in another language's voice.
 */

export interface Reading {
  /** Called as each sentence starts, with its index in the texts given. */
  onSentence: (i: number) => void
  /** Called once, after the last sentence or on a stop. */
  onEnd: () => void
  /**
   * Called before `onEnd` when the browser could not speak, a device with
   * no speech engine among the causes, with its reason.
   */
  onFail?: (why: string) => void
}

/** A stop, which ends a reading early; a second call does nothing. */
export type Stop = () => void

/**
 * Whether the device has a voice for `lang`, matched on its language alone,
 * `ru` for `ru-RU`: none where the browser has no speech at all. Unknown
 * while the browser has yet to list its voices, which some list only after
 * the first ask.
 */
export function hasVoice(lang: string): boolean | undefined {
  const synth = globalThis.speechSynthesis
  if (!synth) return false
  const voices = synth.getVoices()
  if (voices.length === 0) return undefined
  return voices.some((v) => base(v.lang) === base(lang))
}

const base = (lang: string) => lang.toLowerCase().split(/[-_]/)[0]

/** The errors a browser reports for speech that was stopped, not refused. */
const STOPPED = new Set(['canceled', 'interrupted'])

/**
 * Speak `texts` from `from` on, one sentence at a time so each can be lit as
 * it is read, in `lang` at `rate` (1 the voice's own pace). A reading can end
 * before this returns, where the browser has no speech or refuses at once, so
 * the stop it returns is worth keeping only while `onEnd` has not been called.
 */
export function speak(
  texts: string[],
  from: number,
  lang: string | null,
  rate: number,
  reading: Reading,
): Stop {
  const synth = globalThis.speechSynthesis
  let stopped = false
  const end = () => {
    if (stopped) return
    stopped = true
    reading.onEnd()
  }
  if (!synth) {
    reading.onFail?.('synthesis-unavailable')
    end()
    return () => {}
  }
  const voice = lang
    ? synth.getVoices().find((v) => base(v.lang) === base(lang))
    : undefined
  const say = (i: number) => {
    if (stopped) return
    if (i >= texts.length) return end()
    const utterance = new SpeechSynthesisUtterance(texts[i])
    if (lang) utterance.lang = lang
    if (voice) utterance.voice = voice
    utterance.rate = rate
    utterance.onstart = () => {
      if (!stopped) reading.onSentence(i)
    }
    utterance.onend = () => say(i + 1)
    utterance.onerror = (event) => {
      // A stop cancels what is being said, which the browser reports as an
      // error too, and is no failure.
      if (!stopped && !STOPPED.has(event.error)) reading.onFail?.(event.error)
      end()
    }
    synth.speak(utterance)
  }
  say(from)
  return () => {
    if (stopped) return
    end()
    synth.cancel()
  }
}
