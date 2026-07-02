use crate::availability::models::{DoctorAvailability, SetAvailabilityForm};
use crate::db;
use crate::errors::AppError;
use crate::traits::{any_conflict, TimeSlotted, TimeWindow};
use chrono::Datelike;
use sqlx::SqlitePool;

// ============================================================
// Availability enforcement — integrates this module into the
// appointment-booking workflow (scheduling + conflict resolution)
// ============================================================

/// Enforce a doctor's availability rules for a requested time slot.
///
/// Rules, in order:
/// 1. **Blocked entries win.** If any blocked entry (one-off leave on that
///    exact date, or a recurring weekly block like a lunch break) overlaps
///    the requested window, the booking is rejected.
/// 2. **Defined windows constrain.** If the doctor has declared available
///    windows for that day (recurring weekly, or a one-off date entry),
///    the requested slot must fall entirely inside one of them.
/// 3. **No entries means open schedule.** A doctor with no availability
///    rows for that day follows the clinic's default hours (enforced by
///    the booking form's 08:00–17:00 slot grid).
pub async fn ensure_doctor_available(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
) -> Result<(), AppError> {
    let date = chrono::NaiveDate::parse_from_str(appointment_date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid appointment date".into()))?;
    // Schema convention: 0 = Sunday .. 6 = Saturday
    let day_of_week = date.weekday().num_days_from_sunday() as i32;

    // All rules that apply to this day: recurring entries for this weekday
    // plus any one-off entries pinned to this exact date.
    let rules = sqlx::query_as::<_, DoctorAvailability>(
        "SELECT id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability
         WHERE doctor_id = ?
           AND ((is_recurring = 1 AND day_of_week = ?) OR specific_date = ?)",
    )
    .bind(doctor_id)
    .bind(day_of_week)
    .bind(appointment_date)
    .fetch_all(pool)
    .await?;

    // Rule 1: any overlapping blocked entry rejects the booking. The requested
    // window is lifted into the TimeSlotted world (`TimeWindow`) so the shared
    // polymorphic `any_conflict` check performs the overlap comparison.
    let requested = TimeWindow::new(start_time, end_time);
    if any_conflict(&requested, rules.iter().filter(|r| r.blocked())) {
        return Err(AppError::BadRequest(
            "The doctor is unavailable at that time (on leave or blocked).\
             \nPlease pick a different time or doctor, or use the suggestion feature."
                .into(),
        ));
    }

    // Rule 2: if available windows are declared for this day, the slot
    // must sit entirely inside one of them. Containment is a TimeSlotted
    // domain method (sibling of overlaps_with used by Rule 1 above), so the
    // service no longer compares raw start/end fields by hand.
    let windows: Vec<&DoctorAvailability> = rules.iter().filter(|r| !r.blocked()).collect();
    if !windows.is_empty()
        && !windows.iter().any(|w| w.contains(start_time, end_time))
    {
        return Err(AppError::BadRequest(
            "The requested time is outside the doctor's working hours for that day.\
             \nCheck the doctor's availability and choose a time within their schedule."
                .into(),
        ));
    }

    Ok(())
}

/// List all availability slots for a doctor.
pub async fn get_availability_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<DoctorAvailability>, AppError> {
    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "SELECT id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability WHERE doctor_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(doctor_id)
    .fetch_all(pool)
    .await?)
}

/// List all availability slots (admin view).
pub async fn get_all_availability(
    pool: &SqlitePool,
) -> Result<Vec<DoctorAvailability>, AppError> {
    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "SELECT id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability ORDER BY doctor_id, day_of_week, start_time",
    )
    .fetch_all(pool)
    .await?)
}

/// Add a new availability slot for a doctor.
/// Flow: validate the form, resolve the doctor, then persist.
pub async fn add_availability(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &SetAvailabilityForm,
) -> Result<DoctorAvailability, AppError> {
    // Validation — nothing touches the database before this passes
    form.validate()?;
    // Store the canonical zero-padded "HH:MM" form: the availability gate
    // compares these strings lexically against requested booking windows.
    let (start_mins, end_mins) = crate::time::parse_time_range(&form.start_time, &form.end_time)?;

    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "INSERT INTO doctor_availability
         (doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked",
    )
    .bind(doctor_id)
    .bind(form.day_of_week)
    .bind(crate::time::minutes_to_time(start_mins))
    .bind(crate::time::minutes_to_time(end_mins))
    .bind(form.recurring() as i32) // SQLite stores BOOLEAN as INTEGER 0/1
    .bind(form.specific_date_or_none())
    .bind(form.blocked() as i32)
    .fetch_one(pool)
    .await?)
}
