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

/// Clinic operating hours (business rule): appointments are bookable from
/// 08:00 to 17:00. Centralised here next to `SLOT_MINUTES` so the working-day
/// bounds have a single source of truth across the earliest-slot algorithm and
/// the slot-dropdown generators — change the clinic hours in one place only.
pub const CLINIC_OPEN_MINUTES: i32 = 8 * 60; // 08:00
pub const CLINIC_CLOSE_MINUTES: i32 = 17 * 60; // 17:00

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

/// Parse a booking date and enforce the domain rule that appointments
/// cannot be requested for a date in the past. (Internal flows that
/// re-book stored requests — e.g. waitlist promotion — bypass this on
/// purpose; it guards user-submitted input only.)
pub fn parse_booking_date(date: &str) -> Result<chrono::NaiveDate, AppError> {
    let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| {
        AppError::BadRequest("Date must be a valid date in YYYY-MM-DD format".into())
    })?;
    if parsed < chrono::Utc::now().date_naive() {
        return Err(AppError::BadRequest(
            "The appointment date cannot be in the past".into(),
        ));
    }
    Ok(parsed)
}

/// Parse "HH:MM" start/end strings into `(start_mins, end_mins)`.
///
/// Enforces the scheduling design rules so the booking can be decomposed into
/// whole 30-minute slots:
/// 1. valid `HH:MM` format,
/// 2. start strictly before end,
/// 3. both times aligned to the 30-minute grid (`:00` or `:30`),
/// 4. the whole window inside clinic operating hours (08:00–17:00).
///
/// Rule 4 mirrors the booking form's slot grid on the server side, so a
/// hand-crafted POST cannot book outside clinic hours even for a doctor
/// with no declared availability windows (whose schedule is otherwise open).
pub fn parse_slot(start: &str, end: &str) -> Result<(i32, i32), AppError> {
    let (s, e) = parse_time_range(start, end)?;
    if s % SLOT_MINUTES != 0 || e % SLOT_MINUTES != 0 {
        return Err(AppError::BadRequest(
            "Appointment times must fall on a 30-minute boundary (e.g. 09:00, 09:30, 10:00)."
                .into(),
        ));
    }
    if s < CLINIC_OPEN_MINUTES || e > CLINIC_CLOSE_MINUTES {
        return Err(AppError::BadRequest(format!(
            "Appointments must fall within clinic hours ({}-{}).",
            minutes_to_time(CLINIC_OPEN_MINUTES),
            minutes_to_time(CLINIC_CLOSE_MINUTES),
        )));
    }
    Ok((s, e))
}

// ============================================================
// Unit tests for the shared time helpers
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slot_accepts_full_clinic_day() {
        assert_eq!(parse_slot("08:00", "17:00").unwrap(), (480, 1020));
    }

    #[test]
    fn parse_slot_accepts_normal_slot() {
        assert_eq!(parse_slot("09:00", "09:30").unwrap(), (540, 570));
    }

    #[test]
    fn parse_slot_rejects_before_opening() {
        // Grid-aligned but before 08:00 — must be rejected server-side.
        assert!(parse_slot("07:00", "07:30").is_err());
    }

    #[test]
    fn parse_slot_rejects_spilling_past_close() {
        // Starts inside hours but ends after 17:00.
        assert!(parse_slot("16:30", "17:30").is_err());
    }

    #[test]
    fn parse_slot_rejects_middle_of_night() {
        assert!(parse_slot("02:00", "02:30").is_err());
    }

    #[test]
    fn parse_slot_rejects_off_grid() {
        assert!(parse_slot("09:15", "09:45").is_err());
    }

    #[test]
    fn parse_slot_rejects_reversed_range() {
        assert!(parse_slot("10:00", "09:00").is_err());
    }

    #[test]
    fn parse_time_range_allows_non_clinic_hours() {
        // Availability windows (e.g. an on-call block) are NOT bound to
        // clinic hours — only bookable slots are.
        assert_eq!(parse_time_range("00:00", "23:59").unwrap(), (0, 1439));
    }
}
