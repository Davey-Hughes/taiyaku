import type { WorkDetail } from './library'

/**
 * What the reader's top bar says (spec 2026-09-17 §8.3): the work's title and,
 * under it, the edition on screen — `Vol. N · <label>`, or the label alone
 * for a work with no volume.
 *
 * From the library's answer to `/api/library/works/{slug}` rather than the
 * sidecar, because that is where a metadata edit lands (`title_hand`,
 * `label_hand`); the sidecar is the file on disk. Until the library answers,
 * or if it never does, the edition's key is all there is to say, and the
 * title is absent rather than guessed.
 */
export interface TitleLines {
  title?: string
  editionLine: string
}

/** `edition`'s label as the library has it, or its key until it answers. */
export function labelOf(
  detail: WorkDetail | undefined,
  edition: string,
): string {
  return detail?.editions.find((e) => e.key === edition)?.label ?? edition
}

export function titleLines(
  detail: WorkDetail | undefined,
  edition: string,
): TitleLines {
  if (!detail) return { editionLine: edition }
  const label = labelOf(detail, edition)
  return {
    title: detail.title,
    editionLine:
      detail.volume === null ? label : `Vol. ${detail.volume} · ${label}`,
  }
}

/**
 * The reader's name in one line — its hidden `h1` and its tab title (O163,
 * O173): the work, then the edition line, joined the way the edition line
 * joins its own parts.
 */
export function readerName({ title, editionLine }: TitleLines): string {
  return title === undefined ? editionLine : `${title} · ${editionLine}`
}
