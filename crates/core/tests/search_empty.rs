#[tokio::test]
async fn empty_query_returns_no_results_without_network() {
    let dir = std::env::temp_dir().join("nanopipe-test-empty");
    std::fs::create_dir_all(&dir).unwrap();
    let client = nanopipe_core::extractor::Client::new(&dir).unwrap();
    assert!(client.search("   ", 20).await.unwrap().is_empty());
}
