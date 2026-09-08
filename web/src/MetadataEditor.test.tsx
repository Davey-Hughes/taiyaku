import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { MetadataEditor } from './MetadataEditor'
import * as stamp from './stamp'
import { mount } from './testing/mount'

// `AcceptForm.test.tsx`'s shape, for the reason its own comment gives.
// `humanBytes` is deliberately not mocked: `bytes.test.ts` pins it, and
// stubbing it here would leave this file unable to tell a section that calls
// it from one that prints its own number.
vi.mock('./stamp', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./stamp')>()),
  stampPlan: vi.fn(),
  performStamp: vi.fn(),
}))

/**
 * Set a controlled field's value the way a keystroke does — through the
 * prototype's own setter, not the element's. `AcceptForm.test.tsx` carries
 * the same helper for the same reason: React tracks the value it last set on
 * the instance, and a plain `el.value = x` assignment races that tracker, so
 * the `input` event dispatched right after can fire with React believing
 * nothing changed.
 */
async function typeInto(el: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
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

const submitButton = (host: HTMLElement) =>
  host.querySelector('button[type="submit"]') as HTMLButtonElement

const titleField = (host: HTMLElement) =>
  host.querySelector<HTMLInputElement>("input[name='title']")

const planButton = (host: HTMLElement) =>
  host.querySelector<HTMLButtonElement>("button[name='plan-stamp']")

const stampButton = (host: HTMLElement) =>
  host.querySelector<HTMLButtonElement>("button[name='confirm-stamp']")

/**
 * Every read control on screen.
 *
 * **A list rather than the one**, because "one control at a time" is itself
 * an assertion. There were two — `plan-stamp`, gated on there being no plan,
 * and `replan-stamp`, gated on a refusal — sharing a handler and carrying
 * different labels, and after a failed read beside a refused stamp both drew
 * side by side.
 *
 * **Selected by exclusion — every button in the section that is not the
 * confirm — rather than by name.** This queried `button[name='plan-stamp']`,
 * which is the name of the control that survived, so the three assertions
 * spelled `toHaveLength(1)` were blind to the one that did not: re-adding
 * `replan-stamp` verbatim — gated on `refused`, sharing `askPlan`, carrying
 * its own label — left all 55 tests green. A named list of the two would have
 * the same hole one name wider, since the next control to draw beside this one
 * will not be called `replan-stamp` either. This section draws one confirm and
 * read controls, so *not the confirm* is what *a read control* means in it,
 * and a button of any name arriving beside this one is a failure rather than
 * an invisible addition.
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
 * so the two lines that clear the old answer — `setPlan(undefined)` and
 * `setPlanFailure(undefined)` — could both be deleted and the suite stayed
 * green: whatever they blank is overwritten a microtask later by the answer
 * itself. What they actually protect is the frame in between, where a stale
 * `Rewrite 2 archives` button sat live beside "Working out which archives
 * would change…".
 */
function deferred<T>() {
  let settle!: (value: T) => void
  const promise = new Promise<T>((resolve) => {
    settle = resolve
  })
  return { promise, settle }
}

/** Let React drain the microtasks a hand-settled promise queued. */
const drain = async () => {
  await act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })
}

describe('MetadataEditor', () => {
  it('saves a corrected title and shows the new one', async () => {
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

    const field = titleField(host)
    expect(field?.value).toBe('Series - v01 (2021).cbz')

    await typeInto(field!, 'Volume 1')
    await click(submitButton(host))

    expect(patch).toHaveBeenCalledWith('w', { title: 'Volume 1' })
    expect(host.textContent).toContain('Volume 1')
  })

  /**
   * **I6's third shape.** The test above cannot distinguish
   * `setCurrent(result.title ?? value)` from the regressed `setCurrent(value)`
   * — its mock resolves with the exact string just typed, so the two
   * expressions agree by coincidence. This mocks a re-projected title that
   * differs from what was typed (the server's own word for what it actually
   * holds now, per this file's own doc comment), so only the correct
   * expression can make the displayed title match it.
   */
  it('shows the server-reprojected title, not the value just typed, when they differ', async () => {
    const patch = vi.fn().mockResolvedValue({
      title: 'Volume 1: The Reprojected Title',
      persisted: true,
    })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await typeInto(titleField(host)!, 'Volume 1')
    await click(submitButton(host))

    expect(host.querySelector('p.current')?.textContent).toBe(
      'Volume 1: The Reprojected Title',
    )
  })

  it('says so when the correction could not be written beside the library', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: false })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.textContent).toMatch(/could not be saved beside your library/i)
  })

  it('says nothing about being unwritten when the correction landed cleanly', async () => {
    const patch = vi.fn().mockResolvedValue({ title: 'old', persisted: true })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.textContent).not.toMatch(/could not be saved/i)
  })

  /**
   * The claim is stronger than "no particular wording appears": a response
   * with `keeps_documents: false` must render **exactly** what an ordinary
   * successful save renders — nothing at all keyed off that field, not even
   * a new note this test's own pinned string would not know to look for.
   * Pinning one wording (`/could not be saved beside your library/i`, this
   * test's own shape until now) would catch a regression that reuses that
   * exact note but wave through a new alarm element keyed off
   * `keeps_documents` with different wording; comparing two full renders
   * catches either.
   */
  it('renders exactly what an ordinary successful save renders when keeps_documents is false', async () => {
    const ordinary = mount(
      <MetadataEditor
        slug="w"
        title="old"
        onPatch={vi.fn().mockResolvedValue({ title: 'old', persisted: true })}
      />,
    )
    await click(submitButton(ordinary))

    const declined = mount(
      <MetadataEditor
        slug="w"
        title="old"
        onPatch={vi.fn().mockResolvedValue({
          title: 'old',
          persisted: true,
          keeps_documents: false,
        })}
      />,
    )
    await click(submitButton(declined))

    expect(declined.innerHTML).toBe(ordinary.innerHTML)
  })

  it('names every reason a fanned-out edit could not all reach disk', async () => {
    const patch = vi.fn().mockResolvedValue({
      title: 'old',
      persisted: false,
      why: ['root is read-only', 'no on-disk address'],
    })
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

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

    await click(submitButton(host))

    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      '409 a name this route refused',
    )
    expect(host.textContent).not.toMatch(/could not be saved beside/i)
  })

  it('stringifies a rejection that is not an Error, rather than losing it', async () => {
    const patch = vi.fn().mockRejectedValue('offline')
    const host = mount(<MetadataEditor slug="w" title="old" onPatch={patch} />)

    await click(submitButton(host))

    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')
  })

  /**
   * The same shape as `askPlan`'s two blanking lines, found by sweeping for
   * it rather than by reading this function: `submit`'s `setFailure(undefined)`
   * has no observable effect once the answer lands, so deleting it survived
   * the whole suite. What it protects is the frame in between, where the
   * previous save's alert sat over a save that was still out.
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
    const host = mount(
      <MetadataEditor slug="w" title="Volume 1" onPatch={patch} />,
    )
    await click(submitButton(host))
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')

    await click(submitButton(host))

    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(submitButton(host).disabled).toBe(true)

    retried.settle({ title: 'Volume 1', persisted: true })
    await drain()

    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('.current')?.textContent).toBe('Volume 1')
  })
})

describe('MetadataEditor, the ComicInfo stamp', () => {
  const noop = vi.fn().mockResolvedValue({ title: 't', persisted: true })

  /**
   * **The three sizes are chosen so no two of them format alike.** 512 MiB and
   * 1.5 GiB sum to exactly 2 GiB, so `humanBytes` draws `512.0 MiB`,
   * `1.5 GiB` and `2.0 GiB` — three distinct strings. They were 40 MiB and
   * 2 GiB summing to 2.0390625 GiB, which `humanBytes` also draws as
   * `2.0 GiB`: substituting the largest archive's size for the total survived
   * the whole suite, so the total was not pinned by the test that names it.
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
   * as belt and braces. Nothing before this test asserted the structure
   * directly, so a future change that moved the section back inside the form
   * would go uncaught unless it also dropped `type="button"`. A button with
   * no `type` inside a form submits it, which would make an irreversible
   * archive rewrite a side effect of pressing Save.
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
   * **No digit of the count appears in the key.** `content_key` was `'k2'`
   * and the completion test asserted `toContain('2')`, which the key alone
   * satisfied: replacing the whole count phrase with a literal
   * `Rewrote archives.` survived the suite, leaving the number of archives
   * actually rewritten unasserted after an irreversible operation.
   */
  const DONE = {
    rewritten: 2,
    content_key: '9f0e7bd3ac5e1a6f',
    document_moved: true,
    sidecar_rewritten: true,
  }

  /** The key moved and the document did not — O81's stranding case. */
  const LEFT_BEHIND = {
    rewritten: 2,
    content_key: '9f0e7bd3ac5e1a6f',
    document_moved: false,
    sidecar_rewritten: false,
  }

  /** The work's identity did not move at all: `moved_key` was `None`. */
  const KEY_HELD = {
    rewritten: 1,
    content_key: null,
    document_moved: false,
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
    // The `.skipped` note rendered inside the non-empty branch alone, so this
    // work — every archive a `.cbr` — read as "nothing to write … or this
    // work has no zip archive at all" and the rar never appeared. The server
    // had already told the two situations apart.
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
   * **Both presses inside one `act`**, on a control that carries no
   * `disabled` at all — it is conditionally *rendered*, and within one batch
   * it is still in the DOM. So only the ref can stop the second, which is
   * what makes this test about the ref rather than about an attribute.
   *
   * The comment this replaces argued no guard was needed because "a second
   * press is only reachable after a failure, by which time the first has
   * already settled". Verified false: it sent two `stampPlan`s.
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
    // dispatches: React batches the state update, so `disabled={sending}` has
    // not been applied to the DOM when the second lands. Only the ref stops
    // it. #112's two double-press tests clicked twice across two `act`s and
    // so proved `disabled` rather than the ref they named.
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
   * **The count, as a whole phrase.** `toContain('2')` was satisfied by the
   * old fixture's `content_key: 'k2'` alone, so replacing the count with a
   * literal `Rewrote archives.` survived — the one number that says how many
   * of a person's files were irreversibly rewritten went unasserted. The key
   * in `DONE` now holds no digit of the count, and this compares the whole
   * sentence rather than looking for a character in it.
   */
  it('states exactly how many archives were rewritten', async () => {
    const host = await stampWith(PLAN, DONE)

    expect(host.querySelector('.stamped .rewrote')?.textContent).toBe(
      'Rewrote 2 archives.',
    )
  })

  /**
   * **The one outcome a screen reader was never told.** A refusal carries
   * `role="alert"` and announces itself; the completion carries no role, and
   * it was drawn outside every live region — so what a person heard, after
   * agreeing to a rewrite with no undo, was "Rewriting 2 archives — the
   * originals are being replaced now", then that paragraph removed (silent
   * under the default `aria-relevant`), then silence. The completion is inside
   * the region now.
   *
   * **And the region itself is pinned, which it was not.** Deleting
   * `aria-live="polite"` from the wrapper survived all 55 tests of pass 3,
   * while `AcceptForm` — the incumbent this section's wrapper and its comment
   * were both copied from — has carried `expect(why.getAttribute('aria-live'))
   * .toBe('polite')` since it shipped. The comment travelled and the test did
   * not.
   *
   * The identity check is the other half: a region has to exist *before* what
   * it announces is put into it, so this asserts the node the completion lands
   * inside is the same node that was on screen before anything was asked —
   * which a `role="status"` on `.stamped` itself, the alternative, could not
   * be.
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

    // The defect: `content_key` is `Option<String>` in the store and serialised
    // straight through, so `null` reached a field typed `string` and the screen
    // read "This work's content_key is now . It had no document on disk to
    // move." — an empty key, and a claim about a document nothing had checked.
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
   * O81's case, and the one the old copy stated as benign. `stamp::apply`
   * hard-codes `document_moved: false` when the key did not move, and
   * `hand::rename_for_rekey` answers `false` for five causes — two of which
   * leave the document stranded at the old key. The screen must name the
   * uncertainty rather than either reading of it.
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

  it('surfaces a 409 from the stamp as an alert, and keeps the plan on screen', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockRejectedValue(
      new Error(
        '/api/library/works/w/stamp-comicinfo: 409 a scan or another library job is currently running',
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
   * the title in the form above and Save — the rendered XML now differs, so a
   * third zip enters the plan — then confirm. The server answers "this work's
   * archives have changed since the plan was made (2 then, 3 now); nothing
   * was written". Before this the only enabled control resent the same body,
   * which the same guard refuses every time, and closing the whole panel was
   * the only escape.
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
   * The same ref, on the press the false comment was actually about: it
   * claimed a second press was "only reachable after a failure, by which time
   * the first has already settled". This is that press, doubled inside one
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
   * **Not the only state where both controls are live** — that was true when
   * the second read control was gated on a refusal, and the read control has
   * rendered in all eight phases since; `ready` has both too. What a refusal
   * adds is the alert and the plan the confirm would send again, which is what
   * the tests below press against.
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
    // **Pass 3's finding 2.** The ref refused the press; until now nothing
    // said so, and the control sat enabled and inert for the length of a
    // rewrite over a NAS. Every other ref-guarded control in this tree pairs
    // the ref with the attribute.
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
   * wrote every one of them. Two things have to hold at once here. The
   * completion used to render inside `plan.archives.length > 0`: the count,
   * the content_key and the whole O81 stranded-document paragraph were
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
   * **The supersede block, driven past the frame the other three stop at.**
   * `confirmStamp` releases `planInFlight` and clears `asking` on behalf of
   * the read it overtakes, because `settlePlan` returns on the sequence
   * mismatch *before* it reaches either line — so for a superseded read those
   * two statements are the only thing that ever releases them. Three tests
   * press both controls in one batch; all three stop at the frame, and none
   * uses the section again afterwards.
   *
   * Measured, one deletion at a time with the rest of the suite green:
   *
   * - without `planInFlight.current = false`, `askPlan`'s guard refuses every
   *   press for the life of the mount while `busy` is false — the read control
   *   enabled and permanently inert, which is the shape pass 3 raised;
   * - without `setAsking(false)`, `asking` never clears, so once the write
   *   clears the plan slot the phase parks in `reading` for good: "Working out
   *   which archives would change…" on screen, the read control disabled, and
   *   nothing on the wire.
   *
   * Both are dead ends with no way out but a remount, which is why this
   * asserts a press that *lands* rather than a control that merely looks
   * enabled: the first deletion leaves it enabled.
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
   * **The frame between the press and the answer, which nothing observed.**
   * Deleting `setPlan(undefined)` from `askPlan` survived the whole suite,
   * because every test awaited the answer that overwrote it. The live
   * `Rewrite 2 archives` button below is what that line actually protects: it
   * sends the plan the server has already refused. `phaseOf` tests the plan
   * slot before `asking` so that *this* test is the one that sees the
   * deletion: measured with the order reversed, the stale plan hides behind
   * the reading line here and only `draws one read control and one alert when
   * the read and the write both fail` still catches it.
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
    // the ref that refuses a second read now has the attribute beside it.
    expect(readButtons(host)).toHaveLength(1)
    expect(planButton(host)?.disabled).toBe(true)

    replanned.settle(WIDER_PLAN)
    await drain()

    expect(askingLine(host)).toBeNull()
    expect(stampButton(host)?.textContent).toBe('Rewrite 3 archives')
  })

  /**
   * The same frame on the other read, and the other line: deleting
   * `setPlanFailure(undefined)` from `askPlan` also survived the suite,
   * because no test had ever failed a plan read and then asked again.
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
   * **The forecast, weakened to what the outcome can report.** It read "and
   * its <content_key>.library.json moves with it when it does" while
   * `key-moved-alone` — on screen at the same time, since the plan does not
   * unmount on success — has to say the document may be stranded instead.
   * `hand::rename_for_rekey` answers `false` for five causes, two of which
   * strand it, so the certainty was never available to promise.
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
   * **`Format::is_readable` is `Dir | Zip` alone.** The note read "which
   * taiyaku reads and never writes", which is false for every token
   * `stamp.rs` can print but `dir` — `pages.rs`'s `unsupported_container`
   * refuses rar, 7z, tar and PDF outright. What is true of all of them is the
   * write rule.
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
   * `taiyaku_read::sniff` could not open at all. Listing it beside `rar` read
   * "this work holds unreadable, which taiyaku reads and never writes" — a
   * sentence that contradicts itself twice over.
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
   * The third instance of the same shape, and the one that matters most:
   * `confirmStamp`'s `setStampFailure(undefined)` left a 409 on screen for
   * the whole of the retry that was answering it, so the screen said the
   * stamp had been refused while it was in the middle of rewriting the
   * files. Deleting the line survived the suite for the reason `deferred`
   * above records.
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
   * **One test per phase from here down, plus one apiece for pass 3's three
   * Important findings.** The section's eight phases are computed in one
   * place now (`phaseOf`), and what each one draws is a property of the phase
   * rather than of six booleans read separately by nine gates — which is what
   * let three rounds of fixes leave the gates disagreeing with each other.
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
   * `reading`, and the first half of finding 2: the ref that refuses a second
   * read now has the attribute beside it, so the control says it is busy
   * rather than sitting enabled and inert.
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
   * **Finding 3.** `sending` reached the DOM in exactly one place — inside
   * the confirm control's own label — and that control was gated on the plan
   * slot, so the one sentence saying an irreversible rewrite was running
   * could not be drawn in two of the states it runs in. It is a phase now,
   * and the phase carries the plan being written rather than reading the
   * slot.
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
    // Not the read's sentence, which is what a blanked plan used to leave as
    // the only thing on screen, and not an empty plan's.
    expect(askingLine(host)).toBeNull()
    expect(host.querySelector('.stamp-comicinfo .nothing')).toBeNull()

    posted.settle(DONE)
    await drain()

    expect(host.querySelector('.stamp-comicinfo .writing')).toBeNull()
  })

  /** The second half of finding 2: neither control is live during the write. */
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
   * The same sentence in the state that had none, and the empty plan that
   * used to replace it. A re-read pressed in the same batch as the confirm
   * blanks the plan slot; the answer it gets back is empty — the likely one,
   * since the stamp is rewriting every archive — and landing it drew "Nothing
   * to write: every zip archive in this work already holds the ComicInfo.xml
   * taiyaku would write" over files being rewritten at that moment. The
   * answer is now discarded (its GET went out before the POST) and the phase
   * says what is happening whatever the slot holds.
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
   * a question with a mechanical answer instead of the argument the deleted
   * comment was making.
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
   * **Finding 1: the dead end.** A plan read strictly after a rewrite, by an
   * explicit press, describes archives that still need stamping — a work
   * whose title was corrected between the two, or one the confirm's own 409
   * sent the person back to re-read. The confirm control used to be gated on
   * `stamped === undefined` under a comment claiming both plans reachable
   * there predated the rewrite, so this phase drew the total, the archive
   * list, the consequences, the irreversible warning and **no controls at
   * all** — and `stamped` is never cleared by a read, so re-reading could not
   * recover.
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
   * **Findings 4 and 5.** Press the re-read and the confirm in one batch and
   * let both requests fail: this drew `plan-stamp` and `replan-stamp` side by
   * side, same handler and different labels, under two `role="alert"`
   * paragraphs. There is one read control now, and the two failures cannot
   * coexist at all — a stamp supersedes the read it overtook, so that GET's
   * rejection is discarded rather than drawn beside the POST's.
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
   * **Finding 6.** The sentence ended "…, or this work has no zip archive at
   * all", and it can sit above "Rewrote 2 archives." and above a 409 reading
   * "this work's archives have changed since the plan was made" — the
   * disjunct is untrue in both. Nothing was lost by dropping it: "every zip
   * archive already holds it" is true of a work with none, and `.skipped`
   * says what the work holds instead.
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
   * **Finding 7.** An EPUB *is* a zip container, so `epub-fixed` in the
   * skipped list beside "writes ComicInfo.xml into zip archives and nothing
   * else" was a sentence a reader cannot reconcile. What taiyaku filters on
   * is the identification `sniff` made — `stamp::plan` skips every member
   * whose format is not `Format::Zip`, and an EPUB is `EpubFixed` or
   * `EpubReflow` — so that is what the copy states, and the container is
   * named where it would otherwise contradict it.
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
   * The completion rendered inside `plan.archives.length > 0` two rounds ago,
   * so that read replaced the count, the content_key and the whole O81
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
})
