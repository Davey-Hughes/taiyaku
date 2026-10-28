import { act } from 'react'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import { cdp, page, userEvent } from 'vitest/browser'
import { Account } from '../src/Account'
import { RESULT_SHOWN_MS } from '../src/copy'
import '../src/index.css'
import { ABOUT } from './support/about'
import { deferred, settleUntil } from './support/async'
import { holdOf } from './support/held'
import {
  changesOf,
  expectCallout,
  expectLaidUnderConfirm,
  expectPressesTaken,
  expectRoomUnseen,
  expectRowDrawnAt,
  layoutOf,
  liesOver,
  linesOf,
  pressWhereOver,
  roomKept,
} from './support/callout'
import { clipboardContext } from './support/clipboard'
import { tokenColour } from './support/colour'
import { typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { holdTheClock, pass, WAIT_DRAWN_AFTER_MS } from './support/wait'

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
const panelUi = () => (
  <div className="reader">
    <Account visible onClose={() => {}} onSignedOut={() => {}} />
  </div>
)

async function panel(): Promise<HTMLElement> {
  const host = mount(panelUi())
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

/** A real mouse, inside `act` so that React sees what it starts. */
const click = (el: Element) =>
  act(async () => {
    await userEvent.click(el)
  })

/** Real keys, inside `act` so that React sees what they start. */
const keys = (text: string) =>
  act(async () => {
    await userEvent.keyboard(text)
  })

const buttonNamed = (screen: HTMLElement, name: string) => {
  const found = [...screen.querySelectorAll('button')].find(
    (each) => each.textContent === name,
  )
  if (!found) throw new Error(`no ${name} button`)
  return found
}

/** What draws a button held, as `buttons.browser.test.tsx` reads it. */
const LOOK = [
  'border-top-style',
  'border-top-color',
  'color',
  'background-color',
  'opacity',
  'cursor',
]
const look = (el: Element) => {
  const style = getComputedStyle(el)
  return Object.fromEntries(LOOK.map((p) => [p, style.getPropertyValue(p)]))
}

/**
 * How `button` is drawn held by `disabled`, read off a copy of it held so
 * and put beside it, then taken away.
 */
const disabledLook = (button: HTMLButtonElement) => {
  const twin = button.cloneNode(true) as HTMLButtonElement
  twin.removeAttribute('aria-disabled')
  twin.disabled = true
  button.after(twin)
  const drawn = look(twin)
  twin.remove()
  return drawn
}

/** Each submit, the write it sends, and the screen's other submit. */
const SUBMITS = {
  'Change password': {
    route: 'PUT /api/password',
    other: 'Create',
    answer: () => new Response(null, { status: 204 }),
  },
  Create: {
    route: 'POST /api/keys',
    other: 'Change password',
    answer: () => json({ secret: 'tyk_the-only-copy' }),
  },
} as const

/**
 * `server()`, with the write sent to `route` held open until the case
 * answers it, and counted.
 */
function holdWrite(route: string) {
  const answer = deferred<Response>()
  let sent = 0
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      if (`${init?.method ?? 'GET'} ${input}` === route) {
        sent += 1
        return answer.promise
      }
      return rest(input, init)
    }),
  )
  return { answer, sent: () => sent }
}

/**
 * Clicks `name` on `screen` and holds its write open. The button keeps focus
 * through the wait and after the answer; it and the screen's other submit
 * are drawn held only once the wait has lasted `WAIT_DRAWN_AFTER_MS`, and
 * then as a disabled button is drawn; a click inside that delay and Enter
 * while it is drawn held send nothing. Focus is read before how the button
 * is held, so that one held by `disabled` again fails on focus left on the
 * page.
 */
async function pressedThroughItsWait(
  screen: HTMLElement,
  name: keyof typeof SUBMITS,
) {
  const { route, other, answer } = SUBMITS[name]
  const write = holdWrite(route)
  const button = buttonNamed(screen, name)
  const otherButton = buttonNamed(screen, other)
  const free = look(button)
  const held = disabledLook(button)
  // The two looks differ, or reading one where the other is due shows
  // nothing.
  expect(held).not.toEqual(free)

  // The wait's timer on a clock the test moves, so that the button is read
  // a millisecond before its hold is drawn with no race against the time
  // the click itself takes.
  holdTheClock()
  await click(button)
  expect(write.sent()).toBe(1)
  expect(document.activeElement).toBe(button)
  pass(WAIT_DRAWN_AFTER_MS - 1)
  expect(document.activeElement).toBe(button)
  expect([holdOf(button), holdOf(otherButton)]).toEqual(['free', 'free'])
  expect(look(button)).toEqual(free)
  // Inside the delay only the handler's guard stands between a press and a
  // second write.
  await click(button)
  expect(write.sent()).toBe(1)

  pass(1)
  expect(document.activeElement).toBe(button)
  expect([holdOf(button), holdOf(otherButton)]).toEqual(['held', 'held'])
  expect(look(button)).toEqual(held)
  // Drawn held, a press is turned down by `HeldButton` and by the handler's
  // guard alike, so this sends nothing unless both are gone.
  await keys('{Enter}')
  expect(write.sent()).toBe(1)

  vi.useRealTimers()
  await act(async () => {
    write.answer.resolve(answer())
  })
  // A create holds both until the listing after it lands as well.
  await settleUntil(
    `${name} freed after its answer`,
    () => holdOf(button) === 'free',
    1000,
  )
  expect(document.activeElement).toBe(button)
  expect([holdOf(button), holdOf(otherButton)]).toEqual(['free', 'free'])
}

const KEY_MADE =
  'Key made: phone. Copy it now — it is not stored and cannot be shown again.'

/** Types the key's name with real keys, so that Create sends one. */
async function nameTheKey(screen: HTMLElement) {
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  if (!box) throw new Error('no key name box')
  await act(async () => {
    await userEvent.fill(box, 'phone')
  })
}

const CAPTION =
  'Key “phone”: copy this now — it is not stored and cannot be shown again.'
const KEY_FORM = 'form:has(input[placeholder])'

/** API keys' visually hidden polite region, which says a key was made. */
const keyRegion = (screen: HTMLElement) => {
  const region = screen.querySelector<HTMLElement>(
    `${KEY_FORM} > [aria-live="polite"]`,
  )
  if (!region) throw new Error('no region saying a key was made')
  return region
}

const PASSWORD_CHANGED =
  'Password changed. Every other signed-in browser has been signed out; API keys still work.'

/** Password's visually hidden polite region, beside its form. */
const passwordRegion = (screen: HTMLElement) => {
  const region = screen.querySelector<HTMLElement>(
    'form:has(input[type="password"]) + [aria-live="polite"]',
  )
  if (!region) throw new Error('no region saying the password was changed')
  return region
}

/** Lets every promise settle, inside `act`. */
const landed = () =>
  act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })

/** Makes a key named phone with a real mouse and real keys. */
async function makeKey(screen: HTMLElement) {
  await nameTheKey(screen)
  await click(buttonNamed(screen, 'Create'))
  await landed()
}

/**
 * Makes a key, and reads that the line saying so moves nothing: the
 * headings of the two sections above the secret's box, and the form that
 * made it, stay where they were, and the box and what follows it are where
 * they are with the line taken out of the layout.
 */
async function madeWithoutMoving(screen: HTMLElement) {
  const [password, apiKeys, signedIn] = screen.querySelectorAll<HTMLElement>(
    'h3, .settings-section-title',
  )
  const form = screen.querySelector<HTMLElement>(KEY_FORM)
  if (!password || !apiKeys || !signedIn || !form) {
    throw new Error('a section or the key form is missing')
  }
  const tops = () =>
    [password, apiKeys, form].map((el) => el.getBoundingClientRect().top)
  const before = tops()

  await makeKey(screen)
  const region = keyRegion(screen)
  expect(region.textContent).toBe(KEY_MADE)
  expect(passwordRegion(screen).textContent).toBe('')
  expect(tops()).toEqual(before)

  const box = screen.querySelector<HTMLElement>('.secret')
  if (!box) throw new Error('no secret box')
  const below = () =>
    [form, box, signedIn].map((el) => {
      const { top, left, width, height } = el.getBoundingClientRect()
      return [top, left, width, height]
    })
  const drawn = below()
  region.style.display = 'none'
  const without = below()
  region.style.display = ''
  expect(drawn).toEqual(without)
}

/**
 * A click, Enter and Space on the secret's box each copy exactly the
 * secret and say "Copied", focus stays on the box, and the box is named by
 * its caption and never by the secret.
 */
async function copiesOnEachPress(screen: HTMLElement) {
  const writeText = vi.fn((_: string) => Promise.resolve())
  clipboardContext(true, { writeText })
  await makeKey(screen)
  const box = screen.querySelector<HTMLButtonElement>('button.secret')
  if (!box) throw new Error('the secret box is no button')
  expect(
    page.getByRole('button', { name: CAPTION, exact: true }).element(),
  ).toBe(box)
  expect(page.getByRole('button', { name: /tyk_/ }).elements()).toEqual([])
  const said = screen.querySelector(`${KEY_FORM} > [role="status"]`)

  await click(box)
  await landed()
  expect(writeText.mock.calls).toEqual([['tyk_the-only-copy']])
  expect(document.activeElement).toBe(box)
  expect(said?.textContent).toBe('Copied')
  await keys('{Enter}')
  await landed()
  await keys(' ')
  await landed()
  expect(writeText.mock.calls).toEqual([
    ['tyk_the-only-copy'],
    ['tyk_the-only-copy'],
    ['tyk_the-only-copy'],
  ])
  expect(document.activeElement).toBe(box)
  expect(said?.textContent).toBe('Copied')
}

/**
 * The secret's caption, at the window's width, after a key is made: it
 * runs over more than one line, and breaks between its words, never inside
 * one, as the secret above it does.
 */
async function captionBrokenBetweenWords(screen: HTMLElement) {
  clipboardContext(true, { writeText: vi.fn(() => Promise.resolve()) })
  await makeKey(screen)
  const caption = screen.querySelector('.secret-caption')
  if (!caption) throw new Error('no caption')
  const lines = new Set<number>()
  const split: string[] = []
  const texts = document.createTreeWalker(caption, NodeFilter.SHOW_TEXT)
  for (let node = texts.nextNode(); node !== null; node = texts.nextNode()) {
    for (const word of (node.textContent ?? '').matchAll(/\S+/g)) {
      const range = document.createRange()
      range.setStart(node, word.index)
      range.setEnd(node, word.index + word[0].length)
      const tops = new Set(
        [...range.getClientRects()].map((rect) => Math.round(rect.top)),
      )
      for (const top of tops) lines.add(top)
      if (tops.size > 1) split.push(word[0])
    }
  }
  expect(lines.size, 'lines the caption runs over').toBeGreaterThan(1)
  expect(split).toEqual([])
  const secret = screen.querySelector('.secret-value')
  expect(secret && getComputedStyle(secret).wordBreak).toBe('break-all')
}

/**
 * What is drawn in the secret's caption: its words, unless they are
 * clipped out of sight, and each of the results laid in their cell that is
 * shown (`CopyWords`).
 */
function captionDrawn(box: Element) {
  const word = box.querySelector('.copy-word')
  const results = box.querySelector('.copy-results')
  if (!word || !results) throw new Error('no caption words')
  const drawn =
    getComputedStyle(word).clipPath === 'none' ? [word.textContent] : []
  for (const pseudo of ['::before', '::after']) {
    const style = getComputedStyle(results, pseudo)
    if (style.visibility === 'visible') drawn.push(style.content.slice(1, -1))
  }
  return drawn
}

/**
 * The secret's box, where a press copies: under the pointer it takes the
 * ground the screen's other buttons are drawn on; a press that copied draws
 * "Copied" on it in its caption's place for `RESULT_SHOWN_MS`, and then the
 * caption again, and the box keeps its place and size throughout. The
 * hidden region still says it.
 */
async function drawsCopiedOnItself(screen: HTMLElement) {
  clipboardContext(true, { writeText: vi.fn(() => Promise.resolve()) })
  await makeKey(screen)
  const box = screen.querySelector<HTMLButtonElement>('button.secret')
  if (!box) throw new Error('the secret box is no button')
  const said = screen.querySelector(`${KEY_FORM} > [role="status"]`)
  const placed = () => {
    const { top, left, width, height } = box.getBoundingClientRect()
    return [top, left, width, height]
  }
  const before = placed()
  expect(captionDrawn(box)).toEqual([CAPTION])

  await userEvent.unhover(box)
  expect(getComputedStyle(box).backgroundColor).toBe('rgba(0, 0, 0, 0)')
  await userEvent.hover(box)
  expect(getComputedStyle(box).backgroundColor).toBe(
    getComputedStyle(buttonNamed(screen, 'Create')).backgroundColor,
  )
  expect(placed()).toEqual(before)
  await userEvent.unhover(box)

  holdTheClock()
  act(() => box.focus())
  await keys('{Enter}')
  await act(async () => {
    await Promise.resolve()
  })
  expect([captionDrawn(box), said?.textContent]).toEqual([['Copied'], 'Copied'])
  expect(placed()).toEqual(before)
  pass(RESULT_SHOWN_MS - 1)
  expect(captionDrawn(box)).toEqual(['Copied'])
  pass(1)
  expect(captionDrawn(box)).toEqual([CAPTION])
  expect(placed()).toEqual(before)
}

/**
 * What draws the secret's box, where it sits under the form that made it,
 * and where its caption sits in it. Read from the form rather than the
 * page, since Copy address joins the catalog's address above the form
 * where the clipboard can be written.
 */
const BOX_LOOK = [
  'display',
  'margin-top',
  'margin-bottom',
  'padding-top',
  'padding-right',
  'padding-bottom',
  'padding-left',
  'border-top-style',
  'border-top-width',
  'border-top-color',
  'border-top-left-radius',
  'background-color',
  'color',
  'font-family',
  'font-size',
  'font-weight',
  'line-height',
  'text-align',
  'word-break',
]
const boxLook = (box: HTMLElement) => {
  const style = getComputedStyle(box)
  const drawn = box.getBoundingClientRect()
  const form = box.previousElementSibling?.getBoundingClientRect()
  const caption = box.querySelector('.secret-caption')?.getBoundingClientRect()
  return {
    ...Object.fromEntries(BOX_LOOK.map((p) => [p, style.getPropertyValue(p)])),
    box: [
      drawn.left,
      form && drawn.top - form.bottom,
      drawn.width,
      drawn.height,
    ],
    caption: caption && [
      caption.left - drawn.left,
      caption.top - drawn.top,
      caption.width,
      caption.height,
    ],
  }
}

/**
 * Makes a key where nothing can copy it, so its box is drawn as text, then
 * gives the page a clipboard and draws it again, as a button: the button is
 * drawn as the box of text is, not as a button.
 */
async function drawnAsTheTextBox(screen: HTMLElement, again: () => void) {
  await makeKey(screen)
  const text = screen.querySelector<HTMLElement>('.secret')
  if (text?.tagName !== 'DIV') throw new Error('the secret is not text')
  const look = boxLook(text)
  clipboardContext(true, { writeText: vi.fn(() => Promise.resolve()) })
  again()
  const button = screen.querySelector<HTMLElement>('.secret')
  if (button?.tagName !== 'BUTTON') throw new Error('the secret is no button')
  expect(boxLook(button)).toEqual(look)
}

/**
 * `server()`, where a create adds a key named phone and a revoke takes its
 * key away, and each listing of the keys answers 150 ms after it is asked,
 * as a real one takes a while.
 */
function listedLate() {
  const row = (id: number, name: string) => ({
    id,
    name,
    created: 'then',
    last_used: null,
  })
  let listed = [row(1, 'Panels'), row(2, 'Chunky')]
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      if (input === '/api/keys' && method === 'POST') {
        listed = [...listed, row(3, 'phone')]
        return Promise.resolve(json({ secret: 'tyk_the-only-copy' }))
      }
      const revoked = /^\/api\/keys\/(\d+)$/.exec(input)
      if (revoked && method === 'DELETE') {
        listed = listed.filter((key) => key.id !== Number(revoked[1]))
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      if (input === '/api/keys') {
        const now = listed
        return new Promise<Response>((resolve) => {
          setTimeout(() => resolve(json(now)), 150)
        })
      }
      return rest(input, init)
    }),
  )
}

/** The key list's row for `name`, where it ends in a Revoke, if drawn. */
const keyRow = (screen: HTMLElement, name: string) =>
  [...screen.querySelectorAll('li, .reader-section-head')].find(
    (row) =>
      row.querySelector('button')?.textContent === 'Revoke' &&
      (row.textContent ?? '').includes(name),
  )

/**
 * A server holding the keys Panels, Chunky and Tablet, in that order, which
 * revokes any of them at once.
 */
function revokesKeys() {
  let listed = [
    { id: 1, name: 'Panels', created: 'then', last_used: null },
    { id: 2, name: 'Chunky', created: 'then', last_used: null },
    { id: 3, name: 'Tablet', created: 'then', last_used: null },
  ]
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const revoked = /^\/api\/keys\/(\d+)$/.exec(input)
      if (revoked && init?.method === 'DELETE') {
        listed = listed.filter((key) => key.id !== Number(revoked[1]))
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      if (input === '/api/keys') return Promise.resolve(json(listed))
      return rest(input, init)
    }),
  )
}

/**
 * Enter on Chunky's Revoke, and on each Revoke focus goes to as its row
 * goes: Tablet's, the next row's; Panels', the one before, with no row
 * after; then the New key box, with no key left. Chromium moves the focus
 * of a control taken out of the page to the body, which jsdom only stands
 * in for; `screen` is the panel's dialog, or the page's screen, and focus
 * stays in it.
 */
async function focusWalksTheKeysAsTheyGo(screen: HTMLElement) {
  const keyName = screen.querySelector<HTMLInputElement>('input[placeholder]')
  if (!keyName) throw new Error('no key name box')
  const revokeOf = (name: string) => {
    const row = keyRow(screen, name)
    const button = row?.querySelector('button')
    if (!button) throw new Error(`no Revoke for ${name}`)
    return button
  }
  const enter = async () => {
    await keys('{Enter}')
    await landed()
  }
  // A real click first, so that the keys reach the page, not the frame.
  await click(keyName)
  act(() => revokeOf('Chunky').focus())
  await enter()
  expect(document.activeElement).toBe(revokeOf('Tablet'))
  await enter()
  expect(document.activeElement).toBe(revokeOf('Panels'))
  expect(screen.contains(document.activeElement)).toBe(true)
  await enter()
  expect(document.activeElement).toBe(keyName)
  expect(
    ['Panels', 'Chunky', 'Tablet'].map((name) => keyRow(screen, name)),
  ).toEqual([undefined, undefined, undefined])
}

/** Signed in's heading, which stands below the secret's box and the keys. */
const signedInHeading = (screen: HTMLElement) => {
  const found = [
    ...screen.querySelectorAll('h3, .settings-section-title'),
  ].find((heading) => heading.textContent === 'Signed in')
  if (!found) throw new Error('no Signed in heading')
  return found
}

/**
 * `el`'s top in its scroller's content rather than in the window, so that a
 * press that scrolls its button into view, or a scroller that clamps as its
 * content shrinks, is not taken for the layout changing.
 */
const contentTop = (el: Element) => {
  const scroller =
    el.closest<HTMLElement>('.reader-dialog-panel, main') ??
    document.documentElement
  return (
    el.getBoundingClientRect().top -
    scroller.getBoundingClientRect().top +
    scroller.scrollTop
  )
}

/**
 * How many times `later` moved in its scroller's content (`contentTop`),
 * read on every frame from `press` until `settled` has held for twenty
 * frames. Outside `act`, so that React draws each change as it would for a
 * person, a frame at a time.
 */
async function movesOf(
  later: Element,
  press: () => Promise<unknown>,
  settled: () => boolean,
) {
  const top = () => contentTop(later)
  const global = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
  global.IS_REACT_ACT_ENVIRONMENT = false
  const tops = [top()]
  try {
    await press()
    let quiet = 0
    const until = performance.now() + 3000
    while (quiet < 20 && performance.now() < until) {
      await new Promise((resolve) => requestAnimationFrame(resolve))
      tops.push(top())
      quiet = settled() ? quiet + 1 : 0
    }
    expect(quiet, 'settled').toBe(20)
  } finally {
    global.IS_REACT_ACT_ENVIRONMENT = true
  }
  return tops.filter((top, i) => i > 0 && top !== tops[i - 1]).length
}

/**
 * Makes a key with a real mouse and reads that the page changed its layout
 * once: the secret's box and the made key's row are drawn together.
 */
async function madeInOneChange(screen: HTMLElement) {
  listedLate()
  await nameTheKey(screen)
  const create = buttonNamed(screen, 'Create')
  const moves = await movesOf(
    signedInHeading(screen),
    () => userEvent.click(create),
    () =>
      screen.querySelector('.secret') !== null &&
      keyRow(screen, 'phone') !== undefined,
  )
  expect(moves).toBe(1)
}

/**
 * Makes a key, then revokes it with a real mouse, and reads that the page
 * changed its layout once: its box and its row go together.
 */
async function revokedInOneChange(screen: HTMLElement) {
  listedLate()
  await nameTheKey(screen)
  await click(buttonNamed(screen, 'Create'))
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 300)
    })
  })
  const row = keyRow(screen, 'phone')
  expect(screen.querySelector('.secret')).not.toBeNull()
  if (!row) throw new Error('no row for the made key')
  const revoke = row.querySelector('button') as HTMLButtonElement
  const moves = await movesOf(
    signedInHeading(screen),
    () => userEvent.click(revoke),
    () =>
      screen.querySelector('.secret') === null &&
      keyRow(screen, 'phone') === undefined,
  )
  expect(moves).toBe(1)
}

/**
 * `server()`, where a password change signs out every session but this
 * one, and each listing of the sessions answers 150 ms after it is asked,
 * as a real one takes a while.
 */
function signsOthersOutLate() {
  let changed = false
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      if (asked === 'PUT /api/password') {
        changed = true
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      if (asked === 'GET /api/sessions') {
        const signedOut = changed
        return rest(input, init)
          .then((answer) => answer.json())
          .then(
            (rows: { current: boolean }[]) =>
              new Promise<Response>((resolve) => {
                setTimeout(() => {
                  resolve(
                    json(signedOut ? rows.filter((row) => row.current) : rows),
                  )
                }, 150)
              }),
          )
      }
      return rest(input, init)
    }),
  )
}

/** Whether a session row of `screen` names `browser`. */
const sessionListed = (screen: HTMLElement, browser: string) =>
  [...screen.querySelectorAll('li, .reader-section-head')].some(
    (row) =>
      row.querySelector('button')?.textContent === 'Revoke' &&
      (row.textContent ?? '').includes(browser),
  )

/**
 * Changes the password with real keys and a real mouse, and reads that the
 * page changed its layout once: the line saying so takes no room, and is
 * said in the render that draws Signed in without the session the change
 * signed out, which a watch on its region reads as the words land. Sign
 * out, under that list, moves once; API keys' heading, between Password
 * and Signed in, does not move.
 */
async function changedInOneChange(screen: HTMLElement) {
  signsOthersOutLate()
  const { current, change } = passwordParts(screen)
  const apiKeys = [
    ...screen.querySelectorAll('h3, .settings-section-title'),
  ].find((heading) => heading.textContent === 'API keys')
  if (!apiKeys) throw new Error('no API keys heading')
  const region = passwordRegion(screen)
  // Read as the commit that writes the words lands, before anything later
  // is drawn: whether the session the change signed out is still listed.
  let listedAsSaid: boolean | undefined
  const watch = new MutationObserver(() => {
    if (listedAsSaid === undefined && region.textContent === PASSWORD_CHANGED) {
      listedAsSaid = sessionListed(screen, 'Safari')
    }
  })
  watch.observe(region, { childList: true, subtree: true, characterData: true })
  onTestFinished(() => {
    watch.disconnect()
  })
  expect(sessionListed(screen, 'Safari')).toBe(true)
  await click(current)
  await keys('old{Tab}hunter22{Tab}hunter22')
  const keysTop = contentTop(apiKeys)
  const moves = await movesOf(
    buttonNamed(screen, 'Sign out'),
    () => userEvent.click(change),
    () =>
      region.textContent === PASSWORD_CHANGED &&
      !sessionListed(screen, 'Safari'),
  )
  expect(moves).toBe(1)
  expect(listedAsSaid, 'Safari listed as the words land').toBe(false)
  expect(contentTop(apiKeys)).toBe(keysTop)
  expect(region.textContent).toBe(PASSWORD_CHANGED)
  expect(region.className).toBe('visually-hidden')
}

const DIFFER = 'The two passwords do not match.'

/** What the server answers a Create sent with no name (`session.rs`). */
const NEEDS_A_NAME = 'a key needs a name, so the right one can be revoked later'

/** What the address's row says where `/api/about` did not answer with it. */
const ADDRESS_UNREAD = 'The catalog’s address could not be read.'

/** A network failure's words: a refusal long enough to take several lines. */
const RESET =
  'NetworkError when attempting to fetch resource: the connection to the server was reset before an answer came back.'

/** The key list's second row, Chunky's. */
const secondKey = (screen: HTMLElement) => {
  const row = keyRow(screen, 'Chunky')
  if (!row) throw new Error('no second key')
  return row
}

/**
 * A server that refuses Create with 400, in its words for a key with no
 * name, and Change password with 403, the rest as `server()`.
 */
function refusing() {
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      if (asked === 'POST /api/keys') {
        return Promise.resolve(new Response(NEEDS_A_NAME, { status: 400 }))
      }
      if (asked === 'PUT /api/password') {
        return Promise.resolve(new Response(null, { status: 403 }))
      }
      return rest(input, init)
    }),
  )
}

/**
 * The New key row, which a refused Create's callout is drawn after: on the
 * page, the row in its form; in the panel, the form, which is the row.
 */
function newKeyRow(screen: HTMLElement) {
  const form = screen.querySelector<HTMLElement>(KEY_FORM)
  if (!form) throw new Error('no key form')
  return form.querySelector<HTMLElement>(':scope > .settings-row') ?? form
}

/**
 * Where the window and `screen`, which scrolls in the panel, are scrolled
 * to. A refused Create's callout that is in view whole where it lies, as at
 * 1280×900, scrolls nothing as it comes. One whose foot would lie under
 * the edge of the view, as in the panel at 1366×768 and on either screen
 * at 390px, scrolls the view just far enough to be seen whole
 * (`bringIntoView`; the user's ruling of 2026-10-26), which is what lets
 * `eachCalloutInItsSection` find it drawn above what it lies over there.
 */
const scrolledTo = (screen: HTMLElement) => [window.scrollY, screen.scrollTop]

/**
 * Create, refused, says why in a callout laid right under the New key row,
 * over the first key's and not the second's, and, in view whole where it
 * lies, nothing on the screen moves or scrolls; an edit of the box takes it
 * away, and again nothing moves.
 */
async function createRefusedWithoutMoving(screen: HTMLElement) {
  refusing()
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  const first = keyRow(screen, 'Panels')
  if (!box || !first) throw new Error('no key form, or no first key')
  await click(box)
  const before = layoutOf(screen)
  const scrolled = scrolledTo(screen)
  await keys('{Enter}')
  await landed()
  const alert = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  if (!alert) throw new Error('no callout')
  expect(alert.textContent).toBe(NEEDS_A_NAME)
  expect(scrolledTo(screen), 'scrolled').toEqual(scrolled)
  expect(layoutOf(screen, alert.parentElement)).toEqual(before)
  await expectCallout(alert, newKeyRow(screen))
  expect(liesOver(alert, first), 'over the first key’s row').toBe(true)
  expect(liesOver(alert, secondKey(screen)), 'over the second’s').toBe(false)
  await keys('p')
  expect(screen.querySelector('.callout > [role="alert"]')).toBeNull()
  expect(layoutOf(screen)).toEqual(before)
}

/**
 * Change password, refused, says why in a callout laid right under Confirm
 * password's row, over Change password's, and nothing on the screen moves;
 * focus leaving the form takes it away, and again nothing moves.
 */
async function changeRefusedWithoutMoving(screen: HTMLElement) {
  refusing()
  const { current, confirmRow } = passwordParts(screen)
  await click(current)
  const before = layoutOf(screen)
  await keys('old{Enter}')
  await landed()
  const alert = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  if (!alert) throw new Error('no callout')
  expect(alert.textContent).toMatch(/current password did not match/)
  expect(layoutOf(screen, alert.parentElement)).toEqual(before)
  await expectCallout(alert, confirmRow)
  // The screen's title, which takes no focus and is in view.
  await click(screen.querySelector('h1, h2') as Element)
  expect(screen.querySelector('.callout > [role="alert"]')).toBeNull()
  expect(layoutOf(screen)).toEqual(before)
}

/**
 * Each of the screen's callouts in turn, at the window's width: a refused
 * Create, a refused Change password, and the line saying the two passwords
 * differ. Each lies right under the row it answers, over the rows below,
 * inside its section's content, and covers whole each control it lies over
 * (`expectCallout`): Create's, the first key's row, and the second key's
 * only where its words reach on into it (`reachesSecondKey`), as on two
 * lines in the panel at 390px; the other two, Change password, which is
 * covered whole.
 */
async function eachCalloutInItsSection(
  screen: HTMLElement,
  reachesSecondKey = false,
) {
  refusing()
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  const first = keyRow(screen, 'Panels')
  if (!box || !first) throw new Error('no key form, or no first key')
  await click(box)
  await keys('{Enter}')
  await landed()
  const refused = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  expect(refused?.textContent).toBe(NEEDS_A_NAME)
  await expectCallout(refused as Element, newKeyRow(screen))
  expect(liesOver(refused as Element, first)).toBe(true)
  expect(liesOver(refused as Element, secondKey(screen))).toBe(reachesSecondKey)

  const { current, change, line, confirmRow } = passwordParts(screen)
  await click(current)
  await keys('old{Tab}hunter22{Tab}hunter22{Enter}')
  await landed()
  const wrong = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  expect(wrong?.textContent).toMatch(/current password did not match/)
  await expectCallout(wrong as Element, confirmRow)
  expect(liesOver(wrong as Element, change)).toBe(true)

  await keys('{Backspace}x')
  expect(screen.querySelector('.callout > [role="alert"]')).toBeNull()
  expect(line.textContent).toBe(DIFFER)
  await expectCallout(line, confirmRow)
  expect(liesOver(line, change)).toBe(true)
}

/**
 * With no key listed, the New key row is the last of API keys, so a
 * refused Create, which would cross the section's foot under it, lies over
 * the rows above that row instead (`expectCallout`'s `above`), inside API
 * keys: never over the New key box, which is edited to take it away, nor
 * Create. `opened` draws the screen once the server lists no key.
 *
 * The address's row is that row in each state it can be in once the
 * screen is drawn, which is not before `/api/about` has settled, and in
 * each it gives at least the room of a line with Copy address in it:
 * `about` `'failed'` answers `/api/about` with a proxy's page, so that the
 * row says the address could not be read; and `clipboard` false takes the
 * Clipboard API away, so that a read address has no Copy address beside
 * it. Where no Copy address is drawn, an unseen one, drawn as the
 * placement's buttons are, keeps that room, and the row shows only its
 * words.
 */
async function createRefusedAboveItsRow(
  opened: () => Promise<HTMLElement>,
  {
    about = 'answered',
    clipboard = true,
  }: {
    about?: 'answered' | 'failed'
    clipboard?: boolean
  } = {},
) {
  if (!clipboard) clipboardContext(false)
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      if (asked === 'POST /api/keys') {
        return Promise.resolve(new Response(NEEDS_A_NAME, { status: 400 }))
      }
      if (asked === 'GET /api/keys') return Promise.resolve(json([]))
      if (about === 'failed' && asked === 'GET /api/about') {
        return Promise.resolve(
          new Response('<html>Bad gateway</html>', { status: 502 }),
        )
      }
      return rest(input, init)
    }),
  )
  const screen = await opened()
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  if (!box) throw new Error('no key form')
  expect(keyRow(screen, 'Panels')).toBeUndefined()
  const addressRow = screen.querySelector<HTMLElement>('.catalog-address')
  if (about === 'failed') {
    expect(addressRow?.firstElementChild?.textContent).toBe(ADDRESS_UNREAD)
  } else {
    expect(addressRow?.querySelector('code')?.textContent).toBe(
      `${location.origin}${ABOUT.catalog}`,
    )
  }
  expect(
    addressRow?.querySelector('button:not([aria-hidden="true"])') !== null,
    'Copy address drawn',
  ).toBe(about === 'answered' && clipboard)
  const address = `${location.origin}${ABOUT.catalog}`
  const shown = {
    failed: ADDRESS_UNREAD,
    answered: clipboard ? `${address} Copy address` : address,
  }[about]
  expect(
    (addressRow?.innerText ?? '').split(/\s+/).filter(Boolean).join(' '),
    'what the row shows',
  ).toBe(shown)
  const keeper = addressRow?.querySelector<HTMLElement>(
    'button[aria-hidden="true"]',
  )
  if (about === 'answered' && clipboard) {
    expect(keeper).toBeNull()
  } else {
    if (!keeper || !addressRow) throw new Error('no unseen Copy address')
    const create = buttonNamed(screen, 'Create')
    expect(keeper.checkVisibility({ visibilityProperty: true })).toBe(false)
    expect(keeper.className, 'drawn as the placement’s buttons').toBe(
      create.className,
    )
    const kept = keeper.getBoundingClientRect().height
    expect(kept).toBeCloseTo(create.getBoundingClientRect().height, 0)
    expect(addressRow.getBoundingClientRect().height).toBeGreaterThanOrEqual(
      kept - 0.5,
    )
  }
  await click(box)
  await keys('{Enter}')
  await landed()
  const refused = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  expect(refused?.textContent).toBe(NEEDS_A_NAME)
  await expectCallout(refused as Element, newKeyRow(screen), { above: true })
  expect([
    liesOver(refused as Element, box),
    liesOver(refused as Element, buttonNamed(screen, 'Create')),
  ]).toEqual([false, false])
}

/**
 * With no key listed and the catalog's address the one row above New key,
 * a refused Create of two lines has no room above that row either, so it
 * is pulled up over the New key row (`expectCallout`'s `pulledUp`), its
 * foot at API keys' content foot, inside the section, where it covers the
 * New key box and Create whole: the one place inside the box its words
 * fit.
 */
async function createRefusedPulledUp(opened: () => Promise<HTMLElement>) {
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      if (asked === 'POST /api/keys') {
        return Promise.reject(new TypeError(RESET))
      }
      if (asked === 'GET /api/keys') return Promise.resolve(json([]))
      return rest(input, init)
    }),
  )
  const screen = await opened()
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  if (!box) throw new Error('no key form')
  expect(keyRow(screen, 'Panels')).toBeUndefined()
  await click(box)
  await keys('{Enter}')
  await landed()
  const refused = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  expect(refused?.textContent).toContain(RESET)
  await expectCallout(refused as Element, newKeyRow(screen), {
    pulledUp: true,
  })
  expect([
    liesOver(refused as Element, box),
    liesOver(refused as Element, buttonNamed(screen, 'Create')),
  ]).toEqual([true, true])
}

/**
 * Change password, refused for a reason too long to lie under Confirm
 * password's row inside Password, says so over the rows above that row
 * instead (`expectCallout`'s `above`): never over the Confirm password box,
 * the one typed in last, nor Change password.
 */
async function changeRefusedAboveItsRow(screen: HTMLElement) {
  const rest = server()
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      if (`${init?.method ?? 'GET'} ${input}` === 'PUT /api/password') {
        return Promise.reject(new TypeError(RESET))
      }
      return rest(input, init)
    }),
  )
  const { current, confirm, confirmRow, change } = passwordParts(screen)
  await click(current)
  await keys('old{Tab}hunter22{Tab}hunter22{Enter}')
  await landed()
  const refused = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  expect(refused?.textContent).toContain(RESET)
  await expectCallout(refused as Element, confirmRow, { above: true })
  expect([
    liesOver(refused as Element, confirm),
    liesOver(refused as Element, change),
  ]).toEqual([false, false])
}

/**
 * Create, refused while the box of a key just made is shown under the New
 * key row, says why in a callout laid right under that row, over the
 * shown key's box, and not over the first key listed after it.
 */
async function createRefusedOverTheShownKey(screen: HTMLElement) {
  await makeKey(screen)
  const secret = screen.querySelector('.secret')
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  if (!secret || !box) throw new Error('no shown key, or no key form')
  refusing()
  await click(box)
  await keys('{Enter}')
  await landed()
  const refused = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  expect(refused?.textContent).toBe(NEEDS_A_NAME)
  await expectCallout(refused as Element, newKeyRow(screen))
  expect([
    liesOver(refused as Element, secret),
    liesOver(refused as Element, keyRow(screen, 'Panels') as Element),
  ]).toEqual([true, false])
}

/**
 * Create, refused, says why in a callout that lies over the first key's
 * row, Revoke with it. A real mouse pressed and released there
 * (`pressWhereOver`) revokes nothing: a refusal takes the press, which
 * reaches nothing beneath it.
 */
async function pressOnRefusalRevokesNothing(screen: HTMLElement) {
  const rest = server()
  let deletes = 0
  vi.stubGlobal(
    'fetch',
    vi.fn((input: string, init?: RequestInit) => {
      const asked = `${init?.method ?? 'GET'} ${input}`
      if (asked === 'POST /api/keys') {
        return Promise.resolve(new Response(NEEDS_A_NAME, { status: 400 }))
      }
      if (asked.startsWith('DELETE ')) {
        deletes += 1
        return Promise.resolve(new Response(null, { status: 204 }))
      }
      return rest(input, init)
    }),
  )
  const box = screen.querySelector<HTMLInputElement>('input[placeholder]')
  if (!box) throw new Error('no key name box')
  await click(box)
  await keys('{Enter}')
  await landed()
  const alert = screen.querySelector<HTMLElement>('.callout > [role="alert"]')
  const revoke = keyRow(screen, 'Panels')?.querySelector('button')
  if (!alert || !revoke) throw new Error('no refusal, or no Revoke under it')
  await pressWhereOver(alert, revoke)
  await landed()
  expect(deletes).toBe(0)
  expect(keyRow(screen, 'Panels')).toBeDefined()
}

/**
 * The line saying the two passwords differ lies over Change password, held
 * beneath it. A real mouse pressed on the line's middle, and where it lies
 * over Change password, reaches nothing (`expectPressesTaken`): the line
 * takes the press, as a refusal's callout does, so Change password is not
 * pressed and takes no focus, and nothing is sent. Tab still reaches it.
 */
async function pressOnTheLineReachesNothing(screen: HTMLElement) {
  const write = holdWrite('PUT /api/password')
  const { current, confirm, change, line } = passwordParts(screen)
  await click(current)
  await keys('old{Tab}hunter22{Tab}hunted')
  expect(line.textContent).toBe(DIFFER)
  await expectPressesTaken(line, change)
  expect(write.sent()).toBe(0)

  await click(confirm)
  await keys('{Tab}')
  expect(document.activeElement).toBe(change)
  expect(line.textContent).toBe(DIFFER)
}

/**
 * The password form's three boxes, Confirm password's row, Change
 * password, and the line saying the two differ, in the callout drawn after
 * Confirm password's row: on the page the box's row, in the panel its
 * label.
 */
function passwordParts(screen: HTMLElement) {
  const form = screen.querySelector<HTMLFormElement>(
    'form:has(input[type="password"])',
  )
  const [current, next, confirm] = form?.querySelectorAll('input') ?? []
  const change = buttonNamed(screen, 'Change password')
  const confirmRow = confirm?.closest('.settings-row, label')
  const line = form?.querySelector(':scope > .callout > [role="status"]')
  if (
    !form ||
    !current ||
    !next ||
    !confirm ||
    !confirmRow ||
    !(line instanceof HTMLElement)
  ) {
    throw new Error('the password form is missing a part')
  }
  return { form, current, next, confirm, confirmRow, line, change }
}

/**
 * The window `width` wide and the root's font `rootFont` (the browser's
 * own where empty), for this test.
 */
async function sizedWithText(width: number, rootFont: string) {
  const root = document.documentElement
  onTestFinished(() => {
    root.style.fontSize = ''
  })
  root.style.fontSize = rootFont
  await page.viewport(width, 900)
}

/**
 * The line saying the two passwords differ lies under Confirm password's
 * row and over no box, its words on one line or two (the user's ruling of
 * 2026-10-26): where they take two, the row under Confirm password,
 * Change password's, keeps their room whether the line is said or not, so
 * nothing moves when it comes, and a press on Confirm password reaches it.
 * Where they take one, that row keeps no room beyond Change password's, as
 * it kept none before.
 */
async function differLaidUnderConfirm(screen: HTMLElement) {
  const { current, confirm, confirmRow, line, change } = passwordParts(screen)
  const callout = line.parentElement as HTMLElement
  await click(current)
  const before = layoutOf(screen, callout)
  await keys('old{Tab}hunter22{Tab}hunted')
  expect(line.textContent).toBe(DIFFER)
  expect(layoutOf(screen, callout), 'what the line moved').toEqual(before)
  act(() => current.focus())
  await expectLaidUnderConfirm(line, confirm, confirmRow)
  expectRoomUnseen(change, line)
  if (linesOf(line) === 1) {
    expect(roomKept(change), 'room kept beside Change password').toBeCloseTo(
      0,
      1,
    )
  }
}

/**
 * Where the password form's row under Confirm password, Change password
 * and the three boxes are drawn at a desktop width (`expectRowDrawnAt`).
 * There the line saying the two passwords differ takes one line, so the
 * row keeps no room for it, and the form is drawn as a row keeping none
 * draws it. On the page Change password stands at its row's end, where a
 * form's buttons stand (`.settings-actions`); in the panel it spans the
 * panel, and its row is what it takes with its margin.
 */
const DRAWN_WIDE = {
  page: {
    row: [0, 167, 606, 59],
    button: [455, 180, 151, 34],
    boxes: [
      [350, 12, 256, 31],
      [350, 68, 256, 31],
      [350, 124, 256, 31],
    ],
  },
  panel: {
    row: [0, 192, 430, 43],
    button: [0, 204, 430, 31],
    boxes: [
      [0, 25, 430, 39],
      [0, 89, 430, 39],
      [0, 153, 430, 39],
    ],
  },
} as const

/**
 * With real keys, a new password and a confirm that is not its start: a
 * callout laid under Confirm password's row says so, and nothing on the
 * screen moves; Change password is held, and Enter in either
 * box sends nothing. Enter while the confirm is empty, or the start of the
 * password, says so too, until a box is edited. Moving between the two
 * boxes leaves the callout's words as they are, so they are not read out
 * again. Put right, the callout goes and Enter sends the one password.
 */
async function differsWithoutMoving(screen: HTMLElement) {
  const write = holdWrite('PUT /api/password')
  const { current, next, confirm, confirmRow, line, change } =
    passwordParts(screen)
  expect(line.textContent).toBe('')

  await click(current)
  const before = layoutOf(screen, line.parentElement)
  await keys('old{Tab}hunter22{Enter}')
  expect([document.activeElement === next, line.textContent]).toEqual([
    true,
    DIFFER,
  ])
  await keys('{Tab}hunter')
  expect(line.textContent).toBe('')
  await keys('{Enter}')
  expect(line.textContent).toBe(DIFFER)
  await keys('{Backspace}d{Enter}')
  expect(document.activeElement).toBe(confirm)
  expect(line.textContent).toBe(DIFFER)
  expect(holdOf(change)).toBe('held')
  expect(layoutOf(screen, line.parentElement)).toEqual(before)
  await expectCallout(line, confirmRow)
  expect(write.sent()).toBe(0)

  expect(
    await changesOf(line, () => userEvent.keyboard('{Shift>}{Tab}{/Shift}')),
  ).toBe(0)
  expect([document.activeElement, line.textContent]).toEqual([next, DIFFER])
  await keys('{Tab}{End}')

  await keys('{Backspace}{Backspace}er22')
  expect(line.textContent).toBe('')
  expect(holdOf(change)).toBe('free')
  await keys('{Enter}')
  expect(write.sent()).toBe(1)
  await act(async () => {
    write.answer.resolve(new Response(null, { status: 204 }))
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })
}

/**
 * Change password, held while the two passwords differ and reached on Tab,
 * keeps its focus under the callout that covers it, and the ring marking
 * that focus is drawn on the callout, where it can be seen, and not as a
 * sliver of the button's own beside the callout. Confirm password, above
 * the callout and not under it, keeps its own ring, and the callout none.
 */
async function focusMarkedOnTheCallout(screen: HTMLElement) {
  const { current, confirm, change, line } = passwordParts(screen)
  await click(current)
  await keys('old{Tab}hunter22{Tab}hunted{Tab}')
  expect([document.activeElement, change.matches(':focus-visible')]).toEqual([
    change,
    true,
  ])
  expect(line.textContent).toBe(DIFFER)
  expect(getComputedStyle(change).outlineStyle).toBe('none')
  const ring = getComputedStyle(line)
  expect([ring.outlineStyle, ring.outlineWidth, ring.outlineColor]).toEqual([
    'solid',
    '2px',
    tokenColour(line, '--accent'),
  ])
  // Inside the callout's edge, as the callout is inside its section.
  expect(Number.parseFloat(ring.outlineOffset)).toBeLessThanOrEqual(-2)

  await keys('{Shift>}{Tab}{/Shift}')
  expect([document.activeElement, line.textContent]).toEqual([confirm, DIFFER])
  expect([
    getComputedStyle(confirm).outlineStyle,
    getComputedStyle(line).outlineStyle,
  ]).toEqual(['solid', 'none'])
}

/**
 * Change password has focus when a fill of Confirm password, as a password
 * manager makes one, holds it: it is held by `aria-disabled`, which leaves
 * its focus where it is, where Chromium drops the focus of a control made
 * `disabled` to the page.
 */
async function keepsFocusWhenAFillHoldsIt(screen: HTMLElement) {
  const { next, confirm, change } = passwordParts(screen)
  await click(next)
  await keys('hunter22{Tab}hunter22')
  act(() => change.focus())
  expect(document.activeElement).toBe(change)
  await typeInto(confirm, 'hunter2')
  expect(holdOf(change)).toBe('held')
  expect(document.activeElement).toBe(change)
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
  it('breaks the secret’s caption between its words, never inside one, at 1920px', async () => {
    await page.viewport(1920, 1080)
    await captionBrokenBetweenWords(await panel())
  })

  it('keeps the secret’s caption tight inside its box', async () => {
    const el = await panel()
    const form = el.querySelector('form:has(input[placeholder])')
    expect(form).not.toBeNull()
    await act(async () => {
      form?.dispatchEvent(
        new Event('submit', { bubbles: true, cancelable: true }),
      )
    })
    // Drawn with the listing that follows the create.
    await settleUntil(
      'the secret',
      () => el.querySelector('.reader-secret') !== null,
      1000,
    )

    const secret = el.querySelector<HTMLElement>('.reader-secret')
    expect(secret?.textContent).toContain('tyk_the-only-copy')
    const caption = secret?.querySelector<HTMLElement>('.secret-caption')
    expect(caption?.className).toBe('copy-words secret-caption reader-meta')

    const style = getComputedStyle(caption!)
    expect([style.marginTop, style.marginBottom]).toEqual(['8px', '0px'])
    // What that margin is worth on screen: the box's own padding and border
    // below the caption, and nothing else; a bottom margin would land
    // *inside* this border, as a blank line under the secret's last line.
    expect(
      Math.round(
        secret!.getBoundingClientRect().bottom -
          caption!.getBoundingClientRect().bottom,
      ),
    ).toBe(9)
  })

  it('keeps focus on Change password, clicked, through its wait, drawn held as a disabled button only once the wait has lasted; a press then sends nothing', async () => {
    await pressedThroughItsWait(await panel(), 'Change password')
  })

  it('keeps focus on Create, clicked, through its wait and the line saying the key was made, drawn held as a disabled button only once the wait has lasted; a press then sends nothing', async () => {
    const el = await panel()
    await nameTheKey(el)
    await pressedThroughItsWait(el, 'Create')
    expect(keyRegion(el).textContent).toBe(KEY_MADE)
    expect(document.activeElement).toBe(buttonNamed(el, 'Create'))
  })

  it('says a key was made in API keys and moves nothing', async () => {
    await madeWithoutMoving(await panel())
  })

  it('draws the secret’s box and the made key’s row in one change of layout', async () => {
    await madeInOneChange(await panel())
  })

  it('takes the shown key’s box and its row away in one change of layout when it is revoked', async () => {
    await revokedInOneChange(await panel())
  })

  it('gives focus, as Enter on each Revoke takes its row, to the next key’s Revoke, else the one before’s, and with no key left to the New key box, inside its dialog', async () => {
    revokesKeys()
    await focusWalksTheKeysAsTheyGo(await panel())
  })

  it('says Password changed with the Signed in list that follows it, in one change of layout', async () => {
    await changedInOneChange(await panel())
  })

  it('draws Confirm as it draws New: the same box, under the same label, the same step below the box above it', async () => {
    const el = await panel()
    const { current, next, confirm } = passwordParts(el)
    const FIELD = [
      'display',
      'box-sizing',
      'font-size',
      'padding-top',
      'padding-left',
      'border-top-width',
      'border-top-style',
      'border-top-color',
      'border-top-left-radius',
      'background-color',
      'color',
      'margin-top',
    ]
    const drawn = (box: HTMLInputElement) => {
      const style = getComputedStyle(box)
      const label = box.closest('label')
      if (!label) throw new Error('a box outside its label')
      const { left, width, height } = box.getBoundingClientRect()
      return {
        ...Object.fromEntries(FIELD.map((p) => [p, style.getPropertyValue(p)])),
        box: [left, width, height],
        label: [
          getComputedStyle(label).display,
          getComputedStyle(label).color,
          getComputedStyle(label).fontSize,
          label.getBoundingClientRect().left,
        ],
        // The label's top below the box above it, and the box below its
        // label's words.
        step: [
          label.getBoundingClientRect().top -
            (box === next ? current : next).getBoundingClientRect().bottom,
          box.getBoundingClientRect().top - label.getBoundingClientRect().top,
        ],
      }
    }
    expect(drawn(confirm)).toEqual(drawn(next))
  })

  it('says the two passwords differ in a callout laid under Confirm password’s row, over Change password’s, moving nothing, and holds Change password; Enter in either box sends nothing then', async () => {
    await differsWithoutMoving(await panel())
  })

  it('takes no room for the callout under Confirm’s row while it is empty, and keeps Change password where it is', async () => {
    const el = await panel()
    const { line, confirm, change } = passwordParts(el)
    const callout = line.parentElement as HTMLElement
    const gap = () =>
      change.getBoundingClientRect().top -
      confirm.getBoundingClientRect().bottom
    expect(line.textContent).toBe('')
    expect(callout.getBoundingClientRect().height).toBe(0)
    const drawn = gap()
    callout.style.display = 'none'
    const without = gap()
    callout.style.display = ''
    expect(drawn).toBe(without)
  })

  it('keeps focus on Change password when a fill of Confirm holds it', async () => {
    await keepsFocusWhenAFillHoldsIt(await panel())
  })

  it('marks the focus of a held Change password on the callout that covers it, and leaves Confirm password’s, above it, its own', async () => {
    await focusMarkedOnTheCallout(await panel())
  })

  it('takes a press on the line saying the two passwords differ, so that Change password beneath it is neither pressed nor focused, and Tab still reaches it', async () => {
    await pressOnTheLineReachesNothing(await panel())
  })

  for (const [width, rootFont] of [
    [320, ''],
    [390, '20px'],
    [390, ''],
    [1920, ''],
  ] as const) {
    it(`lays the line saying the two passwords differ under Confirm’s row and over no box, so a press on Confirm reaches it, moving nothing, at ${width}px with the root’s font at ${rootFont || 'the browser’s own'}`, async () => {
      await sizedWithText(width, rootFont)
      await differLaidUnderConfirm(await panel())
    })
  }

  for (const width of [1920, 1280]) {
    it(`draws Change password across the panel under the three boxes, its row keeping no room for the line saying the two passwords differ, at ${width}px`, async () => {
      await page.viewport(width, 900)
      const { change } = passwordParts(await panel())
      await expectRowDrawnAt(change, DRAWN_WIDE.panel)
    })
  }

  it('says a refused Create in a callout laid under the New key row, over the first key’s, moving and scrolling nothing, until the box is edited', async () => {
    await createRefusedWithoutMoving(await panel())
  })

  it('says a refused Change password in a callout laid under Confirm password’s row, over Change password’s, moving nothing, until focus leaves its form', async () => {
    await changeRefusedWithoutMoving(await panel())
  })

  it('takes a press on a refused Create’s callout, so that a press there over the first key’s Revoke revokes nothing', async () => {
    await pressOnRefusalRevokesNothing(await panel())
  })

  for (const [width, height, reachesSecondKey] of [
    [1920, 1080, false],
    [1366, 768, false],
    [390, 844, true],
  ] as const) {
    it(`lays each callout right under the row it answers, inside its section’s content, covering whole each control it lies over, at ${width}px`, async () => {
      await page.viewport(width, height)
      await eachCalloutInItsSection(await panel(), reachesSecondKey)
    })
  }

  for (const [width, height] of [
    [1920, 1080],
    [1366, 768],
    [390, 844],
  ] as const) {
    it(`lays a refused Create over the rows above the New key row where no key is listed, inside API keys, over neither the New key box nor Create, at ${width}px`, async () => {
      await page.viewport(width, height)
      await createRefusedAboveItsRow(panel)
    })

    it(`lays a refused Create over the row saying the catalog’s address could not be read, where /api/about failed and no key is listed, over neither the New key box nor Create, at ${width}px`, async () => {
      await page.viewport(width, height)
      await createRefusedAboveItsRow(panel, { about: 'failed' })
    })

    it(`lays a refused Create over the catalog’s address with no Copy address, where the clipboard cannot be written and no key is listed, over neither the New key box nor Create, at ${width}px`, async () => {
      await page.viewport(width, height)
      await createRefusedAboveItsRow(panel, { clipboard: false })
    })

    it(`lays a refused Change password whose words would cross Password’s foot over the rows above Confirm password’s, over neither Confirm password nor Change password, at ${width}px`, async () => {
      await page.viewport(width, height)
      await changeRefusedAboveItsRow(await panel())
    })
  }

  it('lays a refused Create under the New key row over the shown key’s box, at 1366px', async () => {
    await page.viewport(1366, 768)
    await createRefusedOverTheShownKey(await panel())
  })

  it('copies the secret on a click, Enter and Space on its box, keeping focus there, and says Copied', async () => {
    await copiesOnEachPress(await panel())
  })

  it('draws Copied on the secret’s box for a moment after a press that copied, and the other buttons’ ground under the pointer, moving nothing', async () => {
    await drawsCopiedOnItself(await panel())
  })

  it('draws the secret’s box as the box of text it is where nothing can copy it, not as a button', async () => {
    clipboardContext(false)
    await drawnAsTheTextBox(await panel(), () => rerender(panelUi()))
  })

  // Set over the DevTools session, as a system set to them has the browser
  // draw, and cleared after: the setting is the page's, not this frame's.
  it('draws Change password and Create held as disabled buttons under forced colours, their words and edge in GrayText', async () => {
    const force = (value: string) =>
      cdp().send('Emulation.setEmulatedMedia', {
        features: [{ name: 'forced-colors', value }],
      })
    onTestFinished(async () => {
      await force('')
    })
    await force('active')
    expect(matchMedia('(forced-colors: active)').matches).toBe(true)
    const el = await panel()
    // Inside the panel, so that `GrayText` resolves in the panel's dark
    // `color-scheme` as its buttons' colours do.
    const probe = document.createElement('div')
    probe.style.color = 'GrayText'
    el.append(probe)
    const gray = getComputedStyle(probe).color
    probe.remove()
    const buttons = ['Change password', 'Create'].map((name) =>
      buttonNamed(el, name),
    )
    expect(buttons.map((b) => getComputedStyle(b).color)).not.toContain(gray)

    const write = holdWrite('PUT /api/password')
    await click(buttons[0])
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, WAIT_DRAWN_AFTER_MS + 100)
      })
    })
    for (const button of buttons) {
      const kind = button.textContent ?? ''
      expect(holdOf(button), kind).toBe('held')
      expect.soft(look(button), kind).toEqual(disabledLook(button))
      const { color, borderTopColor } = getComputedStyle(button)
      expect.soft([color, borderTopColor], kind).toEqual([gray, gray])
    }
    await act(async () => {
      write.answer.resolve(new Response(null, { status: 204 }))
      await new Promise((resolve) => {
        setTimeout(resolve, 0)
      })
    })
  })
})

/**
 * **The page's boxes are settings rows' controls**, drawn by `settings.css`
 * as every settings page's are, the key's name box among them: it shares
 * its row's control with Create.
 *
 * In the browser project because each figure here is a computed style or a
 * measured box, and jsdom has neither.
 *
 * **Mounted inside `.library`'s `<main>`, as `Shell` puts it.** The rows'
 * sizes are `settings.css`'s and reach a box wherever it is drawn, but the
 * ring the ring case measures is `base.css`'s `:where(.library, .login)
 * :where(:focus-visible)`: in a bare mount that case would measure the
 * browser's own ring instead.
 */
describe('the account page', () => {
  beforeEach(async () => {
    vi.stubGlobal('fetch', server())
    await page.viewport(1280, 900)
  })

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  const pageUi = () => (
    <div className="library">
      <div className="sidebar-column" />
      <main>
        <Account placement="page" onSignedOut={() => {}} />
      </main>
    </div>
  )

  /** The page inside `.library`'s `<main>`, with its lists landed. */
  async function accountPage(): Promise<HTMLElement> {
    const host = mount(pageUi())
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
    return screen
  }

  /**
   * **The key's name box, measured against the password boxes.** A box
   * drawn anywhere but in its row's control takes `base.css`'s bare-control
   * defaults, 16px with `1px 2px` of padding, beside password boxes at 14px
   * and `4px 8px`; and a box that does not sit against Create leaves a hole
   * between the two.
   */
  for (const [width, height, end] of [
    [1920, 1080, 'end'],
    [1024, 768, 'end'],
    [390, 844, 'start'],
  ] as const) {
    it(`puts Copy address at its row’s ${end}, where the boxes’ other buttons are, at ${width}px`, async () => {
      clipboardContext(true, { writeText: vi.fn(() => Promise.resolve()) })
      await page.viewport(width, height)
      const screen = await accountPage()
      const copy = buttonNamed(screen, 'Copy address').getBoundingClientRect()
      const row = screen.querySelector('.catalog-address')
      const address = row?.querySelector('code')?.getBoundingClientRect()
      if (!row || !address) throw new Error('no catalog address')
      const edge = row.getBoundingClientRect()
      if (end === 'end') {
        expect(Math.abs(copy.right - edge.right)).toBeLessThan(0.5)
        // Every other button in the boxes ends on that edge too.
        const signOut = buttonNamed(screen, 'Sign out').getBoundingClientRect()
        expect(Math.abs(signOut.right - edge.right)).toBeLessThan(0.5)
      } else {
        // On a line of its own, under the address, from its start.
        expect(copy.top).toBeGreaterThanOrEqual(address.bottom)
        expect(Math.abs(copy.left - edge.left)).toBeLessThan(0.5)
      }
    })
  }

  for (const [width, height] of [
    [1920, 1080],
    [1024, 768],
    [390, 844],
  ] as const) {
    it(`starts the New key box on the password boxes’ edge, and ends Create on their other edge, at ${width}px`, async () => {
      await page.viewport(width, height)
      const screen = await accountPage()
      const keyName = screen
        .querySelector('input[placeholder]')
        ?.getBoundingClientRect()
      const create = buttonNamed(screen, 'Create').getBoundingClientRect()
      const passwords = [
        ...screen.querySelectorAll('input[type="password"]'),
      ].map((box) => box.getBoundingClientRect())
      if (!keyName || passwords.length !== 3)
        throw new Error('a box is missing')
      for (const box of passwords) {
        expect([box.left, box.right]).toEqual([
          passwords[0].left,
          passwords[0].right,
        ])
      }
      expect(Math.abs(keyName.left - passwords[0].left), 'start').toBeLessThan(
        0.5,
      )
      expect(Math.abs(create.right - passwords[0].right), 'end').toBeLessThan(
        0.5,
      )
      // Create beside the box, on its line.
      expect(create.left).toBeGreaterThan(keyName.right)
      expect(Math.abs(create.top - keyName.top)).toBeLessThan(create.height)
    })
  }

  it('gives the key-name box the screen’s own control treatment, beside its Create', async () => {
    const screen = await accountPage()
    const keyName = screen.querySelector<HTMLInputElement>('input[placeholder]')
    const password = screen.querySelector<HTMLInputElement>(
      'input[type="password"]',
    )
    if (!keyName || !password) throw new Error('missing a box to measure')
    const create = keyName.parentElement?.querySelector('button')
    if (!create) throw new Error('missing the create button')

    // **The size, as a pair rather than as an equality.** Two boxes that
    // agree at 16px would satisfy `toBe(getComputedStyle(password).fontSize)`
    // just as well, and 16px on both is the regression spread rather than
    // fixed — so the value each computes is named.
    expect([
      getComputedStyle(keyName).fontSize,
      getComputedStyle(password).fontSize,
    ]).toEqual(['14px', '14px'])
    expect([
      getComputedStyle(keyName).padding,
      getComputedStyle(password).padding,
    ]).toEqual(['4px 8px', '4px 8px'])

    // **Against Create, stated as the hole it would leave.** The row's
    // control sets its parts a `var(--space-2)` gap apart, so a box against
    // Create sits exactly that far from it.
    expect(
      Math.round(
        create.getBoundingClientRect().left -
          keyName.getBoundingClientRect().right,
      ),
    ).toBe(8)
  })

  /**
   * **The room a focused control needs, on both sides of it.** `base.css`
   * draws `:focus-visible` at `outline: 2px solid var(--accent)` with
   * `outline-offset: 2px`, so the ring paints 4px outside the border box.
   * At a phone's width a row's label stacks above its box, so a label
   * nearer its box than that, or a next row's label nearer than that below
   * it, is painted over by the ring. The labels' boxes are measured, not
   * their ink, which sits inside them, so the gaps read here are the least
   * they can be.
   *
   * **The reach is read off the focused control rather than written as 4.**
   * A ring that grew is then a failure here rather than a silent return of
   * the overlap. What that costs is a guard that the ring is *drawn* — see
   * the comment on `outlineStyle` below.
   */
  it('keeps a focused box’s ring clear of the label above it and the field below', async () => {
    await page.viewport(390, 844)
    const screen = await accountPage()
    const password = screen.querySelector<HTMLFormElement>(
      'form:has(input[type="password"])',
    )
    if (!password) throw new Error('no password form rendered')
    const rows = [...password.querySelectorAll<HTMLElement>('.settings-row')]
    // Current password, New password and Confirm password — the three
    // stacked rows.
    expect(rows).toHaveLength(3)
    const box = rows[0].querySelector('input')
    const label = rows[0].querySelector('label')
    const nextLabel = rows[1].querySelector('label')
    if (!box || !label || !nextLabel) throw new Error('a row is incomplete')

    box.focus()
    const ring = getComputedStyle(box)
    // **`outline-style`, because `outline-width` alone cannot tell a ring
    // from no ring.** An *unfocused* control here computes `none` at
    // `3px`/`0px` — the UA's `medium` — so a focus that never landed would
    // hand the two comparisons below a threshold of 3 and sail under a real
    // ring of 4.
    expect(box.matches(':focus-visible')).toBe(true)
    expect(ring.outlineStyle).toBe('solid')
    const reach =
      Number.parseFloat(ring.outlineWidth) +
      Number.parseFloat(ring.outlineOffset)
    expect(reach).toBeGreaterThan(0)

    const gapAbove =
      box.getBoundingClientRect().top - label.getBoundingClientRect().bottom
    const gapBelow =
      nextLabel.getBoundingClientRect().top - box.getBoundingClientRect().bottom
    expect(Number(gapAbove.toFixed(2))).toBeGreaterThanOrEqual(reach)
    expect(Number(gapBelow.toFixed(2))).toBeGreaterThanOrEqual(reach)

    // The key's row keeps Create on its box's line at this width too, so
    // the box it sends is beside it and not a line above.
    const keys = screen.querySelector<HTMLFormElement>(
      'form:has(input[placeholder])',
    )
    const keyBox = keys?.querySelector('input')
    const create = keys?.querySelector('button')
    if (!keyBox || !create) throw new Error('no API-key form rendered')
    const middle = (el: Element) => {
      const { top, bottom } = el.getBoundingClientRect()
      return (top + bottom) / 2
    }
    expect(Math.abs(middle(create) - middle(keyBox))).toBeLessThan(1)
  })

  it('keeps focus on Change password, clicked, through its wait, drawn held as a disabled button only once the wait has lasted; a press then sends nothing', async () => {
    await pressedThroughItsWait(await accountPage(), 'Change password')
  })

  it('says the two passwords differ in a callout laid under Confirm password’s row, over Change password’s, moving nothing, and holds Change password; Enter in either box sends nothing then', async () => {
    await differsWithoutMoving(await accountPage())
  })

  it('draws nothing for the callout under Confirm password’s row while it is empty: no room, no edge', async () => {
    const screen = await accountPage()
    const { line } = passwordParts(screen)
    const callout = line.parentElement as HTMLElement
    expect(line.textContent).toBe('')
    expect([
      callout.getBoundingClientRect().height,
      getComputedStyle(line).borderTopStyle,
    ]).toEqual([0, 'none'])
  })

  it('keeps focus on Change password when a fill of Confirm password holds it', async () => {
    await keepsFocusWhenAFillHoldsIt(await accountPage())
  })

  it('marks the focus of a held Change password on the callout that covers it, and leaves Confirm password’s, above it, its own', async () => {
    await focusMarkedOnTheCallout(await accountPage())
  })

  it('takes a press on the line saying the two passwords differ, so that Change password beneath it is neither pressed nor focused, and Tab still reaches it', async () => {
    await pressOnTheLineReachesNothing(await accountPage())
  })

  for (const [width, rootFont] of [
    [320, ''],
    [390, '20px'],
    [390, ''],
    [1920, ''],
  ] as const) {
    it(`lays the line saying the two passwords differ under Confirm password’s row and over no box, so a press on Confirm password reaches it, moving nothing, at ${width}px with the root’s font at ${rootFont || 'the browser’s own'}`, async () => {
      await sizedWithText(width, rootFont)
      await differLaidUnderConfirm(await accountPage())
    })
  }

  for (const width of [1920, 1280]) {
    it(`draws Change password at its row’s end under the three boxes, its row keeping no room for the line saying the two passwords differ, at ${width}px`, async () => {
      await page.viewport(width, 900)
      const { change } = passwordParts(await accountPage())
      await expectRowDrawnAt(change, DRAWN_WIDE.page)
    })
  }

  it('says a refused Create in a callout laid under the New key row, over the first key’s, moving and scrolling nothing, until the box is edited', async () => {
    await createRefusedWithoutMoving(await accountPage())
  })

  it('says a refused Change password in a callout laid under Confirm password’s row, over Change password’s, moving nothing, until focus leaves its form', async () => {
    await changeRefusedWithoutMoving(await accountPage())
  })

  for (const [width, height] of [
    [1920, 1080],
    [390, 844],
  ] as const) {
    it(`lays each callout right under the row it answers, inside its section’s content, covering whole each control it lies over, at ${width}px`, async () => {
      await page.viewport(width, height)
      await eachCalloutInItsSection(await accountPage())
    })
  }

  for (const [width, height] of [
    [1920, 1080],
    [390, 844],
  ] as const) {
    it(`lays a refused Create over the rows above the New key row where no key is listed, inside API keys, over neither the New key box nor Create, at ${width}px`, async () => {
      await page.viewport(width, height)
      await createRefusedAboveItsRow(accountPage)
    })

    it(`lays a refused Create over the row saying the catalog’s address could not be read, where /api/about failed and no key is listed, over neither the New key box nor Create, at ${width}px`, async () => {
      await page.viewport(width, height)
      await createRefusedAboveItsRow(accountPage, { about: 'failed' })
    })

    it(`lays a refused Create over the catalog’s address with no Copy address, where the clipboard cannot be written and no key is listed, over neither the New key box nor Create, at ${width}px`, async () => {
      await page.viewport(width, height)
      await createRefusedAboveItsRow(accountPage, { clipboard: false })
    })

    it(`lays a refused Change password whose words would cross Password’s foot over the rows above Confirm password’s, over neither Confirm password nor Change password, at ${width}px`, async () => {
      await page.viewport(width, height)
      await changeRefusedAboveItsRow(await accountPage())
    })
  }

  it('pulls a refused Create with no room above the New key row up over it, its foot at the foot of API keys’ content, covering the New key box and Create whole, where no key is listed, at 1920px', async () => {
    await page.viewport(1920, 1080)
    await createRefusedPulledUp(accountPage)
  })

  it('lays a refused Create under the New key row over the shown key’s box, at 1920px', async () => {
    await page.viewport(1920, 1080)
    await createRefusedOverTheShownKey(await accountPage())
  })

  it('keeps focus on Create, clicked, through its wait and the line saying the key was made, drawn held as a disabled button only once the wait has lasted; a press then sends nothing', async () => {
    const screen = await accountPage()
    await nameTheKey(screen)
    await pressedThroughItsWait(screen, 'Create')
    expect(keyRegion(screen).textContent).toBe(KEY_MADE)
    expect(document.activeElement).toBe(buttonNamed(screen, 'Create'))
  })

  it('says a key was made in API keys and moves nothing', async () => {
    await madeWithoutMoving(await accountPage())
  })

  it('draws the secret’s box and the made key’s row in one change of layout', async () => {
    await madeInOneChange(await accountPage())
  })

  it('takes the shown key’s box and its row away in one change of layout when it is revoked', async () => {
    await revokedInOneChange(await accountPage())
  })

  it('gives focus, as Enter on each Revoke takes its row, to the next key’s Revoke, else the one before’s, and with no key left to the New key box', async () => {
    revokesKeys()
    await focusWalksTheKeysAsTheyGo(await accountPage())
  })

  it('says Password changed with the Signed in list that follows it, in one change of layout', async () => {
    await changedInOneChange(await accountPage())
  })

  it('copies the secret on a click, Enter and Space on its box, keeping focus there, and says Copied', async () => {
    await copiesOnEachPress(await accountPage())
  })

  it('draws Copied on the secret’s box for a moment after a press that copied, and the other buttons’ ground under the pointer, moving nothing', async () => {
    await drawsCopiedOnItself(await accountPage())
  })

  it('draws the secret’s box as the box of text it is where nothing can copy it, not as a button', async () => {
    clipboardContext(false)
    await drawnAsTheTextBox(await accountPage(), () => rerender(pageUi()))
  })

  it('breaks the secret’s caption between its words, never inside one, at 390px', async () => {
    await page.viewport(390, 844)
    await captionBrokenBetweenWords(await accountPage())
  })

  it('says Not copied for a refused write, and draws the secret as text, selected for copying by hand, with focus on its box', async () => {
    clipboardContext(true, {
      writeText: vi.fn(() => Promise.reject(new Error('denied'))),
    })
    const screen = await accountPage()
    await makeKey(screen)
    const button = screen.querySelector<HTMLElement>('button.secret')
    if (!button) throw new Error('the secret box is no button')
    await click(button)
    await landed()
    expect(
      screen.querySelector(`${KEY_FORM} > [role="status"]`)?.textContent,
    ).toBe('Not copied')
    const box = screen.querySelector<HTMLElement>('.secret')
    expect(box?.tagName).toBe('DIV')
    expect(document.activeElement).toBe(box)
    expect(window.getSelection()?.toString()).toBe('tyk_the-only-copy')
  })
})
