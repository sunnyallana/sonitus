//! User-managed consent for opt-in features.
//!
//! Sonitus's default state is "no outbound traffic except what the user
//! has explicitly initiated" — playback, downloads, and source operations.
//! A small set of features make additional outbound calls and require
//! the user to opt in:
//!
//! - **MetadataLookups** — Query MusicBrainz to enrich tags. Sends
//!   artist + track + album text to `musicbrainz.org`.
//! - **AcoustIDFingerprinting** — Send audio fingerprints to `acoustid.org`
//!   for unidentified files. Even more privacy-impactful than MetadataLookups.
//!
//! The toggles are stored in `consent.toml` next to `config.toml`. Default
//! is **always off**. The settings UI shows a clear disclosure for each.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::RwLock;

/// Opt-in features that require explicit user consent before making
/// outbound calls beyond what the user directly initiates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Feature {
    /// MusicBrainz tag lookups (artist/album/track text → MusicBrainz API).
    MetadataLookups,
    /// AcoustID audio fingerprinting (audio fingerprint → AcoustID API).
    AcoustidFingerprinting,
}

impl Feature {
    /// Human-readable name for the settings UI.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::MetadataLookups => "MusicBrainz tag lookups",
            Self::AcoustidFingerprinting => "AcoustID audio fingerprinting",
        }
    }

    /// Plain-language disclosure shown in the consent UI.
    pub fn disclosure(&self) -> &'static str {
        match self {
            Self::MetadataLookups => {
                "When enabled, Sonitus sends artist, album, and track names to \
                 musicbrainz.org to fetch additional metadata (release dates, \
                 cover art, related artists). Audio data is never sent."
            }
            Self::AcoustidFingerprinting => {
                "When enabled, Sonitus computes a 'fingerprint' of audio files \
                 (a small numeric summary, not the audio itself) and sends it \
                 to acoustid.org to identify untagged tracks. Audio bytes never \
                 leave your device, but the fingerprint can be linked to known \
                 recordings."
            }
        }
    }

    /// What gets sent over the wire. Used in audit log explanations.
    pub fn what_is_sent(&self) -> &'static str {
        match self {
            Self::MetadataLookups => "Artist, album, and track text strings.",
            Self::AcoustidFingerprinting => "Numeric audio fingerprints (Chromaprint).",
        }
    }
}

/// Persistent consent state. Cheaply cloneable; underlying state is
/// reference-counted and lock-protected.
#[derive(Clone)]
pub struct ConsentStore {
    inner: Arc<RwLock<ConsentRecord>>,
    path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ConsentRecord {
    metadata_lookups: bool,
    acoustid_fingerprinting: bool,
}

impl ConsentStore {
    /// Load consent state from disk. Returns the default (everything off)
    /// if the file does not yet exist.
    pub fn load(path: PathBuf) -> Result<Self> {
        let record: ConsentRecord = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            toml::from_str(&text)?
        } else {
            ConsentRecord::default()
        };
        Ok(Self {
            inner: Arc::new(RwLock::new(record)),
            path,
        })
    }

    /// Build an in-memory store with everything off. Used in tests.
    pub fn ephemeral() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ConsentRecord::default())),
            path: PathBuf::from("/dev/null"),
        }
    }

    /// True iff the user has consented to `feature`.
    pub fn is_enabled(&self, feature: Feature) -> bool {
        let r = self.inner.read();
        match feature {
            Feature::MetadataLookups => r.metadata_lookups,
            Feature::AcoustidFingerprinting => r.acoustid_fingerprinting,
        }
    }

    /// Set the consent state for `feature`, persisting to disk.
    pub fn set(&self, feature: Feature, enabled: bool) -> Result<()> {
        {
            let mut w = self.inner.write();
            match feature {
                Feature::MetadataLookups => w.metadata_lookups = enabled,
                Feature::AcoustidFingerprinting => w.acoustid_fingerprinting = enabled,
            }
        }
        self.persist()
    }

    fn persist(&self) -> Result<()> {
        if self.path == PathBuf::from("/dev/null") {
            return Ok(()); // ephemeral
        }
        let text = {
            let r = self.inner.read();
            toml::to_string_pretty(&*r)?
        };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut tmp = tempfile::NamedTempFile::new_in(
            self.path.parent().unwrap_or(Path::new(".")),
        )?;
        std::io::Write::write_all(&mut tmp, text.as_bytes())?;
        tmp.as_file().sync_all()?;
        tmp.persist(&self.path)
            .map_err(|e| crate::error::SonitusError::Io(e.error))?;
        Ok(())
    }

    /// Path the store reads/writes from. Used by the privacy UI.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Helper that returns an error if the consent for `feature` is not granted.
/// Used at the entry points of `metadata::musicbrainz` and `metadata::acoustid`.
pub fn require_consent(store: &ConsentStore, feature: Feature) -> Result<()> {
    if !store.is_enabled(feature) {
        return Err(crate::error::SonitusError::ConsentRequired {
            feature: match feature {
                Feature::MetadataLookups => "metadata_lookups",
                Feature::AcoustidFingerprinting => "acoustid_fingerprinting",
            },
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn default_state_has_everything_disabled() {
        let store = ConsentStore::ephemeral();
        assert!(!store.is_enabled(Feature::MetadataLookups));
        assert!(!store.is_enabled(Feature::AcoustidFingerprinting));
    }

    #[test]
    fn set_then_get_round_trips() {
        let store = ConsentStore::ephemeral();
        store.set(Feature::MetadataLookups, true).unwrap();
        assert!(store.is_enabled(Feature::MetadataLookups));
        assert!(!store.is_enabled(Feature::AcoustidFingerprinting));
    }

    #[test]
    fn persisted_state_survives_reload() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("consent.toml");
        let s1 = ConsentStore::load(path.clone()).unwrap();
        s1.set(Feature::MetadataLookups, true).unwrap();
        let s2 = ConsentStore::load(path).unwrap();
        assert!(s2.is_enabled(Feature::MetadataLookups));
    }

    #[test]
    fn require_consent_errors_when_disabled() {
        let store = ConsentStore::ephemeral();
        let r = require_consent(&store, Feature::MetadataLookups);
        assert!(matches!(r, Err(crate::error::SonitusError::ConsentRequired { .. })));
    }

    #[test]
    fn require_consent_passes_when_enabled() {
        let store = ConsentStore::ephemeral();
        store.set(Feature::MetadataLookups, true).unwrap();
        require_consent(&store, Feature::MetadataLookups).unwrap();
    }

    #[test]
    fn disclosure_text_explains_what_is_sent() {
        for f in [Feature::MetadataLookups, Feature::AcoustidFingerprinting] {
            let d = f.disclosure();
            // Sanity: disclosure mentions sending and the destination.
            assert!(d.contains("sends") || d.contains("send"), "feature {f:?}: disclosure must mention sending");
            assert!(!d.is_empty());
        }
    }
}
