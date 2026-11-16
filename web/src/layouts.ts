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
