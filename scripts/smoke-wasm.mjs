// Building the crate for wasm32 and testing its logic natively leaves codegen
// unproven. This loads the real .wasm, parses the real fixture, and checks the
// properties the reader depends on.
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

// The same document as `binding.rs`'s CHAPTERED_FIXTURE: `chapters` is
// assembled from three members and `volume` from one, the shape a
// chapter-organised edition has. `chapterAt` is untested by anything except
// this script and `cargo test -p taiyaku-wasm` — this is the only command
// that runs it through the real, compiled WebAssembly.
const CHAPTERED = JSON.stringify({
  taiyaku: 1,
  work: { title: 'T', reading: 'rtl' },
  editions: {
    volume: { label: 'V', members: [{ path: 'v01.cbz', sha256: '00', pages: 6 }] },
    chapters: {
      label: 'C',
      members: [
        { path: 'c001.cbz', sha256: '01', pages: 2 },
        { path: 'c002.cbz', sha256: '02', pages: 2 },
        { path: 'c003.cbz', sha256: '03', pages: 2 },
      ],
    },
  },
  hub: 'volume',
  alignments: [
    { from: 'volume', to: 'chapters', by: 'solver', runs: [{ a: '0-5', b: '0-5' }] },
  ],
})

const chaptered = WasmSidecar.fromJson(CHAPTERED)
const chapter = chaptered.chapterAt('volume', 0)
check(
  'chapterAt volume 0',
  { member: chapter.member, of: chapter.of, path: chapter.path },
  { member: 0, of: 3, path: 'c001.cbz' },
)
check('chapterAt volume 999', chaptered.chapterAt('volume', 999), undefined)
check('chapterAt nope 0', chaptered.chapterAt('nope', 0), undefined)

console.log('\nthe module runs.')
