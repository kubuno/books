use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct AuthUser {
    pub id:    Uuid,
    pub email: String,
    pub role:  String,
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let user = extract_user_from_headers(headers);

    match user {
        Some(u) => {
            req.extensions_mut().insert(u);
            Ok(next.run(req).await)
        }
        None => {
            let core_url = &state.settings.core.url;
            let token = extract_bearer(headers).ok_or(StatusCode::UNAUTHORIZED)?;
            let url = format!("{}/api/v1/me", core_url);
            let resp = state.http
                .get(&url)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|_| StatusCode::UNAUTHORIZED)?;

            if !resp.status().is_success() {
                return Err(StatusCode::UNAUTHORIZED);
            }

            let user: AuthUser = resp.json().await.map_err(|_| StatusCode::UNAUTHORIZED)?;
            req.extensions_mut().insert(user);
            Ok(next.run(req).await)
        }
    }
}

fn extract_user_from_headers(headers: &HeaderMap) -> Option<AuthUser> {
    let user_id = headers.get("X-Kubuno-User-Id")?.to_str().ok()?;
    let email   = headers.get("X-Kubuno-User-Email")?.to_str().ok()?;
    let role    = headers.get("X-Kubuno-User-Role")?.to_str().ok()?;
    Some(AuthUser {
        id:    user_id.parse().ok()?,
        email: email.to_string(),
        role:  role.to_string(),
    })
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let auth = headers.get("Authorization")?.to_str().ok()?;
    auth.strip_prefix("Bearer ")
}
