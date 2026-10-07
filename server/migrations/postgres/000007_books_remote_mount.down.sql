DROP INDEX IF EXISTS books.uidx_book_formats_book_fmt;

ALTER TABLE books.book_formats
    ALTER COLUMN file_id SET NOT NULL,
    DROP COLUMN IF EXISTS local_cache_path,
    DROP COLUMN IF EXISTS remote_path;

ALTER TABLE books.libraries
    DROP COLUMN IF EXISTS remote_owner_id,
    DROP COLUMN IF EXISTS remote_mount_path,
    DROP COLUMN IF EXISTS remote_mount_id;
