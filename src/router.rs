use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{
    handlers::{admin, content, health, libraries, media, opds, organize, progress},
    middleware::require_auth,
    state::AppState,
};

pub fn build(state: AppState) -> Router {
    let authed = Router::new()
        // Libraries
        .route("/libraries",                 get(libraries::list_libraries).post(libraries::create_library))
        .route("/libraries/files-folders",   get(libraries::list_files_folders))
        .route("/libraries/:id",             patch(libraries::update_library).delete(libraries::delete_library))
        .route("/libraries/:id/scan",        post(libraries::start_scan))
        .route("/libraries/:id/scan/status", get(libraries::scan_status))
        .route("/libraries/:id/refresh-metadata", post(content::refresh_library_metadata))
        // Series
        .route("/series",                    get(content::list_series))
        .route("/series/:id",                get(content::get_series).patch(content::update_series))
        .route("/series/:id/books",          get(content::series_books))
        .route("/series/:id/cover",          get(media::series_cover))
        // Books
        .route("/books",                     get(content::list_books))
        .route("/books/recent",              get(content::recent_books))
        .route("/books/keep-reading",        get(progress::keep_reading))
        .route("/books/bulk",                patch(content::bulk_update_books))
        .route("/books/:id",                 get(content::get_book).patch(content::update_book))
        .route("/books/:id/refresh-metadata", post(content::refresh_book_metadata))
        .route("/books/:id/apply-metadata",  post(content::apply_online_metadata))
        // Online metadata search (OpenLibrary / Google Books)
        .route("/metadata/search",           get(content::search_online_metadata))
        // Organisation: collections / read lists / saved searches / facets
        .route("/collections",               get(organize::list_collections).post(organize::create_collection))
        .route("/collections/:id",           get(organize::get_collection).patch(organize::update_collection).delete(organize::delete_collection))
        .route("/collections/:id/series",    post(organize::add_series_to_collection))
        .route("/collections/:id/series/:sid", delete(organize::remove_series_from_collection))
        .route("/readlists",                 get(organize::list_read_lists).post(organize::create_read_list))
        .route("/readlists/:id",             get(organize::get_read_list).delete(organize::delete_read_list))
        .route("/readlists/:id/books",       post(organize::add_book_to_read_list))
        .route("/readlists/:id/books/:bid",  delete(organize::remove_book_from_read_list))
        .route("/searches",                  get(organize::list_saved_searches).post(organize::create_saved_search))
        .route("/searches/:id",              delete(organize::delete_saved_search))
        .route("/facets",                    get(organize::facets))
        .route("/books/:id/progress",        get(progress::get_progress).put(progress::put_progress))
        .route("/books/:id/read",            post(progress::mark_read))
        .route("/books/:id/unread",          post(progress::mark_unread))
        .route("/books/:id/cover",           get(media::book_cover))
        .route("/books/:id/download",        get(media::book_download))
        .route("/books/:id/pages",           get(media::book_page_count))
        .route("/books/:id/pages/:n",        get(media::book_page))
        .route("/formats/:id/raw",           get(media::format_raw))
        .route("/duplicates",                get(content::duplicates))
        .route("/export",                    get(content::export_catalog))
        // OPDS 1.2 catalog
        .route("/opds",                      get(opds::root))
        .route("/opds/series",               get(opds::series_nav))
        .route("/opds/series/:id",           get(opds::series_acq))
        .route("/opds/recent",               get(opds::recent_acq))
        // Admin
        .route("/admin/settings",            get(admin::get_settings).patch(admin::patch_settings))
        .route("/admin/restrictions/:uid",   get(admin::get_restrictions).put(admin::set_restrictions))
        .layer(axum::middleware::from_fn_with_state(state.clone(), require_auth));

    Router::new()
        // Health (no auth)
        .route("/health", get(health::health))
        .nest("/", authed)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
