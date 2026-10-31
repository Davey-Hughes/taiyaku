import { act } from 'react'
import { describe, expect, it } from 'vitest'
import { noCohort } from '../src/cohort'
import {
  type Decodable,
  type ImageFactory,
  unboundedDecodeQueue,
} from '../src/decode'
import { fitModule } from '../src/modules/fit'
import { PageView } from '../src/PageView'
import { mount, rerender } from './support/mount'

/**
 * `PageView` in `jsdom`, covering the half of it that does not need a real
 * browser.
 *
 * `PageView.browser.test.tsx` already states the split: `jsdom` has no
 * layout engine (`getBoundingClientRect` returns zeroes) and no real
 * `HTMLImageElement.decode()`, and both are load-bearing for that file's own
 * tests — the two-cell geometry assertions and the "a real decode actually
 * resolves and paints" assertions. Neither is attempted here.
 *
 * What jsdom *can* answer honestly is everything `PageView` does around a
 * decode rather than the decode itself: the buffered swap gate, monotonic
 * ordering under overlapping attempts, `onShown` reporting, prop
 * pass-through to `pageStyle`, and default wiring for `decodeQueue` and
 * `cohort`. The seam that makes this possible without faking a browser is
 * `makeImage` — documented on `PageViewProps` as exactly this: "a future
 * test of this component would hand in a fake, the same seam
 * `decode.test.ts` already exercises directly against `presentWhenDecoded`".
 * Every buffered test below does that, modelled on `decode.test.ts`'s own
 * `fakeImage` helper.
 *
 * Deliberately left uncovered, and left to the browser project: the default
 * `makeImage` branch (`newImage`, i.e. a real `new Image()`), because
 * exercising it honestly means exercising a real `decode()` — which is
 * exactly the property `jsdom` does not have.
 */

interface ControlledImage {
  image: Decodable
  resolve: () => void
}

/** A `makeImage` factory whose decodes settle only when the test says so —
 * `decode.test.ts`'s `fakeImage`, reached through the same prop `PageView`
 * documents for this purpose. */
function controlledImages(): {
  makeImage: ImageFactory
  images: ControlledImage[]
} {
  const images: ControlledImage[] = []
  const makeImage: ImageFactory = () => {
    let resolveFn!: () => void
    const promise = new Promise<void>((resolve) => {
      resolveFn = resolve
    })
    const image: Decodable = { src: '', decode: () => promise }
    images.push({ image, resolve: resolveFn })
    return image
  }
  return { makeImage, images }
}

/** Drains the microtask queue a resolved fake decode runs through —
 * `decodeTo`'s await, the queue, the cohort, and the `setShown` it ends in —
 * inside `act` so the resulting state update is not a warning. */
async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

const imgOf = (host: HTMLElement) =>
  host.querySelector('img') as HTMLImageElement

describe('PageView', () => {
  it('draws the initial src immediately, without starting a decode', () => {
    const { makeImage, images } = controlledImages()
    const host = mount(
      <PageView src="a.png" alt="first page" buffered makeImage={makeImage} />,
    )
    const img = imgOf(host)
    expect(img.getAttribute('src')).toBe('a.png')
    expect(img.alt).toBe('first page')
    expect(img.getAttribute('draggable')).toBe('false')
    // `src === shown` on the very first render, so the effect's guard
    // returns before ever calling `makeImage`.
    expect(images).toHaveLength(0)
  })

  it('omits loading=lazy while buffered, and sets it while unbuffered', () => {
    const buffered = mount(<PageView src="a.png" alt="p" buffered />)
    expect(imgOf(buffered).getAttribute('loading')).toBeNull()

    const unbuffered = mount(<PageView src="a.png" alt="p" buffered={false} />)
    expect(imgOf(unbuffered).getAttribute('loading')).toBe('lazy')
  })

  it('unbuffered: swaps the shown src immediately, with no decode gate', () => {
    const host = mount(<PageView src="a.png" alt="p" buffered={false} />)
    rerender(<PageView src="b.png" alt="p" buffered={false} />)
    expect(imgOf(host).getAttribute('src')).toBe('b.png')
  })

  it('buffered: holds the outgoing src until the fake decode resolves, then swaps and reports it', async () => {
    const { makeImage, images } = controlledImages()
    const seen: string[] = []
    const onShown = (s: string) => seen.push(s)
    const host = mount(
      <PageView
        src="a.png"
        alt="p"
        buffered
        makeImage={makeImage}
        onShown={onShown}
      />,
    )
    expect(seen).toEqual(['a.png'])

    rerender(
      <PageView
        src="b.png"
        alt="p"
        buffered
        makeImage={makeImage}
        onShown={onShown}
      />,
    )
    // A decode has started against the new source...
    expect(images).toHaveLength(1)
    expect(images[0]?.image.src).toBe('b.png')
    // ...but nothing is shown until it settles: the old page stays up.
    expect(imgOf(host).getAttribute('src')).toBe('a.png')
    expect(seen).toEqual(['a.png'])

    images[0]?.resolve()
    await flush()

    expect(imgOf(host).getAttribute('src')).toBe('b.png')
    expect(seen).toEqual(['a.png', 'b.png'])
  })

  it('drops an older decode that settles after a newer one already presented', async () => {
    const { makeImage, images } = controlledImages()
    const host = mount(
      <PageView src="a.png" alt="p" buffered makeImage={makeImage} />,
    )

    rerender(<PageView src="b.png" alt="p" buffered makeImage={makeImage} />)
    rerender(<PageView src="c.png" alt="p" buffered makeImage={makeImage} />)
    // One decode attempt per src change, each against its own probe image.
    expect(images).toHaveLength(2)

    // The newer attempt (c) settles first and is drawn.
    images[1]?.resolve()
    await flush()
    expect(imgOf(host).getAttribute('src')).toBe('c.png')

    // The older attempt (b) settles late. Presenting it now would regress
    // the page a reader has already moved past — the whole reason
    // `ordering.ts` exists.
    images[0]?.resolve()
    await flush()
    expect(imgOf(host).getAttribute('src')).toBe('c.png')
  })

  it('drops a decode that settles after src came back to the page already drawn', async () => {
    const { makeImage, images } = controlledImages()
    const host = mount(
      <PageView src="a.png" alt="p" buffered makeImage={makeImage} />,
    )

    rerender(<PageView src="b.png" alt="p" buffered makeImage={makeImage} />)
    // Back to what is on screen before b has decoded: a turn and a turn back.
    // a is drawn already, so no decode starts for it...
    rerender(<PageView src="a.png" alt="p" buffered makeImage={makeImage} />)
    expect(images).toHaveLength(1)

    // ...and b, settling now, is older than the return. Drawn, it would put the
    // page the reader left over the one they came back to, until their next
    // turn.
    images[0]?.resolve()
    await flush()
    expect(imgOf(host).getAttribute('src')).toBe('a.png')
  })

  it('drops a decode that settles while unbuffered, so buffering again resumes from the current src', async () => {
    const { makeImage, images } = controlledImages()
    const host = mount(
      <PageView src="a.png" alt="p" buffered makeImage={makeImage} />,
    )
    rerender(<PageView src="b.png" alt="p" buffered makeImage={makeImage} />)
    expect(images).toHaveLength(1)

    // Unbuffered while b is still decoding, and moved on to c, which is drawn
    // straight away. b settles after that.
    rerender(
      <PageView src="c.png" alt="p" buffered={false} makeImage={makeImage} />,
    )
    images[0]?.resolve()
    await flush()
    expect(imgOf(host).getAttribute('src')).toBe('c.png')

    // Buffered again on c. b is older than c, so it is not what `shown` holds:
    // c stays on screen and nothing needs decoding.
    rerender(<PageView src="c.png" alt="p" buffered makeImage={makeImage} />)
    expect(imgOf(host).getAttribute('src')).toBe('c.png')
    expect(images).toHaveLength(1)
  })

  it('re-syncs shown to src while unbuffered, so a later re-buffer does not resume a stale frame', () => {
    const host = mount(<PageView src="a.png" alt="p" buffered={false} />)
    rerender(<PageView src="b.png" alt="p" buffered={false} />)
    expect(imgOf(host).getAttribute('src')).toBe('b.png')

    // Flip buffered on with the same src: `shown` already equals `src`
    // (kept in sync by the unbuffered branch), so nothing new is drawn.
    rerender(<PageView src="b.png" alt="p" buffered />)
    expect(imgOf(host).getAttribute('src')).toBe('b.png')
  })

  it('draws with fitModule.defaults and a centred hug unless told otherwise', () => {
    const host = mount(<PageView src="a.png" alt="p" buffered={false} />)
    const img = imgOf(host)
    // `fitModule.defaults.upscaleToFit` is true, which is what makes the
    // element fill its box rather than sizing to its own content.
    expect(img.style.width).toBe(
      fitModule.defaults.upscaleToFit ? '100%' : 'auto',
    )
    expect(img.style.objectPosition).toBe('50% 50%')
  })

  it('threads fit and hug straight into the drawn style', () => {
    const host = mount(
      <PageView
        src="a.png"
        alt="p"
        buffered={false}
        fit={{ upscaleToFit: false, upscaleFilter: 'crisp', gap: 0 }}
        hug="left-edge"
      />,
    )
    const img = imgOf(host)
    expect(img.style.width).toBe('auto')
    expect(img.style.objectPosition).toBe('0% 50%')
  })

  it('accepts an explicit decodeQueue and cohort in place of the defaults', async () => {
    const { makeImage, images } = controlledImages()
    const host = mount(
      <PageView
        src="a.png"
        alt="p"
        buffered
        makeImage={makeImage}
        decodeQueue={unboundedDecodeQueue}
        cohort={noCohort}
      />,
    )
    rerender(
      <PageView
        src="b.png"
        alt="p"
        buffered
        makeImage={makeImage}
        decodeQueue={unboundedDecodeQueue}
        cohort={noCohort}
      />,
    )
    images[0]?.resolve()
    await flush()
    expect(imgOf(host).getAttribute('src')).toBe('b.png')
  })
})
