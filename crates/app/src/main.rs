mod api;
mod media;
mod screens;

use dioxus::prelude::*;
use nanopipe_core::VideoSummary;
use screens::search::Status;

const CSS: &str = r#"
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body { margin: 0; background: #111318; color: #e6e8ee; font-family: system-ui, sans-serif; }
.app { padding: 12px; }
.searchbar { display: flex; gap: 8px; margin-bottom: 12px; }
.search-input { flex: 1; min-width: 0; background: #1b1f27; border: 1px solid #2a303b; border-radius: 8px; color: inherit; padding: 10px 12px; font-size: 15px; }
.btn { background: #3b82f6; color: #fff; border: 0; border-radius: 8px; padding: 10px 14px; font-size: 15px; flex: none; }
.btn.ghost { background: #1b1f27; color: #e6e8ee; }
.row { display: flex; gap: 10px; align-items: center; padding: 10px 0; border-bottom: 1px solid #232833; }
.thumb { width: 96px; height: 54px; object-fit: cover; border-radius: 6px; background: #1b1f27; flex: none; }
.meta { flex: 1; min-width: 0; }
.title { font-size: 14px; font-weight: 600; overflow: hidden; word-break: break-word; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
.sub { font-size: 12px; color: #98a0b3; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dur { font-size: 12px; color: #98a0b3; flex: none; }
.muted { color: #98a0b3; padding: 8px 0; }
.error { color: #f87171; padding: 8px 0; display: flex; gap: 8px; align-items: center; }
.topbar { padding: 4px 0 12px; }
.detail { display: flex; flex-direction: column; gap: 8px; }
.hero { width: 100%; aspect-ratio: 16 / 9; object-fit: cover; border-radius: 8px; background: #1b1f27; }
.dtitle { font-size: 17px; font-weight: 700; word-break: break-word; }
.miniplayer { display: flex; gap: 8px; align-items: center; padding: 8px 0; border-top: 1px solid #232833; }
.minititle { flex: 1; min-width: 0; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.btn.small { padding: 6px 10px; font-size: 13px; }
.desc { font-size: 13px; color: #c7cddb; white-space: pre-wrap; word-break: break-word; max-height: 40vh; overflow-y: auto; }
"#;

#[derive(Clone, PartialEq)]
enum Nav {
    Search,
    Detail(String),
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut nav = use_signal(|| Nav::Search);
    let query = use_signal(String::new);
    let results = use_signal(Vec::<VideoSummary>::new);
    let status = use_signal(Status::default);
    let generation = use_signal(|| 0u64);
    let mut now_playing = use_signal(|| None::<String>);
    let mut is_playing = use_signal(|| false);

    rsx! {
        style { dangerous_inner_html: "{CSS}" }
        div { class: "app",
            match nav() {
                Nav::Search => rsx! {
                    screens::SearchScreen {
                        query,
                        results,
                        status,
                        generation,
                        on_open: move |id: String| nav.set(Nav::Detail(id)),
                    }
                },
                Nav::Detail(id) => rsx! {
                    screens::DetailScreen {
                        video_id: id,
                        now_playing,
                        is_playing,
                        on_back: move |_| nav.set(Nav::Search),
                    }
                },
            }
            if let Some(title) = now_playing() {
                div { class: "miniplayer",
                    span { class: "minititle", "{title}" }
                    button {
                        class: "btn ghost small",
                        onclick: move |_| {
                            let next = !is_playing();
                            let result = if next { crate::media::resume() } else { crate::media::pause() };
                            if result.is_ok() {
                                is_playing.set(next);
                            }
                        },
                        if is_playing() { "Jeda" } else { "Lanjut" }
                    }
                    button {
                        class: "btn ghost small",
                        onclick: move |_| {
                            let _ = crate::media::stop();
                            is_playing.set(false);
                            now_playing.set(None);
                        },
                        "Stop"
                    }
                }
            }
        }
    }
}
