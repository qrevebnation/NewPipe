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
