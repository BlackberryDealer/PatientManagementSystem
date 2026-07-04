//! Algorithm 2: Earliest Available Slot.

use crate::appointments::models::{DaySchedule, SuggestSlotForm};
use crate::availability::services::ensure_doctor_available;
use crate::errors::AppError;
use crate::time::{
    minutes_to_time, time_to_minutes, CLINIC_CLOSE_MINUTES, CLINIC_OPEN_MINUTES, SLOT_MINUTES,
};
use sqlx::SqlitePool;

/// Given a doctor, date, and desired duration, find the earliest
/// available start time. Returns `None` if the day is fully booked.
///
/// ## Steps:
/// 1. Fetch all existing appointments for the doctor on that date
/// 2. Sort by start_time
/// 3. Walk through the gaps; for each gap ≥ duration, check the doctor's
///    availability rules (leave, blocked breaks, declared working windows —
///    the same 3-rule gate booking enforces)
/// 4. Return the first gap that passes both; `None` if nothing fits
///
/// Consulting `ensure_doctor_available` here keeps the suggestion feature
/// consistent with booking: a suggested slot is always one the booking
/// path would actually accept.
pub async fn find_earliest_slot(
    pool: &SqlitePool,
    form: &SuggestSlotForm,
) -> Result<Option<String>, AppError> {
    form.validate()?;

    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT start_time, end_time FROM appointments
         WHERE doctor_id = ? AND appointment_date = ? AND status != 'cancelled'
         ORDER BY start_time",
    )
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .fetch_all(pool)
    .await?;

    let busy: Vec<(i32, i32)> = existing
        .iter()
        .filter_map(|(start, end)| Some((time_to_minutes(start)?, time_to_minutes(end)?)))
        .collect();

    let schedule = DaySchedule::new(busy);
    let mut open = CLINIC_OPEN_MINUTES;
    while let Some(start) = schedule.earliest_gap(form.duration_minutes, open, CLINIC_CLOSE_MINUTES) {
        let end = start + form.duration_minutes;
        let available = ensure_doctor_available(
            pool, form.doctor_id, &form.appointment_date,
            &minutes_to_time(start), &minutes_to_time(end),
        )
        .await
        .is_ok();
        if available {
            return Ok(Some(minutes_to_time(start)));
        }
        // Doctor is blocked or outside their declared hours at this gap —
        // resume the search from the next grid-aligned start time.
        open = start + SLOT_MINUTES;
    }
    Ok(None)
}
