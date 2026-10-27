/**
 * 対訳, *taiyaku*: the seal set vertically in the accent, then the name.
 *
 * One component for the three places it is drawn — the sign-in screen's
 * heading, the phone bar's way home and the top of the sidebar (spec
 * 2026-09-17 §7.1) — so the seal cannot drift between them. It sets no size
 * and no weight: each place's own rule does, and the sign-in screen's `<h1>`
 * gives it the heading's. The name is a part of its own, so that where the
 * wordmark is a link its name alone is underlined under the pointer
 * (`shell.css`).
 *
 * The seal is `aria-hidden`, so what a screen reader names is *taiyaku*;
 * `Login.test.tsx` holds that for the heading.
 */
export function Wordmark() {
  return (
    <span className="wordmark">
      <span className="seal" aria-hidden="true">
        対訳
      </span>
      <span className="wordmark-name">taiyaku</span>
    </span>
  )
}
