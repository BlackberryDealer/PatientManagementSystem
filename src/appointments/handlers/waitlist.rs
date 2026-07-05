// Waitlist handlers: the role-filtered view, the patient-only join form, and
// staff-only promotion of a waiting entry into a real appointment.

use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::WaitlistForm;
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser, Role};
use crate::errors::AppError;
use crate::traits::Priority;

/// GET /appointments/waitlist: view waitlist filtered by role.
/// Patients see their own entries; doctors see their patients;
/// admins see all. Entries ordered by urgency (priority ASC).
///
/// An optional `?error=` carries a one-time notice from a failed promotion
/// (the project has no flash-session layer, so the message round-trips through
/// the URL and is rendered once in a notification banner).
pub async fn list_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    services::expire_stale_waitlist(pool.get_ref()).await?;

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
    // One-time promotion-failure notice, if the redirect carried one.
    ctx.insert("error", &query.get("error"));
    ctx.insert("title", "Waitlist");
    let rendered = tera.render("appointments/waitlist.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/waitlist/join: add the current patient to the
/// waitlist for a specific doctor, date, and time window. Patient-only:
/// staff have no patient profile to waitlist, and appointments bumped by a
/// priority booking reach the waitlist automatically. The priority is
/// forced to Normal, triage is a clinical decision, so a patient cannot
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

/// POST /appointments/waitlist/{id}/promote: promote a waitlist entry to a
/// real appointment. Doctor/admin only. If the slot is free it is booked; if it
/// is occupied by strictly lower-priority appointments they are bumped to the
/// waitlist and the entry takes the slot (a priority override). If the override
/// cannot proceed (equal/higher-priority holder, or the doctor is unavailable),
/// the user is redirected back with a notice explaining why, never a silent
/// no-op.
pub async fn promote_waitlist(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;
    let waitlist_id = path.into_inner();

    match services::promote_from_waitlist(pool.get_ref(), waitlist_id).await? {
        services::PromotionOutcome::Promoted(appt, rescheduled) => {
            audit::record(
                pool.get_ref(), &user, "waitlist.promoted", "appointment", Some(appt.id),
                &format!("Waitlist entry #{} promoted to appointment", waitlist_id),
            ).await;
            for r in &rescheduled {
                audit::record(
                    pool.get_ref(), &user, "appointment.auto_rescheduled", "appointment", Some(r.id),
                    &format!("Auto-rescheduled to {} {}–{} after a priority override bumped the original slot",
                        r.appointment_date, r.start_time, r.end_time),
                ).await;
            }
            Ok(HttpResponse::SeeOther()
                .append_header(("Location", format!("/appointments/{}", appt.id)))
                .finish())
        }
        // No flash-session layer: round-trip the reason through the URL so the
        // waitlist page can render it once in a notification banner.
        services::PromotionOutcome::Blocked(reason) => Ok(HttpResponse::SeeOther()
            .append_header((
                "Location",
                format!("/appointments/waitlist?error={}", encode_query(&reason)),
            ))
            .finish()),
    }
}

/// Percent-encode a short message for safe use as a URL query value. Kept
/// minimal (encodes everything outside the RFC 3986 unreserved set) because the
/// only values passed through are our own fixed promotion-failure messages.
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &b in value.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
