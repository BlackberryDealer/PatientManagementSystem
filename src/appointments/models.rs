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
    pub status: String,        // scheduled | completed | cancelled
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub room_id: Option<i64>,
    pub priority: i32,         // 1=Emergency, 2=Urgent, 3=Normal, 4=Follow-up
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
    pub status: String,        // waiting | offered | accepted | expired
    pub created_at: chrono::NaiveDateTime,
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

impl Prioritized for WaitlistEntry {
    fn priority_level(&self) -> i32 { self.priority }

    fn priority_label(&self) -> &str {
        Priority::from_i32(self.priority).label()
    }

    fn priority_badge_class(&self) -> &str {
        Priority::from_i32(self.priority).css_class()
    }
}
