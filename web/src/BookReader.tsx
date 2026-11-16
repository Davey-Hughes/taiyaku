/**
 * A book read as text, with Space swapping to the next edition at the block
 * that shows the same text.
 *
 * Its chrome is the page reader's: `TopBar`, the bottom bar's progress line and
 * readout row (`bars.ts`), and the `Help`, `Settings` and `Account` panels, fed
 * from a registry of its own whose `book-text` module sets the text. Its own
 * panel is `Contents`. Its keys are `BOOK_KEYS` and its modules' own, which
 * `Help` lists.
 *
 * Notes are the page reader's notes document, another edition's marked on the
 * paragraph a swap from them lands on.
 *
 * Where the reader is is the first block whose bottom is below the top of the
 * text, and it is saved as the position's slot, a book edition's slots being
 * its blocks.
 */
import {
  type CSSProperties,
  Fragment,
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { Account } from './Account'
import { getProgress, lookUpWord, putProgress } from './api'
import { bottomBarStyle, readoutStyle, readoutTextStyle } from './bars'
import {
  BOOK_KEYS,
  type Block,
  type Book,
  blockAt,
  contents,
  counterpart,
  editionOrder,
  entryAt,
  firstSentences,
  getBook,
  lineAt,
  nextEdition,
  pinnedBlocks,
  pinRequest,
  putPin,
  columns,
  sentenceAt,
  sentenceMap,
  sentenceSpan,
  sentencesIn,
  sentenceTexts,
  span,
  starts,
  through,
} from './book'
import {
  type Anchor,
  kept,
  type Pages,
  pagesOf,
  pageTurnedBy,
  show,
  shows,
  topBlock,
  topOf,
  turn,
} from './bookPages'
import { LookupPanel, type Looked } from './BookLookup'
import { saveFile } from './saveFile'
import { NoteEditor, NoteList, SelectionBar } from './BookNotes'
import type { Note, TextRange } from './notes'
import {
  ankiCsv,
  meaning,
  newNote,
  notesOn,
  pieces,
  type Segment,
  type Selected,
  selectionIn,
} from './bookNotes'
import { BookProgress } from './BookProgress'
import { Contents } from './Contents'
import { debounce } from './debounce'
import { useGate } from './gate'
import { TAP_HOLD } from './gestures'
import { Help } from './Help'
import { usePrefsMap } from './hooks'
import { controlKind, inPanel, type Intent } from './keys'
import {
  buildRegistry,
  changeSlice,
  fieldChanged,
  sliceFor,
  sliceReset,
} from './module'
import { BOOK_LAYOUTS } from './layouts'
import { bookLayoutModule } from './modules/bookLayout'
import {
  bookNotesModule,
  bookPinsModule,
  LIST_INTENT,
  NEW_INTENT,
  PIN_INTENT,
  REDO_INTENT,
  UNDO_INTENT,
} from './modules/bookNotes'
import {
  bookTextModule,
  resized,
  stepped,
  TEXT_SIZE_INTENTS,
} from './modules/bookText'
import {
  contentsModule,
  TOGGLE_INTENT as CONTENTS_TOGGLE,
} from './modules/contents'
import {
  enabledModules,
  helpModule,
  TOGGLE_INTENT as HELP_TOGGLE,
} from './modules/help'
import {
  fadeStyle,
  isAnimating,
  motionModule,
  scheduleFade,
} from './modules/motion'
import { TOGGLE_INTENT as PEEK_TOGGLE, peekModule } from './modules/peek'
import { RATES, READ_INTENT, readAloudModule } from './modules/readAloud'
import { hasVoice, type Stop, speak } from './speech'
import { readoutRow, timeLine } from './modules/readouts'
import {
  settingsModule,
  TOGGLE_INTENT as SETTINGS_TOGGLE,
} from './modules/settings'
import { DEFAULT_COLOUR, noteColour } from './palette'
import { loadPrefs, resetPrefs, savePrefs } from './prefs'
import { Settings } from './Settings'
import { useDocumentTitle } from './title'
import { wheelNotches } from './viewport'
import { TopBar } from './TopBar'
import { useBookNotes } from './useBookNotes'

interface Props {
  work: string
  onLeave: () => void
}

interface Place {
  edition: string
  block: number
  /** The character of the block's words to keep on screen: `Anchor.at`. */
  at?: number
}

type Panel =
  | 'help'
  | 'settings'
  | 'account'
  | 'contents'
  | 'note'
  | 'notes'
  | 'lookup'

/** The panel each module's toggle key opens and closes. */
const PANELS: Record<string, Panel> = {
  [SETTINGS_TOGGLE]: 'settings',
  [HELP_TOGGLE]: 'help',
  [CONTENTS_TOGGLE]: 'contents',
  [LIST_INTENT]: 'notes',
}

/**
 * The most characters one move of the text may carry and still count as
 * reading: a jump by the contents, the progress line or a swap is not.
 */
const MOST_READ_AT_ONCE = 3000

/** How long scrolling must stop before the position is saved. */
const SAVE_AFTER_MS = 1000

/** How far a press may travel and still be a tap rather than a scroll. */
const TAP_SLOP_PX = 8

/** How soon a second tap must follow the first to make them a double tap. */
const DOUBLE_TAP_MS = 300

/**
 * How long a tap waits before acting, so the second tap of a quick double tap
 * stops it before the bars move.
 */
const TAP_MS = 200

/** How long a swap's blocks stay lit. */
const LIT_MS = 1500

/** How long a swap's light takes to fade, with motion on. */
const LIT_FADE_MS = 600

const registry = buildRegistry([
  motionModule,
  settingsModule,
  helpModule,
  bookTextModule,
  bookLayoutModule,
  contentsModule,
  peekModule,
  readAloudModule,
  bookNotesModule,
  bookPinsModule,
])

const rootStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  position: 'relative',
}

export function BookReader({ work, onLeave }: Props) {
  const gate = useGate()
  const [book, setBook] = useState<Book>()
  const [failed, setFailed] = useState<string>()
  const [place, setPlace] = useState<Place>()
  const [top, setTop] = useState(0)
  const [bars, setBars] = useState(false)
  const [panel, setPanel] = useState<Panel>()
  const [peek, setPeek] = useState<Peeked>()
  const [paired, setPaired] = useState<Paired>()
  const [lit, setLit] = useState<[number, number]>()
  const [prefs, setPrefs] = usePrefsMap(registry.all(), loadPrefs, savePrefs)
  const text = useRef<HTMLDivElement>(null)
  const anchor = useRef<Anchor>({ block: 0, at: 0 })
  const pressed = useRef<{ x: number; y: number; at: number }>(undefined)
  const lastTap = useRef<{ at: number; toggledBars: boolean }>(undefined)
  const tapWait = useRef<number>(undefined)
  useEffect(() => () => window.clearTimeout(tapWait.current), [])
  const notes = useBookNotes(work)
  const [openNote, setOpenNote] = useState<string>()
  const [selected, setSelected] = useState<Selected>()
  const [pinning, setPinning] = useState<{ left?: number }>()
  const [notice, setNotice] = useState<string>()
  const [lookingUp, setLookingUp] = useState<{
    block: number
    range: TextRange
    word: string
    lang: string | null
  }>()
  const [looked, setLooked] = useState<Looked>({ kind: 'looking' })
  useEffect(() => {
    if (!lookingUp) return
    if (!lookingUp.lang) {
      setLooked({ kind: 'none', lang: 'a book that names no language' })
      return
    }
    let live = true
    const lang = lookingUp.lang
    setLooked({ kind: 'looking' })
    lookUpWord(lang, lookingUp.word)
      .then((entries) => {
        if (!live) return
        setLooked(entries ? { kind: 'found', entries } : { kind: 'none', lang })
      })
      .catch((why: unknown) => {
        if (live) setLooked({ kind: 'failed', why: String(why) })
      })
    return () => {
      live = false
    }
  }, [lookingUp])
  // The sentence being read aloud, numbered through the edition.
  const [speaking, setSpeaking] = useState<number>()
  const stopReading = useRef<Stop>(undefined)
  const session = useRef({
    started: Date.now(),
    paced: { since: Date.now(), read: 0 },
  })
  const admin = gate?.who.admin === true

  const motion = sliceFor(prefs, motionModule)
  const typeset = sliceFor(prefs, bookTextModule)
  const {
    mode,
    paged: pagedPref,
    hideTranslation,
  } = sliceFor(prefs, bookLayoutModule)
  const [pages, setPages] = useState<Pages>()
  const animating = isAnimating(motion)
  const [shown, setShown] = useState(!animating)
  useEffect(() => scheduleFade(bars, animating, setShown), [bars, animating])
  useDocumentTitle(book?.title)

  useEffect(() => {
    if (!lit) return
    const done = window.setTimeout(() => setLit(undefined), LIT_MS)
    return () => window.clearTimeout(done)
  }, [lit])

  // Built once an edition, a light and a peek, since every scroll draws the
  // reader again and an edition runs to thousands of blocks.
  const edition = book && place ? book.editions[place.edition] : undefined
  const layout = useMemo(
    () =>
      edition && {
        entries: contents(edition.blocks),
        at: starts(edition.blocks),
      },
    [edition],
  )
  const firsts = useMemo(
    () => edition && firstSentences(edition.blocks),
    [edition],
  )
  const peeked = useMemo(() => {
    if (!book || !place || !peek) return undefined
    const to = nextEdition(book, place.edition)
    const other = book.editions[to]
    const texts =
      peek.sentence === undefined
        ? other.blocks
            .slice(...span(book, place.edition, to, peek.block))
            .map((b) => b.text)
        : [
            sentencesIn(
              other,
              sentenceSpan(book, place.edition, to, peek.sentence),
            ).join(' '),
          ].filter((t) => t !== '')
    return {
      block: peek.block,
      label: other.label,
      language: other.language,
      texts,
    }
  }, [book, place, peek])
  const other =
    book && place && editionOrder(book).length > 1
      ? nextEdition(book, place.edition)
      : undefined
  const comparing = mode === 'compare' && other !== undefined
  const shownEdition = place?.edition
  // The editions compared besides the one read, kept a work at a time in
  // this browser, since one book's editions are not another's.
  const [others, setOthers] = useState<string[]>(() => keptColumns(work))
  useEffect(() => {
    try {
      localStorage.setItem(columnsKey(work), JSON.stringify(others))
    } catch {
      // Remembered where storage allows, and chosen again where not.
    }
  }, [work, others])
  const columnKeys = useMemo(() => {
    if (!book || !shownEdition) return []
    const chosen = [
      ...new Set(
        others.filter((k) => k in book.editions && k !== shownEdition),
      ),
    ]
    if (chosen.length === 0 && other) chosen.push(other)
    return [shownEdition, ...chosen]
  }, [book, shownEdition, others, other])
  // A swap to a compared edition puts it first and the one left in its place.
  const lastRead = useRef<string>(undefined)
  useEffect(() => {
    const left = lastRead.current
    const now = shownEdition
    lastRead.current = now
    if (!left || !now || left === now) return
    setOthers((kept) => kept.map((k) => (k === now ? left : k)))
  }, [shownEdition])
  const pinnable = comparing && columnKeys.length === 2
  const interlinear = mode === 'interlinear' && other !== undefined
  const [revealed, setRevealed] = useState<ReadonlySet<number>>(new Set())
  // Each sentence's translation, worked out once an edition rather than a
  // sentence at a time, since an interlinear book sets one under every one.
  const under = useMemo(
    () =>
      interlinear && book && shownEdition && other
        ? {
            map: sentenceMap(book, shownEdition, other),
            texts: sentenceTexts(book.editions[other]),
            language: book.editions[other].language,
          }
        : undefined,
    [interlinear, book, shownEdition, other],
  )
  // A translation tapped open is closed again by a new edition or layout.
  // biome-ignore lint/correctness/useExhaustiveDependencies: what resets it, not what it reads
  useEffect(() => {
    setRevealed(new Set())
  }, [shownEdition, mode, hideTranslation])
  // A panel whose note went, by an undo or another device's edit, closes,
  // and pinning ends with the comparison it picks from.
  useEffect(() => {
    if (panel === 'note' && !notes.notes.some((n) => n.id === openNote)) {
      setPanel(undefined)
    }
  }, [panel, notes.notes, openNote])
  useEffect(() => {
    if (!pinnable) setPinning(undefined)
  }, [pinnable])
  useEffect(() => {
    if (!comparing) setPaired(undefined)
  }, [comparing])

  const shownNotes = useMemo(
    () => book && place && notesOn(book, place.edition, notes.notes),
    [book, place, notes.notes],
  )
  // The page follows the sentence read aloud, turned only once it is off it.
  useEffect(() => {
    if (speaking === undefined || !edition || !firsts || !place) return
    const block = firsts.findLastIndex((first) => first <= speaking)
    const range = edition.blocks[block]?.sentences[speaking - firsts[block]]
    if (!range) return
    const at = { block, at: range[0] }
    if (place.block === at.block && place.at === at.at) return
    if (!shows(text.current, at)) setPlace({ edition: place.edition, ...at })
  }, [speaking, edition, firsts, place])
  useEffect(() => () => stopReading.current?.(), [])

  const paragraphs = useMemo(() => {
    const litSentence: [number, number] | undefined =
      speaking !== undefined
        ? [speaking, speaking + 1]
        : peek?.sentence !== undefined
          ? [peek.sentence, peek.sentence + 1]
          : undefined
    return edition?.blocks.map((block, i) => {
      const Block = block.heading ? 'h2' : 'p'
      const here = shownNotes?.get(i) ?? []
      const highlights = here
        .filter(({ note, own }) => own && note.text)
        .map(({ note }) => note)
      return (
        <Block
          key={i}
          data-block={i}
          data-line={block.line ?? undefined}
          data-lit={(lit && i >= lit[0] && i < lit[1]) || undefined}
        >
          {here
            .filter(({ note, own }) => !own || !note.text)
            .map(({ note }) => (
              <span
                key={note.id}
                className="book-note-mark"
                data-note={note.id}
                title={note.body || 'a note'}
                style={{ background: noteColour(note.color).edge }}
              />
            ))}
          <span data-text>
            <Sentences
              block={block}
              first={firsts?.[i] ?? 0}
              highlights={highlights}
              lit={litSentence}
              asideAfter={(n) => (
                <>
                  {peeked?.block === i && peek?.sentence === n && (
                    <Peek {...peeked} />
                  )}
                  {under && (
                    <Interlinear
                      n={n}
                      under={under}
                      hidden={hideTranslation && !revealed.has(n)}
                    />
                  )}
                </>
              )}
            />
          </span>
          {peeked?.block === i && peek?.sentence === undefined && (
            <Peek {...peeked} />
          )}
        </Block>
      )
    })
  }, [
    edition,
    firsts,
    lit,
    peek,
    peeked,
    shownNotes,
    under,
    hideTranslation,
    revealed,
    speaking,
  ])
  const compared = useMemo(() => {
    if (!book || !place || !comparing) return undefined
    const pinned =
      columnKeys.length === 2
        ? pinnedBlocks(book, columnKeys[0], columnKeys[1])
        : new Set()
    // A sentence picked in any column lights it and what each other column
    // has for it.
    const litIn = (key: string): [number, number] | undefined =>
      paired === undefined
        ? undefined
        : paired.edition === key
          ? [paired.sentence, paired.sentence + 1]
          : sentenceSpan(book, paired.edition, key, paired.sentence)
    const cell = (
      key: string,
      column: number,
      [from, to]: [number, number],
    ) => {
      const blocks = book.editions[key].blocks
      const first = firstSentences(blocks)
      // The first two columns are the two a pin joins.
      const side = column === 0 ? 'left' : column === 1 ? 'right' : `c${column}`
      return (
        <div
          key={key}
          className="book-cell"
          data-column={key}
          lang={book.editions[key].language ?? undefined}
        >
          {blocks.slice(from, to).map((block, i) => {
            const Block = block.heading ? 'h2' : 'p'
            return (
              <Block
                key={i}
                data-pick={`${side}:${from + i}`}
                data-line={block.line ?? undefined}
              >
                <Sentences
                  block={block}
                  first={first[from + i]}
                  highlights={[]}
                  lit={litIn(key)}
                />
              </Block>
            )
          })}
        </div>
      )
    }
    return columns(book, columnKeys).map((row) => (
      <div
        key={row.map(([start]) => start).join('-')}
        className="book-row"
        data-block={row[0][0]}
        data-pinned={
          (row[0][1] > row[0][0] && pinned.has(row[0][0])) || undefined
        }
      >
        {columnKeys.map((key, column) => cell(key, column, row[column]))}
      </div>
    ))
  }, [book, place, comparing, columnKeys, paired])

  useEffect(() => {
    let live = true
    Promise.all([getBook(work), getProgress(work).catch(() => undefined)])
      .then(([loaded, saved]) => {
        if (!live) return
        const resumed = saved && saved.edition in loaded.editions
        setBook(loaded)
        // A saved block past the end is one a shorter copy of the book, read
        // again since, no longer has; its last block is the nearest.
        setPlace(
          resumed
            ? {
                edition: saved.edition,
                block: Math.max(
                  0,
                  Math.min(
                    saved.slot,
                    loaded.editions[saved.edition].blocks.length - 1,
                  ),
                ),
              }
            : { edition: loaded.hub, block: 0 },
        )
      })
      .catch((why: unknown) => {
        if (live) setFailed(String(why))
      })
    return () => {
      live = false
    }
  }, [work])

  const save = useMemo(
    () =>
      debounce(
        (at: Place) =>
          void putProgress(work, { edition: at.edition, slot: at.block }).catch(
            () => undefined,
          ),
        SAVE_AFTER_MS,
      ),
    [work],
  )
  useEffect(() => () => save.flush(), [save])

  // A swap, the book's first draw, or the text laid out again scrolls to the
  // place, and the place is the anchor until a scroll takes it off the page.
  // Scrolling by hand sets no `place`, so it never fights the reader.
  useLayoutEffect(() => {
    if (!place) return
    show(text.current, place.block, place.at)
    anchor.current = { block: place.block, at: place.at ?? 0 }
    setTop(place.block)
    setPages(pagesOf(text.current, layout?.entries ?? [], place.block))
  }, [place, layout])

  // Paged, the wheel sets the text larger or smaller as it zooms a page in the
  // page reader, a step a notch, the notches of a smooth wheel added up. The
  // text is drawn only once the book has loaded, so that is when it listens.
  const loaded = layout !== undefined
  useEffect(() => {
    const host = text.current
    if (!host || !pagedPref || !loaded) return
    let notches = 0
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      notches += wheelNotches(event) * (event.deltaY < 0 ? 1 : -1)
      const steps = Math.trunc(notches)
      if (steps === 0) return
      notches -= steps
      setPlace(
        (p) => p && { edition: p.edition, ...kept(host, anchor.current) },
      )
      setPrefs((map) =>
        changeSlice(map, bookTextModule, (p) => stepped(p, steps)),
      )
    }
    host.addEventListener('wheel', onWheel, { passive: false })
    return () => host.removeEventListener('wheel', onWheel)
  }, [pagedPref, loaded, setPrefs])

  // A resized window lays the text out again, so it goes back to the anchor
  // the last scroll left, which a paged layout would otherwise slide off. Not
  // measured now: the text has already been laid out at the new width.
  useEffect(() => {
    const keep = () =>
      setPlace((p) => p && { edition: p.edition, ...anchor.current })
    window.addEventListener('resize', keep)
    return () => window.removeEventListener('resize', keep)
  }, [])

  useEffect(() => {
    function swapTo(to: string) {
      if (!book || !place || to === place.edition) return
      const from = topBlock(text.current)
      const block = counterpart(book, place.edition, to, from)
      setPlace({ edition: to, block })
      setLit(span(book, place.edition, to, from))
      setPeek(undefined)
      setPaired(undefined)
      stopReading.current?.()
      // Each edition is read at its own pace in characters: a translation
      // runs shorter or longer than its original.
      session.current.paced = { since: Date.now(), read: 0 }
      save({ edition: to, block })
    }
    // Aloud from the sentence the page or screen starts with, in the
    // edition's own language, which a device with no voice for it says
    // rather than reading in another's.
    function readFromTop() {
      if (!book || !place) return
      const reading = book.editions[place.edition]
      const lang = reading.language
      if (lang && hasVoice(lang) === false) {
        setNotice(`This device has no voice for ${lang}.`)
        return
      }
      const top = topOf(text.current)
      const { rate } = sliceFor(prefs, readAloudModule)
      let ended = false
      const stop = speak(
        sentenceTexts(reading),
        sentenceAt(reading.blocks, top.block, top.at),
        lang,
        Math.min(RATES[1], Math.max(RATES[0], rate)),
        {
          onSentence: setSpeaking,
          onEnd: () => {
            ended = true
            stopReading.current = undefined
            setSpeaking(undefined)
          },
          onFail: () =>
            setNotice(
              `This device could not read ${lang ?? 'this book'} aloud.`,
            ),
        },
      )
      // A stop kept after its reading ended would take Escape and `r` from
      // then on, doing nothing with them.
      if (!ended) stopReading.current = stop
    }
    function act(intent: Intent) {
      if (!book || !place) return
      // Compared, a swap reads the second column, so the columns turn as
      // they are seen rather than by the editions' order.
      if (intent.kind === 'toggle') {
        swapTo(
          comparing && columnKeys[1]
            ? columnKeys[1]
            : nextEdition(book, place.edition),
        )
      } else if (intent.kind === 'view') swapTo(intent.edition)
      else if (intent.kind === 'account') setPanel('account')
      else if (intent.kind === 'step') turn(text.current, intent.by)
      else if (intent.kind === 'library') onLeave()
    }
    function onKey(event: KeyboardEvent) {
      if (!book || event.metaKey || event.ctrlKey || event.altKey) return
      // A field takes every key, a button keeps the keys that press it, and
      // a panel keeps its own but Escape, which closes it.
      const control = controlKind(event.target)
      if (control === 'text') return
      if (
        control === 'button' &&
        (event.key === ' ' || event.key === 'Enter')
      ) {
        return
      }
      if (event.key !== 'Escape' && inPanel(event.target)) return
      if (event.key === 'Escape') {
        if (stopReading.current) stopReading.current()
        else if (pinning) setPinning(undefined)
        else if (selected) {
          window.getSelection()?.removeAllRanges()
          setSelected(undefined)
        } else if (peek) setPeek(undefined)
        else if (paired) setPaired(undefined)
        else if (panel) setPanel(undefined)
        else setBars(false)
        return
      }
      if (event.key === 'Tab' && !bars) {
        event.preventDefault()
        setBars(true)
        return
      }
      const binding = BOOK_KEYS.find((b) => b.keys.includes(event.key))
      const intent = binding?.resolve(event.key, editionOrder(book))
      if (intent) {
        event.preventDefault()
        act(intent)
        return
      }
      const own = registry
        .all()
        .flatMap((m) => m.keys ?? [])
        .find((k) => k.key === event.key)
      if (own?.intent === PEEK_TOGGLE) {
        const at = topBlock(text.current)
        setPeek((p) =>
          p?.block === at && p.sentence === undefined
            ? undefined
            : { block: at },
        )
      }
      const to = own && PANELS[own.intent]
      if (to) setPanel((p) => (p === to ? undefined : to))
      if (place && notes.ready && own?.intent === NEW_INTENT) {
        const note = newNote(
          place.edition,
          topBlock(text.current),
          DEFAULT_COLOUR,
          new Date(),
        )
        notes.change([...notes.notes, note])
        setOpenNote(note.id)
        setPanel('note')
      }
      if (own?.intent === READ_INTENT) {
        if (stopReading.current) stopReading.current()
        else readFromTop()
      }
      if (own?.intent === UNDO_INTENT) notes.undo()
      if (own?.intent === REDO_INTENT) notes.redo()
      if (own?.intent === PIN_INTENT && admin && pinnable) {
        setPinning((p) => (p ? undefined : {}))
      }
      if (place && own && TEXT_SIZE_INTENTS.has(own.intent)) {
        setPlace({
          edition: place.edition,
          ...kept(text.current, anchor.current),
        })
        setPrefs((map) =>
          changeSlice(map, bookTextModule, (p) => resized(p, own.intent)),
        )
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [
    book,
    place,
    panel,
    peek,
    paired,
    bars,
    save,
    onLeave,
    setPrefs,
    notes,
    admin,
    pinning,
    selected,
    prefs,
    pinnable,
    comparing,
    columnKeys,
  ])

  if (failed) {
    return (
      <Message onLeave={onLeave}>
        This book could not be opened: {failed}
      </Message>
    )
  }
  if (!book || !place) {
    return <main className="reader book-reader" aria-busy="true" />
  }
  if (!edition || !layout) {
    return (
      <Message onLeave={onLeave}>
        This work has no book edition to read.
      </Message>
    )
  }

  const { entries, at } = layout
  const progress = through(at, top)
  const reading = entryAt(entries, top)
  const entry = entries[reading]
  const line = lineAt(edition.blocks, top)
  const position = `${
    pages ? `page ${pages.at} of ${pages.of}` : `${Math.round(progress * 100)}%`
  }${line === undefined ? '' : ` · line ${line}`}`
  const chapter =
    entry &&
    `${entry.lines.at(-1)} · ${
      pages?.chapter
        ? `page ${pages.chapter.at} of ${pages.chapter.of}`
        : `${Math.round(
            through(at, top, entry.block, entries[reading + 1]?.block) * 100,
          )}% of the chapter`
    }`
  const elapsed = Date.now() - session.current.started
  // A pace over less than a minute's reading says nothing of the chapter.
  const paced = Date.now() - session.current.paced.since
  const pace =
    paced >= 60_000 ? session.current.paced.read / (paced / 60_000) : 0
  const ahead =
    at[entries[reading + 1]?.block ?? edition.blocks.length] - at[top]
  const time = `${timeLine({ elapsedMs: elapsed, pagesPerMin: pace, slotsLeft: ahead })}${
    pace > 0 && entry ? ' in the chapter' : ''
  }`
  const fade = fadeStyle(motion, shown)
  const toggle = (to: Panel) => setPanel((p) => (p === to ? undefined : to))
  const opening = (note: string) => {
    setOpenNote(note)
    setPanel('note')
  }
  const highlight = (colour: string, thenWrite: boolean) => {
    if (!selected) return
    const note = newNote(
      place.edition,
      selected.block,
      colour,
      new Date(),
      selected.range,
    )
    notes.change([...notes.notes, note])
    window.getSelection()?.removeAllRanges()
    setSelected(undefined)
    if (thenWrite) opening(note.id)
  }
  const pick = async (side: 'left' | 'right', block: number | null) => {
    if (!pinning || !pinnable) return
    if (side === 'left') {
      setPinning({ left: block ?? undefined })
      return
    }
    if (pinning.left === undefined) return
    const request = pinRequest(
      book,
      columnKeys[0],
      columnKeys[1],
      pinning.left,
      block,
    )
    setPinning(undefined)
    if (!request) {
      setNotice(
        'A pin joins the hub to another edition; compare one with the hub.',
      )
      return
    }
    try {
      await putPin(work, request)
      setBook(await getBook(work))
      setPlace({
        edition: place.edition,
        ...kept(text.current, anchor.current),
      })
      setNotice(undefined)
    } catch (why) {
      setNotice(`The pin was not kept: ${String(why)}`)
    }
  }
  const editing = notes.notes.find((note) => note.id === openNote)
  // A setting that redraws the text, its size or its layout, would leave the
  // reader wherever the old scroll offset now falls, so it keeps the anchor.
  const change = (id: string, key: string, raw: unknown) => {
    setPlace({ edition: place.edition, ...kept(text.current, anchor.current) })
    setPrefs((map) => fieldChanged(map, registry, id, key, raw))
  }
  const go = (block: number) => {
    setPlace({ edition: place.edition, block })
    save({ edition: place.edition, block })
  }
  const textStyle = {
    '--book-text-size': `${typeset.textSize}px`,
    '--book-line-spacing': typeset.lineSpacing,
    // In pixels rather than ems, which each element would read against its
    // own size: a heading, set larger, would take a wider column than the
    // paragraphs and sit out of line with them.
    '--book-column-width': `${typeset.columnWidth * typeset.textSize}px`,
    '--book-lit-fade': `${animating ? LIT_FADE_MS : 0}ms`,
  } as CSSProperties

  return (
    <main className="reader book-reader" style={rootStyle}>
      {bars && (
        <TopBar
          title={book.title}
          editionLine={
            comparing
              ? columnKeys.map((k) => book.editions[k].label).join(' · ')
              : interlinear
                ? `${edition.label} · ${book.editions[other].label}`
                : edition.label
          }
          layouts={other === undefined ? undefined : BOOK_LAYOUTS}
          mode={mode}
          onMode={(to) => change(bookLayoutModule.id, 'mode', to)}
          paged={pagedPref}
          onPaged={() => change(bookLayoutModule.id, 'paged', !pagedPref)}
          settingsOpen={panel === 'settings'}
          onSettings={() => toggle('settings')}
          onLeave={onLeave}
          style={fade}
        />
      )}
      {comparing && (
        <ColumnsBar
          book={book}
          shown={columnKeys}
          onChange={(next) => setOthers(next.slice(1))}
        />
      )}
      <div
        className="book-text"
        ref={text}
        style={
          comparing
            ? ({
                ...textStyle,
                '--columns': columnKeys.length,
              } as CSSProperties)
            : textStyle
        }
        lang={edition.language ?? undefined}
        data-typeface={typeset.typeface}
        data-page={typeset.page}
        data-justify={typeset.justify || undefined}
        data-indent={typeset.indent || undefined}
        data-paged={pagedPref || undefined}
        data-compared={comparing || undefined}
        onScroll={() => {
          const now = topBlock(text.current)
          const read = at[now] - at[top]
          if (read > 0 && read <= MOST_READ_AT_ONCE) {
            session.current.paced.read += read
          }
          setSelected(undefined)
          if (!shows(text.current, anchor.current)) {
            anchor.current = topOf(text.current)
          }
          setTop(now)
          setPages(pagesOf(text.current, entries, now))
          save({ edition: place.edition, block: now })
        }}
        onPointerDown={(event) => {
          setSelected(undefined)
          pressed.current = {
            x: event.clientX,
            y: event.clientY,
            at: event.timeStamp,
          }
        }}
        onPointerUp={(event) => {
          const down = pressed.current
          if (
            !down ||
            Math.hypot(event.clientX - down.x, event.clientY - down.y) >
              TAP_SLOP_PX ||
            event.timeStamp - down.at > TAP_HOLD
          ) {
            if (notes.ready) setSelected(selectionIn(text.current))
            return
          }
          const target = event.target as Element
          const picked = target
            .closest<HTMLElement>('[data-pick]')
            ?.dataset.pick?.split(':')
          if (pinning && picked) {
            void pick(picked[0] as 'left' | 'right', Number(picked[1]))
            return
          }
          const noted = target.closest<HTMLElement>('[data-note]')?.dataset.note
          const hiddenLine = hideTranslation
            ? target.closest<HTMLElement>('[data-interlinear]')?.dataset
                .interlinear
            : undefined
          // A tap acts after `TAP_MS`; a second tap before then stops it,
          // and one later in `DOUBLE_TAP_MS` undoes it. Either peeks, taking
          // back what the browser selected for a double or triple click.
          const last = lastTap.current
          if (last && event.timeStamp - last.at < DOUBLE_TAP_MS) {
            lastTap.current = undefined
            if (tapWait.current !== undefined) {
              window.clearTimeout(tapWait.current)
              tapWait.current = undefined
            } else if (last.toggledBars) {
              setBars((b) => !b)
            }
            window.getSelection()?.removeAllRanges()
            const said = target.closest<HTMLElement>('[data-sentence]')
            const sentence = said ? Number(said.dataset.sentence) : undefined
            if (comparing) {
              const column =
                target.closest<HTMLElement>('[data-column]')?.dataset.column
              if (column && sentence !== undefined) {
                setPaired((p) =>
                  p?.edition === column && p.sentence === sentence
                    ? undefined
                    : { edition: column, sentence },
                )
              }
              return
            }
            const block = target.closest<HTMLElement>('[data-block]')
            const peeking = Number(block?.dataset.block)
            if (block) {
              setPeek((p) =>
                p?.block === peeking && p.sentence === sentence
                  ? undefined
                  : { block: peeking, sentence },
              )
            }
            return
          }
          const page = pageTurnedBy(text.current, event.clientX)
          lastTap.current = {
            at: event.timeStamp,
            toggledBars: !panel && !noted && !hiddenLine && page === 0,
          }
          tapWait.current = window.setTimeout(() => {
            tapWait.current = undefined
            if (noted) opening(noted)
            else if (hiddenLine) {
              const n = Number(hiddenLine)
              setRevealed((r) => {
                const next = new Set(r)
                if (!next.delete(n)) next.add(n)
                return next
              })
            } else if (panel) setPanel(undefined)
            else if (page !== 0) turn(text.current, page)
            else setBars((b) => !b)
          }, TAP_MS)
        }}
      >
        {compared ?? paragraphs}
      </div>
      {bars && (
        <div
          className="reader-bottom-bar"
          style={{ ...bottomBarStyle, ...fade }}
        >
          <BookProgress
            progress={progress}
            ticks={entries.map((e) => through(at, e.block))}
            label={position}
            onSeek={(fraction) => go(blockAt(at, fraction))}
          />
          <div className="reader-readout" style={readoutStyle}>
            <button
              type="button"
              className="reader-button"
              aria-expanded={panel === 'contents'}
              onClick={() => toggle('contents')}
            >
              Contents
            </button>
            <span style={readoutTextStyle}>
              {readoutRow({ position, time, chapter })}
            </span>
          </div>
        </div>
      )}
      {pinning && (
        <div className="book-pinning" role="status">
          <span>
            {pinning.left === undefined
              ? 'Pick a paragraph on the left to pin.'
              : 'Now pick its match on the right.'}
          </span>
          {pinning.left !== undefined &&
            pinnable &&
            pinnedBlocks(book, columnKeys[0], columnKeys[1]).has(
              pinning.left,
            ) && (
              <button
                type="button"
                className="reader-button"
                onClick={() => void pick('right', null)}
              >
                Unpin
              </button>
            )}
          <button
            type="button"
            className="reader-button"
            onClick={() => setPinning(undefined)}
          >
            Cancel
          </button>
        </div>
      )}
      {(notes.notice ?? notice) && (
        <p className="book-notice" role="status">
          {notes.notice ?? notice}
        </p>
      )}
      {selected && (
        <SelectionBar
          at={selected.at}
          onHighlight={(colour) => highlight(colour, false)}
          onNote={() => highlight(DEFAULT_COLOUR, true)}
          onLookUp={() => {
            const block = edition.blocks[selected.block]
            setLookingUp({
              block: selected.block,
              range: selected.range,
              word: block.text.slice(selected.range.start, selected.range.end),
              lang: edition.language,
            })
            window.getSelection()?.removeAllRanges()
            setSelected(undefined)
            setPanel('lookup')
          }}
        />
      )}
      {panel === 'note' && editing && (
        <NoteEditor
          key={`${editing.id}:${editing.updated}`}
          book={book}
          note={editing}
          onChange={(next) =>
            notes.change(
              notes.notes.map((note) => (note.id === next.id ? next : note)),
            )
          }
          onDelete={() => {
            notes.change(notes.notes.filter((note) => note.id !== editing.id))
            setPanel(undefined)
          }}
          onClose={() => setPanel(undefined)}
        />
      )}
      {panel === 'lookup' && lookingUp && (
        <LookupPanel
          word={lookingUp.word}
          looked={looked}
          onSave={(entry) => {
            const note = {
              ...newNote(
                place.edition,
                lookingUp.block,
                DEFAULT_COLOUR,
                new Date(),
                lookingUp.range,
              ),
              body: meaning(entry),
            }
            notes.change([...notes.notes, note])
            setPanel(undefined)
          }}
          onClose={() => setPanel(undefined)}
        />
      )}
      {panel === 'notes' && (
        <NoteList
          book={book}
          notes={notes.notes}
          onOpen={(note) => {
            setPlace({ edition: note.edition, block: note.slot })
            save({ edition: note.edition, block: note.slot })
            opening(note.id)
          }}
          onClose={() => setPanel(undefined)}
          onExport={() =>
            saveFile(`${book.title} words.csv`, ankiCsv(book, notes.notes))
          }
        />
      )}
      <Contents
        visible={panel === 'contents'}
        entries={entries}
        current={reading}
        onGo={go}
        onClose={() => setPanel(undefined)}
      />
      <Help
        visible={panel === 'help'}
        modules={enabledModules(registry.all(), prefs)}
        bindings={BOOK_KEYS}
        onClose={() => setPanel(undefined)}
      />
      <Settings
        visible={panel === 'settings'}
        modules={registry.all()}
        prefs={prefs}
        onChange={change}
        onReset={(id) => {
          if (!registry.get(id)) return
          resetPrefs(id)
          setPrefs((map) => sliceReset(map, registry, id))
        }}
        onClose={() => setPanel(undefined)}
        onAccount={() => setPanel('account')}
      />
      <Account
        visible={panel === 'account'}
        onClose={() => setPanel(undefined)}
        onSignedOut={() => gate?.signOut()}
      />
    </main>
  )
}

/** A book that will not open, or a work with no book: why, and the way back. */
function Message({
  children,
  onLeave,
}: {
  children: ReactNode
  onLeave: () => void
}) {
  return (
    <main className="reader book-reader" style={rootStyle}>
      <p className="book-failed">{children}</p>
      <button type="button" className="reader-button" onClick={onLeave}>
        Library
      </button>
    </main>
  )
}

/**
 * The compared editions' names, each over its column: the one read first,
 * then each other with a choice of edition and a way to drop it, and a way to
 * add one while any is left out.
 */
function ColumnsBar({
  book,
  shown,
  onChange,
}: {
  book: Book
  shown: string[]
  onChange: (next: string[]) => void
}) {
  const left = editionOrder(book).filter((k) => !shown.includes(k))
  return (
    <div
      className="book-columns"
      role="group"
      aria-label="Columns"
      style={{ '--columns': shown.length } as CSSProperties}
    >
      {shown.map((key, i) =>
        i === 0 ? (
          <span key={key} className="book-column-name">
            {book.editions[key].label}
          </span>
        ) : (
          <span key={key} className="book-column-name">
            <select
              aria-label={`Column ${i + 1}`}
              value={key}
              onChange={(event) =>
                onChange(
                  shown.map((k, j) => (j === i ? event.target.value : k)),
                )
              }
            >
              {[key, ...left].map((k) => (
                <option key={k} value={k}>
                  {book.editions[k].label}
                </option>
              ))}
            </select>
            {shown.length > 2 && (
              <button
                type="button"
                className="reader-button"
                aria-label={`Remove ${book.editions[key].label}`}
                onClick={() => onChange(shown.filter((_, j) => j !== i))}
              >
                ×
              </button>
            )}
          </span>
        ),
      )}
      {left.length > 0 && (
        <button
          type="button"
          className="reader-button book-column-add"
          aria-label="Add a column"
          onClick={() => onChange([...shown, left[0]])}
        >
          +
        </button>
      )}
    </div>
  )
}

/** Where a work's compared editions are kept in this browser. */
const columnsKey = (work: string) => `taiyaku.book-columns.${work}`

/** The editions last compared besides the one read, none where unknown. */
function keptColumns(work: string): string[] {
  try {
    const kept: unknown = JSON.parse(
      localStorage.getItem(columnsKey(work)) ?? '[]',
    )
    return Array.isArray(kept) ? kept.filter((k) => typeof k === 'string') : []
  } catch {
    return []
  }
}

/**
 * What a peek shows: the next edition's counterpart of a block, or of one of
 * its sentences, numbered through the edition.
 */
interface Peeked {
  block: number
  sentence?: number
}

/** A sentence picked in a comparison, on the side it was picked. */
interface Paired {
  edition: string
  sentence: number
}

/**
 * A block's words, each sentence its own span numbered through the edition
 * from `first`, so a tap can name it; those in `lit` are lit, and
 * `asideAfter` sets what follows one, a peek or its translation.
 */
function Sentences({
  block,
  first,
  highlights,
  lit,
  asideAfter,
}: {
  block: Block
  first: number
  highlights: Note[]
  lit?: [number, number]
  asideAfter?: (n: number) => ReactNode
}) {
  return pieces(block.text, block.sentences, highlights).map((piece, j) => {
    if (piece.sentence === undefined) {
      return <Fragment key={j}>{runsOf(piece.runs)}</Fragment>
    }
    const n = first + piece.sentence
    return (
      <Fragment key={j}>
        <span
          data-sentence={n}
          data-lit={(lit && n >= lit[0] && n < lit[1]) || undefined}
        >
          {runsOf(piece.runs)}
        </span>
        {asideAfter?.(n)}
      </Fragment>
    )
  })
}

/** Runs of a block's words, each highlight a `<mark>` in its note's colour. */
function runsOf(runs: Segment[]): ReactNode[] {
  return runs.map((run, j) =>
    run.note ? (
      <mark
        key={j}
        data-note={run.note.id}
        style={{ background: noteColour(run.note.color).tint }}
      >
        {run.text}
      </mark>
    ) : (
      run.text
    ),
  )
}

/**
 * The next edition's sentences for sentence `n`, set after the last of the
 * sentences that share them, so a translation that joins two is shown once;
 * `hidden` until tapped, to test oneself.
 */
function Interlinear({
  n,
  under,
  hidden,
}: {
  n: number
  under: {
    map: [number, number][]
    texts: string[]
    language: string | null
  }
  hidden: boolean
}) {
  const [start, end] = under.map[n] ?? [0, 0]
  const next = under.map[n + 1]
  if (start >= end || (next && next[0] === start && next[1] === end)) {
    return null
  }
  return (
    <span
      className="book-interlinear"
      data-aside
      data-interlinear={n}
      data-hidden={hidden || undefined}
      lang={under.language ?? undefined}
    >
      {under.texts.slice(start, end).join(' ')}
    </span>
  )
}

/** What the next edition has for a block, set under it. */
function Peek({
  label,
  language,
  texts,
}: {
  label: string
  language: string | null
  texts: string[]
}) {
  return (
    <span
      className="book-peek"
      data-aside
      role="note"
      aria-label={label}
      lang={language ?? undefined}
    >
      {texts.length === 0
        ? `${label} has nothing here.`
        : texts.map((t, i) => <span key={i}>{t}</span>)}
    </span>
  )
}
