//! AcoustID fingerprint matching — **consent-gated**.
//!
//! Sends a Chromaprint fingerprint (a numeric audio summary, not the
//! audio itself) to the AcoustID web service to identify a recording.
//! Stronger privacy disclosure than MusicBrainz: while we never send
//! the audio, the *fingerprint* uniquely identifies the recording, so
//! AcoustID can in principle correlate which user is listening to what.
//!
//! Default state: **disabled**. Even with `MetadataLookups` enabled,
//! AcoustID requires a separate `AcoustidFingerprinting` consent.

use crate::error::{Result, SonitusError};
use crate::privacy::{AuditLogger, ConsentStore, Feature, TriggerSource, http_client, consent::require_consent};
use serde::Deserialize;
use std::sync::Arc;

const API: &str = "https://api.acoustid.org/v2";

/// Result of an AcoustID lookup.
#[derive(Debug, Clone, Deserialize)]
pub struct LookupResult {
    /// AcoustID UUID.
    pub id: String,
    /// Match score 0.0..=1.0.
    pub score: f64,
}

#[derive(Deserialize)]
struct LookupResponse {
    status: String,
    results: Vec<LookupResult>,
}

/// Look up a Chromaprint fingerprint against AcoustID.
///
/// `fingerprint` is a base64-encoded Chromaprint output produced
/// elsewhere; `duration_secs` is the audio's length in seconds.
pub async fn lookup(
    consent: &ConsentStore,
    audit: Arc<AuditLogger>,
    api_key: &str,
    fingerprint: &str,
    duration_secs: u32,
) -> Result<Vec<LookupResult>> {
    require_consent(consent, Feature::AcoustidFingerprinting)?;
    let client = http_client(audit, TriggerSource::MetadataLookup)?;
    let resp: LookupResponse = client
        .get(format!("{API}/lookup"))
        .query(&[
            ("client", api_key),
            ("duration", &duration_secs.to_string()),
            ("fingerprint", fingerprint),
            ("format", "json"),
            ("meta", "recordings+releasegroups"),
        ])
        .send()
        .await
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?
        .error_for_status()
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?
        .json()
        .await
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?;
    if resp.status != "ok" {
        return Err(SonitusError::Source { kind: "acoustid", message: format!("status={}", resp.status) });
    }
    Ok(resp.results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn lookup_fails_without_consent() {
        let dir = TempDir::new().unwrap();
        let audit = Arc::new(AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap());
        let consent = ConsentStore::ephemeral();
        let r = lookup(&consent, audit, "key", "AQADtMm...", 240).await;
        assert!(matches!(r, Err(SonitusError::ConsentRequired { .. })));
    }
}
