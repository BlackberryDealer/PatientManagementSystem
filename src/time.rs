//! Shared time-of-day helpers used by every module that handles
//! "HH:MM" time windows (appointments, waitlist, availability).
//!
//! Centralised here so the validation layer is identical on every
//! write path — no module can accept a time string the others reject.

use crate::errors::AppError;

/// Scheduling granularity. Appointments are booked on a fixed 30-minute grid,
/// so every appointment maps cleanly onto whole 30-minute occupancy slots
/// (see the `appointment_slots` table). This is what lets us enforce
/// double-booking prevention with a database UNIQUE constraint.
pub const SLOT_MINUTES: i32 = 30;

/// Convert "HH:MM" to minutes since midnight for comparison.
pub fn time_to_minutes(t: &str) -> Option<i32> {
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let h: i32 = parts[0].parse().ok()?;
    let m: i32 = parts[1].parse().ok()?;
    if !(0..24).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    Some(h * 60 + m)
}

/// Convert minutes since midnight back to "HH:MM".
pub fn minutes_to_time(mins: i32) -> String {
    format!("{:02}:{:02}", mins / 60, mins % 60)
}

/// Parse "HH:MM" start/end strings into `(start_mins, end_mins)`,
/// enforcing valid format and start strictly before end.
///
/// Used for any time window (e.g. a doctor's availability), which does
/// not need to align to the booking grid.
pub fn parse_time_range(start: &str, end: &str) -> Result<(i32, i32), AppError> {
    let s = time_to_minutes(start)
        .ok_or_else(|| AppError::BadRequest("Invalid start time format".into()))?;
    let e = time_to_minutes(end)
        .ok_or_else(|| AppError::BadRequest("Invalid end time format".into()))?;
    if s >= e {
        return Err(AppError::BadRequest("Start time must be before end time".into()));
    }
    Ok((s, e))
}

/// Parse "HH:MM" start/end strings into `(start_mins, end_mins)`.
///
/// Enforces the scheduling design rules so the booking can be decomposed into
/// whole 30-minute slots:
/// 1. valid `HH:MM` format,
/// 2. start strictly before end,
/// 3. both times aligned to the 30-minute grid (`:00` or `:30`).
pub fn parse_slot(start: &str, end: &str) -> Result<(i32, i32), AppError> {
    let (s, e) = parse_time_range(start, end)?;
    if s % SLOT_MINUTES != 0 || e % SLOT_MINUTES != 0 {
        return Err(AppError::BadRequest(
            "Appointment times must fall on a 30-minute boundary (e.g. 09:00, 09:30, 10:00)."
                .into(),
        ));
    }
    Ok((s, e))
}
