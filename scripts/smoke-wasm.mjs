// The first execution of this module under any JS runtime.
//
// M4 gated that the crate *builds* for wasm32 and tested its logic natively,
// and recorded that this leaves codegen unproven. This closes that: it loads
// the real .wasm, parses the real fixture, and checks the two properties the
// reader depends on before a single line of React exists.
import { readFile } from 'node:fs/promises'
import init, { WasmSidecar } from '../crates/taiyaku-wasm/pkg/taiyaku_wasm.js'

const wasm = await readFile(
  new URL('../crates/taiyaku-wasm/pkg/taiyaku_wasm_bg.wasm', import.meta.url),
)
await init({ module_or_path: wasm })

// The same document as `binding.rs`'s FIXTURE. `b`'s page 2 is a joined
// spread, so `b` presents nine slots for eight pages.
const FIXTURE = JSON.stringify({
  taiyaku: 1,
  work: { title: 'T', reading: 'rtl' },
  editions: {
    a: { label: 'A', members: [{ path: 'a.cbz', sha256: '00', pages: 8 }] },
    b: { label: 'B', members: [{ path: 'b.cbz', sha256: '01', pages: 8 }], spreads: [2] },
  },
  hub: 'a',
  alignments: [
    {
      from: 'a', to: 'b', by: 'solver',
      runs: [{ a: '1-3', b: '0-2R' }, { a: '5-7', b: '5-7' }],
    },
  ],
})

const sc = WasmSidecar.fromJson(FIXTURE)

const check = (label, actual, expected) => {
  const a = JSON.stringify(actual)
  const e = JSON.stringify(expected)
  if (a !== e) throw new Error(`${label}: got ${a}, want ${e}`)
  console.log(`ok  ${label}`)
}

check('editions', [...sc.editions()].sort(), ['a', 'b'])
check('slotCount a', sc.slotCount('a'), 8)
check('slotCount b', sc.slotCount('b'), 9)

const halved = sc.slotAt('b', 2)
check('slotAt b 2', { page: halved.page, half: halved.half }, { page: 2, half: 'R' })
check('indexOf b (2,R)', sc.indexOf('b', 2, 'R'), 2)

const landed = sc.resolveSlot('a', 'b', 1, undefined)
check('resolveSlot a1 to b', { page: landed.page, half: landed.half }, { page: 0, half: undefined })

console.log('\nthe module runs.')
