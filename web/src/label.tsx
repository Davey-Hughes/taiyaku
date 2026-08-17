import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { getSidecar, getTruth, listWorks, putTruth } from './api.ts'
import './index.css'
import { Labeller } from './Labeller.tsx'
import type { TruthPorts } from './truthWrite.ts'

/**
 * `api.ts`'s work/sidecar/truth calls, bound once — `App.tsx`'s
 * `STARTUP_PORTS`/`NOTES_PORTS` are the same idiom for the same reason: an
 * object built once at module level rather than on every render.
 *
 * `works` maps `listWorks`'s objects down to bare slugs: `TruthPorts.works`
 * predates Task 14's richer `/api/works` payload and this picker only ever
 * shows the id (`Labeller.tsx`'s `<option>` list, keyed and labelled by the
 * same string) — the title beside it already comes from a separate sidecar
 * read once a work is chosen, so there is nothing here for the extra fields
 * to do.
 */
const TRUTH_PORTS: TruthPorts = {
  works: () => listWorks().then((works) => works.map((w) => w.slug)),
  sidecar: getSidecar,
  get: getTruth,
  put: putTruth,
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Labeller ports={TRUTH_PORTS} />
  </StrictMode>,
)
