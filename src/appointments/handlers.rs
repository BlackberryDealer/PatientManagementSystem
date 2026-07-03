use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::{
    AssignRoomForm, BookAppointmentForm, CalendarMonth, RescheduleForm, SetPriorityForm,
    SuggestSlotForm, WaitlistForm,
};
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser, Role};
use crate::errors::AppError;

/// GET /appointments — list appointments filtered by role.
/// Patients see their own appointments; doctors see theirs; admins see all.
/// An optional `?date=YYYY-MM-DD` (the calendar's day links) narrows the
/// list to that single day; an unparseable date is ignored rather than 400ing.
pub async fn list_appointments(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let mut appointments = match user.role {
        Role::Patient => services::get_appointments_for_patient(pool.get_ref(), user.user_id).await?,
        Role::Doctor => services::get_appointments_for_doctor(pool.get_ref(), user.user_id).await?,
        Role::Admin => services::get_all_appointments(pool.get_ref()).await?,
    };

    let date_filter = query
        .get("date")
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
    if let Some(date) = date_filter {
        appointments.retain(|a| a.appointment_date == date);
    }

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("appointments", &appointments);
    ctx.insert("date_filter", &date_filter);
    ctx.insert("title", "Appointments");
    let rendered = tera.render("appointments/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /appointments/book — show the booking form with doctor list,
/// 30-min time-slot dropdowns, and priority selector.
pub async fn book_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let doctors = services::get_all_doctors(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("doctors", &doctors);
    ctx.insert("start_slots", &services::start_time_slots());
    ctx.insert("end_slots", &services::end_time_slots());
    ctx.insert("title", "Book Appointment");
    let rendered = tera.render("appointments/book.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/book — process a standard booking.
/// Validates the form, checks for conflicts, resolves the doctor's daily
/// room automatically, and creates the appointment atomically.
pub async fn book_appointment(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookAppointmentForm>,
) -> Result<HttpResponse, AppError> {
    let appointment = services::book_appointment(pool.get_ref(), user.user_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.booked", "appointment", Some(appointment.id),
        &format!("{} {}–{} with doctor #{}",
            appointment.appointment_date, appointment.start_time,
            appointment.end_time, appointment.doctor_id()),
    ).await;
    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/appointments/{}", appointment.id)))
        .finish())
}

/// POST /appointments/book/priority — priority-based booking.
/// Emergency/Urgent appointments may bump lower-priority ones to the
/// waitlist inside a single database transaction.
pub async fn book_with_priority(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookAppointmentForm>,
) -> Result<HttpResponse, AppError> {
    let appointment = services::book_with_priority(pool.get_ref(), user.user_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.priority_booked", "appointment",
        Some(appointment.id),
        &format!("{} priority booking on {}", appointment.priority().label(), appointment.appointment_date),
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

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("waitlist", &waitlist);
    ctx.insert("doctor_label", &doctor_label);
    ctx.insert("title", "Waitlist");
    let rendered = tera.render("appointments/waitlist.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /appointments/waitlist/join — add the current patient to the
/// waitlist for a specific doctor, date, and time window.
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

/// GET /appointments/calendar — render the monthly calendar.
/// Each day cell shows the count of (non-cancelled) appointments
/// visible to the authenticated user's role.
pub async fn calendar_view(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    use chrono::Datelike;

    // HTTP lifecycle: extract inputs, defaulting to the current month
    let today = chrono::Utc::now().date_naive();
    let year = query.get("year").and_then(|y| y.parse().ok()).unwrap_or(today.year());
    let month = query.get("month").and_then(|m| m.parse().ok()).unwrap_or(today.month());

    // Validation + business object: CalendarMonth owns all calendar arithmetic
    let mut calendar = CalendarMonth::new(year, month)?;
    let (from_date, to_date) = calendar.date_range();
    let counts = match user.role {
        Role::Patient => services::get_appointment_counts_for_patient(pool.get_ref(), user.user_id, &from_date, &to_date).await?,
        Role::Doctor  => services::get_appointment_counts_for_doctor(pool.get_ref(), user.user_id, &from_date, &to_date).await?,
        Role::Admin   => services::get_all_appointment_counts(pool.get_ref(), &from_date, &to_date).await?,
    };
    calendar.build_grid(today, &counts);

    // Presentation
    let (prev_year, prev_month) = calendar.prev();
    let (next_year, next_month) = calendar.next();

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("weeks", &calendar.weeks());
    ctx.insert("year", &calendar.year);
    ctx.insert("month", &calendar.month);
    ctx.insert("month_name", calendar.month_name());
    ctx.insert("prev_year", &prev_year);
    ctx.insert("prev_month", &prev_month);
    ctx.insert("next_year", &next_year);
    ctx.insert("next_month", &next_month);
    ctx.insert("title", "Calendar");
    let rendered = tera.render("appointments/calendar.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /appointments/{id} — view a single appointment's detail page.
/// Patients may only view their own appointments; doctors/admins may
/// view any. A "Create Medical Record" link is shown to staff for
/// scheduled appointments.
pub async fn appointment_detail(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointment_id = path.into_inner();
    // Ownership enforced in the service: patients see only their own appointments.
    let appointment = services::get_appointment_by_id_checked(
        pool.get_ref(), appointment_id, user.user_id, user.role,
    ).await?;

    // Staff see a room-override dropdown on scheduled appointments.
    let rooms = match user.role {
        Role::Doctor | Role::Admin => services::get_all_rooms(pool.get_ref()).await?,
        Role::Patient => Vec::new(),
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("appointment", &appointment);
    ctx.insert("rooms", &rooms);
    ctx.insert("title", &format!("Appointment #{}", appointment.id));
    let rendered = tera.render("appointments/detail.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// POST /appointments/{id}/complete — staff closes out a visit
// ============================================================

/// Mark a scheduled appointment as completed (the visit took place).
/// Staff only — completion is a clinical action, like creating the
/// medical record it usually accompanies. Occupancy slots are kept:
/// the time was genuinely used (see `services::complete_appointment`).
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

// ============================================================
// POST /appointments/{id}/assign-room — staff room override
// ============================================================

/// Assign (or change) the consultation room for an appointment. Staff only:
/// rooms are auto-assigned at booking, so this is the manual override for
/// moving one appointment elsewhere (e.g. into the procedure room).
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

/// POST /appointments/{id}/priority — re-triage an appointment.
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

/// POST /appointments/{id}/cancel — cancel a scheduled appointment.
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

// ============================================================
// GET /appointments/{id}/reschedule — show the reschedule form
// ============================================================

/// Render the reschedule form, prefilled with the appointment's current
/// date/time. Reuses `get_appointment_by_id_checked` so the same ownership rule
/// that governs viewing also gates who may open the form (patients: own only).
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

// ============================================================
// POST /appointments/{id}/reschedule — move to a new date/time
// ============================================================

/// Move an appointment to a new slot. Ownership is enforced in the service
/// (patients may only reschedule their own), mirroring the cancel path.
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

// ============================================================
// POST /appointments/{id}/reassign — move to an available doctor
// ============================================================

/// Doctor Reassignment (Algorithm 4): move a scheduled appointment to the
/// best alternative doctor — same specialization preferred, lightest
/// daily load first. Staff only.
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
