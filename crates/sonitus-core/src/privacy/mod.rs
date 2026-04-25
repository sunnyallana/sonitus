//! Privacy enforcement layer.
//!
//! This module is the technical implementation of guarantees ③ (zero
//! telemetry), ④ (credential isolation in logs), and ⑤ (every outbound
//! request is auditable). The four submodules are deliberately small:
//!
//! - [`audit`] — `AuditLogger` writes JSONL records to `audit.log`.
//! - [`middleware`] — A `reqwest_middleware` that records every HTTP request.
//! - [`consent`] — `ConsentStore` for opt-in features (MusicBrainz etc.).
//! - [`redact`] — `tracing` layer that redacts secret-named fields.
//!
//! ## The shared HTTP client
//!
//! [`http_client`] returns a `reqwest_middleware::ClientWithMiddleware` with
//! the [`AuditMiddleware`](middleware::AuditMiddleware) preinstalled. This
//! is the **only** HTTP client constructor exposed by `sonitus-core`. All
//! source providers, metadata lookups, and OAuth flows use it.

pub mod audit;
pub mod consent;
pub mod middleware;
pub mod redact;

use std::sync::Arc;

pub use audit::{AuditEntry, AuditLogger, TriggerSource};
pub use consent::{ConsentStore, Feature};
pub use middleware::AuditMiddleware;
pub use redact::RedactLayer;

use crate::error::Result;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};

/// Build the shared HTTP client with audit middleware installed.
///
/// **Always use this** for outbound HTTP. Constructing a bare
/// `reqwest::Client` bypasses the audit log and violates guarantee ⑤.
pub fn http_client(audit: Arc<AuditLogger>, trigger: TriggerSource) -> Result<ClientWithMiddleware> {
    let inner = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        // No cookie store — we don't want session reuse across destinations.
        .https_only(false) // we explicitly allow http for local NAS / dev servers
        .build()?;

    Ok(ClientBuilder::new(inner)
        .with(AuditMiddleware::new(audit, trigger))
        .build())
}
