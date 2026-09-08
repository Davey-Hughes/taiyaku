# C3 §6 PR B — the stamp confirmation screen: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the ComicInfo write-back the only surface C3 §6 says it may
have — a confirmation inside `MetadataEditor`, below Save and never part of
it — and give an administrator a way to grant `can_stamp` without hand-writing
a `PATCH`.

**Architecture:** `web/src/stamp.ts` is a thin client over the two routes PR A
shipped, echoing the plan back unaltered because `stamp::apply` compares it
field for field. `MetadataEditor` gains a sibling section after its `<form>`
that renders one control, fetches the plan on press, and only then draws the
confirm — `UserRow`'s *the confirm does not exist until the counts do*.
`Series.tsx` decides the section exists at all, from the gate. `UserRow` gains
a third capability checkbox over the `PATCH` field PR A already accepts.

**Tech Stack:** React 19, TypeScript, Vitest + `jsdom`, `web/src/testing/mount`.

The design is
[`docs/superpowers/specs/2026-09-07-comicinfo-write-back-design.md`](../specs/2026-09-07-comicinfo-write-back-design.md).
Section references below (§5, §6, §7, §11) name it; C3 §6 names
[`2026-08-14-library-curation-design.md`](../specs/2026-08-14-library-curation-design.md).

**Nothing on the server changes.** PR A (`7d323e5`) shipped `can_stamp` on the
account row, in `Principal`, in `/api/me`'s body, in `PATCH
/api/library/users/{id}`, and both `stamp-comicinfo` routes. This branch is
`web/` and `docs/` only. If a task here finds itself editing a `.rs` file,
that is a signal the plan was wrong, not a licence.

## Global Constraints

- **Absent, not disabled.** C3 §6 and `Series.tsx`'s own `mayDownload`
  comment: a disabled control invites a reader to find out why and there is
  nothing on the screen that can tell them. This applies to the section as a
  whole and to the confirm inside it.
- **The confirm does not exist until the counts do.** `UserRow.tsx`'s module
  doc. One control; pressing it fetches the plan; only then does the confirm
  render.
- **The plan is echoed, never summarised.** `stamp::apply` compares the POST
  body to a freshly computed plan field for field and answers 409 on any
  difference. The confirm must send back the exact `archives` array the GET
  returned. §5.
- **The plan is fetched on press, never on mount.** §11: the plan's cost on
  the NAS is one seek per zip member and still unmeasured. 2.43 ms over four
  archives is a warm-disk workstation figure. A fetch per opened panel would
  spend that unmeasured cost on every panel nobody stamps.
- **The server is the authority; this asks and explains.** `UserRow.tsx`'s
  rule, one-directional: the screen may withhold what the server would refuse
  and may never offer what it would not.
- **A guarded press carries a ref as well as `disabled`.** React batches
  state, so two presses dispatched before the next render both read
  `sending === false`. The two double-press tests on #112 proved `disabled`
  and never touched the ref they named; the test here drives both clicks
  inside one `act` and has its defect hand-applied as *delete the ref, keep
  `disabled`*.
- **Assert per element, never a whole-page `toContain`.** A page-wide
  `toContain` cannot see a pairing: it passes when the path of archive 1 and
  the size of archive 2 are both somewhere on screen. Query the `<li>` and
  assert against that element.
- **Each test below gets its defect hand-applied and watched to fail** before
  the task is offered. A test that passes against the defect it names is this
  repository's most-logged failure, and neither reading nor a clean mutation
  run has ever caught one.
- **The gate is `scripts/check-all.sh`** and the line to quote is `all green`.
  Piping it through `tail` or appending `; echo` masks the exit status.
- **House documentation style:** every decision records the alternative it
  beat; a figure was measured on this machine or says it is a guess.

## Two things the spec left ambiguous, resolved here

**1. An empty plan cannot make the section absent, because the press already
happened.** §6 lists three cases for absence — no `can_stamp`, no zip member,
an empty plan — but also says the plan is fetched on press. The first is known
before any request; the second and third are the *same* answer from the GET
(`member_paths` with no zip yields an empty plan) and are known only after it.
A section that vanished under the press that summoned it reads as a broken
button, so: **absent before the press for `can_stamp`; a stated "nothing to
write" after it for an empty plan**, with the skipped formats named so the
answer is legible. The alternative — fetch on mount so all three cases can be
genuinely absent — is what §11's unmeasured NAS cost rejects.

**2. The section renders outside the `<form>`, not inside it.** §6's "below
Save and never part of it" is a claim about coupling, and a `<button>` inside
a `<form>` with no `type` submits it — so a section nested in the form would
make the irreversible thing a literal side effect of the reversible one, which
is the sentence C3 §6 is built around. `MetadataEditor` returns a fragment: the
form, then the section. `type="button"` is set as well, and Task 3 tests that
pressing the control does not call `onPatch`.

---

### Task 1: `humanBytes` moves to a module both screens can import

**Files:**
- Create: `web/src/bytes.ts`
- Create: `web/src/bytes.test.ts`
- Modify: `web/src/backup.ts` (delete `UNITS` and `humanBytes`)
- Modify: `web/src/Backup.tsx` (import moves)
- Modify: `web/src/backup.test.ts` (the `humanBytes` describe block moves out)

**Interfaces:**
- Produces: `humanBytes(bytes: number): string` from `./bytes`.

The confirm has to print a total size and `backup.ts` already owns the only
correct formatter in the tree. Importing the backup client into a curation
screen to reach it would couple two unrelated features through a pure
three-line function, so it moves to a module of its own. The alternative — a
second copy in `stamp.ts` — is the drift `users.ts` refuses for `SessionRow`
in its own module doc.

**GiB, not GB.** C3 §6's illustration reads *"9 archives, 4.1 GB"*; this prints
`4.1 GiB`, because `humanBytes` divides by 1024 and a label that says GB over a
1024-divisor is a figure that disagrees with `ls -l`. The spec's copy is
illustrative and the unit is the honest one.

- [ ] **Step 1: Create `web/src/bytes.ts` with the function moved verbatim**

```ts
/**
 * Sizes, printed for a person deciding whether something will fit.
 *
 * **Its own module rather than `backup.ts`'s**, since #114's confirmation
 * screen prints a total size too and importing the backup client into a
 * curation screen would couple two unrelated features through three pure
 * lines. Moved rather than copied: a second formatter is one that drifts.
 */
const UNITS = ['B', 'KiB', 'MiB', 'GiB', 'TiB']

/**
 * A size in the coarsest binary unit that does not lie about it.
 *
 * **KiB rather than kB**, because the divisor is 1024: a label saying kB over
 * a 1024-divisor is a figure that disagrees with `ls -l`. Bytes are drawn
 * whole; everything above takes one decimal, which is the precision an
 * operator deciding whether a file will fit actually reads.
 */
export function humanBytes(bytes: number): string {
  let size = bytes
  let unit = 0
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024
    unit += 1
  }
  return unit === 0 ? `${size} B` : `${size.toFixed(1)} ${UNITS[unit]}`
}
```

- [ ] **Step 2: Move the tests**

Cut the whole `describe('humanBytes', …)` block out of
`web/src/backup.test.ts`, drop `humanBytes` from that file's import list, and
create `web/src/bytes.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { humanBytes } from './bytes'

describe('humanBytes', () => {
  // The block moved here from `backup.test.ts` when the function moved out of
  // `backup.ts`; the assertions are unchanged.
  it('draws bytes whole, below a kibibyte', () => {
    expect(humanBytes(0)).toBe('0 B')
    expect(humanBytes(512)).toBe('512 B')
  })

  it('takes one decimal above a kibibyte, on a 1024 divisor', () => {
    expect(humanBytes(1024)).toBe('1.0 KiB')
    expect(humanBytes(1536)).toBe('1.5 KiB')
    expect(humanBytes(5 * 1024 * 1024 * 1024)).toBe('5.0 GiB')
  })
})
```

Copy the assertions from `backup.test.ts` as they actually stand rather than
from this block if they differ — the two `it` names above are reconstructed.

- [ ] **Step 3: Delete `UNITS` and `humanBytes` from `web/src/backup.ts`**

Leave `backupUrl` and `readableTaken` where they are; neither moves.

- [ ] **Step 4: Repoint `web/src/Backup.tsx`**

Its `humanBytes` import moves from `'./backup'` to `'./bytes'`. Keep the
remaining `./backup` import for whatever else that file takes.

- [ ] **Step 5: Run the web tests**

Run: `cd web && pnpm vitest run --project node src/bytes.test.ts src/backup.test.ts`
and `cd web && pnpm vitest run --project jsdom src/Backup.test.tsx`
Expected: PASS, and the `humanBytes` cases run from `bytes.test.ts`.

Check the project flag and the test-file name against `web/package.json` and
the tree before running; `--project node` is what `status-test-count.mjs`'s own
comment implies the jsdom suite is called.

- [ ] **Step 6: Commit**

```bash
git add web/src/bytes.ts web/src/bytes.test.ts web/src/backup.ts web/src/backup.test.ts web/src/Backup.tsx
git commit -m "humanBytes moves to bytes.ts, so a second screen can print a size"
```

---

### Task 2: `web/src/stamp.ts` and `can_stamp` on `Me`

**Files:**
- Create: `web/src/stamp.ts`
- Create: `web/src/stamp.test.ts`
- Modify: `web/src/api.ts` (the `Me` interface, near line 146)

**Interfaces:**
- Consumes: `getJson`, `sendJson` from `./api`.
- Produces: `Archive`, `StampPlan`, `Stamped`, `stampPlan(slug)`,
  `performStamp(slug, archives)`, and `Me.can_stamp`.

`/api/me` already sends `can_stamp` — `session.rs`'s `whoami` — and the `Me`
interface is what has not caught up.

- [ ] **Step 1: Write the failing tests**

Create `web/src/stamp.test.ts`, using `users.test.ts`'s own stubbing shape —
a real `Response` per call, and assertions against `RequestInit.body`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import { type Archive, performStamp, stampPlan } from './stamp'

/**
 * A fresh `Response` per call, not one shared instance — `Response.json()`
 * consumes its body stream, so a second read of the same instance throws
 * "Body is unusable". `users.test.ts`, `config.test.ts` and `library.test.ts`
 * carry the same note for the same reason.
 */
const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

const EMPTY = { archives: [], total_bytes: 0, skipped_formats: [] }

const ONE: Archive = {
  member_id: 7,
  edition_key: 'e',
  path: '/lib/v01.cbz',
  bytes: 40,
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('stampPlan', () => {
  it('reads the plan for one work', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      json({ archives: [ONE], total_bytes: 40, skipped_formats: ['rar'] }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const plan = await stampPlan('w')

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/works/w/stamp-comicinfo',
    )
    expect(plan.archives).toHaveLength(1)
    expect(plan.archives[0]?.member_id).toBe(7)
    expect(plan.skipped_formats).toEqual(['rar'])
  })

  it('percent-encodes a slug, rather than letting it reshape the path', async () => {
    const fetchMock = vi.fn().mockResolvedValue(json(EMPTY))
    vi.stubGlobal('fetch', fetchMock)

    await stampPlan('a/b')

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/works/a%2Fb/stamp-comicinfo',
    )
  })

  it('carries an empty plan as an answer, not as a failure', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(json(EMPTY)))

    await expect(stampPlan('w')).resolves.toMatchObject({ archives: [] })
  })
})

describe('performStamp', () => {
  it('sends back the archives it was given, unaltered', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      json({
        rewritten: 1,
        content_key: 'k2',
        document_moved: true,
        sidecar_rewritten: true,
      }),
    )
    vi.stubGlobal('fetch', fetchMock)

    const done = await performStamp('w', [ONE])

    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('POST')
    // **The exact array, byte for byte.** `stamp::apply` compares the body to
    // a freshly computed plan field for field and answers 409 on any
    // difference, so a client that reordered or trimmed a field would be
    // refused every time.
    expect(init.body).toBe(JSON.stringify({ archives: [ONE] }))
    expect(done.content_key).toBe('k2')
  })

  it("surfaces a 409 with the server's own sentence", async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response(
          'a scan or another library job is currently running',
          { status: 409 },
        ),
      ),
    )

    await expect(performStamp('w', [])).rejects.toThrow(
      /a scan or another library job/,
    )
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd web && pnpm vitest run --project node src/stamp.test.ts`
Expected: FAIL — `Failed to resolve import "./stamp"`.

- [ ] **Step 3: Create `web/src/stamp.ts`**

```ts
/**
 * The `/api/library/works/{slug}/stamp-comicinfo` client — spec C3 §6, over
 * the two routes that shipped in #114.
 *
 * **Its own module rather than `metadata.ts`'s**, mirroring the split
 * `taiyaku-serve/src/stamp.rs` made one layer down and for the reason C3 §6
 * gives: this is not a save path — *"putting it on the same Save button would
 * make the irreversible thing a side effect of the reversible one"* — and
 * sharing a module with the four PATCH routes is that mistake repeated in the
 * browser.
 *
 * **The plan is echoed, never rebuilt.** `stamp::apply` compares the POST's
 * body to a freshly computed plan field for field and refuses a mismatch with
 * a 409, so `Archive` is the plan's identity rather than a summary of it and
 * nothing here reduces one to a count. A screen that sent `{ member_ids: [7] }`
 * would be refused every time.
 */
import { getJson, sendJson } from './api'

/** One archive a stamp would rewrite, as both directions carry it. */
export interface Archive {
  member_id: number
  edition_key: string
  path: string
  bytes: number
}

/**
 * What a stamp would do. **An empty `archives` is a 200**, not a 404 —
 * "nothing to do" is an answer, which is either a work with no zip member or
 * one whose every zip already holds the document taiyaku would write.
 */
export interface StampPlan {
  archives: Archive[]
  total_bytes: number
  /**
   * The formats this work holds that a stamp will not write — `rar`, `7z`, or
   * `unreadable` for a member nothing could open. **Distinct from the plan
   * being short:** "4 archives" reads as "all of them" unless the screen can
   * also say the rar was left alone.
   */
  skipped_formats: string[]
}

/** What actually moved. */
export interface Stamped {
  rewritten: number
  content_key: string
  document_moved: boolean
  sidecar_rewritten: boolean
}

const route = (slug: string) =>
  `/api/library/works/${encodeURIComponent(slug)}/stamp-comicinfo`

/** What a stamp of `slug` would rewrite. Writes nothing. */
export const stampPlan = (slug: string): Promise<StampPlan> =>
  getJson<StampPlan>(route(slug))

/**
 * Do it.
 *
 * A refusal arrives as a thrown `Error` carrying the server's own sentence: a
 * 409 is either the plan no longer matching the library or a scan already
 * running, and both are worth printing verbatim rather than translating.
 */
export async function performStamp(
  slug: string,
  archives: Archive[],
): Promise<Stamped> {
  const body = await sendJson<Stamped>(route(slug), 'POST', { archives })
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}
```

- [ ] **Step 4: Add `can_stamp` to `Me` in `web/src/api.ts`**

The interface at ~line 146 becomes:

```ts
/** Who the server resolved this browser to, or `Unauthenticated`. */
export interface Me {
  user_id: number
  admin: boolean
  can_download: boolean
  can_stream: boolean
  /**
   * **Not implied by `admin`.** Every other capability here reads
   * `admin || column` in `Principal::from_account`; this one reads the column
   * alone — the one documented exception to O86, because it decides whether an
   * account may rewrite files taiyaku does not own. A screen that treated an
   * administrator as holding it would offer a control the server answers 403.
   */
  can_stamp: boolean
}
```

- [ ] **Step 5: Run the tests and the type check**

Run: `cd web && pnpm vitest run --project node src/stamp.test.ts && pnpm tsc --noEmit`
Expected: PASS, and no type error from the new `Me` field.

If `tsc` reports a `Me` literal somewhere in the tests that now lacks
`can_stamp`, add `can_stamp: false` to it — that is the honest default and it
keeps every existing test asserting what it already asserted.

- [ ] **Step 6: Hand-apply the defects and watch each fail**

One at a time; restore before the next.

| Defect | Test that must fail |
|---|---|
| Drop `encodeURIComponent` from `route` | percent-encodes a slug |
| `sendJson(route(slug), 'POST', archives)` — the bare array, not `{ archives }` | sends back the archives it was given |
| `getJson` a 404 on an empty plan (throw when `archives.length === 0`) | carries an empty plan as an answer |

Record the three outcomes in the commit body.

- [ ] **Step 7: Commit**

```bash
git add web/src/stamp.ts web/src/stamp.test.ts web/src/api.ts
git commit -m "the stamp client, and Me catches up with what /api/me already sends"
```

---

### Task 3: the section, the control, and the plan it draws

**Files:**
- Modify: `web/src/MetadataEditor.tsx`
- Modify: `web/src/MetadataEditor.test.tsx`

**Interfaces:**
- Consumes: `stampPlan`, `type StampPlan`, `type Archive` from `./stamp`;
  `humanBytes` from `./bytes`.
- Produces: `MetadataEditor`'s `canStamp?: boolean` prop. Task 5 passes it.

No POST yet: this task ends with a screen that can say what a stamp would do
and has no way to start one. That is a reviewable unit on its own, and it is
the half where the copy lives.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/MetadataEditor.test.tsx`. Add at the top of the file, beside
the existing imports:

```ts
import * as stamp from './stamp'

// `AcceptForm.test.tsx`'s shape, for the reason its own comment gives.
// `humanBytes` is deliberately not mocked: `bytes.test.ts` pins it, and
// stubbing it here would leave this file unable to tell a section that calls
// it from one that prints its own number.
vi.mock('./stamp', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./stamp')>()),
  stampPlan: vi.fn(),
  performStamp: vi.fn(),
}))

const noop = vi.fn().mockResolvedValue({ title: 't', persisted: true })

const planButton = (host: HTMLElement) =>
  host.querySelector<HTMLButtonElement>("button[name='plan-stamp']")

/** One archive's row. Queried so assertions are per element, never page-wide. */
const archiveRows = (host: HTMLElement) =>
  [...host.querySelectorAll('.stamp-comicinfo li.archive')]
```

Then the block:

```tsx
describe('MetadataEditor, the ComicInfo stamp', () => {
  const PLAN = {
    archives: [
      {
        member_id: 7,
        edition_key: 'e1',
        path: '/lib/Series - v01.cbz',
        bytes: 40 * 1024 * 1024,
      },
      {
        member_id: 9,
        edition_key: 'e1',
        path: '/lib/Series - v02.cbz',
        bytes: 2 * 1024 * 1024 * 1024,
      },
    ],
    total_bytes: 40 * 1024 * 1024 + 2 * 1024 * 1024 * 1024,
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
    expect(rows[0]?.textContent).toContain('40.0 MiB')
    expect(rows[1]?.textContent).toContain('/lib/Series - v02.cbz')
    expect(rows[1]?.textContent).toContain('2.0 GiB')
  })

  it('states the count and the total before anything starts', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const total = host.querySelector('.stamp-comicinfo .total')
    expect(total?.textContent).toContain('2 archives')
    expect(total?.textContent).toContain('2.0 GiB')
  })

  it('states the three consequences in the terms that bite', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)

    const said = host.querySelector('.stamp-comicinfo .consequences')?.textContent
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

    const said = host.querySelector('.stamp-comicinfo .consequences')?.textContent
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
  })
})
```

`beforeEach` may need adding to this file's `vitest` import list.

- [ ] **Step 2: Run them and watch them fail**

Run: `cd web && pnpm vitest run --project jsdom src/MetadataEditor.test.tsx`
Expected: FAIL — every new case, on a missing `canStamp` prop and a missing
section. The pre-existing cases in the file must still pass.

- [ ] **Step 3: Add the imports and the prop**

At the top of `web/src/MetadataEditor.tsx`, extend the imports:

```ts
import { type FormEvent, useState } from 'react'
import { humanBytes } from './bytes'
import { type PatchResult, reasons, type WorkEdit } from './metadata'
import { type Archive, type StampPlan, stampPlan } from './stamp'
```

Extend the props, keeping the existing three:

```tsx
  onPatch,
  canStamp = false,
}: {
  slug: string
  title: string
  onPatch: (slug: string, fields: WorkEdit) => Promise<PatchResult>
  /**
   * Whether to offer the ComicInfo write-back at all — spec C3 §6.
   *
   * **The caller asserts two things by passing `true`**, which is why this is
   * a prop rather than a `useGate()` read here: that the account holds
   * `can_stamp`, and that `slug` is a *work* slug. The second matters because
   * this component's own doc is explicit that `slug` is a loose name for
   * whatever identifier `onPatch` takes, and the stamp routes take a work's.
   *
   * **Absent, not disabled**, per C3 §6 and `Series.tsx`'s `mayDownload`: a
   * disabled control invites a person to find out why and nothing on this
   * screen could tell them.
   */
  canStamp?: boolean
}) {
```

- [ ] **Step 4: Add the state and the fetch**

Below the existing `useState` calls:

```tsx
  /**
   * The plan, once asked for. Three states, like `unwritten` above:
   * `undefined` before any press, and after a press until the answer lands —
   * the two are told apart by `asking`.
   */
  const [plan, setPlan] = useState<StampPlan>()
  const [asking, setAsking] = useState(false)
  const [planFailure, setPlanFailure] = useState<string>()

  const askPlan = () => {
    setAsking(true)
    setPlan(undefined)
    setPlanFailure(undefined)
    stampPlan(slug)
      .then((answer) => {
        setAsking(false)
        setPlan(answer)
      })
      .catch((err: unknown) => {
        setAsking(false)
        setPlanFailure(err instanceof Error ? err.message : String(err))
      })
  }
```

**No sequence counter here, unlike `UserRow.askCost`.** That one guards two
dry runs racing because Remove…/Cancel/Remove… can put two in flight; here the
control is replaced by the plan the moment it answers and there is no cancel
back to it, so a second press is only reachable after a failure, by which time
the first has already settled. If Task 4 adds a way back to the control, this
comment is the thing to revisit.

- [ ] **Step 5: Return a fragment, and render the section after the form**

Wrap the existing `return (…)` body: `<>` … `</>` with the current `<form>`
unchanged as the first child, then:

```tsx
      {canStamp && (
        <section className="stamp-comicinfo" aria-label="Write ComicInfo.xml">
          <h3>Write ComicInfo.xml into this work's archives</h3>
          {plan === undefined && !asking && (
            <button type="button" name="plan-stamp" onClick={askPlan}>
              What would this change?
            </button>
          )}
          {asking && <p>Working out which archives would change…</p>}
          {planFailure !== undefined && (
            <p role="alert">{planFailure}</p>
          )}
          {plan !== undefined && plan.archives.length === 0 && (
            <p className="nothing">
              Nothing to write: every zip archive in this work already holds the
              ComicInfo.xml taiyaku would write, or this work has no zip archive
              at all.
            </p>
          )}
          {plan !== undefined && plan.archives.length > 0 && (
            <>
              <p className="total">
                This rewrites {plan.archives.length}{' '}
                {plan.archives.length === 1 ? 'archive' : 'archives'},{' '}
                {humanBytes(plan.total_bytes)} in all. A zip cannot be edited in
                place, so each one is copied whole and renamed over the
                original.
              </p>
              <ul className="archives">
                {plan.archives.map((archive: Archive) => (
                  <li className="archive" key={archive.member_id}>
                    <span className="path">{archive.path}</span>{' '}
                    <span className="size">{humanBytes(archive.bytes)}</span>
                  </li>
                ))}
              </ul>
              <div className="consequences">
                <p>
                  Every archive above gets a new sha256, and three things follow
                  from that:
                </p>
                <ul>
                  <li>
                    any alignment or labelling recorded against these members is
                    treated as stale until the next scan, because the staleness
                    check compares exactly this digest;
                  </li>
                  <li>
                    every thumbnail already generated for these members is
                    regenerated the next time it is asked for;
                  </li>
                  <li>
                    this work's content_key changes, and its
                    &lt;content_key&gt;.library.json moves with it.
                  </li>
                </ul>
                <p className="irreversible">
                  There is no undo: the original files are replaced, not kept.
                  If this is interrupted partway, the archives already rewritten
                  stay rewritten and their rows stay stale until the next scan.
                </p>
              </div>
              {plan.skipped_formats.length > 0 && (
                <p className="skipped">
                  Left alone: this work also holds{' '}
                  {plan.skipped_formats.join(', ')}, which taiyaku reads and
                  never writes.
                </p>
              )}
            </>
          )}
        </section>
      )}
```

The confirm button is Task 4; this section currently ends after the warnings.

- [ ] **Step 6: Run the tests**

Run: `cd web && pnpm vitest run --project jsdom src/MetadataEditor.test.tsx`
Expected: PASS, all cases including the file's pre-existing ones.

- [ ] **Step 7: Hand-apply the defects and watch each fail**

| Defect | Test that must fail |
|---|---|
| Render the section without the `canStamp &&` guard | has no section at all |
| Call `askPlan()` from a `useEffect` on mount | reads no plan until it is pressed |
| Drop `type="button"` from the plan control | does not save the title |
| Render one `<li>` holding every path and every size | names every archive with its own size |
| `humanBytes(plan.archives[0].bytes)` for the total | states the count and the total |
| Drop the `.skipped` block entirely | names the formats it is leaving alone |
| Render `.skipped` unconditionally | says nothing about skipped formats when there are none |
| Drop the `.irreversible` paragraph | says an interrupted run leaves the archives |

The fourth is the one this repository's record says matters: apply it and
confirm the *page-wide* text would still contain every string, so only the
per-element assertion catches it.

- [ ] **Step 8: Commit**

```bash
git add web/src/MetadataEditor.tsx web/src/MetadataEditor.test.tsx
git commit -m "the stamp section says what it would rewrite, and cannot yet do it"
```

---

### Task 4: the confirm, the POST, and what came back

**Files:**
- Modify: `web/src/MetadataEditor.tsx`
- Modify: `web/src/MetadataEditor.test.tsx`

**Interfaces:**
- Consumes: `performStamp`, `type Stamped` from `./stamp`.

- [ ] **Step 1: Write the failing tests**

Append inside the same `describe('MetadataEditor, the ComicInfo stamp')`
block, and add beside the other queries at the top of the file:

```ts
const stampButton = (host: HTMLElement) =>
  host.querySelector<HTMLButtonElement>("button[name='confirm-stamp']")
```

```tsx
  const DONE = {
    rewritten: 2,
    content_key: 'k2',
    document_moved: true,
    sidecar_rewritten: true,
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

  it('offers no confirm for an empty plan', async () => {
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

  it('reports what moved once the stamp lands', async () => {
    vi.mocked(stamp.stampPlan).mockResolvedValue(PLAN)
    vi.mocked(stamp.performStamp).mockResolvedValue(DONE)
    const host = mount(
      <MetadataEditor slug="w" title="t" onPatch={noop} canStamp />,
    )

    await click(planButton(host)!)
    await click(stampButton(host)!)

    const done = host.querySelector('.stamp-comicinfo .stamped')
    expect(done?.textContent).toContain('2')
    expect(done?.textContent).toContain('k2')
    expect(stampButton(host)).toBeNull()
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
```

`act` and `useRef` may need adding to the test file's and component's import
lists respectively.

- [ ] **Step 2: Run them and watch them fail**

Run: `cd web && pnpm vitest run --project jsdom src/MetadataEditor.test.tsx -t 'ComicInfo stamp'`
Expected: FAIL — no `button[name='confirm-stamp']` exists, so `stampButton`
answers `null` and the `!` assertions throw.

- [ ] **Step 3: Add the state, the ref and the send**

Extend the component's React import to `import { type FormEvent, useRef, useState } from 'react'`,
add `performStamp, type Stamped` to the `./stamp` import, and add below
`askPlan`:

```tsx
  const [stamped, setStamped] = useState<Stamped>()
  const [sending, setSending] = useState(false)
  const [stampFailure, setStampFailure] = useState<string>()
  /**
   * Whether a stamp is already on its way.
   *
   * **A ref as well as `sending`, for the reason every other guarded press in
   * this codebase carries one**: React batches state, so two presses
   * dispatched before the next render both read `sending === false` and
   * `disabled` has not reached the DOM. On this control that is not a
   * cosmetic double-send — the second POST arrives against a library the
   * first has already moved, so it is answered 409 by the plan-mismatch guard
   * and reads to the person as a stamp that failed.
   */
  const inFlight = useRef(false)

  const confirmStamp = () => {
    if (inFlight.current || plan === undefined) return
    inFlight.current = true
    setSending(true)
    setStampFailure(undefined)
    // **`plan.archives`, not a rebuild of it.** `stamp::apply` compares the
    // body to a freshly computed plan field for field; anything reconstructed
    // here is a 409 waiting to happen.
    performStamp(slug, plan.archives)
      .then((answer) => {
        inFlight.current = false
        setSending(false)
        setStamped(answer)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setSending(false)
        setStampFailure(err instanceof Error ? err.message : String(err))
      })
  }
```

- [ ] **Step 4: Render the confirm and the outcome**

Inside the `plan.archives.length > 0` fragment, after the `.skipped`
paragraph:

```tsx
              {stamped === undefined && (
                <button
                  type="button"
                  name="confirm-stamp"
                  onClick={confirmStamp}
                  disabled={sending}
                >
                  {sending
                    ? 'Rewriting…'
                    : `Rewrite ${plan.archives.length} ${
                        plan.archives.length === 1 ? 'archive' : 'archives'
                      }`}
                </button>
              )}
              {stampFailure !== undefined && (
                <p role="alert">{stampFailure}</p>
              )}
              {stamped !== undefined && (
                <p className="stamped">
                  Rewrote {stamped.rewritten}{' '}
                  {stamped.rewritten === 1 ? 'archive' : 'archives'}. This work's
                  content_key is now {stamped.content_key}
                  {stamped.document_moved
                    ? ', and its document moved with it.'
                    : '. It had no document on disk to move.'}
                </p>
              )}
```

- [ ] **Step 5: Run the tests**

Run: `cd web && pnpm vitest run --project jsdom src/MetadataEditor.test.tsx`
Expected: PASS, every case in the file.

- [ ] **Step 6: Hand-apply the defects and watch each fail**

| Defect | Test that must fail |
|---|---|
| `performStamp(slug, plan.archives.map((a) => ({ ...a, bytes: 0 })))` | sends back exactly the archives |
| Render the confirm outside the `archives.length > 0` branch | offers no confirm for an empty plan |
| **Delete `inFlight` and keep `disabled={sending}`** | sends one stamp for two presses |
| Drop the `.stamped` paragraph's `content_key` | reports what moved |
| Clear `plan` in the `.catch` | surfaces a 409 … keeps the plan on screen |

The third is the one to watch most carefully: confirm it fails. If it passes,
the test is dispatching its two clicks across two renders and is proving
`disabled` — the exact #112 shape — and must be rewritten before this task is
offered.

- [ ] **Step 7: Commit**

```bash
git add web/src/MetadataEditor.tsx web/src/MetadataEditor.test.tsx
git commit -m "the confirm, which sends back the plan it was shown"
```

---

### Task 5: `Series.tsx` decides the section exists

**Files:**
- Modify: `web/src/Series.tsx` (the gate reads at ~line 327, the mount at ~line 634)
- Modify: `web/src/Series.test.tsx` — check the file exists and what it already
  mounts; if it does not, this task's test goes in `MetadataEditor.test.tsx`
  as a note that no caller passes `canStamp` yet, and this step says so in the
  commit body rather than inventing a suite.

- [ ] **Step 1: Add the gate read**

Beside `mayDownload` and `mayEdit`:

```tsx
  // **The column alone, never `admin`.** `can_stamp` is the one capability
  // `Principal::from_account` does not fold `admin` into — the documented O86
  // exception — so an administrator without it is offered nothing here, which
  // is exactly the account §1.1 exists to make possible. `mayEdit` already
  // gates the editor itself on `admin`, so this is the second of two.
  const mayStamp = useGate()?.who.can_stamp === true
```

- [ ] **Step 2: Pass it at the mount**

```tsx
                      <MetadataEditor
                        slug={w.slug}
                        title={w.title}
                        onPatch={patchWork}
                        canStamp={mayStamp}
                      />
```

- [ ] **Step 3: Run the suite and the type check**

Run: `cd web && pnpm vitest run --project node && pnpm vitest run --project jsdom && pnpm tsc --noEmit`
Expected: PASS. Any `Me` literal in a `Series` or gate test that now fails to
type gains `can_stamp: false`.

- [ ] **Step 4: Commit**

```bash
git add web/src/Series.tsx web/src/Series.test.tsx
git commit -m "the series page offers a stamp only to an account holding can_stamp"
```

---

### Task 6: `UserRow`'s third checkbox

**Files:**
- Modify: `web/src/users.ts` (`User`, `UserPatch`)
- Modify: `web/src/UserRow.tsx` (~line 400, after `can_stream`)
- Modify: `web/src/UserRow.test.tsx`
- Modify: `web/src/users.test.ts`

This is what makes the capability grantable. Until it lands an administrator
has to `PATCH /api/library/users/{id}` by hand — the intended shape of a
server-half PR, and the reason this task is not optional.

- [ ] **Step 1: Write the failing tests**

In `web/src/UserRow.test.tsx`, using the file's own `user()` factory, its
`show()` mount helper and its `control()` query — not a second fixture shape:

```tsx
  it('offers can_stamp as a third capability, and sends it on its own', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user', can_stamp: false })
    const { host } = await show(other, [me, other])

    const box = control(host, 'input[name="can_stamp"]') as HTMLInputElement
    expect(box).not.toBeNull()
    expect(box.checked).toBe(false)

    await click(box)

    // On its own: a save that also resent `can_stream` would write a column
    // nobody touched, and an absent key is what the route reads as "leave
    // this alone".
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { can_stamp: true })
  })

  it('draws can_stamp unticked for an administrator who does not hold it', async () => {
    // **The one capability `role` does not dominate** — the documented O86
    // exception, because it decides whether an account may rewrite archives
    // taiyaku does not own rather than widening what it may read. A card that
    // drew this ticked because the account is an admin would state a
    // permission the server answers 403.
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'admin', can_stamp: false })
    const { host } = await show(other, [me, other])

    expect(
      (control(host, 'input[name="can_stamp"]') as HTMLInputElement).checked,
    ).toBe(false)
  })
```

Read the file's existing `can_download` cases before writing these and follow
whatever they actually do; the names above (`user`, `show`, `control`, `click`,
`usersApi`) are read from the file as it stands, but its `show()` returns
`{ host, changed }` and takes `(who, everyone, over)`, so check the call shape
rather than trusting this transcription.

In `web/src/users.test.ts`, beside the existing `patchUser` case at ~line 116:

```ts
  it('passes can_stamp through to the PATCH body', async () => {
    const fetchMock = vi.fn().mockResolvedValue(json({ user: user() }))
    vi.stubGlobal('fetch', fetchMock)

    await patchUser(3, { can_stamp: true })

    const init = fetchMock.mock.calls[0]?.[1] as RequestInit
    expect(init.method).toBe('PATCH')
    expect(init.body).toBe(JSON.stringify({ can_stamp: true }))
  })
```

**Both files have one `user()` factory**, so `can_stamp: false` is added there
once rather than to every fixture — and `false` is the honest default, since
migration 013 defaults the column to 0 and nobody gained it on the upgrade.

- [ ] **Step 2: Run them and watch them fail**

Run: `cd web && pnpm vitest run --project jsdom src/UserRow.test.tsx` and
`cd web && pnpm vitest run --project node src/users.test.ts`
Expected: FAIL — `can_stamp` is not on `User`, so the fixtures do not type and
no such input exists.

- [ ] **Step 3: Extend the types in `web/src/users.ts`**

In `User`, after `can_stream`:

```ts
  /**
   * **The one capability `role` does not imply.** Every other capability on
   * this row is read as `admin || column` by `Principal::from_account`; this
   * one is the column alone — the documented exception to O86, because it
   * decides whether an account may rewrite archives taiyaku does not own
   * rather than widening what it may read. Default 0, so nobody gained it on
   * the upgrade that added it.
   */
  can_stamp: boolean
```

In `UserPatch`, after `can_stream`:

```ts
  can_stamp?: boolean
```

- [ ] **Step 4: Add the checkbox in `web/src/UserRow.tsx`**

After the `can_stream` label, inside the same `<div className="fields">`:

```tsx
        <label>
          <input
            type="checkbox"
            name="can_stamp"
            checked={shown.can_stamp}
            disabled={saving}
            onChange={(e) => save({ can_stamp: e.target.checked })}
          />
          <span>May write ComicInfo.xml into archives</span>
        </label>
```

Then extend the card's module doc comment, which currently names two
capability boxes in the `removed` tombstone paragraph — search that file for
"both capability boxes" and make it three.

- [ ] **Step 5: Run the tests**

Run: `cd web && pnpm vitest run --project jsdom src/UserRow.test.tsx`,
`cd web && pnpm vitest run --project node src/users.test.ts`, and `pnpm tsc --noEmit`
Expected: PASS. The two `user()` factories carry `can_stamp: false`, which
keeps every existing case asserting exactly what it already asserted. If `tsc`
names a `User` literal built inline somewhere else, add `can_stamp: false`
there too.

- [ ] **Step 6: Hand-apply the defects and watch each fail**

| Defect | Test that must fail |
|---|---|
| `checked={shown.can_stamp \|\| shown.role === 'admin'}` | draws can_stamp unticked for an administrator |
| `save({ can_stamp: e.target.checked, can_stream: true })` | sends it on its own |
| Drop `can_stamp` from `UserPatch` | passes can_stamp through (a type error is a fail) |

- [ ] **Step 7: Commit**

```bash
git add web/src/users.ts web/src/UserRow.tsx web/src/UserRow.test.tsx web/src/users.test.ts
git commit -m "an administrator can grant can_stamp without writing a PATCH by hand"
```

---

### Task 7: the documents

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/superpowers/specs/2026-08-14-library-curation-design.md` (§6, and §7 if its marker still reads as half-shipped)
- Modify: `docs/superpowers/specs/2026-09-04-c4-screens-design.md` (§6.2, ~line 545)

**Not touched, because PR A already did them:** `README.md`, `docs/notes.md`,
`docs/mutants-baseline.md`, `2026-08-14-library-auth-design.md`,
`2026-08-14-library-store-design.md` §10. Confirm with
`git show --stat 7d323e5` before editing any of them.

- [ ] **Step 1: `2026-08-14-library-curation-design.md` §6**

Replace the standing note at the top of §6 — *"the confirmation screen is PR B
and is still to come, so nothing in the UI reaches these routes yet"* — with a
shipped marker naming this PR's number once it exists. Until then write the
date and leave the number for the merge:

```markdown
**Shipped 2026-09-07**, built to
[`2026-09-07-comicinfo-write-back-design.md`](2026-09-07-comicinfo-write-back-design.md)
in two PRs: the server half (#114) and the confirmation screen. The screen is a
section of `MetadataEditor`, below Save and outside its `<form>` — a button
inside a form submits it, which would have made the irreversible thing a
literal side effect of the reversible one.
```

Re-read §7's own marker in the same pass and correct it if it still reads as
partial.

- [ ] **Step 2: `2026-09-04-c4-screens-design.md` §6.2**

The sentence at ~line 545 gains the third capability:

```markdown
One card per account: name, role, `can_download`, `can_stream`, **`can_stamp`**
— the one `role` does not imply, since it decides whether the account may
rewrite archives taiyaku does not own rather than widening what it may read
(the documented O86 exception) — granted roots, and **`can_log_in`** …
```

Keep the rest of the sentence as it stands.

- [ ] **Step 3: `docs/status.md`**

Three edits:

1. The `web/` row of "What is built" gains the section: one sentence saying
   `MetadataEditor` carries the ComicInfo confirmation, that it is below Save
   and outside the form, and that it is absent rather than disabled without
   `can_stamp`.
2. The C3 line that says the confirmation screen is still to come.
3. The test counts on line 121 — `web/`'s figure, and the per-project split
   that follows it.

**Do not hand-count.** `scripts/status-test-count.mjs` runs inside
`check-all.sh` and fails the gate on a stale figure; run the gate, read the
number it reports, and write that.

- [ ] **Step 4: Run the doc-link check and the gate**

Run: `./scripts/check-all.sh`
Expected: the final line is exactly `all green`. Quote that line in full —
piping through `tail` or appending `; echo` masks the exit status.

- [ ] **Step 5: Commit**

```bash
git add docs/
git commit -m "the documents the confirmation screen changes"
```

---

## Before the PR

- [ ] **The gate.** `./scripts/check-all.sh`, and quote the `all green` line.
- [ ] **A whole-branch review, then a review of its own fixes.** Per-task
  review has passed on every task of every branch this project has shipped
  while a whole-branch pass found seven Criticals on #91 and two on #105; on
  #108 four passes found twenty-nine and did not converge, and on #112 passes
  2–4 found *only* defects inside earlier fixes. The number of rounds is
  decided by what the last one found. After fixing a finding, grep for its
  *shape* rather than its function.
- [ ] **The mutation sweep, and what it means here.** `scripts/mutants.sh
  --since main` diffs commits, not the worktree, so commit first. **This
  branch is `web/` and `docs/` only, so the sweep has no Rust diff to sweep** —
  a clean run is the expected and uninformative result, and reporting it as
  evidence of anything would be a claim the run cannot support. Say so in the
  PR body rather than printing a zero.
- [ ] **Drive it against real material.** The gate does not open a browser.
  Serve a copy of the real library — never the library itself — and confirm on
  screen: the section absent for an admin without `can_stamp`, the plan naming
  real paths and a real total, and a stamp landing. Isolate the writes and
  prove it afterwards.
- [ ] **The PR goes to the Gitea MCP, not `gh`.** Never hard-wrap the body;
  the body *is* the squash commit message and appears once.

## Self-review of this plan against the spec

**§6, the screen.** A section in `MetadataEditor` below Save — Task 3, and the
fragment split is argued above rather than assumed. `UserRow`'s *the confirm
does not exist until the counts do* — Task 3's control, Task 4's confirm. Every
archive and the total — Task 3. The three consequences — Task 3. The rar/7z
members left alone — Task 3, both branches. Absent, not disabled — Task 3 for
`can_stamp`, Task 4 for the empty plan, with the ambiguity resolved above.
`UserRow`'s third checkbox and `users_api`'s `PATCH` — Task 6; the server half
of that sentence shipped in PR A and is verified rather than rebuilt.

**§7, what this does not do.** No bulk stamp: nothing here reaches beyond one
work's slug. No un-stamp: no control offers one, and the copy says the
originals are replaced. "It does not close the crash window … stated on screen,
not only here" — Task 3's `.irreversible` paragraph, with its own test.

**§9, documents.** Task 7 covers the three this PR owns and names the five PR A
already did, with the command to verify that claim rather than trusting it.

**§11, what could not be measured.** The NAS plan cost is the reason the plan
is fetched on press and not on mount; that is a Global Constraint and the
reason is recorded at Task 3's `useEffect` defect.

**§12, branch shape.** PR B is the screen and the docs that describe it. The
gate, a whole-branch review and a sweep run on this PR, with the sweep's
emptiness stated rather than dressed up.

**Types.** `Archive`, `StampPlan`, `Stamped`, `stampPlan`, `performStamp`,
`canStamp`, `mayStamp`, `can_stamp` — each spelled identically at every use
above. `humanBytes` is imported from `./bytes` in all three consumers.
