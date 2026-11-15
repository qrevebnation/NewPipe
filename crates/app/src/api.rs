use nanopipe_core::{extractor::Client, VideoDetails, VideoSummary};
use std::path::PathBuf;
use std::sync::OnceLock;

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NANOPIPE_DATA_DIR") {
        return PathBuf::from(dir);
    }
    std::env::temp_dir().join("nanopipe")
}

static CLIENT: OnceLock<Client> = OnceLock::new();

fn client() -> Result<&'static Client, String> {
    if let Some(c) = CLIENT.get() {
        return Ok(c);
    }
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let c = Client::new(&dir).map_err(|e| e.to_string())?;
    Ok(CLIENT.get_or_init(|| c))
}

pub async fn search(query: String, limit: usize) -> Result<Vec<VideoSummary>, String> {
    let c = client()?;
    c.search(&query, limit).await.map_err(|e| e.to_string())
}

pub async fn video(id: String) -> Result<VideoDetails, String> {
    let c = client()?;
    c.video_details(&id).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn data_dir_is_non_empty() {
        assert!(!super::data_dir().as_os_str().is_empty());
    }
}
