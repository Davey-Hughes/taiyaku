/**
 * A book read as text, with Space swapping to the next edition at the block
 * that shows the same text.
 *
 * Its chrome is the page reader's: `TopBar`, the bottom bar's progress line and
 * readout row (`bars.ts`), and the `Help`, `Settings` and `Account` panels, fed
 * from a registry of its own whose `book-text` module sets the text. Its own
 * panels are `Contents`, `BookSearch`, `NoteEditor`, `NoteList` and
 * `LookupPanel`. Its keys are `BOOK_KEYS` and its modules' own, which `Help`
 * lists.
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
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { Account } from './Account'
import { getProgress, lookUpAt, lookUpWord, putProgress } from './api'
import { useBackdropClose } from './backdropClose'
import { bottomBarStyle, readoutStyle, readoutTextStyle } from './bars'
import {
  BOOK_KEYS,
  type Block,
  type Book,
  blockAt,
  bookImageUrl,
  contents,
  imageLinkLabel,
  counterpart,
  editionOrder,
  entryAt,
  type Excerpt,
  type Figure,
  excerpt,
  excerptsIn,
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
  sentenceExcerpts,
  sentenceSpan,
  sentenceTexts,
  span,
  starts,
  through,
} from './book'
import {
  type Anchor,
  addProgress,
  anchorOf,
  bringIntoSight,
  kept,
  movesAnchor,
  offScreen,
  type Pages,
  pagesOf,
  pageTurnedBy,
  show,
  shows,
  topBlock,
  topOf,
  turn,
  whole,
} from './bookPages'
import { LookupPanel, type Looked } from './BookLookup'
import { type LookupSide, seatLookup, unseatLookup } from './bookLookupSeat'
import { saveFile } from './saveFile'
import { SIDEBAR_LAYOUT } from './phoneLayout'
import { NoteEditor, NoteList, SelectionBar } from './BookNotes'
import type { Note, TextRange } from './notes'
import {
  ankiCsv,
  meaning,
  newNote,
  notesOn,
  offsetAtPoint,
  pieces,
  rangeIn,
  type Selected,
  selectionIn,
} from './bookNotes'
import { BookFootnote } from './BookFootnote'
import { BookProgress } from './BookProgress'
import { Linked, Runs } from './BookRuns'
import { BookSearch } from './BookSearch'
import { type Here, isJapanese, type Match, stepFrom } from './bookSearch'
import { Contents } from './Contents'
import { debounce } from './debounce'
import { useGate } from './gate'
import { TAP_HOLD, TAP_SLOP } from './gestures'
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
  bookSearchModule,
  NEXT_INTENT as SEARCH_NEXT,
  PREVIOUS_INTENT as SEARCH_PREVIOUS,
  TOGGLE_INTENT as SEARCH_TOGGLE,
} from './modules/bookSearch'
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
import { TOGGLE_INTENT as LOOKUP_TOGGLE, lookupModule } from './modules/lookup'
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
import { takeTapClick } from './tapClick'
import { useDocumentTitle } from './title'
import { wheelNotches } from './viewport'
import { TopBar } from './TopBar'
import { useBookNotes } from './useBookNotes'
import { beginsWord, headAt, wordAt } from './wordAt'

interface Props {
  work: string
  onLeave: () => void
}

interface Place {
  edition: string
  block: number
  /** The character of the block's words to keep on screen: `Anchor.at`. */
  at?: number
  /** The figure kept at the top instead: `Anchor.figure`. */
  figure?: number
  /** How far into what is kept the screen starts: `Anchor.into`. */
  into?: number
}

type Panel =
  | 'help'
  | 'settings'
  | 'account'
  | 'contents'
  | 'search'
  | 'note'
  | 'notes'
  | 'lookup'

/** The panels docked beside the text, rather than set over it as a dialog. */
const DOCKED: ReadonlySet<Panel> = new Set<Panel>([
  'help',
  'contents',
  'search',
  'note',
  'notes',
  'lookup',
])

/**
 * Whether the window is in the sidebar layout, where `book.css` sets the text
 * in beside a docked panel. Asked as the reader draws, which a panel opened or
 * closed and a resize both make it do. jsdom has no `matchMedia`, and is
 * taken to be in it.
 */
function sidebarLayout(): boolean {
  return (
    typeof window.matchMedia !== 'function' ||
    window.matchMedia(SIDEBAR_LAYOUT).matches
  )
}

/** The panel each module's toggle key opens and closes. */
const PANELS: Record<string, Panel> = {
  [SETTINGS_TOGGLE]: 'settings',
  [HELP_TOGGLE]: 'help',
  [CONTENTS_TOGGLE]: 'contents',
  [SEARCH_TOGGLE]: 'search',
  [LIST_INTENT]: 'notes',
}

/**
 * The most characters one move of the text by hand may carry and still count
 * as reading, so a drag of the scrollbar past chapters is not; a jump by the
 * contents, the progress line or a swap counts nothing, `place` having moved
 * `top` with it. A guess at a page's worth, which may be short of one at small
 * text on a tall screen (O324).
 */
const MOST_READ_AT_ONCE = 3000

/**
 * How long scrolling must stop before the position is saved. Longer than the
 * page reader's `PROGRESS_DEBOUNCE_MS`, which waits out a burst of arrow
 * presses: a scrolled text sends a stream of scroll events for as long as it
 * moves, and a slow steady scroll should not write at every pause for breath.
 */
const SAVE_AFTER_MS = 1000

/**
 * How soon a second tap must follow the first to make them a double tap.
 * Like `gestures.ts`' figures, this and the ones below were chosen by hand
 * while reading, not measured; one that feels wrong is changed here.
 */
const DOUBLE_TAP_MS = 300

/**
 * How far from a look-up's tap, along either axis, a tap off the text can land
 * and still be its second: about half a fingertip. Further off it is a press
 * of whatever it landed on, which may be a control the reader went straight to.
 */
const DOUBLE_TAP_REACH = 24

/**
 * How long a tap waits before acting, so the second tap of a quick double tap
 * stops it before the bars move.
 */
const TAP_MS = 200

/** How long a swap's blocks stay lit: long enough to find where it landed. */
const LIT_MS = 1500

/** How long a swap's light takes to fade, with motion on. */
const LIT_FADE_MS = 600

/**
 * What key `key`'s `intent` is in vertical text, which is read leftward: ←
 * steps on and → back. Page Down and Up step as they do in any book.
 */
function readLeftward(key: string, intent: Intent): Intent {
  if (intent.kind !== 'step') return intent
  if (key !== 'ArrowLeft' && key !== 'ArrowRight') return intent
  return { kind: 'step', by: intent.by === 1 ? -1 : 1 }
}

const registry = buildRegistry([
  motionModule,
  settingsModule,
  helpModule,
  bookTextModule,
  bookLayoutModule,
  contentsModule,
  bookSearchModule,
  peekModule,
  readAloudModule,
  bookNotesModule,
  bookPinsModule,
  lookupModule,
])

/**
 * Whether `to` lays the text out otherwise than `from` does: any setting of
 * the Text or Layout sections but the page's colour. Any other change leaves
 * every line where it is, so the reader's place is left alone with it.
 */
function laysOutAfresh(
  from: Record<string, object>,
  to: Record<string, object>,
): boolean {
  const fields = (map: Record<string, object>): Record<string, unknown> => ({
    ...sliceFor(map, bookTextModule),
    ...sliceFor(map, bookLayoutModule),
    page: undefined,
  })
  const [before, after] = [fields(from), fields(to)]
  return Object.keys(after).some((key) => before[key] !== after[key])
}

/**
 * The prefs with the look-up mode turned on or off. Anything stored but
 * `true` reads as off, so a press turns it on.
 */
const lookupToggled = (map: Record<string, object>) =>
  changeSlice(map, lookupModule, (p) => ({ ...p, on: p.on !== true }))

const rootStyle: CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  height: '100%',
  position: 'relative',
}

/**
 * The code the server files its Japanese dictionary under. An edition's own
 * tag may be `jpn` or `und`, or be missing, and the server would look for a
 * dictionary of that name.
 */
const JAPANESE_DICTIONARY = 'ja'

const LOOK_UP_PANEL = '[role="dialog"][aria-label="Look up"]'

/** The look-up's panel in the reader `text` is drawn in, if it is open. */
const lookUpPanelOf = (text: HTMLElement | null) =>
  text?.closest('.book-reader')?.querySelector<HTMLElement>(LOOK_UP_PANEL)

/** How far a text is scrolled. */
type ScrollSpot = { left: number; top: number }

/** A word being looked up. */
type LookingUp = {
  /** The edition the block is of, which a swap may since have left. */
  edition: string
  block: number
  range: TextRange
  word: string
  lang: string | null
  /**
   * How a tap looked it up, its range selected once known: by the run of
   * letters tapped, or as the longest word from the character tapped,
   * whose answer is `looked`.
   */
  tap?: 'word' | 'longest'
  /**
   * Whether a selection looked up had already left the screen, both of its
   * ends, which the reader scrolled on from: the look-up then leaves the
   * place alone.
   */
  offScreen?: boolean
}

/**
 * Whether `now` is the answer to the tap `was` asked with: the character
 * tapped, run on to the longest word found from it. Still the same word.
 */
const answers = (was: LookingUp, now: LookingUp) =>
  was.tap === 'longest' &&
  now.tap === 'longest' &&
  was.range.start === was.range.end &&
  was.edition === now.edition &&
  was.block === now.block &&
  was.range.start === now.range.start

/** Whether `text` is scrolled to exactly `spot`. */
const scrolledTo = (text: HTMLElement | null, spot?: ScrollSpot) =>
  spot?.left === text?.scrollLeft && spot?.top === text?.scrollTop

/** What a look-up is on: until its answer, the character tapped. */
const litRange = ({ range, word }: LookingUp): TextRange =>
  range.start < range.end
    ? range
    : { start: range.start, end: range.start + word.length }

export function BookReader({ work, onLeave }: Props) {
  const gate = useGate()
  const [book, setBook] = useState<Book>()
  const [failed, setFailed] = useState<string>()
  const [place, setPlace] = useState<Place>()
  const [top, setTop] = useState(0)
  const [bars, setBars] = useState(false)
  const [panel, setPanel] = useState<Panel>()
  const docked = panel !== undefined && DOCKED.has(panel)
  const [peek, setPeek] = useState<Peeked>()
  const [paired, setPaired] = useState<Paired>()
  const [lit, setLit] = useState<[number, number]>()
  // The search's matches, and the one last gone to, drawn until a tap and
  // kept after it for `]` and `[` to step on from.
  const [matches, setMatches] = useState<Match[]>([])
  const [found, setFound] = useState<{ match: Match; drawn: boolean }>()
  const foundDrawn = found?.drawn ? found.match : undefined
  const [figure, setFigure] = useState<{ src: string; alt: string }>()
  const [footnote, setFootnote] = useState<{
    edition: string
    block: number
    near: { left: number; right: number; top: number; bottom: number }
  }>()
  // Where the reader was before an in-book link took them on, for Back.
  const [back, setBack] = useState<Place>()
  const [prefs, setPrefs] = usePrefsMap(registry.all(), loadPrefs, savePrefs)
  const text = useRef<HTMLDivElement>(null)
  const anchor = useRef<Anchor>({ block: 0, at: 0 })
  // Where the last placing left the text scrolled. The scroll it sends is not
  // the reader's: an anchor taken afresh there is the start of its line as
  // the text is laid out now, so a panel opened and closed beside text it
  // sets in would take the reader back a line each time.
  const placedAt = useRef<ScrollSpot>(undefined)
  // Where the sight check last left the text, whose scroll is not the reader's
  // either, and whether the reader has gone on since the look-up was asked: an
  // answer landing after that must not take them back to the word.
  const sightLeft = useRef<ScrollSpot>(undefined)
  const movedOn = useRef(false)
  const pressed = useRef<{ x: number; y: number; at: number }>(undefined)
  // Where the last press anywhere on the reader was, which `pressed` is only
  // for the text: the second tap of a double tap may land on anything.
  const pressedOnReader = useRef<{ x: number; y: number }>(undefined)
  const lastTap = useRef<{
    at: number
    toggledBars: boolean
    /**
     * What a tap that looked up was on, for the second of a double tap, and
     * where it landed, which that second must be near if it is off the text.
     */
    lookedUp?: Tapped & { x: number; y: number }
  }>(undefined)
  const tapWait = useRef<number>(undefined)
  useEffect(() => () => window.clearTimeout(tapWait.current), [])
  // A panel a tap opened can open under the finger. For as long as the next
  // tap could be that tap's second it lets taps through to the text, or the
  // second would press whatever the panel drew there, Save word among them.
  const [tapsPass, setTapsPass] = useState(false)
  const tapsPassWait = useRef<number>(undefined)
  useEffect(() => () => window.clearTimeout(tapsPassWait.current), [])
  const notes = useBookNotes(work)
  const [openNote, setOpenNote] = useState<string>()
  const [selected, setSelected] = useState<Selected>()
  const [pinning, setPinning] = useState<{ left?: number }>()
  const [notice, setNotice] = useState<string>()
  const [lookingUp, setLookingUp] = useState<LookingUp>()
  const [looked, setLooked] = useState<Looked>({ kind: 'looking' })
  // Each look-up a tap or a selection starts is numbered, so a longest word
  // answered after a later look-up, or after the panel closed, is dropped.
  const asking = useRef(0)
  // The selection a tap set over the word it looked up, let go of when the
  // look-up closes only if the reader has not selected something else since.
  const light = useRef<Range>(undefined)
  useEffect(() => {
    if (panel === 'lookup') return
    asking.current += 1
    letGo(light.current)
    light.current = undefined
  }, [panel])
  // After the panel has taken focus, which may let go of a selection. Only
  // while the edition asked in is drawn: its offsets mean other words in another.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a look-up lights its word as it starts or is answered; a swap alone must not light again the word of one since closed.
  useEffect(() => {
    if (!lookingUp?.tap) return
    const { block, range } = lookingUp
    letGo(light.current)
    light.current =
      range.start < range.end && lookingUp.edition === place?.edition
        ? selectRange(text.current, block, range)
        : undefined
  }, [lookingUp])
  useEffect(() => {
    if (!lookingUp || lookingUp.tap === 'longest') return
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
  // Stored values are not checked on reading, and the button is drawn pressed
  // only for `true`: any other value would look up with the button off.
  const lookupOn = sliceFor(prefs, lookupModule).on === true
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
  const editionKey = place?.edition ?? ''
  const { furigana } = typeset
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
            .map((b) => [excerpt(b, [0, b.text.length])])
        : [
            excerptsIn(
              other,
              sentenceSpan(book, place.edition, to, peek.sentence),
            ),
          ].filter((group) => group.length > 0)
    return {
      block: peek.block,
      work,
      edition: to,
      furigana,
      label: other.label,
      language: other.language,
      texts,
    }
  }, [book, place, peek, work, furigana])
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
  const japanese = useMemo(() => edition && isJapanese(edition), [edition])
  // Compared or interlinear, editions sit side by side or line by line, which
  // a column of vertical text beside a horizontal one cannot.
  const vertical =
    typeset.vertical && japanese === true && !comparing && !interlinear
  const [revealed, setRevealed] = useState<ReadonlySet<number>>(new Set())
  // Each sentence's translation, worked out once an edition rather than a
  // sentence at a time, since an interlinear book sets one under every one.
  const under = useMemo(
    () =>
      interlinear && book && shownEdition && other
        ? {
            map: sentenceMap(book, shownEdition, other),
            texts: sentenceExcerpts(book.editions[other]),
            edition: other,
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
    if (!edition) return undefined
    const figures = figuresBefore(edition.figures)
    const drawn = edition.blocks.map((block, i) => {
      const Block = block.heading ? 'h2' : 'p'
      const here = shownNotes?.get(i) ?? []
      const highlights = here
        .filter(({ note, own }) => own && note.text)
        .map(({ note }) => note)
      return (
        <Fragment key={i}>
          <Figures figures={figures.get(i)} work={work} edition={editionKey} />
          <Block
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
                work={work}
                edition={editionKey}
                furigana={furigana}
                lit={litSentence}
                found={
                  foundDrawn?.edition === editionKey && foundDrawn.block === i
                    ? [foundDrawn.start, foundDrawn.end]
                    : undefined
                }
                upright={vertical}
                asideAfter={(n) => (
                  <>
                    {peeked?.block === i && peek?.sentence === n && (
                      <Peek {...peeked} />
                    )}
                    {under && (
                      <Interlinear
                        n={n}
                        under={under}
                        work={work}
                        furigana={furigana}
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
        </Fragment>
      )
    })
    return (
      <>
        {drawn}
        <Figures
          figures={figures.get(edition.blocks.length)}
          work={work}
          edition={editionKey}
        />
      </>
    )
  }, [
    edition,
    editionKey,
    work,
    furigana,
    firsts,
    lit,
    peek,
    peeked,
    shownNotes,
    under,
    hideTranslation,
    revealed,
    speaking,
    foundDrawn,
    vertical,
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
    const figuresOf = Object.fromEntries(
      columnKeys.map((key) => [key, figuresBefore(book.editions[key].figures)]),
    )
    const cell = (
      key: string,
      column: number,
      [from, to]: [number, number],
    ) => {
      const blocks = book.editions[key].blocks
      const first = firstSentences(blocks)
      const figures = figuresOf[key]
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
              <Fragment key={i}>
                <Figures
                  figures={figures.get(from + i)}
                  work={work}
                  edition={key}
                />
                <Block
                  data-pick={`${side}:${from + i}`}
                  data-line={block.line ?? undefined}
                >
                  <Sentences
                    block={block}
                    first={first[from + i]}
                    highlights={[]}
                    work={work}
                    edition={key}
                    furigana={furigana}
                    lit={litIn(key)}
                    found={
                      foundDrawn?.edition === key &&
                      foundDrawn.block === from + i
                        ? [foundDrawn.start, foundDrawn.end]
                        : undefined
                    }
                  />
                </Block>
              </Fragment>
            )
          })}
          {/* Drawn by the row holding the last block, not an empty one after. */}
          {to === blocks.length && from < to && (
            <Figures figures={figures.get(to)} work={work} edition={key} />
          )}
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
  }, [book, place, comparing, columnKeys, paired, work, furigana, foundDrawn])

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
  // place, and the place is the anchor until the reader scrolls: scrolled, by
  // any amount; paged, off its page (`onScroll`, which tells the placing's own
  // scroll by `placedAt`). Scrolling by hand sets no `place`, so it never
  // fights the reader.
  useLayoutEffect(() => {
    if (!place) return
    // The anchor put back is the reader staying; any other place is a move.
    if (
      place.block !== anchor.current.block ||
      (place.at ?? 0) !== anchor.current.at
    ) {
      movedOn.current = true
    }
    show(text.current, place.block, place.at, place)
    anchor.current = {
      block: place.block,
      at: place.at ?? 0,
      figure: place.figure,
      into: place.into,
    }
    placedAt.current = text.current
      ? { left: text.current.scrollLeft, top: text.current.scrollTop }
      : undefined
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

  // Vertical and scrolled, the text runs on sideways, where a wheel turned
  // down would not take it, so turned down it scrolls on, leftward. A
  // sideways wheel or swipe already scrolls it, and a pinch is a zoom.
  const linePx = typeset.textSize * typeset.lineSpacing
  useEffect(() => {
    const host = text.current
    if (!host || !vertical || pagedPref || !loaded) return
    const onWheel = (event: WheelEvent) => {
      if (event.ctrlKey) return
      if (Math.abs(event.deltaX) >= Math.abs(event.deltaY)) return
      event.preventDefault()
      const unit =
        event.deltaMode === WheelEvent.DOM_DELTA_LINE
          ? linePx
          : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
            ? host.clientWidth
            : 1
      addProgress(host, event.deltaY * unit)
    }
    host.addEventListener('wheel', onWheel, { passive: false })
    return () => host.removeEventListener('wheel', onWheel)
  }, [vertical, pagedPref, loaded, linePx])

  // An image drawn once the text is laid out pushes what follows it on, a
  // page or more where the text is paged, so the reader goes back to the
  // anchor taken before it loaded (`movesAnchor` says when). Not `kept`:
  // measured now, the anchor may already be off the page the image moved it
  // from. Caught on the way down, since an image's `load` does not bubble.
  useEffect(() => {
    const host = text.current
    if (!host || !loaded) return
    const keep = (event: Event) => {
      const image = event.target
      if (!(image instanceof HTMLImageElement)) return
      if (!movesAnchor(host, anchor.current, image)) return
      setPlace((p) => p && { edition: p.edition, ...anchor.current })
    }
    host.addEventListener('load', keep, true)
    return () => host.removeEventListener('load', keep, true)
  }, [loaded])

  // Vertical turned on or off, a layout or paging switched, or a docked panel
  // opened or closed where `book.css` sets the text in beside one lays the
  // text out afresh, so it goes back to the anchor taken before. Not where a
  // new place came with it, as an edition swap's and a setting's kept anchor
  // do: that is shown already. A panel's set-in redraws the same boxes at
  // another width, so of these it alone keeps the anchor's `into`; the rest are
  // placed `whole`. In the phone layout a panel goes over the text, which is
  // not laid out again, so nothing is placed again either.
  const setIn = docked && sidebarLayout()
  const placedBefore = useRef<Place>(undefined)
  const setInBefore = useRef(setIn)
  // biome-ignore lint/correctness/useExhaustiveDependencies: the layout's own keys are triggers, not reads: the text they set out is what is laid out again.
  useLayoutEffect(() => {
    const placed = placedBefore.current !== place
    const redrawn = setInBefore.current !== setIn
    placedBefore.current = place
    setInBefore.current = setIn
    if (placed) return
    const held = redrawn ? anchor.current : whole(anchor.current)
    setPlace((p) => p && { edition: p.edition, ...held })
  }, [place, vertical, mode, pagedPref, setIn])

  // At phone size the look-up's panel goes over the text, on the side of the
  // screen away from its word. Before the sight check below, which measures
  // the panel, and after the place is shown, which moves the word. The side
  // comes from the word and never from the answer, so a panel does not jump
  // as its answer arrives; only a new word or a new window size chooses again.
  // Bars shown or hidden move the edge it sits against, and leave its side.
  const [resizes, setResizes] = useState(0)
  const seated = useRef<{
    asked: LookingUp
    resizes: number
    side?: LookupSide
  }>(undefined)
  // biome-ignore lint/correctness/useExhaustiveDependencies: the bars and the pinning bar are triggers, what the panel must stay clear of; a new place is not, as a reader scrolling on from the word does not move its panel.
  useLayoutEffect(() => {
    const seat = lookUpPanelOf(text.current)
    if (panel !== 'lookup' || !lookingUp || !seat) {
      seated.current = undefined
      return
    }
    const was = seated.current
    const same =
      was && (was.asked === lookingUp || answers(was.asked, lookingUp))
    let side: LookupSide | undefined
    if (sidebarLayout()) {
      unseatLookup(seat)
    } else {
      const word =
        lookingUp.offScreen || lookingUp.edition !== place?.edition
          ? undefined
          : rangeIn(text.current, lookingUp.block, litRange(lookingUp))
      side = seatLookup(
        seat,
        word,
        same && was.resizes === resizes ? was.side : undefined,
        was?.asked !== lookingUp,
      )
      // A window turned with the word scrolled away has no word to choose by.
      if (!side && same && was.side) {
        side = seatLookup(seat, undefined, was.side)
      }
    }
    seated.current = { asked: lookingUp, resizes, side }
  }, [panel, lookingUp, resizes, bars, pinning])

  // The word a look-up is on stays in sight, the one thing that may move the
  // place when a docked panel opens: the text set in beside the panel can
  // reflow it off the screen. Measured for each new word and each new answer,
  // which can lengthen the word or the panel and which comes after the place
  // is back. Only while the reader is where they asked it: a Japanese answer
  // replaces `lookingUp` and not the number in `asking`, so it is the same
  // look-up, and the position catches a move whose scroll is yet to be sent.
  const sightChecked = useRef<{
    asking: number
    word: LookingUp
    answer: Looked
  }>(undefined)
  useLayoutEffect(() => {
    const host = text.current
    if (panel !== 'lookup' || !lookingUp || !host) return
    const seen = sightChecked.current
    if (seen?.word === lookingUp && seen.answer === looked) return
    sightChecked.current = {
      asking: asking.current,
      word: lookingUp,
      answer: looked,
    }
    if (seen?.asking !== asking.current) movedOn.current = false
    else if (
      !scrolledTo(host, placedAt.current) &&
      !scrolledTo(host, sightLeft.current)
    ) {
      movedOn.current = true
    }
    if (movedOn.current) return
    const { block, range } = lookingUp
    const word =
      !lookingUp.offScreen &&
      lookingUp.edition === place?.edition &&
      range.start < range.end
        ? rangeIn(host, block, range)
        : undefined
    if (word) {
      const over = host
        .closest('.book-reader')
        ?.querySelectorAll(
          `${LOOK_UP_PANEL}, .reader-top-bar, .reader-bottom-bar`,
        )
      const [left, top] = [host.scrollLeft, host.scrollTop]
      bringIntoSight(
        host,
        word,
        [...(over ?? [])].map((cover) => cover.getBoundingClientRect()),
      )
      // Brought in, the word is nearer the panel than the panel was sized for.
      const side = seated.current?.side
      const seat = lookUpPanelOf(host)
      if (
        side &&
        seat &&
        (host.scrollLeft !== left || host.scrollTop !== top)
      ) {
        seatLookup(seat, word, side)
      }
    }
    sightLeft.current = { left: host.scrollLeft, top: host.scrollTop }
  }, [panel, lookingUp, looked, place])

  // A hold that closes the look-up lets the text out again, and putting the
  // place back moves the words under the selection it made, so the bar goes
  // with them, unless the new place has left the selection behind. Only with
  // the mode on: with it off, a selection is treated as it always was.
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new place is the trigger, not a read: it is what has just moved the words.
  useLayoutEffect(() => {
    if (!lookupOn) return
    setSelected((s) => {
      const now = s && selectionIn(text.current)
      return now && !leftScreen(text.current, now) ? now : undefined
    })
  }, [place])

  // A resized window lays the text out again, so it goes back to the anchor
  // the last scroll left, which a paged layout would otherwise slide off. Not
  // measured now: the text has already been laid out at the new width.
  useEffect(() => {
    const keep = () => {
      setPlace((p) => p && { edition: p.edition, ...whole(anchor.current) })
      setResizes((n) => n + 1)
    }
    window.addEventListener('resize', keep)
    return () => window.removeEventListener('resize', keep)
  }, [])

  // To edition `to`, at the counterpart of the block at the top, lit, or at
  // `landing`, a place already in `to`, which a search match is.
  const swapTo = useCallback(
    (to: string, landing?: { block: number; at: number }) => {
      if (!book || !place || to === place.edition) return
      const from = topBlock(text.current)
      const block = landing?.block ?? counterpart(book, place.edition, to, from)
      setPlace({ edition: to, block, at: landing?.at })
      if (!landing) setLit(span(book, place.edition, to, from))
      setPeek(undefined)
      setPaired(undefined)
      stopReading.current?.()
      // Each edition is read at its own pace in characters: a translation
      // runs shorter or longer than its original.
      session.current.paced = { since: Date.now(), read: 0 }
      save({ edition: to, block })
    },
    [book, place, save],
  )
  // A match gone to, by the list, Enter, `]` or `[`: the text placed at its
  // first character, in its own edition, and lit.
  const goToMatch = useCallback(
    (m: Match) => {
      if (!place) return
      setFound({ match: m, drawn: true })
      if (m.edition !== place.edition) {
        swapTo(m.edition, { block: m.block, at: m.start })
        return
      }
      setPlace({ edition: m.edition, block: m.block, at: m.start })
      save({ edition: m.edition, block: m.block })
    },
    [place, swapTo, save],
  )
  // Where the reader is, for a step once the match last gone to is no longer
  // listed: the anchor, which the last scroll or placing left. A figure after
  // its block, as one closing the book is, is past all of that block's words.
  const here = useCallback(
    (): Here | undefined =>
      book && place
        ? {
            edition: place.edition,
            block: anchor.current.block,
            at:
              (anchor.current.figure ?? 0) > 0
                ? Number.POSITIVE_INFINITY
                : anchor.current.at,
            order: editionOrder(book),
          }
        : undefined,
    [book, place],
  )

  useEffect(() => {
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
      if (!book || event.metaKey) return
      // AltGr (Ctrl+Alt on Windows) is how some layouts type [ and ] at all.
      if (
        (event.ctrlKey || event.altKey) &&
        !event.getModifierState('AltGraph')
      ) {
        return
      }
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
      // A figure open full size is over everything, the text's keys too.
      if (figure) {
        if (event.key === 'Escape') setFigure(undefined)
        return
      }
      // So is a footnote, which one Escape closes and nothing else.
      if (footnote) {
        if (event.key === 'Escape') setFootnote(undefined)
        return
      }
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
        act(vertical ? readLeftward(event.key, intent) : intent)
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
      // Not typed into the box it opens, nor taken by Firefox's own find.
      if (own?.intent === SEARCH_TOGGLE) event.preventDefault()
      if (own?.intent === SEARCH_NEXT || own?.intent === SEARCH_PREVIOUS) {
        const next = stepFrom(
          matches,
          found?.match,
          own.intent === SEARCH_NEXT ? 1 : -1,
          here(),
        )
        if (next) goToMatch(next)
      }
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
      if (own?.intent === LOOKUP_TOGGLE) setPrefs(lookupToggled)
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
    figure,
    footnote,
    matches,
    found,
    swapTo,
    goToMatch,
    here,
    vertical,
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
  // Block `block` of edition `to`, gone to by its counterpart in the edition
  // read where the link was drawn in a peek or another column, and where the
  // reader was kept for Back, the bars shown and any panel closed so Back is
  // seen.
  const jump = (to: string, block: number) => {
    setBack({ edition: place.edition, ...topOf(text.current) })
    setBars(true)
    setPanel(undefined)
    go(counterpart(book, to, place.edition, block))
  }
  // A footnote's reference opens the note beside it; any other in-book link
  // goes to its block.
  const follow = (link: HTMLElement) => {
    const to = link.dataset.linkEdition
    const block = Number(link.dataset.linkBlock)
    if (!to || !book.editions[to]?.blocks[block]) return
    if (link.dataset.linkNote === undefined) {
      jump(to, block)
      return
    }
    const { left, right, top, bottom } = link.getBoundingClientRect()
    setFootnote({ edition: to, block, near: { left, right, top, bottom } })
  }
  // A tap on a word, with the look-up mode on, looks it up there and then:
  // Japanese by the longest dictionary word from the character tapped, which
  // only the server's answer can select, and any other language by the run of
  // letters tapped. Whether it looked anything up, so a tap off a word, on a
  // note's mark or on an aside keeps its usual meaning.
  const lookUpTapped = (target: Element, x: number, y: number): boolean => {
    if (target.closest('.book-note-mark')) return false
    if (target.closest('[data-aside]:not(rt)')) return false
    const point = text.current && offsetAtPoint(text.current, x, y)
    const block = point && edition.blocks[point.block]
    if (!point || !block) return false
    if (japanese) {
      if (!beginsWord(block.text, point.at)) return false
      const lang = JAPANESE_DICTIONARY
      const seq = ++asking.current
      setLookingUp({
        edition: place.edition,
        block: point.block,
        range: { start: point.at, end: point.at },
        word: headAt(block.text, point.at, 1),
        lang,
        tap: 'longest',
      })
      setLooked({ kind: 'looking' })
      setPanel('lookup')
      lookUpAt(lang, headAt(block.text, point.at))
        .then((answer) => {
          if (seq !== asking.current) return
          if (!answer) {
            setLooked({ kind: 'none', lang })
            return
          }
          if (answer.length > 0) {
            const range = { start: point.at, end: point.at + answer.length }
            setLookingUp({
              edition: place.edition,
              block: point.block,
              range,
              word: block.text.slice(range.start, range.end),
              lang,
              tap: 'longest',
            })
          }
          setLooked({ kind: 'found', entries: answer.entries })
        })
        .catch((why: unknown) => {
          if (seq === asking.current) {
            setLooked({ kind: 'failed', why: String(why) })
          }
        })
      return true
    }
    const word = wordAt(block.text, point.at)
    if (!word) return false
    asking.current += 1
    const range = { start: word[0], end: word[1] }
    setLookingUp({
      edition: place.edition,
      block: point.block,
      range,
      word: block.text.slice(range.start, range.end),
      lang: edition.language,
      tap: 'word',
    })
    setLooked({ kind: 'looking' })
    setPanel('lookup')
    return true
  }
  // A double tap's peek at what it was on: compared, the sentence set beside
  // it in its column, and otherwise the next edition's under its block.
  const peekAt = ({ block, sentence, column }: Tapped) => {
    if (comparing) {
      if (column && sentence !== undefined) {
        setPaired((p) =>
          p?.edition === column && p.sentence === sentence
            ? undefined
            : { edition: column, sentence },
        )
      }
      return
    }
    if (block !== undefined) {
      setPeek((p) =>
        p?.block === block && p.sentence === sentence
          ? undefined
          : { block, sentence },
      )
    }
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
    <main
      className="reader book-reader"
      style={rootStyle}
      data-vertical={vertical || undefined}
      data-panel={docked ? panel : undefined}
      onPointerDownCapture={(event) => {
        pressedOnReader.current = { x: event.clientX, y: event.clientY }
      }}
      onPointerUpCapture={(event) => {
        const last = lastTap.current
        const down = pressedOnReader.current
        if (
          !last?.lookedUp ||
          !down ||
          Math.abs(event.clientX - down.x) > TAP_SLOP ||
          Math.abs(event.clientY - down.y) > TAP_SLOP
        ) {
          return
        }
        const onText = text.current?.contains(event.target as Node)
        // The first tap's panel may have moved the text from under the finger
        // or opened there, so on the text, or anything near the first, this
        // tap undoes the look-up and peeks at what the first was on. A control
        // away from the first is the reader's next move, and left to act.
        const second =
          onText ||
          (Math.abs(event.clientX - last.lookedUp.x) <= DOUBLE_TAP_REACH &&
            Math.abs(event.clientY - last.lookedUp.y) <= DOUBLE_TAP_REACH)
        if (second && event.timeStamp - last.at < DOUBLE_TAP_MS) {
          event.stopPropagation()
          lastTap.current = undefined
          takeTapClick(event, { mouseToo: true })
          setPanel(undefined)
          window.getSelection()?.removeAllRanges()
          peekAt(last.lookedUp)
          return
        }
        // Too slow for that, yet let through the panel while it might have
        // been: the words under the panel are not what the reader can see.
        // Only words: a bar drawn over the panel still takes its own press.
        const over = event.currentTarget.querySelector(LOOK_UP_PANEL)
        const box = over?.getBoundingClientRect()
        if (
          box &&
          onText &&
          event.clientX >= box.left &&
          event.clientX <= box.right &&
          event.clientY >= box.top &&
          event.clientY <= box.bottom
        ) {
          event.stopPropagation()
          takeTapClick(event, { mouseToo: true })
        }
      }}
    >
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
      {/* biome-ignore lint/a11y/noStaticElementInteractions: the keys are its in-book links', each a focusable `role="link"`, caught here once rather than on every link drawn. */}
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
        data-vertical={vertical || undefined}
        onScroll={() => {
          const host = text.current
          const placing = scrolledTo(host, placedAt.current)
          if (!placing) placedAt.current = undefined
          if (!placing && !scrolledTo(host, sightLeft.current)) {
            movedOn.current = true
          }
          const now = topBlock(text.current)
          const read = at[now] - at[top]
          if (read > 0 && read <= MOST_READ_AT_ONCE) {
            session.current.paced.read += read
          }
          // With the mode on the placing's own scroll leaves the selection
          // on the page, and the bar is set again beside it above.
          if (!lookupOn || !placing) setSelected(undefined)
          // Paged, the anchor is held while it is on the page, or sizing the
          // text up and down creeps back a page at a time. Scrolled, a screen
          // has no such edge: the line at the top is the reader's.
          if (!placing && (!pagedPref || !shows(host, anchor.current))) {
            anchor.current = anchorOf(host)
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
            Math.abs(event.clientX - down.x) > TAP_SLOP ||
            Math.abs(event.clientY - down.y) > TAP_SLOP ||
            event.timeStamp - down.at > TAP_HOLD
          ) {
            if (notes.ready) setSelected(selectionIn(text.current))
            if (lookupOn && panel === 'lookup') {
              // The selection may be the light still, now the bar's to keep.
              light.current = undefined
              setPanel(undefined)
            }
            return
          }
          setFound((f) => (f?.drawn ? { match: f.match, drawn: false } : f))
          const target = event.target as Element
          // An image set in a link is the link's, below.
          const tapped = target.closest('[data-link]')
            ? undefined
            : (target.closest<HTMLImageElement>('img.book-inline-image') ??
              target
                .closest('[data-figure]')
                ?.querySelector<HTMLImageElement>('img'))
          if (tapped) {
            // Not the first of a double tap, which would peek on the next,
            // and an earlier tap's wait stopped, which would hide the bars
            // or turn the page under the image.
            lastTap.current = undefined
            window.clearTimeout(tapWait.current)
            tapWait.current = undefined
            setFigure({
              src: tapped.getAttribute('src') ?? '',
              alt: tapped.alt,
            })
            takeTapClick(event)
            return
          }
          const picked = target
            .closest<HTMLElement>('[data-pick]')
            ?.dataset.pick?.split(':')
          if (pinning && picked) {
            void pick(picked[0] as 'left' | 'right', Number(picked[1]))
            takeTapClick(event)
            return
          }
          const link = target.closest<HTMLElement>('[data-link]')
          if (link) {
            // At once, and not the first of a double tap, so a second tap on
            // it neither peeks nor stops the first. A web link is the
            // browser's, which opens it in a new tab. An earlier tap's wait
            // is stopped, which would hide Back or turn the page after a jump.
            lastTap.current = undefined
            window.clearTimeout(tapWait.current)
            tapWait.current = undefined
            if (link.dataset.linkUrl === undefined) {
              follow(link)
              takeTapClick(event)
            }
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
          const double = last && event.timeStamp - last.at < DOUBLE_TAP_MS
          // A look-up acts at once, so it is the first of a double tap as a
          // link is not: the reader takes the second before it comes here.
          // In pages the outer thirds turn them, so only the middle looks up.
          if (
            !double &&
            lookupOn &&
            pageTurnedBy(text.current, event.clientX) === 0 &&
            lookUpTapped(target, event.clientX, event.clientY)
          ) {
            lastTap.current = {
              at: event.timeStamp,
              toggledBars: false,
              lookedUp: {
                ...tappedOn(target),
                x: event.clientX,
                y: event.clientY,
              },
            }
            window.clearTimeout(tapWait.current)
            tapWait.current = undefined
            setTapsPass(true)
            window.clearTimeout(tapsPassWait.current)
            tapsPassWait.current = window.setTimeout(
              () => setTapsPass(false),
              DOUBLE_TAP_MS,
            )
            // The tap's mousedown, held back below, is what would clear the
            // reader's selection, and an answer naming no word lights nothing
            // to replace it.
            window.getSelection()?.removeAllRanges()
            // The panel has just taken the focus, which the tap's mousedown
            // would otherwise hand to the text under the finger.
            takeTapClick(event, { keepFocus: true })
            return
          }
          if (last && double) {
            lastTap.current = undefined
            takeTapClick(event)
            if (tapWait.current !== undefined) {
              window.clearTimeout(tapWait.current)
              tapWait.current = undefined
            } else if (last.toggledBars) {
              setBars((b) => !b)
            }
            window.getSelection()?.removeAllRanges()
            peekAt(tappedOn(target))
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
        onKeyDown={(event) => {
          if (event.key !== 'Enter') return
          const link = (event.target as Element).closest<HTMLElement>(
            '[data-link-block]',
          )
          if (!link) return
          event.preventDefault()
          follow(link)
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
            <button
              type="button"
              className="reader-button"
              aria-expanded={panel === 'search'}
              onClick={() => toggle('search')}
            >
              Search
            </button>
            <button
              type="button"
              className="reader-button"
              aria-pressed={lookupOn}
              onClick={() => setPrefs(lookupToggled)}
            >
              Look up words
            </button>
            {back && (
              <button
                type="button"
                className="reader-button"
                onClick={() => {
                  setPlace(back)
                  save(back)
                  setBack(undefined)
                }}
              >
                Back
              </button>
            )}
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
          near={selected.near}
          vertical={vertical}
          onHighlight={(colour) => highlight(colour, false)}
          onNote={() => highlight(DEFAULT_COLOUR, true)}
          onLookUp={() => {
            asking.current += 1
            light.current = undefined
            const block = edition.blocks[selected.block]
            setLookingUp({
              edition: place.edition,
              block: selected.block,
              range: selected.range,
              word: block.text.slice(selected.range.start, selected.range.end),
              lang: edition.language,
              offScreen: leftScreen(text.current, selected),
            })
            setLooked({ kind: 'looking' })
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
          tapsPass={tapsPass}
          onSave={(entry) => {
            const note = {
              ...newNote(
                lookingUp.edition,
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
      <BookSearch
        visible={panel === 'search'}
        book={book}
        edition={place.edition}
        entries={entries}
        found={found?.match}
        here={here}
        onPick={goToMatch}
        onResults={setMatches}
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
          // As `change` does, for every setting the reset puts back at once,
          // where one of them lays the text out.
          if (laysOutAfresh(prefs, sliceReset(prefs, registry, id))) {
            setPlace({
              edition: place.edition,
              ...kept(text.current, anchor.current),
            })
          }
          resetPrefs(id)
          setPrefs((map) => sliceReset(map, registry, id))
        }}
        onClose={() => setPanel(undefined)}
        onAccount={() => setPanel('account')}
      />
      {figure && (
        <FullFigure {...figure} onClose={() => setFigure(undefined)} />
      )}
      {footnote && (
        <BookFootnote
          block={book.editions[footnote.edition].blocks[footnote.block]}
          work={work}
          edition={footnote.edition}
          furigana={furigana}
          lang={book.editions[footnote.edition].language ?? undefined}
          page={typeset.page}
          near={footnote.near}
          onGo={() => {
            setFootnote(undefined)
            jump(footnote.edition, footnote.block)
          }}
          onClose={() => setFootnote(undefined)}
        />
      )}
      <Account
        visible={panel === 'account'}
        onClose={() => setPanel(undefined)}
        onSignedOut={() => gate?.signOut()}
      />
    </main>
  )
}

/** Whether the first and the last character of `selected` are both off the screen. */
function leftScreen(
  text: HTMLElement | null,
  { block, range }: Pick<Selected, 'block' | 'range'>,
): boolean {
  return (
    offScreen(text, { block, at: range.start }) &&
    offScreen(text, { block, at: range.end - 1 })
  )
}

/** What a tap landed on, as a double tap's peek reads it. */
interface Tapped {
  block?: number
  sentence?: number
  column?: string
}

function tappedOn(target: Element): Tapped {
  const block = target.closest<HTMLElement>('[data-block]')
  const said = target.closest<HTMLElement>('[data-sentence]')
  return {
    block: block ? Number(block.dataset.block) : undefined,
    sentence: said ? Number(said.dataset.sentence) : undefined,
    column: target.closest<HTMLElement>('[data-column]')?.dataset.column,
  }
}

/**
 * Selects `range` of block `block`'s own words in `text`, as the light on a
 * word a tap looked up, and answers the range selected.
 */
function selectRange(
  text: HTMLElement | null,
  block: number,
  range: TextRange,
): Range | undefined {
  const dom = rangeIn(text, block, range)
  const selection = window.getSelection()
  if (!dom || !selection) return undefined
  selection.removeAllRanges()
  selection.addRange(dom)
  return dom
}

/** Lets go of the selection `mine` set, unless the reader has selected something else since. */
function letGo(mine: Range | undefined) {
  const selection = window.getSelection()
  if (!mine || !selection?.rangeCount) return
  const now = selection.getRangeAt(0)
  if (
    now.compareBoundaryPoints(Range.START_TO_START, mine) === 0 &&
    now.compareBoundaryPoints(Range.END_TO_END, mine) === 0
  ) {
    selection.removeAllRanges()
  }
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

/** An edition's figures by the block each is set before, looked up once a block. */
function figuresBefore(figures: Figure[] = []): Map<number, Figure[]> {
  const by = new Map<number, Figure[]>()
  for (const figure of figures) {
    const at = by.get(figure.before)
    if (at) at.push(figure)
    else by.set(figure.before, [figure])
  }
  return by
}

/**
 * The images a book sets between its blocks, from `edition` of `work`. A
 * figure has no `data-block`, so a peek, a note and the block at the top pass
 * over it to the blocks either side, and the reader's place, where the screen
 * starts in one, names it from the block after it (`Anchor.figure`). Tapped,
 * it opens full size (`data-figure`, in the text's `onPointerUp`), or follows
 * the link it is set in.
 */
function Figures({
  figures,
  work,
  edition,
}: {
  figures: Figure[] | undefined
  work: string
  edition: string
}) {
  return figures?.map((figure, k) => {
    const image = (
      <img src={bookImageUrl(work, edition, figure.image)} alt={figure.alt} />
    )
    return (
      <figure key={k} className="book-figure" data-figure>
        {figure.link ? (
          <Linked
            link={figure.link}
            edition={edition}
            label={imageLinkLabel(figure.link, figure.alt)}
          >
            {image}
          </Linked>
        ) : (
          image
        )}
      </figure>
    )
  })
}

/**
 * A figure tapped open, as large as the screen lets it be drawn. A modal
 * `<dialog>` outside the text, so no press on it reaches the text's taps;
 * Escape (the reader's key handler, or the browser's close request) or a tap
 * beside the image closes it.
 */
function FullFigure({
  src,
  alt,
  onClose,
}: {
  src: string
  alt: string
  onClose: () => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  useLayoutEffect(() => {
    if (dialog.current?.open === false) dialog.current.showModal()
  }, [])
  const backdrop = useBackdropClose(onClose)
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the click is a tap beside the image; a keyboard closes it with Escape, the dialog's own cancel
    <dialog
      ref={dialog}
      className="book-figure-full"
      aria-label={alt || 'An image from the book'}
      onCancel={(e) => {
        // Closed by its owner no longer drawing it, so the browser never
        // closes a dialog React still holds open.
        e.preventDefault()
        onClose()
      }}
      onPointerDown={backdrop.onPointerDown}
      onClick={backdrop.onClick}
    >
      <img src={src} alt={alt} />
    </dialog>
  )
}

/**
 * A block's words, each sentence its own span numbered through the edition
 * from `first`, so a tap can name it; those in `lit` are lit, and
 * `asideAfter` sets what follows one, a peek or its translation. `found`
 * lights a search match as a run of its own. `upright`, for vertical text,
 * sets each short number across its line. Its marks are drawn as `Runs`
 * draws them, its images from `edition` of `work`.
 */
export function Sentences({
  block,
  first,
  highlights,
  work,
  edition,
  furigana,
  lit,
  found,
  upright,
  asideAfter,
}: {
  block: Block
  first: number
  highlights: Note[]
  work: string
  edition: string
  furigana: boolean
  lit?: [number, number]
  found?: [number, number]
  upright?: boolean
  asideAfter?: (n: number) => ReactNode
}) {
  const drawn = { work, edition, furigana }
  return pieces(
    block.text,
    block.sentences,
    highlights,
    block.marks,
    found,
    upright,
  ).map((piece, j) => {
    if (piece.sentence === undefined) {
      return <Runs key={j} runs={piece.runs} {...drawn} />
    }
    const n = first + piece.sentence
    return (
      <Fragment key={j}>
        <span
          data-sentence={n}
          data-lit={(lit && n >= lit[0] && n < lit[1]) || undefined}
        >
          <Runs runs={piece.runs} {...drawn} />
        </span>
        {asideAfter?.(n)}
      </Fragment>
    )
  })
}

/**
 * The next edition's sentences for sentence `n`, set after the last of the
 * sentences that share them, so a translation that joins two is shown once;
 * `hidden` until tapped, to test oneself.
 */
function Interlinear({
  n,
  under,
  work,
  furigana,
  hidden,
}: {
  n: number
  under: {
    map: [number, number][]
    texts: Excerpt[]
    edition: string
    language: string | null
  }
  work: string
  furigana: boolean
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
      <Excerpts
        texts={under.texts.slice(start, end)}
        work={work}
        edition={under.edition}
        furigana={furigana}
      />
    </span>
  )
}

/** Excerpts of another edition drawn with their marks, a space between each. */
function Excerpts({
  texts,
  work,
  edition,
  furigana,
}: {
  texts: Excerpt[]
  work: string
  edition: string
  furigana: boolean
}) {
  return texts.map((t, i) => (
    <Fragment key={i}>
      {i > 0 && ' '}
      <Runs
        runs={pieces(t.text, [], [], t.marks).flatMap((p) => p.runs)}
        work={work}
        edition={edition}
        furigana={furigana}
      />
    </Fragment>
  ))
}

/** What the next edition has for a block, set under it. */
function Peek({
  label,
  language,
  texts,
  work,
  edition,
  furigana,
}: {
  label: string
  language: string | null
  texts: Excerpt[][]
  work: string
  edition: string
  furigana: boolean
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
        : texts.map((t, i) => (
            <span key={i}>
              <Excerpts
                texts={t}
                work={work}
                edition={edition}
                furigana={furigana}
              />
            </span>
          ))}
    </span>
  )
}
