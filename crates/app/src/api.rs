use nanopipe_core::{
    extractor::Client, proxy::StreamProxy, VideoDetails, VideoSummary,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

fn candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(d) = std::env::var_os("NANOPIPE_DATA_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(d) = std::env::var_os("TMPDIR") {
        dirs.push(PathBuf::from(d).join("nanopipe"));
    }
    dirs.push(std::env::temp_dir().join("nanopipe"));
    if let Some(d) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(d).join(".nanopipe"));
    }
    dirs
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".nanopipe-write-probe");
    let ok = std::fs::write(&probe, b"ok").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

pub fn pick_writable_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|dir| std::fs::create_dir_all(dir).is_ok() && is_writable(dir))
        .cloned()
}

pub fn data_dir() -> PathBuf {
    pick_writable_dir(&candidate_dirs()).unwrap_or_else(|| std::env::temp_dir().join("nanopipe"))
}

static CLIENT: OnceLock<Arc<Client>> = OnceLock::new();
static ACTIVE_PROXY: Mutex<Option<StreamProxy>> = Mutex::new(None);

async fn client() -> Result<Arc<Client>, String> {
    if let Some(c) = CLIENT.get() {
        return Ok(Arc::clone(c));
    }
    let dir = data_dir();
    let created = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Client::new(&dir).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(Arc::clone(CLIENT.get_or_init(|| Arc::new(created))))
}

pub async fn search(query: String, limit: usize) -> Result<Vec<VideoSummary>, String> {
    let c = client().await?;
    c.search(&query, limit).await.map_err(|e| e.to_string())
}

pub async fn video(id: String) -> Result<VideoDetails, String> {
    let c = client().await?;
    c.video_details(&id).await.map_err(|e| e.to_string())
}

/// Starts playback through the local stream proxy. googlevideo only serves a
/// bounded number of bytes per stream URL, so the player talks to the proxy, which
/// keeps every request inside that budget and re-mints the URL when it runs out.
pub async fn play(id: String) -> Result<String, String> {
    let c = client().await?;
    let mut info = c.streams(&id).await.map_err(|e| e.to_string())?;

    let proxy = StreamProxy::start(Arc::clone(&c), &info).map_err(|e| format!("proxy: {e}"))?;
    info.audio_url = proxy.base_url();
    let title = info.title.clone();

    if let Ok(mut slot) = ACTIVE_PROXY.lock() {
        *slot = Some(proxy);
    }

    crate::media::play(info).map_err(|e| e.to_string())?;
    Ok(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_first_writable_candidate_and_skips_unusable_ones() {
        let tmp = std::env::temp_dir();
        let blocker = tmp.join("nanopipe-test-blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let bad = blocker.join("sub");
        let good = tmp.join("nanopipe-test-good");
        let _ = std::fs::remove_dir_all(&good);

        let picked = pick_writable_dir(&[bad, good.clone()]).unwrap();
        assert_eq!(picked, good);
    }

    #[test]
    fn returns_none_when_no_candidate_is_writable() {
        let tmp = std::env::temp_dir();
        let blocker = tmp.join("nanopipe-test-blocker2");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(pick_writable_dir(&[blocker.join("a"), blocker.join("b")]).is_none());
    }

    #[test]
    fn data_dir_is_actually_writable() {
        let dir = data_dir();
        assert!(!dir.as_os_str().is_empty());
        let probe = dir.join(".nanopipe-probe");
        std::fs::write(&probe, b"ok").expect("data_dir must be writable");
        let _ = std::fs::remove_file(&probe);
    }
}
