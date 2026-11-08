/**
 * `n` of a thing, in the number the count is. Zero takes the plural, "0
 * works", which is English rather than an oversight: only one is singular.
 */
export function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}
