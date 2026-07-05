use actix_session::Session;
use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_admin, require_self_or_admin, AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::users::models::{
    ChangePasswordForm, CreateStaffForm, EditProfileForm, LoginForm, Patient, RegisterForm,
};
use crate::users::services;

// ============================================================
// Registration
// ============================================================

/// GET /users/register: show registration form
pub async fn register_form(
    tera: web::Data<tera::Tera>,
    user: OptionalAuthUser,
) -> Result<HttpResponse, AppError> {
    let mut ctx = Context::new();
    ctx.insert("user", &user.0);
    ctx.insert("title", "Register");
    let rendered = tera.render("users/register.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /users/register: process registration
pub async fn register(
    pool: web::Data<sqlx::SqlitePool>,
    session: Session,
    form: web::Form<RegisterForm>,
) -> Result<HttpResponse, AppError> {
    let user = services::register_user(pool.get_ref(), &form).await?;
    crate::audit::services::record_raw(
        pool.get_ref(),
        crate::audit::services::NewAuditEntry {
            user_id: Some(user.id),
            username: &user.username,
            role: user.role_str(),
            action: "user.registered",
            entity: "user",
            entity_id: Some(user.id),
            details: "",
        },
    )
    .await;

    // Auto-login after registration. `renew()` first: the freshly
    // authenticated session must never continue an anonymous one
    // (session-fixation defence, same as the login handler).
    session.renew();
    session.insert("user_id", user.id)?;
    session.insert("username", &user.username)?;
    session.insert("role", user.role().as_str())?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
}

// ============================================================
// Login / Logout
// ============================================================

/// GET /users/login: show login form
pub async fn login_form(
    tera: web::Data<tera::Tera>,
    user: OptionalAuthUser,
) -> Result<HttpResponse, AppError> {
    let mut ctx = Context::new();
    ctx.insert("user", &user.0);
    ctx.insert("title", "Login");
    let rendered = tera.render("users/login.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /users/login: process login
pub async fn login(
    pool: web::Data<sqlx::SqlitePool>,
    session: Session,
    tera: web::Data<tera::Tera>,
    form: web::Form<LoginForm>,
) -> Result<HttpResponse, AppError> {
    match services::authenticate_user(pool.get_ref(), &form).await {
        Ok(user) => {
            // Rotate the session on privilege change so a session id issued
            // before authentication can never carry the authenticated state
            // (session-fixation defence).
            session.renew();
            session.insert("user_id", user.id)?;
            session.insert("username", &user.username)?;
            session.insert("role", user.role().as_str())?;

            crate::audit::services::record_raw(
                pool.get_ref(),
                crate::audit::services::NewAuditEntry {
                    user_id: Some(user.id),
                    username: &user.username,
                    role: user.role_str(),
                    action: "user.login",
                    entity: "user",
                    entity_id: Some(user.id),
                    details: "",
                },
            )
            .await;

            Ok(HttpResponse::SeeOther()
                .append_header(("Location", "/appointments"))
                .finish())
        }
        Err(AppError::Unauthorized(msg)) => {
            let mut ctx = Context::new();
            ctx.insert("user", &Option::<crate::auth::AuthUser>::None);
            ctx.insert("error", &msg);
            ctx.insert("title", "Login");
            let rendered = tera.render("users/login.html.tera", &ctx)?;
            // Failed login is a 401, but we still show the form with the error
            Ok(HttpResponse::Unauthorized().body(rendered))
        }
        Err(e) => Err(e),
    }
}

/// POST /users/logout: clear session and redirect to login.
/// POST (not GET) because logout changes server-visible state: a GET logout
/// can be triggered by any cross-site image/link (CSRF-style forced logout)
/// and may be prefetched by browsers.
pub async fn logout(session: Session) -> Result<HttpResponse, AppError> {
    session.purge();
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/users/login"))
        .finish())
}

// ============================================================
// Staff creation (admin only)
// ============================================================

/// GET /users/new: show the "add staff" form (admin only)
pub async fn create_staff_form(
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("title", "Add Staff");
    let rendered = tera.render("users/new.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /users/new: create a doctor/admin account (admin only)
pub async fn create_staff(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<CreateStaffForm>,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let created = services::create_staff_user(pool.get_ref(), &form).await?;
    crate::audit::services::record(
        pool.get_ref(), &user, "staff.created", "user", Some(created.id),
        &format!("Created {} account: {}", created.role_str(), created.username),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/users"))
        .finish())
}

// ============================================================
// User listing & profile
// ============================================================

/// GET /users: list all users (admin only)
pub async fn list_users(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let users = services::get_all_users(pool.get_ref()).await?;
    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("users", &users);
    ctx.insert("title", "All Users");
    let rendered = tera.render("users/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /users/{id}: view a user's profile (with patient/doctor details)
pub async fn user_profile(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    current_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();

    // Ownership + anti-enumeration rule lives in the service: patients are
    // silently redirected to their own profile so a forbidden ID and a
    // non-existent ID look identical to a caller.
    let profile_user = match services::get_user_profile_checked(
        pool.get_ref(), current_user.user_id, current_user.role, profile_id,
    ).await? {
        Some(u) => u,
        None => return Ok(HttpResponse::SeeOther()
            .append_header(("Location", format!("/users/{}", current_user.user_id)))
            .finish()),
    };

    let patient = services::get_patient_by_user_id(pool.get_ref(), profile_id).await?;
    let doctor = services::get_doctor_by_user_id(pool.get_ref(), profile_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &current_user);
    ctx.insert("profile_user", &profile_user);
    ctx.insert("patient", &patient);
    ctx.insert("doctor", &doctor);
    ctx.insert("title", &format!("User: {}", profile_user.full_name));
    let rendered = tera.render("users/profile.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// Deletion (admin only)
// ============================================================

/// POST /users/{id}/delete: permanently remove a user account (admin only)
pub async fn delete_user(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;
    let target_id = path.into_inner();

    let deleted = services::delete_user(pool.get_ref(), user.user_id, target_id).await?;
    crate::audit::services::record(
        pool.get_ref(), &user, "user.deleted", "user", Some(target_id),
        &format!("Deleted {} account: {}", deleted.role_str(), deleted.username),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/users"))
        .finish())
}

// ============================================================
// Profile editing
// ============================================================

/// GET /users/{id}/edit: show edit profile form
pub async fn edit_profile_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    current_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();
    require_self_or_admin(&current_user, profile_id)?;
    let profile_user = services::get_user_by_id(pool.get_ref(), profile_id).await?;
    let patient = services::get_patient_by_user_id(pool.get_ref(), profile_id).await?;
    let doctor = services::get_doctor_by_user_id(pool.get_ref(), profile_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &current_user);
    ctx.insert("profile_user", &profile_user);
    ctx.insert("patient", &patient);
    ctx.insert("doctor", &doctor);
    // Canonical blood-group options for the dropdown, same list the
    // domain validates against, so the two never drift apart.
    ctx.insert("blood_groups", &Patient::BLOOD_GROUPS);
    ctx.insert("title", "Edit Profile");
    let rendered = tera.render("users/edit.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /users/{id}/edit: process profile update
pub async fn edit_profile(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    current_user: AuthUser,
    form: web::Form<EditProfileForm>,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();
    require_self_or_admin(&current_user, profile_id)?;

    services::update_profile(pool.get_ref(), profile_id, &form).await?;
    crate::audit::services::record(
        pool.get_ref(), &current_user, "user.profile_updated", "user", Some(profile_id), "",
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/users/{}", profile_id)))
        .finish())
}

// ============================================================
// Password change
// ============================================================

/// GET /users/{id}/change-password: show the change-password form
pub async fn change_password_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    current_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();
    require_self_or_admin(&current_user, profile_id)?;
    let profile_user = services::get_user_by_id(pool.get_ref(), profile_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &current_user);
    ctx.insert("profile_user", &profile_user);
    ctx.insert("title", "Change Password");
    let rendered = tera.render("users/change_password.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /users/{id}/change-password: process a password change.
/// Changing your own password requires the current one; an admin resetting
/// someone else's account does not (they can't know it).
pub async fn change_password(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    current_user: AuthUser,
    form: web::Form<ChangePasswordForm>,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();
    require_self_or_admin(&current_user, profile_id)?;
    let is_self = current_user.user_id == profile_id;

    let (msg, mut builder) = match services::change_password(pool.get_ref(), profile_id, is_self, &form).await {
        Ok(()) => {
            let action = if is_self { "user.password_changed" } else { "user.password_reset" };
            crate::audit::services::record(
                pool.get_ref(), &current_user, action, "user", Some(profile_id), "",
            ).await;

            return Ok(HttpResponse::SeeOther()
                .append_header(("Location", format!("/users/{}", profile_id)))
                .finish());
        }
        Err(AppError::BadRequest(msg)) => (msg, HttpResponse::BadRequest()),
        Err(AppError::Unauthorized(msg)) => (msg, HttpResponse::Unauthorized()),
        Err(e) => return Err(e),
    };

    let profile_user = services::get_user_by_id(pool.get_ref(), profile_id).await?;
    let mut ctx = Context::new();
    ctx.insert("user", &current_user);
    ctx.insert("profile_user", &profile_user);
    ctx.insert("error", &msg);
    ctx.insert("title", "Change Password");
    let rendered = tera.render("users/change_password.html.tera", &ctx)?;
    Ok(builder.body(rendered))
}