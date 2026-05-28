use serde::{Deserialize, Serialize};

/// Represents a doctor's availability slot — either a recurring
/// weekly time window or a one-off blocked/leave date.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DoctorAvailability {
    pub id: i64,
    pub doctor_id: i64,
    pub day_of_week: i32,       // 0=Sun .. 6=Sat
    pub start_time: String,     // HH:MM
    pub end_time: String,       // HH:MM
    pub is_recurring: bool,
    pub specific_date: Option<chrono::NaiveDate>,
    pub is_blocked: bool,
}

/// Form submitted by a doctor to set their availability.
#[derive(Debug, Deserialize)]
pub struct SetAvailabilityForm {
    pub day_of_week: i32,
    pub start_time: String,
    pub end_time: String,
    pub is_recurring: Option<String>,    // "on" if checked
    pub specific_date: Option<String>,   // YYYY-MM-DD for one-off dates
    pub is_blocked: Option<String>,      // "on" if blocked/leave
}
