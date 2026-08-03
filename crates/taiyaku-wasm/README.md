# taiyaku-wasm

The binding a browser reader loads: parse a sidecar, convert between a flat
slot index and a `(page, half)` pair, resolve a slot between two editions, and
say which path the answer walked.

## Building

```sh
./scripts/build-wasm.sh          # emits crates/taiyaku-wasm/pkg/
node scripts/smoke-wasm.mjs      # runs the module and checks six properties
```

`pkg/` is generated and not committed. `web/` depends on it as `file:` and so
must be built before `npm install` in `web/`.

## Why the logic is in `binding.rs`

`lib.rs` is a `wasm_bindgen` shell that converts types and decides nothing;
`binding.rs` is plain Rust and is what the tests exercise, so the behaviour
under test is the behaviour that ships. `scripts/smoke-wasm.mjs` covers the
part that arrangement cannot reach — that the compiled artefact behaves as the
native build does.
