use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        div { style: "font-family: system-ui, sans-serif; padding: 2rem;",
            h1 { "NanoPipe" }
            p { "Rust Android client - M0 spike" }
        }
    }
}
