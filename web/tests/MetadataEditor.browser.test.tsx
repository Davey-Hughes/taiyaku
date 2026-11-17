import latinFace from '@fontsource/zen-kaku-gothic-new/files/zen-kaku-gothic-new-latin-400-normal.woff2?url'
import { act, type ReactNode } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
// Required, the same reason every other file in this project gives: the
// rules under test live in `styles/`, whose sheets reach the app through
// `index.css`. Without this the boxes below measure browser defaults.
import '../src/index.css'
import { SCAN_POLL_MS } from '../src/config'
import * as derive from '../src/libraries'
import { MetadataEditor } from '../src/MetadataEditor'
import * as stamp from '../src/stamp'
import { deferred, settleUntil } from './support/async'
import { pressOn } from './support/callout'
import { tokenColour } from './support/colour'
import { holdOf } from './support/held'
import { lineCount, lineOf, oneLineIn } from './support/lines'
import { mount, rerender } from './support/mount'

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
 * `MetadataEditor.tsx`'s title field renders `<label className="field title">`
 * on the Series page. A bare `.field` selector would reach it and restyle a
 * screen nobody decided to change.
 *
 * Built from the class list by hand rather than by mounting `MetadataEditor`
 * itself — the defect is entirely about which stylesheet rule a class list
 * reaches, not about the component's own behaviour. The pair compared is a
 * hoisted `.field` beside an untouched control in the same `.library`.
 */
describe('the Series page’s title field, after the Rules screen hoisted .field', () => {
  it('is not restyled by a class the Series page never opted into', () => {
    const host = mount(
      <div className="library">
        <label className="field title">
          <span>Title</span>
          <input />
        </label>
        <label className="untouched">
          <span>Untouched</span>
          <input />
        </label>
      </div>,
    )
    const label = host.querySelector('.field.title')
    const hoisted = host.querySelector('.field.title input')
    const untouched = host.querySelector('.untouched input')
    if (!label || !hoisted || !untouched) {
      throw new Error('the fixture did not render')
    }

    // Compared against the untouched control rather than against hard-coded
    // numbers, so this still holds if a token's value changes.
    expect(getComputedStyle(label).textTransform).toBe('none')
    expect(getComputedStyle(hoisted).backgroundColor).toBe(
      getComputedStyle(untouched).backgroundColor,
    )
    expect(getComputedStyle(hoisted).borderRadius).toBe(
      getComputedStyle(untouched).borderRadius,
    )
    expect(getComputedStyle(hoisted).fontSize).toBe(
      getComputedStyle(untouched).fontSize,
    )
  })
})

/**
 * An open volume draws the editor inside its tile, `.library` > `main` >
 * `ul.volumes` > `li`, beside the tile's own `span.title` in its link in
 * `.tile-body`, as `Series.tsx` draws it, and the editor's Title field is
 * `label.field.title`: a rule for the tile's title that named any `.title`
 * in `.volumes` would draw the field in the title face too. The label and
 * the box are held to the form's own face: the label inherits it, and the
 * box takes it through `base.css`'s `font: inherit`.
 */
describe('the editor’s Title field in a volume’s tile', () => {
  it('draws its label and box in the editor’s face, not the tile title’s', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          <main>
            <ul className="volumes">
              <li>
                <div className="tile-body">
                  <a href="/read/w">
                    <span className="title">t</span>
                  </a>
                </div>
                <MetadataEditor
                  slug="w"
                  title="t"
                  onPatch={vi.fn()}
                  canStamp={false}
                />
              </li>
            </ul>
          </main>
        </div>,
      )
    })
    const need = (selector: string) => {
      const el = host?.querySelector(selector)
      if (el == null) throw new Error(`no ${selector} mounted`)
      return el
    }
    const face = (el: Element) => {
      const style = getComputedStyle(el)
      return {
        family: style.fontFamily,
        weight: style.fontWeight,
        size: style.fontSize,
        lineHeight: style.lineHeight,
      }
    }
    const tile = face(need('.volumes li > .tile-body > a > .title'))
    const form = face(need('form.metadata-editor'))

    // The fixture tells: the tile's title differs from the form's face in
    // each of the four, so a field drawn in the title face cannot match.
    expect(tile.family).not.toBe(form.family)
    expect(tile.weight).not.toBe(form.weight)
    expect(tile.size).not.toBe(form.size)
    expect(tile.lineHeight).not.toBe(form.lineHeight)

    // One comparison, so a failure shows the label and the box together.
    expect({
      label: face(need('.metadata-editor .field.title')),
      box: face(need(".metadata-editor .field.title [name='title']")),
    }).toEqual({ label: form, box: form })
  })
})

describe('Write ComicInfo.xml’s confirm, held in a real browser (O51)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [
        { member_id: 7, edition_key: 'e1', path: '/lib/v01.cbz', bytes: 1024 },
      ],
      total_bytes: 1024,
      skipped_formats: [],
    })
    vi.mocked(stamp.performStamp).mockResolvedValue({
      rewritten: 1,
      content_key: null,
      document_moved: false,
      document_rewritten: false,
      sidecar_rewritten: false,
    })
  })

  it('sends nothing for a real mouse press while the job lock holds, and stamps once a poll finds it free', async () => {
    vi.mocked(derive.deriveStatus)
      .mockResolvedValueOnce({ derive_running: false, job_held: true })
      .mockResolvedValue({ derive_running: false, job_held: false })
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MetadataEditor
          slug="w"
          title="t"
          onPatch={vi.fn().mockResolvedValue({ title: 't', persisted: true })}
          canStamp
        />,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    const ask = host.querySelector("button[name='plan-stamp']") as HTMLElement
    await act(async () => {
      await userEvent.click(ask)
    })
    const confirm = () =>
      host?.querySelector("button[name='confirm-stamp']") as HTMLButtonElement
    await settleUntil(
      'the confirm, held',
      () => holdOf(confirm()) === 'held',
      1000,
    )
    await pressOn(confirm())
    expect(stamp.performStamp).not.toHaveBeenCalled()

    await settleUntil(
      'the confirm, free',
      () => holdOf(confirm()) === 'free',
      SCAN_POLL_MS * 3,
    )
    await pressOn(confirm())
    expect(stamp.performStamp).toHaveBeenCalledTimes(1)
  })
})

/**
 * The editor in a `div` `width` wide inside `.library`, its saves answered
 * by `onPatch`. The face is loaded first: the field is fitted as it mounts,
 * and a face that landed later would change its lines and not its height.
 */
async function fieldIn(
  width: string,
  title: string,
  onPatch = vi.fn().mockResolvedValue({ title, persisted: true }),
) {
  const host = mount(
    <div className="library">
      <div style={{ width }} />
    </div>,
  )
  const holder = host.querySelector<HTMLElement>('.library > div')
  if (holder === null) throw new Error('no holder mounted')
  // The form sets no font of its own, so it is drawn in the holder's.
  const face = getComputedStyle(holder)
  await document.fonts.load(
    `${face.fontStyle} ${face.fontWeight} ${face.fontSize} ${face.fontFamily}`,
    title,
  )
  await act(async () => {
    rerender(
      <div className="library">
        <div style={{ width }}>
          <MetadataEditor slug="w" title={title} onPatch={onPatch} />
        </div>
      </div>,
    )
  })
  const label = holder.querySelector('.metadata-editor .field.title')
  const field = holder.querySelector<HTMLTextAreaElement>("[name='title']")
  if (label === null || field === null) throw new Error('no field mounted')
  return { holder, label, field }
}

describe('the Title field fits its title (O283)', () => {
  /** The holder's width: the titles below take 1, 3 and 8 lines there. */
  const WIDTH = '18rem'

  const ONE = 'Kagami no Kuni, Volume 01'

  /** 96 characters, as long as a series title with its edition runs. */
  const THREE =
    'Kagami no Kuni no Harisugawa: The Complete Digital Edition, Volume 01 (Collector’s Box Set, Ed.)'

  const EIGHT = `${THREE} ${THREE} Kagami no Kuni no Harisugawa: The Complete Digital Edition`

  /** The field's lines, and the height of one, as it is drawn. */
  const linesOf = (field: HTMLTextAreaElement) => {
    const style = getComputedStyle(field)
    const line = parseFloat(style.lineHeight)
    const inner =
      field.clientHeight -
      parseFloat(style.paddingTop) -
      parseFloat(style.paddingBottom)
    return { line, inner }
  }

  it.each([
    [1, ONE],
    [3, THREE],
    [8, EIGHT],
  ])(
    'is as tall as a title of %i lines, up to five, at its width',
    async (lines, title) => {
      const { field } = await fieldIn(WIDTH, title)

      // The fixture tells: the title takes that many lines at the field's width.
      expect(lineCount(field, title), 'the title’s lines').toBe(lines)

      const { line, inner } = linesOf(field)
      expect(inner, 'the field’s height in lines').toBe(
        Math.min(lines, 5) * line,
      )
      if (lines > 5) {
        expect(field.scrollHeight, 'scrolls inside').toBeGreaterThan(
          field.clientHeight,
        )
        expect(
          getComputedStyle(field).overflowY,
          'scrolled by a wheel or a finger',
        ).toBe('auto')
      }
    },
  )

  it('wraps the whole title at rest, 8 px in from each side', async () => {
    const { label, field } = await fieldIn(WIDTH, THREE)

    // The fixture tells: on one line the title runs past the field's line.
    expect(
      oneLineIn(label, THREE),
      'the title on one line against the field’s line',
    ).toBeGreaterThan(lineOf(field))

    expect(document.activeElement, 'never focused').not.toBe(field)
    expect(field.scrollWidth, 'its title wraps').toBeLessThanOrEqual(
      field.clientWidth + 1,
    )
    const style = getComputedStyle(field)
    expect({
      whiteSpace: style.whiteSpace,
      paddingLeft: style.paddingLeft,
      paddingRight: style.paddingRight,
    }).toEqual({
      whiteSpace: 'pre-wrap',
      paddingLeft: '8px',
      paddingRight: '8px',
    })
  })

  it('neither moves nor grows when clicked, or typed into without taking another line', async () => {
    const { label, field } = await fieldIn(WIDTH, THREE)
    const typed = `${THREE}x`

    // The fixture tells: the character typed at the end takes no new line.
    expect(lineCount(field, typed), 'typed, against at rest').toBe(
      lineCount(field, THREE),
    )

    const rects = () => ({
      field: field.getBoundingClientRect().toJSON(),
      label: label.getBoundingClientRect().toJSON(),
    })
    const atRest = rects()
    await act(async () => {
      await userEvent.click(field)
    })
    expect(document.activeElement, 'clicked into').toBe(field)
    expect(rects(), 'clicked').toEqual(atRest)

    await act(async () => {
      await userEvent.keyboard('{Control>}{End}{/Control}x')
    })
    expect(field.value).toBe(typed)
    expect(rects(), 'typed into').toEqual(atRest)
  })

  it('grows one line as a typed title takes another', async () => {
    const { field } = await fieldIn(WIDTH, THREE)
    const suffix = ' and its Bonus Booklet'

    // The fixture tells: typed, the suffix takes exactly one line more.
    expect(lineCount(field, `${THREE}${suffix}`), 'typed').toBe(
      lineCount(field, THREE) + 1,
    )

    const atRest = linesOf(field)
    await act(async () => {
      await userEvent.click(field)
    })
    await act(async () => {
      await userEvent.keyboard(`{Control>}{End}{/Control}${suffix}`)
    })
    expect(field.value).toBe(`${THREE}${suffix}`)
    expect(linesOf(field).inner - atRest.inner, 'grown by').toBe(atRest.line)
  })

  it('is fitted again when its width changes', async () => {
    const { holder, field } = await fieldIn('18rem', THREE)
    const rem = parseFloat(getComputedStyle(document.documentElement).fontSize)
    // The field is as wide as its holder, so its line widens as much.
    const wider = lineOf(field) + 12 * rem

    // The fixture tells: the title takes fewer lines at 30rem than at 18rem.
    const narrowLines = lineCount(field, THREE)
    const wideLines = lineCount(field, THREE, wider)
    expect(wideLines, 'at 30rem, against 18rem').not.toBe(narrowLines)

    // A resize the observer could not report in its own frame reaches the
    // page as an error event.
    const loops: string[] = []
    const heard = (event: ErrorEvent) => {
      if (event.message.includes('ResizeObserver loop'))
        loops.push(event.message)
    }
    window.addEventListener('error', heard)
    onTestFinished(() => window.removeEventListener('error', heard))

    holder.style.width = '30rem'
    // One frame for the layout, and the next for what follows from it.
    await new Promise(requestAnimationFrame)
    await new Promise(requestAnimationFrame)

    expect(lineOf(field), 'its line at 30rem').toBe(wider)
    const { line, inner } = linesOf(field)
    expect(inner, 'the field’s height in lines').toBe(wideLines * line)

    // And a third, in which the field is watched again.
    await new Promise(requestAnimationFrame)
    expect(loops, 'the observer’s loop errors').toEqual([])
  })

  it('is fitted again when a face lands after it was fitted', async () => {
    // Named for a face the document does not hold yet, so the field is
    // fitted in the fallback, as a title in a UI-face subset not yet
    // fetched is.
    const holder = (children?: ReactNode) => (
      <div className="library">
        <div style={{ width: WIDTH, fontFamily: '"late face", monospace' }}>
          {children}
        </div>
      </div>
    )
    const host = mount(holder())
    await act(async () => {
      rerender(
        holder(<MetadataEditor slug="w" title={THREE} onPatch={vi.fn()} />),
      )
    })
    const field = host.querySelector<HTMLTextAreaElement>("[name='title']")
    if (field === null) throw new Error('no field mounted')
    const fallback = lineCount(field, THREE)
    expect(linesOf(field).inner, 'fitted in the fallback').toBe(
      fallback * linesOf(field).line,
    )

    const face = new FontFace('late face', `url(${latinFace})`)
    document.fonts.add(face)
    onTestFinished(() => {
      document.fonts.delete(face)
    })
    await face.load()
    // The fixture tells: the face sets the title in another count of lines.
    const landed = lineCount(field, THREE)
    expect(landed, 'in the face, against the fallback').not.toBe(fallback)

    await expect
      .poll(() => linesOf(field).inner, { message: 'the field’s height' })
      .toBe(landed * linesOf(field).line)
  })

  it('O274: a pasted line break is a space, and the caret is left after the paste', async () => {
    const { field: box } = await fieldIn(WIDTH, 'Kagami Volume 1')
    const source = document.createElement('textarea')
    source.value = 'no\nKuni '
    document.body.append(source)
    onTestFinished(() => source.remove())
    source.select()
    await userEvent.copy()

    await act(async () => {
      box.focus()
      box.setSelectionRange(7, 7)
      await userEvent.paste()
    })

    expect({ value: box.value, caret: box.selectionStart }).toEqual({
      value: 'Kagami no Kuni Volume 1',
      caret: 'Kagami no Kuni '.length,
    })
  })
})

describe('the Title field’s label, Saved and what follows it (O283)', () => {
  const WIDTH = '18rem'

  /** One line at the field's width, with room for a character or two more. */
  const TITLE = 'Kagami no Kuni, Volume 01'

  /** The form, the label's own text and the status line under `holder`. */
  const partsOf = (holder: HTMLElement) => {
    const form = holder.querySelector('form.metadata-editor')
    const words = holder.querySelector('.metadata-editor .field.title > span')
    const saved = holder.querySelector('.metadata-editor [role="status"]')
    if (form === null || words === null || saved === null) {
      throw new Error('no form, label text or status line')
    }
    return { form, words, saved }
  }

  /** `text` typed at the field's end with real keys, then a real Enter. */
  const enterAtEnd = async (field: HTMLTextAreaElement, text: string) => {
    await act(async () => {
      await userEvent.click(field)
    })
    await act(async () => {
      await userEvent.keyboard(`{Control>}{End}{/Control}${text}{Enter}`)
    })
  }

  it('sets the label 4 px above the field', async () => {
    const { holder, field } = await fieldIn(WIDTH, TITLE)
    const { words } = partsOf(holder)

    expect(
      field.getBoundingClientRect().top - words.getBoundingClientRect().bottom,
    ).toBeCloseTo(4, 0)
  })

  it('draws Saved at the end of the label’s line, so nothing moves as it comes and goes', async () => {
    const { holder, label, field } = await fieldIn(WIDTH, TITLE)
    const { form, words, saved } = partsOf(holder)

    // The fixture tells: the character typed takes no new line, so only
    // Saved could move what is measured.
    expect(lineCount(field, `${TITLE}!`), 'typed, against at rest').toBe(
      lineCount(field, TITLE),
    )

    const rects = () => ({
      field: field.getBoundingClientRect().toJSON(),
      label: label.getBoundingClientRect().toJSON(),
      form: form.getBoundingClientRect().toJSON(),
    })
    await enterAtEnd(field, '!')
    await settleUntil('Saved', () => saved.textContent === 'Saved', 1000)
    const shown = rects()

    const note = saved.getBoundingClientRect()
    expect(note.height, 'Saved, drawn').toBeGreaterThan(0)
    expect(note.top, 'its top against the label text’s').toBeGreaterThanOrEqual(
      words.getBoundingClientRect().top - 0.5,
    )
    expect(note.bottom, 'its bottom against the field’s top').toBeLessThan(
      field.getBoundingClientRect().top + 0.5,
    )
    expect(note.right, 'its right edge against the form’s').toBeCloseTo(
      form.getBoundingClientRect().right,
      0,
    )

    const probe = document.createElement('span')
    probe.style.color = 'var(--muted)'
    form.append(probe)
    const muted = getComputedStyle(probe).color
    probe.remove()
    expect(getComputedStyle(saved).color, 'its colour').toBe(muted)

    await settleUntil('Saved, cleared', () => saved.textContent === '', 4000)
    expect(rects(), 'Saved cleared, against shown').toEqual(shown)
  })

  it('draws Saving… on the label’s line, so nothing moves as it comes and goes', async () => {
    const out = deferred<{ title: string; persisted: boolean }>()
    const { holder, label, field } = await fieldIn(
      WIDTH,
      TITLE,
      vi.fn().mockReturnValue(out.promise),
    )
    const { form, words, saved } = partsOf(holder)

    // The fixture tells: Saving… is the wider of the two, so it reaches
    // nearer the label's text than Saved does; and the character typed
    // takes no new line, so only the note could move what is measured.
    expect(
      oneLineIn(saved, 'Saving…'),
      'Saving… against Saved, drawn',
    ).toBeGreaterThan(oneLineIn(saved, 'Saved'))
    expect(lineCount(field, `${TITLE}!`), 'typed, against at rest').toBe(
      lineCount(field, TITLE),
    )

    const rects = () => ({
      field: field.getBoundingClientRect().toJSON(),
      label: label.getBoundingClientRect().toJSON(),
      form: form.getBoundingClientRect().toJSON(),
    })
    await act(async () => {
      await userEvent.click(field)
    })
    await act(async () => {
      await userEvent.keyboard('{Control>}{End}{/Control}!')
    })
    const before = rects()
    await act(async () => {
      await userEvent.keyboard('{Enter}')
    })
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250))
    })
    expect(saved.textContent, 'out 250 ms').toBe('Saving…')
    const saving = rects()

    // The label's first line is its text's span, which spans the label: the
    // text's own width is where the words end.
    const note = saved.getBoundingClientRect()
    const line = words.getBoundingClientRect()
    expect(note.height, 'Saving…, drawn').toBeGreaterThan(0)
    expect(note.top, 'its top against the line’s').toBeGreaterThanOrEqual(
      line.top - 0.5,
    )
    expect(note.bottom, 'its bottom against the line’s').toBeLessThanOrEqual(
      line.bottom + 0.5,
    )
    expect(
      note.left,
      'its left edge against the label’s words',
    ).toBeGreaterThan(line.left + oneLineIn(words, 'Title'))
    expect(
      note.right,
      'its right edge against the label’s',
    ).toBeLessThanOrEqual(label.getBoundingClientRect().right + 0.5)

    await act(async () => {
      out.resolve({ title: `${TITLE}!`, persisted: true })
    })
    await settleUntil('Saved', () => saved.textContent === 'Saved', 1000)

    expect(saving, 'Saving… shown, against before the save').toEqual(before)
    expect(rects(), 'Saved shown, against before the save').toEqual(before)
  })

  it('puts a failure 8 px under the field, and the unwritten reasons too', async () => {
    const patch = vi
      .fn()
      .mockRejectedValueOnce(new Error('/api/library/works/w: 500'))
      .mockResolvedValueOnce({
        title: `${TITLE}!!`,
        persisted: false,
        why: ['the root is read-only'],
      })
    const { holder, field } = await fieldIn(WIDTH, TITLE, patch)
    const under = (el: Element) =>
      el.getBoundingClientRect().top - field.getBoundingClientRect().bottom

    await enterAtEnd(field, '!')
    await settleUntil(
      'the failure',
      () => holder.querySelector('.metadata-editor .failure') !== null,
      1000,
    )
    const failure = holder.querySelector('.metadata-editor .failure')
    if (failure === null) throw new Error('no failure drawn')
    expect(under(failure), 'the failure').toBeCloseTo(8, 0)
    // O286: in the colour every other failure takes.
    expect(getComputedStyle(failure).color, 'the failure’s colour').toBe(
      tokenColour(holder, '--danger'),
    )

    await enterAtEnd(field, '!')
    await settleUntil(
      'the unwritten reasons',
      () => holder.querySelector('.metadata-editor .unwritten li') !== null,
      1000,
    )
    const unwritten = holder.querySelector('.metadata-editor .unwritten > p')
    if (unwritten === null) throw new Error('no unwritten note drawn')
    expect(under(unwritten), 'the unwritten reasons').toBeCloseTo(8, 0)
  })
})
