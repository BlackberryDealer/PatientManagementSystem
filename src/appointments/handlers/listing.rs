// Read-only views: the appointment list, the monthly calendar, and a single
// appointment's detail page.

use actix_web::{web, HttpResponse};
use tera::Context;

use crate::appointments::models::CalendarMonth;
use crate::appointments::services;
use crate::auth::{AuthUser, Role};
use crate::errors::AppError;

/// GET /appointments: list appointments filtered by role.
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

/// GET /appointments/calendar: render the monthly calendar.
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

/// GET /appointments/{id}: view a single appointment's detail page.
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
