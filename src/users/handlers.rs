use actix_session::Session;
use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_admin, require_self_or_admin, AuthUser, OptionalAuthUser, Role};
use crate::errors::AppError;
use crate::users::models::{EditProfileForm, LoginForm, RegisterForm};
use crate::users::services;

// ============================================================
// Registration
// ============================================================

/// GET /users/register — show registration form
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

/// POST /users/register — process registration
pub async fn register(
    pool: web::Data<sqlx::SqlitePool>,
    session: Session,
    form: web::Form<RegisterForm>,
) -> Result<HttpResponse, AppError> {
    let user = services::register_user(pool.get_ref(), &form).await?;
    crate::audit::services::record_raw(
        pool.get_ref(), Some(user.id), &user.username, &user.role,
        "user.registered", "user", Some(user.id), "",
    ).await;

    // Auto-login after registration
    session.insert("user_id", user.id)?;
    session.insert("username", &user.username)?;
    session.insert("role", &user.role)?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
}

// ============================================================
// Login / Logout
// ============================================================

/// GET /users/login — show login form
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

/// POST /users/login — process login
pub async fn login(
    pool: web::Data<sqlx::SqlitePool>,
    session: Session,
    tera: web::Data<tera::Tera>,
    form: web::Form<LoginForm>,
) -> Result<HttpResponse, AppError> {
    match services::authenticate_user(pool.get_ref(), &form).await {
        Ok(user) => {
            session.insert("user_id", user.id)?;
            session.insert("username", &user.username)?;
            session.insert("role", &user.role)?;

            crate::audit::services::record_raw(
                pool.get_ref(), Some(user.id), &user.username, &user.role,
                "user.login", "user", Some(user.id), "",
            ).await;

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

/// GET /users/logout — clear session and redirect to login
pub async fn logout(session: Session) -> Result<HttpResponse, AppError> {
    session.purge();
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/users/login"))
        .finish())
}

// ============================================================
// User listing & profile
// ============================================================

/// GET /users — list all users (admin only)
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

/// GET /users/{id} — view a user's profile (with patient/doctor details)
pub async fn user_profile(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    current_user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();

    // Authorization: a patient may only view their OWN profile, which protects
    // other patients' personal and medical details (date of birth, address,
    // blood group, emergency contact). Doctors and admins (clinical staff) may
    // view any profile.
    //
    // Safe fail (anti-enumeration): rather than returning an error that would
    // confirm the target exists, we silently send the patient back to their own
    // profile. This check runs BEFORE the database lookup, so a forbidden id and
    // a non-existent id are indistinguishable — a brute-forcer learns nothing.
    if current_user.role == Role::Patient && current_user.user_id != profile_id {
        return Ok(HttpResponse::SeeOther()
            .append_header(("Location", format!("/users/{}", current_user.user_id)))
            .finish());
    }

    let profile_user = services::get_user_by_id(pool.get_ref(), profile_id).await?;
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
// Profile editing
// ============================================================

/// GET /users/{id}/edit — show edit profile form
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
    ctx.insert("title", "Edit Profile");
    let rendered = tera.render("users/edit.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /users/{id}/edit — process profile update
pub async fn edit_profile(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    current_user: AuthUser,
    form: web::Form<EditProfileForm>,
) -> Result<HttpResponse, AppError> {
    let profile_id = path.into_inner();
    require_self_or_admin(&current_user, profile_id)?;

    services::update_profile(pool.get_ref(), profile_id, &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/users/{}", profile_id)))
        .finish())
}