use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;
use std::sync::Once;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Deserialize)]
pub struct AuthUser {
    pub id:    Uuid,
    pub email: String,
    pub role:  String,
}

/// Authenticates a request, by one of two routes.
///
/// **The proxy route.** The core proxies every user-facing call to this module
/// and injects the caller's identity into `X-Kubuno-User-*`. Those headers are
/// only worth as much as the proof that the core wrote them: the same proxy
/// replaces `Authorization` with the module's own `X-Internal-Secret`, handed to
/// this process at startup. The identity is therefore believed **only** when
/// that secret is presented and matches — otherwise anyone able to reach the
/// module's port directly could declare any account with any role, which since
/// this module gained per-library restrictions and age ratings means reading
/// past both of them, and reaching the admin routes on top.
///
/// **The bearer route.** A caller that holds a user's access token but does not
/// come through the proxy (a reading application talking to the module) is
/// authenticated by the core itself, over `/api/v1/me`. That path proves the
/// identity rather than asserting it, so it needs no shared secret.
///
/// Identity headers that fail the secret check are *dropped*, not fatal: the
/// request simply falls through to the bearer route and is refused there unless
/// it carries a real token. `/health` is mounted outside this middleware and
/// stays reachable without any of this.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let expected = state.settings.core.internal_secret.as_str();

    let user = if internal_secret_matches(headers, expected) {
        extract_user_from_headers(headers)
    } else {
        if has_identity_headers(headers) {
            if expected.is_empty() {
                warn_secret_not_configured();
            }
            // No secret material is logged here, only the fact of the refusal.
            tracing::warn!(
                "books: en-têtes d'identité présentés sans secret interne valide — ignorés"
            );
        }
        None
    };

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
                .map_err(|e| {
                    tracing::warn!(error = %e, "books: vérification du jeton auprès du core impossible");
                    StatusCode::UNAUTHORIZED
                })?;

            if !resp.status().is_success() {
                return Err(StatusCode::UNAUTHORIZED);
            }

            let user: AuthUser = resp.json().await.map_err(|e| {
                tracing::warn!(error = %e, "books: réponse /api/v1/me illisible");
                StatusCode::UNAUTHORIZED
            })?;
            req.extensions_mut().insert(user);
            Ok(next.run(req).await)
        }
    }
}

/// Whether this request carries the internal secret this module was started
/// with, and may therefore have its `X-Kubuno-User-*` headers believed.
///
/// An **empty** configured secret refuses everything. Without that check a
/// module started outside the core's supervision — with no secret in its
/// environment — would accept any request sending an empty header, which is the
/// misconfiguration most likely to happen and the one that opens the module
/// widest.
///
/// The comparison is constant-time. The secret is long and random, so a timing
/// attack against it is theoretical; doing it anyway removes the need for the
/// next reader to re-make that judgement correctly.
fn internal_secret_matches(headers: &HeaderMap, expected: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let provided = headers
        .get("x-internal-secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    constant_time_eq(provided.as_bytes(), expected.as_bytes())
}

/// Byte comparison whose duration does not depend on where the first difference
/// is. The length check leaks the length, which is not a secret.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// True when the request asserts an identity at all — used only to decide
/// whether a refusal is worth reporting.
fn has_identity_headers(headers: &HeaderMap) -> bool {
    headers.contains_key("x-kubuno-user-id")
}

/// Reported once per process: repeating it on every request would let an
/// attacker fill the journal by hammering the port.
fn warn_secret_not_configured() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        tracing::error!(
            "books: core.internal_secret vide — l'identité portée par les en-têtes du \
             proxy ne peut pas être authentifiée et sera refusée. Renseignez \
             KUBUNO_INTERNAL_SECRET (valeur remise par le core au démarrage du module)."
        );
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    /// A request as the core's proxy builds it: identity plus the module secret.
    fn proxied(secret: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            "x-kubuno-user-id",
            HeaderValue::from_static("11111111-1111-4111-8111-111111111111"),
        );
        h.insert("x-kubuno-user-email", HeaderValue::from_static("a@b.c"));
        h.insert("x-kubuno-user-role", HeaderValue::from_static("admin"));
        if let Some(s) = secret {
            h.insert(
                "x-internal-secret",
                HeaderValue::from_str(s).expect("test secret is valid ASCII"),
            );
        }
        h
    }

    #[test]
    fn the_comparison_still_compares() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn the_proxys_secret_is_accepted() {
        assert!(internal_secret_matches(&proxied(Some(SECRET)), SECRET));
    }

    /// The whole point: reaching the port directly and claiming to be an admin.
    #[test]
    fn identity_without_the_secret_is_not_trusted() {
        assert!(!internal_secret_matches(&proxied(None), SECRET));
        assert!(extract_user_from_headers(&proxied(None)).is_some());
    }

    #[test]
    fn a_wrong_secret_is_not_trusted() {
        assert!(!internal_secret_matches(&proxied(Some("wrong")), SECRET));
        // Same length, one byte apart — the case a naive prefix check would pass.
        let mut near = SECRET.to_string();
        near.pop();
        near.push('0');
        assert!(!internal_secret_matches(&proxied(Some(&near)), SECRET));
    }

    /// An unconfigured module must not become an open one.
    #[test]
    fn an_empty_configured_secret_refuses_everything() {
        assert!(!internal_secret_matches(&proxied(Some("")), ""));
        assert!(!internal_secret_matches(&proxied(None), ""));
        assert!(!internal_secret_matches(&proxied(Some(SECRET)), ""));
    }

    #[test]
    fn an_anonymous_request_is_not_reported_as_an_identity() {
        assert!(!has_identity_headers(&HeaderMap::new()));
        assert!(has_identity_headers(&proxied(None)));
    }

    #[test]
    fn the_bearer_route_is_untouched() {
        let mut h = HeaderMap::new();
        h.insert("authorization", HeaderValue::from_static("Bearer tok"));
        assert_eq!(extract_bearer(&h), Some("tok"));
        assert_eq!(extract_bearer(&HeaderMap::new()), None);
    }
}
