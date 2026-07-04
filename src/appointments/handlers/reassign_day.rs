//! Batch Doctor Reassignment (Algorithm 5) — the "cover a doctor's leave" UI.
//! Staff pick the doctor who is out and the date, preview the optimal plan
//! (no changes yet), then apply it atomically.

use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::ReassignDayForm;
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser};
use crate::errors::AppError;

/// GET /appointments/reassign-day — show the "cover a doctor's leave" form.
/// Staff pick the doctor who is out and the date; the optimal redistribution
/// is computed on submit. Staff only.
pub async fn reassign_day_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let doctors = services::get_all_doctors(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("plan", &Option::<()>::None);
    ctx.insert("title", "Cover Doctor Leave");
    let rendered = tera.render("appointments/reassign_day.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/reassign-day — preview the optimal plan (no changes yet).
/// Runs the Hungarian assignment (Algorithm 5) and renders the proposed moves
/// with an Apply button. Staff only.
pub async fn reassign_day_preview(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    form: web::Form<ReassignDayForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let plan = services::plan_day_reassignment(
        pool.get_ref(), form.doctor_id, &form.appointment_date,
    ).await?;
    let doctors = services::get_all_doctors(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("plan", &plan);
    ctx.insert("form_doctor_id", &form.doctor_id);
    ctx.insert("form_date", &form.appointment_date);
    ctx.insert("title", "Cover Doctor Leave");
    let rendered = tera.render("appointments/reassign_day.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/reassign-day/apply — apply the optimal plan atomically.
/// The plan is recomputed against the live schedule and every move commits in
/// one transaction. Staff only.
pub async fn reassign_day_apply(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<ReassignDayForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let (source_name, moved, unassigned) = services::apply_day_reassignment(
        pool.get_ref(), form.doctor_id, &form.appointment_date,
    ).await?;

    audit::record(
        pool.get_ref(), &user, "appointment.batch_reassigned", "doctor", Some(form.doctor_id),
        &format!(
            "Covered {}'s leave on {}: {} appointment(s) reassigned, {} left unplaced",
            source_name, form.appointment_date, moved, unassigned
        ),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
}
