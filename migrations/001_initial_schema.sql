-- ─────────────────────────────────────────────────────────────────────────
-- 001_initial_schema.sql
-- Foundational tables: schema_version, sources, artists, albums, tracks
-- ─────────────────────────────────────────────────────────────────────────

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA synchronous = NORMAL;

CREATE TABLE schema_version (
    version     INTEGER NOT NULL,
    applied_at  INTEGER NOT NULL DEFAULT (unixepoch())
);
INSERT INTO schema_version (version) VALUES (1);

-- ── Sources ──────────────────────────────────────────────────────────────
-- One row per configured library source. The `credentials_enc` blob is
-- XChaCha20-Poly1305 encrypted at the application layer.
CREATE TABLE sources (
    id                TEXT    PRIMARY KEY,
    name              TEXT    NOT NULL,
    kind              TEXT    NOT NULL CHECK (kind IN (
                                'local','google_drive','s3','smb','http','dropbox','onedrive'
                              )),
    config_json       TEXT    NOT NULL,
    credentials_enc   BLOB,
    scan_state        TEXT    NOT NULL DEFAULT 'idle'
                              CHECK (scan_state IN ('idle','scanning','error')),
    last_scanned_at   INTEGER,
    last_error        TEXT,
    track_count       INTEGER NOT NULL DEFAULT 0,
    enabled           INTEGER NOT NULL DEFAULT 1,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch())
);

-- ── Artists ──────────────────────────────────────────────────────────────
CREATE TABLE artists (
    id                TEXT    PRIMARY KEY,
    name              TEXT    NOT NULL,
    sort_name         TEXT    NOT NULL,
    musicbrainz_id    TEXT    UNIQUE,
    bio               TEXT,
    image_url         TEXT,
    image_blob        BLOB,
    play_count        INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_artists_sort_name ON artists(sort_name);
CREATE INDEX idx_artists_name      ON artists(name);

-- ── Albums ───────────────────────────────────────────────────────────────
CREATE TABLE albums (
    id                TEXT    PRIMARY KEY,
    title             TEXT    NOT NULL,
    artist_id         TEXT    REFERENCES artists(id) ON DELETE SET NULL,
    year              INTEGER,
    genre             TEXT,
    cover_art_blob    BLOB,
    cover_art_url     TEXT,
    cover_art_hash    TEXT,
    musicbrainz_id    TEXT    UNIQUE,
    total_tracks      INTEGER,
    disc_count        INTEGER NOT NULL DEFAULT 1,
    play_count        INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_albums_artist_id ON albums(artist_id);
CREATE INDEX idx_albums_title     ON albums(title);
CREATE INDEX idx_albums_year      ON albums(year);
CREATE INDEX idx_albums_genre     ON albums(genre);

-- ── Tracks ───────────────────────────────────────────────────────────────
CREATE TABLE tracks (
    id                TEXT    PRIMARY KEY,
    title             TEXT    NOT NULL,
    artist_id         TEXT    REFERENCES artists(id)  ON DELETE SET NULL,
    album_artist_id   TEXT    REFERENCES artists(id)  ON DELETE SET NULL,
    album_id          TEXT    REFERENCES albums(id)   ON DELETE SET NULL,
    source_id         TEXT    REFERENCES sources(id)  ON DELETE CASCADE,
    remote_path       TEXT    NOT NULL,
    local_cache_path  TEXT,
    duration_ms       INTEGER,
    track_number      INTEGER,
    disc_number       INTEGER NOT NULL DEFAULT 1,
    genre             TEXT,
    year              INTEGER,
    bpm               REAL,
    replay_gain_track REAL,
    replay_gain_album REAL,
    file_size_bytes   INTEGER,
    format            TEXT    CHECK (format IN ('mp3','flac','ogg','aac','wav','opus','alac','aiff') OR format IS NULL),
    bitrate_kbps      INTEGER,
    sample_rate_hz    INTEGER,
    bit_depth         INTEGER,
    channels          INTEGER,
    content_hash      TEXT,
    musicbrainz_id    TEXT,
    play_count        INTEGER NOT NULL DEFAULT 0,
    last_played_at    INTEGER,
    rating            INTEGER CHECK (rating IS NULL OR rating BETWEEN 0 AND 5),
    loved             INTEGER NOT NULL DEFAULT 0,
    created_at        INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at        INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX idx_tracks_album_id    ON tracks(album_id);
CREATE INDEX idx_tracks_artist_id   ON tracks(artist_id);
CREATE INDEX idx_tracks_source_id   ON tracks(source_id);
CREATE INDEX idx_tracks_genre       ON tracks(genre);
CREATE INDEX idx_tracks_year        ON tracks(year);
CREATE INDEX idx_tracks_play_count  ON tracks(play_count);
CREATE INDEX idx_tracks_last_played ON tracks(last_played_at);
CREATE INDEX idx_tracks_loved       ON tracks(loved);
CREATE INDEX idx_tracks_remote_path ON tracks(source_id, remote_path);
