-- Per-user reading progress ("on deck" / "keep reading").
CREATE TABLE books.read_progress (
    user_id    uuid NOT NULL,
    book_id    uuid NOT NULL REFERENCES books.books(id) ON DELETE CASCADE,
    page       int NOT NULL DEFAULT 0,        -- 0-based page (image archives / PDF)
    location   text,                          -- EPUB CFI or scroll fraction
    completed  boolean NOT NULL DEFAULT false,
    started_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, book_id)
);
CREATE INDEX idx_books_progress_user ON books.read_progress(user_id, updated_at DESC);
