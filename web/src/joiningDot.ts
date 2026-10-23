/**
 * A dot written between two parts of one line of text (" · this is you", a
 * last scan's time and outcome), held to the words either side of it by
 * no-break spaces, so that a line wrapping there neither starts nor ends
 * with it. The dots the stylesheet draws between items are marked by
 * `lineEnds.ts`'s `useLineEnds` instead.
 */
export const JOINING_DOT = '\u00a0·\u00a0'
