//! Localhost OAuth2 callback listener.
//!
//! When a user starts an OAuth flow (Google Drive, Dropbox, OneDrive),
//! we open their browser to the provider's authorization URL with
//! `redirect_uri = http://127.0.0.1:8888/callback`. After they grant
//! access, the provider redirects the browser to that URL with the
//! authorization code in the query string.
//!
//! This module spins up a tiny tokio TCP server that:
//!
//! 1. Accepts the first GET to `/callback`.
//! 2. Parses `?code=...&state=...` from the request line.
//! 3. Verifies `state` matches the one given to [`begin_oauth_flow`].
//! 4. Returns a small "you can close this tab now" HTML response.
//! 5. Resolves the awaiting future with `(code, state)`.
//!
//! ## Why a raw TCP socket and not `axum` / `hyper`?
//!
//! We're handling exactly **one** request, by design — single OAuth
//! callback per flow, then the listener shuts down. A 60-line socket
//! reader is auditable; a web framework dependency is not.

use crate::error::{Result, SonitusError};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Default port the listener binds. If you need another port (e.g. tests),
/// use [`listen_on`] directly.
pub const DEFAULT_PORT: u16 = 8888;

/// What was extracted from the OAuth callback URL.
#[derive(Debug, Clone)]
pub struct CallbackResult {
    /// Authorization code to exchange for tokens.
    pub code: String,
    /// CSRF state echoed back from the provider — caller must verify match.
    pub state: String,
}

/// Listen on `127.0.0.1:8888/callback` for the next inbound OAuth redirect.
///
/// Returns when the redirect arrives. Times out after `timeout`. Useful
/// for desktop platforms; on iOS/Android the OS provides its own
/// `ASWebAuthenticationSession` flow.
pub async fn listen_for_callback(timeout: Duration) -> Result<CallbackResult> {
    listen_on(DEFAULT_PORT, timeout).await
}

/// Same as [`listen_for_callback`], but bind a specific port.
pub async fn listen_on(port: u16, timeout: Duration) -> Result<CallbackResult> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| SonitusError::OAuth(format!("listener bind {addr}: {e}")))?;

    let accept = async {
        loop {
            let (stream, _peer) = listener
                .accept()
                .await
                .map_err(|e| SonitusError::OAuth(format!("accept: {e}")))?;
            match handle_connection(stream).await {
                Ok(Some(result)) => return Ok::<_, SonitusError>(result),
                Ok(None) => continue, // wrong path, etc. — keep listening.
                Err(e) => {
                    tracing::warn!(error = %e, "oauth callback connection error; ignoring");
                    continue;
                }
            }
        }
    };

    tokio::time::timeout(timeout, accept)
        .await
        .map_err(|_| SonitusError::OAuth("OAuth callback timed out".into()))?
}

/// Read one HTTP request from the stream. If it's a GET to `/callback`
/// with a `code` query parameter, return the parsed result. Otherwise
/// reply with 404 and return `Ok(None)`.
async fn handle_connection(mut stream: TcpStream) -> Result<Option<CallbackResult>> {
    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| SonitusError::OAuth(format!("read: {e}")))?;
    if n == 0 {
        return Ok(None);
    }
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse only the request line.
    let request_line = request.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("");

    if method != "GET" || !target.starts_with("/callback") {
        respond(&mut stream, 404, "Not found").await?;
        return Ok(None);
    }

    // Extract code + state from the query string.
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut error: Option<String> = None;

    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else { continue; };
        let v = url_decode(v);
        match k {
            "code" => code = Some(v),
            "state" => state = Some(v),
            "error" => error = Some(v),
            "error_description" => error = error.or(Some(v)),
            _ => {}
        }
    }

    if let Some(err) = error {
        respond(&mut stream, 400, &format!("OAuth error: {err}")).await?;
        return Err(SonitusError::OAuth(format!("provider returned error: {err}")));
    }

    match (code, state) {
        (Some(code), Some(state)) => {
            respond(&mut stream, 200, SUCCESS_BODY).await?;
            Ok(Some(CallbackResult { code, state }))
        }
        _ => {
            respond(&mut stream, 400, "Missing code or state parameter").await?;
            Ok(None)
        }
    }
}

const SUCCESS_BODY: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Sonitus — Connected</title>
<style>
  body { font-family: system-ui, sans-serif; background: #0a0a0c; color: #ececef; display: grid; place-items: center; height: 100vh; margin: 0; }
  main { text-align: center; padding: 2rem; }
  h1 { color: #1DB954; }
  p  { color: #9b9ba1; }
</style>
</head>
<body>
<main>
  <h1>✓ Connected</h1>
  <p>You can close this tab and return to Sonitus.</p>
</main>
</body>
</html>
"#;

async fn respond(stream: &mut TcpStream, status: u16, body: &str) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\
         \r\n{body}",
        len = body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|e| SonitusError::OAuth(format!("write: {e}")))?;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
    Ok(())
}

/// Minimal `application/x-www-form-urlencoded` decoder. Supports `+` →
/// space and `%XX` percent-decoding. Strict enough for our purposes;
/// invalid sequences pass through verbatim.
fn url_decode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => { out.push(' '); i += 1; }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            b => { out.push(b as char); i += 1; }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_decode_handles_plus_and_percent() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("a%2Bb"), "a+b");
        assert_eq!(url_decode("ya29.a0%2FBxC"), "ya29.a0/BxC");
        assert_eq!(url_decode("nothing-special"), "nothing-special");
    }

    #[tokio::test]
    async fn callback_listener_extracts_code_and_state() {
        // Bind to an ephemeral port so tests can run in parallel.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let server_task = tokio::spawn(async move {
            listen_on(port, Duration::from_secs(5)).await
        });

        // Tiny client that hits /callback.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let req = "GET /callback?code=4%2F0aSecret&state=xyz HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        client.write_all(req.as_bytes()).await.unwrap();
        let mut resp = Vec::new();
        let _ = client.read_to_end(&mut resp).await;

        let result = server_task.await.unwrap().unwrap();
        assert_eq!(result.code, "4/0aSecret");
        assert_eq!(result.state, "xyz");
    }

    #[tokio::test]
    async fn callback_listener_rejects_other_paths() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let server_task = tokio::spawn(async move {
            listen_on(port, Duration::from_secs(5)).await
        });

        tokio::time::sleep(Duration::from_millis(100)).await;
        // First request: wrong path; should be ignored.
        let mut c1 = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        c1.write_all(b"GET /favicon.ico HTTP/1.1\r\n\r\n").await.unwrap();
        let mut buf = Vec::new();
        let _ = c1.read_to_end(&mut buf).await;

        // Second request: correct path; resolves the future.
        let mut c2 = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        c2.write_all(b"GET /callback?code=ABC&state=def HTTP/1.1\r\n\r\n").await.unwrap();
        let mut buf2 = Vec::new();
        let _ = c2.read_to_end(&mut buf2).await;

        let result = server_task.await.unwrap().unwrap();
        assert_eq!(result.code, "ABC");
    }

    #[tokio::test]
    async fn callback_listener_times_out() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let r = listen_on(port, Duration::from_millis(150)).await;
        assert!(matches!(r, Err(SonitusError::OAuth(_))));
    }
}
