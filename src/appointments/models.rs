use serde::{Deserialize, Serialize};

// ============================================================
// Appointment — core scheduling entity
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Appointment {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub appointment_date: chrono::NaiveDate,
    pub start_time: String, // HH:MM
    pub end_time: String,   // HH:MM
    pub status: String,     // scheduled | completed | cancelled
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

/// Form data submitted when a patient books an appointment.
#[derive(Debug, Deserialize)]
pub struct BookAppointmentForm {
    pub doctor_id: i64,
    pub appointment_date: String, // YYYY-MM-DD
    pub start_time: String,       // HH:MM
    pub end_time: String,         // HH:MM
    pub notes: Option<String>,
}

/// Joined view: appointment with patient and doctor names for display.
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
}
