//! Integration test: the OAuth callback listener accepts a redirect and
//! resolves with the expected code/state.

use sonitus_core::sources::oauth_callback::{self, CallbackResult};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn pick_free_port() -> u16 {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

#[tokio::test]
async fn full_callback_flow() {
    let port = pick_free_port().await;
    let server = tokio::spawn(async move {
        oauth_callback::listen_on(port, Duration::from_secs(5)).await
    });

    // Wait for the server to bind.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let req = "GET /callback?code=4%2F0aSecret&state=csrf_value HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
    client.write_all(req.as_bytes()).await.unwrap();
    let mut resp = Vec::new();
    let _ = client.read_to_end(&mut resp).await;
    let resp = String::from_utf8_lossy(&resp);
    assert!(resp.starts_with("HTTP/1.1 200"));
    assert!(resp.contains("Connected"));

    let result: CallbackResult = server.await.unwrap().unwrap();
    assert_eq!(result.code, "4/0aSecret");
    assert_eq!(result.state, "csrf_value");
}

#[tokio::test]
async fn provider_error_surfaces_as_oauth_error() {
    let port = pick_free_port().await;
    let server = tokio::spawn(async move {
        oauth_callback::listen_on(port, Duration::from_secs(5)).await
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let req = "GET /callback?error=access_denied HTTP/1.1\r\n\r\n";
    client.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let _ = client.read_to_end(&mut buf).await;

    let r = server.await.unwrap();
    assert!(r.is_err());
}
