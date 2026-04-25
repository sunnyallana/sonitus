//! Integration test: every reqwest call through `http_client` is recorded
//! in the audit log, with secret-named query params redacted.

use sonitus_core::privacy::{AuditLogger, TriggerSource, http_client};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn middleware_records_failure_with_redaction() {
    let dir = TempDir::new().unwrap();
    let logger = Arc::new(AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap());
    let client = http_client(logger.clone(), TriggerSource::UserAction).unwrap();

    // RFC 5737 TEST-NET — guaranteed unreachable.
    let _ = client
        .get("http://192.0.2.42/probe?token=ya29.SECRET&user=alice")
        .timeout(std::time::Duration::from_millis(150))
        .send()
        .await;

    let entries = logger.read_entries().unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.method, "GET");
    assert_eq!(e.dest, "192.0.2.42");
    assert!(e.path.contains("token=[REDACTED]"));
    assert!(e.path.contains("user=alice"));
    assert!(!e.path.contains("ya29.SECRET"));
}
