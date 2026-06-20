-- Migrate existing book_formats.storage_path to the canonical "[Drive]/<rel>" form.
-- Old form: "<owner_uuid>/files/<rel>" (raw drive storage path).
-- New form: "[Drive]/<rel>" — owner-agnostic, routable by the path resolver.
-- Only rewrites local (drive-backed) rows that still use the old layout.

UPDATE books.book_formats
SET storage_path = '[Drive]/' || substring(storage_path FROM length(owner_id::text || '/files/') + 1)
WHERE local_cache_path IS NULL
  AND storage_path LIKE (owner_id::text || '/files/%');
