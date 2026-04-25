//! MusicBrainz API v2 client — **consent-gated**.
//!
//! ## Privacy contract
//!
//! Every public function in this module begins with a call to
//! [`require_consent`](crate::privacy::consent::require_consent) for
//! `Feature::MetadataLookups`. Without consent, the function returns
//! `SonitusError::ConsentRequired` without making any network call.
//!
//! What gets sent: artist + album + track text (no audio, no IPs of users
//! beyond what TLS reveals to musicbrainz.org).

use crate::error::{Result, SonitusError};
use crate::privacy::{AuditLogger, ConsentStore, Feature, TriggerSource, http_client, consent::require_consent};
use serde::Deserialize;
use std::sync::Arc;

const API: &str = "https://musicbrainz.org/ws/2";

/// One artist match returned from MusicBrainz.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtistMatch {
    /// MusicBrainz artist UUID.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Sort name.
    #[serde(rename = "sort-name")]
    pub sort_name: Option<String>,
    /// Match score 0-100.
    pub score: Option<i32>,
}

/// One recording (track) match returned from MusicBrainz.
#[derive(Debug, Clone, Deserialize)]
pub struct RecordingMatch {
    /// MusicBrainz recording UUID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Length in milliseconds.
    pub length: Option<i64>,
    /// Match score 0-100.
    pub score: Option<i32>,
}

#[derive(Deserialize)]
struct ArtistSearch { artists: Vec<ArtistMatch> }

#[derive(Deserialize)]
struct RecordingSearch { recordings: Vec<RecordingMatch> }

/// Search for an artist by name. Returns ranked matches.
pub async fn search_artist(
    consent: &ConsentStore,
    audit: Arc<AuditLogger>,
    name: &str,
) -> Result<Vec<ArtistMatch>> {
    require_consent(consent, Feature::MetadataLookups)?;
    let client = http_client(audit, TriggerSource::MetadataLookup)?;
    let resp: ArtistSearch = client
        .get(format!("{API}/artist"))
        .query(&[("query", name), ("fmt", "json"), ("limit", "10")])
        .send()
        .await
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?
        .error_for_status()
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?
        .json()
        .await
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?;
    Ok(resp.artists)
}

/// Search for a recording by title + artist + album.
pub async fn search_recording(
    consent: &ConsentStore,
    audit: Arc<AuditLogger>,
    title: &str,
    artist: Option<&str>,
    album: Option<&str>,
) -> Result<Vec<RecordingMatch>> {
    require_consent(consent, Feature::MetadataLookups)?;
    let mut query = format!("recording:\"{title}\"");
    if let Some(a) = artist { query.push_str(&format!(" AND artist:\"{a}\"")); }
    if let Some(a) = album { query.push_str(&format!(" AND release:\"{a}\"")); }

    let client = http_client(audit, TriggerSource::MetadataLookup)?;
    let resp: RecordingSearch = client
        .get(format!("{API}/recording"))
        .query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "10")])
        .send()
        .await
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?
        .error_for_status()
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?
        .json()
        .await
        .map_err(|e| SonitusError::Other(anyhow::anyhow!(e)))?;
    Ok(resp.recordings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn lookups_fail_without_consent() {
        let dir = TempDir::new().unwrap();
        let audit = Arc::new(AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap());
        let consent = ConsentStore::ephemeral();
        let result = search_artist(&consent, audit, "Pink Floyd").await;
        assert!(matches!(result, Err(SonitusError::ConsentRequired { .. })));
    }
}
