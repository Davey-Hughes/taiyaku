/**
 * The licence texts `/about/licence` draws (`Licence.tsx`), each bundled at
 * build from the file it is, so the page cannot drift from the files that
 * ship: the repository's own `LICENSE`, each `@fontsource` package's, the
 * one vendored beside the marks subset in `styles/fonts/`, and libarchive's
 * `COPYING` in `licences/`, which must be the one from the libarchive
 * release the container image links.
 *
 * **Reached only by `Licence.tsx`'s `import()`**, which puts these texts in a
 * chunk of their own. A static import of this module anywhere would put them
 * in the chunk every page loads.
 *
 * `../../LICENSE` is outside `web/`. The Dockerfile's web stage copies it to
 * where this path resolves, and `vite.config.ts`'s `server.fs.allow` names
 * it, with and without the query; its comment says why.
 */
import shipporiMincho from '@fontsource/shippori-mincho-b1/LICENSE?raw'
import zenKakuGothicNew from '@fontsource/zen-kaku-gothic-new/LICENSE?raw'
import taiyaku from '../../LICENSE?raw'
import libarchive from './licences/libarchive-COPYING.txt?raw'
import marks from './styles/fonts/OFL.txt?raw'

export { libarchive, marks, shipporiMincho, taiyaku, zenKakuGothicNew }
