// HTTP request/response DTOs for the appointment handlers. Each form
// validates itself before anything reaches the service or the DB.

use crate::traits::Priority;
use serde::{Deserialize, Serialize};

/// Raw POST body of the booking form. Who the appointment is FOR and WITH
/// depends on the submitter's role, so both parties are optional at the HTTP
/// layer and `services::resolve_booking_target` applies the business rules:
///   patient → themselves + their chosen doctor, always Normal priority;
///   doctor  → their chosen patient + themselves (a doctor never books
///             another doctor's schedule, reassignment is a separate flow);
///   admin   → their chosen patient + their chosen doctor.
#[derive(Debug, Deserialize)]
pub struct BookRequestForm {
    pub patient_id: Option<i64>,
    pub doctor_id: Option<i64>,
    pub appointment_date: String, // YYYY-MM-DD
    pub start_time: String,       // HH:MM
    pub end_time: String,         // HH:MM
    pub priority: Option<i32>,
    pub notes: Option<String>,
}

/// A fully-resolved booking: every role question already answered, so the
/// booking services stay role-agnostic. Room is auto-assigned from the
/// doctor's daily room allocation, nobody selects a room manually.
#[derive(Debug, Deserialize)]
pub struct BookAppointmentForm {
    pub doctor_id: i64,
    pub appointment_date: String, // YYYY-MM-DD
    pub start_time: String,       // HH:MM
    pub end_time: String,         // HH:MM
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

/// Form submitted when moving an existing appointment to a new date/time.
///
/// Deliberately carries only the fields a reschedule may change (date + slot).
/// The doctor and room stay as they were, changing the doctor is a separate
/// concern already handled by `reassign_appointment` (Algorithm 4), so leaving
/// them out here keeps the two features from overlapping and keeps the form
/// the minimum a reschedule needs.
#[derive(Debug, Deserialize)]
pub struct RescheduleForm {
    pub appointment_date: String, // YYYY-MM-DD
    pub start_time: String,       // HH:MM
    pub end_time: String,         // HH:MM
}

impl RescheduleForm {
    /// Same input rules as a fresh booking (valid, non-past date and a
    /// grid-aligned slot), so a rescheduled slot can never be one a brand-new
    /// booking would have rejected. Returns the parsed date so the service does
    /// not have to parse the same string a second time.
    pub fn validate(&self) -> Result<chrono::NaiveDate, crate::errors::AppError> {
        let date = crate::time::parse_booking_date(&self.appointment_date)?;
        crate::time::parse_slot(&self.start_time, &self.end_time)?;
        Ok(date)
    }
}

/// Form a doctor submits to assign (or change) an appointment's room.
///
/// `room_id` is a plain `i64` (not optional): the assign UI always offers a
/// concrete room, so there is no empty value to mis-parse, the same reason the
/// staff booking form dropped its blank "no room" option.
#[derive(Debug, Deserialize)]
pub struct AssignRoomForm {
    pub room_id: i64,
}

/// Form a doctor submits to re-triage an appointment's priority.
/// Plain `i32` for the same reason as `AssignRoomForm`: the UI always
/// offers a concrete level, so there is no empty value to mis-parse.
#[derive(Debug, Deserialize)]
pub struct SetPriorityForm {
    pub priority: i32,
}

/// Form for the "suggest slot" feature, find next available time.
/// Room is auto-assigned from the doctor's daily room allocation.
#[derive(Debug, Deserialize)]
pub struct SuggestSlotForm {
    pub doctor_id: i64,
    pub appointment_date: String,
    pub duration_minutes: i32,
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

/// Form for the batch-reassignment page: which doctor is unavailable, and on
/// which date should their appointments be redistributed (Algorithm 5).
#[derive(Debug, Deserialize)]
pub struct ReassignDayForm {
    pub doctor_id: i64,
    pub appointment_date: String, // YYYY-MM-DD
}

/// JSON payload for the dynamic booking form: the free 30-minute start slots
/// for a doctor on a date, so the patient picks from times that are actually
/// open instead of guessing and hitting conflict errors.
#[derive(Debug, Serialize)]
pub struct FreeSlotsResponse {
    pub slots: Vec<String>,
}

/// One 30-minute start slot plus whether it is currently unoccupied. Powers the
/// staff "show every slot, mark the taken ones" booking dropdown, where a doctor
/// may deliberately pick an occupied slot to trigger a priority override.
#[derive(Debug, Serialize)]
pub struct SlotAvailability {
    pub time: String, // HH:MM
    pub free: bool,   // true = open, false = already booked (overridable)
}

/// JSON payload for the staff booking form: every 30-minute start slot within
/// clinic hours the doctor's availability rules allow, each marked free or
/// occupied. Unlike `FreeSlotsResponse` (patient view, free slots only) this
/// lets doctors/admins see and select occupied slots for a priority override.
#[derive(Debug, Serialize)]
pub struct AllSlotsResponse {
    pub slots: Vec<SlotAvailability>,
}
