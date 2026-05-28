use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::BookAppointmentForm;
use crate::appointments::services;
use crate::auth::AuthUser;
use crate::errors::AppError;

// ============================================================
// GET /appointments — list appointments filtered by role
// ============================================================

pub async fn list_appointments(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointments = match user.role.as_str() {
        "patient" => {
            services::get_appointments_for_patient(pool.get_ref(), user.user_id).await?
        }
        "doctor" => {
            services::get_appointments_for_doctor(pool.get_ref(), user.user_id).await?
        }
        "admin" => services::get_all_appointments(pool.get_ref()).await?,
        _ => vec![],
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("appointments", &appointments);
    ctx.insert("title", "Appointments");
    let rendered = tera.render("appointments/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// GET /appointments/book — show booking form
// ============================================================

pub async fn book_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let doctors = services::get_all_doctors(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("title", "Book Appointment");
    let rendered = tera.render("appointments/book.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// POST /appointments/book — process booking (with conflict check)
// ============================================================

pub async fn book_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookAppointmentForm>,
) -> Result<HttpResponse, AppError> {
    let appointment =
        services::book_appointment(pool.get_ref(), user.user_id, &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

// ============================================================
// GET /appointments/{id} — view single appointment detail
// ============================================================

pub async fn appointment_detail(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();
    let appointment = services::get_appointment_by_id(pool.get_ref(), appointment_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("appointment", &appointment);
    ctx.insert("title", &format!("Appointment #{}", appointment.id));
    let rendered = tera.render("appointments/detail.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// POST /appointments/{id}/cancel — cancel an appointment
// ============================================================

pub async fn cancel_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    _user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();

    // Only patient who owns the appointment, the doctor, or an admin can cancel
    // (simplified: allow any authenticated user for now — tighten in production)
    services::cancel_appointment(pool.get_ref(), appointment_id).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
}
