use crate::traits::{Prioritized, Reportable, StatusManaged, TimeSlotted};
use serde::{Deserialize, Serialize};

// ============================================================
// Priority Levels (lower number = higher priority)
// ============================================================

/// Priority levels matching hospital triage standards.
/// 1 = Emergency (life-threatening), 2 = Urgent, 3 = Normal, 4 = Follow-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Emergency = 1,
    Urgent = 2,
    Normal = 3,
    FollowUp = 4,
}

impl Priority {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Priority::Emergency,
            2 => Priority::Urgent,
            4 => Priority::FollowUp,
            _ => Priority::Normal,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Priority::Emergency => "Emergency",
            Priority::Urgent => "Urgent",
            Priority::Normal => "Normal",
            Priority::FollowUp => "Follow-up",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Priority::Emergency => "is-danger",
            Priority::Urgent => "is-warning",
            Priority::Normal => "is-info",
            Priority::FollowUp => "is-success",
        }
    }

    /// Triage rule: only Emergency and Urgent cases may bump an existing
    /// booking out of its slot. Normal and Follow-up visits must use
    /// standard booking or join the waitlist.
    pub fn can_override(&self) -> bool {
        matches!(self, Priority::Emergency | Priority::Urgent)
    }
}

// ============================================================
// Room — consultation room or equipment resource
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Room {
    pub id: i64,
    pub name: String,
    pub room_type: String,
    pub floor: Option<String>,
    pub is_active: i32,
    pub notes: Option<String>,
}

// ============================================================
// Appointment — core scheduling entity
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Appointment {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub appointment_date: chrono::NaiveDate,
    pub start_time: String,    // HH:MM
    pub end_time: String,      // HH:MM
    status: String,            // scheduled | completed | cancelled
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub room_id: Option<i64>,
    pub priority: i32,         // 1=Emergency, 2=Urgent, 3=Normal, 4=Follow-up
}

impl Appointment {
    /// Cancel this appointment.
    ///
    /// Domain rule: only an active (scheduled) appointment may be cancelled —
    /// completed or already-cancelled appointments are immutable history.
    /// Mutates internal state, so it requires `&mut self`; callers persist
    /// the new status afterwards.
    pub fn cancel(&mut self) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Appointment is already cancelled or completed".into(),
            ));
        }
        self.status = "cancelled".to_string();
        Ok(())
    }

    /// Reassign this appointment to a different doctor.
    ///
    /// Domain rule (same as `cancel`): only an active/scheduled appointment may
    /// be reassigned — completed or cancelled appointments are immutable
    /// history. The rule and the state change live on the object, so callers
    /// can never drive a closed appointment to a new doctor; they persist the
    /// new `doctor_id` afterwards.
    pub fn reassign_to(&mut self, new_doctor_id: i64) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a scheduled appointment can be reassigned".into(),
            ));
        }
        self.doctor_id = new_doctor_id;
        Ok(())
    }
}

/// Form data submitted when a patient books an appointment.
#[derive(Debug, Deserialize)]
pub struct BookAppointmentForm {
    pub doctor_id: i64,
    pub appointment_date: String, // YYYY-MM-DD
    pub start_time: String,       // HH:MM
    pub end_time: String,         // HH:MM
    pub room_id: Option<i64>,
    pub priority: Option<i32>,
    pub notes: Option<String>,
}

impl BookAppointmentForm {
    /// All booking input rules in one place: a valid, non-past date and a
    /// grid-aligned slot. Mirrors `SuggestSlotForm::validate`, so every form
    /// owns its own validation before anything reaches the service or DB
    /// (Route -> Validation -> Business Logic -> DB).
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        crate::time::parse_booking_date(&self.appointment_date)?;
        crate::time::parse_slot(&self.start_time, &self.end_time)?;
        Ok(())
    }

    /// Requested triage priority, defaulting to Normal when the form omits it.
    /// Keeps the "Normal is the default" rule and the numeric mapping on the
    /// `Priority` domain type, not as a magic `3` in the service layer.
    pub fn requested_priority(&self) -> Priority {
        Priority::from_i32(self.priority.unwrap_or(Priority::Normal as i32))
    }
}

/// Form for the "suggest slot" feature — find next available time.
#[derive(Debug, Deserialize)]
pub struct SuggestSlotForm {
    pub doctor_id: i64,
    pub appointment_date: String,
    pub duration_minutes: i32,
    pub room_id: Option<i64>,
}

impl SuggestSlotForm {
    /// All request rules in one place: a valid, non-past date and a sane
    /// duration. Checked before the earliest-slot algorithm runs, so the
    /// algorithm stays purely about gap-finding (Route -> Validation -> Logic).
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        crate::time::parse_booking_date(&self.appointment_date)?;
        if self.duration_minutes <= 0 || self.duration_minutes > 480 {
            return Err(crate::errors::AppError::BadRequest(
                "Duration must be 1–480 minutes".into(),
            ));
        }
        Ok(())
    }
}

// ============================================================
// DaySchedule — pure earliest-gap finder (Algorithm 2 core)
// ============================================================

/// A doctor's booked intervals for a single day, as `(start, end)` pairs in
/// minutes since midnight.
///
/// Holds the pure scheduling math for Algorithm 2 (earliest available slot) so
/// the service layer only fetches rows and maps the result, and so the
/// gap-finding logic is unit-testable without a database. This mirrors how
/// Algorithm 1 (interval overlap) lives on the `TimeSlotted` trait — both
/// scheduling algorithms now sit on domain objects, not inline in services.
pub struct DaySchedule {
    busy: Vec<(i32, i32)>, // sorted by start time
}

impl DaySchedule {
    /// Build from booked intervals; sorts defensively so callers need not
    /// rely on the SQL `ORDER BY`.
    pub fn new(mut busy: Vec<(i32, i32)>) -> Self {
        busy.sort_by_key(|&(start, _)| start);
        Self { busy }
    }

    /// Earliest start time (minutes since midnight) with room for a
    /// `duration`-minute appointment between `open` and `close`, walking the
    /// gaps between booked intervals. Returns `None` if the day is full.
    pub fn earliest_gap(&self, duration: i32, open: i32, close: i32) -> Option<i32> {
        let mut cursor = open;
        for &(start, end) in &self.busy {
            // A gap wide enough sits before this interval.
            if cursor + duration <= start {
                return Some(cursor);
            }
            // Otherwise advance past this interval.
            if end > cursor {
                cursor = end;
            }
        }
        // Gap at the end of the day, if one remains.
        if cursor + duration <= close {
            Some(cursor)
        } else {
            None
        }
    }
}

/// Joined view: appointment with patient/doctor names and room for display.
/// Field names match the aliased columns in `APPOINTMENT_VIEW_SELECT`, so rows
/// deserialize directly via `FromRow` (no manual tuple mapping needed).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AppointmentView {
    pub id: i64,
    pub patient_name: String,
    pub doctor_name: String,
    pub appointment_date: chrono::NaiveDate,
    pub start_time: String,
    pub end_time: String,
    pub status: String,
    pub notes: Option<String>,
    pub room_name: Option<String>,
    pub priority: i32,
}

// ============================================================
// Waitlist — priority queue entry
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct WaitlistEntry {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub room_id: Option<i64>,
    pub appointment_date: chrono::NaiveDate,
    pub requested_start: String,
    pub requested_end: String,
    pub priority: i32,
    pub notes: Option<String>,
    status: String,            // waiting | offered | accepted | expired
    pub created_at: chrono::NaiveDateTime,
}

impl WaitlistEntry {
    /// Mark this waitlist entry as accepted (promoted into a real appointment).
    ///
    /// Domain rule: only an entry still in the `waiting` state can be
    /// accepted; offered/accepted/expired entries are final.
    pub fn accept(&mut self) -> Result<(), crate::errors::AppError> {
        if self.status != "waiting" {
            return Err(crate::errors::AppError::BadRequest(
                "Only a waiting entry can be promoted".into(),
            ));
        }
        self.status = "accepted".to_string();
        Ok(())
    }
}

/// Form to add a patient to the waitlist.
#[derive(Debug, Deserialize)]
pub struct WaitlistForm {
    pub doctor_id: i64,
    pub appointment_date: String,
    pub requested_start: String,
    pub requested_end: String,
    pub priority: i32,
    pub room_id: Option<i64>,
    pub notes: Option<String>,
}

impl WaitlistForm {
    /// All waitlist input rules in one place: a valid, non-past date, a
    /// grid-aligned requested slot (a promotion books it as a real
    /// appointment, which decomposes into 30-minute slots), and a real
    /// triage priority. Mirrors `BookAppointmentForm::validate`, so every
    /// form owns its own validation before anything reaches the service or
    /// DB (Route -> Validation -> Business Logic -> DB).
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        crate::time::parse_booking_date(&self.appointment_date)?;
        crate::time::parse_slot(&self.requested_start, &self.requested_end)?;
        if !(1..=4).contains(&self.priority) {
            return Err(crate::errors::AppError::BadRequest(
                "Priority must be between 1 (Emergency) and 4 (Follow-up)".into(),
            ));
        }
        Ok(())
    }
}

// ============================================================
// Trait implementations — OOP via Rust traits (Tutorial 05)
// ============================================================

impl TimeSlotted for Appointment {
    fn start_time(&self) -> &str { &self.start_time }
    fn end_time(&self) -> &str { &self.end_time }
}

impl StatusManaged for Appointment {
    fn current_status(&self) -> &str { &self.status }

    fn is_active(&self) -> bool { self.status == "scheduled" }

    fn status_badge_class(&self) -> &str {
        match self.status.as_str() {
            "scheduled" => "is-info",
            "completed" => "is-success",
            "cancelled" => "is-danger",
            _           => "is-light",
        }
    }
}

impl Prioritized for Appointment {
    fn priority_level(&self) -> i32 { self.priority }

    // Override defaults to delegate to the Priority enum, keeping the enum in use
    fn priority_label(&self) -> &str {
        Priority::from_i32(self.priority).label()
    }

    fn priority_badge_class(&self) -> &str {
        Priority::from_i32(self.priority).css_class()
    }
}

impl Reportable for Appointment {
    fn generate_summary(&self) -> String {
        format!(
            "Appointment on {} from {}–{} | Status: {} | Priority: {}",
            self.appointment_date, self.start_time, self.end_time,
            self.status, self.priority_label()
        )
    }
}

impl TimeSlotted for WaitlistEntry {
    fn start_time(&self) -> &str { &self.requested_start }
    fn end_time(&self) -> &str { &self.requested_end }
}

impl StatusManaged for WaitlistEntry {
    fn current_status(&self) -> &str { &self.status }

    fn is_active(&self) -> bool { self.status == "waiting" }

    fn status_badge_class(&self) -> &str {
        match self.status.as_str() {
            "waiting"  => "is-info",
            "offered"  => "is-warning",
            "accepted" => "is-success",
            "expired"  => "is-light",
            _          => "is-light",
        }
    }
}

// ============================================================
// CalendarMonth — business object for the calendar view
// ============================================================

/// One cell in the calendar grid. Filler cells from the previous month
/// have `is_current_month = false` and `day = 0`.
#[derive(Debug, Serialize)]
pub struct CalendarDay {
    pub day: u32,
    pub date: String,
    pub is_today: bool,
    pub is_current_month: bool,
    pub count: usize,
}

/// A validated calendar month plus its week grid.
///
/// Owns all calendar arithmetic (days-in-month, weekday offset,
/// previous/next month) so the route handler only extracts inputs
/// and renders the result. Construction validates the year/month,
/// making an out-of-range month unrepresentable downstream.
pub struct CalendarMonth {
    pub year: i32,
    pub month: u32,
    days: Vec<CalendarDay>,
}

impl CalendarMonth {
    const MONTH_NAMES: [&'static str; 12] = [
        "January", "February", "March", "April", "May", "June", "July",
        "August", "September", "October", "November", "December",
    ];

    /// Validation gate: rejects an out-of-range month/year with a 400
    /// instead of letting it panic deeper in date arithmetic.
    pub fn new(year: i32, month: u32) -> Result<Self, crate::errors::AppError> {
        if !(1..=12).contains(&month) || !(1970..=2100).contains(&year) {
            return Err(crate::errors::AppError::BadRequest(
                "Invalid calendar year or month".into(),
            ));
        }
        Ok(Self { year, month, days: Vec::new() })
    }

    fn first_day(&self) -> chrono::NaiveDate {
        // Safe: year/month were validated in `new`.
        chrono::NaiveDate::from_ymd_opt(self.year, self.month, 1).unwrap()
    }

    pub fn days_in_month(&self) -> u32 {
        use chrono::NaiveDate;
        let next = if self.month == 12 {
            NaiveDate::from_ymd_opt(self.year + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(self.year, self.month + 1, 1).unwrap()
        };
        next.signed_duration_since(self.first_day()).num_days() as u32
    }

    /// Inclusive `(from, to)` ISO date strings spanning the whole month,
    /// for querying appointment counts.
    pub fn date_range(&self) -> (String, String) {
        (
            format!("{}-{:02}-01", self.year, self.month),
            format!("{}-{:02}-{:02}", self.year, self.month, self.days_in_month()),
        )
    }

    /// Build the day grid: leading filler cells so the 1st lands on the
    /// correct weekday column (Monday-first), then one cell per day with
    /// its appointment count.
    pub fn build_grid(
        &mut self,
        today: chrono::NaiveDate,
        counts: &std::collections::HashMap<String, usize>,
    ) {
        use chrono::Datelike;
        self.days.clear();

        let start_dow = self.first_day().weekday().num_days_from_monday(); // 0=Mon
        for _ in 0..start_dow {
            self.days.push(CalendarDay {
                day: 0,
                date: String::new(),
                is_today: false,
                is_current_month: false,
                count: 0,
            });
        }

        let today_str = today.format("%Y-%m-%d").to_string();
        for d in 1..=self.days_in_month() {
            let date_str = format!("{}-{:02}-{:02}", self.year, self.month, d);
            self.days.push(CalendarDay {
                day: d,
                is_today: date_str == today_str,
                is_current_month: true,
                count: counts.get(&date_str).copied().unwrap_or(0),
                date: date_str,
            });
        }
    }

    /// The grid grouped into rows of seven for rendering.
    pub fn weeks(&self) -> Vec<&[CalendarDay]> {
        self.days.chunks(7).collect()
    }

    pub fn prev(&self) -> (i32, u32) {
        if self.month == 1 { (self.year - 1, 12) } else { (self.year, self.month - 1) }
    }

    pub fn next(&self) -> (i32, u32) {
        if self.month == 12 { (self.year + 1, 1) } else { (self.year, self.month + 1) }
    }

    pub fn month_name(&self) -> &'static str {
        Self::MONTH_NAMES[(self.month - 1) as usize]
    }
}

impl Prioritized for WaitlistEntry {
    fn priority_level(&self) -> i32 { self.priority }

    fn priority_label(&self) -> &str {
        Priority::from_i32(self.priority).label()
    }

    fn priority_badge_class(&self) -> &str {
        Priority::from_i32(self.priority).css_class()
    }
}

// ============================================================
// Unit tests for the DaySchedule earliest-gap algorithm
// ============================================================

#[cfg(test)]
mod tests {
    use super::DaySchedule;

    // Working day: 08:00 (480) .. 17:00 (1020), matching the clinic hours.
    const OPEN: i32 = 8 * 60;
    const CLOSE: i32 = 17 * 60;

    #[test]
    fn empty_day_returns_opening_time() {
        let day = DaySchedule::new(vec![]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(OPEN));
    }

    #[test]
    fn gap_before_first_appointment() {
        // First booking at 09:00 leaves the 08:00 slot free.
        let day = DaySchedule::new(vec![(9 * 60, 10 * 60)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(OPEN));
    }

    #[test]
    fn finds_gap_between_appointments() {
        // 08:00–09:00 and 09:30–10:00 booked; a 30-min slot fits at 09:00.
        let day = DaySchedule::new(vec![(8 * 60, 9 * 60), (9 * 60 + 30, 10 * 60)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(9 * 60));
    }

    #[test]
    fn unsorted_input_is_sorted_defensively() {
        // Same intervals supplied out of order must give the same answer.
        let day = DaySchedule::new(vec![(9 * 60 + 30, 10 * 60), (8 * 60, 9 * 60)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(9 * 60));
    }

    #[test]
    fn gap_at_end_of_day() {
        // Booked solid until 16:30; a 30-min slot fits at 16:30.
        let day = DaySchedule::new(vec![(OPEN, 16 * 60 + 30)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(16 * 60 + 30));
    }

    #[test]
    fn full_day_returns_none() {
        let day = DaySchedule::new(vec![(OPEN, CLOSE)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), None);
    }

    #[test]
    fn remaining_gap_too_short_returns_none() {
        // Only 15 minutes left before close — a 30-min slot cannot fit.
        let day = DaySchedule::new(vec![(OPEN, CLOSE - 15)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), None);
    }
}
