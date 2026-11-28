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
  memo,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react'
import { Account } from './Account'
import {
  getProgress,
  lookUpAt,
  lookUpWord,
  OPENING_SAVE_WAIT_MS,
  progressSavedWithin,
  putProgress,
  Unauthenticated,
} from './api'
import { type Destination, destination } from './back'
import { useBackdropClose } from './backdropClose'
import { bottomBarStyle, readoutStyle, readoutTextStyle } from './bars'
import {
  BOOK_KEYS,
  type Block,
  type Book,
  blockAt,
  bookImageUrl,
  CARDS_WAIT_MS,
  type CharacterCard,
  charactersText,
  contents,
  imageLinkLabel,
  counterpart,
  editionOrder,
  entryAt,
  type Excerpt,
  type Figure,
  isPageImage,
  excerpt,
  excerptsIn,
  firstSentences,
  getBook,
  getCharacters,
  charactersBefore,
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
  keepsRead,
  kept,
  matchShows,
  movesAnchor,
  offScreen,
  onPage,
  onPageLink,
  onPicture,
  type Pages,
  pagesOf,
  pageTurnedBy,
  readingAt,
  readPlaced,
  resumeBlock,
  show,
  shows,
  topBlock,
  topChild,
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
  type Shown,
} from './bookNotes'
import {
  blocksNear,
  markLookedUp,
  markMentions,
  mentionPattern,
  mentionRanges,
} from './bookMentions'
import { BookFootnote } from './BookFootnote'
import { BookProgress } from './BookProgress'
import { Linked, Runs } from './BookRuns'
import { BookSearch } from './BookSearch'
import { type Here, isJapanese, type Match, stepFrom } from './bookSearch'
import { Contents } from './Contents'
import { debounce } from './debounce'
import { useKeyboardBars } from './focus'
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
import { BOOK_LAYOUT_WORDS, BOOK_LAYOUTS } from './layouts'
import { work as workDetail, type WorkDetail } from './library'
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
  type BookTextPrefs,
  bookTextModule,
  resized,
  stepped,
  TEXT_SIZE_INTENTS,
} from './modules/bookText'
import {
  contentsModule,
  TOGGLE_INTENT as CONTENTS_TOGGLE,
} from './modules/contents'
import { fitModule } from './modules/fit'
import {
  enabledModules,
  TOGGLE_KEY as HELP_KEY,
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
import { beginsWord, headAt, isKorean, wordAt } from './wordAt'

interface Props {
  work: string
  /** The way out (`back.ts`), and what it is called: the library where none is given. */
  onLeave: () => void
  leaveTo?: Destination
  /** As `App`'s: the work's series from the library's answer, as it lands (O431). */
  onSeries?: (series: WorkDetail['series']) => void
  /** As `App`'s: whether "Back from reading" was "Series page" as the reader opened, for the help's `L` row (O431). */
  putsSeriesUnder?: boolean
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
  /** The character of the column read kept on the page: `Anchor.read`. */
  read?: Anchor['read']
  /** The block the book was left at, as it opens: placed by its own words where `keepsRead` holds. */
  saved?: boolean
  /** Placed again as a docked panel sets the text in beside it or lets it out. */
  setIn?: boolean
  /**
   * Laid out again where it stands, by a text size, a setting, a pin, a window
   * size, an image or a docked panel's set-in, rather than moved to: a match
   * gone to stays held through it, whatever page the new layout shows.
   */
  relaid?: boolean
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

/**
 * `place` sent back to `anchor`, for what moved the text from under it. A place
 * still to be committed stays: the anchor is the place before it until that
 * commit, which sets the new place out as the text is laid out by then. The
 * place put back is `relaid`: the reader has not moved.
 */
function backTo(
  place: Place | undefined,
  committed: Place | undefined,
  anchor: Anchor,
): Place | undefined {
  return place && place === committed
    ? { edition: place.edition, ...anchor, relaid: true }
    : place
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
   * letters tapped, a Korean run with the digits joined to it, or as the
   * longest word from the character tapped, whose answer is `looked`.
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

/**
 * What the look-up panel draws while it holds its first draw for the cards:
 * one object throughout, so that the sight check, run for each answer drawn,
 * runs as the hold ends and not as the dictionary's answer lands under it.
 */
const HOLDING: Looked = { kind: 'looking' }

/** Whether `text` is scrolled to exactly `spot`. */
const scrolledTo = (text: HTMLElement | null, spot?: ScrollSpot) =>
  spot?.left === text?.scrollLeft && spot?.top === text?.scrollTop

/** What a look-up is on: until its answer, the character tapped. */
const litRange = ({ range, word }: LookingUp): TextRange =>
  range.start < range.end
    ? range
    : { start: range.start, end: range.start + word.length }

/**
 * What a tap's look-up lights: its word, and with it the name its cards are
 * for, which runs `name` units from `from` units after the word's start,
 * or before it where `from` is below 0. Never less than the word, which is
 * still what the dictionary's answer is for, and has the particle a name
 * can end before.
 */
const lightRange = ({ range }: LookingUp, name = 0, from = 0): TextRange => {
  if (name <= 0) return range
  const start = Math.max(0, range.start + from)
  return {
    start: Math.min(range.start, start),
    end: Math.max(range.end, start + name),
  }
}

export function BookReader({
  work,
  onLeave,
  leaveTo = destination('/'),
  onSeries,
  putsSeriesUnder = true,
}: Props) {
  const gate = useGate()
  const [book, setBook] = useState<Book>()
  const [failed, setFailed] = useState<string>()
  const [place, setPlace] = useState<Place>()
  const [top, setTop] = useState(0)
  const [bars, setBars] = useState(false)
  const keyboardBars = useKeyboardBars()
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
  // Whether a page image, or a figure opened full size, may be drawn larger
  // than its own size: the comic reader's upscale-to-fit, so a cover reads
  // alike in either reader, and a small figure opens as large as the screen
  // lets it be. Its control is in the comic reader's settings, not the
  // book's, so it cannot change while a book is open: read once, as the comic
  // reader reads it.
  const [upscale] = useState(
    () => loadPrefs(fitModule.id, fitModule.defaults).upscaleToFit,
  )
  const text = useRef<HTMLDivElement>(null)
  const anchor = useRef<Anchor>({ block: 0, at: 0 })
  // The place last committed, which the anchor belongs to: `backTo`.
  const placedBefore = useRef<Place>(undefined)
  // Where the last placing left the text scrolled. The scroll it sends is not
  // the reader's: an anchor taken afresh there is the start of its line as
  // the text is laid out now, so a panel opened and closed beside text it
  // sets in would take the reader back a line each time.
  const placedAt = useRef<ScrollSpot>(undefined)
  // Whether that placing was the one a docked panel's set-in causes, which
  // keeps the reader where they were in a layout of another width.
  const placedSetIn = useRef(false)
  // Where the sight check last left the text, whose scroll is not the reader's
  // either, and whether the reader has gone on since the look-up was asked: an
  // answer landing after that must not take them back to the word.
  const sightLeft = useRef<ScrollSpot>(undefined)
  const movedOn = useRef(false)
  // Where the sight check's scroll is still to be heard, set only when it
  // moved the text: a change of scroll always sends one scroll event, which
  // takes this whatever it finds, so a hand move landing on that spot later is
  // the reader's. Not `sightLeft`, which stays and a reader can scroll back to.
  const sightScroll = useRef<ScrollSpot>(undefined)
  // The match a step placed, until a hand move or a navigation ends the hold,
  // which the text laid out again keeps (`Place.relaid`): what `steppedFrom`
  // goes on from, whether or not its words can be seen.
  const heldMatch = useRef<Match>(undefined)
  // Set by `savePlacing`, for the place effect to save the page it shows.
  const savePlaced = useRef(false)
  // The place the book would open at if left now: the one it opened at, until
  // a save names another. Saving it again would send what is held already.
  const reopensAt = useRef<Place>(undefined)
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
  // The cards of the look-up `asking` numbers, where its edition has
  // character data. The panel holds its first draw until they and the
  // dictionary's outcome are both in, so that nothing drawn moves when the
  // cards come; `waited` once `CARDS_WAIT_MS` has passed since that outcome,
  // which ends the hold with no cards and drops a later answer. Asked for
  // where the look-up starts: the Japanese answer replaces `lookingUp`, so an
  // effect on it would ask twice.
  const [named, setNamed] = useState<{
    cards?: CharacterCard[]
    start?: number
    length?: number
    waited?: true
  }>()
  const holding =
    named !== undefined &&
    (named.cards === undefined || looked.kind === 'looking')
  const lookedDrawn = holding ? HOLDING : looked
  const cardsDrawn = holding ? undefined : named?.cards
  // How much of the text the name on the cards drawn takes, from where the
  // look-up starts: the light on the word runs on over it as the cards show.
  const nameLength = cardsDrawn?.length ? named?.length : undefined
  const nameStart = nameLength === undefined ? undefined : named?.start
  useEffect(() => {
    if (!named || named.cards || looked.kind === 'looking') return
    const wait = window.setTimeout(
      () => setNamed({ cards: [], waited: true }),
      CARDS_WAIT_MS,
    )
    return () => window.clearTimeout(wait)
  }, [named, looked])
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
  // biome-ignore lint/correctness/useExhaustiveDependencies: a look-up lights its word as it starts or is answered, and again as its cards say how far the name runs; a swap alone must not light again the word of one since closed, and neither must the panel closing.
  useEffect(() => {
    // The cards can be drawn after the panel has closed, when the dictionary
    // answers a hold late: that must light nothing.
    if (!lookingUp?.tap || panel !== 'lookup') return
    const range = lightRange(lookingUp, nameLength, nameStart)
    letGo(light.current)
    light.current =
      range.start < range.end && lookingUp.edition === place?.edition
        ? selectRange(text.current, lookingUp.block, range)
        : undefined
  }, [lookingUp, nameLength, nameStart])
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
  const helpOn = sliceFor(prefs, helpModule).enabled
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
  // What hangs on the edition alone is keyed by it rather than by `place`,
  // which changes with every placing: a size step, a resize or a panel set
  // in would otherwise rebuild every block of the book.
  const shownEdition = place?.edition
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
  const figuresAt = useMemo(
    () => edition && figuresBefore(edition.figures),
    [edition],
  )
  const peeked = useMemo(() => {
    if (!book || !shownEdition || !peek) return undefined
    const to = nextEdition(book, shownEdition)
    const other = book.editions[to]
    const texts =
      peek.sentence === undefined
        ? other.blocks
            .slice(...span(book, shownEdition, to, peek.block))
            .map((b) => [excerpt(b, [0, b.text.length])])
        : [
            excerptsIn(
              other,
              sentenceSpan(book, shownEdition, to, peek.sentence),
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
  }, [book, shownEdition, peek, work, furigana])
  const other =
    book && place && editionOrder(book).length > 1
      ? nextEdition(book, place.edition)
      : undefined
  const comparing = mode === 'compare' && other !== undefined
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
    if (!book || !left || !now || left === now) return
    setOthers((kept) => {
      // With no columns chosen, the one shown is the edition after the one
      // read, which a swap to it would leave for the edition after that.
      const chosen = kept.some((k) => k in book.editions && k !== left)
      if (!chosen)
        return comparing && nextEdition(book, left) === now ? [left] : kept
      return kept.map((k) => (k === now ? left : k))
    })
  }, [book, shownEdition, comparing])
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
    // Compared, no peek is drawn, and one kept unseen would take Escape.
    else setPeek(undefined)
  }, [comparing])

  const shownNotes = useMemo(
    () =>
      book && shownEdition
        ? notesOn(book, shownEdition, notes.notes)
        : undefined,
    [book, shownEdition, notes.notes],
  )
  // The page follows the sentence read aloud, turned only once it is off it.
  useEffect(() => {
    if (speaking === undefined || !edition || !firsts || !place) return
    const block = firsts.findLastIndex((first) => first <= speaking)
    const range = edition.blocks[block]?.sentences[speaking - firsts[block]]
    if (!range) return
    const at = { block, at: range[0] }
    if (place.block === at.block && place.at === at.at) return
    // Looked for as a match is, since compared a block's words are drawn in
    // its row's column rather than on their own.
    if (!matchShows(text.current, { block, start: range[0] })) {
      setPlace({ edition: place.edition, ...at })
    }
  }, [speaking, edition, firsts, place])
  useEffect(() => () => stopReading.current?.(), [])

  // The translations tapped open, by the block they are in. Worked out apart
  // from `paragraphs`, which a sentence read aloud or a peek rebuilds, so a
  // block with one open keeps the same set and is not drawn again.
  const revealedIn = useMemo(() => {
    const by = new Map<number, ReadonlySet<number>>()
    if (!firsts || !under || !hideTranslation) return by
    for (const n of revealed) {
      const at = firsts.findLastIndex((first) => first <= n)
      by.set(at, new Set([...(by.get(at) ?? []), n]))
    }
    return by
  }, [firsts, under, hideTranslation, revealed])

  const paragraphs = useMemo(() => {
    const litSentence = speaking ?? peek?.sentence
    if (!edition || !firsts || !figuresAt) return undefined
    // Each block is handed only its own share of what is lit, found, peeked
    // and revealed, so the blocks it leaves alone keep their props and are not
    // drawn again.
    const drawn = edition.blocks.map((block, i) => {
      const first = firsts[i]
      const holds = (n: number | undefined) =>
        n !== undefined && n >= first && n < firsts[i + 1]
      const found =
        foundDrawn?.edition === editionKey && foundDrawn.block === i
          ? foundDrawn
          : undefined
      const peekedHere = peeked?.block === i ? peeked : undefined
      return (
        <BookBlock
          key={i}
          index={i}
          block={block}
          first={first}
          figures={figuresAt.get(i)}
          notes={shownNotes?.get(i) ?? NO_NOTES}
          lit={(lit && i >= lit[0] && i < lit[1]) || false}
          litSentence={holds(litSentence) ? litSentence : undefined}
          foundStart={found?.start}
          foundEnd={found?.end}
          peeked={peekedHere}
          peekSentence={peekedHere && peek?.sentence}
          under={under}
          hideTranslation={hideTranslation}
          revealed={revealedIn.get(i) ?? NONE_REVEALED}
          work={work}
          edition={editionKey}
          furigana={furigana}
          vertical={vertical}
        />
      )
    })
    return (
      <>
        {drawn}
        <Figures
          figures={figuresAt.get(edition.blocks.length)}
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
    figuresAt,
    lit,
    peek,
    peeked,
    shownNotes,
    under,
    hideTranslation,
    revealedIn,
    speaking,
    foundDrawn,
    vertical,
  ])
  const compared = useMemo(() => {
    if (!book || !shownEdition || !comparing) return undefined
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
    // Once a column, not once a cell: a cell holds a row's few blocks, and
    // numbering the whole edition's sentences for each would cost the rows
    // times the blocks.
    const firstsOf = Object.fromEntries(
      columnKeys.map((key) => [key, firstSentences(book.editions[key].blocks)]),
    )
    const cell = (
      key: string,
      column: number,
      [from, to]: [number, number],
    ) => {
      const blocks = book.editions[key].blocks
      const first = firstsOf[key]
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
  }, [
    book,
    shownEdition,
    comparing,
    columnKeys,
    paired,
    work,
    furigana,
    foundDrawn,
  ])

  // The word a tap looked up stays lit for as long as the look-up's panel is
  // open, and only in the edition asked in, whose offsets its range is in.
  // The selection lights it too (`light`), but that is the reader's: a press
  // in the panel takes it there. So the word is marked on its own as well,
  // and again as the paragraphs are drawn again, since a range marked ends
  // with the text node it was in.
  const litWord = useMemo(() => {
    if (panel !== 'lookup' || !lookingUp?.tap) return undefined
    if (lookingUp.edition !== editionKey) return undefined
    const range = lightRange(lookingUp, nameLength, nameStart)
    return range.start < range.end
      ? { block: lookingUp.block, range }
      : undefined
  }, [panel, lookingUp, editionKey, nameLength, nameStart])
  // biome-ignore lint/correctness/useExhaustiveDependencies: the paragraphs drawn again are a trigger and not a read: their text nodes can be new ones.
  useEffect(() => {
    if (!litWord) return
    markLookedUp(rangeIn(text.current, litWord.block, litWord.range))
    return () => markLookedUp(undefined)
  }, [litWord, paragraphs])
  // While the look-up shows a character's card, the other places the text on
  // screen names that character are marked: the first card's, where the
  // cards say who is meant. Not the word lit, and only in the edition asked
  // in, whose names the card's are.
  const mentioned = useMemo(() => {
    if (panel !== 'lookup' || lookingUp?.edition !== editionKey) {
      return undefined
    }
    const names = cardsDrawn && mentionPattern(cardsDrawn)
    return names && { names, lit: litWord }
  }, [panel, cardsDrawn, lookingUp?.edition, editionKey, litWord])
  // Marked again as the reader scrolls or turns the page, since only the
  // blocks near the screen are searched, and as the paragraphs are drawn
  // again, since a range marked ends with the text node it was in.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the paragraphs drawn again are a trigger and not a read: their text nodes can be new ones.
  useEffect(() => {
    const host = text.current
    if (!host || !mentioned || !edition || comparing) return
    let frame = 0
    let marked: string | undefined
    const mark = () => {
      frame = 0
      const near = blocksNear(host)
      // A scroll that brings no new block near has nothing new to mark.
      const span = `${near[0]?.dataset.block}-${near.at(-1)?.dataset.block}`
      if (span === marked) return
      marked = span
      markMentions(
        mentionRanges(near, edition.blocks, mentioned.names, mentioned.lit),
      )
    }
    mark()
    const onScroll = () => {
      frame ||= window.requestAnimationFrame(mark)
    }
    host.addEventListener('scroll', onScroll, { passive: true })
    return () => {
      host.removeEventListener('scroll', onScroll)
      window.cancelAnimationFrame(frame)
      markMentions([])
    }
  }, [mentioned, edition, comparing, paragraphs])

  // The series the top bar links to, from the library's answer for the work,
  // asked once beside the load and never waited on: until it answers, if it
  // fails, and for a work filed in no series, the bar links to none. No
  // stale-answer guard: `Routes.tsx` keys this component by `work`.
  const [series, setSeries] = useState<WorkDetail['series']>(null)
  // The route hears the answer, never this state, whose `null` before the
  // answer would tell it "in no series" (O431). A ref, so a new function
  // from the route does not ask the library again.
  const onSeriesRef = useRef(onSeries)
  onSeriesRef.current = onSeries
  useEffect(() => {
    workDetail(work).then(
      (detail) => {
        setSeries(detail.series)
        onSeriesRef.current?.(detail.series)
      },
      (err: unknown) => {
        if (!(err instanceof Unauthenticated)) console.error(err)
      },
    )
  }, [work])

  useEffect(() => {
    let live = true
    // The place is read after every progress save still out, within the same
    // bound, for the reason `App.tsx`'s `STARTUP_PORTS` gives: a book left
    // and reopened at once flushed its place as it left.
    const stored = progressSavedWithin(OPENING_SAVE_WAIT_MS).then(() =>
      getProgress(work),
    )
    Promise.all([getBook(work), stored.catch(() => undefined)])
      .then(([loaded, saved]) => {
        if (!live) return
        const resumed = saved && saved.edition in loaded.editions
        setBook(loaded)
        // A saved block past the end is one a shorter copy of the book, read
        // again since, no longer has; its last block is the nearest. A book
        // opened afresh starts at its first figure where one is set before
        // its first block, as a cover is: placed by that block, it would open
        // on the block's own page or line, past the cover. A work with no
        // book edition has no hub edition, and says so once it is drawn.
        const leading =
          loaded.editions[loaded.hub]?.figures?.filter(
            (figure) => figure.before === 0,
          ).length ?? 0
        const opening: Place = resumed
          ? {
              edition: saved.edition,
              block: Math.max(
                0,
                Math.min(
                  saved.slot,
                  loaded.editions[saved.edition].blocks.length - 1,
                ),
              ),
              saved: true,
            }
          : {
              edition: loaded.hub,
              block: 0,
              figure: leading > 0 ? -leading : undefined,
            }
        reopensAt.current = opening
        setPlace(opening)
      })
      .catch((why: unknown) => {
        if (live) setFailed(String(why))
      })
    return () => {
      live = false
    }
  }, [work])

  const save = useMemo(() => {
    const put = debounce(
      (at: Place) =>
        void putProgress(work, { edition: at.edition, slot: at.block }).catch(
          () => undefined,
        ),
      SAVE_AFTER_MS,
    )
    return Object.assign(
      (at: Place) => {
        reopensAt.current = at
        put(at)
      },
      { flush: put.flush },
    )
  }, [work])
  useEffect(() => () => save.flush(), [save])
  // What a placing saves as it is asked for, which paged the place effect
  // replaces with the page it comes to show.
  const savePlacing = useCallback(
    (at: Place) => {
      savePlaced.current = true
      save(at)
    },
    [save],
  )

  // A swap, the book's first draw, or the text laid out again scrolls to the
  // place, and the place is the anchor until the reader scrolls: scrolled, by
  // any amount; paged, off its page (`onScroll`, which tells the placing's own
  // scroll by `placedAt`). Scrolling by hand sets no `place`, so it never
  // fights the reader.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the panel is read, not a trigger: one opened or closed beside the text comes here as the place its set-in commits.
  useLayoutEffect(() => {
    if (!place) return
    // The anchor put back is the reader staying; any other place is a move.
    if (
      place.block !== anchor.current.block ||
      (place.at ?? 0) !== anchor.current.at
    ) {
      movedOn.current = true
    }
    const held = heldMatch.current
    const own =
      held?.edition === place.edition &&
      held.block === place.block &&
      held.start === (place.at ?? 0)
    // Laid out again, the place can name another block than the match, as a
    // size step compared does in going back to the start of a row before it;
    // the reader has not moved, so the hold stays.
    if (!own && !place.relaid) heldMatch.current = undefined
    // A row's start can lie pages before the saved block's words, and a panel
    // and its closing go by those words, so the book opens on them too.
    const words = place.saved && keepsRead(text.current)
    const stood = text.current && {
      left: text.current.scrollLeft,
      top: text.current.scrollTop,
    }
    show(
      text.current,
      place.block,
      place.at,
      words ? { read: { block: place.block, at: 0 } } : place,
    )
    const at = place.at ?? 0
    anchor.current = {
      block: place.block,
      at,
      figure: place.figure,
      into: place.into,
      // None at a match, held and named by this place, which a panel keeps in
      // view rather than the page's first character. None taken with `into`
      // either: that page was named by the row, and a character taken here
      // would name another once the panel closes.
      read:
        place.read ??
        (own || place.into !== undefined
          ? undefined
          : readPlaced(text.current, { block: place.block, at })),
    }
    placedAt.current = text.current
      ? { left: text.current.scrollLeft, top: text.current.scrollTop }
      : undefined
    placedSetIn.current = place.setIn === true
    setTop(place.block)
    setPages(pagesOf(text.current, layout?.entries ?? [], place.block))
    // A placing on the page already showing sends no scroll for `onScroll` to
    // save from, and the block it named can be one carried onto this page,
    // which a book opened at it would go pages back for.
    if (savePlaced.current) {
      if (text.current?.dataset.paged !== undefined) {
        save({ edition: place.edition, block: resumeBlock(text.current) })
      }
    } else if (
      stood &&
      scrolledTo(text.current, stood) &&
      !place.saved &&
      !(place.setIn && panel === 'lookup')
    ) {
      // Nor does the text laid out again where it stands, by a size step or
      // a window made shorter, though the page can now begin at another
      // block: the one saved from the old layout would reopen pages on. Not
      // the book's opening, the place already saved, nor the look-up's set-in,
      // which saves nothing for the reason `onScroll` gives.
      const block = resumeBlock(text.current)
      const last = reopensAt.current
      if (last?.edition !== place.edition || last.block !== block) {
        save({ edition: place.edition, block })
      }
    }
    savePlaced.current = false
  }, [place, layout, save])

  // The text's settings as last asked for, ahead of the drawing that shows
  // them, so a size step can tell whether it changes the size at all.
  const typesetAsked = useRef(typeset)
  typesetAsked.current = typeset
  // A size step keeps the place and lays the book out again. One that changes
  // nothing, at the smallest or largest size, does neither: laying out again
  // costs the whole book's layout, and compared and paged the place kept can
  // fall on another page of a row taller than the screen.
  const resizeText = useCallback(
    (to: (prefs: BookTextPrefs) => BookTextPrefs) => {
      const asked = to(typesetAsked.current)
      if (asked.textSize === typesetAsked.current.textSize) return
      typesetAsked.current = asked
      const at = kept(text.current, anchor.current)
      setPlace((p) => p && { edition: p.edition, ...at, relaid: true })
      setPrefs((map) => changeSlice(map, bookTextModule, to))
    },
    [setPrefs],
  )

  // Paged, the wheel sets the text larger or smaller as it zooms a page in the
  // page reader, a step a notch, the notches of a smooth wheel added up. The
  // text is drawn only once the book has loaded, so that is when it listens.
  // A step lays the whole book out again, so after one the notches turned
  // before the next frame wait for it and are stepped together: a fast spin
  // lays the book out once a frame, not once a notch, while a lone notch
  // still steps at once.
  const loaded = layout !== undefined
  useEffect(() => {
    const host = text.current
    if (!host || !pagedPref || !loaded) return
    let notches = 0
    let frame = 0
    const step = () => {
      const steps = Math.trunc(notches)
      if (steps === 0) return false
      notches -= steps
      resizeText((p) => stepped(p, steps))
      return true
    }
    const waitAFrame = () => {
      frame = requestAnimationFrame(() => {
        frame = 0
        if (step()) waitAFrame()
      })
    }
    const onWheel = (event: WheelEvent) => {
      event.preventDefault()
      notches += wheelNotches(event) * (event.deltaY < 0 ? 1 : -1)
      if (frame === 0 && step()) waitAFrame()
    }
    host.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      host.removeEventListener('wheel', onWheel)
      cancelAnimationFrame(frame)
    }
  }, [pagedPref, loaded, resizeText])

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
      setPlace((p) => backTo(p, placedBefore.current, anchor.current))
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
  const setInBefore = useRef(setIn)
  // biome-ignore lint/correctness/useExhaustiveDependencies: the layout's own keys are triggers, not reads: the text they set out is what is laid out again.
  useLayoutEffect(() => {
    const placed = placedBefore.current !== place
    const redrawn = setInBefore.current !== setIn
    placedBefore.current = place
    setInBefore.current = setIn
    if (placed) return
    const held = redrawn ? anchor.current : whole(anchor.current)
    setPlace(
      (p) =>
        p && {
          edition: p.edition,
          ...held,
          setIn: redrawn || undefined,
          relaid: true,
        },
    )
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
  // reflow it off the screen. Measured for each new word and each new answer
  // drawn, which can lengthen the word or the panel and which comes after the
  // place is back. Only while the reader is where they asked it: a Japanese
  // answer replaces `lookingUp` and not the number in `asking`, so it is the
  // same look-up, and the position catches a move whose scroll is yet to be
  // sent.
  const sightChecked = useRef<{
    asking: number
    word: LookingUp
    answer: Looked
  }>(undefined)
  useLayoutEffect(() => {
    const host = text.current
    if (panel !== 'lookup' || !lookingUp || !host) return
    const seen = sightChecked.current
    if (seen?.word === lookingUp && seen.answer === lookedDrawn) return
    sightChecked.current = {
      asking: asking.current,
      word: lookingUp,
      answer: lookedDrawn,
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
      const moved = host.scrollLeft !== left || host.scrollTop !== top
      if (moved) {
        sightScroll.current = { left: host.scrollLeft, top: host.scrollTop }
      }
      // Brought in, the word is nearer the panel than the panel was sized for.
      const side = seated.current?.side
      const seat = lookUpPanelOf(host)
      if (side && seat && moved) seatLookup(seat, word, side)
    }
    sightLeft.current = { left: host.scrollLeft, top: host.scrollTop }
  }, [panel, lookingUp, lookedDrawn, place])

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
      setPlace((p) => backTo(p, placedBefore.current, whole(anchor.current)))
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
      const read = book.editions[place.edition].blocks.length
      // Compared, a last row holding only the other columns' blocks is named
      // past the end of the edition read, which has nothing there to find a
      // counterpart of: the swap lands on, and lights, that row's own blocks.
      const past =
        comparing && from >= read
          ? topRowOf(book, text.current, columnKeys)?.[columnKeys.indexOf(to)]
          : undefined
      const row = past && past[1] > past[0] ? past : undefined
      const near = Math.min(from, read - 1)
      const block =
        landing?.block ?? row?.[0] ?? counterpart(book, place.edition, to, near)
      setPlace({ edition: to, block, at: landing?.at })
      if (!landing) setLit(row ?? span(book, place.edition, to, near))
      setPeek(undefined)
      setPaired(undefined)
      stopReading.current?.()
      // Each edition is read at its own pace in characters: a translation
      // runs shorter or longer than its original.
      session.current.paced = { since: Date.now(), read: 0 }
      savePlacing({ edition: to, block })
    },
    [book, place, comparing, columnKeys, savePlacing],
  )
  // A match gone to, by the list, Enter, `]` or `[`: the text placed at its
  // first character, in its own edition, and lit.
  const goToMatch = useCallback(
    (m: Match) => {
      if (!place) return
      heldMatch.current = m
      setFound({ match: m, drawn: true })
      if (m.edition !== place.edition) {
        swapTo(m.edition, { block: m.block, at: m.start })
        return
      }
      setPlace({ edition: m.edition, block: m.block, at: m.start })
      savePlacing({ edition: m.edition, block: m.block })
    },
    [place, swapTo, savePlacing],
  )
  // Where the reader is, for a step with no match to go on from (`steppedFrom`)
  // or one no longer listed: what the last scroll or placing left in the anchor,
  // as `readingAt` reads it. A figure after its block, as one closing the book
  // is, is past all of that block's words.
  const here = useCallback((): Here | undefined => {
    if (!book || !place) return undefined
    const { block, at } = readingAt(text.current, anchor.current)
    return {
      edition: place.edition,
      block,
      at: (anchor.current.figure ?? 0) > 0 ? Number.POSITIVE_INFINITY : at,
      order: editionOrder(book),
    }
  }, [book, place])
  // The match a step goes on from: the one last gone to, in the edition read,
  // while it is held or its first character is on the screen. Going to a match
  // holds it, and the text laid out again keeps the hold whatever page it then
  // shows (`Place.relaid`): a figure can push its words off the page, a layout
  // may give no box to measure, and a step from behind it would land on it
  // again and never move on. A hand move or a navigation ends the hold; once
  // the match is off the screen too, a step goes from the reader's place.
  const steppedFrom = useCallback((): Match | undefined => {
    const match = found?.match
    if (!match || match.edition !== place?.edition) return undefined
    const held = heldMatch.current === match
    return held || matchShows(text.current, match) ? match : undefined
  }, [found, place])

  useEffect(() => {
    // Aloud from the sentence the page or screen starts with, the one holding
    // `from`, the character at the top in the edition read (`topRead`), in
    // the edition's own language, which a device with no voice for it says
    // rather than reading in another's.
    function readFromTop(from: { block: number; at: number }) {
      if (!book || !place) return
      const reading = book.editions[place.edition]
      const lang = reading.language
      if (lang && hasVoice(lang) === false) {
        setNotice(`This device has no voice for ${lang}.`)
        return
      }
      const { rate } = sliceFor(prefs, readAloudModule)
      let ended = false
      const stop = speak(
        sentenceTexts(reading),
        sentenceAt(reading.blocks, from.block, from.at),
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
        // The bars only while the keyboard holds them (`useKeyboardBars`),
        // else the reader itself, by its way back.
        else if (keyboardBars.held()) setBars(false)
        else onLeave()
        return
      }
      if (event.key === 'Tab' && !bars) {
        event.preventDefault()
        keyboardBars.shown()
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
      // The character at the top in the edition read: compared and paged,
      // the page's first of that edition (`Anchor.read`), as Back keeps it,
      // since the row's first block can lie pages back. Else `topOf`'s, which
      // compared is a row's start, or at a last row of only the other
      // columns' blocks, the start of that edition's last block.
      const topRead = () => {
        const read = keepsRead(text.current)
          ? anchorOf(text.current).read
          : undefined
        if (read) return read
        const top = topOf(text.current)
        const last = book.editions[place?.edition ?? book.hub].blocks.length - 1
        return top.block > last ? { block: last, at: 0 } : top
      }
      if (own?.intent === PEEK_TOGGLE && !comparing) {
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
          steppedFrom(),
          own.intent === SEARCH_NEXT ? 1 : -1,
          here(),
        )
        if (next) goToMatch(next)
      }
      if (place && notes.ready && own?.intent === NEW_INTENT) {
        const note = newNote(
          place.edition,
          topRead().block,
          DEFAULT_COLOUR,
          new Date(),
        )
        notes.change([...notes.notes, note])
        setOpenNote(note.id)
        setPanel('note')
      }
      if (own?.intent === READ_INTENT) {
        if (stopReading.current) stopReading.current()
        else readFromTop(topRead())
      }
      if (own?.intent === LOOKUP_TOGGLE) setPrefs(lookupToggled)
      if (own?.intent === UNDO_INTENT) notes.undo()
      if (own?.intent === REDO_INTENT) notes.redo()
      if (own?.intent === PIN_INTENT && admin && pinnable) {
        setPinning((p) => (p ? undefined : {}))
      }
      if (place && own && TEXT_SIZE_INTENTS.has(own.intent)) {
        resizeText((p) => resized(p, own.intent))
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
    keyboardBars,
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
    swapTo,
    goToMatch,
    here,
    steppedFrom,
    vertical,
    resizeText,
  ])

  if (failed) {
    return (
      <Message onLeave={onLeave} phrase={leaveTo.phrase}>
        This book could not be opened: {failed}
      </Message>
    )
  }
  if (!book || !place) {
    return <main className="reader book-reader" aria-busy="true" />
  }
  if (!edition || !layout) {
    return (
      <Message onLeave={onLeave} phrase={leaveTo.phrase}>
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
        relaid: true,
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
    setPlace({
      edition: place.edition,
      ...kept(text.current, anchor.current),
      relaid: true,
    })
    setPrefs((map) => fieldChanged(map, registry, id, key, raw))
  }
  const go = (block: number) => {
    setPlace({ edition: place.edition, block })
    savePlacing({ edition: place.edition, block })
  }
  // Block `block` of edition `to`, gone to by its counterpart in the edition
  // read where the link was drawn in a peek or another column, and where the
  // reader was kept for Back, the bars shown and any panel closed so Back is
  // seen.
  const jump = (to: string, block: number) => {
    // Compared and paged, a row can carry one block over many pages, so the
    // page is kept by how far into the row it is and the character read on it.
    setBack({
      edition: place.edition,
      ...(keepsRead(text.current)
        ? anchorOf(text.current)
        : topOf(text.current)),
    })
    keyboardBars.shown()
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
  // The cards look-up `n` asked for end its hold, unless a later look-up has
  // started, the panel has closed or the wait has run out: drawn then, they
  // would be another look-up's, or move what is drawn already.
  const settle = (
    n: number,
    cards: CharacterCard[],
    name?: { start?: number; length?: number },
  ) => {
    if (n !== asking.current) return
    setNamed((was) => (was?.waited ? was : { cards, ...name }))
  }
  // Who `from` names. The server looks for a name from its start, and where
  // a tap sends `before`, the words before the one tapped, from each of
  // those first, so that a tap on a later word of a name can find it.
  const askCharacters = (block: number, from: string, before = '') => {
    if (edition.characters !== true) {
      setNamed(undefined)
      return
    }
    const n = asking.current
    setNamed({})
    getCharacters(work, place.edition, {
      block,
      text: charactersText(from),
      before,
      earlier: sliceFor(prefs, lookupModule).includeEarlierBooks !== false,
    }).then(
      (answer) =>
        settle(n, answer.characters, {
          start: answer.start,
          length: answer.length,
        }),
      // A failure leaves the dictionary to show alone, logged rather than
      // left an unhandled rejection.
      (why: unknown) => {
        console.warn('The characters a look-up names were not read', why)
        settle(n, [])
      },
    )
  }
  // A tap on a word, with the look-up mode on, looks it up there and then:
  // Japanese by the longest dictionary word from the character tapped, which
  // only the server's answer can select, and any other language by the run of
  // letters tapped, Korean's taking in the digits written onto it, since 10년
  // is not the 년 that stands alone. Whether it looked anything up, so a tap
  // off a word, on a note's mark or on an aside keeps its usual meaning.
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
      askCharacters(
        point.block,
        block.text.slice(point.at),
        charactersBefore(block.text, point.at),
      )
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
    const word = wordAt(block.text, point.at, {
      digits: isKorean(edition.language),
    })
    if (!word) return false
    asking.current += 1
    askCharacters(
      point.block,
      block.text.slice(word[0]),
      charactersBefore(block.text, word[0]),
    )
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
        const inPage = onPage(text.current, event.target, event.clientX)
        // The first tap's panel may have moved the text from under the finger
        // or opened there, so on the page, the room beside the text as well as
        // the words, or anything near the first, this tap undoes the look-up
        // and peeks at what the first was on. A control away from the first is
        // the reader's next move, and left to act.
        const second =
          inPage ||
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
        // Only the page: a bar drawn over the panel still takes its own press.
        const over = event.currentTarget.querySelector(LOOK_UP_PANEL)
        const box = over?.getBoundingClientRect()
        if (
          box &&
          inPage &&
          event.clientX >= box.left &&
          event.clientX <= box.right &&
          event.clientY >= box.top &&
          event.clientY <= box.bottom
        ) {
          event.stopPropagation()
          takeTapClick(event, { mouseToo: true })
        }
      }}
      // The page's presses, on the text and on the room beside it inside the
      // reading area (`onPage`): on a wide window a paged column leaves room
      // either side, which is read by the area's thirds as the words are. A
      // press on a bar or a panel is its own.
      onPointerDown={(event) => {
        if (!onPage(text.current, event.target, event.clientX)) return
        setSelected(undefined)
        pressed.current = {
          x: event.clientX,
          y: event.clientY,
          at: event.timeStamp,
        }
      }}
      onPointerUp={(event) => {
        if (!onPage(text.current, event.target, event.clientX)) return
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
        // A page image fills its page, so a tap on it is the page's, as it
        // would be over words: its outer thirds turn the page, and its middle
        // shows or hides the bars. It opens nothing, its page already sizing
        // it as the comic reader sizes a page, and only its picture follows a
        // link it is set in (`onPageLink`). Scrolled or compared, it is
        // drawn as any figure is, and a tap on it is a figure's. A tap that
        // is the page's takes its click, a mouse's too, which would
        // otherwise follow a web link the image is set in as well.
        const pageImage =
          pagedPref && !comparing ? target.closest('[data-page-image]') : null
        const pagesOwn =
          pageImage !== null &&
          !onPageLink(text.current, pageImage, event.clientX, event.clientY)
        if (pagesOwn) {
          // A mousedown after this, as a finger's is, or before it, as a
          // mouse's is, would leave focus on that link, and Enter and Space
          // with it. So one after is held, and focus leaves whatever holds
          // it, a link a page away too, as a press on words would take it.
          takeTapClick(event, { keepFocus: true, mouseToo: true })
          ;(document.activeElement as HTMLElement | null)?.blur()
        }
        // An image set in a link is the link's, below.
        const tapped =
          pagesOwn || target.closest('[data-link]')
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
        const link = pagesOwn
          ? null
          : target.closest<HTMLElement>('[data-link]')
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
        // A tap that neither looks a word up nor turns a page at once (both
        // below) acts after `TAP_MS`; a second tap before then stops it,
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
        // A tap in a page's outer third turns it at once rather than after
        // `TAP_MS`, a wait every turn would feel. So it is not the first of
        // a double tap either: a second quick tap there is a tap of its own
        // on the new page, turning it again unless what the turn drew under
        // the point takes a tap, such as a link or a figure (O596), and a
        // peek is a double tap in the middle third. Compared, it waits, so
        // a sentence double-tapped in any column lights what each has for it.
        if (page !== 0 && !comparing && !panel && !noted && !hiddenLine) {
          lastTap.current = undefined
          window.clearTimeout(tapWait.current)
          tapWait.current = undefined
          // A double-click's second click there, where it turns the page
          // again, has selected a word on the page it turns from.
          window.getSelection()?.removeAllRanges()
          turn(text.current, page)
          // A finger's mousedown comes after the turn, onto what the new
          // page drew under it, where a web link would take focus and
          // Enter and Space with it. So it is held, and focus leaves what
          // holds it as a press on the text would.
          takeTapClick(event, { keepFocus: true })
          ;(document.activeElement as HTMLElement | null)?.blur()
          return
        }
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
      {bars && (
        <TopBar
          title={book.title}
          series={series ?? undefined}
          editionLine={
            comparing
              ? columnKeys.map((k) => book.editions[k].label).join(' · ')
              : interlinear
                ? `${edition.label} · ${book.editions[other].label}`
                : edition.label
          }
          layouts={other === undefined ? undefined : BOOK_LAYOUTS}
          layoutWords={BOOK_LAYOUT_WORDS}
          mode={mode}
          onMode={(to) => change(bookLayoutModule.id, 'mode', to)}
          paged={pagedPref}
          onPaged={() => change(bookLayoutModule.id, 'paged', !pagedPref)}
          settingsOpen={panel === 'settings'}
          onSettings={() => toggle('settings')}
          onLeave={onLeave}
          leaveWord={leaveTo.word}
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
      <div className="book-area">
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
          data-upscale={upscale || undefined}
          onScroll={() => {
            const host = text.current
            const placing = scrolledTo(host, placedAt.current)
            // The sight check's scroll is the app moving the text, not the
            // reader: it ends no hold and saves no place.
            const sight = scrolledTo(host, sightScroll.current)
            sightScroll.current = undefined
            if (!placing) placedAt.current = undefined
            if (!placing && !sight) {
              heldMatch.current = undefined
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
            // has no such edge: the line at the top is the reader's. A held
            // match stays the anchor through the sight check's scroll, so the
            // text laid out again, as closing a panel set in beside it does,
            // goes back to the match; with none held the anchor follows that
            // scroll, so laying out again keeps the word where it was taken.
            if (
              !placing &&
              !(sight && heldMatch.current) &&
              (!pagedPref || !shows(host, anchor.current))
            ) {
              anchor.current = anchorOf(host)
            }
            setTop(now)
            setPages(pagesOf(text.current, entries, now))
            // Beside the look-up the text is set in narrower. The set-in's own
            // placing keeps the reader where they were, in a layout they never
            // read in, so the block saved before still names their page and
            // one taken here can reopen a page early at full width. Any other
            // placing, a size step, a resize or reading aloud, lays the text
            // out again or moves the reader, which leaves that block stale.
            const kept = placing && placedSetIn.current && panel === 'lookup'
            if (!sight && !kept) {
              save({ edition: place.edition, block: resumeBlock(host) })
            }
          }}
          onPointerMove={(event) => {
            // A page image's link shows its pointer only where a press follows
            // it: no stylesheet can tell its picture from the room `contain`
            // leaves around it inside the image's own box.
            const figure = (event.target as Element).closest(
              '[data-page-image]',
            )
            if (!figure || !pagedPref || comparing) return
            figure
              .querySelector('a[data-link] img')
              ?.toggleAttribute(
                'data-follows',
                onPageLink(text.current, figure, event.clientX, event.clientY),
              )
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
      </div>
      {bars && (
        <div
          className="reader-bottom-bar"
          data-ring="bottom"
          style={{ ...bottomBarStyle, ...fade }}
        >
          <BookProgress
            progress={progress}
            ticks={entries.map((e) => through(at, e.block))}
            label={position}
            edition={place.edition}
            onSeek={(fraction, by) => {
              const block = blockAt(at, fraction)
              // A step forward that lands in the block at the top, or compared
              // in the row at the top, would go back to its start, from a
              // later page of it: the reader stays, and the next step goes on
              // from there.
              const held = comparing
                ? topRowOf(book, text.current, columnKeys)?.[0][1]
                : undefined
              if (by !== undefined && by > 0 && block < (held ?? top + 1)) {
                return
              }
              go(block)
            }}
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
                  // Out of compare and paging, which the reader may have left
                  // since, `into` would measure a box of another kind or size,
                  // so the block is placed `whole`, as a new layout places it.
                  // `read` holds in any layout but goes with `into`
                  // (`Anchor.read`), so a Back kept on a later page of a row
                  // lands on the row's first block, which can be pages before.
                  const { into: _, read: __, ...placed } = back
                  const to = keepsRead(text.current) ? back : placed
                  setPlace(to)
                  savePlacing(to)
                  setBack(undefined)
                }}
              >
                Back
              </button>
            )}
            <span style={readoutTextStyle}>
              {readoutRow({ position, time, chapter })}
            </span>
            {/* Where help is, as `Chrome`'s readout row ends (O425). */}
            {helpOn && (
              <button
                type="button"
                className="reader-help"
                aria-label="Help"
                aria-expanded={panel === 'help'}
                title={`Help (${HELP_KEY})`}
                onClick={() => toggle('help')}
              >
                ?
              </button>
            )}
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
            const word = block.text.slice(
              selected.range.start,
              selected.range.end,
            )
            askCharacters(selected.block, word)
            setLookingUp({
              edition: place.edition,
              block: selected.block,
              range: selected.range,
              word,
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
          looked={lookedDrawn}
          cards={cardsDrawn}
          tapsPass={tapsPass}
          onSave={
            notes.ready
              ? (entry) => {
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
                }
              : undefined
          }
          onClose={() => setPanel(undefined)}
        />
      )}
      {panel === 'notes' && (
        <NoteList
          book={book}
          notes={notes.notes}
          onOpen={(note) => {
            setPlace({ edition: note.edition, block: note.slot })
            savePlacing({ edition: note.edition, block: note.slot })
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
        from={steppedFrom}
        here={here}
        onPick={goToMatch}
        onResults={setMatches}
        onClose={() => setPanel(undefined)}
      />
      <Help
        visible={panel === 'help'}
        modules={enabledModules(registry.all(), prefs)}
        bindings={BOOK_KEYS}
        putsSeriesUnder={putsSeriesUnder}
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
              relaid: true,
            })
          }
          resetPrefs(id)
          setPrefs((map) => sliceReset(map, registry, id))
        }}
        onClose={() => setPanel(undefined)}
        onAccount={() => setPanel('account')}
      />
      {figure && (
        <FullFigure
          {...figure}
          upscale={upscale}
          onClose={() => setFigure(undefined)}
        />
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

/** A book that will not open, or a work with no book: why, and the way back, named by `phrase` (`''`: not known). */
function Message({
  children,
  onLeave,
  phrase,
}: {
  children: ReactNode
  onLeave: () => void
  phrase: string
}) {
  return (
    <main className="reader book-reader" style={rootStyle}>
      <p className="book-failed">{children}</p>
      <button type="button" className="reader-button" onClick={onLeave}>
        {phrase === '' ? 'Back' : `Back to ${phrase}`}
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
 * Compared, the row at the top of `text`, as each column's `[start, end)` of
 * blocks. Found by its place among the rows rather than by its name: the rows
 * past the end of the edition read, holding only the others' blocks, all
 * carry that edition's length as their name.
 */
function topRowOf(
  book: Book,
  text: HTMLElement | null,
  shown: string[],
): [number, number][] | undefined {
  const child = text && topChild(text)
  const index = text && child ? [...text.children].indexOf(child) : -1
  return index < 0 ? undefined : columns(book, shown)[index]
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

/** A block with no notes on it, shared so its props stay the same from one drawing to the next. */
const NO_NOTES: Shown[] = []
/** A block with no translation tapped open, shared for the same reason. */
const NONE_REVEALED: ReadonlySet<number> = new Set()

/**
 * Block `index` of the edition read, after the figures set before it. Memoised,
 * and handed only its own share of the reader's state (the sentence lit, a
 * match, a peek, the translations tapped open), so a placing, a sentence read
 * aloud or a peek draws again only the blocks whose words change, not the
 * thousands an edition runs to.
 */
const BookBlock = memo(function BookBlock({
  index,
  block,
  first,
  figures,
  notes,
  lit,
  litSentence,
  foundStart,
  foundEnd,
  peeked,
  peekSentence,
  under,
  hideTranslation,
  revealed,
  work,
  edition,
  furigana,
  vertical,
}: {
  index: number
  block: Block
  first: number
  figures: Figure[] | undefined
  notes: Shown[]
  lit: boolean
  litSentence: number | undefined
  foundStart: number | undefined
  foundEnd: number | undefined
  peeked: Parameters<typeof Peek>[0] | undefined
  peekSentence: number | undefined
  under: Parameters<typeof Interlinear>[0]['under'] | undefined
  hideTranslation: boolean
  revealed: ReadonlySet<number>
  work: string
  edition: string
  furigana: boolean
  vertical: boolean
}) {
  const Block = block.heading ? 'h2' : 'p'
  const highlights = notes
    .filter(({ note, own }) => own && note.text)
    .map(({ note }) => note)
  return (
    <>
      <Figures figures={figures} work={work} edition={edition} />
      <Block
        data-block={index}
        data-line={block.line ?? undefined}
        data-lit={lit || undefined}
      >
        {notes
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
            first={first}
            highlights={highlights}
            work={work}
            edition={edition}
            furigana={furigana}
            lit={
              litSentence === undefined
                ? undefined
                : [litSentence, litSentence + 1]
            }
            found={
              foundStart === undefined || foundEnd === undefined
                ? undefined
                : [foundStart, foundEnd]
            }
            upright={vertical}
            asideAfter={(n) => (
              <>
                {peeked && peekSentence === n && <Peek {...peeked} />}
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
        {peeked && peekSentence === undefined && <Peek {...peeked} />}
      </Block>
    </>
  )
})

/**
 * The images a book sets between its blocks, from `edition` of `work`. A
 * figure has no `data-block`, so a peek, a note and the block at the top pass
 * over it to the blocks either side, and the reader's place, where the screen
 * starts in one, names it from the block after it (`Anchor.figure`). Tapped,
 * it opens full size (`data-figure`, in the reader's `onPointerUp`), or follows
 * the link it is set in; a page image, paged and not compared, opens nothing
 * (`onPageLink`).
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
      <figure
        key={k}
        className="book-figure"
        data-figure
        data-page-image={isPageImage(figure) || undefined}
      >
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
 * A figure tapped open, no larger than the screen lets it be drawn, and larger
 * than its own size only while `upscale`, the comic reader's upscale-to-fit,
 * is on. A modal `<dialog>` outside the text, so no press on it reaches the
 * text's taps; Escape (the reader's key handler, or the browser's close
 * request) or a tap beside the picture closes it, on the room `contain`
 * leaves around it inside the image too.
 */
function FullFigure({
  src,
  alt,
  upscale,
  onClose,
}: {
  src: string
  alt: string
  upscale: boolean
  onClose: () => void
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  useLayoutEffect(() => {
    if (dialog.current?.open === false) dialog.current.showModal()
  }, [])
  const backdrop = useBackdropClose(
    onClose,
    (event) =>
      event.target === event.currentTarget ||
      (event.target instanceof HTMLImageElement &&
        !onPicture(event.target, event.clientX, event.clientY)),
  )
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the click is a tap beside the image; a keyboard closes it with Escape, the dialog's own cancel
    <dialog
      ref={dialog}
      className="book-figure-full"
      data-upscale={upscale || undefined}
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
