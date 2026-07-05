// The core scheduling entity: Appointment, its lifecycle status enum, and the
// joined read model (AppointmentView) the list/detail pages render.

use crate::traits::{Prioritized, Priority, Reportable, StatusManaged, TimeSlotted};
use serde::{Deserialize, Serialize};

/// Lifecycle status of an appointment.
///
/// Stored as TEXT and compared by the Tera templates as lowercase strings, so
/// both sqlx (`FromRow`/binding) and serde render the variants in lowercase.
/// Using an enum instead of `String` makes illegal states unrepresentable and
/// lets every `match` on status be exhaustive (no catch-all arm).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AppointmentStatus {
    Scheduled,
    Completed,
    Cancelled,
}

impl AppointmentStatus {
    /// Canonical lowercase string, matches the DB CHECK values and the
    /// strings the templates compare against.
    pub fn as_str(&self) -> &'static str {
        match self {
            AppointmentStatus::Scheduled => "scheduled",
            AppointmentStatus::Completed => "completed",
            AppointmentStatus::Cancelled => "cancelled",
        }
    }
}

/// The core scheduling entity: a patient-doctor meeting at a specific
/// date and time window in an auto-assigned room. Status and priority
/// are private with guarded accessors so state transitions (cancel,
/// reassign, reschedule, assign_room) live on the struct itself.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Appointment {
    pub id: i64,
    pub patient_id: i64,
    doctor_id: i64,            // private: only reassign_to() may change it
    pub appointment_date: chrono::NaiveDate,
    pub start_time: String,    // HH:MM
    pub end_time: String,      // HH:MM
    status: AppointmentStatus, // scheduled | completed | cancelled
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub room_id: Option<i64>,
    priority: i32,             // 1=Emergency, 2=Urgent, 3=Normal, 4=Follow-up
}

impl Appointment {
    /// Read-only accessor for the assigned doctor. The field is private so the
    /// only way to change it is `reassign_to`, which keeps the "only a
    /// scheduled appointment may be reassigned" rule in one place.
    pub fn doctor_id(&self) -> i64 { self.doctor_id }

    /// Cancel this appointment. Only an active (scheduled) appointment can be
    /// cancelled; completed or already-cancelled ones are locked history.
    pub fn cancel(&mut self) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Appointment is already cancelled or completed".into(),
            ));
        }
        self.status = AppointmentStatus::Cancelled;
        Ok(())
    }

    /// Mark this appointment as completed (the visit took place). Unlike
    /// cancellation, completion keeps the slot occupied: the time was
    /// genuinely used, so `check_conflict` still counts it as busy.
    pub fn complete(&mut self) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a scheduled appointment can be marked completed".into(),
            ));
        }
        self.status = AppointmentStatus::Completed;
        Ok(())
    }

    /// Move this appointment to a different doctor. Guarded the same way as
    /// `cancel`, so nothing can hand off a completed or cancelled visit.
    pub fn reassign_to(&mut self, new_doctor_id: i64) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a scheduled appointment can be reassigned".into(),
            ));
        }
        self.doctor_id = new_doctor_id;
        Ok(())
    }

    /// Move this appointment to a new date and time window, keeping the same
    /// doctor and room. Changing the doctor is a separate flow
    /// (`reassign_to`); this only touches when the visit happens.
    pub fn reschedule_to(
        &mut self,
        new_date: chrono::NaiveDate,
        new_start: &str,
        new_end: &str,
    ) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a scheduled appointment can be rescheduled".into(),
            ));
        }
        self.appointment_date = new_date;
        self.start_time = new_start.to_string();
        self.end_time = new_end.to_string();
        Ok(())
    }

    /// Assign or change the consultation room. Rooms are auto-assigned at
    /// booking time from the doctor's daily allocation; this is the manual
    /// override for moving one appointment into a different room.
    pub fn assign_room(&mut self, room_id: i64) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a scheduled appointment can be assigned a room".into(),
            ));
        }
        self.room_id = Some(room_id);
        Ok(())
    }

    /// Read-only accessor for the triage priority as a typed enum. The field
    /// stays private so the range check in `set_priority` can't be bypassed.
    pub fn priority(&self) -> Priority { Priority::from_i32(self.priority) }

    /// Re-triage this appointment. The 1-4 range is checked explicitly here
    /// because `Priority::from_i32` fails safe (clamps to Normal) rather than
    /// rejecting bad input, which is the wrong behaviour for a form submission.
    pub fn set_priority(&mut self, priority: i32) -> Result<(), crate::errors::AppError> {
        if !self.is_active() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a scheduled appointment can be re-prioritized".into(),
            ));
        }
        if !(1..=4).contains(&priority) {
            return Err(crate::errors::AppError::BadRequest(
                "Priority must be between 1 (Emergency) and 4 (Follow-up)".into(),
            ));
        }
        self.priority = priority;
        Ok(())
    }
}

// ============================================================
// Trait implementations (OOP via Rust traits, Tutorial 05)
// ============================================================

impl TimeSlotted for Appointment {
    fn start_time(&self) -> &str { &self.start_time }
    fn end_time(&self) -> &str { &self.end_time }
}

impl StatusManaged for Appointment {
    fn current_status(&self) -> &str { self.status.as_str() }

    fn is_active(&self) -> bool { self.status == AppointmentStatus::Scheduled }

    fn status_badge_class(&self) -> &str {
        match self.status {
            AppointmentStatus::Scheduled => "is-info",
            AppointmentStatus::Completed => "is-success",
            AppointmentStatus::Cancelled => "is-danger",
        }
    }
}

impl Prioritized for Appointment {
    // The trait's default `priority_label` / `priority_badge_class` already
    // delegate to the Priority enum, so only the raw level is supplied here.
    fn priority_level(&self) -> i32 { self.priority }
}

impl Reportable for Appointment {
    fn generate_summary(&self) -> String {
        format!(
            "Appointment on {} from {}–{} | Status: {} | Priority: {}",
            self.appointment_date, self.start_time, self.end_time,
            self.current_status(), self.priority_label()
        )
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
    #[sqlx(default)]
    pub patient_id: i64,
    #[sqlx(default)]
    pub doctor_id: i64,
}

#[cfg(test)]
mod tests {
    use super::AppointmentStatus;

    // The Tera templates compare `status == 'scheduled'`, so the serde
    // representation MUST stay lowercase and equal `as_str()`.
    #[test]
    fn appointment_status_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&AppointmentStatus::Scheduled).unwrap(), "\"scheduled\"");
        assert_eq!(serde_json::to_string(&AppointmentStatus::Completed).unwrap(), "\"completed\"");
        assert_eq!(serde_json::to_string(&AppointmentStatus::Cancelled).unwrap(), "\"cancelled\"");
        // serde output and the badge-logic `as_str()` must never diverge.
        assert_eq!(AppointmentStatus::Scheduled.as_str(), "scheduled");
    }
}
