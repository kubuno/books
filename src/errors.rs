use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
#[allow(dead_code)] // Some variants are reserved for future books handlers.
pub enum BooksError {
    #[error("Non authentifié")]
    Unauthorized,

    #[error("Accès refusé")]
    Forbidden,

    #[error("Ressource introuvable: {0}")]
    NotFound(String),

    #[error("Données invalides: {0}")]
    Validation(String),

    #[error("Conflit: {0}")]
    Conflict(String),

    #[error("Erreur base de données")]
    Database(#[from] sqlx::Error),

    #[error("Erreur de stockage: {0}")]
    Storage(String),

    #[error("Service amont indisponible: {0}")]
    Upstream(String),

    #[error("Erreur interne")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for BooksError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            BooksError::Unauthorized  => (StatusCode::UNAUTHORIZED,           "UNAUTHORIZED",    self.to_string()),
            BooksError::Forbidden     => (StatusCode::FORBIDDEN,              "FORBIDDEN",       self.to_string()),
            BooksError::NotFound(m)   => (StatusCode::NOT_FOUND,              "NOT_FOUND",       m.clone()),
            BooksError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY,   "VALIDATION",      m.clone()),
            BooksError::Conflict(m)   => (StatusCode::CONFLICT,               "CONFLICT",        m.clone()),
            BooksError::Upstream(m)   => (StatusCode::BAD_GATEWAY,            "UPSTREAM_ERROR",  m.clone()),
            BooksError::Storage(m)    => (StatusCode::INTERNAL_SERVER_ERROR,  "STORAGE_ERROR",   m.clone()),
            BooksError::Database(e)   => {
                tracing::error!(error = %e, "Erreur base de données");
                (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR", "Erreur base de données".into())
            }
            BooksError::Internal(e)   => {
                tracing::error!(error = %e, "Erreur interne");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "Erreur interne".into())
            }
        };
        (status, Json(json!({ "error": code, "message": message }))).into_response()
    }
}
