pub mod access;
pub mod decode;
pub mod formats;
pub mod jsonq;

use kubuno_db::dialect::Backend;

/// The `settings.key` column, spelled per engine: `key` is reserved on MySQL and
/// must be back-quoted; PostgreSQL and SQLite accept it bare.
pub fn settings_key_col(backend: Backend) -> &'static str {
    match backend {
        Backend::MySql => "`key`",
        _ => "key",
    }
}
pub mod metadata;
pub mod providers;
pub mod scan;
