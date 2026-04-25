-- ─────────────────────────────────────────────────────────────────────────
-- 003_fts5_search.sql
-- Full-text search index over tracks (title + artist + album + genre)
-- ─────────────────────────────────────────────────────────────────────────

INSERT INTO schema_version (version) VALUES (3);

CREATE VIRTUAL TABLE tracks_fts USING fts5(
    track_id    UNINDEXED,
    title,
    artist_name,
    album_title,
    genre,
    tokenize    = 'unicode61 remove_diacritics 1'
);

-- ── Sync triggers ────────────────────────────────────────────────────────
-- Insert: append the corresponding FTS row.
CREATE TRIGGER tracks_fts_insert AFTER INSERT ON tracks BEGIN
    INSERT INTO tracks_fts(track_id, title, artist_name, album_title, genre)
    VALUES (
        NEW.id,
        NEW.title,
        (SELECT name  FROM artists WHERE id = NEW.artist_id),
        (SELECT title FROM albums  WHERE id = NEW.album_id),
        NEW.genre
    );
END;

-- Delete: drop the FTS row.
CREATE TRIGGER tracks_fts_delete AFTER DELETE ON tracks BEGIN
    DELETE FROM tracks_fts WHERE track_id = OLD.id;
END;

-- Update: re-insert.
CREATE TRIGGER tracks_fts_update AFTER UPDATE ON tracks BEGIN
    DELETE FROM tracks_fts WHERE track_id = OLD.id;
    INSERT INTO tracks_fts(track_id, title, artist_name, album_title, genre)
    VALUES (
        NEW.id,
        NEW.title,
        (SELECT name  FROM artists WHERE id = NEW.artist_id),
        (SELECT title FROM albums  WHERE id = NEW.album_id),
        NEW.genre
    );
END;

-- When an artist is renamed, refresh all FTS rows pointing to that artist.
CREATE TRIGGER artists_fts_rename AFTER UPDATE OF name ON artists BEGIN
    UPDATE tracks_fts
       SET artist_name = NEW.name
     WHERE track_id IN (SELECT id FROM tracks WHERE artist_id = NEW.id);
END;

-- When an album is renamed, refresh all FTS rows pointing to that album.
CREATE TRIGGER albums_fts_rename AFTER UPDATE OF title ON albums BEGIN
    UPDATE tracks_fts
       SET album_title = NEW.title
     WHERE track_id IN (SELECT id FROM tracks WHERE album_id = NEW.id);
END;
