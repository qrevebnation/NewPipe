// Proves the localhost proxy serves past googlevideo's per-URL byte budget.
// Run with: cargo test -p nanopipe-core --test proxy_network -- --ignored --nocapture
use std::sync::Arc;

#[test]
#[ignore]
fn proxy_serves_past_the_per_url_budget() {
    let dir = std::env::temp_dir().join("nanopipe-proxy");
    std::fs::create_dir_all(&dir).unwrap();
    let client = Arc::new(nanopipe_core::extractor::Client::new(&dir).unwrap());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (proxy, info) = rt.block_on(async {
        let results = client.search("rust in 100 seconds", 1).await.unwrap();
        let info = client.streams(&results[0].id).await.unwrap();
        let proxy = nanopipe_core::proxy::StreamProxy::start(client.clone(), &info).unwrap();
        (proxy, info)
    });
    println!("video={} url_len={}", info.video_id, info.audio_url.len());
    let base = proxy.base_url();
    println!("proxy={base}");

    let http = reqwest::blocking::Client::new();
    let mut start = 0u64;
    let mut served = 0u64;
    for i in 0..10u64 {
        let end = start + 204_799;
        let resp = http
            .get(&base)
            .header("range", format!("bytes={start}-{end}"))
            .send()
            .unwrap();
        let code = resp.status().as_u16();
        let body = resp.bytes().unwrap();
        println!("window {i}: {start}-{end} -> {code}, {} bytes", body.len());
        assert_eq!(code, 206, "window {i} returned {code}");
        assert!(!body.is_empty(), "window {i} was empty");
        served += body.len() as u64;
        start = end + 1;
    }

    println!("served total {served} bytes");
    assert!(
        served > 1_600_000,
        "proxy should serve past the ~1.2MB per-URL budget, served {served}"
    );
}
