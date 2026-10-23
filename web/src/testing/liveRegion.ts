/**
 * Counts the changes made inside `region` from now on, which are what a
 * screen reader reads out of a live region: the function handed back stops
 * counting and answers the count.
 */
export function changesIn(region: Element): () => number {
  const seen: MutationRecord[] = []
  const watch = new MutationObserver((records) => seen.push(...records))
  watch.observe(region, { subtree: true, childList: true, characterData: true })
  return () => {
    seen.push(...watch.takeRecords())
    watch.disconnect()
    return seen.length
  }
}
