-- SQLite — `books` is an ATTACHed database file, attached on every pooled
-- connection by kubuno-db, so the qualified names below resolve as they do on
-- the other two engines. This single file declares the FINAL shape the
-- PostgreSQL side reached across its 000001..000010 migrations.
--
-- Differences from PostgreSQL, and why:
--   * UUID -> BLOB, TIMESTAMPTZ -> TEXT (`%F %T%.f`, UTC), as sqlx encodes them.
--   * No DEFAULT on `id`: SQLite has no UUID generator; the process supplies it.
--   * JSONB -> TEXT holding JSON. uuid[] (user_restrictions.library_ids) -> TEXT
--     holding a JSON array of UUID strings; the app binds/decodes it whole.
--   * The per-user access rules (library visibility, age gate) are decided in
--     Rust: no lib_allowed/age_ok/content_ok SQL functions.
--   * updated_at is maintained in Rust (every UPDATE sets it explicitly).
--   * Foreign-key REFERENCES are unqualified (SQLite assumes the same database);
--     kubuno-db enables PRAGMA foreign_keys, so CASCADE deletes fire.

CREATE TABLE books.libraries (
    id                BLOB    NOT NULL PRIMARY KEY,
    owner_id          BLOB,
    name              TEXT    NOT NULL,
    lib_type          TEXT    NOT NULL
                          CHECK (lib_type IN ('books','comics','ebooks')),
    path              TEXT    NOT NULL,
    icon              TEXT    NOT NULL DEFAULT '📚',
    color             TEXT    NOT NULL DEFAULT '#1a73e8',
    is_shared         INTEGER NOT NULL DEFAULT 1,
    item_count        INTEGER NOT NULL DEFAULT 0,
    last_scan_at      TEXT,
    scan_status       TEXT    NOT NULL DEFAULT 'idle'
                          CHECK (scan_status IN ('idle','scanning','error')),
    scan_error        TEXT,
    source_type       TEXT    NOT NULL DEFAULT 'filesystem',
    files_folder_id   BLOB,
    files_owner_id    BLOB,
    remote_mount_id   TEXT,
    remote_mount_path TEXT    NOT NULL DEFAULT '',
    remote_owner_id   BLOB,
    settings          TEXT    NOT NULL DEFAULT '{}',
    created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX books.idx_books_libs_type ON libraries(lib_type);

CREATE TABLE books.settings (
    "key"      TEXT NOT NULL PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);

CREATE TABLE books.series (
    id                BLOB    NOT NULL PRIMARY KEY,
    library_id        BLOB    NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    owner_id          BLOB    NOT NULL,
    name              TEXT    NOT NULL,
    sort_name         TEXT,
    folder_id         BLOB    NOT NULL,
    folder_path       TEXT,
    description       TEXT,
    publisher         TEXT,
    genres            TEXT    NOT NULL DEFAULT '[]',
    tags              TEXT    NOT NULL DEFAULT '[]',
    language          TEXT,
    age_rating        INTEGER,
    reading_direction TEXT,
    total_book_count  INTEGER,
    book_count        INTEGER NOT NULL DEFAULT 0,
    cover_format_id   BLOB,
    metadata          TEXT    NOT NULL DEFAULT '{}',
    created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    UNIQUE (library_id, folder_id)
);
CREATE INDEX books.idx_books_series_library ON series(library_id);

CREATE TABLE books.books (
    id                BLOB    NOT NULL PRIMARY KEY,
    library_id        BLOB    NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    series_id         BLOB    REFERENCES series(id) ON DELETE SET NULL,
    owner_id          BLOB    NOT NULL,
    folder_id         BLOB,
    title             TEXT    NOT NULL,
    sort_title        TEXT,
    book_key          TEXT    NOT NULL,
    series_index      REAL,
    description       TEXT,
    publisher         TEXT,
    published_date    TEXT,
    isbn              TEXT,
    identifiers       TEXT    NOT NULL DEFAULT '{}',
    language          TEXT,
    page_count        INTEGER,
    rating            REAL,
    age_rating        INTEGER,
    reading_direction TEXT,
    release_date      TEXT,
    authors           TEXT    NOT NULL DEFAULT '[]',
    tags              TEXT    NOT NULL DEFAULT '[]',
    cover_format_id   BLOB,
    metadata          TEXT    NOT NULL DEFAULT '{}',
    added_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    file_modified_at  TEXT,
    last_scanned_at   TEXT,
    created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    UNIQUE (library_id, folder_id, book_key)
);
CREATE INDEX books.idx_books_books_library ON books(library_id);
CREATE INDEX books.idx_books_books_series  ON books(series_id);
CREATE INDEX books.idx_books_books_added   ON books(library_id, added_at);

CREATE TABLE books.book_formats (
    id               BLOB    NOT NULL PRIMARY KEY,
    book_id          BLOB    NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    owner_id         BLOB    NOT NULL,
    format           TEXT    NOT NULL,
    file_id          BLOB    UNIQUE,
    file_name        TEXT    NOT NULL,
    storage_path     TEXT    NOT NULL,
    size_bytes       INTEGER NOT NULL DEFAULT 0,
    content_hash     TEXT,
    page_count       INTEGER,
    format_metadata  TEXT    NOT NULL DEFAULT '{}',
    pages_indexed    INTEGER NOT NULL DEFAULT 0,
    remote_path      TEXT,
    local_cache_path TEXT,
    added_at         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    file_modified_at TEXT,
    updated_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    UNIQUE (book_id, format)
);
CREATE INDEX books.idx_books_formats_book ON book_formats(book_id);

CREATE TABLE books.pages (
    id         BLOB    NOT NULL PRIMARY KEY,
    format_id  BLOB    NOT NULL REFERENCES book_formats(id) ON DELETE CASCADE,
    idx        INTEGER NOT NULL,
    name       TEXT,
    mime_type  TEXT,
    width      INTEGER,
    height     INTEGER,
    size_bytes INTEGER,
    UNIQUE (format_id, idx)
);

CREATE TABLE books.read_progress (
    user_id    BLOB    NOT NULL,
    book_id    BLOB    NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    page       INTEGER NOT NULL DEFAULT 0,
    location   TEXT,
    completed  INTEGER NOT NULL DEFAULT 0,
    started_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    PRIMARY KEY (user_id, book_id)
);
CREATE INDEX books.idx_books_progress_user ON read_progress(user_id, updated_at);

CREATE TABLE books.collections (
    id          BLOB    NOT NULL PRIMARY KEY,
    owner_id    BLOB    NOT NULL,
    name        TEXT    NOT NULL,
    description TEXT,
    is_public   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX books.idx_books_coll_owner ON collections(owner_id);

CREATE TABLE books.collection_series (
    collection_id BLOB    NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    series_id     BLOB    NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, series_id)
);

CREATE TABLE books.read_lists (
    id          BLOB    NOT NULL PRIMARY KEY,
    owner_id    BLOB    NOT NULL,
    name        TEXT    NOT NULL,
    description TEXT,
    is_public   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX books.idx_books_rl_owner ON read_lists(owner_id);

CREATE TABLE books.read_list_books (
    read_list_id BLOB    NOT NULL REFERENCES read_lists(id) ON DELETE CASCADE,
    book_id      BLOB    NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (read_list_id, book_id)
);

CREATE TABLE books.saved_searches (
    id         BLOB NOT NULL PRIMARY KEY,
    owner_id   BLOB NOT NULL,
    name       TEXT NOT NULL,
    filters    TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
CREATE INDEX books.idx_books_ss_owner ON saved_searches(owner_id);

CREATE TABLE books.user_restrictions (
    user_id     BLOB NOT NULL PRIMARY KEY,
    library_ids TEXT,   -- NULL = all libraries allowed; else a JSON array of UUID strings
    age_max     INTEGER,
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now'))
);
