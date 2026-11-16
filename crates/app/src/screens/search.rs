use crate::api;
use dioxus::prelude::*;
use nanopipe_core::{duration_label, VideoSummary};

#[derive(Clone, PartialEq, Default)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Error(String),
}

pub fn start_search(
    mut results: Signal<Vec<VideoSummary>>,
    mut status: Signal<Status>,
    mut generation: Signal<u64>,
    query: String,
) {
    let q = query.trim().to_string();
    let gen = generation() + 1;
    generation.set(gen);

    results.set(Vec::new());
    if q.is_empty() {
        status.set(Status::Idle);
        return;
    }
    status.set(Status::Loading);

    spawn(async move {
        let outcome = api::search(q, 20).await;
        if generation() != gen {
            return;
        }
        match outcome {
            Ok(items) => {
                results.set(items);
                status.set(Status::Idle);
            }
            Err(e) => {
                results.set(Vec::new());
                status.set(Status::Error(e));
            }
        }
    });
}

#[component]
pub fn SearchScreen(
    query: Signal<String>,
    results: Signal<Vec<VideoSummary>>,
    status: Signal<Status>,
    generation: Signal<u64>,
    on_open: EventHandler<String>,
) -> Element {
    let mut query = query;

    rsx! {
        div { class: "screen",
            div { class: "searchbar",
                input {
                    class: "search-input",
                    placeholder: "Cari video",
                    value: query(),
                    oninput: move |e| query.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter {
                            start_search(results, status, generation, query());
                        }
                    },
                }
                button {
                    class: "btn",
                    onclick: move |_| start_search(results, status, generation, query()),
                    "Cari"
                }
            }

            if let Status::Loading = status() {
                div { class: "muted", "Memuat..." }
            }
            if let Status::Error(msg) = status() {
                div { class: "error",
                    span { "{msg}" }
                    button {
                        class: "btn",
                        onclick: move |_| start_search(results, status, generation, query()),
                        "Coba lagi"
                    }
                }
            }

            div { class: "list",
                for v in results().iter() {
                    div {
                        key: "{v.id}",
                        class: "row",
                        onclick: {
                            let id = v.id.clone();
                            move |_| on_open.call(id.clone())
                        },
                        if let Some(url) = v.thumb_url.clone() {
                            img { class: "thumb", src: "{url}", alt: "" }
                        }
                        div { class: "meta",
                            div { class: "title", "{v.title}" }
                            div { class: "sub", "{v.uploader}" }
                        }
                        div { class: "dur", "{duration_label(v.is_live, v.duration_s)}" }
                    }
                }
            }
        }
    }
}
