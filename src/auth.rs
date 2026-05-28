use actix_session::SessionExt;
use actix_web::{dev::Payload, FromRequest, HttpRequest};
use std::future::{ready, Ready};

// ============================================================
// AuthUser — extractor that requires an active session
// ============================================================

/// Extracts the authenticated user from the session cookie.
/// Returns `401 Unauthorized` if no valid session exists.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub role: String,
}

impl FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let session = req.get_session();
        match session.get::<i64>("user_id") {
            Ok(Some(user_id)) => {
                let username = session
                    .get::<String>("username")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let role = session
                    .get::<String>("role")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "patient".to_string());
                ready(Ok(AuthUser {
                    user_id,
                    username,
                    role,
                }))
            }
            Ok(None) => ready(Err(actix_web::error::ErrorUnauthorized(
                "Please log in to access this page.",
            ))),
            Err(_) => ready(Err(actix_web::error::ErrorInternalServerError(
                "Session read error",
            ))),
        }
    }
}

// ============================================================
// OptionalAuthUser — extractor that does NOT require a session
// ============================================================

/// Extracts the authenticated user if a session exists, otherwise returns `None`.
/// Use this for pages that behave differently for logged-in vs anonymous users
/// (e.g., navigation bar, home page).
#[derive(Debug, Clone, serde::Serialize)]
pub struct OptionalAuthUser(pub Option<AuthUser>);

impl FromRequest for OptionalAuthUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let session = req.get_session();
        match session.get::<i64>("user_id") {
            Ok(Some(user_id)) => {
                let username = session
                    .get::<String>("username")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                let role = session
                    .get::<String>("role")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "patient".to_string());
                ready(Ok(OptionalAuthUser(Some(AuthUser {
                    user_id,
                    username,
                    role,
                }))))
            }
            _ => ready(Ok(OptionalAuthUser(None))),
        }
    }
}

// ============================================================
// Role-based guard helpers
// ============================================================

/// Check that the authenticated user has one of the allowed roles.
/// Returns `AppError::Forbidden` if not.
pub fn require_role(
    user: &AuthUser,
    allowed_roles: &[&str],
) -> Result<(), crate::errors::AppError> {
    if allowed_roles.contains(&user.role.as_str()) {
        Ok(())
    } else {
        Err(crate::errors::AppError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ))
    }
}

/// Convenience: require admin role.
pub fn require_admin(user: &AuthUser) -> Result<(), crate::errors::AppError> {
    require_role(user, &["admin"])
}

/// Convenience: require doctor role (or admin).
pub fn require_doctor(user: &AuthUser) -> Result<(), crate::errors::AppError> {
    require_role(user, &["doctor", "admin"])
}
