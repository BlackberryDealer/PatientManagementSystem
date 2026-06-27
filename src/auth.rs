use actix_session::SessionExt;
use actix_web::{dev::Payload, FromRequest, HttpRequest};
use std::future::{ready, Ready};

// ============================================================
// Role — the three user roles as a single typed source of truth
// ============================================================

/// User role. Replaces stringly-typed role comparisons throughout the app:
/// matching on a `Role` is exhaustive (the compiler checks every variant, so
/// no catch-all `_` is needed) and typos like `"pateint"` become impossible.
///
/// Serializes to its lowercase name (`"patient"` / `"doctor"` / `"admin"`),
/// which matches both the database CHECK values and the strings the Tera
/// templates compare against (e.g. `{% if user.role == "admin" %}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Patient,
    Doctor,
    Admin,
}

impl Role {
    /// Canonical lowercase string — matches the DB CHECK values and templates.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Patient => "patient",
            Role::Doctor => "doctor",
            Role::Admin => "admin",
        }
    }

    /// Parse a role from its stored string form. An unknown/missing value
    /// falls back to the least-privileged role (`Patient`) so the system
    /// fails safe rather than accidentally granting elevated access.
    pub fn from_session(s: &str) -> Role {
        match s {
            "admin" => Role::Admin,
            "doctor" => Role::Doctor,
            _ => Role::Patient,
        }
    }
}

// ============================================================
// AuthUser — extractor that requires an active session
// ============================================================

/// Extracts the authenticated user from the session cookie.
/// Returns `401 Unauthorized` if no valid session exists.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub role: Role,
}

/// Read the authenticated user from the session, shared by both extractors.
/// `Ok(None)` = no session, `Err(())` = session backend error.
fn auth_user_from_session(req: &HttpRequest) -> Result<Option<AuthUser>, ()> {
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
                .map(|s| Role::from_session(&s))
                .unwrap_or(Role::Patient);
            Ok(Some(AuthUser { user_id, username, role }))
        }
        Ok(None) => Ok(None),
        Err(_) => Err(()),
    }
}

impl FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        match auth_user_from_session(req) {
            Ok(Some(user)) => ready(Ok(user)),
            Ok(None) => ready(Err(actix_web::error::ErrorUnauthorized(
                "Please log in to access this page.",
            ))),
            Err(()) => ready(Err(actix_web::error::ErrorInternalServerError(
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
        // A session error here is treated as "not logged in" rather than a hard
        // failure, since these pages must still render for anonymous visitors.
        ready(Ok(OptionalAuthUser(
            auth_user_from_session(req).unwrap_or(None),
        )))
    }
}

// ============================================================
// Role-based guard helpers
// ============================================================

/// Check that the authenticated user has one of the allowed roles.
/// Returns `AppError::Forbidden` if not.
pub fn require_role(
    user: &AuthUser,
    allowed_roles: &[Role],
) -> Result<(), crate::errors::AppError> {
    if allowed_roles.contains(&user.role) {
        Ok(())
    } else {
        Err(crate::errors::AppError::Forbidden(
            "You do not have permission to access this resource.".into(),
        ))
    }
}

/// Convenience: require admin role.
pub fn require_admin(user: &AuthUser) -> Result<(), crate::errors::AppError> {
    require_role(user, &[Role::Admin])
}

/// Convenience: require doctor role (or admin).
pub fn require_doctor(user: &AuthUser) -> Result<(), crate::errors::AppError> {
    require_role(user, &[Role::Doctor, Role::Admin])
}

/// Convenience: the acting user must be the target user themselves, or an admin.
/// Used for profile pages where users manage their own data.
pub fn require_self_or_admin(
    user: &AuthUser,
    target_user_id: i64,
) -> Result<(), crate::errors::AppError> {
    if user.user_id == target_user_id || user.role == Role::Admin {
        Ok(())
    } else {
        Err(crate::errors::AppError::Forbidden(
            "You can only edit your own profile".into(),
        ))
    }
}
