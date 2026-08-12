import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { getSidecar, getTruth, listWorks, putTruth } from './api.ts'
import './index.css'
import { Labeller } from './Labeller.tsx'
import type { TruthPorts } from './truthWrite.ts'

/** `api.ts`'s work/sidecar/truth calls, bound once — `App.tsx`'s `STARTUP_PORTS`/`NOTES_PORTS` are the same idiom for the same reason: an object built once at module level rather than on every render. */
const TRUTH_PORTS: TruthPorts = {
  works: listWorks,
  sidecar: getSidecar,
  get: getTruth,
  put: putTruth,
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Labeller ports={TRUTH_PORTS} />
  </StrictMode>,
)
