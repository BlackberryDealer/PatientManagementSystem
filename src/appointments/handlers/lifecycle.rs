// Single-appointment lifecycle actions: complete, cancel, reschedule (form +
// submit), single-doctor reassignment (Algorithm 4), room override, and
// re-triage. Every state change is a guarded method on the Appointment
// struct in the service layer; these handlers just add auth, audit, redirects.

use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::{AssignRoomForm, RescheduleForm, SetPriorityForm};
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser};
use crate::errors::AppError;

/// POST /appointments/{id}/complete: mark a scheduled appointment as completed.
/// Staff only, completion is a clinical action, like creating the medical
/// record it usually accompanies. Occupancy slots are kept: the time was
/// genuinely used (see `services::complete_appointment`).
pub async fn complete_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?; // doctor or admin
    let appointment_id = path.into_inner();
    let appointment = services::complete_appointment(pool.get_ref(), appointment_id).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.completed", "appointment", Some(appointment.id),
        &format!("Visit on {} {}–{} completed",
            appointment.appointment_date, appointment.start_time, appointment.end_time),
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

/// POST /appointments/{id}/assign-room: assign (or change) the consultation
/// room for an appointment. Staff only: rooms are auto-assigned at booking, so
/// this is the manual override for moving one appointment elsewhere (e.g. into
/// the procedure room).
pub async fn assign_room(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
    form: web::Form<AssignRoomForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?; // doctor or admin
    let appointment_id = path.into_inner();
    let appointment = services::assign_room(pool.get_ref(), appointment_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.room_assigned", "appointment", Some(appointment.id),
        &format!("Assigned room #{}", form.room_id),
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

/// POST /appointments/{id}/priority: re-triage an appointment.
/// Staff-only: priority is a clinical decision, so patients never reach this.
pub async fn update_priority(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
    form: web::Form<SetPriorityForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?; // doctor or admin
    let appointment_id = path.into_inner();
    let appointment = services::set_priority(pool.get_ref(), appointment_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.priority_changed", "appointment", Some(appointment.id),
        &format!("Priority set to {}", appointment.priority().label()),
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

/// POST /appointments/{id}/cancel: cancel a scheduled appointment.
/// Ownership is checked: patients may only cancel their own.
/// After cancellation, auto-promotion attempts to fill the freed
/// slot from the waitlist (Algorithm 3).
pub async fn cancel_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();
    services::cancel_appointment_checked(
        pool.get_ref(), appointment_id, user.user_id, user.role,
    ).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.cancelled", "appointment", Some(appointment_id), "",
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments"))
        .finish())
}

/// GET /appointments/{id}/reschedule: show the reschedule form, prefilled with
/// the appointment's current date/time. Reuses `get_appointment_by_id_checked`
/// so the same ownership rule that governs viewing also gates who may open the
/// form (patients: own only).
pub async fn reschedule_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();
    let appointment = services::get_appointment_by_id_checked(
        pool.get_ref(), appointment_id, user.user_id, user.role,
    ).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("appointment", &appointment);
    // Same grid-aligned slot options the booking form offers, so the user can
    // only express a valid slot in the UI (the server re-validates regardless).
    ctx.insert("start_slots", &services::start_time_slots());
    ctx.insert("end_slots", &services::end_time_slots());
    ctx.insert("title", &format!("Reschedule Appointment #{}", appointment.id));
    let rendered = tera.render("appointments/reschedule.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/{id}/reschedule: move an appointment to a new slot.
/// Ownership is enforced in the service (patients may only reschedule their
/// own), mirroring the cancel path.
pub async fn reschedule_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
    form: web::Form<RescheduleForm>,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();
    let appointment = services::reschedule_appointment_checked(
        pool.get_ref(), appointment_id, user.user_id, user.role, &form,
    ).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.rescheduled", "appointment", Some(appointment.id),
        &format!("Moved to {} {}–{}",
            appointment.appointment_date, appointment.start_time, appointment.end_time),
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

/// POST /appointments/{id}/reassign: Doctor Reassignment (Algorithm 4): move a
/// scheduled appointment to the best alternative doctor, same specialization
/// preferred, lightest daily load first. Staff only.
pub async fn reassign_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let appointment_id = path.into_inner();

    let (appointment, new_doctor_name) =
        services::reassign_appointment(pool.get_ref(), appointment_id).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.reassigned", "appointment", Some(appointment.id),
        &format!("Reassigned to {}", new_doctor_name),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}
