use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::extractor::Client;
use crate::player::StreamInfo;

/// Largest byte window we ask upstream for in one request. googlevideo hands out a
/// per-URL budget of roughly 1.2 MB before it starts answering 403, so every window
/// is kept comfortably below that and the URL is re-minted when it runs out.
pub const WINDOW: u64 = 1_000_000;

/// Local HTTP endpoint the player points at. It forwards each byte range to
/// googlevideo the way NewPipe does (range/rn parameters, identity encoding) and
/// re-mints the stream URL when the CDN answers 403.
pub struct StreamProxy {
    port: u16,
}

struct Session {
    client: Arc<Client>,
    handle: tokio::runtime::Handle,
    video_id: String,
    url: String,
    user_agent: String,
    total_size: Option<u64>,
    rn: AtomicU64,
}

impl Session {
    fn next_upstream(&self, start: u64, end: u64) -> String {
        build_upstream_url(&self.url, start, end, self.rn.fetch_add(1, Ordering::Relaxed))
    }

    /// Mint a fresh stream URL for the same video. googlevideo's budget is per URL,
    /// so this is what lets playback continue past the first megabyte.
    fn refresh(&mut self) -> anyhow::Result<()> {
        // The rustypipe client owns an HTTP connection pool bound to the runtime that
        // created it, so the refresh has to run on that same runtime.
        let info = self.handle.block_on(self.client.streams(&self.video_id))?;
        let changed = info.audio_url != self.url;
        println!(
            "[proxy] refresh changed={changed} old_len={} new_len={}",
            self.url.len(),
            info.audio_url.len()
        );
        self.url = info.audio_url;
        self.user_agent = info.user_agent;
        self.rn = AtomicU64::new(0);
        Ok(())
    }
}

/// Appends the `range` and `rn` query parameters the way NewPipe's
/// YoutubeHttpDataSource does. `base` already carries a query string.
pub fn build_upstream_url(base: &str, start: u64, end: u64, rn: u64) -> String {
    format!("{base}&range={start}-{end}&rn={rn}")
}

/// Clamp a requested range to one window, so a single upstream request can never
/// cross the CDN's byte budget.
pub fn clamp_range(start: u64, requested_end: Option<u64>) -> (u64, u64) {
    let max_end = start.saturating_add(WINDOW - 1);
    match requested_end {
        Some(end) if end < max_end => (start, end),
        _ => (start, max_end),
    }
}

fn parse_range(header: Option<&str>) -> (u64, Option<u64>) {
    let Some(value) = header else {
        return (0, None);
    };
    let Some(rest) = value.strip_prefix("bytes=") else {
        return (0, None);
    };
    let mut parts = rest.split('-');
    let start = parts.next().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
    let end = parts
        .next()
        .and_then(|s| s.trim().parse::<u64>().ok());
    (start, end)
}

impl StreamProxy {
    /// Binds an ephemeral port on loopback and starts serving in the background.
    pub fn start(client: Arc<Client>, info: &StreamInfo) -> anyhow::Result<Self> {
        let server = tiny_http::Server::http("127.0.0.1:0")
            .map_err(|e| anyhow::anyhow!("proxy bind failed: {e}"))?;
        let port = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| anyhow::anyhow!("proxy has no IP address"))?
            .port();

        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| anyhow::anyhow!("StreamProxy must be started inside a tokio runtime"))?;

        let session = Session {
            client,
            handle,
            video_id: info.video_id.clone(),
            url: info.audio_url.clone(),
            user_agent: info.user_agent.clone(),
            total_size: info.audio_size,
            rn: AtomicU64::new(0),
        };

        std::thread::spawn(move || serve(server, session));
        Ok(Self { port })
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/stream", self.port)
    }
}

fn serve(server: tiny_http::Server, mut session: Session) {
    let http = reqwest::blocking::Client::builder()
        .build()
        .expect("http client");

    for request in server.incoming_requests() {
        let range_header = request
            .headers()
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case("range"))
            .map(|h| h.value.as_str().to_string());

        let (start, requested_end) = parse_range(range_header.as_deref());
        let (start, end) = clamp_range(start, requested_end);

        // First attempt, plus one retry after re-minting the URL if the CDN says
        // the budget for this URL is exhausted.
        let mut response = None;
        for attempt in 0..2 {
            let upstream = session.next_upstream(start, end);
            let result = http
                .get(&upstream)
                .header("user-agent", &session.user_agent)
                .header("accept-encoding", "identity")
                .header("te", "trailers")
                .send();
            match result {
                Ok(resp) => {
                    let code = resp.status().as_u16();
                    if (code == 403 || code == 410) && attempt == 0 {
                        match session.refresh() {
                            Ok(()) => continue,
                            Err(e) => {
                                println!("[proxy] refresh failed: {e}");
                                response = Some(resp);
                                break;
                            }
                        }
                    }
                    response = Some(resp);
                    break;
                }
                Err(_) => break,
            }
        }

        let Some(resp) = response else {
            let _ = request.respond(tiny_http::Response::empty(502));
            continue;
        };

        let mut code = resp.status().as_u16();
        let mut content_range = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let mut body = Vec::new();
        let mut reader = resp;
        let _ = reader.read_to_end(&mut body);

        // googlevideo answers the `range` parameter with 200 and only the requested
        // bytes, but a range client expects 206 + Content-Range.
        if code == 200 && !body.is_empty() {
            code = 206;
            let last = start + body.len() as u64 - 1;
            content_range = Some(match session.total_size {
                Some(total) => format!("bytes {start}-{last}/{total}"),
                None => format!("bytes {start}-{last}/*"),
            });
        }

        let mut headers = Vec::new();
        if let Some(cr) = content_range {
            if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Range"[..], cr.as_bytes()) {
                headers.push(h);
            }
        }
        if let Some(ct) = content_type {
            if let Ok(h) = tiny_http::Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()) {
                headers.push(h);
            }
        }
        if let Ok(h) = tiny_http::Header::from_bytes(&b"Accept-Ranges"[..], &b"bytes"[..]) {
            headers.push(h);
        }

        let response = tiny_http::Response::new(
            tiny_http::StatusCode(code),
            headers,
            std::io::Cursor::new(body),
            None,
            None,
        );
        let _ = request.respond(response);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_url_carries_range_and_request_number() {
        let url = build_upstream_url("https://x/videoplayback?a=1", 0, 999, 3);
        assert!(url.ends_with("&range=0-999&rn=3"), "{url}");
    }

    #[test]
    fn window_is_clamped_to_the_budget() {
        assert_eq!(clamp_range(0, Some(10)), (0, 10));
        assert_eq!(clamp_range(0, None), (0, WINDOW - 1));
        assert_eq!(clamp_range(0, Some(WINDOW * 3)), (0, WINDOW - 1));
        assert_eq!(clamp_range(5_000_000, None), (5_000_000, 5_999_999));
    }

    #[test]
    fn parses_range_headers() {
        assert_eq!(parse_range(None), (0, None));
        assert_eq!(parse_range(Some("bytes=0-1023")), (0, Some(1023)));
        assert_eq!(parse_range(Some("bytes=500-")), (500, None));
        assert_eq!(parse_range(Some("garbage")), (0, None));
    }
}
