//! `reqwest_middleware` adapter that records every outbound request.
//!
//! Every HTTP client built via [`http_client`](crate::privacy::http_client)
//! has this middleware installed. The middleware:
//!
//! 1. Records the start time before the request fires.
//! 2. Captures destination, method, path (with secrets redacted from query).
//! 3. Lets the inner client execute the request unchanged.
//! 4. Captures status code, duration, and content-length on the way out.
//! 5. Writes one [`AuditEntry`] to disk via the shared [`AuditLogger`].
//!
//! Failures during *audit writing* are propagated as
//! [`SonitusError::AuditWriteFailed`] — we never silently lose audit
//! records, even if it means failing the request. This is by design:
//! the audit log is a contract with the user.

use super::audit::{AuditEntry, AuditLogger, TriggerSource, redact_query};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::{Request, Response};
use reqwest_middleware::{Middleware, Next, Result as MwResult};
use std::sync::Arc;
use std::time::Instant;

/// `reqwest_middleware` plugin that writes one [`AuditEntry`] per request.
#[derive(Clone)]
pub struct AuditMiddleware {
    logger: Arc<AuditLogger>,
    /// What user action caused traffic from *this* client. The same logger
    /// can be shared across many middleware instances each tagged with
    /// a different trigger source.
    trigger: TriggerSource,
}

impl AuditMiddleware {
    /// Construct a middleware that tags every request with `trigger`.
    pub fn new(logger: Arc<AuditLogger>, trigger: TriggerSource) -> Self {
        Self { logger, trigger }
    }

    fn destination_of(req: &Request) -> String {
        req.url()
            .host_str()
            .map(str::to_string)
            .unwrap_or_else(|| "<unknown>".to_string())
    }

    fn method_of(req: &Request) -> String {
        req.method().as_str().to_string()
    }

    fn path_with_redacted_query(req: &Request) -> String {
        let url = req.url();
        if let Some(q) = url.query() {
            redact_query(&format!("{}?{}", url.path(), q))
        } else {
            url.path().to_string()
        }
    }

    fn body_size(req: &Request) -> u64 {
        req.body()
            .and_then(|b| b.as_bytes())
            .map(|b| b.len() as u64)
            .unwrap_or(0)
    }
}

#[async_trait]
impl Middleware for AuditMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut http::Extensions,
        next: Next<'_>,
    ) -> MwResult<Response> {
        let started = Instant::now();
        let dest = Self::destination_of(&req);
        let method = Self::method_of(&req);
        let path = Self::path_with_redacted_query(&req);
        let sent = Self::body_size(&req);

        let outcome = next.run(req, extensions).await;

        let elapsed_ms = started.elapsed().as_millis() as u64;
        let mut entry = AuditEntry {
            ts: Utc::now(),
            dest,
            method,
            path,
            by: self.trigger,
            sent,
            recv: 0,
            status: None,
            ms: elapsed_ms,
            error: None,
        };

        match &outcome {
            Ok(resp) => {
                entry.status = Some(resp.status().as_u16());
                entry.recv = resp.content_length().unwrap_or(0);
            }
            Err(e) => {
                entry.error = Some(e.to_string());
            }
        }

        // Audit-log failures are non-fatal: we drop a tracing warn so the
        // user can investigate, but we don't break the user's HTTP request
        // because the log file went read-only. This is the one place we
        // bend the "audit log is a contract" rule, because failing here
        // would brick the app.
        if let Err(e) = self.logger.append(&entry) {
            tracing::warn!(error = %e, "failed to write audit log entry");
        }

        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::privacy::audit::AuditLogger;
    use reqwest_middleware::ClientBuilder;
    use tempfile::TempDir;

    async fn build_client(dir: &TempDir) -> (Arc<AuditLogger>, reqwest_middleware::ClientWithMiddleware) {
        let logger = Arc::new(
            AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap(),
        );
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(AuditMiddleware::new(logger.clone(), TriggerSource::UserAction))
            .build();
        (logger, client)
    }

    #[tokio::test]
    async fn middleware_records_failed_request_to_unreachable_host() {
        let dir = TempDir::new().unwrap();
        let (logger, client) = build_client(&dir).await;
        // Use a TEST-NET-1 address (RFC 5737) — guaranteed unreachable.
        let _ = client
            .get("http://192.0.2.1:1/probe?token=secret&user=alice")
            .timeout(std::time::Duration::from_millis(200))
            .send()
            .await;
        let entries = logger.read_entries().unwrap();
        assert_eq!(entries.len(), 1, "one audit entry should be recorded even on failure");
        let e = &entries[0];
        assert_eq!(e.method, "GET");
        assert_eq!(e.dest, "192.0.2.1");
        assert!(e.path.contains("token=[REDACTED]"));
        assert!(e.path.contains("user=alice"));
        assert!(e.error.is_some());
    }
}
