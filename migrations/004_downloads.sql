-- ─────────────────────────────────────────────────────────────────────────
-- 004_downloads.sql
-- Download queue + state
-- ─────────────────────────────────────────────────────────────────────────

INSERT INTO schema_version (version) VALUES (4);

CREATE TABLE downloads (
    id            TEXT    PRIMARY KEY,
    track_id      TEXT    NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    status        TEXT    NOT NULL DEFAULT 'queued'
                          CHECK (status IN ('queued','downloading','paused','done','failed','cancelled')),
    progress      REAL    NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 1),
    bytes_total   INTEGER,
    bytes_done    INTEGER NOT NULL DEFAULT 0,
    speed_bps     INTEGER,
    local_path    TEXT,
    error_msg     TEXT,
    retry_count   INTEGER NOT NULL DEFAULT 0,
    queued_at     INTEGER NOT NULL DEFAULT (unixepoch()),
    started_at    INTEGER,
    finished_at   INTEGER
);
CREATE INDEX idx_downloads_status   ON downloads(status);
CREATE INDEX idx_downloads_track_id ON downloads(track_id);
CREATE INDEX idx_downloads_queued_at ON downloads(queued_at);
