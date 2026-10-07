-- Per-library settings (scanner / options / metadata import options).
ALTER TABLE books.libraries ADD COLUMN settings jsonb NOT NULL DEFAULT '{}';
