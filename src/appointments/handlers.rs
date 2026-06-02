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
// GET /appointments/calendar — calendar view
// ============================================================

#[derive(serde::Serialize)]
struct CalendarDay {
    day: u32,
    date: String,
    is_today: bool,
    is_current_month: bool,
    count: usize,
}

pub async fn calendar_view(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    use chrono::{Datelike, NaiveDate};
    let today = chrono::Utc::now().date_naive();
    let year = query.get("year").and_then(|y| y.parse().ok()).unwrap_or(today.year());
    let month = query.get("month").and_then(|m| m.parse().ok()).unwrap_or(today.month());

    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let days_in_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    }.signed_duration_since(first).num_days() as u32;

    let start_dow = first.weekday().num_days_from_monday(); // 0=Mon

    // Build calendar grid
    let mut days: Vec<CalendarDay> = Vec::new();

    // Previous month filler
    for _d in 0..start_dow {
        days.push(CalendarDay { day: 0, date: String::new(), is_today: false, is_current_month: false, count: 0 });
    }

    // Current month days
    let from_date = format!("{}-{:02}-01", year, month);
    let to_date = format!("{}-{:02}-{}", year, month, days_in_month);

    let counts: Vec<(String, i64)> = match user.role.as_str() {
        "patient" => {
            sqlx::query_as::<_, (String, i64)>(
                "SELECT a.appointment_date, COUNT(*) FROM appointments a
                 JOIN patients p ON a.patient_id = p.id
                 WHERE p.user_id = ? AND a.appointment_date >= ? AND a.appointment_date <= ? AND a.status != 'cancelled'
                 GROUP BY a.appointment_date",
            ).bind(user.user_id).bind(&from_date).bind(&to_date).fetch_all(pool.get_ref()).await?
        }
        "doctor" => {
            sqlx::query_as::<_, (String, i64)>(
                "SELECT a.appointment_date, COUNT(*) FROM appointments a
                 JOIN doctors d ON a.doctor_id = d.id
                 WHERE d.user_id = ? AND a.appointment_date >= ? AND a.appointment_date <= ? AND a.status != 'cancelled'
                 GROUP BY a.appointment_date",
            ).bind(user.user_id).bind(&from_date).bind(&to_date).fetch_all(pool.get_ref()).await?
        }
        _ => {
            sqlx::query_as::<_, (String, i64)>(
                "SELECT appointment_date, COUNT(*) FROM appointments
                 WHERE appointment_date >= ? AND appointment_date <= ? AND status != 'cancelled'
                 GROUP BY appointment_date",
            ).bind(&from_date).bind(&to_date).fetch_all(pool.get_ref()).await?
        }
    };

    let count_map: std::collections::HashMap<String, usize> = counts
        .into_iter().map(|(d, c)| (d, c as usize)).collect();

    for d in 1..=days_in_month {
        let date_str = format!("{}-{:02}-{:02}", year, month, d);
        let is_today = date_str == today.format("%Y-%m-%d").to_string();
        days.push(CalendarDay {
            day: d, date: date_str.clone(), is_today, is_current_month: true,
            count: count_map.get(&date_str).copied().unwrap_or(0),
        });
    }

    let prev_month = if month == 1 { (year - 1, 12) } else { (year, month - 1) };
    let next_month = if month == 12 { (year + 1, 1) } else { (year, month + 1) };

    // Group days into weeks (rows of 7)
    let weeks: Vec<&[CalendarDay]> = days.chunks(7).collect();

    let month_names = ["January","February","March","April","May","June","July","August","September","October","November","December"];

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("weeks", &weeks);
    ctx.insert("year", &year);
    ctx.insert("month", &month);
    ctx.insert("month_name", month_names[(month - 1) as usize]);
    ctx.insert("prev_year", &prev_month.0);
    ctx.insert("prev_month", &prev_month.1);
    ctx.insert("next_year", &next_month.0);
    ctx.insert("next_month", &next_month.1);
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
