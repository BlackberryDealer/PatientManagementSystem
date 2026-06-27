use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::{
    BookAppointmentForm, CalendarMonth, SuggestSlotForm, WaitlistForm,
};
use crate::appointments::services;
use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser, Role};
use crate::errors::AppError;

// ============================================================
// GET /appointments — list appointments filtered by role
// ============================================================

pub async fn list_appointments(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let appointments = match user.role {
        Role::Patient => services::get_appointments_for_patient(pool.get_ref(), user.user_id).await?,
        Role::Doctor => services::get_appointments_for_doctor(pool.get_ref(), user.user_id).await?,
        Role::Admin => services::get_all_appointments(pool.get_ref()).await?,
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

// ============================================================
// POST /appointments/book/priority — priority-based booking
// ============================================================

pub async fn book_with_priority(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<BookAppointmentForm>,
) -> Result<HttpResponse, AppError> {
    let appointment = services::book_with_priority(pool.get_ref(), user.user_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "appointment.priority_booked", "appointment",
        Some(appointment.id),
        &format!("Priority {} booking on {}", appointment.priority, appointment.appointment_date),
    ).await;
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

// ============================================================
// GET /appointments/calendar — calendar view
// ============================================================

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
    let counts = services::get_appointment_counts_by_date(
        pool.get_ref(), user.role, user.user_id, &from_date, &to_date,
    ).await?;
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
    // Ownership enforced in the service: patients see only their own appointments.
    let appointment = services::get_appointment_by_id_checked(
        pool.get_ref(), appointment_id, user.user_id, user.role,
    ).await?;

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
