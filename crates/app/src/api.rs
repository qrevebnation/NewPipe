use nanopipe_core::{extractor::Client, StreamInfo, VideoDetails, VideoSummary};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

static CLIENT: OnceLock<Client> = OnceLock::new();

async fn client() -> Result<&'static Client, String> {
    if let Some(c) = CLIENT.get() {
        return Ok(c);
    }
    let dir = data_dir();
    let c = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        Client::new(&dir).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(CLIENT.get_or_init(|| c))
}

pub async fn search(query: String, limit: usize) -> Result<Vec<VideoSummary>, String> {
    let c = client().await?;
    c.search(&query, limit).await.map_err(|e| e.to_string())
}

pub async fn streams(id: String) -> Result<StreamInfo, String> {
    let c = client().await?;
    c.streams(&id).await.map_err(|e| e.to_string())
}

pub async fn video(id: String) -> Result<VideoDetails, String> {
    let c = client().await?;
    c.video_details(&id).await.map_err(|e| e.to_string())
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
