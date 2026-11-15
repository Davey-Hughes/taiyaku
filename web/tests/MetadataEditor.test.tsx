import { act, createRef, type RefObject } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as derive from '../src/libraries'
import { MetadataEditor, type TitleFlush } from '../src/MetadataEditor'
import * as stamp from '../src/stamp'
import { holdOf } from './support/held'
import { tick } from './support/libraries'
import { mount, rerender } from './support/mount'

// `AcceptForm.test.tsx`'s shape, for the reason its own comment gives.
// `humanBytes` is deliberately not mocked: `bytes.test.ts` pins it, and
// stubbing it here would leave this file unable to tell a section that calls
// it from one that prints its own number.
vi.mock('../src/stamp', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/stamp')>()),
  stampPlan: vi.fn(),
  performStamp: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  deriveStatus: vi.fn(),
}))

/**
 * Set a controlled field's value the way a keystroke does — through the
 * prototype's own setter, not the element's. `AcceptForm.test.tsx` carries
 * the same helper for the same reason: React tracks the value it last set on
 * the instance, and a plain `el.value = x` assignment races that tracker, so
 * the `input` event dispatched right after can fire with React believing
 * nothing changed.
 */
async function typeInto(el: HTMLTextAreaElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      Object.getPrototypeOf(el),
      'value',
    )?.set?.call(el, value)
    el.dispatchEvent(new Event('input', { bubbles: true }))
    await Promise.resolve()
  })
}

const click = async (el: Element) => {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

const titleField = (host: HTMLElement) =>
  host.querySelector<HTMLTextAreaElement>("[name='title']")

const planButton = (host: HTMLElement) =>
  host.querySelector<HTMLButtonElement>("button[name='plan-stamp']")

const stampButton = (host: HTMLElement) =>
  host.querySelector<HTMLButtonElement>("button[name='confirm-stamp']")

/**
 * Every read control on screen.
 *
 * **A list rather than the one**, because "one control at a time" is itself
 * an assertion.
 *
 * **Selected by exclusion — every button in the section that is not the
 * confirm — rather than by name.** A query by `button[name='plan-stamp']`
 * would be blind to a second control under another name, such as a
 * `replan-stamp` sharing `askPlan` and carrying its own label, and a named
 * list of the two would have the same hole one name wider, since the next
 * control to draw beside this one will not be called `replan-stamp` either.
 * This section draws one confirm and read controls, so *not the confirm* is
 * what *a read control* means in it, and a button of any name arriving beside
 * this one is a failure rather than an invisible addition.
 */
const readButtons = (host: HTMLElement) => [
  ...host.querySelectorAll<HTMLButtonElement>(
    ".stamp-comicinfo button:not([name='confirm-stamp'])",
  ),
]

/** One archive's row. Queried so assertions are per element, never page-wide. */
const archiveRows = (host: HTMLElement) => [
  ...host.querySelectorAll('.stamp-comicinfo li.archive'),
]

/** The line that says a plan read is out. */
const askingLine = (host: HTMLElement) =>
  host.querySelector('.stamp-comicinfo .asking')

/**
 * A promise this test settles by hand.
 *
 * **The only way to observe an in-flight render.** A test that awaits the
 * whole of `askPlan`'s promise sees the answer and never the gap it opened,
 * so `askPlan`'s `setPlan(undefined)` could be deleted without a failure:
 * what it blanks is overwritten a microtask later by the answer itself.
 * What it actually protects is the frame in between, where a stale
 * `Rewrite 2 archives` button would sit live while the new read is out.
 */
function deferred<T>() {
  let settle!: (value: T) => void
  let fail!: (reason: unknown) => void
  const promise = new Promise<T>((resolve, reject) => {
    settle = resolve
    fail = reject
  })
  return { promise, settle, fail }
}

/** Let React drain the microtasks a hand-settled promise queued. */
const drain = async () => {
  await act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })
}

/** A keydown of `key` in `el`; whether its default action was left to run. */
const keyDown = async (el: Element, key: string, init?: KeyboardEventInit) => {
  let runs = true
  await act(async () => {
    runs = el.dispatchEvent(
      new KeyboardEvent('keydown', {
        key,
        bubbles: true,
        cancelable: true,
        ...init,
      }),
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  return runs
}

const focusOn = async (el: HTMLElement) => {
  await act(async () => {
    el.focus()
  })
}

const blurFrom = async (el: HTMLElement) => {
  await act(async () => {
    el.blur()
  })
}

/** `text` typed into the Title field, and Enter pressed there. */
const enterWith = async (host: HTMLElement, text: string) => {
  const field = titleField(host)!
  await focusOn(field)
  await typeInto(field, text)
  await keyDown(field, 'Enter')
}

/** `text` typed into the Title field, and the field left. */
const leaveWith = async (host: HTMLElement, text: string) => {
  const field = titleField(host)!
  await focusOn(field)
  await typeInto(field, text)
  await blurFrom(field)
}

describe('MetadataEditor', () => {
  it('saves a corrected title as the field is left, and says Saved', async () => {
    const patch = vi
      .fn()
      .mockResolvedValue({ title: 'Volume 1', persisted: true })
    const host = mount(
      <MetadataEditor
        slug="w"
        title="Series - v01 (2021).cbz"
        onPatch={patch}
      />,
    )
    expect(titleField(host)?.value).toBe('Series - v01 (2021).cbz')

    await leaveWith(host, 'Volume 1')

    expect(patch.mock.calls).toEqual([['w', { title: 'Volume 1' }]])
    expect(host.querySelector('[role="status"]')?.textContent).toBe('Saved')
  })

  /**
   * The server answers the title as it re-projected it, which can differ from
   * what was typed. The field keeps what was typed (O283), and nothing
   * repeats the server's word under it.
   */
  it('keeps the typed title in the field, not the server’s re-projected one', async () => {
    const patch = vi.fn().mockResolvedValue({
      title: 'Volume 1: The Reprojected Title',
      persisted: true,
    })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await enterWith(host, 'Volume 1')
    await drain()

    expect(patch.mock.calls).toEqual([['w', { title: 'Volume 1' }]])
    expect(titleField(host)?.value).toBe('Volume 1')
    expect(host.querySelector('p.current')).toBeNull()
  })

  it('says so when the correction could not be written beside the library', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: false })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await enterWith(host, 'new')

    expect(host.textContent).toMatch(/could not be saved beside your library/i)
  })

  it('says nothing about being unwritten when the correction landed cleanly', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await enterWith(host, 'new')

    expect(patch).toHaveBeenCalledTimes(1)
    expect(host.textContent).not.toMatch(/could not be saved/i)
  })

  /**
   * The claim is stronger than "no particular wording appears": a response
   * with `keeps_documents: false` must render **exactly** what an ordinary
   * successful save renders — nothing at all keyed off that field, not even
   * a new note a pinned string would not know to look for. Pinning one
   * wording (`/could not be saved beside your library/i`) would catch a
   * regression that reuses that exact note but wave through a new alarm
   * element keyed off `keeps_documents` with different wording; comparing two
   * full renders catches either.
   */
  it('renders exactly what an ordinary successful save renders when keeps_documents is false', async () => {
    const ordinaryPatch = vi
      .fn()
      .mockResolvedValue({ title: 'new', persisted: true })
    const ordinary = mount(
      <MetadataEditor slug="w" title="old" onPatch={ordinaryPatch} />,
    )
    await enterWith(ordinary, 'new')
    expect(ordinaryPatch, 'on Enter').toHaveBeenCalledTimes(1)

    const declinedPatch = vi.fn().mockResolvedValue({
      title: 'new',
      persisted: true,
      keeps_documents: false,
    })
    const declined = mount(
      <MetadataEditor slug="w" title="old" onPatch={declinedPatch} />,
    )
    // Focus moving here left the first field; this one is left too, so the
    // two differ by nothing their focus marks.
    await enterWith(declined, 'new')
    expect(declinedPatch, 'on Enter').toHaveBeenCalledTimes(1)
    await blurFrom(titleField(declined)!)

    expect(ordinaryPatch, 'after both were left').toHaveBeenCalledTimes(1)
    expect(declinedPatch, 'after both were left').toHaveBeenCalledTimes(1)
    expect(declined.innerHTML).toBe(ordinary.innerHTML)
  })

  it('names every reason a fanned-out edit could not all reach disk', async () => {
    const patch = vi.fn().mockResolvedValue({
      title: 'old',
      persisted: false,
      why: ['root is read-only', 'no on-disk address'],
    })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await enterWith(host, 'new')

    expect(host.textContent).toContain('root is read-only')
    expect(host.textContent).toContain('no on-disk address')
  })

  it('surfaces a hard failure as an alert, distinct from an unwritten sidecar', async () => {
    const patch = vi
      .fn()
      .mockRejectedValue(
        new Error('/api/library/works/w: 409 a name this route refused'),
      )
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await enterWith(host, 'new')

    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      '409 a name this route refused',
    )
    expect(host.textContent).not.toMatch(/could not be saved beside/i)
  })

  it('stringifies a rejection that is not an Error, rather than losing it', async () => {
    const patch = vi.fn().mockRejectedValue('offline')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await enterWith(host, 'new')

    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })

  /**
   * What `saveTitle`'s `setFailure(undefined)` protects is the frame while
   * the save is out, where the previous save's alert would sit over it.
   */
  it('clears the last save failure while the next save is out', async () => {
    // The two fields this editor reads, not the whole document — the other
    // `onPatch` stubs in this file are untyped `vi.fn()`s and answer the same
    // pair.
    const retried = deferred<{ title: string; persisted: boolean }>()
    const patch = vi
      .fn()
      .mockRejectedValueOnce(new Error('offline'))
      .mockReturnValueOnce(retried.promise)
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    await enterWith(host, 'Volume 1')
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')

    // Resent: the title still differs from the one stored.
    await keyDown(titleField(host)!, 'Enter')

    expect(patch).toHaveBeenCalledTimes(2)
    expect(host.querySelector('[role="alert"]')).toBeNull()

    retried.settle({ title: 'Volume 1', persisted: true })
    await drain()

    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('[role="status"]')?.textContent).toBe('Saved')
  })
})

/**
 * Paste `text` into `el` at its selection as a browser does: the `paste`
 * event, then, unless a handler took it, the text put in and an `input`.
 */
const pasteInto = async (el: HTMLTextAreaElement, text: string) => {
  await act(async () => {
    const paste = new Event('paste', { bubbles: true, cancelable: true })
    Object.defineProperty(paste, 'clipboardData', {
      value: { getData: () => text },
    })
    if (el.dispatchEvent(paste)) {
      el.setRangeText(text, el.selectionStart, el.selectionEnd, 'end')
      el.dispatchEvent(new Event('input', { bubbles: true }))
    }
    await Promise.resolve()
  })
}

describe('MetadataEditor, the Title box’s keys (O274)', () => {
  it('saves on Enter what the field holds, and puts no line break in it', async () => {
    const patch = vi
      .fn()
      .mockResolvedValue({ title: 'Volume 1', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, 'Volume 1')

    const runs = await keyDown(field, 'Enter')

    expect(patch.mock.calls).toEqual([['w', { title: 'Volume 1' }]])
    expect(runs, 'the Enter’s own line break').toBe(false)
  })

  it('sends no second save for an Enter while one is out, and none after it for an unchanged title', async () => {
    const out = deferred<{ title: string; persisted: boolean }>()
    const patch = vi.fn().mockReturnValue(out.promise)
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'new')
    expect(patch).toHaveBeenCalledTimes(1)

    await keyDown(field, 'Enter')

    expect(patch, 'while the save is out').toHaveBeenCalledTimes(1)
    out.settle({ title: 'new', persisted: true })
    await drain()

    await keyDown(field, 'Enter')

    expect(patch, 'after it, for the title it stored').toHaveBeenCalledTimes(1)
  })

  it('leaves an Enter or an Escape that ends an input method’s composition to that method', async () => {
    const patch = vi
      .fn()
      .mockResolvedValue({ title: '鏡の国', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, '鏡の国')

    const enter = await keyDown(field, 'Enter', { isComposing: true })
    const escaped = await keyDown(field, 'Escape', { isComposing: true })

    // One comparison: each key's default left to run, no save, no restore.
    expect({
      enter,
      escape: escaped,
      saves: patch.mock.calls.length,
      value: field.value,
    }).toEqual({ enter: true, escape: true, saves: 0, value: '鏡の国' })

    // The pair: the same Enter, the composition over, saves.
    await keyDown(field, 'Enter')
    expect(patch.mock.calls).toEqual([['w', { title: '鏡の国' }]])
  })

  it('leaves to the input method the Enter Safari sends once its composition has ended, keyCode 229', async () => {
    const patch = vi
      .fn()
      .mockResolvedValue({ title: '鏡の国', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, '鏡の国')

    const runs = await keyDown(field, 'Enter', { keyCode: 229 })

    expect({ runs, saves: patch.mock.calls.length }).toEqual({
      runs: true,
      saves: 0,
    })
    // The pair: an Enter of its own saves.
    await keyDown(field, 'Enter', { keyCode: 13 })
    expect(patch.mock.calls).toEqual([['w', { title: '鏡の国' }]])
  })

  it('puts back on Escape the title saved as the field was last left, and keeps the focus', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'held', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await leaveWith(host, 'held')
    await drain()
    expect(patch.mock.calls).toEqual([['w', { title: 'held' }]])
    await focusOn(field)
    await typeInto(field, 'typed')

    await keyDown(field, 'Escape')

    expect(field.value).toBe('held')
    expect(document.activeElement).toBe(field)
  })

  it('puts back on Escape the title last saved, where the save came after the box took focus', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'saved', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, 'saved')
    await keyDown(field, 'Enter')
    await drain()
    expect(patch.mock.calls).toEqual([['w', { title: 'saved' }]])
    await typeInto(field, 'saved, then typed')

    await keyDown(field, 'Escape')

    expect(field.value).toBe('saved')
  })

  it('puts a pasted title in at the caret, each line break a space, and leaves the caret after it', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'x', persisted: true })
    const host = mount(
      <MetadataEditor slug="w" title="Kagami Volume 1" onPatch={patch} />,
    )
    const field = titleField(host)!
    await focusOn(field)
    field.setSelectionRange(7, 7)

    await pasteInto(field, 'no\nKuni ')

    expect({ value: field.value, caret: field.selectionStart }).toEqual({
      value: 'Kagami no Kuni Volume 1',
      caret: 'Kagami no Kuni '.length,
    })
    await keyDown(field, 'Enter')
    expect(patch.mock.calls).toEqual([
      ['w', { title: 'Kagami no Kuni Volume 1' }],
    ])
  })

  it('turns each line break that reaches the box another way, such as a drop, into one space', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'x', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)

    await typeInto(field, 'Kagami no Kuni\nno Harisugawa\r\nVolume 1')

    expect(field.value).toBe('Kagami no Kuni no Harisugawa Volume 1')
    await keyDown(field, 'Enter')
    expect(patch.mock.calls).toEqual([
      ['w', { title: 'Kagami no Kuni no Harisugawa Volume 1' }],
    ])
  })

  it('saves the typed title when focus leaves the field, and keeps it there', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'typed', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, 'typed')

    await blurFrom(field)
    await drain()

    expect(field.value).toBe('typed')
    expect(patch.mock.calls).toEqual([['w', { title: 'typed' }]])
  })

  it('asks for no spellcheck, which a textarea gets by default and a romaji title does not want, and labels Enter as done', () => {
    const host = mount(
      <MetadataEditor slug="w" title="old" onPatch={vi.fn()} />,
    )
    const field = titleField(host)!

    expect({
      spellcheck: field.getAttribute('spellcheck'),
      enterkeyhint: field.getAttribute('enterkeyhint'),
    }).toEqual({ spellcheck: 'false', enterkeyhint: 'done' })
  })
})

describe('MetadataEditor, the Title field saves as it is left (O283)', () => {
  /** A save that lands, so a save sent where none should be is counted. */
  const landing = () =>
    vi.fn().mockResolvedValue({ title: 'saved', persisted: true })

  const status = (host: HTMLElement) => [
    ...host.querySelectorAll('[role="status"]'),
  ]

  it('sends nothing when the field is left, or Enter pressed, with its title unchanged', async () => {
    const patch = landing()
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!

    await focusOn(field)
    await blurFrom(field)
    await focusOn(field)
    await keyDown(field, 'Enter')
    await drain()

    expect(patch).not.toHaveBeenCalled()
  })

  it.each(['leave', 'Enter'])(
    'puts the stored title back for a blank title on %s, and sends nothing',
    async (how) => {
      const patch = landing()
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      const field = titleField(host)!
      await focusOn(field)
      await typeInto(field, '   ')

      if (how === 'leave') await blurFrom(field)
      else await keyDown(field, 'Enter')
      await drain()

      expect({ value: field.value, saves: patch.mock.calls.length }).toEqual({
        value: 'old',
        saves: 0,
      })
    },
  )

  // Never the label's text: the field's value is text inside the label too.
  it('says Saved in a status region there from the first render, outside the label', async () => {
    const host = mount(
      <MetadataEditor slug="w" title="old" onPatch={landing()} />,
    )
    const label = host.querySelector('label.field.title')!

    expect(
      status(host).map((s) => s.textContent),
      'at mount',
    ).toEqual([''])
    expect(label.contains(status(host)[0]), 'at mount, in the label').toBe(
      false,
    )
    expect(label.querySelector('span')?.textContent).toBe('Title')

    await enterWith(host, 'new')
    await drain()

    expect(
      status(host).map((s) => s.textContent),
      'saved',
    ).toEqual(['Saved'])
    expect(label.contains(status(host)[0]), 'saved, in the label').toBe(false)
  })

  describe('clears Saved after 3 s, and at the next edit', () => {
    beforeEach(() => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    })
    afterEach(() => {
      vi.useRealTimers()
    })

    it('after 3 s', async () => {
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={landing()} />,
      )
      await enterWith(host, 'new')
      await drain()

      await act(async () => {
        vi.advanceTimersByTime(2999)
      })
      expect(status(host)[0]?.textContent, 'at 2999 ms').toBe('Saved')
      await act(async () => {
        vi.advanceTimersByTime(1)
      })
      expect(status(host)[0]?.textContent, 'at 3000 ms').toBe('')
    })

    it('at once at the next edit', async () => {
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={landing()} />,
      )
      await enterWith(host, 'new')
      await drain()
      expect(status(host)[0]?.textContent, 'saved').toBe('Saved')

      await typeInto(titleField(host)!, 'newer')

      expect(status(host)[0]?.textContent, 'edited').toBe('')
    })
  })

  // Settled by `drain`: `allLanded` waits on a `setTimeout` the fakes hold.
  describe('says Saving… in Saved’s place once a save has been out 200 ms', () => {
    beforeEach(() => {
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
    })
    afterEach(() => {
      vi.useRealTimers()
    })

    /** The clock run `ms`, and what it set drawn. */
    const wait = async (ms: number) => {
      await act(async () => {
        vi.advanceTimersByTime(ms)
      })
    }

    /** One save, left out until the test settles it. */
    const heldSave = () => {
      const out = deferred<Answer>()
      return { out, patch: vi.fn().mockReturnValue(out.promise) }
    }

    it('says Saving… once a title save has been out 200 ms, and Saved as it lands', async () => {
      const { out, patch } = heldSave()
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      const note = status(host)[0]
      await leaveWith(host, 'new')

      await wait(199)
      expect(status(host)[0], 'the region at 199 ms').toBe(note)
      expect(note?.textContent, 'at 199 ms').toBe('')
      await wait(1)
      expect(status(host)[0], 'the region at 200 ms').toBe(note)
      expect(note?.textContent, 'at 200 ms').toBe('Saving…')

      out.settle({ title: 'new', persisted: true })
      await drain()

      expect(status(host)[0], 'the region once landed').toBe(note)
      expect(note?.textContent, 'landed').toBe('Saved')
    })

    it('says no Saving… for a save that lands within 200 ms', async () => {
      const { out, patch } = heldSave()
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      await leaveWith(host, 'new')

      await wait(150)
      expect(status(host)[0]?.textContent, 'out at 150 ms').toBe('')

      out.settle({ title: 'new', persisted: true })
      await drain()
      expect(status(host)[0]?.textContent, 'landed at 150 ms').toBe('Saved')
      await wait(100)
      expect(status(host)[0]?.textContent, '100 ms after').toBe('Saved')
    })

    it('keeps Saving… through the newer title sent after the save that was out', async () => {
      const first = deferred<Answer>()
      const second = deferred<Answer>()
      const patch = vi
        .fn()
        .mockReturnValueOnce(first.promise)
        .mockReturnValueOnce(second.promise)
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      await leaveWith(host, 'A')
      await wait(250)
      expect(status(host)[0]?.textContent, 'A out').toBe('Saving…')

      await leaveWith(host, 'AB')
      expect(patch, 'AB left while A is out').toHaveBeenCalledTimes(1)
      first.settle({ title: 'A', persisted: true })
      await drain()

      expect(patch.mock.calls, 'A landed').toEqual([
        ['w', { title: 'A' }],
        ['w', { title: 'AB' }],
      ])
      expect(status(host)[0]?.textContent, 'AB out').toBe('Saving…')

      second.settle({ title: 'AB', persisted: true })
      await drain()

      expect(status(host)[0]?.textContent, 'AB landed').toBe('Saved')
    })

    it('takes Saving… away when the save fails', async () => {
      const { out, patch } = heldSave()
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      await leaveWith(host, 'new')
      await wait(250)
      expect(status(host)[0]?.textContent, 'out').toBe('Saving…')

      out.fail(new Error('offline'))
      await drain()

      expect({
        status: status(host)[0]?.textContent,
        alert: host.querySelector('[role="alert"]')?.textContent,
      }).toEqual({ status: '', alert: 'offline' })
    })
  })

  it('puts back the stored title on Escape after a failed save', async () => {
    const patch = vi.fn().mockRejectedValue(new Error('offline'))
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await leaveWith(host, 'X')
    await drain()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')

    await focusOn(field)
    await keyDown(field, 'Escape')

    expect(field.value).toBe('old')
  })

  // The failure spoke of the title Escape discarded.
  it('takes the failure away on the Escape that puts back the stored title', async () => {
    const patch = vi.fn().mockRejectedValue(new Error('offline'))
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await leaveWith(host, 'X')
    await drain()
    expect(
      host.querySelector('[role="alert"]')?.textContent,
      'before Escape',
    ).toBe('offline')

    await focusOn(field)
    await keyDown(field, 'Escape')

    expect(field.value, 'put back').toBe('old')
    expect(host.querySelector('[role="alert"]'), 'after Escape').toBeNull()
  })

  // That note speaks of the stored title, which Escape puts back.
  it('keeps the note that a save was not written beside the library through that Escape', async () => {
    const patch = vi
      .fn()
      .mockResolvedValueOnce({ title: 'A', persisted: false })
      .mockRejectedValueOnce(new Error('offline'))
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await leaveWith(host, 'A')
    await drain()
    await leaveWith(host, 'AB')
    await drain()
    const shown = () => ({
      value: field.value,
      alert: host.querySelector('[role="alert"]')?.textContent ?? null,
      unwritten: host.querySelector('.unwritten') !== null,
    })
    expect(shown(), 'AB failed').toEqual({
      value: 'AB',
      alert: 'offline',
      unwritten: true,
    })

    await focusOn(field)
    await keyDown(field, 'Escape')

    expect(shown(), 'after Escape').toEqual({
      value: 'A',
      alert: null,
      unwritten: true,
    })
  })

  /**
   * A title save that failed as the field was left: the field holds the
   * failed title, and its failure is shown under it.
   */
  const afterAFailure = async () => {
    const patch = vi.fn().mockRejectedValue(new Error('offline'))
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    const field = titleField(host)!
    await leaveWith(host, 'X')
    await drain()
    expect(host.querySelector('[role="alert"]')?.textContent, 'X failed').toBe(
      'offline',
    )
    return { patch, handle, host, field }
  }

  // The failure spoke of a title no longer in the field.
  it('takes the failure away when the field is left blank after a failed save', async () => {
    const { patch, host, field } = await afterAFailure()

    await focusOn(field)
    await typeInto(field, '')
    await blurFrom(field)
    await drain()

    expect(field.value, 'put back').toBe('old')
    expect(host.querySelector('[role="alert"]'), 'left').toBeNull()
    expect(patch, 'X only').toHaveBeenCalledTimes(1)
  })

  it('takes the failure away when the stored title is typed back and the field left', async () => {
    const { patch, host } = await afterAFailure()

    await leaveWith(host, 'old')
    await drain()

    expect(host.querySelector('[role="alert"]'), 'left').toBeNull()
    expect(patch, 'X only').toHaveBeenCalledTimes(1)
  })

  it('takes the failure away when the stored title is typed back and Enter pressed', async () => {
    const { patch, host } = await afterAFailure()

    await enterWith(host, 'old')
    await drain()

    expect(host.querySelector('[role="alert"]'), 'Enter').toBeNull()
    expect(patch, 'X only').toHaveBeenCalledTimes(1)
  })

  it('takes the failure away when a close flushes a field back at the stored title', async () => {
    const { handle, host, field } = await afterAFailure()
    await focusOn(field)
    await typeInto(field, 'old')

    let answer: Promise<boolean> | null | undefined
    await act(async () => {
      answer = handle.current?.flush()
    })

    expect(answer, 'flush').toBeNull()
    expect(host.querySelector('[role="alert"]'), 'flushed').toBeNull()
  })

  // That note speaks of the stored title, the one the field is left at.
  it('keeps the unwritten note through a leave back at the stored title', async () => {
    const patch = vi
      .fn()
      .mockResolvedValueOnce({ title: 'A', persisted: false })
      .mockRejectedValueOnce(new Error('offline'))
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await leaveWith(host, 'A')
    await drain()
    await leaveWith(host, 'AB')
    await drain()
    const shown = () => ({
      alert: host.querySelector('[role="alert"]')?.textContent ?? null,
      unwritten: host.querySelector('.unwritten') !== null,
    })
    expect(shown(), 'AB failed').toEqual({ alert: 'offline', unwritten: true })

    await leaveWith(host, 'A')
    await drain()

    expect(field.value, 'typed back').toBe('A')
    expect(shown(), 'left at A').toEqual({ alert: null, unwritten: true })
  })

  it('sends nothing when the field is left after Escape put its title back', async () => {
    const patch = landing()
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, 'typed')
    await keyDown(field, 'Escape')

    await blurFrom(field)
    await drain()

    expect(patch).not.toHaveBeenCalled()
  })

  it('marks the field data-editing from focus until Escape, again at the next edit, and not after blur', async () => {
    const host = mount(
      <MetadataEditor slug="w" title="old" onPatch={landing()} />,
    )
    const field = titleField(host)!
    const marked = () => field.hasAttribute('data-editing')
    expect(marked(), 'at mount').toBe(false)

    await focusOn(field)
    expect(marked(), 'focused').toBe(true)

    await keyDown(field, 'Escape')
    expect(marked(), 'after Escape').toBe(false)

    await typeInto(field, 'typed')
    expect(marked(), 'typed after Escape').toBe(true)

    await blurFrom(field)
    await drain()
    expect(marked(), 'left').toBe(false)
  })
})

/** The two fields this editor reads of a save's answer. */
type Answer = { title: string; persisted: boolean }

/**
 * Every microtask a settled save queued has run, a save it sent in turn
 * included: a macrotask comes after them all.
 */
const allLanded = async () => {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

describe('MetadataEditor, a title changed while a save is out (O283)', () => {
  /** A first save left out until the test settles it; every later one lands. */
  const firstOut = (later: string) => {
    const out = deferred<Answer>()
    const patch = vi
      .fn()
      .mockReturnValueOnce(out.promise)
      .mockResolvedValue({ title: later, persisted: true })
    return { out, patch }
  }

  it('sends a title changed while a save is out once it returns, the newest text last, one at a time', async () => {
    const { out, patch } = firstOut('ABC')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')
    await keyDown(field, 'Enter')
    await typeInto(field, 'ABC')
    await blurFrom(field)
    await drain()

    expect(patch.mock.calls, 'while it is out').toEqual([['w', { title: 'A' }]])

    out.settle({ title: 'A', persisted: true })
    await allLanded()

    expect(patch.mock.calls, 'after it').toEqual([
      ['w', { title: 'A' }],
      ['w', { title: 'ABC' }],
    ])
  })

  it.each(['leave', 'Enter'])(
    'sends a title changed while a save is out on %s, once that save returns',
    async (how) => {
      const { out, patch } = firstOut('AB')
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      const field = titleField(host)!
      await enterWith(host, 'A')
      await typeInto(field, 'AB')

      if (how === 'leave') await blurFrom(field)
      else await keyDown(field, 'Enter')
      out.settle({ title: 'A', persisted: true })
      await allLanded()

      expect(patch.mock.calls).toEqual([
        ['w', { title: 'A' }],
        ['w', { title: 'AB' }],
      ])
    },
  )

  it('sends nothing queued after a save that failed, and shows the failure', async () => {
    const { out, patch } = firstOut('AB')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')
    await keyDown(field, 'Enter')

    out.fail(new Error('offline'))
    await allLanded()

    expect(patch.mock.calls, 'after the failure').toEqual([
      ['w', { title: 'A' }],
    ])
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')

    await keyDown(field, 'Enter')

    expect(patch.mock.calls, 'at the next Enter').toEqual([
      ['w', { title: 'A' }],
      ['w', { title: 'AB' }],
    ])
  })

  it('sends no blank title once the save out returns, where the field has been cleared since', async () => {
    const { out, patch } = firstOut('AB')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')
    await keyDown(field, 'Enter')
    await typeInto(field, '')

    out.settle({ title: 'A', persisted: true })
    await allLanded()

    expect(patch.mock.calls).toEqual([['w', { title: 'A' }]])
  })

  // The field saves when it is left or on Enter, never as it is typed in.
  it('sends nothing once the save out returns for a title typed since but neither left nor entered', async () => {
    const { out, patch } = firstOut('AB')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    await enterWith(host, 'A')
    await typeInto(titleField(host)!, 'AB')

    out.settle({ title: 'A', persisted: true })
    await allLanded()

    expect(patch.mock.calls).toEqual([['w', { title: 'A' }]])
  })

  it('sends nothing after the newest text’s own save for a title typed since but neither left nor entered', async () => {
    const first = deferred<Answer>()
    const second = deferred<Answer>()
    const patch = vi
      .fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
      .mockResolvedValue({ title: 'ABC', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')
    await keyDown(field, 'Enter')
    first.settle({ title: 'A', persisted: true })
    await allLanded()
    await typeInto(field, 'ABC')

    second.settle({ title: 'AB', persisted: true })
    await allLanded()

    expect(patch.mock.calls).toEqual([
      ['w', { title: 'A' }],
      ['w', { title: 'AB' }],
    ])
  })

  it('sends a title changed once the save before it has landed', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'B', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await allLanded()

    await typeInto(field, 'B')
    await blurFrom(field)
    await allLanded()

    expect(patch.mock.calls).toEqual([
      ['w', { title: 'A' }],
      ['w', { title: 'B' }],
    ])
  })

  it.each(['leave', 'Enter'])(
    'puts back the title being sent for a blank title on %s while the field’s own save is out, and sends nothing after it',
    async (how) => {
      const { out, patch } = firstOut('A')
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={patch} />,
      )
      const field = titleField(host)!
      await enterWith(host, 'A')
      await typeInto(field, '   ')

      if (how === 'leave') await blurFrom(field)
      else await keyDown(field, 'Enter')

      expect(field.value, 'put back').toBe('A')
      out.settle({ title: 'A', persisted: true })
      await allLanded()

      expect({ value: field.value, calls: patch.mock.calls }).toEqual({
        value: 'A',
        calls: [['w', { title: 'A' }]],
      })
    },
  )

  it('puts back on Escape the title being sent while the field’s own save is out', async () => {
    const { out, patch } = firstOut('A')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')

    await keyDown(field, 'Escape')

    // Soft, so the leave below still runs on a wrong put-back: left while
    // the save is out, an older title would be sent over the newer one.
    expect.soft(field.value, 'put back').toBe('A')
    await blurFrom(field)
    out.settle({ title: 'A', persisted: true })
    await allLanded()

    expect({ value: field.value, calls: patch.mock.calls }).toEqual({
      value: 'A',
      calls: [['w', { title: 'A' }]],
    })
  })

  it('withdraws on Escape a save asked for while one is out, and sends no title typed after it', async () => {
    const { out, patch } = firstOut('AX')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')
    await keyDown(field, 'Enter')
    await keyDown(field, 'Escape')
    await typeInto(field, 'AX')

    out.settle({ title: 'A', persisted: true })
    await allLanded()

    expect(patch.mock.calls).toEqual([['w', { title: 'A' }]])
  })

  it('puts back on Escape the title being sent while the field’s own save is out, and the stored one once that save has failed', async () => {
    const { out, patch } = firstOut('A')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)
    const field = titleField(host)!
    await enterWith(host, 'A')
    await typeInto(field, 'AB')
    await keyDown(field, 'Escape')
    expect(field.value, 'while it is out').toBe('A')

    out.fail(new Error('offline'))
    await allLanded()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
    // `Series` leaves the field an Escape only while it is marked.
    expect(
      field.hasAttribute('data-editing'),
      'marked again, focused, once it has failed',
    ).toBe(true)

    await keyDown(field, 'Escape')

    expect(field.value, 'once it has failed').toBe('old')
  })
})

describe('MetadataEditor’s flush, which a close waits on (O283)', () => {
  /**
   * `flush` called as `Series` calls it, from outside the editor. Its
   * answer comes back wrapped: an async function's own answer would wait
   * on it.
   */
  const flushFrom = async (handle: RefObject<TitleFlush | null>) => {
    let answer: Promise<boolean> | null | undefined
    await act(async () => {
      answer = handle.current?.flush()
    })
    return { answer }
  }

  /** What an answer has settled to so far: `'out'` until it settles. */
  const watch = (answer: Promise<boolean> | null | undefined) => {
    let now: boolean | 'out' = 'out'
    void answer?.then((ok) => {
      now = ok
    })
    return () => now
  }

  it('flush answers null with nothing unsaved and no save out', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: true })
    const handle = createRef<TitleFlush>()
    mount(<MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />)

    const { answer } = await flushFrom(handle)
    await allLanded()

    expect({ answer, saves: patch.mock.calls.length }).toEqual({
      answer: null,
      saves: 0,
    })
  })

  it('flush answers null once the save out has landed with nothing typed since', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'new', persisted: true })
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    await enterWith(host, 'new')
    await allLanded()

    const { answer } = await flushFrom(handle)
    await allLanded()

    expect({ answer, saves: patch.mock.calls.length }).toEqual({
      answer: null,
      saves: 1,
    })
  })

  it('flush sends a changed title and resolves true once it is stored', async () => {
    const out = deferred<Answer>()
    const patch = vi.fn().mockReturnValueOnce(out.promise)
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    await typeInto(titleField(host)!, 'new')

    const now = watch((await flushFrom(handle)).answer)
    await allLanded()

    expect(patch.mock.calls, 'sent').toEqual([['w', { title: 'new' }]])
    expect(now(), 'while the save is out').toBe('out')

    out.settle({ title: 'new', persisted: true })
    await allLanded()

    expect(now(), 'once it is stored').toBe(true)
    expect(patch, 'once').toHaveBeenCalledTimes(1)
  })

  it('flush resolves false when the save fails, and the failure is under the field', async () => {
    const patch = vi.fn().mockRejectedValue(new Error('offline'))
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    await typeInto(titleField(host)!, 'new')

    const now = watch((await flushFrom(handle)).answer)
    await allLanded()

    expect(now()).toBe(false)
    expect(
      host.querySelector('form.metadata-editor [role="alert"]')?.textContent,
    ).toBe('offline')
  })

  it('flush resolves false when the title is stored but not written beside the library, the reason under the field, and answers null next', async () => {
    const patch = vi
      .fn()
      .mockResolvedValue({ title: 'new', persisted: false, why: ['disk full'] })
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    await typeInto(titleField(host)!, 'new')

    const now = watch((await flushFrom(handle)).answer)
    await allLanded()

    expect({
      answer: now(),
      reason: host.querySelector('form.metadata-editor .unwritten li')
        ?.textContent,
    }).toEqual({ answer: false, reason: 'disk full' })
    const { answer } = await flushFrom(handle)
    expect({ next: answer, saves: patch.mock.calls.length }).toEqual({
      next: null,
      saves: 1,
    })
  })

  it('flush waits for the save out and the newest text after it, sending each once', async () => {
    const first = deferred<Answer>()
    const second = deferred<Answer>()
    const patch = vi
      .fn()
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise)
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    await enterWith(host, 'A')
    await typeInto(titleField(host)!, 'AB')

    const now = watch((await flushFrom(handle)).answer)
    await allLanded()
    expect(patch.mock.calls, 'while the first is out').toEqual([
      ['w', { title: 'A' }],
    ])

    first.settle({ title: 'A', persisted: true })
    await allLanded()
    expect(patch.mock.calls, 'after the first').toEqual([
      ['w', { title: 'A' }],
      ['w', { title: 'AB' }],
    ])
    expect(now(), 'while the newest is out').toBe('out')

    second.settle({ title: 'AB', persisted: true })
    await allLanded()

    expect(now(), 'once the newest is stored').toBe(true)
    expect(patch, 'each once').toHaveBeenCalledTimes(2)
  })

  it('flush puts a blank title back and answers null', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: true })
    const handle = createRef<TitleFlush>()
    const host = mount(
      <MetadataEditor ref={handle} slug="w" title="old" onPatch={patch} />,
    )
    const field = titleField(host)!
    await typeInto(field, '   ')

    const { answer } = await flushFrom(handle)
    await allLanded()

    expect({
      answer,
      value: field.value,
      saves: patch.mock.calls.length,
    }).toEqual({ answer: null, value: 'old', saves: 0 })
  })
})

describe('MetadataEditor, the Title field’s watch on its own width (O283)', () => {
  /** jsdom has no `ResizeObserver`; this one records what it is asked. */
  class Watch {
    static made: Watch[] = []
    observed: Element[] = []
    disconnected = false
    constructor() {
      Watch.made.push(this)
    }
    observe(el: Element) {
      this.observed.push(el)
    }
    unobserve() {}
    disconnect() {
      this.disconnected = true
    }
  }

  beforeEach(() => {
    Watch.made = []
    vi.stubGlobal('ResizeObserver', Watch)
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  const watches = () =>
    Watch.made.map(({ observed, disconnected }) => ({ observed, disconnected }))

  it('watches its own field for a width change, and lets the watch go when the editor goes', () => {
    const host = mount(
      <MetadataEditor slug="w" title="old" onPatch={vi.fn()} />,
    )
    const field = titleField(host)
    expect(watches(), 'mounted').toEqual([
      { observed: [field], disconnected: false },
    ])

    rerender(<div />)
    expect(watches(), 'gone').toEqual([
      { observed: [field], disconnected: true },
    ])
  })
})

describe('MetadataEditor, the Title field fitted again as a face lands (O283)', () => {
  /** jsdom has no `document.fonts`; this one only carries its events. */
  beforeEach(() => {
    Object.defineProperty(document, 'fonts', {
      configurable: true,
      value: new EventTarget(),
    })
  })
  afterEach(() => {
    Reflect.deleteProperty(document, 'fonts')
    vi.unstubAllGlobals()
  })

  /** `field` laid out with 20 px lines, `content` px of title and a 1 px border. */
  const laidOut = (field: HTMLTextAreaElement, content: number) => {
    field.style.padding = '0px'
    Object.defineProperty(field, 'clientHeight', {
      configurable: true,
      value: 20,
    })
    Object.defineProperty(field, 'scrollHeight', {
      configurable: true,
      value: content,
    })
    vi.spyOn(field, 'getBoundingClientRect').mockReturnValue({
      height: 22,
    } as DOMRect)
  }

  it.each([
    ['without', false],
    ['with', true],
  ] as const)(
    'fits the field again as a face lands, and no longer once the editor is gone, %s a ResizeObserver',
    (_, observer) => {
      if (observer) {
        vi.stubGlobal(
          'ResizeObserver',
          class {
            observe() {}
            unobserve() {}
            disconnect() {}
          },
        )
      }
      const host = mount(
        <MetadataEditor slug="w" title="old" onPatch={vi.fn()} />,
      )
      const field = titleField(host)!
      laidOut(field, 60)
      // The fixture tells: the fit made as it mounted had nothing laid out.
      expect(field.style.height, 'as mounted').not.toBe('62px')

      document.fonts.dispatchEvent(new Event('loadingdone'))
      expect(field.style.height, 'a face landed').toBe('62px')

      rerender(<div />)
      laidOut(field, 80)
      document.fonts.dispatchEvent(new Event('loadingdone'))
      expect(field.style.height, 'a face landed after it went').toBe('62px')
    },
  )
})

describe('MetadataEditor, the ComicInfo stamp', () => {
  const noop = vi.fn().mockResolvedValue({ title: 't', persisted: true })

  /**
   * **The three sizes are chosen so no two of them format alike.** 512 MiB and
   * 1.5 GiB sum to exactly 2 GiB, so `humanBytes` draws `512.0 MiB`,
   * `1.5 GiB` and `2.0 GiB` — three distinct strings. A total that drew like
   * the largest archive's size would let substituting that size for the total
   * go unnoticed, so the total would not be pinned by the test that names it.
   */
  const PLAN = {
    archives: [
      {
        member_id: 7,
        edition_key: 'e1',
        path: '/lib/Series - v01.cbz',
        bytes: 512 * 1024 * 1024,
      },
      {
        member_id: 9,
        edition_key: 'e1',
        path: '/lib/Series - v02.cbz',
        bytes: 1536 * 1024 * 1024,
      },
    ],
    total_bytes: 2 * 1024 * 1024 * 1024,
    skipped_formats: [] as string[],
  }

  /** The same work after a third zip entered the plan — see the 409 below. */
  const WIDER_PLAN = {
    ...PLAN,
    archives: [
      ...PLAN.archives,
      {
        member_id: 11,
        edition_key: 'e1',
        path: '/lib/Series - v03.cbz',
        bytes: 256 * 1024 * 1024,
      },
    ],
  }

  /**
   * A work whose one rewritable archive sits in a *spoke* edition, which is
   * what makes `content_key` come back `null`: `identity::content_key` hashes
   * the hub edition's member digests alone.
   */
  const SPOKE_PLAN = {
    archives: [
      {
        member_id: 21,
        edition_key: 'e2',
        path: '/lib/Series - v01 (scan).cbz',
        bytes: 512 * 1024 * 1024,
      },
    ],
    total_bytes: 512 * 1024 * 1024,
    skipped_formats: [] as string[],
  }

  beforeEach(() => {
    vi.mocked(stamp.stampPlan).mockReset()
    vi.mocked(stamp.performStamp).mockReset()
    vi.mocked(derive.deriveStatus).mockReset()
    vi.mocked(derive.deriveStatus).mockResolvedValue({
      derive_running: false,
      job_held: false,
    })
  })

  it('has no section at all for an account without can_stamp', () => {
    const host = mount(<MetadataEditor slug="w" title="t" onPatch={noop} />)

    expect(planButton(host)).toBeNull()
    expect(host.querySelector('.stamp-comicinfo')).toBeNull()
    expect(host.textContent).not.toMatch(/ComicInfo/i)
  })

  it('offers one control and reads no plan until it is pressed', () => {
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    expect(planButton(host)).not.toBeNull()
    expect(stamp.stampPlan).not.toHaveBeenCalled()
  })

  it('does not save the title when the stamp control is pressed', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const patch = vi.fn().mockResolvedValue({ title: 't', persisted: true })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={patch} canStamp />,
    )

    await click(planButton(host)!)

    expect(patch).not.toHaveBeenCalled()
    expect(stamp.stampPlan).toHaveBeenCalledWith('w')
  })

  it('saves a changed title when focus leaves the field for the stamp controls', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const patch = vi
      .fn()
      .mockResolvedValue({ title: 'Volume 1', persisted: true })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={patch} canStamp />,
    )
    const field = titleField(host)!
    await focusOn(field)
    await typeInto(field, 'Volume 1')

    await focusOn(planButton(host)!)
    await drain()

    expect(patch.mock.calls).toEqual([['w', { title: 'Volume 1' }]])
    await click(planButton(host)!)
    expect(stamp.stampPlan).toHaveBeenCalledWith('w')
  })

  it('names every archive with its own size, one row each', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const rows = archiveRows(host)
    expect(rows).toHaveLength(2)
    // Per element: a page-wide `toContain` passes when v01's path and v02's
    // size are both somewhere on screen, which is the pairing it cannot see.
    expect(rows[0]?.textContent).toContain('/lib/Series - v01.cbz')
    expect(rows[0]?.textContent).toContain('512.0 MiB')
    expect(rows[1]?.textContent).toContain('/lib/Series - v02.cbz')
    expect(rows[1]?.textContent).toContain('1.5 GiB')
  })

  // O274: a path holds no space, so without these breaks a phone's narrow
  // tile can only break it where `overflow-wrap` runs out of line.
  it('draws each path with a break after each / and -, its text whole', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const rows = archiveRows(host)
    expect(rows).toHaveLength(PLAN.archives.length)
    PLAN.archives.forEach((archive, i) => {
      // The fixture tells: each path has places to break, so a path drawn
      // plain has fewer breaks than it should.
      expect(archive.path.match(/[/-]/g)).toHaveLength(3)
      const path = rows[i]?.querySelector('.path')
      expect(path?.textContent).toBe(archive.path)
      expect(path?.querySelectorAll('.path-break')).toHaveLength(3)
    })
  })

  it('states the count and the total before anything starts', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const total = host.querySelector('.stamp-comicinfo .total')
    expect(total?.textContent).toContain('2 archives')
    // The sum, and no archive of this fixture formats to it — see `PLAN`.
    expect(total?.textContent).toContain('2.0 GiB')
    expect(total?.textContent).not.toContain('1.5 GiB')
    expect(total?.textContent).not.toContain('512.0 MiB')
  })

  it('states the three consequences in the terms that bite', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const said = host.querySelector(
      '.stamp-comicinfo .consequences',
    )?.textContent
    expect(said).toMatch(/sha256/i)
    expect(said).toMatch(/stale/i)
    expect(said).toMatch(/thumbnail/i)
    expect(said).toMatch(/content_key/i)
  })

  it('says an interrupted run leaves the archives it already rewrote', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const said = host.querySelector(
      '.stamp-comicinfo .consequences',
    )?.textContent
    expect(said).toMatch(/interrupted/i)
    expect(said).toMatch(/no undo|cannot be undone|not kept/i)
  })

  it('names the formats it is leaving alone, so a short plan is not read as all of them', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      ...PLAN,
      skipped_formats: ['7z', 'rar'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const skipped = host.querySelector('.stamp-comicinfo .skipped')
    expect(skipped?.textContent).toContain('7z')
    expect(skipped?.textContent).toContain('rar')
  })

  it('says nothing about skipped formats when there are none', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    expect(host.querySelector('.stamp-comicinfo .skipped')).toBeNull()
  })

  it('reports a plan that could not be read, and lets it be asked again', async () => {
    vi.mocked(stamp.stampPlan).mockRejectedValue(
      new Error('/api/library/works/w/stamp-comicinfo: 403 forbidden'),
    )
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    expect(
      host.querySelector('.stamp-comicinfo [role="alert"]')?.textContent,
    ).toContain('403 forbidden')
    expect(planButton(host)).not.toBeNull()
    expect(planButton(host)?.disabled).toBe(false)
  })

  /**
   * The protection above rides entirely on structure — `.stamp-comicinfo`
   * sits as a sibling after `</form>`, not inside it — plus `type="button"`
   * as belt and braces. Without this test, a future change that moved the
   * section inside the form would go uncaught unless it also dropped
   * `type="button"`. A button with no `type` inside a form submits it, which
   * would make an irreversible archive rewrite a side effect of a title save.
   */
  it('does not render the stamp section inside the metadata form', () => {
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    expect(
      host.querySelector('form.metadata-editor .stamp-comicinfo'),
    ).toBeNull()
    expect(host.querySelector('.stamp-comicinfo')).not.toBeNull()
  })

  /**
   * **No digit of the count appears in the key.** A key such as `'k2'` would
   * satisfy a completion test's `toContain('2')` by itself, so replacing the
   * whole count phrase with a literal `Rewrote archives.` would leave the
   * number of archives actually rewritten unasserted after an
   * irreversible operation.
   */
  const DONE = {
    rewritten: 2,
    content_key: '9f0e7bd3ac5e1a6f',
    document_moved: true,
    document_rewritten: true,
    sidecar_rewritten: true,
  }

  /** The key moved and the document did not — O81's stranding case. */
  const LEFT_BEHIND = {
    rewritten: 2,
    content_key: '9f0e7bd3ac5e1a6f',
    document_moved: false,
    document_rewritten: false,
    sidecar_rewritten: false,
  }

  /** The work's identity did not move at all: `moved_key` was `None`. */
  const KEY_HELD = {
    rewritten: 1,
    content_key: null,
    document_moved: false,
    document_rewritten: false,
    sidecar_rewritten: false,
  }

  it('sends back exactly the archives the plan named', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockResolvedValue(DONE)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)
    await click(stampButton(host)!)

    // The array itself, not a summary: `stamp::apply` compares it field for
    // field and answers 409 on any difference.
    expect(stamp.performStamp).toHaveBeenCalledWith('w', PLAN.archives)
  })

  it('offers no confirm for an empty plan, and still names what it left alone', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [],
      total_bytes: 0,
      skipped_formats: ['rar'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    expect(stampButton(host)).toBeNull()
    expect(host.querySelector('.stamp-comicinfo .nothing')).not.toBeNull()
    // A `.skipped` note rendered inside the non-empty branch alone would leave
    // this work — every archive a `.cbr` — reading as "nothing to write", with
    // the rar never appearing. The server already tells a work with nothing
    // to write from one with no zip archive at all.
    expect(
      host.querySelector('.stamp-comicinfo .skipped')?.textContent,
    ).toContain('rar')
  })

  it('says nothing about skipped formats on an empty plan that has none', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [],
      total_bytes: 0,
      skipped_formats: [],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    expect(host.querySelector('.stamp-comicinfo .skipped')).toBeNull()
  })

  /**
   * **Both presses inside one `act`**: `disabled={busy}` has not reached the
   * DOM within one batch, so only the ref can stop the second, which is what
   * makes this test about the ref rather than about an attribute.
   */
  it('reads one plan for two presses dispatched before a render', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    const button = planButton(host)!

    await act(async () => {
      button.click()
      button.click()
      await Promise.resolve()
    })

    expect(stamp.stampPlan).toHaveBeenCalledTimes(1)
  })

  it('sends one stamp for two presses dispatched before a render', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockResolvedValue(DONE)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    await click(planButton(host)!)
    const button = stampButton(host)!

    // **Both presses inside one `act`**, which is what a double-click
    // dispatches: React batches the state update, so the confirm's
    // `disabled={phase.kind === 'writing'}` has not been applied to the DOM
    // when the second lands. Only the ref stops it.
    await act(async () => {
      button.click()
      button.click()
      await Promise.resolve()
    })

    expect(stamp.performStamp).toHaveBeenCalledTimes(1)
  })

  /** Drive a plan and a confirmation through to the completion paragraph. */
  const stampWith = async (plan: stamp.StampPlan, outcome: stamp.Stamped) => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(plan)
    vi.mocked(stamp.performStamp).mockResolvedValue(outcome)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    await click(planButton(host)!)
    await click(stampButton(host)!)
    return host
  }

  it('reports the key and the document together when both moved', async () => {
    const host = await stampWith(PLAN, DONE)

    const said = host.querySelector('.stamped .key-moved')?.textContent
    expect(said).toContain('9f0e7bd3ac5e1a6f')
    expect(said).toMatch(/moved with it/)
    expect(host.querySelector('.stamped .key-moved-alone')).toBeNull()
    expect(host.querySelector('.stamped .key-held')).toBeNull()
    expect(stampButton(host)).toBeNull()
  })

  /**
   * **The count, as a whole phrase.** The one number that says how many of a
   * person's files were irreversibly rewritten is compared as the whole
   * sentence, not by looking for a character in it: `toContain('2')` would be
   * satisfied by a `content_key` holding a `2` alone. The key in `DONE` holds
   * no digit of the count.
   */
  it('states exactly how many archives were rewritten', async () => {
    const host = await stampWith(PLAN, DONE)

    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
  })

  /**
   * **The one outcome a screen reader could miss.** A refusal carries
   * `role="alert"` and announces itself; the completion carries no role, so
   * drawn outside every live region it would leave a person, after agreeing
   * to a rewrite with no undo, hearing "Rewriting 2 archives — the originals
   * are being replaced now", then that paragraph removed (silent under the
   * default `aria-relevant`), then silence. The completion is
   * inside the region.
   *
   * **And the region itself is pinned.** `AcceptForm` — the incumbent this
   * section's wrapper and its comment were both copied from — carries
   * `expect(why.getAttribute('aria-live')).toBe('polite')`, and so does
   * this test.
   *
   * The identity check is the other half: a region has to exist *before* what
   * it announces is put into it, so this asserts the node the completion lands
   * inside is the same node that was on screen before anything was asked —
   * which a `role="status"` on `.stamped` itself, the alternative,
   * could not be.
   */
  it('announces the completion, inside the region that was there before it', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockResolvedValue(DONE)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    const region = host.querySelector('.stamp-comicinfo .stamp-state')
    expect(region).not.toBeNull()
    expect(region?.getAttribute('aria-live')).toBe('polite')

    await click(planButton(host)!)
    await click(stampButton(host)!)

    expect(host.querySelector('.stamp-comicinfo .stamp-state')).toBe(region)
    const rewrote = host.querySelector('.stamped .rewrote')
    expect(rewrote?.textContent).toBe('Rewrote 2 archives.')
    expect(region?.contains(rewrote!)).toBe(true)
  })

  it('says the identity did not move when the key came back null, rather than printing an empty one', async () => {
    const host = await stampWith(SPOKE_PLAN, KEY_HELD)

    // `content_key` is `Option<String>` in the store and serialised straight
    // through, so `null` would reach a field typed `string` and the screen
    // would read "This work's content_key is now . It had no document on disk
    // to move." — an empty key, and a claim about a document nothing
    // had checked.
    const stamped = host.querySelector('.stamped')
    expect(stamped?.textContent).not.toMatch(/content_key is now/)
    expect(host.querySelector('.stamped .key-held')?.textContent).toMatch(
      /content_key did not change/,
    )
    expect(host.querySelector('.stamped .key-moved')).toBeNull()
    expect(host.querySelector('.stamped .key-moved-alone')).toBeNull()
    // The singular, on the one-archive plan this outcome comes from.
    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 1 archive.',
    )
  })

  /**
   * O81's case. `stamp::apply` hard-codes `document_moved: false` when the
   * key did not move, and for a stamp `hand::rename_from` answers `false`
   * for five causes — two of which leave the document stranded at the old
   * key. The screen must name the uncertainty rather than either reading of
   * it.
   */
  it('warns that a document may be stranded when the key moved and it did not', async () => {
    const host = await stampWith(PLAN, LEFT_BEHIND)

    const said = host.querySelector('.stamped .key-moved-alone')?.textContent
    expect(said).toContain('9f0e7bd3ac5e1a6f')
    expect(said).toMatch(/did not move with it/)
    expect(said).toMatch(/stranded/i)
    expect(said).toMatch(/orphan/i)
    // Neither reading stated as certain: not "there was no document", and not
    // "your document is lost".
    expect(said).toMatch(/either there was no document/i)
    expect(host.querySelector('.stamped .key-moved')).toBeNull()
    expect(host.querySelector('.stamped .key-held')).toBeNull()
  })

  it('says when the sidecar was rewritten alongside the archives, and nothing when it was not', async () => {
    const withOne = await stampWith(PLAN, DONE)
    expect(withOne.querySelector('.stamped .sidecar')?.textContent).toMatch(
      /taiyaku\.json/,
    )

    const without = await stampWith(SPOKE_PLAN, KEY_HELD)
    expect(without.querySelector('.stamped .sidecar')).toBeNull()
  })

  it('says when the hand document was rewritten alongside the archives, and nothing when it was not', async () => {
    const withOne = await stampWith(PLAN, DONE)
    expect(withOne.querySelector('.stamped .document')?.textContent).toBe(
      "This work's <content_key>.library.json was rewritten too, so it records the new digests.",
    )

    const without = await stampWith(SPOKE_PLAN, KEY_HELD)
    expect(without.querySelector('.stamped .document')).toBeNull()
  })

  it('surfaces a 409 from the stamp as an alert, and keeps the plan on screen', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockRejectedValue(
      new Error(
        '/api/library/works/w/stamp-comicinfo: 409 a scan or another library job is running; stamp again once it has finished',
      ),
    )
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)
    await click(stampButton(host)!)

    expect(
      host.querySelector('.stamp-comicinfo [role="alert"]')?.textContent,
    ).toContain('409 a scan or another library job')
    // The plan is still there: a 409 is "the world moved", and re-reading it
    // is the person's next move rather than something this screen guesses at.
    expect(archiveRows(host)).toHaveLength(2)
  })

  /**
   * **Reachable without any mistake.** Open the panel, read the plan, correct
   * the title in the form above and leave it — the rendered XML now differs,
   * so a third zip enters the plan — then confirm. The server answers "this
   * work's archives have changed since the plan was made (2 then, 3 now);
   * nothing was written". If the only enabled control resent the same body,
   * the same guard would refuse it every time, and closing the whole panel
   * would be the only escape.
   */
  it('offers a fresh plan after a refusal, and draws the new one over the old', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockRejectedValue(
      new Error(
        "/api/library/works/w/stamp-comicinfo: 409 this work's archives have changed since the plan was made (2 then, 3 now); nothing was written",
      ),
    )
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)
    await click(stampButton(host)!)
    // The way back is the one read control, enabled again now that nothing is
    // on the wire — not a second control that only a refusal draws.
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(false)

    vi.mocked(stamp.stampPlan).mockResolvedValue(WIDER_PLAN)
    await click(planButton(host)!)

    expect(stamp.stampPlan).toHaveBeenCalledTimes(2)
    expect(archiveRows(host)).toHaveLength(3)
    expect(stampButton(host)?.textContent).toBe('Rewrite 3 archives')
    // The refusal was about the plan that is now gone, so it goes with it.
    expect(host.querySelector('.stamp-comicinfo [role="alert"]')).toBeNull()
    // Still one control, and still the same label: the phase changed under it
    // and the way to ask again did not.
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.textContent).toBe('What would this change?')
  })

  /**
   * The same ref, on the press of the re-read control, doubled inside one
   * batch, in the `refused` phase — where the read control carries
   * `disabled={false}`, so the attribute cannot be what stops the second and
   * only the ref can.
   */
  it('reads one plan for two presses of the re-read control before a render', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockRejectedValue(new Error('409 refused'))
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    await click(planButton(host)!)
    await click(stampButton(host)!)
    const again = planButton(host)!
    expect(again.disabled).toBe(false)

    await act(async () => {
      again.click()
      again.click()
      await Promise.resolve()
    })

    expect(stamp.stampPlan).toHaveBeenCalledTimes(2)
  })

  /**
   * Drive a refusal, so the confirm sits beside the read control with an alert
   * over a plan a retry would resend.
   *
   * **Not the only state where both controls are live** — the read control
   * renders in all eight phases, and `ready` has both too. What a refusal adds
   * is the alert and the plan the confirm would send again, which is what the
   * tests below press against.
   */
  const refused = async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockRejectedValue(
      new Error(
        "/api/library/works/w/stamp-comicinfo: 409 this work's archives have changed since the plan was made (2 then, 3 now); nothing was written",
      ),
    )
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    await click(planButton(host)!)
    await click(stampButton(host)!)
    return host
  }

  /**
   * **A plan read under a running POST answers about a library that is
   * neither the one confirmed nor the one that follows**, so `askPlan`
   * refuses while `inFlight` is held. Both controls are in the DOM after a
   * refusal and React batches state, so this is one double-click away rather
   * than a contrivance.
   */
  it('refuses a re-read while a stamp is on the wire', async () => {
    const host = await refused()
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)
    const confirm = stampButton(host)!
    const again = planButton(host)!

    await act(async () => {
      confirm.click()
      again.click()
      await Promise.resolve()
    })

    // One read, from the press that opened the panel — the re-read did not go.
    expect(stamp.stampPlan).toHaveBeenCalledTimes(1)
    expect(stamp.performStamp).toHaveBeenCalledTimes(2)
    expect(askingLine(host)).toBeNull()
    // The ref refuses the press, so the attribute says so: otherwise the
    // control would sit enabled and inert for the length of a rewrite over a
    // NAS. Every other ref-guarded control in this tree pairs the ref with
    // the attribute.
    expect(planButton(host)?.disabled).toBe(true)

    posted.settle(DONE)
    await drain()

    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
  })

  /**
   * **The other order, which nothing blocks and nothing should.** Press the
   * re-read first and the confirm second inside one batch: `askPlan` runs
   * with no stamp in flight, blanks the plan and starts a GET, and
   * `confirmStamp` then posts the plan its closure still holds — which is
   * exactly the plan the person was looking at, and which `stamp::apply`
   * checks field for field before writing anything.
   *
   * So the archives are rewritten with no plan on screen, and the answer that
   * lands for that read is empty — the likely answer, since the stamp just
   * wrote every one of them. Two things have to hold at once here. A
   * completion rendered inside `plan.archives.length > 0` would have the
   * count, the content_key and the whole O81 stranded-document paragraph
   * replaced by "Nothing to write", which is the opposite of what had just
   * happened to the person's files. And that empty answer must not draw at
   * all: its GET went out before the POST, so it describes a library the
   * write was in the middle of moving.
   */
  it('reports what a stamp did even when the plan was cleared under it', async () => {
    const host = await refused()
    const replanned = deferred<stamp.StampPlan>()
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.stampPlan).mockReturnValue(replanned.promise)
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)
    const confirm = stampButton(host)!
    const again = planButton(host)!

    await act(async () => {
      again.click()
      confirm.click()
      await Promise.resolve()
    })

    // Twice: the refusal that opened the re-read, and this press. The plan
    // its closure holds is the one on screen when it was pressed.
    expect(stamp.performStamp).toHaveBeenCalledTimes(2)
    expect(stamp.performStamp).toHaveBeenLastCalledWith('w', PLAN.archives)

    posted.settle(DONE)
    replanned.settle({ archives: [], total_bytes: 0, skipped_formats: [] })
    await drain()

    // **The read that raced the stamp is discarded, not drawn.**
    // `confirmStamp` bumps `latestPlan`, so `settlePlan` drops an answer to a
    // read the stamp overtook. What is left is what actually happened.
    expect(host.querySelector('.stamp-comicinfo .nothing')).toBeNull()
    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
    expect(host.querySelector('.stamped .key-moved')?.textContent).toContain(
      '9f0e7bd3ac5e1a6f',
    )
  })

  /**
   * **The supersede block, driven past the frame the others stop at.**
   * `confirmStamp` releases `planInFlight` and clears `asking` on behalf of
   * the read it overtakes, because `settlePlan` returns on the sequence
   * mismatch *before* it reaches either line — so for a superseded read those
   * two statements are the only thing that ever releases them. The tests that
   * press both controls in one batch stop at the frame, and none uses the
   * section again afterwards.
   *
   * Without either line the section is left in a dead end:
   *
   * - without `planInFlight.current = false`, `askPlan`'s guard refuses every
   *   press for the life of the mount while `busy` is false — the read control
   *   enabled and permanently inert;
   * - without `setAsking(false)`, `asking` never clears, so once the write
   *   clears the plan slot the phase parks in `reading` for good: "Working out
   *   which archives would change…" on screen, the read control disabled, and
   *   nothing on the wire.
   *
   * Both are dead ends with no way out but a remount, which is why this
   * asserts a press that *lands* rather than a control that merely looks
   * enabled: the first leaves it enabled.
   */
  it('reads again after a stamp superseded the read it overtook', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    await click(planButton(host)!)

    const superseded = deferred<stamp.StampPlan>()
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.stampPlan).mockReturnValue(superseded.promise)
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)
    const again = planButton(host)!
    const confirm = stampButton(host)!

    await act(async () => {
      again.click()
      confirm.click()
      await Promise.resolve()
    })

    expect(stamp.stampPlan).toHaveBeenCalledTimes(2)
    expect(stamp.performStamp).toHaveBeenCalledTimes(1)

    // Both settle: the GET the stamp overtook — discarded, since its answer
    // describes a library the write was in the middle of moving — and then
    // the write itself.
    superseded.settle({ archives: [], total_bytes: 0, skipped_formats: [] })
    await drain()
    posted.settle(DONE)
    await drain()

    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
    expect(askingLine(host)).toBeNull()
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(false)

    // The press has to reach `stampPlan`, not merely find a live-looking
    // button: a held `planInFlight` refuses it with the attribute unset.
    vi.mocked(stamp.stampPlan).mockResolvedValue(WIDER_PLAN)
    await click(planButton(host)!)

    expect(stamp.stampPlan).toHaveBeenCalledTimes(3)
    expect(archiveRows(host)).toHaveLength(3)
    expect(stampButton(host)?.textContent).toBe('Rewrite 3 archives')
  })

  /**
   * **The frame between the press and the answer.** Deleting
   * `setPlan(undefined)` from `askPlan` would go unseen by a test that awaits
   * the answer that overwrites it. The live `Rewrite 2 archives` button below
   * is what that line actually protects: it sends the plan the server has
   * already refused. `phaseOf` tests the plan slot before `asking` so that
   * *this* test is the one that sees the deletion: with the order reversed,
   * the stale plan would hide behind the reading line here and only `draws one
   * read control and one alert when the read and the write both fail` would
   * still catch it.
   */
  it('blanks the refused plan and its confirm control while the re-read is out', async () => {
    const host = await refused()
    const replanned = deferred<stamp.StampPlan>()
    vi.mocked(stamp.stampPlan).mockReturnValue(replanned.promise)

    await click(planButton(host)!)

    expect(askingLine(host)).not.toBeNull()
    expect(archiveRows(host)).toHaveLength(0)
    expect(stampButton(host)).toBeNull()
    // The refusal was about the plan now being re-read, so it goes at the
    // press rather than at the answer.
    expect(host.querySelector('.stamp-comicinfo [role="alert"]')).toBeNull()
    // The control stays on screen and goes disabled, rather than vanishing:
    // the ref that refuses a second read has the attribute beside it.
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(true)

    replanned.settle(WIDER_PLAN)
    await drain()

    expect(askingLine(host)).toBeNull()
    expect(stampButton(host)?.textContent).toBe('Rewrite 3 archives')
  })

  /**
   * The same frame on the other read, and the other line: deleting
   * `setPlanFailure(undefined)` from `askPlan` is seen only by a test that
   * fails a plan read and then asks again.
   */
  it('blanks a refused plan read while the next one is out', async () => {
    vi.mocked(stamp.stampPlan).mockRejectedValue(
      new Error('/api/library/works/w/stamp-comicinfo: 403 forbidden'),
    )
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )
    await click(planButton(host)!)
    const retried = deferred<stamp.StampPlan>()
    vi.mocked(stamp.stampPlan).mockReturnValue(retried.promise)

    await click(planButton(host)!)

    expect(host.querySelector('.stamp-comicinfo [role="alert"]')).toBeNull()
    expect(askingLine(host)).not.toBeNull()

    retried.settle(PLAN)
    await drain()

    expect(host.querySelector('.stamp-comicinfo [role="alert"]')).toBeNull()
    expect(archiveRows(host)).toHaveLength(2)
  })

  /**
   * **The forecast is no stronger than what the outcome can report.** A
   * forecast reading "and its <content_key>.library.json moves with it when
   * it does" would contradict `key-moved-alone` — on screen at the same time,
   * since the plan does not unmount on success — which has to say the document
   * may be stranded instead. For a stamp, `hand::rename_from` answers
   * `false` for five causes, two of which strand it, so the certainty is not
   * available to promise.
   */
  it('does not promise the document move before the person agrees to it', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const said = host.querySelector(
      '.stamp-comicinfo .consequences',
    )?.textContent
    expect(said).toMatch(/attempted, not promised/i)
    expect(said).toMatch(/stranded at the old key/i)
    // The promise itself, in either tense, before anything has happened.
    expect(said).not.toMatch(/moves with it/i)
  })

  /**
   * **`Format::is_readable` is `Dir | Zip` alone.** A note reading "which
   * taiyaku reads and never writes" would be false for every token `stamp.rs`
   * can print but `dir` — `pages.rs`'s `unsupported_container` refuses rar,
   * 7z, tar and PDF outright. What is true of all of them is the write rule.
   */
  it('states the write rule rather than claiming taiyaku reads what it skips', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      ...PLAN,
      skipped_formats: ['dir', 'rar'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const said = host.querySelector(
      '.stamp-comicinfo .skipped .formats',
    )?.textContent
    expect(said).toContain('dir')
    expect(said).toContain('rar')
    // The rule, in the terms `stamp::plan` actually applies: it filters on
    // what `sniff` identified, `Format::Zip` and nothing else.
    expect(said).toMatch(/identifies as a plain zip, and nothing else/i)
    expect(said).not.toMatch(/taiyaku reads/i)
  })

  /**
   * `unreadable` is not a format: `skipped_formats` prints it for a member
   * `taiyaku_read::sniff` could not open at all. Listing it beside `rar` would
   * read "this work holds unreadable, which taiyaku reads and never writes" —
   * a sentence that contradicts itself twice over.
   */
  it('gives a file nothing could open its own sentence, not a place in the format list', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [],
      total_bytes: 0,
      skipped_formats: ['unreadable'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const skipped = host.querySelector('.stamp-comicinfo .skipped')
    expect(skipped?.querySelector('.formats')).toBeNull()
    expect(skipped?.textContent).not.toMatch(/holds unreadable/i)
    expect(skipped?.querySelector('.unopened')?.textContent).toMatch(
      /could not be opened at all/i,
    )
  })

  /**
   * `copied` is not a format either: a file held in more than one directory,
   * which a stamp leaves alone so its copies stay one book.
   */
  it('gives a file held in two directories its own sentence, not a place in the format list', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [],
      total_bytes: 0,
      skipped_formats: ['copied'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const skipped = host.querySelector('.stamp-comicinfo .skipped')
    expect(skipped?.querySelector('.formats')).toBeNull()
    expect(skipped?.textContent).not.toMatch(/holds copied/i)
    expect(skipped?.querySelector('.copied')?.textContent).toMatch(
      /held in more than one directory/i,
    )
  })

  /**
   * `unknown` is the other non-format, and it means something different: the
   * file was read and matched no signature `taiyaku_read` carries. Same
   * treatment, its own sentence.
   */
  it('gives a file matching no known format its own sentence', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      ...PLAN,
      skipped_formats: ['rar', 'unknown'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const skipped = host.querySelector('.stamp-comicinfo .skipped')
    expect(skipped?.querySelector('.formats')?.textContent).toContain('rar')
    expect(skipped?.querySelector('.formats')?.textContent).not.toContain(
      'unknown',
    )
    expect(skipped?.querySelector('.unrecognised')?.textContent).toMatch(
      /matches no format taiyaku recognises/i,
    )
  })

  /**
   * `confirmStamp`'s `setStampFailure(undefined)` clears the 409 that the
   * retry is answering.
   */
  it('clears the refusal while the stamp answering it is on the wire', async () => {
    const host = await refused()
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)

    await click(stampButton(host)!)

    expect(host.querySelector('.stamp-comicinfo [role="alert"]')).toBeNull()
    expect(planButton(host)?.disabled).toBe(true)
    expect(stampButton(host)?.textContent).toBe('Rewriting…')

    posted.settle(DONE)
    await drain()

    expect(host.querySelector('.stamp-comicinfo [role="alert"]')).toBeNull()
    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
  })
  /**
   * **One test per phase from here down.** The section's eight phases are
   * computed in one place (`phaseOf`), and what each one draws is a property
   * of the phase rather than of booleans read separately by each gate.
   *
   * `idle`: nothing asked, one control, nothing said.
   */
  it('draws one read control and says nothing before anything is asked', () => {
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(false)
    expect(stampButton(host)).toBeNull()
    expect(
      host.querySelector('.stamp-comicinfo .stamp-state')?.textContent,
    ).toBe('')
  })

  /**
   * `reading`: the ref that refuses a second read has the attribute beside it,
   * so the control says it is busy rather than sitting enabled and inert.
   */
  it('disables the read control while a plan read is out', async () => {
    const pending = deferred<stamp.StampPlan>()
    vi.mocked(stamp.stampPlan).mockReturnValue(pending.promise)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    expect(askingLine(host)).not.toBeNull()
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(true)

    pending.settle(PLAN)
    await drain()

    expect(planButton(host)?.disabled).toBe(false)
  })

  /**
   * **A running rewrite is a phase of its own**, and the phase carries the
   * plan being written rather than reading the slot. Drawn only inside the
   * confirm control's own label, which is gated on the plan slot, the one
   * sentence saying an irreversible rewrite is running could not be drawn in
   * two of the states it runs in.
   */
  it('says at section level that a rewrite is running', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)
    await click(stampButton(host)!)

    const running = host.querySelector('.stamp-comicinfo .writing')
    expect(running?.textContent).toContain('2 archives')
    expect(running?.textContent).toMatch(/nothing here can stop it/i)
    // Not the read's sentence, and not an empty plan's.
    expect(askingLine(host)).toBeNull()
    expect(host.querySelector('.stamp-comicinfo .nothing')).toBeNull()

    posted.settle(DONE)
    await drain()

    expect(host.querySelector('.stamp-comicinfo .writing')).toBeNull()
  })

  /** Neither control is live during the write. */
  it('disables both controls while a rewrite is on the wire', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)
    await click(stampButton(host)!)

    expect(planButton(host)?.disabled).toBe(true)
    expect(stampButton(host)?.disabled).toBe(true)
    expect(stampButton(host)?.textContent).toBe('Rewriting…')

    posted.settle(DONE)
    await drain()

    expect(planButton(host)?.disabled).toBe(false)
  })

  /**
   * The same sentence, with the plan slot blanked. A re-read pressed in the
   * same batch as the confirm blanks the plan slot; the answer it gets back is
   * empty — the likely one, since the stamp is rewriting every archive — and
   * drawing it would show "Nothing to write: every zip archive in this work
   * already holds the ComicInfo.xml taiyaku would write" over files being
   * rewritten at that moment. The answer is discarded (its GET went out before
   * the POST) and the phase says what is happening whatever the slot holds.
   */
  it('says a rewrite is running with the plan slot blanked and an empty answer landed', async () => {
    const host = await refused()
    const replanned = deferred<stamp.StampPlan>()
    const posted = deferred<stamp.Stamped>()
    vi.mocked(stamp.stampPlan).mockReturnValue(replanned.promise)
    vi.mocked(stamp.performStamp).mockReturnValue(posted.promise)
    const confirm = stampButton(host)!
    const again = planButton(host)!

    await act(async () => {
      again.click()
      confirm.click()
      await Promise.resolve()
    })

    replanned.settle({ archives: [], total_bytes: 0, skipped_formats: [] })
    await drain()

    const running = host.querySelector('.stamp-comicinfo .writing')
    expect(running?.textContent).toContain('2 archives')
    expect(host.querySelector('.stamp-comicinfo .nothing')).toBeNull()
    expect(askingLine(host)).toBeNull()
    // The archives on screen are the ones being written, from the phase.
    expect(archiveRows(host)).toHaveLength(2)

    posted.settle(DONE)
    await drain()

    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
  })

  /**
   * `done`. The plan that was applied is spent — every archive it named now
   * holds the document taiyaku would write, so `stamp::apply` answers 409 for
   * it — and clearing it is what makes "does this plan predate the rewrite?"
   * a question with a mechanical answer.
   */
  it('offers a read and no confirm once a rewrite has landed', async () => {
    const host = await stampWith(PLAN, DONE)

    expect(stampButton(host)).toBeNull()
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(false)
    expect(archiveRows(host)).toHaveLength(0)
    expect(host.querySelector('.stamped .rewrote')).not.toBeNull()
  })

  /**
   * **The dead end.** A plan read strictly after a rewrite, by an explicit
   * press, describes archives that still need stamping — a work whose title
   * was corrected between the two, or one the confirm's own 409 sent the
   * person back to re-read. A confirm control gated on `stamped === undefined`
   * would leave this phase drawing the total, the archive list, the
   * consequences and the irreversible warning with no confirm — and
   * `stamped` is never cleared by a read, so re-reading could not recover.
   */
  it('offers a confirm for a plan read after the rewrite, and keeps the report above it', async () => {
    const host = await stampWith(PLAN, DONE)

    vi.mocked(stamp.stampPlan).mockResolvedValue(WIDER_PLAN)
    await click(planButton(host)!)

    expect(archiveRows(host)).toHaveLength(3)
    expect(stampButton(host)?.textContent).toBe('Rewrite 3 archives')
    expect(stampButton(host)?.disabled).toBe(false)
    // The rewrite that already happened is still on screen: what a write with
    // no undo did is not conditional on a plan read afterwards.
    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
  })

  /**
   * Press the re-read and the confirm in one batch and let both requests
   * fail. There is one read control, and the two failures cannot coexist at
   * all — a stamp supersedes the read it overtook, so that GET's rejection is
   * discarded rather than drawn beside the POST's.
   */
  it('draws one read control and one alert when the read and the write both fail', async () => {
    const host = await refused()
    vi.mocked(stamp.stampPlan).mockRejectedValue(
      new Error('/api/library/works/w/stamp-comicinfo: 403 forbidden'),
    )
    vi.mocked(stamp.performStamp).mockRejectedValue(
      new Error('/api/library/works/w/stamp-comicinfo: 409 refused again'),
    )
    const confirm = stampButton(host)!
    const again = planButton(host)!

    await act(async () => {
      again.click()
      confirm.click()
      await Promise.resolve()
    })
    await drain()

    const alerts = [...host.querySelectorAll('.stamp-comicinfo [role="alert"]')]
    expect(alerts).toHaveLength(1)
    expect(alerts[0]?.textContent).toContain('409 refused again')
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(false)
    // Nothing to retry: the re-read blanked the slot, so there is no plan the
    // person is looking at for a confirm to send.
    expect(stampButton(host)).toBeNull()
  })

  /**
   * **The sentence offers no "or this work has no zip archive at all".** It
   * can sit above "Rewrote 2 archives." and above a 409 reading "this work's
   * archives have changed since the plan was made" — the disjunct is untrue in
   * both. Nothing is lost by leaving it out: "every zip archive already holds
   * it" is true of a work with none, and `.skipped` says what the work
   * holds instead.
   */
  it('does not offer "no zip archive at all" as a reason nothing needs writing', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [],
      total_bytes: 0,
      skipped_formats: [],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const said = host.querySelector('.stamp-comicinfo .nothing')?.textContent
    expect(said).toMatch(/already holds the ComicInfo.xml taiyaku would write/i)
    expect(said).not.toMatch(/no zip archive at all/i)
  })

  /**
   * **An EPUB *is* a zip container**, so `epub-fixed` in the skipped list
   * beside "writes ComicInfo.xml into zip archives and nothing else" would be
   * a sentence a reader cannot reconcile. What taiyaku filters on is the
   * identification `sniff` made — `stamp::plan` skips every member whose
   * format is not `Format::Zip`, and an EPUB is `EpubFixed` or `EpubReflow` —
   * so that is what the copy states, and the container is named where it would
   * otherwise contradict it.
   */
  it('says why an EPUB is skipped when it is a zip container itself', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      ...PLAN,
      skipped_formats: ['epub-fixed', 'rar'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const skipped = host.querySelector('.stamp-comicinfo .skipped')
    expect(skipped?.querySelector('.formats')?.textContent).toContain(
      'epub-fixed',
    )
    const containers = skipped?.querySelector('.containers')?.textContent
    expect(containers).toMatch(/zip container/i)
    expect(containers).toMatch(/EPUB/)
  })

  /** And it is not said about a work whose skipped formats hold no EPUB. */
  it('says nothing about containers when nothing skipped is one', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      ...PLAN,
      skipped_formats: ['rar'],
    })
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    expect(host.querySelector('.stamp-comicinfo .skipped')).not.toBeNull()
    expect(host.querySelector('.stamp-comicinfo .containers')).toBeNull()
  })
  /**
   * **The report belongs to one rewrite, and the press that starts another
   * takes it down.** Two completions cannot be told apart on screen: leaving
   * the first would put "Rewrote 2 archives." above a refusal of a different
   * write with nothing to say which was which — and the archives named in
   * that refusal are exactly the ones a reader would take it to be about.
   * The alternative, keeping it and labelling the two, needs a clock this
   * section does not have.
   */
  it('takes the previous rewrite report down when a second rewrite is pressed', async () => {
    const host = await stampWith(PLAN, DONE)
    expect(host.querySelector('.stamped .rewrote')).not.toBeNull()

    vi.mocked(stamp.stampPlan).mockResolvedValue(WIDER_PLAN)
    await click(planButton(host)!)
    vi.mocked(stamp.performStamp).mockRejectedValue(
      new Error('/api/library/works/w/stamp-comicinfo: 409 refused'),
    )
    await click(stampButton(host)!)

    expect(host.querySelector('.stamped')).toBeNull()
    expect(
      host.querySelector('.stamp-comicinfo [role="alert"]')?.textContent,
    ).toContain('409 refused')
  })
  /**
   * **The report survives whatever the plan slot ends up holding**, and this
   * is the ordinary way that is tested: read again after a rewrite and the
   * answer is empty, because the stamp just wrote every archive it named.
   * Rendered inside `plan.archives.length > 0`, the completion would have that
   * read replace the count, the content_key and the whole O81
   * stranded-document paragraph with "Nothing to write".
   */
  it('keeps the rewrite report under an empty plan read after it', async () => {
    const host = await stampWith(PLAN, DONE)

    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [],
      total_bytes: 0,
      skipped_formats: [],
    })
    await click(planButton(host)!)

    expect(host.querySelector('.stamp-comicinfo .nothing')).not.toBeNull()
    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
    expect(host.querySelector('.stamped .key-moved')?.textContent).toContain(
      '9f0e7bd3ac5e1a6f',
    )
    expect(stampButton(host)).toBeNull()
  })

  describe('held while a derive or the job lock holds (O51)', () => {
    const JOB_WAITS = 'Waits for a scan or another library job to finish.'
    beforeEach(() => {
      vi.useFakeTimers({ shouldAdvanceTime: true })
    })
    afterEach(() => {
      vi.useRealTimers()
    })

    /** What the confirm's `aria-describedby` names, or nothing. */
    const stampWaits = (host: HTMLElement) =>
      document.getElementById(
        stampButton(host)?.getAttribute('aria-describedby') ?? '',
      )?.textContent

    it.each([
      { holder: 'a derive', status: { derive_running: true, job_held: false } },
      {
        holder: 'the job lock',
        status: { derive_running: false, job_held: true },
      },
    ])(
      'holds the confirm while $holder holds, says why, sends nothing for a press, and frees it once a poll finds neither',
      async ({ status }) => {
        vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
        vi.mocked(stamp.performStamp).mockResolvedValue({
          rewritten: 2,
          content_key: null,
          document_moved: false,
          document_rewritten: false,
          sidecar_rewritten: false,
        })
        vi.mocked(derive.deriveStatus).mockResolvedValue(status)
        const host = mount(
          <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
        )
        expect(derive.deriveStatus).not.toHaveBeenCalled()
        await click(planButton(host)!)
        await drain()
        expect(derive.deriveStatus).toHaveBeenCalledTimes(1)
        expect(holdOf(stampButton(host)!)).toBe('held')
        expect(stampWaits(host)).toBe(JOB_WAITS)
        await click(stampButton(host)!)
        expect(stamp.performStamp).not.toHaveBeenCalled()

        vi.mocked(derive.deriveStatus).mockResolvedValue({
          derive_running: false,
          job_held: false,
        })
        await tick()
        await drain()
        expect(derive.deriveStatus).toHaveBeenCalledTimes(2)
        expect(holdOf(stampButton(host)!)).toBe('free')
        expect(stampWaits(host)).toBeUndefined()
        await click(stampButton(host)!)
        expect(stamp.performStamp).toHaveBeenCalledTimes(1)
      },
    )

    it('holds nothing for its own stamp, found by the read sent as its plan could first be confirmed, and holds the confirm from the read after the stamp failed', async () => {
      vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
      let fail!: (why: Error) => void
      vi.mocked(stamp.performStamp).mockReturnValueOnce(
        new Promise<stamp.Stamped>((_, reject) => {
          fail = reject
        }),
      )
      const first = deferred<derive.DeriveStatus>()
      vi.mocked(derive.deriveStatus).mockReturnValueOnce(first.promise)
      const host = mount(
        <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
      )
      await click(planButton(host)!)
      await drain()
      expect(derive.deriveStatus).toHaveBeenCalledTimes(1)
      await click(stampButton(host)!)
      expect(stamp.performStamp).toHaveBeenCalledTimes(1)

      // The server counts the stamp's own job lock and lease.
      first.settle({ derive_running: true, job_held: true })
      await drain()
      await act(async () => {
        fail(
          new Error(
            '/api/library/works/w/stamp-comicinfo: 500 the archive could not be written',
          ),
        )
      })
      await drain()
      expect(holdOf(stampButton(host)!)).toBe('free')
      expect(stampWaits(host)).toBeUndefined()

      // Sent with nothing of the page's out, this read's job is another's.
      vi.mocked(derive.deriveStatus).mockResolvedValue({
        derive_running: false,
        job_held: true,
      })
      await tick()
      await drain()
      expect(derive.deriveStatus).toHaveBeenCalledTimes(2)
      expect(holdOf(stampButton(host)!)).toBe('held')
      expect(stampWaits(host)).toBe(JOB_WAITS)
    })

    it('reads the status again when a stamp is refused for a hold, and holds the confirm from that read, saying why', async () => {
      vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
      vi.mocked(stamp.performStamp).mockRejectedValue(
        new Error(
          '/api/library/works/w/stamp-comicinfo: 409 a scan or another library job is running; stamp again once it has finished',
        ),
      )
      const host = mount(
        <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
      )
      await click(planButton(host)!)
      await drain()
      expect(derive.deriveStatus).toHaveBeenCalledTimes(1)
      expect(holdOf(stampButton(host)!)).toBe('free')

      vi.mocked(derive.deriveStatus).mockResolvedValue({
        derive_running: false,
        job_held: true,
      })
      await click(stampButton(host)!)
      await drain()
      expect(stamp.performStamp).toHaveBeenCalledTimes(1)
      expect(derive.deriveStatus).toHaveBeenCalledTimes(2)
      expect(holdOf(stampButton(host)!)).toBe('held')
      expect(stampWaits(host)).toBe(JOB_WAITS)
    })
  })
})
