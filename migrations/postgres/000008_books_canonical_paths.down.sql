-- Revert canonical "[Drive]/<rel>" paths back to raw "<owner>/files/<rel>".
UPDATE books.book_formats
SET storage_path = owner_id::text || '/files/' || substring(storage_path FROM length('[Drive]/') + 1)
WHERE local_cache_path IS NULL
  AND storage_path LIKE '[Drive]/%';
