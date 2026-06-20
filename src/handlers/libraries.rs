use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use kubuno_storage::path::user_folder_dir;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    errors::BooksError,
    middleware::auth::AuthUser,
    models::library::{BookLibrary, CreateLibraryDto, UpdateLibraryDto},
    state::AppState,
};

/// Valid library types for the books module.
const LIB_TYPES: &[&str] = &["books", "comics", "ebooks"];

const LIBRARY_COLUMNS: &str = r#"id, owner_id, name, lib_type, path, icon, color,
    is_shared, item_count, last_scan_at, scan_status, scan_error,
    source_type, files_folder_id, files_owner_id,
    remote_mount_id, remote_mount_path, remote_owner_id,
    settings, created_at, updated_at"#;

pub async fn list_libraries(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    let sql = format!(
        "SELECT {LIBRARY_COLUMNS} FROM books.libraries \
         WHERE (is_shared = TRUE OR owner_id = $1) AND books.lib_allowed($1, id) ORDER BY name"
    );
    let rows = sqlx::query_as::<_, BookLibrary>(&sql)
        .bind(user.id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(json!({ "libraries": rows })))
}

pub async fn create_library(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Json(dto): Json<CreateLibraryDto>,
) -> Result<(StatusCode, Json<Value>), BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    if !LIB_TYPES.contains(&dto.lib_type.as_str()) {
        return Err(BooksError::Validation("lib_type invalide".into()));
    }

    let source_type = dto.source_type.as_deref().unwrap_or("filesystem");

    let row: BookLibrary = if source_type == "remote_mount" {
        let mount_id = dto.remote_mount_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| BooksError::Validation("remote_mount_id requis pour source remote_mount".into()))?
            .to_string();
        let mount_path = dto.remote_mount_path.as_deref().unwrap_or("").to_string();

        let sql = format!(
            "INSERT INTO books.libraries \
             (owner_id, name, lib_type, path, icon, color, is_shared, \
              source_type, remote_mount_id, remote_mount_path, remote_owner_id, settings) \
             VALUES ($1,$2,$3,'',$4,$5,$6,'remote_mount',$7,$8,$1,$9) \
             RETURNING {LIBRARY_COLUMNS}"
        );
        sqlx::query_as::<_, BookLibrary>(&sql)
            .bind(user.id)
            .bind(&dto.name)
            .bind(&dto.lib_type)
            .bind(dto.icon.as_deref().unwrap_or("📚"))
            .bind(dto.color.as_deref().unwrap_or("#1a73e8"))
            .bind(dto.is_shared.unwrap_or(true))
            .bind(&mount_id)
            .bind(&mount_path)
            .bind(dto.settings.clone().unwrap_or_else(|| serde_json::json!({})))
            .fetch_one(&state.db)
            .await?
    } else if source_type == "files_folder" {
        let folder_id = dto.files_folder_id
            .ok_or_else(|| BooksError::Validation("files_folder_id requis pour source files_folder".into()))?;
        // The folder's owner can be supplied or resolved from the drive folder itself.
        let owner_id = match dto.files_owner_id {
            Some(o) => o,
            None => sqlx::query_scalar::<_, Uuid>(
                "SELECT owner_id FROM drive.folders WHERE id = $1 AND is_trashed = FALSE",
            )
            .bind(folder_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| BooksError::NotFound(format!("Dossier drive {folder_id}")))?,
        };
        let base = state.settings.storage.files_storage_base.as_deref()
            .ok_or_else(|| BooksError::Validation("storage.files_storage_base non configuré sur ce serveur".into()))?;

        // Cross-schema read into the drive module's schema (kept as-is).
        let folder_path: String = sqlx::query_scalar::<_, String>(
            "SELECT path FROM drive.folders WHERE id = $1 AND owner_id = $2",
        )
        .bind(folder_id)
        .bind(owner_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Dossier drive {folder_id}")))?;

        let rel = user_folder_dir(owner_id, &folder_path);
        let resolved = format!("{}/{}", base.trim_end_matches('/'), rel.to_string_lossy());

        let sql = format!(
            "INSERT INTO books.libraries \
             (owner_id, name, lib_type, path, icon, color, is_shared, \
              source_type, files_folder_id, files_owner_id, settings) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,'files_folder',$8,$9,$10) \
             RETURNING {LIBRARY_COLUMNS}"
        );
        sqlx::query_as::<_, BookLibrary>(&sql)
            .bind(user.id)
            .bind(&dto.name)
            .bind(&dto.lib_type)
            .bind(&resolved)
            .bind(dto.icon.as_deref().unwrap_or("📚"))
            .bind(dto.color.as_deref().unwrap_or("#1a73e8"))
            .bind(dto.is_shared.unwrap_or(true))
            .bind(folder_id)
            .bind(owner_id)
            .bind(dto.settings.clone().unwrap_or_else(|| serde_json::json!({})))
            .fetch_one(&state.db)
            .await?
    } else {
        let path = dto.path
            .filter(|p| !p.is_empty())
            .ok_or_else(|| BooksError::Validation("path requis pour source filesystem".into()))?;

        let sql = format!(
            "INSERT INTO books.libraries (owner_id, name, lib_type, path, icon, color, is_shared, settings) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
             RETURNING {LIBRARY_COLUMNS}"
        );
        sqlx::query_as::<_, BookLibrary>(&sql)
            .bind(user.id)
            .bind(&dto.name)
            .bind(&dto.lib_type)
            .bind(&path)
            .bind(dto.icon.as_deref().unwrap_or("📚"))
            .bind(dto.color.as_deref().unwrap_or("#1a73e8"))
            .bind(dto.is_shared.unwrap_or(true))
            .bind(dto.settings.clone().unwrap_or_else(|| serde_json::json!({})))
            .fetch_one(&state.db)
            .await?
    };

    Ok((StatusCode::CREATED, Json(json!(row))))
}

pub async fn update_library(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(dto): Json<UpdateLibraryDto>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    let sql = format!(
        "UPDATE books.libraries \
         SET name      = COALESCE($2, name), \
             path      = COALESCE($3, path), \
             icon      = COALESCE($4, icon), \
             color     = COALESCE($5, color), \
             is_shared = COALESCE($6, is_shared), \
             settings  = COALESCE($7, settings) \
         WHERE id = $1 \
         RETURNING {LIBRARY_COLUMNS}"
    );
    let row = sqlx::query_as::<_, BookLibrary>(&sql)
        .bind(id)
        .bind(dto.name)
        .bind(dto.path)
        .bind(dto.icon)
        .bind(dto.color)
        .bind(dto.is_shared)
        .bind(dto.settings)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| BooksError::NotFound(format!("Bibliothèque {id}")))?;
    Ok(Json(json!(row)))
}

/// Folder picker source from the drive module (admins only).
#[derive(Debug, sqlx::FromRow)]
struct FilesFolderRow {
    id:                 Uuid,
    owner_id:           Uuid,
    path:               String,
    name:               String,
    owner_email:        String,
    owner_display_name: Option<String>,
}

pub async fn list_files_folders(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }
    // Cross-schema read into the drive + core schemas (kept as-is).
    let rows = sqlx::query_as::<_, FilesFolderRow>(
        r#"SELECT f.id, f.owner_id, f.path, f.name,
                  u.email        AS owner_email,
                  u.display_name AS owner_display_name
           FROM drive.folders f
           JOIN core.users u ON u.id = f.owner_id
           ORDER BY u.email, f.path"#,
    )
    .fetch_all(&state.db)
    .await?;

    let folders: Vec<_> = rows.iter().map(|r| json!({
        "id":                 r.id,
        "owner_id":           r.owner_id,
        "path":               r.path,
        "name":               r.name,
        "owner_email":        r.owner_email,
        "owner_display_name": r.owner_display_name,
    })).collect();

    Ok(Json(json!({ "folders": folders })))
}

pub async fn delete_library(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }

    // Capture this library's cached remote files before the cascade removes the rows.
    let cached: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT bf.local_cache_path FROM books.book_formats bf \
         JOIN books.books b ON b.id = bf.book_id \
         WHERE b.library_id = $1 AND bf.local_cache_path IS NOT NULL",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;

    let affected = sqlx::query("DELETE FROM books.libraries WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(BooksError::NotFound(format!("Bibliothèque {id}")));
    }

    // Remove now-orphaned cache files (a file may be shared by another library on
    // the same mount — only delete it when no remaining format references it).
    for path in cached {
        let still_used: bool = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM books.book_formats WHERE local_cache_path = $1)",
        )
        .bind(&path)
        .fetch_one(&state.db)
        .await
        .unwrap_or(true);
        if !still_used {
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Start an asynchronous scan of the library's Drive folder (admins only).
pub async fn start_scan(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    if user.role != "admin" {
        return Err(BooksError::Forbidden);
    }

    let exists: Option<Uuid> = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM books.libraries WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(BooksError::NotFound(format!("Bibliothèque {id}")));
    }

    sqlx::query("UPDATE books.libraries SET scan_status = 'scanning', scan_error = NULL WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?;

    let st = state.clone();
    tokio::spawn(async move { crate::services::scan::scan_library(st, id).await });

    Ok(Json(json!({ "message": "Scan démarré", "library_id": id })))
}

pub async fn scan_status(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, BooksError> {
    let status: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT scan_status FROM books.libraries WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;

    Ok(Json(json!({ "status": status.unwrap_or_else(|| "idle".into()) })))
}
