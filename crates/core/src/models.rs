use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoSummary {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration_s: Option<u32>,
    pub thumb_url: Option<String>,
    pub is_live: bool,
}

impl VideoSummary {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        uploader: Option<String>,
        duration_s: Option<u32>,
        thumb_url: Option<String>,
        is_live: bool,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            uploader: uploader.unwrap_or_else(|| String::from("Unknown")),
            duration_s,
            thumb_url,
            is_live,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoDetails {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration_s: Option<u32>,
    pub thumb_url: Option<String>,
    pub is_live: bool,
    pub description: String,
    pub view_count: Option<u64>,
}

pub fn format_duration(secs: Option<u32>) -> String {
    match secs {
        None => String::new(),
        Some(s) => {
            let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
            if h > 0 {
                format!("{h}:{m:02}:{sec:02}")
            } else {
                format!("{m}:{sec:02}")
            }
        }
    }
}

pub fn duration_label(is_live: bool, secs: Option<u32>) -> String {
    if is_live {
        String::from("LIVE")
    } else {
        format_duration(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_short_and_long_durations() {
        assert_eq!(format_duration(Some(7)), "0:07");
        assert_eq!(format_duration(Some(187)), "3:07");
        assert_eq!(format_duration(Some(3723)), "1:02:03");
        assert_eq!(format_duration(Some(0)), "0:00");
    }

    #[test]
    fn missing_duration_is_empty_not_zero() {
        assert_eq!(format_duration(None), "");
    }

    #[test]
    fn summary_defaults_uploader_and_keeps_unicode() {
        let s = VideoSummary::new("abc", "café ☕ 日本語", None, Some(90), None, false);
        assert_eq!(s.uploader, "Unknown");
        assert_eq!(s.title, "café ☕ 日本語");
    }

    #[test]
    fn duration_label_covers_live_and_missing_duration() {
        assert_eq!(duration_label(true, None), "LIVE");
        assert_eq!(duration_label(true, Some(120)), "LIVE");
        assert_eq!(duration_label(false, None), "");
        assert_eq!(duration_label(false, Some(187)), "3:07");
    }
}
