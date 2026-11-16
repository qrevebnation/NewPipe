// Run with: cargo test -p nanopipe-core --test network -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn searches_and_fetches_details() {
    let dir = std::env::temp_dir().join("nanopipe-test-net");
    std::fs::create_dir_all(&dir).unwrap();
    let client = nanopipe_core::extractor::Client::new(&dir).unwrap();

    let results = client.search("rust programming", 5).await.unwrap();
    assert!(!results.is_empty(), "search returned no results");

    let v = &results[0];
    assert!(!v.id.is_empty() && !v.title.is_empty());
    let details = client.video_details(&v.id).await.unwrap();
    assert_eq!(details.id, v.id);
    assert!(!details.title.is_empty());
}

#[tokio::test]
#[ignore]
async fn selects_audio_stream() {
    let dir = std::env::temp_dir().join("nanopipe-test-stream");
    std::fs::create_dir_all(&dir).unwrap();
    let client = nanopipe_core::extractor::Client::new(&dir).unwrap();

    let results = client.search("rust programming", 1).await.unwrap();
    let info = client.streams(&results[0].id).await.unwrap();
    assert!(!info.audio_url.is_empty(), "no audio url");
    println!("audio mime: {}", info.audio_mime);
    println!("AUDIO_URL={}", info.audio_url);
    println!("USER_AGENT={}", info.user_agent);
    println!("has_video={}", info.video_url.is_some());
    let xml = info.dash_manifest();
    std::fs::write("/tmp/manifest.xml", &xml).unwrap();
    println!("MANIFEST_LEN={}", xml.len());
}
