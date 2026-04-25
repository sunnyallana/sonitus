-- ─────────────────────────────────────────────────────────────────────────
-- 005_audit_log.sql
-- Audit log table — mirrors the JSONL audit.log file in DB form so the
-- UI can query/filter without parsing the file.
-- ─────────────────────────────────────────────────────────────────────────

INSERT INTO schema_version (version) VALUES (5);

CREATE TABLE audit_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp     INTEGER NOT NULL,
    destination   TEXT    NOT NULL,
    method        TEXT    NOT NULL,
    path          TEXT    NOT NULL,
    triggered_by  TEXT    NOT NULL
                  CHECK (triggered_by IN (
                      'user_action','background_scan','metadata_lookup',
                      'oauth_refresh','download','playback'
                  )),
    bytes_sent    INTEGER NOT NULL DEFAULT 0,
    bytes_recv    INTEGER NOT NULL DEFAULT 0,
    status        INTEGER,
    duration_ms   INTEGER,
    error_msg     TEXT
);
CREATE INDEX idx_audit_log_timestamp   ON audit_log(timestamp);
CREATE INDEX idx_audit_log_destination ON audit_log(destination);
CREATE INDEX idx_audit_log_triggered   ON audit_log(triggered_by);
