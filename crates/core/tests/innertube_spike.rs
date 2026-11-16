// Spike: does innertube-rs yield stream URLs that are not capped at ~1.2MB?
// Run with: cargo test -p nanopipe-core --test innertube_spike -- --ignored --nocapture
use innertube_rs::{FormatFilter, FormatType, Innertube, QualityPreference};

#[tokio::test]
#[ignore]
async fn resolves_audio_stream() {
    let yt = Innertube::new().await.expect("client");
    let results = yt.search("rust programming", None).await.expect("search");
    println!("items={}", results.items.len());

    let video_id = results
        .items
        .iter()
        .find_map(|item| match item {
            innertube_rs::SearchResultItem::Video(v) => Some(v.video_id.clone()),
            _ => None,
        })
        .expect("no video in results");
    println!("VIDEO_ID={video_id}");

    let info = yt.get_video_info(&video_id).await.expect("video info");
    if let Some(details) = info.video_details.as_ref() {
        println!("TITLE={}", details.title);
        println!("DURATION={}", details.length_seconds);
    }

    let filter = FormatFilter {
        format_type: FormatType::AudioOnly,
        quality: QualityPreference::Highest,
        container: None,
    };
    let url = yt.get_stream_url(&video_id, &filter).await.expect("stream url");
    println!("STREAM_URL={url}");
}
