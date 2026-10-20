import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import { forgetAbout } from './about'
import { Account } from './Account'
import './index.css'
import { ABOUT } from './testing/about'
import { mount } from './testing/mount'

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

/**
 * **Keys and sessions, because the sweep below can only see what renders.**
 * With both lists empty the panel draws no key row, no session row, no
 * `Revoke` and neither `<span className={look.meta}>` — and `carries no
 * inline style attribute anywhere` stayed green with
 * `style={{ opacity: 0.6 }}` put back on the session's "last seen" span,
 * which is the whole defect it names. The unstubbed `fetch` here reaches
 * Vite's proxy and is refused by a `serve` that is not running, so the
 * component's own `.catch` left both lists at `[]`.
 *
 * **Two of each rather than one**, so that `keeps each stacked row off the
 * one above it` measures a row following another row *of the same list* —
 * the repeat a `.map` draws. With one key and one session the only
 * boundaries on screen are the create form above the first key row and the
 * `<h3>` above the first session row, and a separation reaching the first
 * item of each list and nothing after it would measure exactly the same.
 *
 * **And `/api/about`**, so the catalog's address the screen draws beside the
 * OPDS line is on screen for the sweep and the checks below as well. Left
 * unanswered, the panel draws the OPDS line alone.
 */
const server = () =>
  vi.fn((input: string, init?: RequestInit) => {
    // Matched on the method as well as the path, because `POST /api/keys`
    // and `GET /api/keys` are different answers — the path alone handed
    // `createKey` the *list* of keys, whose `.secret` is `undefined`, and
    // the panel then drew a secret box holding nothing.
    if (input === '/api/keys' && (init?.method ?? 'GET') === 'POST') {
      return Promise.resolve(json({ secret: 'tyk_the-only-copy' }))
    }
    if (input === '/api/keys') {
      return Promise.resolve(
        json([
          { id: 1, name: 'Panels', created: 'then', last_used: 'today' },
          { id: 2, name: 'Chunky', created: 'then', last_used: null },
        ]),
      )
    }
    if (input === '/api/about') return Promise.resolve(json(ABOUT))
    if (input === '/api/sessions') {
      return Promise.resolve(
        json([
          {
            id: 'aa',
            created: 'then',
            last_seen: 'today',
            user_agent: 'Firefox',
            ip: null,
            current: true,
          },
          {
            id: 'bb',
            created: 'then',
            last_seen: 'yesterday',
            user_agent: 'Safari',
            ip: null,
            current: false,
          },
        ]),
      )
    }
    return Promise.resolve(new Response(null, { status: 404 }))
  })

/**
 * **The account panel is one of the reader's dialogs, measured rather than
 * asserted by class name.** ③ drew Settings as `.reader-dialog-panel` — Help, the
 * note list and Credits are `.docked-panel`, a different shape; this one was 25 inline `style=` props drawing a
 * different dialog — 12px corners against 0, a drop shadow against none,
 * `rgba(24,24,24,0.96)` against `--reader-panel`, 16px body against 14, a
 * sans title against a serif one. ⑤c makes it the same panel.
 *
 * In the browser project because jsdom computes no styles: a class-name
 * assertion passes against a panel whose rules never loaded, which is the
 * failure this file exists to rule out.
 *
 * **Mounted inside `<div className="reader">`, which is load-bearing rather
 * than scenery.** `App` renders `<Account>` as a sibling of `<Settings>`
 * inside that element, and `reader.css` sets `color-scheme: dark` on it. The
 * same classes outside it would resolve `--text` to its *light* value over a
 * fixed dark ground. A fixture that mounted the panel bare would pass every
 * assertion below and prove nothing about the screen.
 */
async function panel(): Promise<HTMLElement> {
  const host = mount(
    <div className="reader">
      <Account visible onClose={() => {}} onSignedOut={() => {}} />
    </div>,
  )
  // The key list and the session list arrive over two separate promise
  // chains, and a fixed number of microtask ticks flushed the first and not
  // the second — the keys row rendered, the session row did not, and React
  // warned about an update outside `act`. A macrotask flushes both.
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })
  const el = host.querySelector<HTMLElement>('.reader-dialog-panel')
  if (!el) throw new Error('no .reader-dialog-panel rendered')
  return el
}

describe('the account panel', () => {
  /**
   * **A desktop viewport, and the width assertion below needs it to mean
   * anything.** `.reader-dialog-panel` is `min(480px, 90vw)`, and the browser
   * project's default viewport is 414px wide — where that resolves to 373px
   * and the 480 cap is never reached. A panel widened back to the old
   * `min(560px, 90vw)` would measure 373 there too, so the assertion would
   * stay green under the exact change it exists to catch. 1280 x 900 is the
   * desktop `Config.browser.test.tsx` uses.
   */
  beforeEach(async () => {
    vi.stubGlobal('fetch', server())
    await page.viewport(1280, 900)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    forgetAbout()
    // Beside the global cleanup rather than at the end of the one case that
    // sets it: removed after the assertion, a failed assertion left `paper`
    // on the document for every case after it, and each of those resolves
    // its colours against the theme this attribute names.
    document.documentElement.removeAttribute('data-theme')
  })

  it('is drawn as a reader dialog, not as its own', async () => {
    const el = await panel()
    const style = getComputedStyle(el)
    // The four the old panel differed on, read off the box rather than the
    // class list.
    expect(style.borderRadius).toBe('0px')
    expect(style.boxShadow).toBe('none')
    expect(Math.round(el.getBoundingClientRect().width)).toBe(480)
    expect(style.fontSize).toBe('14px')

    const scrim = el.closest<HTMLElement>('.reader-dialog')
    expect(scrim).not.toBeNull()

    // **The reader's scrim, stated as what `--scrim` resolves to here.**
    // This was `not.toBe('rgba(0, 0, 0, 0.6)')`, the old hand-mixed black,
    // and a negative assertion against one literal is green against a scrim
    // with no background at all: `background: transparent` on
    // `.reader-dialog` passed it.
    //
    // The token cannot be compared as text — a custom property's value is
    // `light-dark(#231e1759, #0000008c)` while a computed `background-color`
    // is `rgba(0, 0, 0, 0.55)` — so it goes through a probe, mounted inside
    // the scrim so that `color-scheme` and everything else it inherits are
    // the ones the real element resolves against.
    const probe = document.createElement('div')
    probe.style.backgroundColor = getComputedStyle(scrim!).getPropertyValue(
      '--scrim',
    )
    scrim!.append(probe)
    const scrimToken = getComputedStyle(probe).backgroundColor
    probe.remove()
    // **The probe resolved to a colour**, which is what stops the assertion
    // below from being vacuous: with `--scrim` gone the probe would be
    // transparent too, and a scrim drawing nothing would compare equal to it.
    expect(scrimToken).not.toBe('rgba(0, 0, 0, 0)')
    expect(getComputedStyle(scrim!).backgroundColor).toBe(scrimToken)
  })

  it('resolves its colours dark although the document is light', async () => {
    // The containment check, stated as a colour rather than as a DOM path:
    // outside `.reader` this computes the *light* `--text` over a fixed dark
    // ground, which is the failure `LOOKS` was written to avoid, inverted.
    document.documentElement.setAttribute('data-theme', 'paper')
    const el = await panel()
    expect(getComputedStyle(el).color).toBe('rgb(236, 228, 212)')
    // The catalog's address sets no colour of its own (`admin.css`), so it
    // takes the panel's.
    const address = el.querySelector('.catalog-address-text')
    expect(address).not.toBeNull()
    expect(getComputedStyle(address!).color).toBe('rgb(236, 228, 212)')
    // The attribute comes off in `afterEach`, not here: this line is not
    // reached when the assertion above fails.
  })

  it('gives its fields the panel’s own font size, which is what puts them inside O151’s floor', async () => {
    const field = (await panel()).querySelector<HTMLInputElement>(
      'input[type="password"]',
    )
    expect(field).not.toBeNull()
    // 14px, inherited from the panel — not 16px by accident of never having
    // been given a size. The coarse-pointer floor above it is what takes it
    // back to 16 on a phone, and `base.css.test.ts` pins that pairing.
    expect(getComputedStyle(field!).fontSize).toBe('14px')
  })

  it('carries no inline style attribute anywhere', async () => {
    const el = await panel()
    // **The rows are on screen before the sweep runs.** Both lists empty is
    // the state this fixture had first, and there the sweep visits only the
    // frame — every element that ever carried `rowStyle` or
    // `{ opacity: 0.6 }` is a key row or a session row, so it would report
    // `[]` whatever those rows held.
    expect(el.textContent).toContain('Panels')
    expect(el.textContent).toContain('Firefox')

    const styled = [...el.querySelectorAll('[style]')].map(
      (node) => `${node.tagName}: ${node.getAttribute('style')}`,
    )
    expect(styled).toEqual([])
  })

  /**
   * **The rows this panel stacks, which ③'s panels never did.**
   * `.reader-section-head` is ③'s row and carries no margin, because
   * `Settings.tsx` draws one per section under the section's own rule.
   * Here they stack — the create form and a row per key; a row per session —
   * and taking that class alone closed every gap between them: four of the
   * boundaries measured below all read 0 px at this viewport, each row's top
   * exactly on the previous row's bottom, where each had 8 px above it
   * before ⑤c.
   */
  it('keeps each stacked row off the one above it', async () => {
    const el = await panel()
    const rows = [...el.querySelectorAll<HTMLElement>('.reader-section-head')]
    // The create form, two key rows, two session rows.
    expect(rows).toHaveLength(5)

    // **The gap, read where a gap can be read.** A row's top margin
    // collapses with the bottom margin of whatever precedes it, so this
    // measures the five boundaries where the element above contributes
    // none: the catalog's address above the create form, the create form
    // above the first key row, one key row above the next,
    // `.reader-panel-heading` (`margin: … 0 0`) above the first session row,
    // one session row above the next. All five measured 0 with the margin
    // gone.
    const gapAbove = (row: Element) => {
      const above = row.previousElementSibling
      if (!above) throw new Error('a stacked row with nothing above it')
      return Math.round(
        row.getBoundingClientRect().top - above.getBoundingClientRect().bottom,
      )
    }
    expect(rows.map(gapAbove)).toEqual([8, 8, 8, 8, 8])

    // **And the margin itself, which is what reaches the create form when
    // the address is not drawn.** Then the OPDS paragraph is above it, with
    // a 12 px bottom margin a top margin collapses into rather than adds
    // to, so the gap there measures 12 with this class and 12 without and
    // the gaps above cannot see that site. This reads the class itself,
    // whatever is above the row.
    expect(rows.map((row) => getComputedStyle(row).marginTop)).toEqual([
      '8px',
      '8px',
      '8px',
      '8px',
      '8px',
    ])
  })

  /**
   * **The caption inside the secret box, and the space under it.** The
   * inline style it lost in ⑤c was `margin: '8px 0 0'`; a `<p>`'s own
   * margin is 1em top *and bottom*, and the bottom one falls inside this
   * box's border, so the box grew a blank line beneath its last line of
   * text. This is also the fourth `look.meta` site, and the only one
   * `Account.test.tsx`'s count cannot reach — that count runs before a key
   * is minted, when no secret is on screen.
   */
  it('keeps the secret’s caption tight inside its box', async () => {
    const el = await panel()
    const form = el.querySelector('form:has(input[placeholder])')
    expect(form).not.toBeNull()
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })

    const secret = el.querySelector<HTMLElement>('.reader-secret')
    expect(secret?.textContent).toContain('tyk_the-only-copy')
    const caption = secret?.querySelector('p')
    expect(caption?.className).toBe('reader-meta')

    const style = getComputedStyle(caption!)
    expect([style.marginTop, style.marginBottom]).toEqual(['8px', '0px'])
    // What that margin is worth on screen: the box's own padding and border
    // below the caption, and nothing else. A `<p>`'s own bottom margin here
    // is 12 px, and it lands *inside* this border — which is the blank line
    // the secret grew under its last line of text.
    expect(
      Math.round(
        secret!.getBoundingClientRect().bottom -
          caption!.getBoundingClientRect().bottom,
      ),
    ).toBe(9)
  })
})

/**
 * **The key-name box, the one control on this screen with no `<label
 * class="field">` around it.** ⑤c replaced `admin.css`'s broad `.account
 * input` with the shared `.field` block, whose control reset is
 * `.account .field input` — a selector that cannot reach a box labelled by a
 * placeholder. Measured on this fixture before `.account .account-row input` joined
 * that reset: **166px wide with a 399px hole before the Create button, at
 * 16px against the password boxes' 14px, and `1px 2px` of padding against
 * their `4px 8px`** — `base.css`'s bare-control defaults, on a screen where
 * every other control is a settings field.
 *
 * In the browser project because all three numbers are a computed style or a
 * measured box, and jsdom has neither: the jsdom file beside this one can
 * only see that a class is on an element, and this input carries no class at
 * all — it is reached by its tag through the row's.
 *
 * **Mounted inside `.library`'s `<main>`, which is load-bearing rather than
 * scenery** — the same reason `Config.browser.test.tsx`'s `inShell` gives.
 * Every `base.css` rule these assertions compare against is written
 * `:where(.library, .login) …`, so a bare mount would measure a browser
 * default and the comparisons would pass over nothing.
 */
describe('the account page', () => {
  beforeEach(async () => {
    vi.stubGlobal('fetch', server())
    await page.viewport(1280, 900)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
    forgetAbout()
  })

  it('gives the key-name box the screen’s own control treatment, filling its row', async () => {
    const host = mount(
      <div className="library">
        <div className="sidebar-column" />
        <main>
          <Account placement="page" onSignedOut={() => {}} />
        </main>
      </div>,
    )
    // A macrotask, not a fixed number of microtask ticks: the key list and
    // the session list arrive over two separate promise chains. See
    // `panel()` above.
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })

    const screen = host.querySelector<HTMLElement>('.account')
    if (!screen) throw new Error('no .account rendered')
    const keyName = screen.querySelector<HTMLInputElement>('input[placeholder]')
    const password = screen.querySelector<HTMLInputElement>(
      'input[type="password"]',
    )
    if (!keyName || !password) throw new Error('missing a box to measure')
    const row = keyName.closest('form')
    const create = row?.querySelector('button')
    if (!row || !create) throw new Error('missing the create row')

    // **The size, as a pair rather than as an equality.** Two boxes that
    // agree at 16px would satisfy `toBe(getComputedStyle(password).fontSize)`
    // just as well, and 16px on both is the regression spread rather than
    // fixed — so the value each computes is named.
    expect([
      getComputedStyle(keyName).fontSize,
      getComputedStyle(password).fontSize,
    ]).toEqual(['14px', '14px'])

    // The rest of the control treatment the reset carries. Padding is the
    // half of it that moved: the border and the ground did not, because
    // `base.css` already gives every control on this screen both.
    expect([
      getComputedStyle(keyName).padding,
      getComputedStyle(password).padding,
    ]).toEqual(['4px 8px', '4px 8px'])

    // **The fill, stated as the hole it closes.** `.account-row` is a flex
    // row with `gap: var(--space-3)`, so a box that fills it sits exactly
    // one gap from the button beside it; the box that did not measured 399.
    // `wider than the Create button` would not say this — the 166px box was
    // already wider than the 75px button, so that comparison was green
    // against the defect and is not written here.
    expect(
      Math.round(
        create.getBoundingClientRect().left -
          keyName.getBoundingClientRect().right,
      ),
    ).toBe(12)
  })

  /**
   * **The room a focused control needs, on both sides of it.** `base.css`
   * draws `:focus-visible` at `outline: 2px solid var(--accent)` with
   * `outline-offset: 2px`, so the ring paints 4px outside the border box —
   * and the shared `.field` block put the micro-label 2.4px above its
   * control and the account page's `<form>` put the next field 0px below
   * it. Both were seen on the user's iPhone during ⑤c's device pass, and
   * reproduced at 390px in a Chromium context with `hasTouch` and
   * `isMobile`: the label's ink stopped 2.00px above the ring, and the ring
   * ended **1.00px** above the ink of the label under it.
   *
   * **The reach is read off the focused control rather than written as 4.**
   * A ring that grew would then be a failure here rather than a silent
   * return of the overlap, and the two figures below cannot drift apart
   * from the declarations that produce them. What that costs is a vacuity
   * guard that has to be about the ring being *drawn* rather than about the
   * reach being positive — see the comment on `outlineStyle` below, which
   * is where the first version of this case was wrong.
   *
   * The two forms on this screen take their separation differently and this
   * is why `.account form` is not where the fix lives: the API-key form is
   * `.account-row`, a flex row, and `.account form` is the higher
   * specificity of the two, so a `display: grid` written there wins on that
   * row as well — measured, injected at 390px: 54px with a 237px box beside
   * a 75px button became 110px with the button stacked under a 178px box.
   * The `display` is named here, and the width of that row is the case
   * above.
   */
  it('keeps a focused box’s ring clear of the label above it and the field below', async () => {
    const host = mount(
      <div className="library">
        <div className="sidebar-column" />
        <main>
          <Account placement="page" onSignedOut={() => {}} />
        </main>
      </div>,
    )
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })

    const screen = host.querySelector<HTMLElement>('.account')
    if (!screen) throw new Error('no .account rendered')
    const password = screen.querySelector<HTMLFormElement>(
      'form:has(input[type="password"])',
    )
    if (!password) throw new Error('no password form rendered')
    const fields = [...password.querySelectorAll<HTMLElement>('.field')]
    // Current and New — the two stacked fields the overlap was between.
    expect(fields).toHaveLength(2)
    const box = fields[0].querySelector('input')
    if (!box) throw new Error('the first field wraps no control')

    box.focus()
    const ring = getComputedStyle(box)
    // **`outline-style`, because `outline-width` alone cannot tell a ring
    // from no ring.** An *unfocused* control here computes `none` at
    // `3px`/`0px` — the UA's `medium` — so a focus that never landed would
    // hand the two comparisons below a threshold of 3 and sail under a real
    // ring of 4. Probed in Chromium on 2026-09-25, and watched: with
    // `box.focus()` deleted the case stayed green until these two lines.
    expect(box.matches(':focus-visible')).toBe(true)
    expect(ring.outlineStyle).toBe('solid')
    const reach =
      Number.parseFloat(ring.outlineWidth) +
      Number.parseFloat(ring.outlineOffset)
    expect(reach).toBeGreaterThan(0)

    // The micro-label is a bare text node inside the `<label>`, not a
    // `<span>` as it is on the configuration screens, so its box is read
    // with a range rather than from an element.
    const caption = document.createRange()
    const text = [...fields[0].childNodes].find(
      (node) => node.nodeType === Node.TEXT_NODE && node.textContent?.trim(),
    )
    if (!text) throw new Error('the first field carries no micro-label')
    caption.selectNodeContents(text)

    const gapAbove =
      box.getBoundingClientRect().top - caption.getBoundingClientRect().bottom
    const gapBelow =
      fields[1].getBoundingClientRect().top - box.getBoundingClientRect().bottom
    expect(Number(gapAbove.toFixed(2))).toBeGreaterThanOrEqual(reach)
    expect(Number(gapBelow.toFixed(2))).toBeGreaterThanOrEqual(reach)

    // The row that a `gap` on the form would have flattened.
    const keys = screen.querySelector<HTMLFormElement>(
      'form:has(input[placeholder])',
    )
    if (!keys) throw new Error('no API-key form rendered')
    expect(getComputedStyle(keys).display).toBe('flex')
  })
})
