use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_doctor, AuthUser, Role};
use crate::availability::models::{EditAvailabilityForm, SetAvailabilityForm};
use crate::availability::services;
use crate::errors::AppError;

/// GET /availability: list availability (doctors see theirs; admin sees all)
pub async fn list_availability(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let slots = match user.role {
        Role::Admin => services::get_all_availability(pool.get_ref()).await?,
        Role::Doctor => services::get_availability_for_doctor(pool.get_ref(), user.user_id).await?,
        Role::Patient => {
            return Err(AppError::Forbidden(
                "Availability is managed by doctors and admins.".into(),
            ))
        }
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("slots", &slots);
    ctx.insert("title", "Doctor Availability");
    let rendered = tera.render("availability/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /availability/set: show form to set availability (doctor only)
pub async fn set_availability_form(
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("title", "Set Availability");
    let rendered = tera.render("availability/set.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /availability/set: add availability (doctor only). A recurring
/// submit may create several weekly rules in one go.
pub async fn set_availability(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<SetAvailabilityForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    services::add_availability(pool.get_ref(), user.user_id, &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/availability"))
        .finish())
}

/// GET /availability/{id}/edit: form to edit one slot (owner or admin)
pub async fn edit_availability_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let slot = services::get_owned_slot(pool.get_ref(), &user, path.into_inner()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("slot", &slot);
    ctx.insert("title", "Edit Availability");
    let rendered = tera.render("availability/edit.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /availability/{id}/edit: update one slot (owner or admin)
pub async fn edit_availability(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    path: web::Path<i64>,
    form: web::Form<EditAvailabilityForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    services::update_availability(pool.get_ref(), &user, path.into_inner(), &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/availability"))
        .finish())
}

/// POST /availability/{id}/delete: remove one slot (owner or admin)
pub async fn delete_availability(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    path: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    services::delete_availability(pool.get_ref(), &user, path.into_inner()).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/availability"))
        .finish())
}
