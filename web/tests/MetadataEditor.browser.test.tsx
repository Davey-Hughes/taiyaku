import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
// Required, the same reason every other file in this project gives: the
// rules under test live in `admin.css`, which reaches the app through
// `index.css`. Without this the boxes below measure browser defaults.
import '../src/index.css'
import { SCAN_POLL_MS } from '../src/config'
import * as derive from '../src/libraries'
import { MetadataEditor } from '../src/MetadataEditor'
import * as stamp from '../src/stamp'
import { settleUntil } from './support/async'
import { pressOn } from './support/callout'
import { holdOf } from './support/held'
import { mount } from './support/mount'

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
 * on the Series page, and no `.metadata-editor` rule exists anywhere. A bare
 * `.field` selector would reach it and restyle a screen nobody decided
 * to change.
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
