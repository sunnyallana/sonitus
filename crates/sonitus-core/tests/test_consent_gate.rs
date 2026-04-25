//! Integration test: metadata lookups refuse to fire without consent.

use sonitus_core::error::SonitusError;
use sonitus_core::metadata;
use sonitus_core::privacy::{AuditLogger, ConsentStore, Feature};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn musicbrainz_refuses_without_consent() {
    let dir = TempDir::new().unwrap();
    let audit = Arc::new(AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap());
    let consent = ConsentStore::ephemeral();
    let r = metadata::musicbrainz::search_artist(&consent, audit.clone(), "Pink Floyd").await;
    assert!(matches!(r, Err(SonitusError::ConsentRequired { feature: "metadata_lookups" })));

    // After enabling, the call attempts a network request — won't succeed
    // in CI but won't be ConsentRequired.
    consent.set(Feature::MetadataLookups, true).unwrap();
    let r2 = metadata::musicbrainz::search_artist(&consent, audit, "Pink Floyd").await;
    match r2 {
        Err(SonitusError::ConsentRequired { .. }) => panic!("should not be consent error"),
        _ => (),
    }
}

#[tokio::test]
async fn acoustid_refuses_without_separate_consent() {
    let dir = TempDir::new().unwrap();
    let audit = Arc::new(AuditLogger::new(dir.path().join("audit.log"), 5, 3).unwrap());
    let consent = ConsentStore::ephemeral();

    // MetadataLookups consent does NOT imply AcoustID consent.
    consent.set(Feature::MetadataLookups, true).unwrap();
    let r = metadata::acoustid::lookup(&consent, audit, "key", "fp", 240).await;
    assert!(matches!(r, Err(SonitusError::ConsentRequired { feature: "acoustid_fingerprinting" })));
}
