use crate::api;
use crate::media;
use dioxus::prelude::*;
use nanopipe_core::{duration_label, VideoDetails};

pub fn format_count(n: u64) -> String {
    if n >= 1_000_000 {
        scaled(n as f64 / 1_000_000.0, "M")
    } else if n >= 1_000 {
        scaled(n as f64 / 1_000.0, "K")
    } else {
        n.to_string()
    }
}

fn scaled(v: f64, suffix: &str) -> String {
    let s = format!("{v:.1}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}{suffix}")
}

fn start_load(
    mut details: Signal<Option<VideoDetails>>,
    mut error: Signal<Option<String>>,
    mut loading: Signal<bool>,
    video_id: String,
) {
    details.set(None);
    error.set(None);
    loading.set(true);
    spawn(async move {
        match api::video(video_id).await {
            Ok(d) => details.set(Some(d)),
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    });
}

fn start_play(
    video_id: String,
    mut now_playing: Signal<Option<String>>,
    mut is_playing: Signal<bool>,
    mut play_error: Signal<Option<String>>,
    mut play_busy: Signal<bool>,
) {
    play_busy.set(true);
    play_error.set(None);
    spawn(async move {
        match api::streams(video_id).await {
            Ok(info) => {
                let title = info.title.clone();
                match media::play(info) {
                    Ok(()) => {
                        now_playing.set(Some(title));
                        is_playing.set(true);
                    }
                    Err(e) => play_error.set(Some(e)),
                }
            }
            Err(e) => play_error.set(Some(e)),
        }
        play_busy.set(false);
    });
}

#[component]
pub fn DetailScreen(
    video_id: String,
    now_playing: Signal<Option<String>>,
    is_playing: Signal<bool>,
    on_back: EventHandler<()>,
) -> Element {
    let details = use_signal(|| None::<VideoDetails>);
    let error = use_signal(|| None::<String>);
    let loading = use_signal(|| true);
    let play_error = use_signal(|| None::<String>);
    let play_busy = use_signal(|| false);

    let initial_id = video_id.clone();
    use_future(move || {
        let id = initial_id.clone();
        async move {
            start_load(details, error, loading, id);
        }
    });

    rsx! {
        div { class: "screen",
            div { class: "topbar",
                button { class: "btn ghost", onclick: move |_| on_back.call(()), "Kembali" }
            }
            if loading() {
                div { class: "muted", "Memuat..." }
            }
            if let Some(msg) = error() {
                div { class: "error",
                    span { "{msg}" }
                    button {
                        class: "btn",
                        onclick: {
                            let id = video_id.clone();
                            move |_| start_load(details, error, loading, id.clone())
                        },
                        "Coba lagi"
                    }
                }
            }
            if let Some(d) = details() {
                div { class: "detail",
                    if let Some(url) = d.thumb_url.clone() {
                        img { class: "hero", src: "{url}", alt: "" }
                    }
                    div { class: "dtitle", "{d.title}" }
                    button {
                        class: "btn",
                        onclick: {
                            let id = video_id.clone();
                            move |_| start_play(id.clone(), now_playing, is_playing, play_error, play_busy)
                        },
                        if play_busy() { "Menyiapkan..." } else { "Putar audio" }
                    }
                    if let Some(msg) = play_error() {
                        div { class: "error", span { "{msg}" } }
                    }
                    div { class: "sub", "{d.uploader}" }
                    div { class: "sub",
                        span { "{duration_label(d.is_live, d.duration_s)}" }
                        if let Some(vc) = d.view_count {
                            span { " {format_count(vc)} views" }
                        }
                    }
                    div { class: "desc", "{d.description}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_view_counts() {
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1500), "1.5K");
        assert_eq!(format_count(2_000_000), "2M");
    }
}
