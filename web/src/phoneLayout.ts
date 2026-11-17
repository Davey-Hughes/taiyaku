/**
 * The phone layout's one condition, and its complement where the sidebar
 * stands. A screen under 500 px tall is crowded by the sidebar's foot as a
 * narrow one is by its width, so either turns the layout. Every viewport
 * query in the stylesheets writes one of these letter for letter, but for
 * two named exceptions that turn at a width their own content needs
 * (`phoneLayout.css.test.ts`), and a script that must know the layout
 * before anything is drawn asks `matchMedia` with these rather than a
 * number of its own.
 */
export const PHONE_LAYOUT_PARTS = [
  '(width < 900px)',
  '(height < 500px)',
] as const
export const PHONE_LAYOUT = PHONE_LAYOUT_PARTS.join(' or ')
export const SIDEBAR_LAYOUT = '(width >= 900px) and (height >= 500px)'
