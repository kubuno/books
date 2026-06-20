-- Organisation: collections (groups of series), read lists (ordered books across series),
-- saved searches (virtual libraries).

CREATE TABLE books.collections (
    id          uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id    uuid NOT NULL,
    name        varchar(500) NOT NULL,
    description text,
    is_public   boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_books_coll_owner ON books.collections(owner_id);

CREATE TABLE books.collection_series (
    collection_id uuid NOT NULL REFERENCES books.collections(id) ON DELETE CASCADE,
    series_id     uuid NOT NULL REFERENCES books.series(id) ON DELETE CASCADE,
    position      int NOT NULL DEFAULT 0,
    PRIMARY KEY (collection_id, series_id)
);

CREATE TABLE books.read_lists (
    id          uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id    uuid NOT NULL,
    name        varchar(500) NOT NULL,
    description text,
    is_public   boolean NOT NULL DEFAULT true,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_books_rl_owner ON books.read_lists(owner_id);

CREATE TABLE books.read_list_books (
    read_list_id uuid NOT NULL REFERENCES books.read_lists(id) ON DELETE CASCADE,
    book_id      uuid NOT NULL REFERENCES books.books(id) ON DELETE CASCADE,
    position     int NOT NULL DEFAULT 0,
    PRIMARY KEY (read_list_id, book_id)
);

CREATE TABLE books.saved_searches (
    id         uuid PRIMARY KEY DEFAULT uuid_generate_v4(),
    owner_id   uuid NOT NULL,
    name       varchar(500) NOT NULL,
    filters    jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX idx_books_ss_owner ON books.saved_searches(owner_id);

CREATE TRIGGER collections_updated_at BEFORE UPDATE ON books.collections FOR EACH ROW EXECUTE FUNCTION books.set_updated_at();
CREATE TRIGGER read_lists_updated_at  BEFORE UPDATE ON books.read_lists  FOR EACH ROW EXECUTE FUNCTION books.set_updated_at();
