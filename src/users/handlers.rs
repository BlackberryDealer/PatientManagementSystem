use actix_session::Session;
use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_admin, AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::users::models::{LoginForm, RegisterForm};
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
    form: web::Form<LoginForm>,
) -> Result<HttpResponse, AppError> {
    let user = services::authenticate_user(pool.get_ref(), &form).await?;

    session.insert("user_id", user.id)?;
    session.insert("username", &user.username)?;
    session.insert("role", &user.role)?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
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
