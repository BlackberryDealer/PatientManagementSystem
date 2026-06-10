use crate::traits::TimeSlotted;
use serde::{Deserialize, Serialize};

/// Represents a doctor's availability slot — either a recurring
/// weekly time window or a one-off blocked/leave date.
/// Note: `is_recurring` and `is_blocked` are `i32` (not `bool`)
/// because SQLite stores BOOLEAN as INTEGER 0/1.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DoctorAvailability {
    pub id: i64,
    pub doctor_id: i64,
    pub day_of_week: i32,       // 0=Sun .. 6=Sat
    pub start_time: String,     // HH:MM
    pub end_time: String,       // HH:MM
    pub is_recurring: i32,      // 1 = recurring weekly, 0 = one-off
    pub specific_date: Option<chrono::NaiveDate>,
    pub is_blocked: i32,        // 1 = blocked/unavailable, 0 = available
}

impl DoctorAvailability {
    /// SQLite stores BOOLEAN as INTEGER 0/1; expose proper bools so
    /// callers never compare against raw integers.
    pub fn recurring(&self) -> bool { self.is_recurring != 0 }
    pub fn blocked(&self) -> bool { self.is_blocked != 0 }
}

impl TimeSlotted for DoctorAvailability {
    fn start_time(&self) -> &str { &self.start_time }
    fn end_time(&self) -> &str { &self.end_time }
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

impl SetAvailabilityForm {
    /// Validate the submitted window: real day-of-week, well-formed
    /// times, start strictly before end. Availability windows are not
    /// forced onto the 30-minute booking grid — only appointments are.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        if !(0..=6).contains(&self.day_of_week) {
            return Err(crate::errors::AppError::BadRequest(
                "Day of week must be between 0 (Sunday) and 6 (Saturday)".into(),
            ));
        }
        crate::time::parse_time_range(&self.start_time, &self.end_time)?;
        if let Some(date) = self.specific_date.as_deref().filter(|s| !s.is_empty()) {
            if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
                return Err(crate::errors::AppError::BadRequest(
                    "Specific date must be a valid date in YYYY-MM-DD format".into(),
                ));
            }
        }
        Ok(())
    }

    /// HTML checkboxes send "on" when checked and are absent otherwise;
    /// that decoding detail belongs to the form, not the service layer.
    pub fn recurring(&self) -> bool {
        self.is_recurring.as_deref() == Some("on")
    }

    pub fn blocked(&self) -> bool {
        self.is_blocked.as_deref() == Some("on")
    }

    /// The one-off date with empty strings normalised to `None`,
    /// so SQLite receives a proper NULL.
    pub fn specific_date_or_none(&self) -> Option<String> {
        self.specific_date
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
    }
}
