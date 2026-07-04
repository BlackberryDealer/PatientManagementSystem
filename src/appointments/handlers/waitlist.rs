//! Waitlist handlers: the role-filtered view, the patient-only join form, and
//! staff-only promotion of a waiting entry into a real appointment.

use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::WaitlistForm;
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser, Role};
use crate::errors::AppError;
use crate::traits::Priority;

/// GET /appointments/waitlist — view waitlist filtered by role.
/// Patients see their own entries; doctors see their patients;
/// admins see all. Entries ordered by urgency (priority ASC).
pub async fn list_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let (waitlist, doctor_label) = match user.role {
        Role::Patient => {
            let entries = services::get_waitlist_for_patient(pool.get_ref(), user.user_id).await?;
            (entries, String::from("Your Waitlist"))
        }
        Role::Admin => {
            let all = services::get_all_waitlist(pool.get_ref()).await?;
            (all, String::from("All Waitlisted Patients"))
        }
        Role::Doctor => {
            let entries =
                services::get_waitlist_for_doctor(pool.get_ref(), user.user_id).await?;
            (entries, String::from("Patient Waitlist"))
        }
    };

    // Patients get a join form on this page, which needs the doctor list
    // and the same grid-aligned slot options the booking form offers.
    let doctors = match user.role {
        Role::Patient => services::get_all_doctors(pool.get_ref()).await?,
        Role::Doctor | Role::Admin => Vec::new(),
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("waitlist", &waitlist);
    ctx.insert("doctor_label", &doctor_label);
    ctx.insert("doctors", &doctors);
    ctx.insert("start_slots", &services::start_time_slots());
    ctx.insert("end_slots", &services::end_time_slots());
    ctx.insert("title", "Waitlist");
    let rendered = tera.render("appointments/waitlist.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/waitlist/join — add the current patient to the
/// waitlist for a specific doctor, date, and time window. Patient-only:
/// staff have no patient profile to waitlist, and appointments bumped by a
/// priority booking reach the waitlist automatically. The priority is
/// forced to Normal — triage is a clinical decision, so a patient cannot
/// jump the queue by self-reporting an emergency.
pub async fn join_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<WaitlistForm>,
) -> Result<HttpResponse, AppError> {
    if user.role != Role::Patient {
        return Err(AppError::Forbidden(
            "Only patients join the waitlist. Bumped appointments are waitlisted automatically."
                .into(),
        ));
    }
    let mut form = form.into_inner();
    form.priority = Priority::Normal as i32;
    services::add_to_waitlist(pool.get_ref(), user.user_id, &form).await?;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/appointments/waitlist"))
        .finish())
}

/// POST /appointments/waitlist/{id}/promote — promote a waitlist entry
/// to a real appointment. Doctor/admin only. If the slot is now free,
/// the entry is booked atomically.
pub async fn promote_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let waitlist_id = path.into_inner();
    let result = services::promote_from_waitlist(pool.get_ref(), waitlist_id).await?;

    match result {
        Some(appt) => {
            audit::record(
                pool.get_ref(), &user, "waitlist.promoted", "appointment", Some(appt.id),
                &format!("Waitlist entry #{} promoted to appointment", waitlist_id),
            ).await;
            Ok(HttpResponse::SeeOther()
                .append_header(("Location", format!("/appointments/{}", appt.id)))
                .finish())
        }
        None => Ok(HttpResponse::SeeOther()
            .append_header(("Location", "/appointments/waitlist"))
            .finish()),
    }
}
