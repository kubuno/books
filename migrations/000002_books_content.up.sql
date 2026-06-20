-- Books content model:
--   series        a folder of books inside a library (comics/manga); optional for loose ebooks
--   books         a bibliographic work = metadata + 1..N format files
--   book_formats  one physical file per (book, format): cbz/cbr/cb7/pdf/epub
--   pages         per-format page index (image-based formats), filled by the decoders (P2)

-- ── Series ───────────────────────────────────────────────────────────────────────
CREATE TABLE books.series (
    id                uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    library_id        uuid NOT NULL REFERENCES books.libraries(id) ON DELETE CASCADE,
    owner_id          uuid NOT NULL,
    name              varchar(500) NOT NULL,
    sort_name         varchar(500),
    folder_id         uuid NOT NULL,                       -- drive.folders.id (a series = a drive folder)
    folder_path       text,
    description       text,
    publisher         varchar(500),
    genres            jsonb NOT NULL DEFAULT '[]',
    tags              jsonb NOT NULL DEFAULT '[]',
    language          varchar(20),
    age_rating        int,
    reading_direction varchar(20),                         -- ltr | rtl | vertical | webtoon
    total_book_count  int,                                 -- declared total (metadata)
    book_count        int NOT NULL DEFAULT 0,
    cover_format_id   uuid,                                -- format providing the series cover
    metadata          jsonb NOT NULL DEFAULT '{}',
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (library_id, folder_id)
);
CREATE INDEX idx_books_series_library ON books.series(library_id);

-- ── Books ────────────────────────────────────────────────────────────────────────
CREATE TABLE books.books (
    id                uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    library_id        uuid NOT NULL REFERENCES books.libraries(id) ON DELETE CASCADE,
    series_id         uuid REFERENCES books.series(id) ON DELETE SET NULL,
    owner_id          uuid NOT NULL,
    folder_id         uuid,                                -- drive folder containing the book
    title             varchar(1000) NOT NULL,
    sort_title        varchar(1000),
    book_key          varchar(1000) NOT NULL,              -- grouping key: lower(basename) within folder
    series_index      double precision,                    -- position within its series
    description       text,
    publisher         varchar(500),
    published_date    date,
    isbn              varchar(20),
    identifiers       jsonb NOT NULL DEFAULT '{}',         -- {isbn, asin, google, ...}
    language          varchar(20),
    page_count        int,
    rating            double precision,                    -- 0..5
    age_rating        int,
    reading_direction varchar(20),
    release_date      date,
    authors           jsonb NOT NULL DEFAULT '[]',         -- denormalized in P1; normalized in P4
    tags              jsonb NOT NULL DEFAULT '[]',
    cover_format_id   uuid,
    metadata          jsonb NOT NULL DEFAULT '{}',
    added_at          timestamptz NOT NULL DEFAULT now(),
    file_modified_at  timestamptz,
    last_scanned_at   timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    UNIQUE (library_id, folder_id, book_key)
);
CREATE INDEX idx_books_books_library ON books.books(library_id);
CREATE INDEX idx_books_books_series  ON books.books(series_id);
CREATE INDEX idx_books_books_added   ON books.books(library_id, added_at DESC);

-- ── Book formats (one physical file per format) ──────────────────────────────────
CREATE TABLE books.book_formats (
    id               uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    book_id          uuid NOT NULL REFERENCES books.books(id) ON DELETE CASCADE,
    owner_id         uuid NOT NULL,
    format           varchar(10) NOT NULL,                 -- cbz | cbr | cb7 | pdf | epub
    file_id          uuid NOT NULL UNIQUE,                 -- drive.files.id (idempotent rescans)
    file_name        varchar(1000) NOT NULL,
    storage_path     text NOT NULL,                        -- RELATIVE storage path (resolve w/ files_storage_base)
    size_bytes       bigint NOT NULL DEFAULT 0,
    content_hash     varchar(64),
    page_count       int,                                  -- filled by the decoders (P2)
    format_metadata  jsonb NOT NULL DEFAULT '{}',
    pages_indexed    boolean NOT NULL DEFAULT false,
    added_at         timestamptz NOT NULL DEFAULT now(),
    file_modified_at timestamptz,
    updated_at       timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_books_formats_book ON books.book_formats(book_id);

-- ── Pages (per format, image-based formats) ──────────────────────────────────────
CREATE TABLE books.pages (
    id         uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    format_id  uuid NOT NULL REFERENCES books.book_formats(id) ON DELETE CASCADE,
    idx        int NOT NULL,                               -- 0-based page index
    name       text,                                       -- entry name inside the archive
    mime_type  varchar(100),
    width      int,
    height     int,
    size_bytes bigint,
    UNIQUE (format_id, idx)
);

CREATE TRIGGER series_updated_at  BEFORE UPDATE ON books.series       FOR EACH ROW EXECUTE FUNCTION books.set_updated_at();
CREATE TRIGGER books_updated_at   BEFORE UPDATE ON books.books        FOR EACH ROW EXECUTE FUNCTION books.set_updated_at();
CREATE TRIGGER formats_updated_at BEFORE UPDATE ON books.book_formats FOR EACH ROW EXECUTE FUNCTION books.set_updated_at();
