use crate::traits::Reportable;
use serde::{Deserialize, Serialize};

/// Medical record: diagnosis and treatment linked to an appointment.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct MedicalRecord {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub appointment_id: Option<i64>,
    pub diagnosis: Option<String>,
    pub treatment: Option<String>,
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

// ============================================================
// Trait implementations — OOP via Rust traits (Tutorial 05)
// ============================================================

impl Reportable for MedicalRecord {
    fn generate_summary(&self) -> String {
        format!(
            "Record #{} | Diagnosis: {} | Treatment: {}",
            self.id,
            self.diagnosis.as_deref().unwrap_or("N/A"),
            self.treatment.as_deref().unwrap_or("N/A"),
        )
    }
}

/// Form for creating a new medical record.
#[derive(Debug, Deserialize)]
pub struct CreateRecordForm {
    pub patient_id: i64,
    pub appointment_id: Option<i64>,
    pub diagnosis: String,
    pub treatment: String,
    pub notes: Option<String>,
}

/// Prescription linked to an appointment.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Prescription {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub appointment_id: Option<i64>,
    pub medication_name: String,
    pub dosage: String,
    pub frequency: String,
    pub duration: Option<String>,
    pub notes: Option<String>,
    pub prescribed_at: chrono::NaiveDateTime,
}
