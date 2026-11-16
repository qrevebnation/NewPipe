# NanoPipe M2 — Playback (Media3 + JNI) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Play a video's audio (and later its video) from the detail screen, including with the app in the background, using Media3/ExoPlayer behind a small Kotlin foreground service driven from Rust.

**Architecture:** `crates/core` gains a `player` module that turns a rustypipe `VideoPlayer` into a `StreamInfo` (audio URL, optional video URL, mime, title, artist, thumbnail). The Dioxus UI asks a `MediaController` for transport actions; the desktop/Android-absent implementation is a no-op stub so the crate still builds on the host. On Android, Rust reaches Kotlin through JNI: a Kotlin `PlaybackService` (in a custom `MainActivity.kt` file wired via `Dioxus.toml`) owns ExoPlayer plus a `MediaSession`.

**Tech Stack:** Rust 2021, rustypipe 0.11, jni 0.21, Dioxus 0.7 (`mobile`), Kotlin + AndroidX Media3 (ExoPlayer + MediaSession), Gradle via `dx`.

**Spec:** `docs/superpowers/specs/2026-11-15-nanopipe-design.md` (§8 playback, §12 risks)

## Global Constraints

- Rust edition `2021`; license `GPL-3.0`.
- `rustls-tls-webpki-roots` only; never native-tls/OpenSSL.
- Every Android build goes through `dx bundle --platform android`; `dx` **rewrites** generated Gradle/Kotlin files each build, so all custom native config must live in `crates/app/Dioxus.toml` + files it points at (verified in `dioxus-cli-0.7.10/src/build/android.rs` — it writes template files unconditionally and has no `remove_dir_all`).
- Supported `Dioxus.toml` keys (verified in `dioxus-cli-0.7.10/src/config/`):
  `[application] android_manifest`, `[application] android_main_activity`, `[android] gradle_dependencies`, `[android] gradle_plugins`, `[android] proguard_rules`, `[android] foreground_service_types`, `[android] permissions`.
- All commands run on the dev host `kantor`: `ssh kantor 'cd ~/nanopipe/NewPipe && source ~/nanopipe/env.sh && <cmd>'`.
- **No `apt`/`sudo`/system changes on `kantor`** — this machine is shared and was broken once by package installs. Gradle/Maven downloads are fine (project cache only).
- Emulator verification needs a 4 GB / 4-core AVD; a 2 GB AVD ANRs during WebView init.
- Never run a heavy build at the same time as another heavy build on `kantor`.

## Review Focus

1. Video with **no audio stream** selected (rare, but possible) — must show an error, not a silent dead player.
2. **Age-restricted / PO-token-gated** stream (HTTP 403) — must surface a readable error, not a spinner forever.
3. **Rapid repeated play/pause taps** — must not leak ExoPlayer instances or crash the service.
4. Playback while the app is **backgrounded / screen off** — audio must continue and the notification must show transport controls.
5. Starting a **second video** while one plays — must replace the current item, not stack players.

---

### Task 1: Wire the native Android config (service + deps + manifest)

**Files:**
- Modify: `crates/app/Dioxus.toml`
- Create: `crates/app/android/MainActivity.kt` (custom MainActivity + `PlaybackService` in the same file)
- Create: `crates/app/android/AndroidManifest.xml`

**Interfaces:**
- Consumes: nothing.
- Produces: an APK whose merged manifest declares `com.example.NanopipeApp.PlaybackService` with `foregroundServiceType="mediaPlayback"` and the permissions `INTERNET`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, `POST_NOTIFICATIONS`.

- [ ] **Step 1: Add the Android config to `Dioxus.toml`**

```toml
[application]
name = "nanopipe"
android_main_activity = "android/MainActivity.kt"
android_manifest = "android/AndroidManifest.xml"

[android]
gradle_dependencies = [
  "androidx.media3:media3-exoplayer:1.5.1",
  "androidx.media3:media3-session:1.5.1",
]
foreground_service_types = ["mediaPlayback"]
```

- [ ] **Step 2: Write the Kotlin file**

`android/MainActivity.kt` keeps the `dev.dioxus.main` package (dx's template expects it) and adds a sibling class:

```kotlin
package dev.dioxus.main

import android.app.*
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer

class PlaybackService : Service() {
    companion object {
        var player: ExoPlayer? = null
        const val CHANNEL_ID = "nanopipe_playback"
    }

    override fun onCreate() {
        super.onCreate()
        player = ExoPlayer.Builder(this).build()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(1, buildNotification())
        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
```

Keep the generated `MainActivity` body by copying it from `target/dx/nanopipe-app/release/android/app/app/src/main/kotlin/dev/dioxus/main/MainActivity.kt` and appending `PlaybackService` to the same file.

- [ ] **Step 3: Write the manifest**

Start from the generated `AndroidManifest.xml`, add the four permissions, `android:foregroundServiceType="mediaPlayback"` on a `<service android:name=".PlaybackService" android:exported="false"/>`, and keep the existing `MainActivity` entry and `usesCleartextTraffic` setting.

- [ ] **Step 4: Build and verify the service is declared**

`cd crates/app && dx bundle --platform android --package-types apk --release` then
`$ANDROID_HOME/build-tools/34.0.0/aapt2 dump xmltree --file AndroidManifest.xml <apk> | grep -i -E "service|NanopipeApp.PlaybackService|foregroundServiceType"`.

Expected: the APK exists and the dump shows `dev.dioxus.main.PlaybackService` with `foregroundServiceType=0x2` (mediaPlayback), plus the four permissions.

- [ ] **Step 5: Commit**

```bash
git add crates/app/Dioxus.toml crates/app/android
git commit -m "feat(app): add Android playback service scaffolding (Media3, foreground service)"
```

---

### Task 2: Rust stream selection in `core::player`

**Files:**
- Create: `crates/core/src/player.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod player;` + re-exports)
- Test: `crates/core/src/player.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `extractor::Client` (Task M1-2).
- Produces:
  - `StreamInfo { audio_url: String, audio_mime: String, video_url: Option<String>, title: String, uploader: String, thumb_url: Option<String> }`
  - `Client::streams(&self, video_id: &str) -> anyhow::Result<StreamInfo>`
  - `pick_audio<'a>(streams: &'a [Stream]) -> Option<&'a Stream>` (pure, testable)

rustypipe API (verified against docs.rs and the crate source): `rp.query().player(id)` → `VideoPlayer`, whose `select_video_audio_stream(&StreamFilter)` returns an audio-only stream and an optional progressive video stream; `Stream` carries `url` and `mime_type`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn prefers_audio_only_and_tolerates_missing_audio() {
    assert!(pick_audio(&[]).is_none());
}
```

- [ ] **Step 2: Run it and watch it fail**

`cargo test -p nanopipe-core` → FAIL to compile (`pick_audio` not found).

- [ ] **Step 3: Implement `player.rs`**

Map the selected streams into `StreamInfo`; error with a readable message when no audio stream exists (`anyhow::bail!("no audio stream available for {id}")`).

- [ ] **Step 4: Run the core tests**

`cargo test -p nanopipe-core` → PASS.

- [ ] **Step 5: Add an ignored real-network test**

`crates/core/tests/network.rs`: fetch streams for the first search hit and assert `!info.audio_url.is_empty()`.

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -m "feat(core): add stream selection for playback"
```

---

### Task 3: JNI bridge Rust → PlaybackService

**Files:**
- Create: `crates/app/src/media.rs`
- Modify: `crates/app/Cargo.toml` (add `jni = "0.21"` for `cfg(target_os = "android")`)

**Interfaces:**
- Consumes: `StreamInfo` (Task 2), the Kotlin class from Task 1.
- Produces: `media::play(info: StreamInfo) -> Result<(), String>`, `media::pause()`, `media::resume()`, `media::stop()`. On non-Android targets these are stubs returning `Ok(())` so the host build and tests still work.

- [ ] **Step 1: Write the failing test**

`#[test] fn stubs_exist_on_all_platforms()` asserting `media::pause().is_ok()` on the host.

- [ ] **Step 2: Run it and watch it fail**

`cargo test -p nanopipe-app` → FAIL to compile.

- [ ] **Step 3: Implement, with the Android path behind `#[cfg(target_os = "android")]`**

Attach to the Java VM, look up `dev/dioxus/main/PlaybackService`, and call static helpers. Verify the Android path compiles with `cargo ndk -t arm64-v8a build -p nanopipe-app`.

- [ ] **Step 4: Run the host tests**

`cargo test -p nanopipe-app` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): add JNI bridge to the playback service"
```

---

### Task 4: Transport UI + background playback

**Files:**
- Modify: `crates/app/src/screens/detail.rs` (Play/Pause button)
- Modify: `crates/app/src/main.rs` (mini player bar, CSS)

**Interfaces:**
- Consumes: `media::*` (Task 3), `StreamInfo` (Task 2).
- Produces: a working Play button on the detail screen and a mini bar showing the current title with a Pause action.

- [ ] **Step 1: Add the Play action** — on tap, resolve `core::Client::streams(video_id)` then `media::play(info)`; render `Status::Loading`/`Error` like the existing detail states, with the error text visible (Review Focus 1 and 2).
- [ ] **Step 2: Add the mini player** — a fixed bar at the bottom showing title + Pause/Resume, driven by a `now_playing: Signal<Option<String>>` owned by `App`.
- [ ] **Step 3: Host build** — `cargo build -p nanopipe-app` → `Finished`.
- [ ] **Step 4: Emulator verification** — build the release APK and run: search → open a result → tap Play → **audio audible / player state advances**, then press Home and confirm playback continues and the notification is present (Review Focus 4). Use `adb shell dumpsys media_session` to prove the session is active.
- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): add transport controls and mini player"
```

---

## Known risks carried into M2

- **PO tokens:** since Aug 2024 YouTube requires a PO token for web-client streams; `rustypipe` delegates this to the external `rustypipe-botguard` binary. If streams come back 403, Task 2 must fall back to a client that does not need a PO token (see `rustypipe`'s `ClientType` list) and, only if that fails, we revisit shipping `botguard`.
- **Video rendering:** Media3 renders video to a `SurfaceView`; overlaying that on the WebView layout is deferred to M5. M2 is audio-only on purpose.

## Self-Review

- **Spec coverage:** spec §8 (playback + background + risks) maps to Tasks 1–4; §11 (toolchain) unchanged; the video-surface half of §8 is explicitly deferred to M5, as the spec already says.
- **Step scan:** each step names one file/command with an observable result; the Kotlin step points at the exact generated file to copy from.
- **Type consistency:** `StreamInfo` fields and `media::play/pause/resume/stop` signatures are used identically in Tasks 3 and 4.
- **Review Focus:** (1) no-audio error → Task 2 Step 3; (2) 403/PO token → Task 2 Step 3 + Task 4 Step 1 error state; (3) rapid taps → Task 4 Step 1 (single service-owned player); (4) background → Task 4 Step 4; (5) second video replaces the item → Task 4 Step 1.
- **Proportion:** comparable to the M1 plan; code appears as signatures and short snippets, not full bodies.
