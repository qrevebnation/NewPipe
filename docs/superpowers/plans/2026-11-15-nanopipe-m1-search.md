# NanoPipe M1 — Search & Video Detail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Search YouTube from the app and open a video's detail page, both backed by the Rust core and verified on an Android emulator.

**Architecture:** `crates/core` gains a `models` module (app-owned DTOs + pure formatters) and an `extractor` module that wraps `rustypipe` behind a small `Client` type whose signatures do not leak rustypipe types. `crates/app` gains an `api` module (lazy client + string-error async wrappers) and `screens/` for search and detail, wired with plain Dioxus signals (no router yet).

**Tech Stack:** Rust 2021, rustypipe 0.11 (`default-features = false`, `rustls-tls-webpki-roots`), tokio, serde, Dioxus 0.7 (`mobile`).

**Spec:** `docs/superpowers/specs/2026-11-15-nanopipe-design.md`

## Global Constraints

- Rust edition `2021`; workspace `resolver = "2"`.
- License `GPL-3.0` on every crate.
- Must cross-compile for `aarch64-linux-android` (checked with `cargo ndk -t arm64-v8a build`).
- TLS must be `rustls-tls-webpki-roots`; **never** enable OpenSSL/native-tls (breaks the Android build).
- Dioxus is `0.7` with the `mobile` feature; rustypipe is `0.11` with `default-features = false`.
- Keep the forced `rquickjs = { version = "0.9", default-features = false, features = ["bindgen"] }` dependency — Android has no shipped rquickjs bindings.
- Every command runs on the dev host `kantor`, i.e. `ssh kantor 'cd ~/nanopipe/NewPipe && source ~/nanopipe/env.sh && <command>'`. Steps below show `<command>` only.
- UI text/labels are plain ASCII for now (localization is out of scope).

## Review Focus

The spec is silent on these; each is pinned to a test in the task that owns the code.

1. Whitespace-only search query — must not hit the network or panic; it returns an empty list.
2. Non-ASCII / emoji titles and queries (e.g. `日本語`, `café ☕`) — must survive the DTO round-trip unmangled.
3. Livestream / missing duration (`duration_s = None`) — must not render `0:00` or panic; duration text is empty and `LIVE` is shown.
4. YouTube/network failure (rate limit, bot check, offline) — must surface an error string in the UI with a retry, never a blank screen or crash.
5. Very long title/description (>2000 chars) — must not break layout; text wraps/truncates in CSS.

---

### Task 1: Core models and pure formatters

**Files:**
- Modify: `crates/core/Cargo.toml`
- Create: `crates/core/src/models.rs`
- Create: `crates/core/src/lib.rs` (replace contents)
- Test: `crates/core/src/models.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `VideoSummary { id: String, title: String, uploader: String, duration_s: Option<u32>, thumb_url: Option<String>, is_live: bool }`
  - `VideoDetails { id: String, title: String, uploader: String, duration_s: Option<u32>, thumb_url: Option<String>, is_live: bool, description: String, view_count: Option<u64> }`
  - `VideoSummary::new(id: impl Into<String>, title: impl Into<String>, uploader: Option<String>, duration_s: Option<u32>, thumb_url: Option<String>, is_live: bool) -> VideoSummary` (uploads `"Unknown"` when `uploader` is `None`)
  - `format_duration(secs: Option<u32>) -> String`

- [ ] **Step 1: Write the failing tests**

In `crates/core/src/models.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_short_and_long_durations() {
        assert_eq!(format_duration(Some(7)), "0:07");
        assert_eq!(format_duration(Some(187)), "3:07");
        assert_eq!(format_duration(Some(3723)), "1:02:03");
        assert_eq!(format_duration(Some(0)), "0:00");
    }

    #[test]
    fn missing_duration_is_empty_not_zero() {
        assert_eq!(format_duration(None), "");
    }

    #[test]
    fn summary_defaults_uploader_and_keeps_unicode() {
        let s = VideoSummary::new("abc", "café ☕ 日本語", None, Some(90), None, false);
        assert_eq!(s.uploader, "Unknown");
        assert_eq!(s.title, "café ☕ 日本語");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

`cargo test -p nanopipe-core` — expected: FAIL to compile (`format_duration` / `VideoSummary` not found).

- [ ] **Step 3: Implement `models.rs`**

Add `serde = { version = "1", features = ["derive"] }` to `crates/core/Cargo.toml`. Implement the two structs with `#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]`, the `new` constructor, and `format_duration` (`None -> ""`; `h:mm:ss` when `>= 3600`, else `m:ss`, zero-padded seconds).

- [ ] **Step 4: Run tests to verify they pass**

`cargo test -p nanopipe-core` — expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/core/Cargo.toml crates/core/src/models.rs
git commit -m "feat(core): add video DTOs and duration formatter"
```

---

### Task 2: Core extractor over rustypipe

**Files:**
- Modify: `crates/core/Cargo.toml` (add `anyhow`)
- Create: `crates/core/src/extractor.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod extractor;` and `pub use` re-exports)
- Test: `crates/core/tests/search_empty.rs`, `crates/core/tests/network.rs`

**Interfaces:**
- Consumes: `VideoSummary`, `VideoDetails`, `VideoSummary::new` from Task 1.
- Produces:
  - `extractor::Client::new(storage_dir: impl AsRef<std::path::Path>) -> anyhow::Result<Client>`
  - `Client::search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<VideoSummary>>`
  - `Client::video_details(&self, video_id: &str) -> anyhow::Result<VideoDetails>`

rustypipe calls used (verified against docs.rs/rustypipe 0.11): `RustyPipe::builder().storage_dir(dir).build()`, `rp.query().search::<rustypipe::model::VideoItem, _>(q)`, `rp.query().video_details(id)`. Map `VideoItem { id, name, duration, thumbnail, channel, is_live, .. }` and the `VideoDetails` model (same field names) into the DTOs; take the last `thumbnail` entry's `url` as `thumb_url`.

- [ ] **Step 1: Write the failing tests**

`crates/core/tests/search_empty.rs`:

```rust
#[tokio::test]
async fn empty_query_returns_no_results_without_network() {
    let dir = std::env::temp_dir().join("nanopipe-test-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let client = nanopipe_core::extractor::Client::new(&dir).unwrap();
    assert!(client.search("   ", 20).await.unwrap().is_empty());
}
```

`crates/core/tests/network.rs` (network; run only when explicitly asked):

```rust
// Run with: cargo test -p nanopipe-core --test network -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn searches_and_fetches_details() {
    let dir = std::env::temp_dir().join("nanopipe-test-net");
    std::fs::create_dir_all(&dir).unwrap();
    let client = nanopipe_core::extractor::Client::new(&dir).unwrap();

    let results = client.search("rust programming", 5).await.unwrap();
    assert!(!results.is_empty(), "search returned no results");

    let v = &results[0];
    assert!(!v.id.is_empty() && !v.title.is_empty());
    let details = client.video_details(&v.id).await.unwrap();
    assert_eq!(details.id, v.id);
    assert!(!details.title.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

`cargo test -p nanopipe-core` — expected: FAIL to compile (`nanopipe_core::extractor` not found).

- [ ] **Step 3: Implement `extractor.rs`**

Add `anyhow = "1"`. Implement `Client` holding `rustypipe::client::RustyPipe`. `search` trims the query; if empty, return `Ok(Vec::new())` **before** touching the network; otherwise run the query, take `limit` items, and map. `video_details` maps the model struct. Both return `anyhow::Result`. Do not leak rustypipe types in the public signatures.

- [ ] **Step 4: Run tests to verify they pass**

`cargo test -p nanopipe-core` — expected: PASS (network test reported as `ignored`).

- [ ] **Step 5: Verify it still cross-compiles for Android**

`cargo ndk -t arm64-v8a build -p nanopipe-core` — expected: `Finished`, no errors.

- [ ] **Step 6: Commit**

```bash
git add crates/core/Cargo.toml crates/core/src/extractor.rs crates/core/src/lib.rs crates/core/tests
git commit -m "feat(core): add rustypipe-backed extractor (search, video details)"
```

---

### Task 3: App API layer and client bootstrap

**Files:**
- Modify: `crates/app/Cargo.toml`
- Create: `crates/app/src/api.rs`
- Modify: `crates/app/src/main.rs` (add `mod api;`)

**Interfaces:**
- Consumes: `extractor::Client` from Task 2.
- Produces:
  - `api::data_dir() -> std::path::PathBuf`
  - `api::search(query: String, limit: usize) -> Result<Vec<VideoSummary>, String>`
  - `api::video(id: String) -> Result<VideoDetails, String>`

Errors cross the UI boundary as `String` (the UI shows the message plus a Retry button).

- [ ] **Step 1: Write the failing test**

In `crates/app/src/api.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn data_dir_is_non_empty() {
        assert!(!super::data_dir().as_os_str().is_empty());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

`cargo test -p nanopipe-app` — expected: FAIL to compile (`data_dir` not found). Note: this compiles the app crate for the host; that is expected to work.

- [ ] **Step 3: Implement `api.rs`**

Add `nanopipe-core = { path = "../core" }` and `tokio` to `crates/app/Cargo.toml`. Implement `data_dir()` as `NANOPIPE_DATA_DIR` env var if set, else `std::env::temp_dir().join("nanopipe")`, created on first use. Keep one `tokio::sync::OnceCell`/`std::sync::OnceLock` holding the `Client` so the rustypipe cache is reused. Map `anyhow::Error` to `err.to_string()`.

- [ ] **Step 4: Run it to verify it passes**

`cargo test -p nanopipe-app` — expected: 1 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/app/Cargo.toml crates/app/src/api.rs crates/app/src/main.rs
git commit -m "feat(app): add api layer for core extractor"
```

---

### Task 4: Search screen

**Files:**
- Create: `crates/app/src/screens/mod.rs`
- Create: `crates/app/src/screens/search.rs`
- Modify: `crates/app/src/main.rs` (swap placeholder for the search screen)

**Interfaces:**
- Consumes: `api::search` from Task 3, `VideoSummary` from Task 1.
- Produces: `screens::search::SearchScreen(on_open: EventHandler<String>) -> Element`

- [ ] **Step 1: Implement the screen**

`SearchScreen` renders a text input, a Search button, a status line, and a result list. State: `query: Signal<String>`, `results: Signal<Vec<VideoSummary>>`, `status: Signal<Status>` where `Status` is `Idle | Loading | Error(String)`. On submit, spawn `api::search(query(), 20)`; on `Ok` store results and set `Loading`→`Idle`, on `Err` set `Error(msg)`. Each row shows thumbnail (`img` with CSS `object-fit: cover`), title, uploader, and `format_duration(duration_s)`; when `is_live` show a `LIVE` badge instead of the duration. Clicking a row calls `on_open(video_id)`. Use the slick-dark styling already in `main.rs` (single accent colour, hairline separators, no cards/shadows).

- [ ] **Step 2: Verify the host build**

`cargo build -p nanopipe-app` — expected: `Finished`.

- [ ] **Step 3: Bundle and run on the emulator**

`cd crates/app && dx bundle --platform android --package-types apk`, then run `bash /tmp/np-run.sh` (starts the headless AVD, installs the APK, launches, screenshots to `/tmp/np.png`).

- [ ] **Step 4: Verify the observed result**

Pull the screenshot and confirm visually: search box rendered, and after entering `rust programming` a list of video rows appears. Expected: no crash and the rows show real titles. (If the network is refused from the emulator, the status line must show the error text and a Retry button — that also passes.)

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/screens crates/app/src/main.rs
git commit -m "feat(app): add search screen"
```

---

### Task 5: Video detail screen

**Files:**
- Create: `crates/app/src/screens/detail.rs`
- Modify: `crates/app/src/screens/mod.rs`
- Modify: `crates/app/src/main.rs` (Navigation enum: `Search` / `Detail(String)`)

**Interfaces:**
- Consumes: `api::video` from Task 3, `VideoDetails` from Task 1.
- Produces: `screens::detail::DetailScreen(video_id: String, on_back: EventHandler<()>) -> Element`

- [ ] **Step 1: Implement navigation + screen**

Add `enum Nav { Search, Detail(String) }` in `main.rs` and switch on it. `DetailScreen` spawns `api::video(video_id)` on mount, shows a loading state, an error state with Retry, or: thumbnail, title, uploader, `format_duration`/`LIVE`, view count (`format_count` helper with `K`/`M` suffixes — add a unit test for `format_count(1500) == "1.5K"` and `format_count(2_000_000) == "2M"` in `crates/app/src/screens/detail.rs`), and the description in a scrollable block that wraps long text. A back button calls `on_back`.

- [ ] **Step 2: Run the unit test**

`cargo test -p nanopipe-app` — expected: PASS (includes the Task 3 test).

- [ ] **Step 3: Bundle and run on the emulator**

`cd crates/app && dx bundle --platform android --package-types apk`, then `bash /tmp/np-run.sh`.

- [ ] **Step 4: Verify the observed result**

Screenshot must show the detail page (title, uploader, description) after tapping a search result; back returns to the results list. Expected: no crash, text wraps, `LIVE`/duration correct.

- [ ] **Step 5: Commit**

```bash
git add crates/app/src/screens crates/app/src/main.rs
git commit -m "feat(app): add video detail screen and navigation"
```

---

## Self-Review

- **Spec coverage:** M1 in the spec is "Search + halaman detail video (rustypipe → Dioxus)" — Tasks 1–5 deliver exactly that. Cache persistence (`storage_dir`) is Task 2/3. UI theme requirement is Task 4.
- **Step scan:** every step names one action/output; no step leaves a decision open (DTO fields, function signatures, and expected test values are all fixed).
- **Type consistency:** `VideoSummary`/`VideoDetails` field names and `Client::search`/`video_details`/`api::search`/`api::video` signatures are used identically in Tasks 3–5.
- **Review Focus:** (1) empty query → Task 2 Step 1; (2) Unicode → Task 1 Step 1; (3) live/missing duration → Task 1 Step 1 + Task 4 Step 1; (4) network failure → Task 4 Step 1 error state; (5) long text → Task 5 Step 1 wrapping.
- **Proportion:** plan is comparable in size to the spec; code appears as signatures and test assertions, not bodies.
