-- Support for remote mount libraries (Nextcloud, SMB, …).
-- Libraries of source_type='remote_mount' reference a core remote_mounts entry
-- and do not need files_storage_base; book files are cached locally after scan.

ALTER TABLE books.libraries
    ADD COLUMN remote_mount_id    VARCHAR(36),
    ADD COLUMN remote_mount_path  TEXT        NOT NULL DEFAULT '',
    ADD COLUMN remote_owner_id    UUID;

-- file_id was NOT NULL for drive-backed formats; remote formats have no drive file.
ALTER TABLE books.book_formats
    ALTER COLUMN file_id DROP NOT NULL,
    ADD COLUMN remote_path       TEXT,
    ADD COLUMN local_cache_path  TEXT;

-- Unique constraint on (book_id, format) so remote scans can upsert cleanly.
-- Drive formats already guarantee this through (file_id) uniqueness; the new
-- constraint makes it explicit for both source types.
CREATE UNIQUE INDEX IF NOT EXISTS uidx_book_formats_book_fmt
    ON books.book_formats (book_id, format);
