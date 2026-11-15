use crate::models::{VideoDetails, VideoSummary};
use rustypipe::model::richtext::ToPlaintext;
use std::path::Path;

pub struct Client {
    rp: rustypipe::client::RustyPipe,
}

impl Client {
    pub fn new(storage_dir: impl AsRef<Path>) -> anyhow::Result<Self> {
        let rp = rustypipe::client::RustyPipe::builder()
            .storage_dir(storage_dir.as_ref().to_string_lossy().to_string())
            .build()?;
        Ok(Self { rp })
    }

    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<VideoSummary>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        let res = self
            .rp
            .query()
            .search::<rustypipe::model::VideoItem, _>(q)
            .await?;
        let out = res
            .items
            .items
            .into_iter()
            .take(limit)
            .map(summary_from_item)
            .collect::<Vec<_>>();
        Ok(out)
    }

    pub async fn video_details(&self, video_id: &str) -> anyhow::Result<VideoDetails> {
        let v = self.rp.query().video_details(video_id).await?;
        Ok(VideoDetails {
            id: v.id.clone(),
            title: v.name,
            uploader: v.channel.name,
            duration_s: None,
            thumb_url: Some(video_thumb_url(&v.id)),
            is_live: v.is_live,
            description: v.description.to_plaintext().trim().to_string(),
            view_count: Some(v.view_count),
        })
    }
}

fn video_thumb_url(video_id: &str) -> String {
    format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

fn thumb_url(thumbs: &[rustypipe::model::Thumbnail]) -> Option<String> {
    thumbs.last().map(|t| t.url.clone())
}

fn summary_from_item(v: rustypipe::model::VideoItem) -> VideoSummary {
    VideoSummary::new(
        v.id,
        v.name,
        v.channel.map(|c| c.name),
        v.duration,
        thumb_url(&v.thumbnail),
        v.is_live,
    )
}
