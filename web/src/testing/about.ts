// What `GET /api/about` answers, as `taiyaku-serve`'s `about.rs` builds it:
// every format in the route's order, at the versions the writers carry,
// with the sentence the route sends for each.
import type { About } from '../about'

/** A build with a commit. */
export const ABOUT: About = {
  commit: '5ee51597e6269deff221b5954e72296b697d71b2',
  dirty: false,
  repository: 'https://github.com/Davey-Hughes/taiyaku',
  catalog: '/opds/v1.2/catalog',
  formats: [
    {
      name: 'taiyaku.json',
      version: '1',
      description:
        'Sits beside a work’s archives and records how its editions correspond to each other.',
    },
    {
      name: '<stem>.library.json',
      version: '1',
      description:
        'A work’s metadata, kept in a file so fixes made by hand outlive the database.',
    },
    {
      name: '<stem>.progress.json',
      version: '1',
      description: 'One reader’s place in a work, edition by edition.',
    },
    {
      name: '<stem>.truth.json',
      version: '1',
      description:
        'Boxes a person drew around the text on pages, to score text detection against.',
    },
    {
      name: '<stem>.regions.json',
      version: '1',
      description:
        'Where the text is on each page, as the regions command found it.',
    },
    {
      name: '<stem>.notes.json',
      version: null,
      description: 'Notes one reader left on a work’s pages.',
    },
    {
      name: 'Database schema',
      version: '17',
      description:
        'The layout of the SQLite database taiyaku keeps its library in.',
    },
    {
      name: 'OPDS, with the Page Streaming Extension',
      version: '1.2',
      description:
        'The catalog that reading apps browse and stream pages from.',
    },
    {
      name: 'HTTP API',
      version: null,
      description: 'The /api routes this web app uses to talk to the server.',
    },
    {
      name: 'wasm module',
      version: null,
      description:
        'taiyaku’s core built for the browser, which the reader uses to match pages between editions.',
    },
  ],
}

/** The same build reporting no commit. */
export const UNKNOWN_BUILD: About = { ...ABOUT, commit: null, dirty: false }

/** The same commit, built from a tree whose tracked files differed from it. */
export const DIRTY_BUILD: About = { ...ABOUT, dirty: true }
