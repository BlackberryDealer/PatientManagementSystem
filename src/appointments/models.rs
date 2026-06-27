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

/// Form for the "suggest slot" feature — find next available time.
#[derive(Debug, Deserialize)]
pub struct SuggestSlotForm {
    pub doctor_id: i64,
    pub appointment_date: String,
    pub duration_minutes: i32,
    pub room_id: Option<i64>,
}

/// Joined view: appointment with patient/doctor names and room for display.
#[derive(Debug, Serialize)]
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
