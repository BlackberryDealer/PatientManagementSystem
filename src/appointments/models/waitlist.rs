// The waitlist entity: a patient queued for a slot to open up, its lifecycle
// status enum, and the form patients submit to join.

use crate::traits::{Prioritized, Priority, Reportable, StatusManaged, TimeSlotted};
use serde::{Deserialize, Serialize};

/// Lifecycle status of a waitlist entry.
///
/// Stored as TEXT and compared by the templates as lowercase strings, so serde
/// and sqlx both render the variants in lowercase (same contract as
/// `AppointmentStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WaitlistStatus {
    Waiting,
    Offered,
    Accepted,
    Expired,
}

impl WaitlistStatus {
    /// Canonical lowercase string matching DB CHECK values and template comparisons.
    pub fn as_str(&self) -> &'static str {
        match self {
            WaitlistStatus::Waiting => "waiting",
            WaitlistStatus::Offered => "offered",
            WaitlistStatus::Accepted => "accepted",
            WaitlistStatus::Expired => "expired",
        }
    }
}

/// A patient waiting in the priority queue for a slot to open up.
/// Ordered by priority (lower = more urgent) then by created_at.
/// `patient_name` and `doctor_name` are resolved via JOIN in list queries.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct WaitlistEntry {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub room_id: Option<i64>,
    pub appointment_date: chrono::NaiveDate,
    pub requested_start: String,
    pub requested_end: String,
    priority: i32,             // 1=Emergency, 2=Urgent, 3=Normal, 4=Follow-up
    pub notes: Option<String>,
    status: WaitlistStatus,    // waiting | offered | accepted | expired
    pub created_at: chrono::NaiveDateTime,
    #[sqlx(default)]
    pub patient_name: String,  // resolved via JOIN in list queries
    #[sqlx(default)]
    pub doctor_name: String,   // resolved via JOIN in list queries
}

impl WaitlistEntry {
    /// Mark this waitlist entry as accepted (promoted into a real appointment).
    ///
    /// Domain rule: only an entry still in the `waiting` state can be
    /// accepted; offered/accepted/expired entries are final.
    pub fn accept(&mut self) -> Result<(), crate::errors::AppError> {
        if self.status != WaitlistStatus::Waiting {
            return Err(crate::errors::AppError::BadRequest(
                "Only a waiting entry can be promoted".into(),
            ));
        }
        self.status = WaitlistStatus::Accepted;
        Ok(())
    }

    /// Mark this entry as expired (its requested time passed while waiting).
    /// Domain rule: only a waiting entry can expire, final states are immutable.
    pub fn expire(&mut self) -> Result<(), crate::errors::AppError> {
        if self.status != WaitlistStatus::Waiting {
            return Err(crate::errors::AppError::BadRequest(
                "Only a waiting entry can expire".into(),
            ));
        }
        self.status = WaitlistStatus::Expired;
        Ok(())
    }

    /// Read-only accessor for the triage priority as a typed enum (mirrors
    /// `Appointment::priority` so both lifecycle types treat priority the same way).
    pub fn priority(&self) -> Priority { Priority::from_i32(self.priority) }
}

impl TimeSlotted for WaitlistEntry {
    fn start_time(&self) -> &str { &self.requested_start }
    fn end_time(&self) -> &str { &self.requested_end }
}

impl StatusManaged for WaitlistEntry {
    fn current_status(&self) -> &str { self.status.as_str() }

    fn is_active(&self) -> bool { self.status == WaitlistStatus::Waiting }

    fn status_badge_class(&self) -> &str {
        match self.status {
            WaitlistStatus::Waiting  => "is-info",
            WaitlistStatus::Offered  => "is-warning",
            WaitlistStatus::Accepted => "is-success",
            WaitlistStatus::Expired  => "is-light",
        }
    }
}

impl Prioritized for WaitlistEntry {
    // Same as Appointment: the trait defaults handle label/badge mapping.
    fn priority_level(&self) -> i32 { self.priority }
}

impl Reportable for WaitlistEntry {
    fn generate_summary(&self) -> String {
        format!(
            "Waitlist #{} | Requested {} {}-{} | Status: {} | Priority: {}",
            self.id, self.appointment_date, self.requested_start, self.requested_end,
            self.current_status(), self.priority_label()
        )
    }
}

/// Form to add a patient to the waitlist.
/// Room is auto-assigned from the doctor's daily room allocation.
#[derive(Debug, Deserialize)]
pub struct WaitlistForm {
    pub doctor_id: i64,
    pub appointment_date: String,
    pub requested_start: String,
    pub requested_end: String,
    pub priority: i32,
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

#[cfg(test)]
mod tests {
    use super::WaitlistStatus;

    // The templates compare `status == 'waiting'`, so serde output MUST stay
    // lowercase and equal `as_str()`.
    #[test]
    fn waitlist_status_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&WaitlistStatus::Waiting).unwrap(), "\"waiting\"");
        assert_eq!(serde_json::to_string(&WaitlistStatus::Expired).unwrap(), "\"expired\"");
        assert_eq!(WaitlistStatus::Accepted.as_str(), "accepted");
    }

    fn waiting_entry() -> super::WaitlistEntry {
        super::WaitlistEntry {
            id: 1,
            patient_id: 1,
            doctor_id: 1,
            room_id: None,
            appointment_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            requested_start: "09:00".into(),
            requested_end: "09:30".into(),
            priority: 3,
            notes: None,
            status: WaitlistStatus::Waiting,
            created_at: chrono::NaiveDateTime::default(),
            patient_name: String::new(),
            doctor_name: String::new(),
        }
    }

    #[test]
    fn expire_on_waiting_entry_succeeds() {
        use crate::traits::StatusManaged;
        let mut entry = waiting_entry();
        assert!(entry.expire().is_ok());
        assert_eq!(entry.current_status(), "expired");
    }

    #[test]
    fn expire_on_accepted_entry_errors() {
        let mut entry = waiting_entry();
        entry.accept().unwrap();
        assert!(entry.expire().is_err());
    }
}
