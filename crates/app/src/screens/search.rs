use crate::api;
use dioxus::prelude::*;
use nanopipe_core::{format_duration, VideoSummary};

#[derive(Clone, PartialEq, Default)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Error(String),
}

fn start_search(mut results: Signal<Vec<VideoSummary>>, mut status: Signal<Status>, query: String) {
    let q = query.trim().to_string();
    if q.is_empty() {
        results.set(Vec::new());
        status.set(Status::Idle);
        return;
    }
    status.set(Status::Loading);
    spawn(async move {
        match api::search(q, 20).await {
            Ok(items) => {
                results.set(items);
                status.set(Status::Idle);
            }
            Err(e) => status.set(Status::Error(e)),
        }
    });
}

#[component]
pub fn SearchScreen(on_open: EventHandler<String>) -> Element {
    let mut query = use_signal(String::new);
    let results = use_signal(Vec::<VideoSummary>::new);
    let status = use_signal(Status::default);

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
                            start_search(results, status, query());
                        }
                    },
                }
                button {
                    class: "btn",
                    onclick: move |_| start_search(results, status, query()),
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
                        onclick: move |_| start_search(results, status, query()),
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
                        img { class: "thumb", src: "{v.thumb_url.clone().unwrap_or_default()}" }
                        div { class: "meta",
                            div { class: "title", "{v.title}" }
                            div { class: "sub", "{v.uploader}" }
                        }
                        div { class: "dur",
                            if v.is_live { "LIVE" } else { "{format_duration(v.duration_s)}" }
                        }
                    }
                }
            }
        }
    }
}
