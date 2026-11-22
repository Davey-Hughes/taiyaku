/**
 * The state of a volume's `i` panel (O283): which volume's is open, each
 * volume's detail once read, and every open and close. Apart from `Series`
 * so that the Library home's Continue row can draw the panel too (O417). A
 * screen may draw the panel outside its tile, so "inside" asks of the open
 * tile and the open panel both. Each screen keeps its tiles' markup and how
 * its panel is laid. `VolumePanel.tsx` draws the panel, from a file of its
 * own because `useComponentExportOnlyModules` refuses a hook and a component
 * in one, as `gate.ts` says.
 */
import {
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import type { BrowsePrefs } from './browseModule'
import { landFocus } from './landFocus'
import {
  type AlignmentCard,
  type EditionCard,
  type KeptApart,
  work as fetchWork,
  type WorkCard,
  type WorkDetail,
} from './library'
import type { TitleFlush } from './MetadataEditor'
import { messageOf } from './message'
import { invalidateLibraryViews } from './pageReads'

/**
 * What a volume's panel draws from `/api/library/works/{slug}`, or why it
 * could not be read. `editions` are the work's own, which carry the content
 * match marks an administrator is sent; the series' cards carry none.
 */
export type Detail =
  | {
      alignments: AlignmentCard[]
      editions: EditionCard[]
      kept_apart: KeptApart[]
    }
  | { error: string }

/** The parts of work `w` its panel draws. */
function detailOf(w: WorkDetail): Detail {
  return {
    alignments: w.alignments,
    editions: w.editions,
    kept_apart: w.kept_apart ?? [],
  }
}

/**
 * Where focus goes once a press's answer has been read again: the button
 * named `name` of the first of `pairs` drawn, else the panel.
 */
export interface Landing {
  name: 'split' | 'undo-split'
  pairs: number[]
}

/**
 * Whether a press at `event`'s point lands on `scroller`'s scrollbar: past the
 * right or the foot of the box it shows its content in, while it overflows that
 * way, where a classic scrollbar is drawn; the strip `scrollbar-gutter: stable`
 * (`library.css`) keeps where nothing scrolls is blank space. A press on either
 * reaches the page as one on the scroller, so only the point tells them apart.
 * The root's own box scrolls with the document, so the window's is read for it.
 */
function onScrollbarOf(
  scroller: Element,
  { clientX, clientY }: PointerEvent,
): boolean {
  const right = scroller.scrollHeight > scroller.clientHeight
  const foot = scroller.scrollWidth > scroller.clientWidth
  if (scroller === document.documentElement) {
    return (
      (right && clientX >= scroller.clientWidth) ||
      (foot && clientY >= scroller.clientHeight)
    )
  }
  const box = scroller.getBoundingClientRect()
  return (
    (right &&
      clientX >= box.left + scroller.clientLeft + scroller.clientWidth) ||
    (foot && clientY >= box.top + scroller.clientTop + scroller.clientHeight)
  )
}

/** What a screen hands the panel it draws under its tiles. */
export interface PanelScreen {
  /** The browse `panel` preference. */
  mode: BrowsePrefs['panel']
  /**
   * The works a panel can be drawn under, as the last read answered: the
   * same array until a read answers again. A volume pressed on in its panel
   * that the next answer leaves out sends focus to `main`.
   */
  works: readonly WorkCard[] | undefined
  /**
   * Lays the open panel from its tile, now and while it stays open; answers
   * what stops that. Called as passed by the render that opened the panel
   * or changed `laidFor`, so what it closes over is that render's.
   */
  follow: (panel: HTMLElement, tile: HTMLElement) => (() => void) | undefined
  /** A value whose change lays the panel again (Series: `layout`). */
  laidFor?: unknown
  /**
   * The element of `tile` a close by Escape gives focus back to. Called as
   * passed by the render that opened the panel or changed the mode, so what
   * it closes over is that render's.
   */
  backTo: (tile: HTMLElement) => HTMLElement | null
}

/** What `useVolumePanel(screen)` answers. */
export interface PanelState {
  /** The slug whose panel is open. */
  open: string | undefined
  /** Each volume's detail once read, by slug. */
  detail: Readonly<Record<string, Detail>>
  /** Refs for the open tile, the open panel and its editor. */
  tile: RefObject<HTMLLIElement | null>
  box: RefObject<HTMLDivElement | null>
  editor: RefObject<TitleFlush | null>
  /** Opens `slug`'s panel and reads its detail once. */
  reveal: (slug: string) => void
  /** `i`'s press: opens, or closes and gives `button` focus back. */
  toggle: (slug: string, button: HTMLElement) => void
  /** `on-select`'s pointer and focus arriving on a tile, each with its guard. */
  pointed: (slug: string) => void
  focused: (slug: string) => void
  /** Closes the open panel, after the editor's save; `after` runs once it has. */
  close: (after?: () => void) => void
  /**
   * `close` for a tile that has left the screen's view: its own focus and
   * pointer coming back into it or its panel leave the close waiting.
   */
  leave: (after?: () => void) => void
  /** Drops a `leave` still waiting: its tile is back in the screen's view. */
  stay: () => void
  /**
   * A close's `after` that gives `target` focus unless the person moved it,
   * focused with `options`.
   */
  refocus: (target: HTMLElement | null, options?: FocusOptions) => () => void
  /** For `VolumePanel`'s marks: the reads after a Split or an Undo answered in `slug`'s panel. */
  rereadAfterMatch: (slug: string, to: Landing) => Promise<void>
  /** For `VolumePanel`'s marks: focus onto the open panel itself. */
  focusPanel: () => void
}

/**
 * The state of the panel a screen draws under its tiles: which volume's is
 * open, each volume's detail once read, and every open and close, each once
 * the open editor has saved.
 */
export function useVolumePanel({
  mode,
  works,
  follow,
  laidFor,
  backTo,
}: PanelScreen): PanelState {
  const [open, setOpen] = useState<string | undefined>()
  // Keyed by slug and kept for the life of the page. **The cache is what makes
  // `on-select` affordable**: the panel then follows the pointer and the focus
  // ring, and without this that is one request per volume per hover. A failure
  // is cached too, as its own message — retrying on every re-render would turn
  // a 500 into a request loop.
  //
  // **Neither is reset when `works` changes**: a slug names one work wherever
  // it is drawn, so a cached detail stays valid, and a leftover `open` that
  // no tile drawn now carries draws no panel.
  const [detail, setDetail] = useState<Record<string, Detail>>({})
  // `detail[slug] !== undefined` only tells us a request has *settled* — a
  // second `reveal()` for the same slug while the first is still in flight
  // would sail past it and fire again. That is not hypothetical: in
  // `on-select` mode a tile carries both `onMouseEnter` and
  // `onFocusCapture`, and a real click moves focus to the link before the
  // click fires, so hovering a tile and then clicking it routes through
  // both handlers before the first request can have returned. A `Set` of
  // slugs currently in flight, marked *before* the fetch is issued, closes
  // that gap; a `ref` rather than `useState` so marking it is synchronous
  // and never itself schedules a render. It must not double as a loading
  // sentinel in `detail` — the "Reading…" state is `detail[slug] ===
  // undefined`, and seeding that with a placeholder would change what the
  // panel shows while the real request is still outstanding.
  const pending = useRef<Set<string>>(new Set())
  /**
   * The open volume's panel: its editor, its alignment detail and, in
   * `button` mode, ↓ Download.
   */
  const openPanel = useRef<HTMLDivElement>(null)
  /** The open volume's tile, which its panel is laid from. */
  const openTile = useRef<HTMLLIElement>(null)
  /** The open panel's editor, whose `flush` every close waits on. */
  const editor = useRef<TitleFlush>(null)
  /** The newest `choosePanel` call; one waiting on a save is then dropped. */
  const choosing = useRef<symbol>(undefined)
  /**
   * The volume whose panel is open as of the last change `choosePanel`
   * made, which `open` holds only from the next render.
   */
  const opened = useRef<string>(undefined)
  /**
   * True only while a close sends focus back to its tile: in `on-select`
   * mode that focus would otherwise reveal the panel again.
   */
  const refocusing = useRef(false)
  /** The `choosePanel` call `leave` made. */
  const leaving = useRef<symbol>(undefined)

  // The screen lays the open panel as it opens and while it stays open, and
  // again when `laidFor` changes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `open` and `laidFor` are triggers; the body reads the refs, and `follow` as the render that opened or laid the panel gave it.
  useLayoutEffect(() => {
    const panelBox = openPanel.current
    const tile = openTile.current
    if (panelBox === null || tile === null) return
    return follow(panelBox, tile)
  }, [open, laidFor])

  /**
   * Every change of which volume's panel is open: the open editor's `flush`
   * first, asked again after each `true` (text typed while a save was out),
   * then the change once it answers `null`; none on `false`, the failure or
   * the unwritten reason then shown in the panel still open. A call made
   * while one waits replaces it.
   * `after` runs once the change is made. The open volume asked for again is
   * no change, so its tile's focus and pointer send nothing. Answers the call.
   */
  const choosePanel = (
    next: string | undefined,
    after?: () => void,
  ): symbol => {
    const call = Symbol()
    choosing.current = call
    const ask = () => {
      const answer =
        next === opened.current ? null : (editor.current?.flush() ?? null)
      if (answer === null) {
        opened.current = next
        setOpen(next)
        after?.()
        return
      }
      void answer.then((stored) => {
        if (stored && choosing.current === call) ask()
      })
    }
    ask()
    return call
  }

  /** Whether `slug`'s panel is closing by `leave`, the close still waiting. */
  const held = (slug: string) =>
    leaving.current !== undefined &&
    leaving.current === choosing.current &&
    slug === opened.current

  /**
   * A close's `after` that focuses `target`, with `options`. Focus left in
   * the panel would be dropped with it, and the next Tab would start again
   * at the top of the page. Focus the person has moved elsewhere, while the
   * close waited on a save, stays where they put it.
   */
  const refocus = (target: HTMLElement | null, options?: FocusOptions) => {
    const from = document.activeElement
    return () => {
      const now = document.activeElement
      const dropped =
        now === document.body || openPanel.current?.contains(now) === true
      if (now !== from && !dropped) return
      refocusing.current = true
      try {
        target?.focus(options)
      } finally {
        refocusing.current = false
      }
    }
  }

  // The open panel's closes, as `usePopover`'s are: an Escape, caught on the
  // way down and stopped so that it closes nothing below it, unless it is the
  // Title field's own or ends a composition; a press outside the tile and its
  // panel, though not on the scrollbar of what scrolls the page, `main` or the
  // document's root, which may be the way to ↓ Download; and in `button` mode,
  // focus arriving outside. In `on-select` mode, focus on another tile moves
  // the panel instead.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `choosePanel`, `refocus` and `backTo` read only refs, `setOpen` and the mode, so those from the render that opened the panel serve until it closes or the mode changes.
  useEffect(() => {
    if (open === undefined) return
    const inside = (target: EventTarget | null) =>
      target instanceof Node &&
      ((openTile.current?.contains(target) ?? false) ||
        (openPanel.current?.contains(target) ?? false))
    const onKey = (event: KeyboardEvent) => {
      const tile = openTile.current
      if (event.key !== 'Escape' || tile === null) return
      if (event.isComposing || event.keyCode === 229) return
      const { target } = event
      if (
        target instanceof Element &&
        target.matches("textarea[name='title'][data-editing]")
      ) {
        return
      }
      event.stopPropagation()
      choosePanel(undefined, refocus(backTo(tile)))
    }
    const onPress = (event: PointerEvent) => {
      const { target } = event
      if (inside(target)) return
      if (
        target instanceof Element &&
        (target === document.documentElement ||
          target.matches('.library > main')) &&
        onScrollbarOf(target, event)
      ) {
        return
      }
      choosePanel(undefined)
    }
    const onFocus = (event: FocusEvent) => {
      if (!inside(event.target)) choosePanel(undefined)
    }
    document.addEventListener('keydown', onKey, true)
    document.addEventListener('pointerdown', onPress)
    if (mode === 'button') document.addEventListener('focusin', onFocus)
    return () => {
      document.removeEventListener('keydown', onKey, true)
      document.removeEventListener('pointerdown', onPress)
      document.removeEventListener('focusin', onFocus)
    }
  }, [open, mode])

  const reveal = (slug: string) => {
    choosePanel(slug)
    if (detail[slug] !== undefined || pending.current.has(slug)) return
    pending.current.add(slug)
    fetchWork(slug)
      .then((w) => {
        pending.current.delete(slug)
        setDetail((prev) => ({ ...prev, [slug]: detailOf(w) }))
      })
      .catch((err: unknown) => {
        pending.current.delete(slug)
        setDetail((prev) => ({
          ...prev,
          [slug]: { error: messageOf(err) },
        }))
      })
  }

  /**
   * After a Split or an Undo of volume `slug`'s panel has answered: the views
   * drawn are read again, and the volume's detail with it, since the derive the
   * press ran moves editions between works. Every other volume's detail is
   * dropped, to be read again as its panel opens, since the derive can have
   * moved its marks too. Resolves once the volume's read has landed, so the
   * panel's press is held until it draws the new state.
   */
  const rereadAfterMatch = (slug: string, to: Landing): Promise<void> => {
    if (works !== undefined) pressedOn.current = { slug, from: works }
    invalidateLibraryViews()
    setDetail((prev) => {
      const kept = prev[slug]
      return kept === undefined ? {} : { [slug]: kept }
    })
    return fetchWork(slug)
      .then(detailOf, (err: unknown): Detail => ({ error: messageOf(err) }))
      .then((next) => {
        landing.current = { ...to, slug }
        setDetail((prev) => ({ ...prev, [slug]: next }))
      })
  }

  /**
   * Focus onto the open panel itself, where a press of its marks takes away
   * the control that had it.
   */
  const focusPanel = () => {
    if (openPanel.current !== null) landFocus(openPanel.current)
  }

  /**
   * The volume a press of its marks was made on, and the works as drawn
   * then. A derive can empty that volume and delete its work, so the read
   * after the press draws no tile for it, and its panel goes with the
   * control that had focus: focus then goes to the page's `main`, as the
   * skip link's does.
   */
  const pressedOn = useRef<
    { slug: string; from: readonly WorkCard[] } | undefined
  >(undefined)
  useEffect(() => {
    const pressed = pressedOn.current
    if (pressed === undefined || works === undefined) return
    if (works === pressed.from) return
    pressedOn.current = undefined
    if (works.some((w) => w.slug === pressed.slug)) return
    if (document.activeElement !== document.body) return
    const main = document.querySelector<HTMLElement>('.library > main')
    if (main !== null) landFocus(main)
  }, [works])

  /** Where focus goes once `rereadAfterMatch`'s read is drawn. */
  const landing = useRef<(Landing & { slug: string }) | undefined>(undefined)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `detail` is the trigger; the body reads the refs.
  useEffect(() => {
    const to = landing.current
    if (to === undefined) return
    landing.current = undefined
    const box = openPanel.current
    if (box === null || opened.current !== to.slug) return
    // Focus the person has put elsewhere meanwhile stays where they put it.
    const now = document.activeElement
    if (now !== null && now !== document.body && !box.contains(now)) return
    const target = to.pairs
      .map((pair) =>
        box.querySelector<HTMLElement>(
          `button[name="${to.name}"][data-pair="${pair}"]`,
        ),
      )
      .find((button): button is HTMLElement => button !== null)
    if (target === undefined) focusPanel()
    else target.focus({ preventScroll: true })
  }, [detail])

  // **Toggle lives here, not inside `reveal`.** `reveal` is also what
  // `on-select`'s `pointed` and `focused` call, where pointing at a tile a
  // second time must still show it — toggling there would close the panel on
  // the second hover instead. The button is the one caller that means "open
  // *or* close this one", so this is the only place that decides between them;
  // `reveal` stays a plain "make this the open one" and the per-slug `detail`
  // cache and `pending` guard are untouched either way, so closing loses
  // nothing fetched and reopening never refetches.
  const toggle = (slug: string, button: HTMLElement) => {
    if (open === slug) {
      // A click need not focus the button, and focus in the panel would go with
      // it.
      choosePanel(undefined, refocus(button))
    } else {
      reveal(slug)
    }
  }

  const focused = (slug: string) => {
    if (!refocusing.current && !held(slug)) reveal(slug)
  }

  const pointed = (slug: string) => {
    // Not while focus is in the open panel: a pointer passing over other tiles,
    // the rows the panel lies over among them, would take the panel from the
    // person typing in it. Nor while its field holds a title whose save failed,
    // which the move would send again at every tile passed. Focus moving to
    // this tile is a choice, and still reveals it.
    if (
      openPanel.current?.contains(document.activeElement) ||
      editor.current?.failed() === true ||
      held(slug)
    ) {
      return
    }
    reveal(slug)
  }

  return {
    open,
    detail,
    tile: openTile,
    box: openPanel,
    editor,
    reveal,
    toggle,
    pointed,
    focused,
    close: (after) => {
      choosePanel(undefined, after)
    },
    leave: (after) => {
      leaving.current = choosePanel(undefined, after)
    },
    stay: () => {
      const slug = opened.current
      if (slug !== undefined && held(slug)) choosePanel(slug)
    },
    refocus,
    rereadAfterMatch,
    focusPanel,
  }
}
