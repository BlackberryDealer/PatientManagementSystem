use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::{BookAppointmentForm, SuggestSlotForm, WaitlistForm};
use crate::appointments::services;
use crate::auth::{require_doctor, AuthUser};
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
        "patient" => services::get_appointments_for_patient(pool.get_ref(), user.user_id).await?,
        "doctor" => services::get_appointments_for_doctor(pool.get_ref(), user.user_id).await?,
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
    let rooms = services::get_all_rooms(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("rooms", &rooms);
    ctx.insert("start_slots", &services::start_time_slots());
    ctx.insert("end_slots", &services::end_time_slots());
    ctx.insert("title", "Book Appointment");
    let rendered = tera.render("appointments/book.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// POST /appointments/book — standard booking
// ============================================================

pub async fn book_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookAppointmentForm>,
) -> Result<HttpResponse, AppError> {
    let appointment = services::book_appointment(pool.get_ref(), user.user_id, &form).await?;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

// ============================================================
// POST /appointments/book/priority — priority-based booking
// ============================================================

pub async fn book_with_priority(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookAppointmentForm>,
) -> Result<HttpResponse, AppError> {
    let appointment = services::book_with_priority(pool.get_ref(), user.user_id, &form).await?;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

// ============================================================
// GET /appointments/suggest — show suggestion form
// ============================================================

pub async fn suggest_slot_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let doctors = services::get_all_doctors(pool.get_ref()).await?;
    let rooms = services::get_all_rooms(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("rooms", &rooms);
    ctx.insert("suggested_slot", &Option::<String>::None);
    ctx.insert("title", "Find Available Slot");
    let rendered = tera.render("appointments/suggest.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// POST /appointments/suggest — run earliest-slot algorithm
// ============================================================

pub async fn suggest_slot(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    form: web::Form<SuggestSlotForm>,
) -> Result<HttpResponse, AppError> {
    let result = services::find_earliest_slot(pool.get_ref(), &form).await?;
    let doctors = services::get_all_doctors(pool.get_ref()).await?;
    let rooms = services::get_all_rooms(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("rooms", &rooms);
    ctx.insert("suggested_slot", &result);
    ctx.insert("form_doctor_id", &form.doctor_id);
    ctx.insert("form_date", &form.appointment_date);
    ctx.insert("form_duration", &form.duration_minutes);
    ctx.insert("title", "Find Available Slot");
    let rendered = tera.render("appointments/suggest.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// GET /appointments/waitlist — view waitlist
// ============================================================

pub async fn list_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let (waitlist, doctor_label) = match user.role.as_str() {
        "patient" => {
            let entries = services::get_waitlist_for_patient(pool.get_ref(), user.user_id).await?;
            (entries, String::from("Your Waitlist"))
        }
        "admin" => {
            let all = services::get_all_waitlist(pool.get_ref()).await?;
            (all, String::from("All Waitlisted Patients"))
        }
        "doctor" => {
            let doc_id = crate::db::get_doctor_id(pool.get_ref(), user.user_id).await?;
            let entries = services::get_waitlist_for_doctor(pool.get_ref(), doc_id).await?;
            (entries, String::from("Patient Waitlist"))
        }
        _ => (vec![], String::new()),
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("waitlist", &waitlist);
    ctx.insert("doctor_label", &doctor_label);
    ctx.insert("title", "Waitlist");
    let rendered = tera.render("appointments/waitlist.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// POST /appointments/waitlist/join — join the waitlist
// ============================================================

pub async fn join_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<WaitlistForm>,
) -> Result<HttpResponse, AppError> {
    services::add_to_waitlist(pool.get_ref(), user.user_id, &form).await?;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments/waitlist"))
        .finish())
}

// ============================================================
// POST /appointments/waitlist/{id}/promote — promote from waitlist
// ============================================================

pub async fn promote_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let waitlist_id = path.into_inner();
    let result = services::promote_from_waitlist(pool.get_ref(), waitlist_id).await?;

    match result {
        Some(appt) => Ok(HttpResponse::SeeOther()
            .append_header(("Location", format!("/appointments/{}", appt.id)))
            .finish()),
        None => Ok(HttpResponse::SeeOther()
            .append_header(("Location", "/appointments/waitlist"))
            .finish()),
    }
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
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();
    services::cancel_appointment_checked(
        pool.get_ref(), appointment_id, user.user_id, &user.role,
    ).await?;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
}
