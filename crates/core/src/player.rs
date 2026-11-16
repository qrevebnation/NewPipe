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
    pub audio_codec: String,
    pub audio_bitrate: u32,
    /// Inclusive byte range of the segment index, as YouTube reports it.
    pub audio_index_range: Option<(u32, u32)>,
    /// Inclusive byte range of the initialization segment.
    pub audio_init_range: Option<(u32, u32)>,
    pub duration_ms: Option<u32>,
    /// Size of the audio file in bytes, as reported by YouTube.
    pub audio_size: Option<u64>,
    pub video_url: Option<String>,
    pub video_mime: Option<String>,
    /// User-Agent of the client the stream URLs were issued for. YouTube returns
    /// HTTP 403 for these URLs when they are fetched with a different agent.
    pub user_agent: String,
}

pub fn require_audio<'a>(audio_url: Option<&'a str>, video_id: &str) -> anyhow::Result<&'a str> {
    match audio_url {
        Some(url) if !url.is_empty() => Ok(url),
        _ => anyhow::bail!("no audio stream available for video {video_id}"),
    }
}

fn range_pair(r: Option<std::ops::Range<u32>>) -> Option<(u32, u32)> {
    r.map(|r| (r.start, r.end))
}

impl Client {
    pub async fn streams(&self, video_id: &str) -> anyhow::Result<StreamInfo> {
        let player = self.rp().query().player(video_id).await?;
        let (video, audio) =
            player.select_video_audio_stream(&rustypipe::param::StreamFilter::default());

        let audio_url = require_audio(audio.map(|a| a.url.as_str()), video_id)?.to_string();
        let audio_mime = audio.map(|a| a.mime.clone()).unwrap_or_default();
        let audio_codec = audio
            .map(|a| format!("{:?}", a.codec).to_lowercase())
            .unwrap_or_default();
        let audio_bitrate = audio.map(|a| a.bitrate).unwrap_or(0);
        let audio_index_range = range_pair(audio.and_then(|a| a.index_range.clone()));
        let audio_init_range = range_pair(audio.and_then(|a| a.init_range.clone()));
        let duration_ms = audio.and_then(|a| a.duration_ms);
        let audio_size = audio.map(|a| a.size);
        let user_agent = self.rp().query().user_agent(player.client_type).to_string();

        let details = &player.details;
        Ok(StreamInfo {
            video_id: details.id.clone(),
            title: details.name.clone().unwrap_or_else(|| details.id.clone()),
            uploader: details
                .channel_name
                .clone()
                .unwrap_or_else(|| String::from("Unknown")),
            thumb_url: details.thumbnail.last().map(|t| t.url.clone()),
            audio_url,
            audio_mime,
            audio_codec,
            audio_bitrate,
            audio_index_range,
            audio_init_range,
            duration_ms,
            audio_size,
            video_url: video.map(|v| v.url.clone()),
            video_mime: video.map(|v| v.mime.clone()),
            user_agent,
        })
    }
}

/// DASH isoff-on-demand manifest for the single audio file. ExoPlayer plays this
/// with bounded Range requests (initialization segment + index + segments), which
/// is the only shape googlevideo serves: open-ended ranges answer 302 then 403.
impl StreamInfo {
    pub fn dash_manifest(&self) -> String {
        let duration = match self.duration_ms {
            Some(ms) => format!(
                r#" mediaPresentationDuration="PT{}.{:03}S""#,
                ms / 1000,
                ms % 1000
            ),
            None => String::new(),
        };
        let segment_base = match (self.audio_index_range, self.audio_init_range) {
            (Some((index_start, index_end)), Some((init_start, init_end))) => format!(
                r#"<SegmentBase indexRange="{index_start}-{index_end}" timescale="1000"><Initialization range="{init_start}-{init_end}"/></SegmentBase>"#
            ),
            _ => String::new(),
        };

        format!(
            concat!(
                r#"<?xml version="1.0" encoding="UTF-8"?>"#,
                r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static"{duration} minBufferTime="PT1.5S" profiles="urn:mpeg:dash:profile:isoff-on-demand:2011">"#,
                r#"<Period><AdaptationSet mimeType="{mime}" segmentAlignment="true" startWithSAP="1" subsegmentAlignment="true" subsegmentStartsWithSAP="1">"#,
                r#"<Representation id="0" bandwidth="{bandwidth}" codecs="{codecs}">"#,
                r#"<BaseURL>{url}</BaseURL>{segment_base}"#,
                r#"</Representation></AdaptationSet></Period></MPD>"#,
            ),
            duration = duration,
            mime = xml_escape(base_mime(&self.audio_mime)),
            bandwidth = self.audio_bitrate,
            codecs = xml_escape(&self.audio_codec),
            url = xml_escape(&self.audio_url),
            segment_base = segment_base,
        )
    }
}

/// rustypipe reports MIME as e.g. `audio/webm; codecs="opus"`. DASH wants the base
/// type in `mimeType` and the codec separately, and the embedded quotes would
/// otherwise break the XML attribute.
fn base_mime(mime: &str) -> &str {
    mime.split(';').next().unwrap_or(mime).trim()
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn sample(duration_ms: Option<u32>) -> StreamInfo {
    StreamInfo {
        video_id: String::from("vid1"),
        title: String::from("A & B"),
        uploader: String::from("chan"),
        thumb_url: None,
        audio_url: String::from("https://rr1.googlevideo.com/videoplayback?itag=251&mime=audio%2Fwebm"),
        audio_mime: String::from("audio/webm; codecs=\"opus\""),
        audio_codec: String::from("opus"),
        audio_bitrate: 128_000,
        audio_index_range: Some((200, 399)),
        audio_init_range: Some((0, 199)),
        duration_ms,
        audio_size: Some(2_806_189),
        video_url: None,
        video_mime: None,
        user_agent: String::from("agent"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_audio_is_an_error_not_a_silent_player() {
        let err = require_audio(None, "abc123").unwrap_err().to_string();
        assert!(err.contains("abc123"), "error should name the video: {err}");
        assert!(
            err.to_lowercase().contains("audio"),
            "error should mention audio: {err}"
        );
    }

    #[test]
    fn present_audio_is_returned() {
        assert_eq!(
            require_audio(Some("https://x/a.m4a"), "abc").unwrap(),
            "https://x/a.m4a"
        );
    }

    #[test]
    fn dash_manifest_is_iso_on_demand_with_bounded_ranges() {
        let xml = sample(Some(50_349)).dash_manifest();

        assert!(xml.contains("urn:mpeg:dash:profile:isoff-on-demand:2011"), "{xml}");
        assert!(xml.contains("<BaseURL>https://rr1.googlevideo.com/videoplayback?itag=251&amp;mime=audio%2Fwebm</BaseURL>"), "url must be escaped: {xml}");
        assert!(xml.contains(r#"indexRange="200-399""#), "{xml}");
        assert!(xml.contains(r#"<Initialization range="0-199"/>"#), "{xml}");
        assert!(
            xml.contains(r#"mimeType="audio/webm" segmentAlignment"#),
            "mimeType must be the base MIME only (rustypipe appends ; codecs=\"...\"): {xml}"
        );
        assert!(xml.contains(r#"codecs="opus""#), "{xml}");
        assert!(xml.contains(r#"bandwidth="128000""#), "{xml}");
        assert!(xml.contains(r#"mediaPresentationDuration="PT50.349S""#), "{xml}");
    }

    #[test]
    fn dash_manifest_without_duration_is_still_valid() {
        let xml = sample(None).dash_manifest();
        assert!(!xml.contains("mediaPresentationDuration"), "{xml}");
        assert!(xml.contains("<Period>"), "{xml}");
    }

    /// Diagnostic: prints the audio URL a given client yields, so the per-URL byte
    /// budget can be measured per client from the host.
    #[tokio::test]
    #[ignore]
    async fn probe_client_stream() {
        use rustypipe::client::ClientType;

        let ctype = match std::env::var("NP_CLIENT").unwrap_or_default().as_str() {
            "Android" => ClientType::Android,
            "Desktop" => ClientType::Desktop,
            "Mobile" => ClientType::Mobile,
            "Tv" => ClientType::Tv,
            _ => ClientType::Ios,
        };
        let dir = std::env::temp_dir().join("nanopipe-probe");
        std::fs::create_dir_all(&dir).unwrap();
        let client = Client::new(&dir).unwrap();

        let results = client.search("rust programming", 1).await.unwrap();
        let id = results[0].id.clone();
        let player = match client
            .rp()
            .query()
            .player_from_clients(&id, &[ctype])
            .await
        {
            Ok(player) => player,
            Err(e) => {
                println!("PROBE_ERROR={e}");
                return;
            }
        };
        let (_, audio) =
            player.select_video_audio_stream(&rustypipe::param::StreamFilter::default());
        let audio = audio.expect("no audio stream");

        println!("PROBE_URL={}", audio.url);
        println!("PROBE_UA={}", client.rp().query().user_agent(ctype));
        println!("PROBE_ITAG={}", audio.itag);
        println!("PROBE_MIME={}", audio.mime);
    }
}
