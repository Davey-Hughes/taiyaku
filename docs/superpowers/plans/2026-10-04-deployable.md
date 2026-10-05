# Day-to-day deployment, Part B — deployable: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `taiyaku serve --web DIR` serves the built web app beside the API, the
repository builds a container image, CI pushes it to the forge's registry on
every merge to `main`, and the NAS runs it behind Nginx Proxy Manager, with WUD
redeploying each new `:latest`.

**Architecture:** A new `crates/taiyaku-serve/src/web.rs` answers every path no
route matched, using `tower-http`'s `ServeDir`, with two exceptions: a path
under `/api` or `/opds` keeps today's plain 404, and a missing file under
`/assets/` is a 404, never `index.html`. `routes::build_router_serving` sets that
handler as the router's fallback after the guarded and open routes merge and
before the request-log layer. `build_router` keeps its signature for the tests.
`serve()`'s six non-library positional arguments, plus the new `web`, move into
a `ServeOptions` struct. The CLI gains `--web DIR` (refused at startup if `DIR`
holds no `index.html`), and `--trusted-proxy` gains an environment fallback,
`TAIYAKU_TRUSTED_PROXY`. A four-stage `Dockerfile`, a `deploy/probe.sh` health
probe and a `docker` CI job follow. The image is tried on the NAS after merge,
from the registry (amended 2026-10-04; see Task 6).

**Tech Stack:** Rust 2024, axum 0.8.9, tower 0.5.3, tower-http 0.7.1 (`fs`
feature only), Docker 29 with buildx (locally 29.8.2 / buildx 0.37.1; NAS
daemon 29.5.3 / API 1.54), Forgejo Actions on the NAS's `forgejo-runner`,
Unraid Compose Manager, WUD 8.3.1, Nginx Proxy Manager (jlesage image).

## Spec corrections needed

Readings taken while planning contradict or sharpen §3 of
`docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`. The plan
follows the readings. Each one needs the spec edited, or the user's decision
where marked.

1. **§3.1: "An unknown path under `/api` or `/opds` keeps its JSON 404."**
   There is no JSON 404 today. An unmatched path gets axum's default
   fallback: status 404, empty body.
   `auth/tests.rs::an_unmatched_path_is_still_a_404` asserts only the status.
   The plan keeps that exact answer (404, empty body) and does not invent a
   JSON one.
2. **§3.2: the read-write library mount does not, by itself, let the
   container write into the library.** Measured on the NAS:
   - `/mnt/user/media 1/Manga` itself is `root:root 755`.
   - 73 of its 580 directories cannot be written by uid 99, the container's
     user. Fifty-seven of the 206 directories within two levels below the
     root are mode 555.
   - All 67,925 files are world-readable, so reading is fine.

   On a root with `keeps_documents` on (off by default), uid 99 therefore
   cannot create `<root>/.taiyaku/`, and metadata edits report
   `persisted: false` (O82). It cannot write the watch probe's file,
   so the probe fails and the root takes the 15-minute interval; that
   matters to Part C. And ComicInfo write-back fails in the unwritable
   directories. **Decided 2026-10-04: the permissions stay.** The user loosens
   them only to flush writes, and spec Part E (pending library writes)
   queues the writes until then.
3. **§3.3: the runner's `--cpus 4` does not cap the image build.** A buildx
   `docker-container` builder runs BuildKit as its own container on the
   host daemon, outside the job container's limits. The plan caps the
   builder with `--driver-opt cpu-quota=400000 --driver-opt cpu-period=100000`.
   Task 4 checks that the options are accepted.
4. **§3.4: adding `proxy_set_header X-Forwarded-Host $host;` in NPM's Advanced
   tab would be silently ignored.**
   - NPM's generated `location /` sets its own `proxy_set_header` lines: its
     `proxy.conf` sets `Host`, `X-Forwarded-Scheme`, `X-Forwarded-Proto`,
     `X-Forwarded-For` and `X-Real-IP`, never `X-Forwarded-Host`.
   - nginx does not inherit server-level `proxy_set_header` into a location
     that sets any of its own, and NPM places Advanced config at server
     level.
   - The Advanced config must therefore carry a whole `location / { … }`.
     NPM then drops its own (`internal/nginx.js`,
     `advancedConfigHasDefaultLocation`).
   - **It must use `$http_host`, not `$host`.** NPM listens on host ports 180
     and 1443. A browser at `https://name:1443` sends
     `Origin: https://name:1443`, and `$host` drops the port, so taiyaku's
     Origin check would 403 every login. `$http_host` keeps it.
   - **"Cache Assets" and "Block Common Exploits" must be off.**
     `assets.conf` caches `.js`/`.css`/`.woff2` and similar in a public
     proxy cache while ignoring `Cache-Control` and `Set-Cookie`.
     `block-exploits.conf` answers 403 to any query parameter whose value
     starts with `/`.
5. **§3.2: "Appdata Backup's default stops the container before copying."**
   Not verified from here (the plugin's settings were not read). **Decided
   2026-10-04:** taiyaku stays on the stop list and the cache directory is
   excluded; the user sets both, and Task 6 asks them to confirm.
6. **The deployment shape follows the user's existing stacks**, which the spec
   left open. `redextape`, `ws-sim` and `davehugheslegacy` are Unraid
   Compose Manager stacks pulling `forge.daveynet.xyz/davey/<name>:latest`.
   Each carries the labels `wud.watch=true`, `wud.watch.digest=true` and
   `wud.trigger.include=docker.redeploy`, and WUD runs with
   `WATCHBYDEFAULT=false` on a 5-minute cron. taiyaku does the same, on host
   port 55006: the next free one in the 550xx run those stacks use.

## Global Constraints

- **Branch:** `deployable`, cut from the head of `deployment-spec` (the
  commit that adds this plan). If the spec's own PR merges first, move the
  branch with `git rebase --onto origin/main deployment-spec deployable`.
- **Part A (live works) may run as a parallel lane.** It changes
  `ServeState.works` and the handlers that read it; this plan changes
  `serve()`'s signature in `lib.rs` and adds `build_router_serving` in
  `routes.rs`. Whichever lands second rebases. Expect a conflict in
  `serve()`'s parameter list only. A parallel lane runs in its own harness
  worktree cut from the same base. Copy `crates/taiyaku-wasm/pkg` into it,
  and never `cd` into another checkout.
- **The spec is §3 of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`.**
  Where this plan and §3 disagree beyond "Spec corrections needed" above,
  stop and ask; do not pick one.
- **No attribution lines anywhere**: not in commit messages, code comments,
  docs, the compose file or the PR body.
- **Every command runs from the checkout's root.** This shell's `cd` is zoxide
  and has landed in another repository. Use `pnpm -C web …` and absolute
  paths. The Bash tool runs zsh with `noclobber`: use `>|` or a new file
  name. Quote globs (`--include='*.rs'`).
- **Scratch goes in the session scratchpad**, written below as `$S`. Before a
  step that uses it, run `S=<the scratchpad path from the system prompt>/deployable`
  and `mkdir -p "$S"`. Never write into `$HOME`, `/tmp` or the working tree.
  Recursive deletes write variables as `"${VAR:?}"`.
- **Rust tests run throttled**, as `cargo test … -- --test-threads=8` or
  `NEXTEST_TEST_THREADS=8`. An unthrottled run has been killed for memory
  here. Never run a gate with `TAIYAKU_CORPUS` set: use
  `env -u TAIYAKU_CORPUS`.
- **Every new test is watched failing** with its defect put back by hand:
  1. copy the production file to `$S/<name>.good`;
  2. apply the named defect;
  3. run the test and read the failure;
  4. copy the file back;
  5. run the test green again.

  The failure's first line goes in the task's report. Never use `git stash`
  or `git checkout` for this, because they would take uncommitted work with
  them.
- **A task is done when its crate's whole suite passes**, not just the files
  it touched:
  - `cargo test -p taiyaku-serve -- --test-threads=8` for Task 1;
  - both `taiyaku-serve` and `taiyaku-cli` for Task 2;
  - plus `cargo clippy --workspace --all-targets --all-features -- -D warnings`
    and `cargo fmt --all -- --check` before every commit.

  A failure counts as "pre-existing" only if it fails the same way on the
  branch's base, checked in a worktree.
- **No figure in a code comment that a later change would have to correct.**
  No counts of routes, tests or construction sites.
- **Every outward step needs the user's go-ahead first.** That means anything
  on the NAS (`ssh`, `docker` there, NPM, Compose Manager), any push, any
  forge write, any secret. Such steps are marked **[go-ahead]**. Read-only
  inspection over `ssh -o BatchMode=yes root@192.168.1.69` needs none. Kill
  local processes by PID or port, never by pattern.
- **A gate's verdict is the literal `all green` line in its log**, never an
  exit code reported through `; echo`.

---

## Decided while planning

Readings from the tree at `a1a1cb6a` and from the NAS on 2026-10-04. Each is a
fact a task depends on.

1. **Serve flags** are parsed in `crates/taiyaku-cli/src/main.rs::serve`.
   - Valued flags go through `option_value(args, flag, expects)`, which
     refuses a following `--…` token.
   - Bare flags go through `flag_present`.
   - Unknown flags are refused by `reject_unknown_flags("serve", args, &[…])`.
     Its alphabetical list is pinned verbatim by
     `tests.rs::serve_refuses_an_unknown_flag`.
   - `--trusted-proxy` is read repeatedly in `serve_auth`. `TAIYAKU_STUMP_URL`
     and `TAIYAKU_STUMP_KEY` set the precedent: environment as the documented
     way, flag as the override.
   - `serve_auth` is called directly by nine test sites in
     `crates/taiyaku-cli/src/tests.rs`.
2. **`taiyaku_serve::serve` takes eight positional arguments** behind
   `#[expect(clippy::too_many_arguments)]`. Its doc says the next option
   "should join `AuthConfig` or start a second struct beside it". A struct
   holding only `web` would still leave nine positional arguments, so
   `ServeOptions` takes all seven non-library options. The callers are
   `main.rs` and two tests in `crates/taiyaku-serve/src/tests.rs`.
3. **`ServeState` is constructed by hand in dozens of test sites** across
   the crate. Adding a field would touch all of them, and Part A touches the
   same sites, so `web` reaches the router as a parameter of
   `build_router_serving`, not as state.
4. **`build_router`'s layering**:
   - `guarded` gets `.route_layer(guard)` (so unmatched paths stay 404,
     not 401) and then `.layer(per_reader_by_default)`.
   - `open_routes` merge after.
   - The request-log middleware wraps the merged router.
   - A fallback set on the merged router after the merge, and before the log
     layer, is outside the guard and outside `per_reader_by_default`, and
     inside the log. That is exactly where the app belongs.
5. **`tower` is a dev-dependency only** (`crates/taiyaku-serve/Cargo.toml`).
   `web.rs` calls `ServiceExt::oneshot` in production, so `tower` moves to
   `[dependencies]`. axum already depends on it, so no new crate enters the
   tree.
6. **`tower-http` 0.7.1** is the current release (`cargo info tower-http`). It
   depends on `http` 1, `tower` 0.5 and `http-body` 1, which match axum
   0.8.9.
   - `ServeDir<F = DefaultServeDirFallback, B = TokioBackend>`.
   - `ServeDir::new(dir).fallback(ServeFile::new(index))`.
   - The service's `Error = Infallible` and
     `Response = Response<ResponseBody>`, the same type with or without a
     fallback.
   - **A path ServeDir refuses as unsafe (`..`, a prefix, a root) is handed
     to the fallback, not answered 404**
     (`serve_dir/mod.rs`, `ResponseFuture::invalid_path`). So the traversal
     test asserts the secret is never in the body, not a status code.
7. **A built `web/dist`** holds `index.html`, `label.html` and `assets/`.
   Vite content-hashes everything under `assets/`, including
   `taiyaku_wasm_bg-*.wasm`, `theme-*.css` and the `.woff2` fonts. There is
   no `public/` directory. The build needs only `web/` and
   `crates/taiyaku-wasm/pkg` (`vite.config.ts`'s `wasmPkgDir`). `tsc -b`
   needs the package's `.d.ts`. CI's `web` job runs `pnpm install` without
   the package present, so install does not need it.
8. **The binary links the system `libsqlite3.so.0`, `libgcc_s`, `libm` and
   `libc`** (`ldd target/release/taiyaku`). `ureq` uses rustls with bundled
   `webpki-roots`, so the runtime image needs `libsqlite3-0` and no
   `ca-certificates`. `rust:1-bookworm` already carries `libsqlite3-dev`,
   because CI's `test` job builds on it without installing it.
9. **A cold `cargo build --release -p taiyaku-cli -j 4`** (thin LTO,
   `codegen-units = 1`) compiled 234 crates in **103 s** on this
   workstation (Ryzen 9 9950X3D), in a fresh target directory (the planning
   session's reading; no log was kept). The NAS is an 8-core i7-9700K, and
   slower per core. Nothing was timed there.
10. **CI facts**:
    - Every existing job sets `container:` and clones with `git` (no
      actions), because `rust:1-bookworm` has no node.
    - The runner (`/mnt/user/appdata/forgejo/runner/config.yml`) has labels
      `docker` and `unraid`, `docker_host: "automount"`, `options: "--cpus 4"`
      and capacity 2.
    - Redextape's `docker` job uses no `container:` and gets the host's
      Docker socket.
    - Branch protection on `main` requires exactly
      `CI / lint|test|wasm|web (pull_request)`, so a push-only job is never
      a required context.
    - The `taiyaku` repository has no Actions secrets. `redextape` has
      `REGISTRY_USER` and `REGISTRY_TOKEN`.
11. **CLI pins**, both checksummed here:
    - Docker static CLI 29.5.3: sha256
      `34eea64e9c3435f5af1b760827a56a561cd67fc2d6e9cd1813b8bb1e3ff7930b`,
      matching the NAS daemon.
    - buildx v0.34.0 `linux-amd64`: sha256
      `0144479d5a1cd710be3464ae898628cfa68033e16b225aef52f81930c45ac9b5`,
      from its `checksums.txt`.
    - wasm-pack v0.15.0, with its checksum, as in CI's `wasm` job.
12. **Paths inside the container**:
    - `XDG_DATA_HOME=/config` puts the database at
      `/config/taiyaku/library.db` (`library.rs::db_path`) and backups at
      `/config/taiyaku/backups`.
    - `XDG_CACHE_HOME=/cache` puts covers and tiles under `/cache/taiyaku/`.
    - `library user promote` reads the password with `stty -echo` plus
      stdin, so `docker exec -it` works, and so does piping into
      `docker exec -i` for a scratch trial.
    - Login is `POST /api/session` with JSON `{"name","password"}`. It needs
      an `Origin` whose authority equals `Host`, and answers `204` with
      `Set-Cookie`.
    - A page is `GET /api/works/{work}/{edition}/pages/{index}`.
13. **The NAS**:
    - NPM is on Docker's default `bridge` at 172.17.0.16. It proxies to
      `$server "192.168.1.69"` plus the published port.
    - NPM holds a wildcard certificate for `*.daveynet.xyz` (`npm-41`).
    - `taiyaku.daveynet.xyz` does not resolve publicly, so a name under that
      domain is LAN-only unless someone adds it.
    - Compose Manager projects live under
      `/boot/config/plugins/compose.manager/projects/<name>/`.
    - `appdata` is `shareUseCache="prefer"` on `nvme_cache`.
    - The first scan of the real library hashes every member once
      (`derive.rs`'s module doc), so it reads the whole share.

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-serve/src/web.rs` (new) | The built app as a fallback: path rules, cache headers, `Web` |
| `crates/taiyaku-serve/src/web/tests.rs` (new) | Router-level tests of the app beside the API |
| `crates/taiyaku-serve/src/routes.rs` | `build_router` delegates to the new `build_router_serving(state, web)` |
| `crates/taiyaku-serve/src/lib.rs` | `mod web;`, `ServeOptions`, `serve(library, works, options)` |
| `crates/taiyaku-serve/src/tests.rs` | The two `serve()` bind tests take `ServeOptions` |
| `crates/taiyaku-serve/Cargo.toml` | `tower` to `[dependencies]`; `tower-http` (`fs`) |
| `crates/taiyaku-cli/src/main.rs` | `--web`, `web_root`, `TAIYAKU_TRUSTED_PROXY`, usage line, `ServeOptions` call |
| `crates/taiyaku-cli/src/tests.rs` | Flag list, `web_root`, environment fallback; `serve_auth(…, None)` at old sites |
| `Dockerfile` (new) | wasm → web → server → runtime |
| `.dockerignore` (new) | Keeps the 28 GB corpus, `target/` and `node_modules` out of the build context |
| `deploy/probe.sh` (new) | The image's health check and CI's probe: one GET over IPv4 loopback, status compared |
| `.forgejo/workflows/ci.yml` | The `docker` job |
| `README.md` | "Running it on a server" |
| `docs/ci.md` | The `docker` job, its secrets, the runner exception |
| `docs/status.md` | Synopsis line, dependency sentence, a paragraph, the test count twice |
| `docs/notes.md` | "Deployable (2026-10-04)": the measurements behind this plan and Task 6's |

---

### Task 1: The app beside the API

**Files:**
- Create: `crates/taiyaku-serve/src/web.rs`
- Create: `crates/taiyaku-serve/src/web/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (`build_router`, around line 2835)
- Modify: `crates/taiyaku-serve/src/lib.rs` (the `mod` list at the top)
- Modify: `crates/taiyaku-serve/Cargo.toml`

**Interfaces:**
- Consumes: `crate::test_support::{Fixture, state}`, `crate::reqlog::Log::new(&Path) -> Result<Log, String>`
- Produces:
  - `pub(crate) fn build_router_serving(state: Arc<ServeState>, web: Option<&std::path::Path>) -> Router` in `routes.rs`;
  - `pub(crate) mod web` with `Web::new(&Path)`, `Web::answer(self, Request) -> Response`, `is_api(&str) -> bool`, and the constants `HASHED`, `REVALIDATE`.

  Task 2 calls `build_router_serving`.

- [ ] **Step 1: Dependencies**

In `crates/taiyaku-serve/Cargo.toml`, under `[dependencies]`, insert these two
lines after `tokio = …` and before `ureq = …`:

```toml
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.7", features = ["fs"] }
```

Delete `tower = { version = "0.5", features = ["util"] }` from
`[dev-dependencies]`. A crate in both tables is legal, but the dev entry would
then say nothing.

Run: `cargo tree -p taiyaku-serve -i tower-http --depth 0`
Expected: `tower-http v0.7.1`.

- [ ] **Step 2: Write the failing tests**

Create `crates/taiyaku-serve/src/web/tests.rs`:

```rust
//! The built app beside the API — spec `2026-10-04-day-to-day-deployment-design.md`
//! §3.1 and §3.5.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt as _;

use crate::routes::build_router_serving;
use crate::test_support::{Fixture, state};
use crate::web::{HASHED, REVALIDATE};

const INDEX: &str = "<!doctype html><title>the app</title>";
const SECRET: &str = "a file beside the app, never inside it";

/// A built app laid out the way Vite lays one out — `index.html` and
/// `label.html` at the top, content-hashed files under `assets/` — plus one
/// file *beside* the app directory that no request may ever reach.
fn built_app(tmp: &Fixture) -> PathBuf {
    let dist = tmp.join("dist");
    std::fs::create_dir_all(dist.join("assets")).unwrap();
    std::fs::write(dist.join("index.html"), INDEX).unwrap();
    std::fs::write(dist.join("label.html"), "<!doctype html><title>labeller</title>").unwrap();
    std::fs::write(dist.join("assets/main-Ab12Cd34.js"), "console.log('app')").unwrap();
    std::fs::write(
        dist.join("assets/taiyaku_wasm_bg-Ef56Gh78.wasm"),
        b"\0asm\x01\0\0\0",
    )
    .unwrap();
    std::fs::write(tmp.join("secret.txt"), SECRET).unwrap();
    dist
}

/// One anonymous `GET` through the whole router, app included.
async fn get(tmp: &Fixture, dist: &Path, path: &str) -> (StatusCode, HeaderMap, String) {
    let router = build_router_serving(Arc::new(state(tmp)), Some(dist));
    let response = router
        .oneshot(Request::get(path).body(Body::empty()).unwrap())
        .await
        .expect("answers");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("reads the body");
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

/// A reload on any of the app's own routes gets `index.html`, and the app's
/// router takes it from there.
#[tokio::test]
async fn a_deep_link_answers_the_app() {
    let tmp = Fixture::new("web-deep-link");
    let dist = built_app(&tmp);
    for path in ["/", "/read/some-work", "/series/3", "/config/users"] {
        let (status, headers, body) = get(&tmp, &dist, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert_eq!(body, INDEX, "{path}");
        let kind = headers[header::CONTENT_TYPE].to_str().unwrap();
        assert!(kind.starts_with("text/html"), "{path}: {kind}");
    }
}

/// A path under `/api` or `/opds` that no route matched is the same plain 404
/// it was before the app existed — never the app, or a client asking for a
/// route that does not exist would be handed HTML with a 200. A path that only
/// shares the letters is the app's.
#[tokio::test]
async fn the_api_keeps_its_own_404() {
    let tmp = Fixture::new("web-api-404");
    let dist = built_app(&tmp);
    for path in ["/api/nothing-here", "/api", "/api/", "/opds/nothing-here", "/opds"] {
        let (status, _, body) = get(&tmp, &dist, path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_ne!(body, INDEX, "{path} answered the app");
    }
    let (status, _, body) = get(&tmp, &dist, "/apiary").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, INDEX);
}

/// No spelling of a path leaves the app's directory. `ServeDir` hands a path it
/// refuses to the fallback rather than answering 404, so what is asserted is
/// the body, not the status.
#[tokio::test]
async fn a_path_outside_the_app_is_never_served() {
    let tmp = Fixture::new("web-traversal");
    let dist = built_app(&tmp);
    for path in [
        "/../secret.txt",
        "/%2e%2e/secret.txt",
        "/..%2fsecret.txt",
        "/assets/../../secret.txt",
        "/assets/%2e%2e/%2e%2e/secret.txt",
    ] {
        let (_, _, body) = get(&tmp, &dist, path).await;
        assert!(!body.contains(SECRET), "{path} reached a file outside the app");
    }
}

/// The sign-in screen is part of the app and has to load before there is any
/// credential, so the app asks for none — and the API behind it still does.
#[tokio::test]
async fn the_app_needs_no_credential_and_the_api_still_does() {
    let tmp = Fixture::new("web-anonymous");
    let dist = built_app(&tmp);
    for path in ["/", "/label.html", "/assets/main-Ab12Cd34.js"] {
        let (status, _, _) = get(&tmp, &dist, path).await;
        assert_eq!(status, StatusCode::OK, "{path}");
    }
    let (status, _, _) = get(&tmp, &dist, "/api/me").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// A content-hashed name never changes meaning, so it may be kept a year;
/// everything else — `index.html` above all — is asked for again, which is how
/// a phone picks up a new deployment. Neither is the guarded routes'
/// `private, no-store`.
#[tokio::test]
async fn hashed_assets_are_immutable_and_the_rest_revalidates() {
    let tmp = Fixture::new("web-cache");
    let dist = built_app(&tmp);
    let (status, headers, _) = get(&tmp, &dist, "/assets/main-Ab12Cd34.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CACHE_CONTROL], HASHED);
    for path in ["/", "/label.html", "/read/some-work"] {
        let (_, headers, _) = get(&tmp, &dist, path).await;
        assert_eq!(headers[header::CACHE_CONTROL], REVALIDATE, "{path}");
    }
}

/// A hashed name that does not exist is a 404 with no caching instruction.
/// Answering it with `index.html` would hand a browser HTML under a name it may
/// keep for a year.
#[tokio::test]
async fn a_missing_asset_is_a_404_and_is_not_cached() {
    let tmp = Fixture::new("web-missing-asset");
    let dist = built_app(&tmp);
    let (status, headers, body) = get(&tmp, &dist, "/assets/gone-Zz99Yy88.js").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_ne!(body, INDEX);
    assert!(!headers.contains_key(header::CACHE_CONTROL));
}

/// `WebAssembly.instantiateStreaming` refuses a module served as anything but
/// `application/wasm`, and the reader's alignment code is that module.
#[tokio::test]
async fn wasm_is_served_as_application_wasm() {
    let tmp = Fixture::new("web-wasm");
    let dist = built_app(&tmp);
    let (status, headers, _) = get(&tmp, &dist, "/assets/taiyaku_wasm_bg-Ef56Gh78.wasm").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/wasm");
}

/// `--log-requests` sees what the app served, not only the API.
#[tokio::test]
async fn the_request_log_records_what_the_app_served() {
    let tmp = Fixture::new("web-log");
    let dist = built_app(&tmp);
    let log_path = tmp.join("requests.jsonl");
    let mut served = state(&tmp);
    served.log = Some(Arc::new(
        crate::reqlog::Log::new(&log_path).expect("opens the log"),
    ));
    let response = build_router_serving(Arc::new(served), Some(&dist))
        .oneshot(Request::get("/read/some-work").body(Body::empty()).unwrap())
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
    let logged = std::fs::read_to_string(&log_path).expect("the log was written");
    assert!(logged.contains("\"path\":\"/read/some-work\""), "{logged}");
}
```

Register the module: in `crates/taiyaku-serve/src/lib.rs`, in the `mod` list,
add `mod web;` after `mod users_api;` (before `mod works;`). Create
`crates/taiyaku-serve/src/web.rs` holding only:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 3: Run the tests to see them fail to compile**

Run: `cargo test -p taiyaku-serve web:: -- --test-threads=8`
Expected: compile errors, including `cannot find function build_router_serving
in module crate::routes` and `unresolved imports crate::web::HASHED,
crate::web::REVALIDATE`.

- [ ] **Step 4: Write `web.rs`**

Replace `crates/taiyaku-serve/src/web.rs` with:

```rust
//! The built web app, served beside the API when `taiyaku serve --web DIR` is
//! given — spec `2026-10-04-day-to-day-deployment-design.md` §3.1.
//!
//! **Every path no route matched is the app's**, so a reload on a deep link
//! such as `/read/some-work` gets `index.html`, and the app's own router takes
//! it from there. Two exceptions keep the API's answers its own. A path under
//! `/api` or `/opds` that no route matched is still the plain 404 it always
//! was, never the app. And a missing file under `/assets/` is a 404 too:
//! Vite names those files by content hash, and a browser may keep whatever such
//! a name answers for a year.
//!
//! **No credential is asked for here.** The sign-in screen is part of the app
//! and has to load before there is anything to sign in with. Everything the
//! app then asks for goes through `/api`, behind the guard as before.
//!
//! `tower-http`'s `ServeDir` does the serving — content types, ranges,
//! `If-Modified-Since`, and refusing a path that would leave the directory — so
//! none of that is written here.

use std::path::Path;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt as _;
use tower_http::services::{ServeDir, ServeFile};

/// `Cache-Control` for a file under `/assets/`, whose name carries its content
/// hash and so never changes meaning.
pub(crate) const HASHED: &str = "public, max-age=31536000, immutable";

/// `Cache-Control` for everything else the app serves, `index.html` above all:
/// a phone has to ask again to pick up a new deployment.
pub(crate) const REVALIDATE: &str = "no-cache";

/// Whether `path` belongs to the API, whose unmatched paths stay 404.
///
/// A segment boundary rather than a bare prefix, so `/apiary` is the app's.
pub(crate) fn is_api(path: &str) -> bool {
    ["/api", "/opds"].iter().any(|prefix| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// The two ways a path reaches the built app's directory.
#[derive(Clone, Debug)]
pub(crate) struct Web {
    /// Everything but `/assets/`: the file if there is one, else `index.html`.
    app: ServeDir<ServeFile>,
    /// `/assets/`: the file if there is one, else a 404 — never `index.html`.
    assets: ServeDir,
}

impl Web {
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            app: ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))),
            assets: ServeDir::new(dir),
        }
    }

    /// Answer one request that no API route matched.
    pub(crate) async fn answer(self, request: Request) -> Response {
        let path = request.uri().path();
        if is_api(path) {
            return StatusCode::NOT_FOUND.into_response();
        }
        let hashed = path.starts_with("/assets/");
        let served = if hashed {
            self.assets.oneshot(request).await
        } else {
            self.app.oneshot(request).await
        };
        let mut response = match served {
            Ok(response) => response.map(Body::new),
            Err(never) => match never {},
        };
        let status = response.status();
        if status.is_success() || status == StatusCode::NOT_MODIFIED {
            let policy = if hashed { HASHED } else { REVALIDATE };
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static(policy));
        }
        response
    }
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 5: Wire it into the router**

In `crates/taiyaku-serve/src/routes.rs`, replace:

```rust
pub fn build_router(state: Arc<ServeState>) -> Router {
    let log = state.log.clone();
```

with:

```rust
pub fn build_router(state: Arc<ServeState>) -> Router {
    build_router_serving(state, None)
}

/// [`build_router`], plus — given `web` — the built app in that directory,
/// answering every path no route matched (see [`crate::web`]).
///
/// **A parameter rather than a field on [`ServeState`]**: where the app lives
/// is fixed for the life of the process and read once, here, while the state
/// is built by hand across this crate's tests, none of which serve an app.
pub(crate) fn build_router_serving(
    state: Arc<ServeState>,
    web: Option<&std::path::Path>,
) -> Router {
    let log = state.log.clone();
```

and, at the end of the same function, replace:

```rust
    guarded
        .merge(open)
        .layer(axum::middleware::from_fn(
```

with:

```rust
    let mut app = guarded.merge(open);
    // **After the merge, before the log.** A fallback set on `guarded` would
    // sit inside `per_reader_by_default` and give the app's files `private,
    // no-store`; one set after the log layer below would be the only thing
    // `--log-requests` could not see.
    if let Some(dir) = web {
        let web = crate::web::Web::new(dir);
        app = app.fallback(move |request: axum::extract::Request| web.clone().answer(request));
    }

    app.layer(axum::middleware::from_fn(
```

Leave the rest of the chain (`move |request …` through `.with_state(state)`)
as it is, then run `cargo fmt --all`.

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test -p taiyaku-serve web:: -- --test-threads=8`
Expected: `test result: ok. 8 passed`.

- [ ] **Step 7: Watch each test fail against its defect**

Copy `crates/taiyaku-serve/src/web.rs` and `routes.rs` to `$S` first. Apply
each defect alone, run
`cargo test -p taiyaku-serve web:: -- --test-threads=8`, read the failure, then
copy the file back before the next one:

| Defect (one at a time) | Test that must fail |
| --- | --- |
| `web.rs`: `app: ServeDir::new(dir).fallback(…)` → `app: ServeDir::new(dir).fallback(ServeFile::new(dir.join("missing.html")))` | `a_deep_link_answers_the_app` |
| `web.rs`: `is_api`'s body → `false` | `the_api_keeps_its_own_404` |
| `web.rs`: `is_api`'s closure → `path.starts_with(prefix)` | `the_api_keeps_its_own_404` (`/apiary`) |
| `web.rs`: add a field `root: std::path::PathBuf` set to `dir.to_path_buf()` in `new`, and as the first line of `answer` insert `if let Ok(text) = std::fs::read_to_string(self.root.join(request.uri().path().trim_start_matches('/'))) { return text.into_response(); }` (a hand-rolled handler that joins the raw path) | `a_path_outside_the_app_is_never_served` |
| `routes.rs`: after the `if let Some(dir) = web { … }` block insert `let app = app.layer(axum::middleware::from_fn_with_state(state.clone(), crate::auth::guard));` | `the_app_needs_no_credential_and_the_api_still_does` |
| `web.rs`: `if hashed { HASHED } else { REVALIDATE }` → `if hashed { REVALIDATE } else { HASHED }` | `hashed_assets_are_immutable_and_the_rest_revalidates` |
| `web.rs`: `let served = if hashed {` → `let served = if false {` | `a_missing_asset_is_a_404_and_is_not_cached` |
| `web.rs`: before `response` at the end of `answer`, insert `if hashed && response.headers()[header::CONTENT_TYPE] == "application/wasm" { response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")); }` | `wasm_is_served_as_application_wasm` |
| `routes.rs`: move the `if let Some(dir) = web { … }` block to after the log `.layer(…)` call (bind the layered router to `app` first) | `the_request_log_records_what_the_app_served` |

Record each failure's first line in the task report. Then run the step's
command once more with both files restored.
Expected: `8 passed`.

- [ ] **Step 8: Whole crate, lint, format**

Run:
```bash
cargo test -p taiyaku-serve -- --test-threads=8
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --workspace --no-deps
```
Expected: every test passes (including
`auth::tests::an_unmatched_path_is_still_a_404`, unchanged), and the other
three exit 0.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-serve/Cargo.toml Cargo.lock crates/taiyaku-serve/src/web.rs crates/taiyaku-serve/src/web/tests.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/lib.rs
git commit -m "serve: the built web app beside the API, as the router's fallback"
```

---

### Task 2: `ServeOptions`, `--web`, and `TAIYAKU_TRUSTED_PROXY`

**Files:**
- Modify: `crates/taiyaku-serve/src/lib.rs` (`serve`, its doc comment, a new `ServeOptions`)
- Modify: `crates/taiyaku-serve/src/tests.rs` (the two `crate::serve(` calls)
- Modify: `crates/taiyaku-cli/src/main.rs` (`print_usage`, `serve`, `serve_auth`, new `web_root`)
- Modify: `crates/taiyaku-cli/src/tests.rs`

**Interfaces:**
- Consumes: `build_router_serving(state, web)` from Task 1.
- Produces:
  - `pub struct taiyaku_serve::ServeOptions { addr: String, stump: Option<StumpConfig>, log_requests: Option<PathBuf>, refresh_stump: bool, cache: Option<PathBuf>, auth: AuthConfig, web: Option<PathBuf> }`;
  - `pub async fn serve(library: Arc<Library>, works: Vec<Work>, options: ServeOptions) -> Result<(), String>`;
  - CLI `fn web_root(dir: Option<&str>) -> Result<Option<PathBuf>, String>` and `fn serve_auth(args: &[String], from_environment: Option<&str>) -> Result<AuthConfig, String>`.

- [ ] **Step 1: Write the failing CLI tests**

In `crates/taiyaku-cli/src/tests.rs`:

(a) Every existing call `super::serve_auth(&argv(&[…]))` gains `, None`. The
multi-line calls close with `]))` on a later line, so use perl, not sed:

```bash
perl -0pi -e 's/super::serve_auth\((&argv\(&\[.*?\]\))\)/super::serve_auth($1, None)/gs' crates/taiyaku-cli/src/tests.rs
grep -c 'super::serve_auth(' crates/taiyaku-cli/src/tests.rs
```

The count is the number of sites (nine at `a1a1cb6a`). Step 4's compile is
the check that every one gained the argument.

(b) In `serve_refuses_an_unknown_flag`, the expected message becomes:

```rust
        "serve has no flag `--bogus`; its flags are --addr, --allow-web-setup, --cache, \
             --db, --insecure-cookies, --log-requests, --refresh-stump, \
             --secure-cookies, --stump-key, --stump-url, --trusted-proxy, --web"
```

(c) Append these tests after `a_malformed_trusted_proxy_is_refused_by_name`:

```rust
/// A container's command is fixed by its image, so a deployment names its
/// proxy in `TAIYAKU_TRUSTED_PROXY`: comma- or space-separated, and implying
/// `Secure` exactly as the flag does.
#[test]
fn trusted_proxy_falls_back_to_the_environment() {
    let auth = super::serve_auth(&argv(&[]), Some("172.17.0.0/16, 192.168.1.69"))
        .expect("parses");
    assert_eq!(auth.trusted_proxies.len(), 2);
    assert!(auth.trusted_proxies[0].contains("172.17.0.5".parse().unwrap()));
    assert!(auth.trusted_proxies[1].contains("192.168.1.69".parse().unwrap()));
    assert!(auth.secure_cookies);
}

/// Any `--trusted-proxy` on the command line replaces the environment's list
/// rather than adding to it — the precedence `--stump-url` already holds over
/// `TAIYAKU_STUMP_URL`.
#[test]
fn a_trusted_proxy_flag_replaces_the_environment() {
    let auth = super::serve_auth(&argv(&["--trusted-proxy", "10.0.0.1"]), Some("172.17.0.0/16"))
        .expect("parses");
    assert_eq!(auth.trusted_proxies.len(), 1);
    assert!(auth.trusted_proxies[0].contains("10.0.0.1".parse().unwrap()));
    assert!(!auth.trusted_proxies[0].contains("172.17.0.5".parse().unwrap()));
}

/// An empty value, or one of separators alone, trusts nobody — what an Unraid
/// template's blank field hands over.
#[test]
fn an_empty_environment_value_trusts_nobody() {
    for value in ["", " , ,"] {
        let auth = super::serve_auth(&argv(&[]), Some(value)).expect("parses");
        assert!(auth.trusted_proxies.is_empty(), "{value:?}");
        assert!(!auth.secure_cookies, "{value:?}");
    }
}

/// A value that is not a range is refused at startup, naming the variable and
/// the value — the variable, because nothing on the command line was wrong.
#[test]
fn a_malformed_environment_value_is_refused_by_name() {
    let err = super::serve_auth(&argv(&[]), Some("172.17.0.0/16,nonsense")).unwrap_err();
    assert!(err.contains("TAIYAKU_TRUSTED_PROXY"), "{err}");
    assert!(err.contains("nonsense"), "{err}");
}

/// `--web` names a built app, so a directory with no `index.html` is refused
/// before anything binds — naming the directory and what to build.
#[test]
fn web_refuses_a_directory_with_no_index() {
    let scratch = TempDir::new("serve-web-empty");
    let dir = scratch.to_str().unwrap();
    let err = super::web_root(Some(dir)).unwrap_err();
    assert!(err.contains("--web"), "{err}");
    assert!(err.contains(dir), "{err}");
    assert!(err.contains("index.html"), "{err}");
}

/// A directory holding `index.html` is taken as given, and no flag serves no
/// app at all.
#[test]
fn web_accepts_a_built_app_and_is_off_by_default() {
    let scratch = TempDir::new("serve-web-built");
    std::fs::write(scratch.join("index.html"), "<!doctype html>").unwrap();
    assert_eq!(
        super::web_root(Some(scratch.to_str().unwrap())).unwrap(),
        Some(scratch.to_path_buf())
    );
    assert_eq!(super::web_root(None).unwrap(), None);
}

/// `--web` with nothing after it is refused by `option_value`'s own rule.
#[test]
fn serve_refuses_web_with_no_directory() {
    let err = serve(&argv(&["--web"])).unwrap_err();
    assert_eq!(err, "--web needs a directory, and nothing followed it");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p taiyaku-cli -- --test-threads=8 serve web trusted`
Expected: compile errors, including `this function takes 1 argument but 2
arguments were supplied` for `serve_auth` and `cannot find function web_root`.

- [ ] **Step 3: `ServeOptions` and `serve()`**

In `crates/taiyaku-serve/src/lib.rs`, insert directly above the doc comment
that begins `/// Bind and serve until interrupted.`:

```rust
/// How [`serve`] runs, beyond the library and the works it serves.
///
/// **A struct because `serve` reached its limit.** Its eighth positional
/// argument was `AuthConfig`, and its doc said a ninth must not be positional.
/// `--web` was the ninth, and it brought the other six with it rather than
/// starting a struct of one.
#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// `host:port` to listen on.
    pub addr: String,
    /// A Stump instance to read pages from, if one is configured.
    pub stump: Option<StumpConfig>,
    /// Where `--log-requests` writes, if it was given.
    pub log_requests: Option<std::path::PathBuf>,
    /// Throw away every remembered Stump resolution before resolving again.
    pub refresh_stump: bool,
    /// `--cache`: the cache root, overriding `$XDG_CACHE_HOME/taiyaku`.
    pub cache: Option<std::path::PathBuf>,
    /// What this server knows about the deployment in front of it.
    pub auth: crate::session::AuthConfig,
    /// A built web app (`web/dist`) to serve beside the API — see
    /// [`crate::web`]. `None` serves the API alone, which is what `vite dev`'s
    /// proxy expects.
    pub web: Option<std::path::PathBuf>,
}
```

In `serve`'s own doc comment, replace the paragraph that begins
`/// **Eight parameters, which is one past clippy's limit and is the signal to`
and ends `/// nobody can read at the call site.` with:

```rust
/// Everything but the library and the works arrives as [`ServeOptions`]; the
/// next option belongs there too.
```

Delete the `#[expect(clippy::too_many_arguments, …)]` attribute. Replace the
signature:

```rust
pub async fn serve(
    library: Arc<taiyaku_library::Library>,
    works: Vec<Work>,
    addr: &str,
    stump: Option<StumpConfig>,
    log_requests: Option<std::path::PathBuf>,
    refresh_stump: bool,
    cache: Option<std::path::PathBuf>,
    auth: crate::session::AuthConfig,
) -> Result<(), String> {
```

with:

```rust
pub async fn serve(
    library: Arc<taiyaku_library::Library>,
    works: Vec<Work>,
    options: ServeOptions,
) -> Result<(), String> {
    let ServeOptions {
        addr,
        stump,
        log_requests,
        refresh_stump,
        cache,
        auth,
        web,
    } = options;
    let addr = addr.as_str();
```

At the end of `serve`, replace
`build_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),`
with
`crate::routes::build_router_serving(state, web.as_deref())
    .into_make_service_with_connect_info::<std::net::SocketAddr>(),`

In `crates/taiyaku-serve/src/tests.rs`, both `crate::serve(` calls pass the
same values as a struct. In `serve_binds_the_address_and_answers_on_it`:

```rust
            crate::serve(
                crate::test_support::library(),
                works,
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
```

(`addr` is the clone the spawned block already moves in; it was passed as
`&addr`). In the second test, the call that passes `vec![work]`:

```rust
            crate::serve(
                crate::test_support::library(),
                vec![work],
                crate::ServeOptions {
                    addr,
                    cache: Some(cache),
                    ..crate::ServeOptions::default()
                },
            )
            .await
```

- [ ] **Step 4: The CLI**

In `crates/taiyaku-cli/src/main.rs`:

(a) `print_usage`: the serve line ends `[--insecure-cookies] [--allow-web-setup] [--web DIR]"`.

(b) `serve`'s `reject_unknown_flags` list gains `"--web",` after `"--trusted-proxy",`.

(c) Replace `let auth = serve_auth(args)?;` with:

```rust
    let auth = serve_auth(args, std::env::var("TAIYAKU_TRUSTED_PROXY").ok().as_deref())?;
```

(d) After `let refresh_stump = flag_present(args, "--refresh-stump");` insert:

```rust
    // Checked before the library is opened, so a mistyped `--web` is refused
    // for what is actually wrong.
    let web = web_root(option_value(args, "--web", "a directory")?)?;
```

(e) Replace the `.block_on(taiyaku_serve::serve( … ))` call with:

```rust
        .block_on(taiyaku_serve::serve(
            Arc::new(db_library),
            works,
            taiyaku_serve::ServeOptions {
                addr: addr.to_string(),
                stump,
                log_requests: log,
                refresh_stump,
                cache,
                auth,
                web,
            },
        ))
```

(f) Change `serve_auth`. Its signature becomes
`fn serve_auth(args: &[String], from_environment: Option<&str>) -> Result<taiyaku_serve::session::AuthConfig, String> {`.
Add to its doc comment, before `/// # Errors`:

```rust
/// `from_environment` is `TAIYAKU_TRUSTED_PROXY`, read by the caller so that
/// this function's tests never depend on the environment they run in.
```

Add to `# Errors`: `or if \`TAIYAKU_TRUSTED_PROXY\` holds one that is not,
naming the variable.` Then insert after the `for (index, arg) in
args.iter().enumerate() { … }` loop:

```rust
    // **The environment is the container's way in, and a flag still wins.** An
    // image's command is fixed, so a deployment names its proxy in
    // `TAIYAKU_TRUSTED_PROXY`; any `--trusted-proxy` replaces that list rather
    // than adding to it, the same precedence `--stump-url` holds over
    // `TAIYAKU_STUMP_URL`.
    if trusted_proxies.is_empty()
        && let Some(listed) = from_environment
    {
        for value in listed
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|value| !value.is_empty())
        {
            trusted_proxies.push(
                value
                    .parse()
                    .map_err(|why| format!("TAIYAKU_TRUSTED_PROXY {why}"))?,
            );
        }
    }
```

(g) Add after `serve_auth`:

```rust
/// `--web DIR`: a built web app to serve beside the API.
///
/// **Refused at startup when `DIR` holds no `index.html`**, rather than served.
/// Otherwise every page of the app would answer 404 from a server that looks
/// healthy in every other way, and the mistake — naming `web/` rather than
/// `web/dist`, or a tree nobody built — is one a person makes once and should
/// hear about at once.
///
/// # Errors
///
/// When `DIR` has no `index.html` in it, naming the directory and the command
/// that builds one.
fn web_root(dir: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(dir) = dir else {
        return Ok(None);
    };
    let dir = PathBuf::from(dir);
    if !dir.join("index.html").is_file() {
        return Err(format!(
            "--web {}: there is no index.html in it. Build the app with \
             `pnpm -C web run build` and name `web/dist`",
            dir.display()
        ));
    }
    Ok(Some(dir))
}
```

- [ ] **Step 5: Run the tests to see them pass**

Run:
```bash
cargo test -p taiyaku-cli -- --test-threads=8
cargo test -p taiyaku-serve -- --test-threads=8
```
Expected: both pass, including the seven new tests and the two `serve_binds…`
tests.

- [ ] **Step 6: Watch each new test fail against its defect**

Copy `crates/taiyaku-cli/src/main.rs` to `$S/main.rs.good`. Apply each defect
alone, run `cargo test -p taiyaku-cli -- --test-threads=8 <test name>`, read
the failure, then copy the file back:

| Defect | Test that must fail |
| --- | --- |
| `if trusted_proxies.is_empty() && let Some(listed)` → `if let Some(listed)` | `a_trusted_proxy_flag_replaces_the_environment` |
| delete `.filter(\|value\| !value.is_empty())` | `an_empty_environment_value_trusts_nobody` |
| `format!("TAIYAKU_TRUSTED_PROXY {why}")` → `format!("--trusted-proxy {why}")` | `a_malformed_environment_value_is_refused_by_name` |
| call site: `std::env::var("TAIYAKU_TRUSTED_PROXY").ok().as_deref()` → `None` | none of the tests above can see it, because they pass the value in. Drive the binary instead (the block below) |
| `web_root`: delete the `if !dir.join("index.html").is_file() { … }` block | `web_refuses_a_directory_with_no_index` |
| remove `"--web",` from `reject_unknown_flags` | `serve_refuses_web_with_no_directory` and `serve_refuses_an_unknown_flag` |

For the call-site row, with the code correct and then with the defect applied:

```bash
cargo build -p taiyaku-cli
TAIYAKU_TRUSTED_PROXY=nonsense target/debug/taiyaku serve --db "$S/env.db" --addr 127.0.0.1:18486 >| "$S/env-serve.log" 2>&1 &
pid=$!
sleep 3
kill "$pid" 2>/dev/null; cat "$S/env-serve.log"
```

Expected with the code correct: the log holds
`taiyaku: TAIYAKU_TRUSTED_PROXY …nonsense…`, and the process had already
exited, so `kill` finds nothing. With the defect: the log reads
`taiyaku serve listening on http://127.0.0.1:18486`, and `kill` stops it.

Then restore, and run Step 5's commands again.
Expected: all pass.

- [ ] **Step 7: Lint, format, docs**

Run the four commands of Task 1 Step 8, with `-p taiyaku-cli` added to the
test run. Expected: all pass. There is no `#[expect(clippy::too_many_arguments)]`
left on `serve`: run
`grep -n "too_many_arguments" crates/taiyaku-serve/src/lib.rs`, expecting no
output.

- [ ] **Step 8: The real binary serves a real build**

```bash
pnpm -C web install --frozen-lockfile
./scripts/build-wasm.sh
pnpm -C web run build
cargo build --release -p taiyaku-cli
target/release/taiyaku serve --db "$S/serve-web.db" --cache "$S/serve-web-cache" --addr 127.0.0.1:18484 --web web/dist >| "$S/serve-web.log" 2>&1 &
echo $! >| "$S/serve-web.pid"
```

Wait until `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:18484/`
prints `200`, polling with
`until … ; do sleep 0.2; done` and at most 50 attempts. Then:

```bash
curl -s -o /dev/null -w '%{http_code} %{content_type}\n' http://127.0.0.1:18484/read/anything
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:18484/api/me
asset=$(curl -s http://127.0.0.1:18484/ | grep -o '/assets/main-[^"]*\.js' | head -1)
curl -s -D - -o /dev/null "http://127.0.0.1:18484$asset" | grep -i '^cache-control'
target/release/taiyaku serve --addr 127.0.0.1:18485 --db "$S/serve-web2.db" --web docs
kill "$(cat "$S/serve-web.pid")"
```

(`--web docs`, not `--web web`: `web/index.html` is Vite's source entry, so
naming `web/` passes the check.)

Expected, in order:
- `200 text/html`;
- `401`;
- `cache-control: public, max-age=31536000, immutable`;
- the second serve exits 1 with `taiyaku: --web docs: there is no index.html in it…`;
- port 18484 is free afterwards
  (`curl -s -m 2 http://127.0.0.1:18484/` fails).

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/tests.rs crates/taiyaku-cli/src/main.rs crates/taiyaku-cli/src/tests.rs
git commit -m "serve: --web DIR, ServeOptions, and TAIYAKU_TRUSTED_PROXY"
```

---

### Task 3: The image

**Files:**
- Create: `Dockerfile`
- Create: `.dockerignore`
- Create: `deploy/probe.sh` (mode 0755)

**Interfaces:**
- Consumes: `--web` from Task 2.
- Produces:
  - an image whose `ENTRYPOINT` is `taiyaku` and whose `CMD` is
    `serve --addr 0.0.0.0:8484 --web /app/web`;
  - `taiyaku-probe PATH STATUS` on its `PATH`, which exits 0 when the server
    answers `PATH` with `STATUS`;
  - a `HEALTHCHECK` that runs `taiyaku-probe / 200`.

  Task 4 runs `taiyaku-probe` through `docker exec`.

- [ ] **Step 1: `deploy/probe.sh`**

```bash
#!/bin/bash
# One GET to the server in this container, over IPv4 loopback, and whether the
# status it answered is the one expected. The image's HEALTHCHECK, and CI's
# probe of a freshly built image.
#
# `127.0.0.1`, never `localhost`: `serve --addr 0.0.0.0:8484` listens on IPv4
# only, and a resolver that tries `::1` first would report a healthy server as
# down. Bash's `/dev/tcp`, because the runtime image carries no curl or wget.
#
# Usage: taiyaku-probe PATH EXPECTED_STATUS
set -euo pipefail
path="${1:?a path, such as /}"
want="${2:?the status expected, such as 200}"
exec 3<>/dev/tcp/127.0.0.1/8484
printf 'GET %s HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n' "$path" >&3
read -r _ status _ <&3
[ "$status" = "$want" ]
```

Run: `chmod 0755 deploy/probe.sh && bash -n deploy/probe.sh && echo ok`
Expected: `ok`.

- [ ] **Step 2: `.dockerignore`**

```
# The build context is the whole repository, and these are what is large or
# generated in it. The Dockerfile copies named paths only, so this list is
# about what is sent to the daemon, not about what reaches the image.
.git
target
corpus
models
spike
mutants.out
.taiyaku-cache
.superpowers
.claude
web/node_modules
web/dist
crates/taiyaku-wasm/pkg
**/*.jsonl
```

- [ ] **Step 3: `Dockerfile`**

```dockerfile
# syntax=docker/dockerfile:1
#
# taiyaku: the server, the web app it serves, and nothing else. Four stages:
# the WebAssembly package the app imports, the app, the server binary, and a
# slim runtime holding the last two. See
# docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md §3.2.
#
# Versions follow what CI already pins: `rust:1-bookworm` and
# `node:26-bookworm` as its jobs' images, wasm-pack 0.15.0 by checksum as its
# `wasm` job, pnpm 11.3.0 as its `web` job. `rust-toolchain.toml` floats on
# `stable`; its install is a layer of its own, so a build reuses the
# toolchain it last installed until that file changes.

########################  1. The WebAssembly package  ###########################
FROM rust:1-bookworm AS wasm
WORKDIR /src
RUN curl -fsSL -o /tmp/wasm-pack.tar.gz \
      https://github.com/wasm-bindgen/wasm-pack/releases/download/v0.15.0/wasm-pack-v0.15.0-x86_64-unknown-linux-musl.tar.gz \
 && echo "c09f971ecaed9a2efc80fdcea7a00ef6b53c7fadc8c57d1f61b53a6aa66b668a  /tmp/wasm-pack.tar.gz" | sha256sum -c - \
 && tar -xzf /tmp/wasm-pack.tar.gz -C /tmp \
 && install -m 0755 /tmp/wasm-pack-v0.15.0-x86_64-unknown-linux-musl/wasm-pack /usr/local/bin/wasm-pack
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY scripts/build-wasm.sh scripts/
RUN --mount=type=cache,id=taiyaku-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=taiyaku-wasm-target,target=/src/target \
    ./scripts/build-wasm.sh

########################  2. The web app  #######################################
FROM node:26-bookworm AS web
RUN npm i -g pnpm@11.3.0
WORKDIR /src/web
# Installed before the package exists, as CI's `web` job does: the
# `link:../crates/taiyaku-wasm/pkg` dependency needs no target to install, so
# this layer survives every Rust change.
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
COPY --from=wasm /src/crates/taiyaku-wasm/pkg /src/crates/taiyaku-wasm/pkg
RUN pnpm run build

########################  3. The server  ########################################
FROM rust:1-bookworm AS server
WORKDIR /src
COPY rust-toolchain.toml ./
RUN rustup toolchain install
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
# `rusqlite` links the system SQLite (`libsqlite3-dev` ships in this image),
# so this stage and the runtime one below must be the same distribution.
RUN --mount=type=cache,id=taiyaku-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=taiyaku-server-target,target=/src/target \
    cargo build --release --locked -p taiyaku-cli \
 && install -m 0755 target/release/taiyaku /usr/local/bin/taiyaku

########################  4. Runtime  ###########################################
FROM debian:bookworm-slim AS runtime
# `libsqlite3-0` is the one library the binary links that this image lacks;
# `ureq`'s TLS roots are compiled in, so no `ca-certificates`.
RUN apt-get update \
 && apt-get install -y --no-install-recommends libsqlite3-0 \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /config /cache \
 && chown 99:100 /config /cache
COPY --from=server /usr/local/bin/taiyaku /usr/local/bin/taiyaku
COPY --from=web /src/web/dist /app/web
COPY --chmod=0755 deploy/probe.sh /usr/local/bin/taiyaku-probe
# The database lands at /config/taiyaku/library.db and its backups beside it;
# covers and tiles under /cache/taiyaku. Map /config somewhere backed up and
# /cache somewhere that is not: everything in it is regenerable.
ENV XDG_DATA_HOME=/config \
    XDG_CACHE_HOME=/cache
# Unraid's `nobody:users`, which owns its shares.
USER 99:100
EXPOSE 8484
HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --start-interval=2s --retries=3 \
  CMD ["taiyaku-probe", "/", "200"]
ENTRYPOINT ["taiyaku"]
CMD ["serve", "--addr", "0.0.0.0:8484", "--web", "/app/web"]
```

- [ ] **Step 4: Build it locally, timed**

```bash
start=$(date +%s)
docker buildx build --load -t taiyaku:trial . >| "$S/image-build.log" 2>&1
echo "exit=$?" >> "$S/image-build.log"
echo "elapsed=$(( $(date +%s) - start ))s" >> "$S/image-build.log"
tail -3 "$S/image-build.log"
docker image inspect taiyaku:trial --format '{{.Size}}'
```

The `echo "exit=$?"` goes on its own line so that `$?` is the build's.
Expected: `exit=0`, an elapsed time, and a size. Record both figures for
Task 5's notes entry.

Two places could fail on a first build. Each has a known fix; record whichever
was needed, in the Dockerfile comment and the report:
- **`rustup toolchain install` refuses to run without an argument.** The
  image's rustup is then older than 1.28. Replace both lines with
  `RUN rustup show`, which installs the toolchain the file names on that
  older rustup.
- **`pnpm install` fails on the missing link target.** Move
  `COPY --from=wasm … pkg` above `RUN pnpm install`, and delete the comment
  that says install does not need it.

- [ ] **Step 5: Run it against a one-volume scratch library**

```bash
T="$S/trial"
mkdir -p "$T/config" "$T/cache" "$T/library/One Series"
chmod 0777 "$T/config" "$T/cache"
small=$(find /home/davey/temp/manga-test/Volumes -name '*.cbz' -printf '%s %p\n' | sort -n | head -1 | cut -d' ' -f2-)
cp "$small" "$T/library/One Series/One Series v01.cbz"
docker run -d --name taiyaku-trial -p 127.0.0.1:18484:8484 \
  -v "$T/config:/config" -v "$T/cache:/cache" -v "$T/library:/library" taiyaku:trial
for _ in $(seq 1 30); do s=$(docker inspect -f '{{.State.Health.Status}}' taiyaku-trial); [ "$s" = healthy ] && break; sleep 2; done; echo "$s"
```

Expected: `healthy`. If it is not, read `docker logs taiyaku-trial` before
anything else. Then:

```bash
curl -s -o /dev/null -w '%{http_code} %{content_type}\n' http://127.0.0.1:18484/
curl -s -o /dev/null -w '%{http_code} %{content_type}\n' http://127.0.0.1:18484/read/anything
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:18484/api/me
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:18484/api/nothing-here
wasm=$(docker exec taiyaku-trial ls /app/web/assets | grep '\.wasm$')
curl -s -D - -o /dev/null "http://127.0.0.1:18484/assets/$wasm" | grep -i -E '^(content-type|cache-control)'
printf 'trial-password\ntrial-password\n' | docker exec -i taiyaku-trial taiyaku library user promote davey
docker exec taiyaku-trial taiyaku library scan --add /library
docker restart taiyaku-trial
```

Expected:
- `200 text/html`, twice;
- `401`, then `404`;
- `content-type: application/wasm` and
  `cache-control: public, max-age=31536000, immutable`;
- the promote prints the echo warning (no terminal) and succeeds;
- the scan prints `root /library watch probe failed (…) — scanning every
  15m`. The trial directory is owned by this user, not 99, which is
  correction 2's permission finding in miniature. It derives one work.
- The restart is needed because Part A is not on this branch.

After the restart reports healthy again:

```bash
docker logs taiyaku-trial 2>&1 | grep ' editions | '
curl -s -c "$T/cookies" -o /dev/null -w '%{http_code}\n' \
  -H 'Origin: http://127.0.0.1:18484' -H 'Content-Type: application/json' \
  -d '{"name":"davey","password":"trial-password"}' http://127.0.0.1:18484/api/session
```

Expected: one `<slug> | 1 editions | <title>` line, then `204`. With that
slug:

```bash
slug=<the slug from the line above>
edition=$(curl -s -b "$T/cookies" "http://127.0.0.1:18484/api/works/$slug" | python3 -c 'import json,sys; print(next(iter(json.load(sys.stdin)["editions"])))')
curl -s -b "$T/cookies" -o /dev/null -w '%{http_code} %{content_type}\n' "http://127.0.0.1:18484/api/works/$slug/$edition/pages/0"
docker exec taiyaku-trial ls -ld /config/taiyaku /config/taiyaku/library.db /cache/taiyaku
```

Expected: `200 image/…`; and the database and cache owned by `99 100`.

- [ ] **Step 6: Watch the health check fail against its defect**

The probe has to be able to say "unhealthy". Serve on the wrong port with
faster health settings:

```bash
docker run -d --name taiyaku-trial-defect --health-start-period 0s --health-interval 2s --health-retries 2 \
  taiyaku:trial serve --addr 0.0.0.0:9999 --web /app/web
sleep 10; docker inspect -f '{{.State.Health.Status}}' taiyaku-trial-defect
docker rm -f taiyaku-trial-defect
```

Expected: `unhealthy`. Record it in the task report.

- [ ] **Step 7: Clean up the trial container, keep the image**

```bash
docker rm -f taiyaku-trial
rm -rf "${T:?}"
docker image inspect taiyaku:trial --format '{{.Id}}'
```

`taiyaku:trial` stays: Task 6 loads it onto the NAS.

- [ ] **Step 8: Commit**

```bash
git add Dockerfile .dockerignore deploy/probe.sh
git commit -m "An image: the wasm package, the app, the server, and a slim runtime"
```

Confirm the mode survived: `git ls-files -s deploy/probe.sh` shows `100755`.

---

### Task 4: The CI job

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (append a job after `web`)

**Interfaces:**
- Consumes: the `Dockerfile` and `taiyaku-probe` from Task 3; repository
  secrets `REGISTRY_USER` and `REGISTRY_TOKEN`, which the user creates in
  Task 7.
- Produces: `forge.daveynet.xyz/davey/taiyaku:sha-<7>` and `:latest` on every
  push to `main` whose four gate jobs pass.

- [ ] **Step 1: Append the job**

Add at the end of `.forgejo/workflows/ci.yml`, indented under `jobs:` like
`web`:

```yaml
  # Builds the image, runs it until its health check passes, and pushes it —
  # on `main` only, after the four jobs above pass. Never a required status:
  # branch protection names the four `(pull_request)` contexts, and this job
  # does not run on a pull request. See docs/ci.md, "The image job".
  docker:
    needs: [lint, test, wasm, web]
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'
    runs-on: docker
    # A cold build measured 103 s for the server stage alone at -j 4 on a
    # Ryzen 9 9950X3D. The NAS's i7-9700K is slower per core and carries
    # other load, the wasm and web stages run beside the server stage, and a
    # first run also pulls three base images and installs the toolchain
    # twice. 60 minutes is about three times a pessimistic cold estimate; the
    # builder's cache makes later runs far shorter. Re-size it from the first
    # real runs.
    timeout-minutes: 60
    # No `container:`, unlike every job above. The runner's default image
    # carries git and curl, and the runner automounts the host's Docker socket
    # into it (`docker_host: automount`), which is all this job needs.
    # redextape's image job runs this way on the same runner.
    steps:
      - name: Checkout
        env:
          TOKEN: ${{ secrets.FORGEJO_TOKEN }}
        run: |
          host="${FORGEJO_SERVER_URL#http://}"; host="${host#https://}"
          scheme="${FORGEJO_SERVER_URL%%://*}"
          git init -q .
          git remote add origin "${scheme}://x-access-token:${TOKEN}@${host}/${FORGEJO_REPOSITORY}.git"
          git fetch -q --depth 1 origin "$FORGEJO_REF"
          git checkout -q FETCH_HEAD
      - name: Install the docker CLI and buildx
        run: |
          curl -fsSL -o /tmp/docker.tgz \
            https://download.docker.com/linux/static/stable/x86_64/docker-29.5.3.tgz
          echo "34eea64e9c3435f5af1b760827a56a561cd67fc2d6e9cd1813b8bb1e3ff7930b  /tmp/docker.tgz" | sha256sum -c -
          tar -xzf /tmp/docker.tgz -C /usr/local/bin --strip-components=1 docker/docker
          mkdir -p "$HOME/.docker/cli-plugins"
          curl -fsSL -o "$HOME/.docker/cli-plugins/docker-buildx" \
            https://github.com/docker/buildx/releases/download/v0.34.0/buildx-v0.34.0.linux-amd64
          echo "0144479d5a1cd710be3464ae898628cfa68033e16b225aef52f81930c45ac9b5  $HOME/.docker/cli-plugins/docker-buildx" | sha256sum -c -
          chmod +x "$HOME/.docker/cli-plugins/docker-buildx"
          docker version
          docker buildx version
      # An isolated docker-container builder, so CI never tags into the host's
      # image store — a local `:latest` moving underneath WUD would look like
      # no change at all. Its cache volume outlives the container, which is
      # why the stale container is removed with `docker rm` and never
      # `docker buildx rm`. The name is this repository's alone: redextape's
      # job removes its own builder by name the same way. The CPU cap is
      # here because BuildKit runs as its own container, outside the
      # runner's `--cpus 4`.
      - name: Set up the isolated builder
        run: |
          docker rm -f buildx_buildkit_taiyaku-builder0 >/dev/null 2>&1 || true
          docker buildx create --name taiyaku-builder --driver docker-container \
            --driver-opt cpu-period=100000 --driver-opt cpu-quota=400000 --use
      - name: Name the build
        id: t
        env:
          SHA: ${{ github.sha }}
        run: echo "short=$(echo "$SHA" | cut -c1-7)" >> "$GITHUB_OUTPUT"
      - name: Build, and load it under a throwaway tag
        env:
          SHORT: ${{ steps.t.outputs.short }}
        run: |
          docker buildx build --builder taiyaku-builder --load \
            --provenance=false --sbom=false -t "taiyaku-ci:$SHORT" .
      # The image is started here because nothing else ever starts it before
      # it is deployed: a health check that cannot pass would otherwise ship
      # on a green pipeline, which is what redextape's own Dockerfile records.
      - name: Run it until it is healthy, and probe it
        env:
          SHORT: ${{ steps.t.outputs.short }}
        run: |
          name="taiyaku-ci-$SHORT"
          docker run -d --name "$name" "taiyaku-ci:$SHORT"
          status=starting
          for _ in $(seq 1 30); do
            status=$(docker inspect -f '{{.State.Health.Status}}' "$name")
            [ "$status" = healthy ] && break
            [ "$(docker inspect -f '{{.State.Running}}' "$name")" = true ] || break
            sleep 2
          done
          docker logs "$name"
          [ "$status" = healthy ]
          docker exec "$name" taiyaku-probe /read/anything 200
          docker exec "$name" taiyaku-probe /api/me 401
          docker exec "$name" taiyaku-probe /api/nothing-here 404
      - name: Remove the probe container and image
        if: always()
        env:
          SHORT: ${{ steps.t.outputs.short }}
        run: |
          docker rm -f "taiyaku-ci-$SHORT" >/dev/null 2>&1 || true
          docker rmi "taiyaku-ci:$SHORT" >/dev/null 2>&1 || true
      - name: Log in to the forge's registry
        env:
          REGISTRY_USER: ${{ secrets.REGISTRY_USER }}
          REGISTRY_TOKEN: ${{ secrets.REGISTRY_TOKEN }}
        run: echo "$REGISTRY_TOKEN" | docker login forge.daveynet.xyz -u "$REGISTRY_USER" --password-stdin
      - name: Push
        env:
          SHORT: ${{ steps.t.outputs.short }}
        run: |
          docker buildx build --builder taiyaku-builder --push \
            --provenance=false --sbom=false \
            -t "forge.daveynet.xyz/davey/taiyaku:sha-$SHORT" \
            -t forge.daveynet.xyz/davey/taiyaku:latest .
```

- [ ] **Step 2: Parse it**

Run:
```bash
python3 -c 'import yaml; d = yaml.safe_load(open(".forgejo/workflows/ci.yml")); j = d["jobs"]["docker"]; print(sorted(d["jobs"]), j["needs"], j["if"], j["timeout-minutes"], len(j["steps"]))'
```
Expected: `['docker', 'lint', 'test', 'wasm', 'web'] ['lint', 'test', 'wasm', 'web'] github.event_name == 'push' && github.ref == 'refs/heads/main' 60 9`.

- [ ] **Step 3: Rehearse the job's shell steps locally**

The job cannot run before merge, but its scripts can. Use a builder name of
the rehearsal's own and the local Docker:

```bash
docker rm -f buildx_buildkit_taiyaku-rehearsal0 >/dev/null 2>&1 || true
docker buildx create --name taiyaku-rehearsal --driver docker-container \
  --driver-opt cpu-period=100000 --driver-opt cpu-quota=400000
docker buildx inspect taiyaku-rehearsal --bootstrap >/dev/null
docker inspect buildx_buildkit_taiyaku-rehearsal0 --format '{{.HostConfig.CpuQuota}} {{.HostConfig.CpuPeriod}}'
SHORT=rehears
docker buildx build --builder taiyaku-rehearsal --load --provenance=false --sbom=false -t "taiyaku-ci:$SHORT" .
```

Expected: `400000 100000`, which shows both driver options were accepted and
applied, followed by a successful build. Then paste the body of "Run it until
it is healthy, and probe it", with `SHORT=rehears`, into the shell.
Expected: `docker logs` output, then exit 0. Paste "Remove the probe container
and image" too, and finally:

```bash
docker buildx rm taiyaku-rehearsal
docker images taiyaku-ci --format '{{.Tag}}'
```

Expected: no `taiyaku-ci` image left.

- [ ] **Step 4: Commit**

```bash
git add .forgejo/workflows/ci.yml
git commit -m "CI: build, run and push the image on main"
```

---

### Task 5: The docs

**Files:**
- Modify: `README.md` (a new section before `## Design principles`)
- Modify: `docs/ci.md`
- Modify: `docs/status.md`
- Modify: `docs/notes.md` (a new entry at the end)

**Interfaces:**
- Consumes: Task 3's build time and image size; the readings in "Decided while
  planning" and "Spec corrections needed".
- Produces: the documents Task 6 amends with its NAS readings.

- [ ] **Step 1: README**

Insert before `## Design principles`:

````markdown
## Running it on a server

The repository builds one image: the server, with the web app it serves.
CI pushes it as `forge.daveynet.xyz/davey/taiyaku:latest` and
`:sha-<commit>` on every merge to `main`. It listens on 8484 and runs as uid
99, gid 100 (Unraid's `nobody:users`). It needs three mappings:

| Container | What | Back it up? |
| --- | --- | --- |
| `/config` | `taiyaku/library.db` and `taiyaku/backups/`: accounts, reading positions, notes, corrections | yes — it is the only copy |
| `/cache` | covers, tiles and reading-size pages | no — all of it regenerates |
| `/library` | the manga | it is your library |

A compose file in the shape this repository is deployed with:

```yaml
services:
  taiyaku:
    image: forge.daveynet.xyz/davey/taiyaku:latest
    container_name: taiyaku
    restart: unless-stopped
    ports:
      - 55006:8484
    environment:
      TAIYAKU_TRUSTED_PROXY: "172.17.0.1"   # the address the proxy connects from; see below
    volumes:
      - /mnt/user/appdata/taiyaku/config:/config
      - /mnt/user/appdata/taiyaku-cache:/cache
      - "/mnt/user/media 1/Manga:/library"
```

`/config` and `/cache` must be writable by 99:100. So must the library's root
directory, if hand corrections should be written beside it (`.taiyaku/`) and
the watch probe should work. Without that, both fall back: corrections stay in
the database and say so, and the root is scanned on the short interval.

Give the first account a password from the host, then add the library:

```sh
docker exec -it taiyaku taiyaku library user promote davey
docker exec taiyaku taiyaku library scan --add /library
```

The first scan reads every archive once to hash it, so on a large library it
takes as long as reading the whole share.

**The database is in WAL mode**, so a backup tool that copies files must stop
the container first. `taiyaku library backup` is the alternative that does not
need one.

**Behind Nginx Proxy Manager**, turn "Cache Assets" and "Block Common
Exploits" off. The first caches script and font files in a shared cache while
ignoring `Cache-Control`; the second refuses any query parameter whose value
starts with `/`. Put this in the host's Advanced tab:

```nginx
location / {
    proxy_pass http://192.168.1.69:55006;
    proxy_http_version 1.1;
    proxy_set_header Host              $http_host;
    proxy_set_header X-Forwarded-Host  $http_host;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Real-IP         $remote_addr;
    proxy_read_timeout 300s;
    proxy_send_timeout 300s;
    proxy_buffering off;
}
```

Why the whole `location`, and not just the one header taiyaku needs:
- A `location /` in the Advanced tab replaces NPM's own, and that is the only
  way in. NPM's own location sets headers of its own, and nginx then ignores
  any `proxy_set_header` written at server level.
- `$http_host`, not `$host`, keeps the port. With `$host`, a proxy on a port
  other than 443 makes every login answer 403, because the browser's `Origin`
  carries the port and `Host` would not.

`TAIYAKU_TRUSTED_PROXY` names the address the proxy's connections arrive from,
as taiyaku sees it. Read it off a running deployment rather than guessing: on
a Docker bridge it is often a gateway, not the proxy container's own address.

Updates arrive through WUD watching `:latest` (labels `wud.watch=true`,
`wud.watch.digest=true`, `wud.trigger.include=docker.redeploy`). To roll back,
pin a `:sha-…` tag and remove the labels while it is pinned.
````

The `TAIYAKU_TRUSTED_PROXY` value in the YAML above is provisional. Task 6
Step 9 replaces it with the measured address, or rewrites the comment if the
measured address is not a fixed one.

- [ ] **Step 2: `docs/ci.md`**

(a) In the `## Jobs` table, add a row after `web`:

```markdown
| `docker` | **on pushes to `main` only, after the four above pass**: builds the image in an isolated buildx builder, runs it until its health check passes and probes three paths, then pushes `forge.daveynet.xyz/davey/taiyaku:sha-<7>` and `:latest` — see "The image job" |
```

(b) In `## Runner requirements`, replace the bullet
`- Docker support, since every job specifies a \`container:\`` with:

```markdown
- Docker support, since every job but `docker` specifies a `container:`
- the host's Docker socket in the default job container (`docker_host:
  automount` in the runner's config), which the `docker` job builds with
```

(c) Insert a section immediately before
`## Why the variables say Forgejo and not GitHub`:

```markdown
## The image job

`docker` runs on pushes to `main` only, and only once `lint`, `test`, `wasm`
and `web` have passed. It is never a required status: branch protection on
`main` names the four `(pull_request)` contexts, and this job never runs on a
pull request.

**It sets no `container:`.** It needs a Docker CLI and the host's daemon, not
a toolchain. The runner's default image has git and curl, and the runner
automounts the host's socket into it. It installs a static Docker CLI matching
the NAS's daemon (29.5.3) and buildx 0.34.0, both by checksum.

**It builds in an isolated builder named `taiyaku-builder`**, a
`docker-container` driver, so nothing it builds is tagged under the registry
name in the host's image store (the throwaway `taiyaku-ci:<sha7>` is loaded
there, and removed). A local `:latest` moving underneath WUD would look to WUD like no
change at all. BuildKit then runs as a container of its own, outside the
runner's `--cpus 4`, so the builder is capped at four CPUs with
`cpu-quota=400000` and `cpu-period=100000`.

**It runs the image before pushing it.** The image is loaded under a throwaway
tag (`taiyaku-ci:<sha7>`) and started. The job waits for its health check to
report `healthy`, and probes `/read/anything` for 200, `/api/me` for 401 and
`/api/nothing-here` for 404 through `taiyaku-probe`. It then removes the
container and the image, and pushes. No other job starts the image, and a
health check that cannot pass would otherwise reach the NAS on a green
pipeline.

**Two secrets**, `REGISTRY_USER` and `REGISTRY_TOKEN`, hold a forge account and
a token with package write access. Without them the push step fails and
nothing else does.

**Its timeout is 60 minutes**, unlike the others' 10. A cold build of the
server stage alone measured 103 s at `-j 4` on a Ryzen 9 9950X3D, and the
NAS's i7-9700K is slower per core and carries other load. A first run also
pulls three base images and installs the toolchain twice. Re-size the figure
from the first real runs.
```

- [ ] **Step 3: `docs/status.md`**

(a) The `taiyaku serve` synopsis line in the "Seventeen commands" block ends
`[--insecure-cookies] [--allow-web-setup] [--web DIR]`.

(b) In the `## What is built` paragraph, replace
``plus `axum`, `tokio`,
`tower` and `ureq` in the server`` with
``plus `axum`, `tokio`,
`tower`, `tower-http` (feature `fs`, for `--web`) and `ureq` in the server``.
Search for the words and keep the line breaks as they fall.

(c) Insert immediately before `## What is built`:

```markdown
**It can be deployed.** `taiyaku serve --web DIR` serves the built web app
beside the API, so one process and one port serve both; a path no route
matched is the app's, except under `/api` and `/opds`, which keep their 404.
The repository builds an image (`Dockerfile`), CI pushes it to the forge's
registry on every merge to `main` after running it, and `README.md`'s
"Running it on a server" is how it is meant to run on the NAS. Spec
`2026-10-04-day-to-day-deployment-design.md`, Part B.
```

(d) The test count. After Task 1 and Task 2 the Rust count has grown. Measure
it:

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 cargo nextest run --workspace --all-features >| "$S/nextest.log" 2>&1
echo "exit=$?" >> "$S/nextest.log"
node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md
```

The script names the figure the document should hold. Edit both sites, the
`Six crates, N tests` line and the later `of the N are standing checks`, then
run the script again. Expected: it passes. The web counts do not move: this
part changes no TypeScript.

- [ ] **Step 4: `docs/notes.md`**

Append:

```markdown
## Deployable (2026-10-04)

Part B of `2026-10-04-day-to-day-deployment-design.md`. What was measured
while planning it, and what the plan changed because of it.

**A cold release build** of `taiyaku-cli` at `-j 4`, thin LTO and one codegen
unit, in a fresh target directory: 234 crates in 103 s on this workstation
(Ryzen 9 9950X3D). The NAS is an i7-9700K, 8 cores, at a load average of
3.6–4.7 that evening; nothing was timed there. The image built locally in
<Task 3 Step 4's elapsed>, at <its size>.

**The NAS's Docker** is 29.5.3 (API 1.54). The forge's runner automounts the
host socket into job containers and caps them at four CPUs, a cap that does
not reach a buildx `docker-container` builder.

**Nginx Proxy Manager**, read from the running container:
- `conf.d/include/proxy.conf` sets `Host $host`, `X-Forwarded-Scheme`,
  `X-Forwarded-Proto`, `X-Forwarded-For` and `X-Real-IP`, and no
  `X-Forwarded-Host`.
- Every generated `location /` sets headers of its own, so a server-level
  `proxy_set_header` in the Advanced tab never reaches it.
- An Advanced config containing `location / {` replaces the generated one
  (`internal/nginx.js`, `advancedConfigHasDefaultLocation`).
- `assets.conf` caches by file extension in a public cache, ignoring
  `Cache-Control` and `Set-Cookie`.
- `block-exploits.conf` refuses any query parameter whose value starts with
  `/`.
- `nginx.conf` sets `proxy_read_timeout 90s`.
- The proxy listens on host ports 180 and 1443, which is why the Advanced
  block uses `$http_host`.

**The library share's permissions**: `/mnt/user/media 1/Manga` is
`root:root 755`; 73 of its 580 directories cannot be written by uid 99; all
67,925 files are world-readable. So a read-write mount alone lets the
container write neither `.taiyaku/` nor the watch probe's file at the root.
```

Replace the two `<…>` with Task 3 Step 4's figures before committing.

- [ ] **Step 5: Commit**

```bash
git add README.md docs/ci.md docs/status.md docs/notes.md
git commit -m "Docs: running it on a server, the image job, and what was measured"
```

---

### Task 6: A trial on the NAS

*Amended 2026-10-04:* the user decided the trial runs **after merge, on the
image CI builds and pushes**; no image is loaded onto the NAS by hand. So Task
6 runs after the merge, Step 2 is dropped, and Step 4's stack uses
`forge.daveynet.xyz/davey/taiyaku:latest` without WUD labels during the trial
(Task 8 adds the labels afterwards). The name is also proxied through
Cloudflare, not LAN-only (see Steps 7 and 8).

Every step that changes something on the NAS is **[go-ahead]**: ask, and wait
for a yes, each time. Nothing here pushes to the forge.

**Files:**
- Modify: `docs/notes.md` (the entry Task 5 added)
- Modify: `README.md` (the `TAIYAKU_TRUSTED_PROXY` line)

**Interfaces:**
- Consumes: `taiyaku:trial` from Task 3; the README's compose file and NPM
  block from Task 5.
- Produces: the measured trusted-proxy address and a deployment the user has
  signed in to from a phone.

- [ ] **Step 1: The user's answers (settled 2026-10-04)**

1. **The library root's permissions stay as they are** (`root:root 755`, 73
   unwritable directories). The user loosens them only to flush writes, which
   is spec Part E, not this part. Expect, and record rather than fix: on a
   root with `keeps_documents` on (off by default), `.taiyaku/` writes fail
   (`persisted: false`); the watch probe fails to
   write its file; ComicInfo write-back fails. No `chown` or `chmod` on the
   library in this task.
2. **The hostname is `taiyaku.daveynet.xyz`**, proxied through Cloudflare (the
   dated note at the top of this task).
3. **Appdata Backup: taiyaku stays on its stop list, and
   `/mnt/user/appdata/taiyaku-cache` is excluded.** The user sets both in the
   plugin; ask them to confirm once the container exists.

The steps below use `HOST=taiyaku.daveynet.xyz`, and `HTTPS_PORT` for the port:
443 through Cloudflare, 1443 to NPM directly on the LAN.

- [ ] **Step 2: [go-ahead] Load the image onto the NAS**

```bash
docker save taiyaku:trial | ssh -o BatchMode=yes root@192.168.1.69 docker load
ssh -o BatchMode=yes root@192.168.1.69 "docker image inspect taiyaku:trial --format '{{.Id}}'"
```

Expected: the same id as `docker image inspect taiyaku:trial --format '{{.Id}}'`
locally.

- [ ] **Step 3: [go-ahead] The two appdata directories, and the root per Step 1**

```bash
ssh -o BatchMode=yes root@192.168.1.69 'mkdir -p /mnt/user/appdata/taiyaku/config /mnt/user/appdata/taiyaku-cache && chown 99:100 /mnt/user/appdata/taiyaku/config /mnt/user/appdata/taiyaku-cache && ls -ld /mnt/user/appdata/taiyaku/config /mnt/user/appdata/taiyaku-cache'
```

Expected: both `nobody users`. The library root is not touched (Step 1).

- [ ] **Step 4: [go-ahead] The stack, on the trial image**

The user creates a Compose Manager stack named `taiyaku` in the Unraid UI.
Paste the README's compose file into it with three changes:
- `image: taiyaku:trial`;
- no `labels` (WUD must not touch the trial);
- `TAIYAKU_TRUSTED_PROXY: ""` for now, so there is nothing to trust before
  it is measured.

Then press Compose Up. Verify from here:

```bash
ssh -o BatchMode=yes root@192.168.1.69 'sleep 25; docker inspect -f "{{.State.Health.Status}} {{.Config.User}} {{.Config.Image}}" taiyaku; docker logs taiyaku 2>&1 | tail -5'
```

Expected: `healthy 99:100 taiyaku:trial`. The log names
`/config/taiyaku/library.db` and warns that no account has a password.

- [ ] **Step 5: [go-ahead] The first account**

The user runs `docker exec -it taiyaku taiyaku library user promote davey` in
the NAS's terminal and types a password twice. The password never passes
through this session.

- [ ] **Step 6: [go-ahead] Add the library, and time the first scan**

The first scan hashes every archive once, so it reads the whole share and
wakes the disks. Run it in the background over ssh and time it:

```bash
ssh -o BatchMode=yes root@192.168.1.69 'start=$(date +%s); docker exec taiyaku taiyaku library scan --add /library; echo "exit=$? elapsed=$(( $(date +%s) - start ))s"' >| "$S/nas-first-scan.log" 2>&1
```

Use `run_in_background` with a timeout of 7,200,000 ms. When it finishes,
read the log's last lines: the probe line for `/library`, the `derived …`
line, and `exit=0 elapsed=…`. Then
`ssh -o BatchMode=yes root@192.168.1.69 docker restart taiyaku`, since Part A
is not deployed. Record the elapsed time and the probe line.

- [ ] **Step 7: [go-ahead] The NPM proxy host**

The user adds a proxy host in NPM:
- Domain `HOST`; scheme `http`; forward host `192.168.1.69`; port `55006`.
- Cache Assets **off**; Block Common Exploits **off**; Websockets off.
- SSL: the `*.daveynet.xyz` certificate, with Force SSL on.
- Advanced: the README's whole Advanced snippet, the `real_ip_header` line and
  the `location / { … }` block together. Block Common Exploits must
  be off, and the block carries the `real_ip_header CF-Connecting-IP;` line
  **above** `location / {`, outside it (the name is proxied through
  Cloudflare). nginx applies its real-IP module once per request, at server
  level first, and NPM sets `real_ip_header X-Real-IP` at http level, so a line
  inside the location loses to a client-supplied `X-Real-IP`.
- After saving, `ssh -o BatchMode=yes root@192.168.1.69 docker exec
  NginxProxyManager nginx -t` must report the configuration is ok.

Verify that NPM used the block and dropped its own location:

```bash
ssh -o BatchMode=yes root@192.168.1.69 'f=$(grep -l "server_name HOST;" /mnt/user/appdata/NginxProxyManager/nginx/proxy_host/*.conf); echo "$f"; grep -c "location / {" "$f"; grep -n "X-Forwarded-Host\|assets.conf\|block-exploits" "$f"'
```

(with `HOST` substituted). Expected: one file; `1`; the
`proxy_set_header X-Forwarded-Host  $http_host;` line; and no `assets.conf`
or `block-exploits` line.

- [ ] **Step 8: [go-ahead] Measure what arrives through the proxy**

Stand an echo server where taiyaku is, on the same network and port. Write
`$S/echo.py`:

```python
import http.server

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        print("peer", self.client_address[0], flush=True)
        for name, value in self.headers.items():
            print("header", name, value, flush=True)
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b"echo\n")

http.server.HTTPServer(("0.0.0.0", 8484), Handler).serve_forever()
```

Then:

```bash
scp -o BatchMode=yes "$S/echo.py" root@192.168.1.69:/mnt/user/appdata/taiyaku/echo.py
net=$(ssh -o BatchMode=yes root@192.168.1.69 "docker inspect taiyaku --format '{{range \$k, \$v := .NetworkSettings.Networks}}{{\$k}}{{end}}'")
ssh -o BatchMode=yes root@192.168.1.69 "docker stop taiyaku && docker run -d --rm --name taiyaku-echo --network $net -p 55006:8484 -v /mnt/user/appdata/taiyaku/echo.py:/echo.py:ro python:3-alpine python3 -u /echo.py"
curl -sk --resolve "HOST:HTTPS_PORT:192.168.1.69" -H 'X-Forwarded-Host: spoofed.example' -H 'X-Forwarded-For: 203.0.113.9' "https://HOST:HTTPS_PORT/probe"
curl -sk -H 'X-Forwarded-For: 203.0.113.9' -H 'X-Real-IP: 198.51.100.7' "https://HOST/probe"
ssh -o BatchMode=yes root@192.168.1.69 'docker logs taiyaku-echo; docker rm -f taiyaku-echo; rm -f /mnt/user/appdata/taiyaku/echo.py; docker start taiyaku'
```

Two requests reach the echo. The first goes straight to NPM on the LAN, with
`--resolve`; the second goes through Cloudflare (the public name, no
`--resolve`) and carries a forged `X-Real-IP`. Expected:
- `echo` twice;
- a `peer` line: the address to trust;
- on the first, `header X-Forwarded-Host HOST:HTTPS_PORT`, not
  `spoofed.example`, and `header X-Forwarded-For 203.0.113.9, <this
  workstation's LAN address>`;
- on the second, a rightmost `X-Forwarded-For` entry that is this network's
  real public address: not `198.51.100.7`, and not a Cloudflare address.
  taiyaku reads the rightmost entry, which is the client as Cloudflare reports
  it. If it is `198.51.100.7`, the `real_ip_header` line is inside the
  location, where it loses to the forged header.

Record every line in the task report.

- [ ] **Step 9: [go-ahead] Trust what was measured, then sign in**

Set `TAIYAKU_TRUSTED_PROXY` in the stack to the measured `peer` address, then
Compose Up. Check the cookie and the warning:

```bash
curl -sk --resolve "HOST:HTTPS_PORT:192.168.1.69" -o /dev/null -D - -H "Origin: https://HOST:HTTPS_PORT" -H 'Content-Type: application/json' -d '{"name":"davey","password":"wrong"}' "https://HOST:HTTPS_PORT/api/session" | head -1
ssh -o BatchMode=yes root@192.168.1.69 'docker logs taiyaku 2>&1 | grep -c "carried X-Forwarded-For from" || true'
```

Expected: a `401` status line, not 403, which shows the Origin check passes
through the proxy; and `0`, no untrusted-forwarded warning of either kind (each
names the peer it saw).

Then the user signs in on the phone at `https://HOST:HTTPS_PORT/`, opens a
volume and turns a page. Confirm it from the log: the session's page `GET`s
must appear.

```bash
ssh -o BatchMode=yes root@192.168.1.69 'docker logs --since 10m taiyaku 2>&1 | tail -20'
```

If the peer address is not fixed (a gateway can change when the network is
recreated), write that in `docs/notes.md`; the README carries the procedure,
not a value.

- [ ] **Step 10: Record it**

- Append to `docs/notes.md`'s "Deployable (2026-10-04)" entry: the first
  scan's elapsed time and probe line; the `peer` address and the three
  forwarded headers as they arrived; the `401` through the proxy; and the
  phone's page turn, with its log lines' times.
- The README has no address to copy, only the procedure that finds it (start
  with the variable empty, read the peer from the warning in `docker logs`),
  so the measured address goes in `docs/notes.md` and nowhere else. Leave the
  README's procedure as it is.
- Commit:

```bash
git add docs/notes.md
git commit -m "Docs: what the NAS trial measured"
```

---

### Task 7: Gate, and what must exist before merge

**Files:** none new.

- [ ] **Step 1: The full gate**

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| "$S/gate.log" 2>&1
echo "exit=$?" >> "$S/gate.log"
grep -n "all green\|NOT MEASURED\|exit=" "$S/gate.log"
```

Expected: `all green, except NOT MEASURED: standing corpus checks` (or the
exact form the script prints for a run without the corpus), and `exit=0`.
Quote the line in the report. If the `status-test-count` step fails, redo
Task 5 Step 3(d).

- [ ] **Step 2: [go-ahead] The two secrets**

The user creates `REGISTRY_USER` and `REGISTRY_TOKEN` under the `taiyaku`
repository's Settings → Actions → Secrets, the same pair `redextape` holds.
Verify the names only:

```bash
TOKEN=$(grep -m1 -E '^\s*token:' ~/.config/tea/config.yml | awk '{print $2}')
curl -s -H "Authorization: token $TOKEN" https://forge.daveynet.xyz/api/v1/repos/davey/taiyaku/actions/secrets | python3 -c 'import json,sys; print(sorted(s["name"] for s in json.load(sys.stdin)))'
```

Expected: `['REGISTRY_TOKEN', 'REGISTRY_USER']`.

- [ ] **Step 3: Hand over to finishing**

The branch goes through `superpowers:finishing-a-development-branch`: a
whole-branch review, the pre-merge claim check over the PR body as well as
the docs, and the PR through the gitea MCP with a body that is not
hard-wrapped. Nothing here merges.

---

### Task 8: After merge

Each step is **[go-ahead]**.

- [ ] **Step 1: Watch the first image run**

Find the push run for the squash commit and poll its statuses with `curl`
(not Python `urllib`, which has been refused here):

```bash
TOKEN=$(grep -m1 -E '^\s*token:' ~/.config/tea/config.yml | awk '{print $2}')
sha=$(git rev-parse origin/main)
curl -s -H "Authorization: token $TOKEN" "https://forge.daveynet.xyz/api/v1/repos/davey/taiyaku/commits/$sha/statuses" | python3 -c 'import json,sys; [print(s["context"], s["status"]) for s in json.load(sys.stdin)]'
```

Expected, eventually: `CI / docker (push) success` beside the four. Record its
duration from the run's jobs API. If it is far under 60 minutes, file the
re-size as a note for the next CI change; do not change it here.

Then confirm both tags exist with
`mcp__gitea__package_read` (`method: list`, `owner: davey`,
`type: container`, `q: taiyaku`). Expected: `latest` and `sha-<7>`.

- [ ] **Step 2: Switch the stack to the registry image**

*Per the 2026-10-04 decision: with Task 6 run after merge on the registry image
(the note at the top of Task 6), the stack is already on `:latest`; this step
only adds the labels, and Step 3's NAS command has nothing to remove.*

In the Compose Manager stack:
- `image: forge.daveynet.xyz/davey/taiyaku:latest`;
- add the three WUD labels (`wud.watch=true`, `wud.watch.digest=true`,
  `wud.trigger.include=docker.redeploy`).

Compose Up, then:

```bash
ssh -o BatchMode=yes root@192.168.1.69 'sleep 25; docker inspect -f "{{.State.Health.Status}} {{.Config.Image}}" taiyaku; docker inspect taiyaku --format "{{json .Config.Labels}}" | tr "," "\n" | grep wud'
```

Expected: `healthy forge.daveynet.xyz/davey/taiyaku:latest`, and the three
labels.

- [ ] **Step 3: Remove the trial image**

```bash
ssh -o BatchMode=yes root@192.168.1.69 'docker rmi taiyaku:trial'
docker rmi taiyaku:trial
```

- [ ] **Step 4: See WUD redeploy once**

On the next merge to `main`, after CI pushes, WUD's five-minute cron should
recreate the container. Confirm with
`ssh -o BatchMode=yes root@192.168.1.69 "docker inspect -f '{{.Image}} {{.State.StartedAt}}' taiyaku"`
before and after. The image id changes and the start time follows the push.
Record it in `docs/notes.md` on that next branch.

---

## Spec coverage

| Spec §3 requirement | Task |
| --- | --- |
| `--web DIR`, `ServeDir`, unchanged without it, `tower-http` `fs` only | 1, 2 |
| Non-API, non-file paths answer `index.html`; unknown `/api`, `/opds` keep their 404 | 1 (correction 1) |
| Static files need no credential; `/api` and `/opds` stay guarded | 1 |
| `/assets/*` immutable, `index.html` `no-cache` | 1 |
| `--web` joins a struct | 2 |
| Rejected alternatives (hand-written handler, embedding) | not built; README and status say what was |
| Four stages, bookworm both sides, `libsqlite3-0`, 99:100 | 3 |
| `/config`, `/cache`, `/library` mappings; read-write library | 3, 5, 6 (correction 2) |
| `serve --addr 0.0.0.0:8484 --web /app/web`; `TAIYAKU_TRUSTED_PROXY` | 2, 3 |
| `HEALTHCHECK` on `GET /` | 3 |
| First account via `docker exec -it` | 5, 6 |
| WAL, Appdata Backup stop list | 5, 6 (correction 5) |
| CI on `main` only, isolated builder, `sha-<7>` + `latest`, secrets, provenance/sbom off | 4, 7 |
| Runs and probes the image before pushing | 4 |
| WUD deploys `:latest`; every merge goes live | 8 |
| NPM proxy host, `X-Forwarded-Host`, trusted proxy read off the running container | 6 (correction 4) |
| Route tests of §3.5 | 1 |
| CI's run-and-probe | 4 |
| By hand on the NAS: starts, sign-in through NPM, page turn on the phone, health healthy | 6 |
