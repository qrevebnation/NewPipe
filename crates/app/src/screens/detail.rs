use crate::api;
use dioxus::prelude::*;
use nanopipe_core::{format_duration, VideoDetails};

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

#[component]
pub fn DetailScreen(video_id: String, on_back: EventHandler<()>) -> Element {
    let mut details = use_signal(|| None::<VideoDetails>);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);

    use_future(move || {
        let id = video_id.clone();
        async move {
            match api::video(id).await {
                Ok(d) => details.set(Some(d)),
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
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
                div { class: "error", span { "{msg}" } }
            }
            if let Some(d) = details() {
                div { class: "detail",
                    img { class: "hero", src: "{d.thumb_url.clone().unwrap_or_default()}" }
                    div { class: "dtitle", "{d.title}" }
                    div { class: "sub", "{d.uploader}" }
                    div { class: "sub",
                        if d.is_live { "LIVE" } else { "{format_duration(d.duration_s)}" }
                        if let Some(vc) = d.view_count {
                            span { " - {format_count(vc)} views" }
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
