use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_doctor, AuthUser};
use crate::availability::models::SetAvailabilityForm;
use crate::availability::services;
use crate::errors::AppError;

/// GET /availability — list availability (doctors see theirs; admin sees all)
pub async fn list_availability(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let slots = match user.role.as_str() {
        "admin" => services::get_all_availability(pool.get_ref()).await?,
        _ => {
            require_doctor(&user)?;
            services::get_availability_for_doctor(pool.get_ref(), user.user_id).await?
        }
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("slots", &slots);
    ctx.insert("title", "Doctor Availability");
    let rendered = tera.render("availability/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /availability/set — show form to set availability (doctor only)
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

/// POST /availability/set — add a new availability slot (doctor only)
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
