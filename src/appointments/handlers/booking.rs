//! Booking flow: the role-aware booking form, the POST that creates the
//! appointment, the live free-slot JSON API that drives the form's dropdowns,
//! and the earliest-slot suggestion feature.

use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::{
    AllSlotsResponse, BookRequestForm, FreeSlotsResponse, SuggestSlotForm,
};
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser, Role};
use crate::db;
use crate::errors::AppError;

/// GET /appointments/book — show the role-aware booking form.
/// Patients pick a doctor; doctors pick a patient (the appointment lands in
/// their own schedule); admins pick both. 30-min time-slot dropdowns and a
/// staff-only priority selector round out the form.
pub async fn book_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    // Doctors book into their own schedule, so they get a patient list
    // instead of a doctor list; admins get both.
    let doctors = match user.role {
        Role::Patient | Role::Admin => services::get_all_doctors(pool.get_ref()).await?,
        Role::Doctor => Vec::new(),
    };
    let patients = match user.role {
        Role::Doctor | Role::Admin => db::get_all_patients(pool.get_ref()).await?,
        Role::Patient => Vec::new(),
    };

    // A doctor books into their own schedule (no doctor dropdown), so the live
    // slot lookup needs their doctor_id up front; other roles read it from the
    // doctor <select> instead.
    let self_doctor_id = match user.role {
        Role::Doctor => Some(db::get_doctor_id(pool.get_ref(), user.user_id).await?),
        Role::Patient | Role::Admin => None,
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("patients", &patients);
    ctx.insert("self_doctor_id", &self_doctor_id);
    ctx.insert("start_slots", &services::start_time_slots());
    ctx.insert("end_slots", &services::end_time_slots());
    ctx.insert("title", "Book Appointment");
    let rendered = tera.render("appointments/book.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /appointments/availability?doctor_id=&date= — JSON list of the doctor's
/// free 30-minute start slots for a date. Drives the booking form's live slot
/// dropdown. Any logged-in user may call it (it exposes only free/busy times,
/// no patient data). Invalid or missing inputs yield an empty list rather than
/// an error, so the front-end can call it freely as the form is filled in.
pub async fn available_slots_api(
    pool: web::Data<sqlx::SqlitePool>,
    _user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let doctor_id = query.get("doctor_id").and_then(|s| s.parse::<i64>().ok());
    let date = query.get("date").cloned().unwrap_or_default();

    let slots = match doctor_id {
        Some(did) if !date.is_empty() => {
            services::free_slots(pool.get_ref(), did, &date).await.unwrap_or_default()
        }
        _ => Vec::new(),
    };

    Ok(HttpResponse::Ok().json(FreeSlotsResponse { slots }))
}

/// GET /appointments/all-slots?doctor_id=&date= — JSON list of every 30-minute
/// start slot for a doctor on a date, each marked free or occupied. Drives the
/// staff booking form, where a doctor/admin may deliberately select an occupied
/// slot to trigger a priority override (which bumps the lower-priority occupant
/// to the waitlist). Staff-only: patients never see occupied slots, and like
/// `available_slots_api` the payload exposes only free/busy times, no patient
/// data. Invalid or missing inputs yield an empty list rather than an error.
pub async fn all_slots_api(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let doctor_id = query.get("doctor_id").and_then(|s| s.parse::<i64>().ok());
    let date = query.get("date").cloned().unwrap_or_default();

    let slots = match doctor_id {
        Some(did) if !date.is_empty() => {
            services::all_slots(pool.get_ref(), did, &date).await.unwrap_or_default()
        }
        _ => Vec::new(),
    };

    Ok(HttpResponse::Ok().json(AllSlotsResponse { slots }))
}

/// POST /appointments/book — process a booking for any role.
/// `resolve_booking_target` applies the role rules (who the appointment is
/// for/with, and that patients always book at Normal priority). Staff
/// bookings at Emergency/Urgent go through the priority path, which bumps
/// lower-priority appointments to the waitlist when the slot is contested;
/// everything else is a standard conflict-checked booking.
pub async fn book_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookRequestForm>,
) -> Result<HttpResponse, AppError> {
    let (patient_user_id, booking) =
        services::resolve_booking_target(pool.get_ref(), &user, &form).await?;

    // Emergency/Urgent is staff-only by construction: patients are already
    // forced to Normal by the resolver above.
    let appointment = if booking.requested_priority().can_override() {
        services::book_with_priority(pool.get_ref(), patient_user_id, &booking).await?
    } else {
        services::book_appointment(pool.get_ref(), patient_user_id, &booking).await?
    };

    audit::record(
        pool.get_ref(), &user, "appointment.booked", "appointment", Some(appointment.id),
        &format!("{} {}–{} with doctor #{} for patient #{} ({})",
            appointment.appointment_date, appointment.start_time,
            appointment.end_time, appointment.doctor_id(),
            appointment.patient_id, appointment.priority().label()),
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

/// GET /appointments/suggest — show the slot suggestion form.
/// The user picks a doctor, date, and desired duration.
pub async fn suggest_slot_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let doctors = services::get_all_doctors(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("suggested_slot", &Option::<String>::None);
    ctx.insert("title", "Find Available Slot");
    let rendered = tera.render("appointments/suggest.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/suggest — run the earliest-slot algorithm.
/// Scans the doctor's schedule for the given date and returns the first
/// available gap that fits the requested duration (Algorithm 2).
pub async fn suggest_slot(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    form: web::Form<SuggestSlotForm>,
) -> Result<HttpResponse, AppError> {
    let result = services::find_earliest_slot(pool.get_ref(), &form).await?;
    let doctors = services::get_all_doctors(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("suggested_slot", &result);
    ctx.insert("form_doctor_id", &form.doctor_id);
    ctx.insert("form_date", &form.appointment_date);
    ctx.insert("form_duration", &form.duration_minutes);
    ctx.insert("title", "Find Available Slot");
    let rendered = tera.render("appointments/suggest.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}
