# NanoPipe — Design Spec

- **Status:** Draft (menunggu review)
- **Tanggal:** 2026-11-15
- **Penulis:** qrevebnation (dibantu CommandCode)
- **Repo:** https://github.com/qrevebnation/NewPipe (fork dari https://github.com/TeamNewPipe/NewPipe)
- **Branch kerja:** `rust-rewrite`
- **Lisensi:** GPL-3.0 (wajib — lihat §10)

---

## 1. Ringkasan

NanoPipe adalah client YouTube/YouTube Music untuk **Android**, ditulis hampir seluruhnya
dalam **Rust**. Aplikasi ini menyediakan cari → putar → langganan → unduh offline, dengan
UI simple dan modern, dan seukuran mungkin tetap ringan.

Proyek ini adalah **rewrite penuh dalam Rust** atas gagasan NewPipe. Ia tidak berbagi kode
dengan NewPipe (Java/Kotlin); yang dipertahankan adalah **lineage** (fork GitHub +
riwayat commit upstream), **atribusi**, dan **lisensi**.

### Intent (yang harus disetujui)

- **Untuk siapa:** pemakaian pribadi; distribusi sideload APK (GitHub Releases).
- **Sukses =** di Android nyata: bisa mencari video, memutarnya (video di foreground,
  audio di background dengan notifikasi), menyimpan langganan, dan mengunduh video/audio
  untuk diputar offline — semuanya dari satu APK Rust.
- **Bukan tujuan (v1):** impersonasi akun YouTube, komentar, live chat, Play Store,
  semua fitur NewPipe (mis. SponsorBlock, feed kompleks, multi-service lain).

## 2. Ruang lingkup v1

Fitur wajib (semua dipilih pengguna):

1. **Browse + Search + Play** — cari video, buka detail, putar video & audio.
2. **Background audio** — audio tetap jalan saat app di background/layar mati, dengan
   notifikasi media + kontrol lock-screen.
3. **Downloads offline** — unduh stream untuk diputar tanpa jaringan.
4. **Subscriptions + Feed** — langganan channel + feed penampilan video terbaru.

Di luar v1 (eksplisit): login akun, komentar, muxing DASH via ffmpeg, multi-service.

## 3. Batasan (constraints)

- **Hukum/ToS:** YouTube tidak menyediakan API resmi untuk client pihak ketiga.
  Aplikasi ini bekerja dengan mengekstrak endpoint internal YouTube — menyalahi ToS.
  Risiko ditanggung pemakai. **Tidak untuk Play Store** (kebijakan Play melarang
  pengunduh YouTube); distribusi via GitHub/F-Droid.
- **Platform:** Android, target utama `aarch64-linux-android` (ARM64). Emulator x86_64
  untuk pengujian.
- **Ringan:** hindari webview-berat bila bisa; hindari membawa ffmpeg di v1.
- **"100% Rust" tidak realistis di Android:** background media wajib memakai API
  framework Java (ForegroundService + MediaSession). Keputusan: **Opsi B — "99% Rust"**,
  bagian itu dipakai modul Kotlin kecil.

## 4. Keputusan arsitektur (yang disetujui)

| Keputusan | Pilihan | Alasan |
|---|---|---|
| Platform | Android native | permintaan pengguna |
| UI | **Dioxus mobile** (UI full Rust) | permintaan pengguna; render via system WebView |
| Core | **Rust** (extractor/db/download) | permintaan "full Rust" |
| Background media | **Kotlin kecil** (Media3/ExoPlayer) via JNI | satu-satunya cara andal di Android |
| Extractor | crate **`rustypipe`** | client YouTube Innertube pure-Rust, "inspired by NewPipe" |
| Host kerja | mesin `kantor` (SSH, user `cme`) | permintaan pengguna (tidak di lokal) |

## 5. Arsitektur & komponen

```
<repo root>                        # branch rust-rewrite
├─ Cargo.toml                      # workspace
├─ crates/
│  ├─ core/                        # lib Rust, platform-agnostic
│  │   ├─ extractor/               # bungkus rustypipe; persist cache
│  │   ├─ models/                  # Video, Channel, Playlist, StreamInfo, DownloadTask
│  │   ├─ db/                      # rusqlite: subscriptions, history, downloads, settings
│  │   └─ download/                # reqwest streaming + progress
│  └─ app/                         # Dioxus (UI), cdylib target Android
├─ android/                        # shell Android
│  ├─ app/src/main/AndroidManifest.xml
│  ├─ app/src/main/java/.../MainActivity.kt        # host Dioxus/WebView
│  ├─ app/src/main/java/.../PlaybackService.kt     # Media3 foreground service
│  └─ build.gradle.kts
├─ LICENSE                         # GPL-3.0 (dari upstream)
├─ NOTICE                          # kredit upstream (NewPipe, rustypipe)
└─ README.md
```

**Batas unit (mudah diuji mandiri):**

- `core` tidak tahu apa pun soal Android/Dioxus → bisa diuji di desktop.
- `app` (Dioxus) bergantung pada `core` + abstraksi `MediaController`.
- `PlaybackService` (Kotlin) hanya tahu perintah media (play/pause/seek/queue) dan
  tidak tahu soal extractor/DB.

## 6. Alur data

```
[UI Dioxus] --panggil--> [core::extractor] --via rustypipe--> [YouTube Innertube]
     |                          |
     |                          +--> [core::db] (SQLite) : langganan, history, cache
     |                          +--> [core::download] : file offline
     v
[MediaController (trait Rust)] --JNI--> [PlaybackService.kt : Media3/ExoPlayer]
                                            |
                                            +--> audio output / SurfaceView (video)
```

Video: Media3 memutar ke `SurfaceView` native yang di-overlay pada rect `<div>` placeholder
di WebView (rect diambil via bridge JS). Audio/background: tanpa surface.

## 7. Model data (SQLite via rusqlite)

- `subscriptions(id TEXT PK, name, avatar_url, added_at)`
- `videos(id TEXT PK, title, uploader, duration_s, thumb_url, updated_at)` — cache metadata
- `history(video_id TEXT PK, played_at, position_s)`
- `downloads(id INTEGER PK, video_id, title, kind TEXT[video|audio], path, status, bytes, created_at)`
- `playlists(id, name)` + `playlist_items(playlist_id, video_id, pos)`
- `settings(key TEXT PK, value TEXT)`

File unduhan disimpan di app-specific external dir (tanpa izin runtime).

## 8. Playback & background

- Satu pemutar: **Media3/ExoPlayer** di `PlaybackService.kt` (foreground service).
- Kontrol dari Rust via JNI: `play(stream: StreamInfo)`, `pause()`, `resume()`,
  `seekTo(ms)`, `setQueue(list)`, `setVideoSurfaceOn/Off()`.
- `MediaSession` + notifikasi (kontrol lock-screen); butuh izin
  `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, `POST_NOTIFICATIONS`.
- Sumber stream dari `rustypipe` (URL DASH/adaptif; audio-only untuk mode musik).
- **Risiko teknis tertinggi:** menyatukan SurfaceView native dengan layout WebView.
  Mitigasi: milestone M2 hanya audio; video surface masuk M5.

## 9. Unduhan

- v1: unduh **progressive** (mis. format 18/22 = video+audio dalam satu file, tanpa
  muxing) dan **audio-only** (m4a) untuk mode musik.
- Progres & kontrol via `core::download` (reqwest streaming, resume via range).
- Muxing DASH (video+audio terpisah → satu file) memakai ffmpeg → **v2**.

## 10. Atribusi & lisensi

- `rustypipe` berlisensi **GPL-3.0**, NewPipe juga **GPL-3.0** → proyek ini **wajib
  GPL-3.0**. `LICENSE` diambil dari upstream.
- `NOTICE` + README mencantumkan kredit:
  - NewPipe — TeamNewPipe (https://github.com/TeamNewPipe/NewPipe), inspirasi & lineage.
  - RustyPipe — ThetaDev (https://codeberg.org/ThetaDev/rustypipe), extractor.
- Commit upstream NewPipe tidak disentuh; pekerjaan hanya di branch `rust-rewrite`.
- Commit ditulis atas nama pemilik akun (qrevebnation), tanpa trailer bot.

## 11. Build & toolchain (mesin `kantor`, tanpa sudo)

Semua user-level di `~` (tidak menyentuh paket sistem):

- rustup + stable (rustc 1.100) + target `aarch64-linux-android` — di `~/.cargo`
- Temurin **JDK 17** portable — `~/opt/jdk17`
- Android SDK — `~/Android/Sdk` (cmdline-tools, platform-tools r37,
  build-tools 34.0.0, platforms;android-34, **NDK 26.1.10909125**)
- `cargo-ndk` 4.1.2, `dioxus-cli` (`dx`) — `~/.cargo/bin`
- Env: `~/nanopipe/env.sh` (JAVA_HOME, ANDROID_HOME, ANDROID_NDK_HOME, PATH)

Catatan: `pkg-config` tidak terpasang dan tidak ada sudo → bila ada crate butuh
`pkg-config`, tangani via env/alternatif (mis. `PKG_CONFIG=false`) atau pasang
user-level. Dipantau sebagai risiko.

Pengujian di kantor: tidak ada adb/emulator. Opsi: emulator (perlu KVM) atau pasang APK
dari lokal (ada adb). Diputuskan saat M0/M2.

## 12. Error handling & risiko

| Risiko | Dampak | Mitigasi |
|---|---|---|
| YouTube berubah → extractor rusak | app tak bisa memutar | ikuti update `rustypipe`; pesan error jelas |
| "Sign in to confirm you're not a bot" / PO token | stream gagal | client rotation di rustypipe; retry |
| Cross-compile `rustypipe` (rquickjs/oxc/wreq) ke Android | build gagal | dibuktikan di spike M0 |
| SurfaceView + WebView | video tidak tampil | audio dulu; surface di M5 |
| Dioxus mobile eksperimental | build/bundling rewel | ikuti `dx` guide; versi dipin |
| ToS/legal | akun/legal | disclaimer, sideload saja |
| `pkg-config` absen | build gagal | env override |

Pola error: kegagalan jaringan/extract → state error yang jelas + tombol retry; cache
URL stream dengan expiry; jangan crash, selalu ada fallback pesan.

## 13. Testing

- **Unit (Rust):** `db` (CRUD), `download` (mock HTTP server), pemetaan model.
- **Integrasi:** extractor terhadap 1 URL publik nyata, ditandai `#[ignore]` (butuh net).
- **Manual di device/emulator per milestone:** M2 putar 1 audio sampai keluar suara;
  M4 unduh lalu putar offline; M5 background audio saat layar mati.
- Bukti "nyata": output yang bisa diamati (bunyi keluar, file terputar), bukan sekadar
  status HTTP.

## 14. Milestone

- **M0 — Skeleton & spike.** Workspace + `dx` "hello" jalan di emulator/device +
  bukti `rustypipe` cross-compile ke `aarch64-linux-android`. (Gate keandalan fondasi.)
- **M1 — Search & detail.** `core::extractor` → UI Dioxus: cari, daftar hasil, halaman detail.
- **M2 — Playback (audio).** `PlaybackService.kt` + jembatan JNI; putar audio.
- **M3 — Subscriptions & Feed.** SQLite + tab Home/Subscriptions.
- **M4 — Downloads.** Unduh (progressive + audio-only), tab Downloads, putar offline.
- **M5 — Background audio, video surface, Settings, polish.**

## 15. Pertanyaan terbuka

1. Nama "NanoPipe" final atau diganti?
2. Pengujian di kantor: pasang emulator (KVM) di kantor, atau build di kantor + install
   dari lokal?
3. Repo root: upstream NewPipe dihapus dari branch `rust-rewrite` (murni Rust), atau
   disimpan berdampingan? (Rencana saat ini: murni Rust, `dev` tetap utuh.)
