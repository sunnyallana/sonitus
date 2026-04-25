//! Structural validation of a [`LibraryMeta`].
//!
//! Catches problems that TOML parsing alone won't: duplicate IDs,
//! orphaned playlist track refs, unknown source kinds, contradictory
//! flags. This is the kind of integrity check we run before saving so
//! the user can't end up with an unloadable file due to manual edits.

use crate::schema::LibraryMeta;
use crate::{MetaError, MetaResult};
use std::collections::HashSet;

/// Validate `meta`. Returns the first problem found, or `Ok(())`.
pub fn validate(meta: &LibraryMeta) -> MetaResult<()> {
    // Source ID uniqueness.
    let mut seen_source_ids: HashSet<&str> = HashSet::new();
    for s in &meta.sources {
        if !seen_source_ids.insert(s.id.as_str()) {
            return Err(MetaError::Invalid(format!("duplicate source id: {}", s.id)));
        }
        validate_source_kind(&s.kind)?;
        validate_source_fields(s)?;
    }

    // Playlist ID uniqueness.
    let mut seen_pl_ids: HashSet<&str> = HashSet::new();
    for p in &meta.playlists {
        if !seen_pl_ids.insert(p.id.as_str()) {
            return Err(MetaError::Invalid(format!("duplicate playlist id: {}", p.id)));
        }
        // Smart playlists shouldn't have track_refs (they're rule-driven).
        if p.is_smart && !p.track_refs.is_empty() {
            return Err(MetaError::Invalid(format!(
                "playlist {} is marked smart but has manual track refs",
                p.id
            )));
        }
        // Track refs must reference known sources.
        for r in &p.track_refs {
            if !seen_source_ids.contains(r.source_id.as_str()) {
                return Err(MetaError::Invalid(format!(
                    "playlist {} references unknown source {}",
                    p.id, r.source_id
                )));
            }
        }
    }

    // Privacy: telemetry is always off in Sonitus.
    if meta.privacy.telemetry_enabled {
        return Err(MetaError::Invalid(
            "telemetry_enabled must be false (zero telemetry guarantee)".into(),
        ));
    }
    if meta.privacy.crash_reporting_enabled {
        return Err(MetaError::Invalid(
            "crash_reporting_enabled must be false (zero telemetry guarantee)".into(),
        ));
    }

    Ok(())
}

const KNOWN_KINDS: &[&str] = &[
    "local", "google_drive", "s3", "smb", "http", "dropbox", "onedrive",
];

fn validate_source_kind(kind: &str) -> MetaResult<()> {
    if !KNOWN_KINDS.contains(&kind) {
        return Err(MetaError::Invalid(format!("unknown source kind: {kind}")));
    }
    Ok(())
}

fn validate_source_fields(s: &crate::schema::SourceDef) -> MetaResult<()> {
    match s.kind.as_str() {
        "local" => {
            if s.path.is_none() {
                return Err(MetaError::Invalid(format!("local source {} requires path", s.id)));
            }
        }
        "google_drive" | "dropbox" | "onedrive" => {} // creds in encrypted DB
        "s3" => {
            if s.bucket.is_none() {
                return Err(MetaError::Invalid(format!("s3 source {} requires bucket", s.id)));
            }
        }
        "smb" => {
            if s.host.is_none() || s.share.is_none() {
                return Err(MetaError::Invalid(format!("smb source {} requires host and share", s.id)));
            }
        }
        "http" => {
            if s.url.is_none() {
                return Err(MetaError::Invalid(format!("http source {} requires url", s.id)));
            }
        }
        _ => {} // already caught by validate_source_kind
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    fn fresh_source(id: &str, kind: &str) -> SourceDef {
        let mut s = SourceDef {
            id: id.into(),
            name: id.into(),
            kind: kind.into(),
            enabled: true,
            path: None,
            root_folder: None,
            bucket: None,
            region: None,
            endpoint_url: None,
            host: None,
            share: None,
            base_path: None,
            url: None,
            tenant: None,
        };
        match kind {
            "local" => s.path = Some("/Music".into()),
            "s3" => s.bucket = Some("b".into()),
            "smb" => { s.host = Some("h".into()); s.share = Some("share".into()); }
            "http" => s.url = Some("https://example".into()),
            _ => {}
        }
        s
    }

    #[test]
    fn validates_default_meta() {
        validate(&LibraryMeta::default()).unwrap();
    }

    #[test]
    fn rejects_duplicate_source_ids() {
        let mut m = LibraryMeta::default();
        m.sources.push(fresh_source("s1", "local"));
        m.sources.push(fresh_source("s1", "http"));
        assert!(validate(&m).is_err());
    }

    #[test]
    fn rejects_unknown_source_kind() {
        let mut m = LibraryMeta::default();
        m.sources.push(fresh_source("s1", "ipfs"));
        assert!(validate(&m).is_err());
    }

    #[test]
    fn rejects_local_without_path() {
        let mut m = LibraryMeta::default();
        let mut s = fresh_source("s1", "local");
        s.path = None;
        m.sources.push(s);
        assert!(validate(&m).is_err());
    }

    #[test]
    fn rejects_smart_playlist_with_manual_refs() {
        let mut m = LibraryMeta::default();
        m.sources.push(fresh_source("s1", "local"));
        let now = chrono::Utc::now();
        m.playlists.push(PlaylistDef {
            id: "p1".into(),
            name: "X".into(),
            description: None,
            created_at: now,
            updated_at: now,
            is_smart: true,
            smart_rules: None,
            track_refs: vec![TrackRef { source_id: "s1".into(), path: "/a.mp3".into() }],
        });
        assert!(validate(&m).is_err());
    }

    #[test]
    fn rejects_telemetry_enabled() {
        let mut m = LibraryMeta::default();
        m.privacy.telemetry_enabled = true;
        assert!(validate(&m).is_err());
    }
}
