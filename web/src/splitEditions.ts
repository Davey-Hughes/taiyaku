/**
 * The `GET /api/library/split-editions` client: the editions a library's
 * directories hold some of and not all of, which an account granted only
 * some of the libraries holding them sees narrowed (spec §3.8). Administrators only.
 */
import { getJson } from './api'

/** A directory holding a copy of a member, and the library it is in. */
export interface MemberDirectory {
  /** The directory's id, which its page is addressed by. */
  root: number
  path: string
  /** The library's id, one of the edition's `libraries`. */
  library: number
}

/** One file of a split edition and the directories holding a copy of it. */
export interface SplitMember {
  /** As the store keeps it. */
  path: string
  pages: number
  /** By path. */
  directories: MemberDirectory[]
}

/**
 * A library holding some of a split edition's files: how many, and their
 * pages, which is what an account granted it alone is sent.
 */
export interface SplitLibrary {
  id: number
  name: string
  members: number
  pages: number
}

export interface SplitEdition {
  /** The work's slug. */
  work: string
  title: string
  /** The edition's key. */
  edition: string
  /** Every member's pages. */
  pages: number
  /** In reading order. */
  members: SplitMember[]
  /** By name. */
  libraries: SplitLibrary[]
}

/** By work title, then edition key: the order the route answers in. */
export const listSplitEditions = () =>
  getJson<{ editions: SplitEdition[] }>('/api/library/split-editions')
