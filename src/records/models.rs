use crate::traits::Reportable;
use serde::{Deserialize, Serialize};

/// Deserialize an optional numeric form field that may arrive as an empty value.
///
/// An HTML form submits an unset optional field as `key=` (empty string).
/// `serde_urlencoded` can't parse that straight into `Option<i64>`, it errors
/// on the empty string rather than yielding `None`, which would 400 the whole
/// submission. Treating a missing key or an empty/whitespace value as `None`
/// makes an "optional appointment link" genuinely optional.
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let raw = Option::<String>::deserialize(de)?;
    match raw.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(s) => s.parse::<T>().map(Some).map_err(serde::de::Error::custom),
    }
}

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
// Trait implementations, OOP via Rust traits (Tutorial 05)
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
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub appointment_id: Option<i64>,
    pub diagnosis: String,
    pub treatment: String,
    pub notes: Option<String>,
}

impl CreateRecordForm {
    /// A record must carry a diagnosis and a treatment, checked before
    /// anything touches the database, mirroring `PrescriptionForm::validate`.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.diagnosis.trim().is_empty() {
            return Err(AppError::BadRequest("Diagnosis is required".into()));
        }
        if self.treatment.trim().is_empty() {
            return Err(AppError::BadRequest("Treatment is required".into()));
        }
        Ok(())
    }
}

/// One option for the create-record form's "link to appointment" dropdown: a
/// patient's appointment plus a ready-made label the JS drops straight into a
/// `<select>`. Populated on the fly when the doctor picks a patient, so a visit
/// is chosen from a list instead of an appointment id typed by hand, the same
/// "no id typing" guard the patient dropdown already gives.
#[derive(Debug, Serialize)]
pub struct PatientAppointmentOption {
    pub id: i64,
    pub label: String,
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

impl Reportable for Prescription {
    fn generate_summary(&self) -> String {
        format!(
            "{}, {}, {}{}",
            self.medication_name,
            self.dosage,
            self.frequency,
            self.duration
                .as_deref()
                .map(|d| format!(" for {}", d))
                .unwrap_or_default(),
        )
    }
}

/// Form for a doctor writing a new prescription.
#[derive(Debug, Deserialize)]
pub struct PrescriptionForm {
    pub patient_id: i64,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub appointment_id: Option<i64>,
    pub medication_name: String,
    pub dosage: String,
    pub frequency: String,
    pub duration: Option<String>,
    pub notes: Option<String>,
}

impl PrescriptionForm {
    /// A prescription must name the medication, dose, and frequency.
    /// Checked before anything touches the database.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.medication_name.trim().is_empty() {
            return Err(AppError::BadRequest("Medication name is required".into()));
        }
        if self.dosage.trim().is_empty() {
            return Err(AppError::BadRequest("Dosage is required".into()));
        }
        if self.frequency.trim().is_empty() {
            return Err(AppError::BadRequest("Frequency is required".into()));
        }
        Ok(())
    }
}

// ============================================================
// Patient History Timeline (advanced feature, project spec)
// ============================================================

/// Semantic kind of a timeline event. Each variant knows its own
/// Font Awesome icon and default Bulma colour, so the service layer
/// never hardcodes presentation strings, it picks the variant and the
/// kind supplies the display details.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventKind {
    Appointment,
    MedicalRecord,
    Prescription,
    Invoice,
}

impl TimelineEventKind {
    /// Snake-case string used in CSS classes and data attributes.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Appointment   => "appointment",
            Self::MedicalRecord => "record",
            Self::Prescription  => "prescription",
            Self::Invoice       => "invoice",
        }
    }

    /// Font Awesome icon class. A single place to change the icon pack.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Appointment   => "fa-calendar-check",
            Self::MedicalRecord => "fa-notes-medical",
            Self::Prescription  => "fa-prescription",
            Self::Invoice       => "fa-file-invoice-dollar",
        }
    }

    /// Default Bulma colour name (no `is-` prefix) used when no dynamic
    /// status colour overrides it.
    pub fn default_color(&self) -> &'static str {
        match self {
            Self::Appointment   => "info",
            Self::MedicalRecord => "success",
            Self::Prescription  => "link",
            Self::Invoice       => "warning",
        }
    }
}

/// One event on a patient's chronological history timeline. Appointments,
/// medical records, prescriptions, and invoices are all normalised into
/// this shape (the same polymorphic idea as the Reportable trait: each
/// source type contributes its own summary).
#[derive(Debug, Serialize)]
pub struct TimelineEvent {
    pub event_type: String,      // appointment | record | prescription | invoice
    pub icon: String,            // Font Awesome icon class
    pub color: String,           // Bulma colour class for the marker
    pub when: String,            // "YYYY-MM-DD HH:MM", also the sort key
    pub title: String,
    pub summary: String,
    pub link: Option<String>,    // detail page, when one exists
}

impl TimelineEvent {
    /// Build a timeline event from a typed `TimelineEventKind`.
    /// Icon and colour are derived from the kind's own mappings so the
    /// service layer never hardcodes presentation strings at the call site.
    /// A dynamic `status_color` (e.g. from an appointment's live status)
    /// overrides the kind's default colour when provided.
    pub fn new(
        kind: TimelineEventKind,
        when: String,
        title: String,
        summary: String,
        status_color: Option<String>,
        link: Option<String>,
    ) -> Self {
        TimelineEvent {
            icon: kind.icon().to_string(),
            color: status_color.unwrap_or_else(|| kind.default_color().to_string()),
            event_type: kind.as_str().to_string(),
            when,
            title,
            summary,
            link,
        }
    }
}

/// Joined view: a medical record plus its resolved patient/doctor display
/// names, for the detail page. Mirrors `RecordReportData`, the service layer
/// owns every name lookup so the route handler only extracts inputs and
/// renders (it never queries the database itself).
#[derive(Debug, Serialize)]
pub struct RecordDetail {
    pub record: MedicalRecord,
    pub patient_name: String,
    pub doctor_name: String,
}

/// One row of the medical records list: a record plus its patient/doctor
/// display names, resolved via a single joined query rather than per-row
/// lookups. Mirrors `RecordDetail` but shaped for `sqlx::FromRow` since the
/// list query selects both the record's own columns and the joined names
/// in one pass.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RecordListItem {
    pub id: i64,
    pub patient_id: i64,
    pub doctor_id: i64,
    pub appointment_id: Option<i64>,
    pub diagnosis: Option<String>,
    pub treatment: Option<String>,
    pub notes: Option<String>,
    pub created_at: chrono::NaiveDateTime,
    pub patient_name: String,
    pub doctor_name: String,
}

/// Data bundle for the printable medical report (advanced feature,
/// project spec: "medical report generation").
#[derive(Debug, Serialize)]
pub struct RecordReportData {
    pub record: MedicalRecord,
    pub summary: String,         // from Reportable::generate_summary
    pub patient_name: String,
    pub patient_dob: Option<String>,
    pub patient_blood_group: Option<String>,
    pub doctor_name: String,
    pub doctor_specialization: String,
    pub prescriptions: Vec<Prescription>,
    pub generated_at: String,
}
