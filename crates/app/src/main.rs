const CSS: &str = r#"
:root { color-scheme: dark; }
* { box-sizing: border-box; }
body { margin: 0; background: #111318; color: #e6e8ee; font-family: system-ui, sans-serif; }
.app { padding: 12px; }
.searchbar { display: flex; gap: 8px; margin-bottom: 12px; }
.search-input { flex: 1; background: #1b1f27; border: 1px solid #2a303b; border-radius: 8px; color: inherit; padding: 10px 12px; font-size: 15px; }
.btn { background: #3b82f6; color: #fff; border: 0; border-radius: 8px; padding: 10px 14px; font-size: 15px; }
.row { display: flex; gap: 10px; align-items: center; padding: 10px 0; border-bottom: 1px solid #232833; }
.thumb { width: 96px; height: 54px; object-fit: cover; border-radius: 6px; background: #1b1f27; flex: none; }
.meta { flex: 1; min-width: 0; }
.title { font-size: 14px; font-weight: 600; overflow: hidden; }
.sub { font-size: 12px; color: #98a0b3; }
.dur { font-size: 12px; color: #98a0b3; flex: none; }
.muted { color: #98a0b3; padding: 8px 0; }
.error { color: #f87171; padding: 8px 0; display: flex; gap: 8px; align-items: center; }
"#;

mod api;
mod screens;

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        style { dangerous_inner_html: "{CSS}" }
        div { class: "app",
            screens::SearchScreen { on_open: move |_id: String| {} }
        }
    }
}
