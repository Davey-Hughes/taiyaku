import type { Kind } from './labelling'

/**
 * One text run a person drew, in page fractions — the wire shape of
 * `taiyaku-core`'s `truth.rs::TruthRegion`, field for field.
 *
 * `axis` is `'v' | 'h' | null`, not optional: a radial sound effect has no
 * reading direction, and the server records that refusal as `null` rather
 * than as an absent key. `ignore` and `text` *are* optional, matching the
 * Rust's `#[serde(default)]` on both — a document written before either
 * existed still parses.
 */
export interface TruthRegion {
  x: number
  y: number
  w: number
  h: number
  axis: 'v' | 'h' | null
  kind: Kind
  /** Neither a hit nor a false positive, and counted out loud. Absent means false. */
  ignore?: boolean
  /** Reserved. An OCR pass fills this in later; nothing here writes it yet. */
  text?: string | null
}

/**
 * One edition's labelling: which archive it was drawn against, and what was
 * found on each page looked at.
 *
 * `pages` is keyed by the page's string index because that is what a
 * `BTreeMap<u32, _>` becomes crossing `serde_json` into a JS object — an
 * absent key means nobody has looked at that page yet, and a present empty
 * array means somebody looked and found no text. The distinction is the
 * progress tracker.
 */
export interface TruthEdition {
  members: { sha256: string }[]
  pages: Record<string, TruthRegion[]>
}

/** The whole `<stem>.truth.json` document, exactly as the server holds it. */
export interface TruthDoc {
  taiyaku_truth: number
  /** Edition key, exactly as the sidecar spells it. */
  editions: Record<string, TruthEdition>
}
