// Live free-slot lookup that drives the booking form's time dropdowns. Not
// one of the five numbered scheduling algorithms, but the same family: it
// reuses DaySchedule (the gap-finder behind Algorithm 2) and the availability
// gate to answer "which 30-minute starts are actually open?".

use crate::appointments::models::{DaySchedule, SlotAvailability};
use crate::availability::services::{fetch_rules_for_day, slot_allowed_by_rules};
use crate::errors::AppError;
use crate::time::{
    minutes_to_time, time_to_minutes, CLINIC_CLOSE_MINUTES, CLINIC_OPEN_MINUTES, SLOT_MINUTES,
};
use sqlx::SqlitePool;

/// Every 30-minute start slot the doctor's availability rules allow on `date`,
/// within clinic hours, each marked free (unoccupied) or occupied.
///
/// A slot appears here only when it is allowed by the doctor's availability
/// rules, the same gate booking enforces via `ensure_doctor_available`, so a
/// slot outside those rules is never shown, because no priority can book it.
/// Among the allowed slots, `free` reports whether an existing appointment
/// already holds it; occupied slots are still returned so a doctor/admin can
/// select one and trigger a priority override, which bumps the lower-priority
/// occupant to the waitlist.
///
/// Occupancy is read from the `appointments` table (the source of truth that
/// `check_conflict` and `find_earliest_slot` also use), not from the
/// `appointment_slots` ledger: that ledger is the write-time concurrency guard
/// and is not guaranteed to be populated for seed or externally-inserted rows,
/// so reading it here would let already-booked times show up as free. The
/// UNIQUE index on the ledger still arbitrates the real race when two patients
/// grab the same slot at once.
pub async fn all_slots(
    pool: &SqlitePool,
    doctor_id: i64,
    date: &str,
) -> Result<Vec<SlotAvailability>, AppError> {
    // Reject past/invalid dates the same way booking does.
    crate::time::parse_booking_date(date)?;

    // The doctor's booked (scheduled or completed) intervals for the day.
    // Cancelled appointments free their time, so they are excluded.
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT start_time, end_time FROM appointments
         WHERE doctor_id = ? AND appointment_date = ? AND status != 'cancelled'",
    )
    .bind(doctor_id)
    .bind(date)
    .fetch_all(pool)
    .await?;

    let busy: Vec<(i32, i32)> = existing
        .iter()
        .filter_map(|(start, end)| Some((time_to_minutes(start)?, time_to_minutes(end)?)))
        .collect();
    let schedule = DaySchedule::new(busy);

    let rules = fetch_rules_for_day(pool, doctor_id, date).await?;

    let mut slots = Vec::new();
    let mut s = CLINIC_OPEN_MINUTES;
    while s < CLINIC_CLOSE_MINUTES {
        let start = minutes_to_time(s);
        let end = minutes_to_time(s + SLOT_MINUTES);
        if slot_allowed_by_rules(&rules, &start, &end) {
            slots.push(SlotAvailability {
                free: schedule.is_free(s, SLOT_MINUTES),
                time: start,
            });
        }
        s += SLOT_MINUTES;
    }
    Ok(slots)
}

/// The doctor's free 30-minute start slots on `date`, within clinic hours.
///
/// The free-only projection of [`all_slots`]: a slot shown here is both
/// unoccupied and rule-allowed, so the booking path will accept it as-is. This
/// powers the patient booking form's live dropdown, turning the old "guess a
/// time and hope it is free" flow into picking from open slots.
pub async fn free_slots(
    pool: &SqlitePool,
    doctor_id: i64,
    date: &str,
) -> Result<Vec<String>, AppError> {
    Ok(all_slots(pool, doctor_id, date)
        .await?
        .into_iter()
        .filter(|slot| slot.free)
        .map(|slot| slot.time)
        .collect())
}
