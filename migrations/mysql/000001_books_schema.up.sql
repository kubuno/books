-- MySQL / MariaDB — the `books` database is created by kubuno-db's schema setup
-- before the migrator runs, so there is no CREATE DATABASE here. This single
-- file declares the FINAL shape the PostgreSQL side reached across its
-- 000001..000010 migrations (content model, remote mounts, canonical paths,
-- portable per-user restrictions).
--
-- Differences from PostgreSQL, and why:
--   * UUID -> BINARY(16): what sqlx encodes a `uuid::Uuid` as on MySQL.
--   * No DEFAULT on `id`: MySQL has no gen_random_uuid() and no RETURNING, so
--     the process supplies every primary key (kubuno_db::new_id).
--   * TIMESTAMPTZ -> DATETIME(6); every value written is UTC (the pool pins
--     time_zone = '+00:00'). updated_at uses ON UPDATE CURRENT_TIMESTAMP(6).
--   * JSONB -> JSON. uuid[] (user_restrictions.library_ids) -> JSON array of
--     UUID strings; the app always binds/decodes it whole.
--   * The per-user access rules (library visibility, age gate) are decided in
--     Rust, so the lib_allowed/age_ok/content_ok SQL functions do not exist.
--   * The updated_at trigger becomes ON UPDATE CURRENT_TIMESTAMP(6).
--   * book_key is VARCHAR(512): a composite UNIQUE with two BINARY(16) columns
--     must fit InnoDB's 3072-byte index limit under utf8mb4.
--   * utf8mb4_bin so a UNIQUE key stays case- and accent-sensitive.

CREATE TABLE libraries (
    id                BINARY(16)   NOT NULL PRIMARY KEY,
    owner_id          BINARY(16)   NULL,
    name              VARCHAR(255) NOT NULL,
    lib_type          VARCHAR(20)  NOT NULL
                          CHECK (lib_type IN ('books','comics','ebooks')),
    path              TEXT         NOT NULL,
    icon              VARCHAR(50)  NOT NULL DEFAULT '📚',
    color             VARCHAR(7)   NOT NULL DEFAULT '#1a73e8',
    is_shared         BOOLEAN      NOT NULL DEFAULT TRUE,
    item_count        INT          NOT NULL DEFAULT 0,
    last_scan_at      DATETIME(6)  NULL,
    scan_status       VARCHAR(10)  NOT NULL DEFAULT 'idle'
                          CHECK (scan_status IN ('idle','scanning','error')),
    scan_error        TEXT         NULL,
    source_type       VARCHAR(32)  NOT NULL DEFAULT 'filesystem',
    files_folder_id   BINARY(16)   NULL,
    files_owner_id    BINARY(16)   NULL,
    remote_mount_id   VARCHAR(36)  NULL,
    remote_mount_path TEXT         NOT NULL DEFAULT '',
    remote_owner_id   BINARY(16)   NULL,
    settings          JSON         NOT NULL,
    created_at        DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at        DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_libs_type ON libraries(lib_type);

CREATE TABLE settings (
    `key`      VARCHAR(191) NOT NULL PRIMARY KEY,
    value      TEXT         NOT NULL,
    updated_at DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE series (
    id                BINARY(16)   NOT NULL PRIMARY KEY,
    library_id        BINARY(16)   NOT NULL,
    owner_id          BINARY(16)   NOT NULL,
    name              VARCHAR(500) NOT NULL,
    sort_name         VARCHAR(500) NULL,
    folder_id         BINARY(16)   NOT NULL,
    folder_path       TEXT         NULL,
    description       TEXT         NULL,
    publisher         VARCHAR(500) NULL,
    genres            JSON         NOT NULL,
    tags              JSON         NOT NULL,
    language          VARCHAR(20)  NULL,
    age_rating        INT          NULL,
    reading_direction VARCHAR(20)  NULL,
    total_book_count  INT          NULL,
    book_count        INT          NOT NULL DEFAULT 0,
    cover_format_id   BINARY(16)   NULL,
    metadata          JSON         NOT NULL,
    created_at        DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at        DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6),
    UNIQUE (library_id, folder_id),
    FOREIGN KEY (library_id) REFERENCES libraries(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_series_library ON series(library_id);

CREATE TABLE books (
    id                BINARY(16)    NOT NULL PRIMARY KEY,
    library_id        BINARY(16)    NOT NULL,
    series_id         BINARY(16)    NULL,
    owner_id          BINARY(16)    NOT NULL,
    folder_id         BINARY(16)    NULL,
    title             VARCHAR(1000) NOT NULL,
    sort_title        VARCHAR(1000) NULL,
    book_key          VARCHAR(512)  NOT NULL,
    series_index      DOUBLE        NULL,
    description       TEXT          NULL,
    publisher         VARCHAR(500)  NULL,
    published_date    DATE          NULL,
    isbn              VARCHAR(20)   NULL,
    identifiers       JSON          NOT NULL,
    language          VARCHAR(20)   NULL,
    page_count        INT           NULL,
    rating            DOUBLE        NULL,
    age_rating        INT           NULL,
    reading_direction VARCHAR(20)   NULL,
    release_date      DATE          NULL,
    authors           JSON          NOT NULL,
    tags              JSON          NOT NULL,
    cover_format_id   BINARY(16)    NULL,
    metadata          JSON          NOT NULL,
    added_at          DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    file_modified_at  DATETIME(6)   NULL,
    last_scanned_at   DATETIME(6)   NULL,
    created_at        DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at        DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6),
    UNIQUE (library_id, folder_id, book_key),
    FOREIGN KEY (library_id) REFERENCES libraries(id) ON DELETE CASCADE,
    FOREIGN KEY (series_id)  REFERENCES series(id)     ON DELETE SET NULL
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_books_library ON books(library_id);
CREATE INDEX idx_books_books_series  ON books(series_id);
CREATE INDEX idx_books_books_added   ON books(library_id, added_at);

CREATE TABLE book_formats (
    id               BINARY(16)    NOT NULL PRIMARY KEY,
    book_id          BINARY(16)    NOT NULL,
    owner_id         BINARY(16)    NOT NULL,
    format           VARCHAR(10)   NOT NULL,
    file_id          BINARY(16)    NULL UNIQUE,
    file_name        VARCHAR(1000) NOT NULL,
    storage_path     TEXT          NOT NULL,
    size_bytes       BIGINT        NOT NULL DEFAULT 0,
    content_hash     VARCHAR(64)   NULL,
    page_count       INT           NULL,
    format_metadata  JSON          NOT NULL,
    pages_indexed    BOOLEAN       NOT NULL DEFAULT FALSE,
    remote_path      TEXT          NULL,
    local_cache_path TEXT          NULL,
    added_at         DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    file_modified_at DATETIME(6)   NULL,
    updated_at       DATETIME(6)   NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6),
    UNIQUE (book_id, format),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_formats_book ON book_formats(book_id);

CREATE TABLE pages (
    id         BINARY(16)   NOT NULL PRIMARY KEY,
    format_id  BINARY(16)   NOT NULL,
    idx        INT          NOT NULL,
    name       TEXT         NULL,
    mime_type  VARCHAR(100) NULL,
    width      INT          NULL,
    height     INT          NULL,
    size_bytes BIGINT       NULL,
    UNIQUE (format_id, idx),
    FOREIGN KEY (format_id) REFERENCES book_formats(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE read_progress (
    user_id    BINARY(16)  NOT NULL,
    book_id    BINARY(16)  NOT NULL,
    page       INT         NOT NULL DEFAULT 0,
    location   TEXT        NULL,
    completed  BOOLEAN     NOT NULL DEFAULT FALSE,
    started_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6),
    PRIMARY KEY (user_id, book_id),
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_progress_user ON read_progress(user_id, updated_at);

CREATE TABLE collections (
    id          BINARY(16)   NOT NULL PRIMARY KEY,
    owner_id    BINARY(16)   NOT NULL,
    name        VARCHAR(500) NOT NULL,
    description TEXT         NULL,
    is_public   BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_coll_owner ON collections(owner_id);

CREATE TABLE collection_series (
    collection_id BINARY(16) NOT NULL,
    series_id     BINARY(16) NOT NULL,
    position      INT        NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, series_id),
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE,
    FOREIGN KEY (series_id)     REFERENCES series(id)      ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE read_lists (
    id          BINARY(16)   NOT NULL PRIMARY KEY,
    owner_id    BINARY(16)   NOT NULL,
    name        VARCHAR(500) NOT NULL,
    description TEXT         NULL,
    is_public   BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
    updated_at  DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_rl_owner ON read_lists(owner_id);

CREATE TABLE read_list_books (
    read_list_id BINARY(16) NOT NULL,
    book_id      BINARY(16) NOT NULL,
    position     INT        NOT NULL DEFAULT 0,
    PRIMARY KEY (read_list_id, book_id),
    FOREIGN KEY (read_list_id) REFERENCES read_lists(id) ON DELETE CASCADE,
    FOREIGN KEY (book_id)      REFERENCES books(id)      ON DELETE CASCADE
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE saved_searches (
    id         BINARY(16)   NOT NULL PRIMARY KEY,
    owner_id   BINARY(16)   NOT NULL,
    name       VARCHAR(500) NOT NULL,
    filters    JSON         NOT NULL,
    created_at DATETIME(6)  NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
CREATE INDEX idx_books_ss_owner ON saved_searches(owner_id);

CREATE TABLE user_restrictions (
    user_id     BINARY(16)  NOT NULL PRIMARY KEY,
    library_ids JSON        NULL,   -- NULL = all libraries allowed; else a JSON array of UUID strings
    age_max     INT         NULL,   -- NULL = no age limit
    updated_at  DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
                          ON UPDATE CURRENT_TIMESTAMP(6)
) DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;
