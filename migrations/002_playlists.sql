-- ─────────────────────────────────────────────────────────────────────────
-- 002_playlists.sql
-- Playlists (manual + smart) and playlist membership
-- ─────────────────────────────────────────────────────────────────────────

INSERT INTO schema_version (version) VALUES (2);

CREATE TABLE playlists (
    id                  TEXT    PRIMARY KEY,
    name                TEXT    NOT NULL,
    description         TEXT,
    cover_art           BLOB,
    is_smart            INTEGER NOT NULL DEFAULT 0,
    smart_rules         TEXT,
    track_count         INTEGER NOT NULL DEFAULT 0,
    total_duration_ms   INTEGER NOT NULL DEFAULT 0,
    created_at          INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at          INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_playlists_name ON playlists(name);

CREATE TABLE playlist_tracks (
    playlist_id  TEXT    NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id     TEXT    NOT NULL REFERENCES tracks(id)    ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    added_at     INTEGER NOT NULL DEFAULT (unixepoch()),
    added_by     TEXT    NOT NULL DEFAULT 'user',
    PRIMARY KEY (playlist_id, track_id)
);
CREATE INDEX idx_playlist_tracks_pos ON playlist_tracks(playlist_id, position);
