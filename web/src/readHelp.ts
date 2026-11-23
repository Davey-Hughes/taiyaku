/**
 * The help a directory's page gives for setting it up (O315): under the
 * "How it is read" heading, under each of its three settings, and under
 * Exclude globs in "How it is scanned".
 *
 * The words are the owner's to approve, so they are kept here and changed
 * nowhere else. RootRow.test.tsx holds them all word for word, and
 * markup.browser.test.tsx, in the browser project CI does not run, holds
 * Exclude globs': each fails until it is given newly approved words. Plain
 * text only: no backticks, which the UI font draws with no advance (O179).
 */
export const READ_HELP = {
  /** Under the "How it is read" heading. */
  section:
    'Set these before the directory’s first scan. To do that, add it with Don’t scan yet ticked, set them here, then press Scan directory.',
  /** Under Edition key. */
  editionKey:
    'The name of the edition this directory’s volumes become, unless a rule says otherwise. Left empty, the key is raw. Two directories that hold the same volumes need different keys, such as en and ja: under one key, their copies of a volume become one long edition.',
  /** Under Default kind. */
  defaultKind:
    'What this directory’s editions are, unless a rule says otherwise: raw, official or fan. It labels them; only the key decides which files make one edition.',
  /** Under Default language. */
  defaultLanguage:
    'The language of this directory’s editions, unless a rule says otherwise, as a short tag such as en or ja. Like the kind, it labels them.',
  /** Under Exclude globs, in "How it is scanned". */
  excludeGlobs:
    "One gitignore pattern per line. A trailing slash or a bare name excludes the directory and all under it; a '/**' suffix excludes only what is inside it. A [ starts a set of characters, as in [0-9], so to match a [ in a name, write \\[ instead.",
} as const
