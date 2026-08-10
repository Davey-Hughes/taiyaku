# Test coverage, and the number that was flattering us — design

**Status: settled in brainstorming on 2026-08-09. Built, and shipped on
`web-coverage-gate`.** Web coverage went 48.13% → 98.03% statements and
45.21% → 94.38% branches; Rust `main.rs` 43.79% → 95.82% regions. Both gates
are enforced — aggregate thresholds plus `scripts/coverage-floor.mjs`.

**Two things this document did not anticipate, recorded here because a design
read later should not have to rediscover them.** §8 assumed thresholds would be
the gate; proving them found that an aggregate cannot see one module go dark —
22 Rust modules of 100+ covered lines, and any web module `App.test.tsx` also
exercises — so the gate grew a per-file floor and a minimum file count that §8
never mentions. And §6's premise, that extraction would shrink `App.tsx` enough
to matter, held for test *quality* and not for line counts: four tasks moved 749
lines and 159 statements, against estimates roughly 2× higher, because JSX is
statement-sparse and every extraction writes a pointer paragraph back. What took
`App.tsx` from 0% to 94.84% was Task 14's integration tests, not the
extractions.

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

## Where it came from

Asked for directly, and stacked on the pnpm branch: measure test coverage and
make the PR checks enforce a bar. What the measuring found changed the shape of
the work twice before any of it was designed.

## 1. The default number is 98% and it is meaningless

`vitest --coverage` with no `include` reports **98.43%** statements over the
`web/` node project. It lists fifteen files. `web/src` holds fifty-two source
files, of which fifty are inside the gate once `main.tsx` and `browser-setup.ts`
are excluded.

v8 coverage instruments what the test run loaded, so a file no test ever
imports is not at 0% — it is absent. The headline answers "is the code we test
tested?", to which the answer is always yes.

Setting `coverage.include` explicitly to `src/**/*.ts` and `src/**/*.tsx`:

| counted | statements | branches | functions |
| ------- | ---------- | -------- | --------- |
| only files a test imported | 98.43% | 95.22% | 98.49% |
| all of `src/`, node project | **48.08%** | 45.21% | 52.40% |
| all of `src/`, both projects | **49.28%** | 46.27% | 53.61% |

**The gate must set `coverage.include` explicitly.** This is the single most
important line in the whole design: without it the gate would have shipped
reporting 98% and enforcing nothing.

## 2. Both languages are a covered core and an uncovered periphery

### Web

```
.ts logic, src/modules/     93–100%     effectively done
.tsx components                  0%     except PageView.tsx
resolver.ts                      0%     imports taiyaku-wasm as a value
```

The six browser tests take `PageView.tsx` from 0% to 92.3% and move the total
by 1.2 points. They cannot help further: they need a downloaded Chromium, and
`docs/ci.md` records why CI will not carry one.

### Rust

`cargo llvm-cov --workspace --all-features`, `TAIYAKU_CORPUS` unset (which is
CI's state): **81.41% regions, 78.26% functions, 82.73% lines**.

But four files in that average are untested *on purpose*:

| file | regions | missed | cover |
| ---- | ------- | ------ | ----- |
| `taiyaku-cli/src/geometry_experiment.rs` | 1272 | 1071 | 15.80% |
| `taiyaku-cli/src/region_experiment.rs` | 1022 | 586 | 42.66% |
| `taiyaku-cli/src/corpus.rs` | 626 | 539 | 13.90% |
| `taiyaku-serve/src/corpus.rs` | 267 | 247 | 7.49% |

The two `_experiment` modules are gated one-off measurements — `docs/status.md`
already describes `geometry_experiment` as exactly that, and it fingerprints
roughly 1.4 million crops when it does run. The two `corpus` modules cannot
execute without `TAIYAKU_CORPUS`, which CI never has.

**Excluding those four takes Rust from 81.41% to 89.34% regions** — 20,838
covered of 23,324, arithmetic from the table above. No work earns that; the
81.41% was simply averaging in code nobody intends to test.

## 3. What is excluded, and why it is a list rather than a pattern

Every exclusion is named individually with its reason recorded beside it. No
wildcard exclusions, because a wildcard silently swallows the next file that
matches it, and the whole failure this design exists to prevent is a coverage
number that describes something other than what a reader thinks.

| excluded | reason |
| -------- | ------ |
| `taiyaku-cli/src/geometry_experiment.rs` | gated one-off measurement module |
| `taiyaku-cli/src/region_experiment.rs` | gated one-off measurement module |
| `taiyaku-cli/src/corpus.rs` | needs `TAIYAKU_CORPUS`; CI has no corpus |
| `taiyaku-serve/src/corpus.rs` | needs `TAIYAKU_CORPUS`; CI has no corpus |
| `web/src/main.tsx` | entry point, 10 lines, nothing to assert |
| `web/src/browser-setup.ts` | vitest browser harness, 15 lines |
| `web/src/**/*.test.*` | tests are not the subject |

`resolver.ts` is **not** excluded. It is 0% because it imports `taiyaku-wasm` as
a value and no test loads it, but that is a testability problem with a fix — a
mocked module — not a property of the code that deserves permanent amnesty.

## 4. Rust: the gate costs almost nothing

`cargo llvm-cov` *runs the tests*. So it **replaces** `cargo test --workspace
--all-features` in the existing `test` job rather than adding a job — the cost
is instrumentation overhead on a build CI already pays for, not a second cold
compile. Rejected alternative: a separate `coverage` job, which would have
duplicated the whole workspace build.

The binary is fetched as a prebuilt release tarball, the precedent this
workflow already sets for `wasm-pack`:
`cargo-llvm-cov-x86_64-unknown-linux-musl.tar.gz`, **1.8 MB**, confirmed to
exist at v0.8.7. musl, so it runs on the glibc `bookworm` container without a
matching libc — the same reasoning `docs/ci.md` records for `wasm-pack`.

`llvm-tools` is also required. It is present on this development machine as
`llvm-tools-x86_64-unknown-linux-gnu`; **in CI it is not yet measured**, and the
plan must add `rustup component add llvm-tools-preview` and confirm.

**Timing, measured and unmeasured.** A warm run on this machine is **7.8s
wall** — but that is a warm instrumented tree. The cold instrumented build was
not timed, and instrumented artefacts do not share a cache with ordinary ones,
so CI pays a full cold compile. **Guess, not measurement:** comparable to the
existing `test` job plus instrumentation. Confirm against a real run, and if it
proves expensive, that is the moment to reconsider the separate-job rejection
above.

### The work, ranked

| file | cover | missed regions |
| ---- | ----- | -------------- |
| `taiyaku-cli/src/main.rs` | 43.79% | **1159** |
| `taiyaku-cli/src/inventory.rs` | 72.94% | 187 |
| `taiyaku-cli/src/report.rs` | 32.56% | 116 |
| `taiyaku-cli/src/fingerprint.rs` | 77.49% | 113 |
| `taiyaku-serve/src/lib.rs` | 84.46% | 101 |
| `taiyaku-serve/src/stump/http.rs` | 66.80% | 83 |
| `taiyaku-cli/src/cache.rs` | 12.50% | 35 |

`main.rs` alone is 55% of the remaining Rust work. It is argument parsing and
dispatch for nine commands — unglamorous and entirely testable.

## 5. Web: a third vitest project

Component tests go in a new **`jsdom`** project, not the existing `browser` one.
The reason is the gate's own integrity: `browser` needs a downloaded Chromium
that CI deliberately does not carry, so anything measured there is invisible to
the gate and a developer's local number would exceed CI's. A gate must measure
what CI can reproduce.

That leaves three projects with three jobs:

| project | environment | answers |
| ------- | ----------- | ------- |
| `node` | none | pure logic, no DOM — 654 tests today |
| `jsdom` | jsdom | component rendering, props, events — **new** |
| `browser` | real Chromium | layout and real `decode()` — 6 tests, local only |

**`jsdom` is the only new dependency, and `@testing-library/react` is not
added.** This corrects an earlier draft of this section, which assumed Testing
Library before the existing tests had been read. `PageView.browser.test.tsx`
already mounts components with `createRoot` and `act` from `react-dom/client`
directly, and carries a nine-line `mount(ui)` helper doing exactly what a
Testing Library `render` would. That helper moves to a shared test utility and
both the `jsdom` and `browser` projects use it. This repository's own rule —
recorded in the M5b plan as "a dev-only dependency needs a stated reason" — is
the deciding argument: a second mounting API earns nothing over the one already
working here.

Revisit only if a component task demonstrates a concrete need Testing Library
answers and hand-rolled queries do not, and record that need when it happens.

The `browser` project keeps the properties `jsdom` structurally cannot see:
`docs/notes.md` records that jsdom has no layout engine, so
`getBoundingClientRect` returns zeroes, and there is no real
`HTMLImageElement.decode()`.

## 6. `App.tsx`: extract, then test

`App()` is **one function spanning lines 334 to 3359** — roughly 3,025 lines,
holding somewhere in the low-to-mid twenties of each of `useState`, `useRef`
and `useEffect`. The file has five top-level declarations in total. It is 54%
of all component code in the repository.

**An earlier draft of this section said "33 `useState`, 33 `useRef` and 21
`useEffect`", and that was wrong** — those are `grep -c` counts, and this file
writes at length *about* its own hooks, so the prose was counted alongside the
calls. Stripping comments and counting real call sites gives 23/23/20 by one
method and 26/30/20 by a careful read during the extraction map's inventory;
`useMemo` is 0 real call sites against 2 raw matches. The disagreement is in
how multi-line and generic-parameterised calls are matched, and it does not
matter here: what the design rests on is that this is *dozens* of pieces of
state in one function, not the precise dozen. It is corrected because a figure
this document asserts should be one somebody measured, and this one was not.

It cannot be unit-tested. It can only be rendered whole and driven, and a
failure in a 3,000-line component names nothing.

**The approach is extraction, and the evidence is in this repository already.**
Every part of this codebase that was pulled into a pure module — `src/modules/`,
`pairing.ts`, `notes.ts`, `viewport.ts`, `alignedPairs.ts` — sits at 93–100%
coverage with no DOM and no browser. The pattern is proven here. `App.tsx` is
the one file that never followed it.

So: state machines and effects move into pure modules beside their neighbours,
tested directly; `App()` keeps wiring and is covered by a small number of
integration tests.

**The module boundaries are deliberately not specified here.** They come from
reading the file during implementation, and inventing them from a `grep` of
hook counts would be exactly the kind of guess this document is supposed to
forbid. The 33 `useState` do cluster into recognisable responsibilities —
correction and history, decode and prefetch orchestration, session and progress
persistence, notes — and that is a starting hypothesis, not a design.

Rejected alternative: **integration-test it whole**, rendering `<App/>` with
`api` and `resolver` mocked. It would raise the number fastest and change no
existing code. It was rejected because the tests would be slow, coupled to
markup, awkward to set up across 33 pieces of state, and — the deciding
reason — a failure would point at three thousand lines rather than at a unit.

## 7. The other eleven components

`@testing-library/react` on the `jsdom` project, smallest first so the setup is
proven before the hard ones:

```
Viewport.tsx      70      Chrome.tsx       294
Gap.tsx          119      NotePanel.tsx    345
NoteList.tsx     129      Filmstrip.tsx    397
Help.tsx         147      Annotations.tsx  695
CreditsPanel.tsx 166      PageView.tsx     260   (92.3% via browser already)
Settings.tsx     218
```

About 2,840 lines excluding `App.tsx`.

## 8. Thresholds

**Set at the end, at the numbers actually achieved, minus a small margin.** Not
chosen now, and not aspirational. A threshold picked before the work is either
unreachable or vacuous, and this document has already shown what happens when a
coverage figure is chosen for how it reads rather than what it measures.

Enforced in two places that must agree:

- `web/vite.config.ts` — `coverage.thresholds`, so `vitest --coverage` fails
- `cargo llvm-cov --fail-under-lines` in the `test` job

Both run in `scripts/check-all.sh` and in CI, so the local gate and the forge
gate cannot disagree — the property `docs/ci.md` already asserts for everything
else.

## 9. Scope, stated plainly

**This is a large branch, and it was chosen as one deliberately.** A
decomposition into four branches was proposed — infrastructure and today's
numbers first, then Rust CLI coverage, then components, then `App.tsx` — and
declined in favour of a single branch. The trade-off, recorded so nobody has to
reconstruct it: nothing is gated until all of it lands, and `App.tsx` is the
long pole.

The work: roughly 2,100 missed Rust regions and roughly 6,200 lines of
untested component code, including a 3,025-line function to decompose.

## 10. Verification

1. `./scripts/check-all.sh` green, with both thresholds enforced.
2. **A deliberate regression must fail the gate.** Delete or skip a covered
   test on each side, confirm the threshold catches it, restore. The lockfile
   check on the pnpm branch was proven this way, and an unproven gate is
   indistinguishable from no gate.
3. The exclusion list must be shown to exclude *only* what it names — the
   post-exclusion file list checked against the table in §3.
4. CI green, and the `test` job's real duration recorded against §4's guess.

## Carried on this branch

`web/vite.config.ts:10` still describes `taiyaku-wasm` as a `file:` dependency;
it became `link:` on the pnpm branch. Every sweep there missed it because the
verification greps targeted `.md`, `.sh` and `.yml`, and this is a `.ts` — the
same reason `docs/notes.md` was missed. Fixed here as a labelled drive-by.
