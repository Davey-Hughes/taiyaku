import type { Mode } from './pairing'

/** A reader's layout: the page reader's three, or two editions compared. */
export type Layout = Mode | 'compare'

/** The page reader's three layouts, in the order its top bar draws them. */
export const PAGE_LAYOUTS: readonly Mode[] = ['single', 'two', 'scroll']

/** The page reader's layouts for a work of more than one edition. */
export const PAGE_LAYOUTS_COMPARED: readonly Layout[] = [
  ...PAGE_LAYOUTS,
  'compare',
]

/**
 * The book reader's layouts: one edition, two compared row by row, or one
 * with the next edition's sentences under its own.
 */
export type BookLayout = 'scroll' | 'compare' | 'interlinear'

export const BOOK_LAYOUTS: readonly BookLayout[] = [
  'scroll',
  'compare',
  'interlinear',
]

/** Any reader's layout, as the top bar draws them. */
export type AnyLayout = Layout | BookLayout

/**
 * What the page reader calls each layout: its top bar button's name and hover
 * text, and its word in the settings panel. The stored value stays the key,
 * so a word can change and a saved layout still loads.
 */
export const PAGE_LAYOUT_WORDS: Record<Layout, string> = {
  single: 'One page',
  two: 'Two pages',
  scroll: 'Scroll through the pages',
  compare: 'Compare editions',
}

/**
 * What the book reader calls each layout, as `PAGE_LAYOUT_WORDS` does. The
 * one-edition layout is stored as `scroll`, but the paged switch decides
 * whether it scrolls or turns, so its words say neither.
 */
export const BOOK_LAYOUT_WORDS: Record<BookLayout, string> = {
  scroll: 'One edition',
  compare: 'Compare editions',
  interlinear: 'Each sentence over its translation',
}
