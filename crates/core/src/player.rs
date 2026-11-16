use crate::extractor::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamInfo {
    pub video_id: String,
    pub title: String,
    pub uploader: String,
    pub thumb_url: Option<String>,
    pub audio_url: String,
    pub audio_mime: String,
    pub video_url: Option<String>,
    pub video_mime: Option<String>,
}

pub fn require_audio<'a>(audio_url: Option<&'a str>, video_id: &str) -> anyhow::Result<&'a str> {
    match audio_url {
        Some(url) if !url.is_empty() => Ok(url),
        _ => anyhow::bail!("no audio stream available for video {video_id}"),
    }
}

impl Client {
    pub async fn streams(&self, video_id: &str) -> anyhow::Result<StreamInfo> {
        let player = self.rp().query().player(video_id).await?;
        let (video, audio) =
            player.select_video_audio_stream(&rustypipe::param::StreamFilter::default());

        let audio_url = require_audio(audio.map(|a| a.url.as_str()), video_id)?.to_string();
        let audio_mime = audio.map(|a| a.mime.clone()).unwrap_or_default();

        let details = &player.details;
        Ok(StreamInfo {
            video_id: details.id.clone(),
            title: details
                .name
                .clone()
                .unwrap_or_else(|| details.id.clone()),
            uploader: details
                .channel_name
                .clone()
                .unwrap_or_else(|| String::from("Unknown")),
            thumb_url: details.thumbnail.last().map(|t| t.url.clone()),
            audio_url,
            audio_mime,
            video_url: video.map(|v| v.url.clone()),
            video_mime: video.map(|v| v.mime.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_audio_is_an_error_not_a_silent_player() {
        let err = require_audio(None, "abc123").unwrap_err().to_string();
        assert!(err.contains("abc123"), "error should name the video: {err}");
        assert!(err.to_lowercase().contains("audio"), "error should mention audio: {err}");
    }

    #[test]
    fn present_audio_is_returned() {
        assert_eq!(
            require_audio(Some("https://x/a.m4a"), "abc").unwrap(),
            "https://x/a.m4a"
        );
    }
}
