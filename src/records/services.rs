use crate::auth::Role;
use crate::db;
use crate::errors::AppError;
use crate::records::models::{
    CreateRecordForm, MedicalRecord, PatientAppointmentOption, Prescription, PrescriptionForm,
    RecordDetail, RecordListItem, RecordReportData, TimelineEvent, TimelineEventKind,
};
use crate::traits::{Reportable, StatusManaged};
use sqlx::SqlitePool;

/// Create a new medical record (doctor only).
pub async fn create_record(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &CreateRecordForm,
) -> Result<MedicalRecord, AppError> {
    // Validation first, nothing reaches the database until the form's
    // own rules pass (Route -> Validation -> Business Logic -> DB).
    form.validate()?;

    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    // The target patient must exist (clean 400 instead of an FK error).
    db::ensure_patient_exists(pool, form.patient_id).await?;

    Ok(sqlx::query_as::<_, MedicalRecord>(
        "INSERT INTO medical_records (patient_id, doctor_id, appointment_id, diagnosis, treatment, notes)
         VALUES (?, ?, ?, ?, ?, ?)
         RETURNING id, patient_id, doctor_id, appointment_id, diagnosis, treatment, notes, created_at",
    )
    .bind(form.patient_id)
    .bind(doctor_id)
    .bind(form.appointment_id)
    .bind(&form.diagnosis)
    .bind(&form.treatment)
    .bind(&form.notes)
    .fetch_one(pool)
    .await?)
}

/// Shared SELECT for the records list: a medical record joined to its
/// patient's and doctor's display names in one query, so the list page never
/// falls back to showing raw ids.
const RECORD_LIST_SELECT: &str = "
    SELECT mr.id, mr.patient_id, mr.doctor_id, mr.appointment_id, mr.diagnosis, mr.treatment,
           mr.notes, mr.created_at, pu.full_name AS patient_name, du.full_name AS doctor_name
    FROM medical_records mr
    JOIN patients p ON mr.patient_id = p.id
    JOIN users pu ON p.user_id = pu.id
    JOIN doctors d ON mr.doctor_id = d.id
    JOIN users du ON d.user_id = du.id";

/// List medical records for a patient.
pub async fn get_records_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<RecordListItem>, AppError> {
    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

    Ok(sqlx::query_as::<_, RecordListItem>(&format!(
        "{RECORD_LIST_SELECT} WHERE mr.patient_id = ? ORDER BY mr.created_at DESC"
    ))
    .bind(patient_id)
    .fetch_all(pool)
    .await?)
}

/// List all medical records (admin view).
pub async fn get_all_records(pool: &SqlitePool) -> Result<Vec<RecordListItem>, AppError> {
    Ok(sqlx::query_as::<_, RecordListItem>(&format!(
        "{RECORD_LIST_SELECT} ORDER BY mr.created_at DESC"
    ))
    .fetch_all(pool)
    .await?)
}

/// Get a single medical record by ID.
pub async fn get_record_by_id(
    pool: &SqlitePool,
    record_id: i64,
) -> Result<MedicalRecord, AppError> {
    sqlx::query_as::<_, MedicalRecord>("SELECT * FROM medical_records WHERE id = ?")
        .bind(record_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Medical record not found".into()))
}

/// Get a medical record, enforcing ownership for patients.
/// Patients may only access their own records; staff roles see any.
pub async fn get_record_checked(
    pool: &SqlitePool,
    record_id: i64,
    user_id: i64,
    role: Role,
) -> Result<MedicalRecord, AppError> {
    let record = get_record_by_id(pool, record_id).await?;
    if role == Role::Patient {
        let patient_id = db::get_patient_id(pool, user_id).await?;
        if record.patient_id != patient_id {
            return Err(AppError::Forbidden(
                "You do not have permission to view this record.".into(),
            ));
        }
    }
    Ok(record)
}

/// Assemble the record-detail view (record + resolved patient/doctor names),
/// enforcing the same patient-ownership rule as `get_record_checked`. All the
/// persistence lives here so the route handler stays a pure HTTP-lifecycle
/// function (Route -> Logic/DB -> render).
pub async fn get_record_detail_checked(
    pool: &SqlitePool,
    record_id: i64,
    user_id: i64,
    role: Role,
) -> Result<RecordDetail, AppError> {
    let record = get_record_checked(pool, record_id, user_id, role).await?;
    // Both lookups fall back to "#id" rather than erroring here, applied the
    // same way for patient and doctor, so a record still renders even if one
    // side's row is ever missing. `get_patient_name`/`get_doctor_name` stay
    // strict (they error on a missing row) because their other caller,
    // the timeline handler, needs a real 404 on a bad `patient_id`.
    let patient_name = get_patient_name(pool, record.patient_id)
        .await
        .unwrap_or_else(|_| format!("Patient #{}", record.patient_id));
    let doctor_name = get_doctor_name(pool, record.doctor_id)
        .await
        .unwrap_or_else(|_| format!("Doctor #{}", record.doctor_id));
    Ok(RecordDetail { record, patient_name, doctor_name })
}

/// Get prescriptions for a patient.
pub async fn get_prescriptions_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<Prescription>, AppError> {
    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

    Ok(sqlx::query_as::<_, Prescription>(
        "SELECT * FROM prescriptions WHERE patient_id = ? ORDER BY prescribed_at DESC",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?)
}

/// All prescriptions (doctor/admin view).
pub async fn get_all_prescriptions(pool: &SqlitePool) -> Result<Vec<Prescription>, AppError> {
    Ok(sqlx::query_as::<_, Prescription>(
        "SELECT * FROM prescriptions ORDER BY prescribed_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// Write a new prescription (doctor only).
/// Flow: validate the form, verify the patient exists, then persist.
pub async fn create_prescription(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &PrescriptionForm,
) -> Result<Prescription, AppError> {
    form.validate()?;

    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    // The target patient must exist (clean 400 instead of an FK error).
    db::ensure_patient_exists(pool, form.patient_id).await?;

    Ok(sqlx::query_as::<_, Prescription>(
        "INSERT INTO prescriptions
         (patient_id, doctor_id, appointment_id, medication_name, dosage, frequency, duration, notes)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING id, patient_id, doctor_id, appointment_id, medication_name,
                   dosage, frequency, duration, notes, prescribed_at",
    )
    .bind(form.patient_id)
    .bind(doctor_id)
    .bind(form.appointment_id)
    .bind(form.medication_name.trim())
    .bind(form.dosage.trim())
    .bind(form.frequency.trim())
    .bind(&form.duration)
    .bind(&form.notes)
    .fetch_one(pool)
    .await?)
}

/// All patients as (patient_id, full_name) pairs, for staff dropdowns.
/// Delegates to the shared helper in `db` (also used by billing and the
/// staff booking form).
pub async fn get_all_patients(pool: &SqlitePool) -> Result<Vec<(i64, String)>, AppError> {
    crate::db::get_all_patients(pool).await
}

/// A patient's appointments as pick-list options for the create-record form,
/// newest first. Each option carries a ready-made label
/// ("#12, 2026-07-01 09:00, Dr. Smith (completed)") so the doctor links a real
/// visit from a dropdown instead of typing an appointment id by hand.
pub async fn get_patient_appointment_options(
    pool: &SqlitePool,
    patient_id: i64,
) -> Result<Vec<PatientAppointmentOption>, AppError> {
    let rows = crate::appointments::services::get_appointment_options_for_patient(pool, patient_id)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(id, date, start, status, doctor)| PatientAppointmentOption {
            id,
            label: format!("#{id}, {date} {start}, {doctor} ({status})"),
        })
        .collect())
}

// ============================================================
// Patient History Timeline (advanced feature, project spec)
// ============================================================

/// Display name for a patient by their patients-table row ID.
pub async fn get_patient_name(pool: &SqlitePool, patient_id: i64) -> Result<String, AppError> {
    sqlx::query_as::<_, (String,)>(
        "SELECT u.full_name FROM patients p JOIN users u ON p.user_id = u.id WHERE p.id = ?",
    )
    .bind(patient_id)
    .fetch_optional(pool)
    .await?
    .map(|(name,)| name)
    .ok_or_else(|| AppError::NotFound("Patient not found".into()))
}

/// Display name for a doctor by their doctors-table row ID (sibling of
/// `get_patient_name`, same strict "error if missing" contract). Callers that
/// want resilience against a missing row (e.g. `get_record_detail_checked`)
/// apply their own fallback at the call site instead of baking one in here.
pub async fn get_doctor_name(pool: &SqlitePool, doctor_id: i64) -> Result<String, AppError> {
    sqlx::query_as::<_, (String,)>(
        "SELECT u.full_name FROM doctors d JOIN users u ON d.user_id = u.id WHERE d.id = ?",
    )
    .bind(doctor_id)
    .fetch_optional(pool)
    .await?
    .map(|(name,)| name)
    .ok_or_else(|| AppError::NotFound("Doctor not found".into()))
}

/// Build a patient's full chronological history: appointments, medical
/// records, prescriptions, and invoices merged into one timeline, newest
/// first. Each entity contributes its own summary via the Reportable
/// trait, the polymorphism the timeline is built on.
///
/// Appointments and invoices are fetched through
/// `appointments::services::get_appointments_for_patient_id` and
/// `billing::services::get_invoices_for_patient_id`, narrow read functions
/// that live in their owning modules rather than duplicating those tables'
/// SQL here.
pub async fn build_patient_timeline(
    pool: &SqlitePool,
    patient_id: i64,
) -> Result<Vec<TimelineEvent>, AppError> {
    let mut events: Vec<TimelineEvent> = Vec::new();

    // Doctor display names, for appointment titles
    let doctor_names: std::collections::HashMap<i64, String> =
        sqlx::query_as::<_, (i64, String)>(
            "SELECT d.id, u.full_name FROM doctors d JOIN users u ON d.user_id = u.id",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();

    push_appointment_events(pool, patient_id, &doctor_names, &mut events).await?;
    push_record_events(pool, patient_id, &mut events).await?;
    push_prescription_events(pool, patient_id, &mut events).await?;
    push_invoice_events(pool, patient_id, &mut events).await?;

    // Newest first. ISO "YYYY-MM-DD HH:MM" strings sort chronologically.
    events.sort_by(|a, b| b.when.cmp(&a.when));
    Ok(events)
}

/// Timeline entry 1/4: this patient's appointments, titled with the doctor's
/// display name resolved from `doctor_names`.
async fn push_appointment_events(
    pool: &SqlitePool,
    patient_id: i64,
    doctor_names: &std::collections::HashMap<i64, String>,
    events: &mut Vec<TimelineEvent>,
) -> Result<(), AppError> {
    let appointments =
        crate::appointments::services::get_appointments_for_patient_id(pool, patient_id).await?;

    for appt in appointments {
        let doctor = doctor_names
            .get(&appt.doctor_id())
            .cloned()
            .unwrap_or_else(|| format!("Doctor #{}", appt.doctor_id()));
        events.push(TimelineEvent::new(
            TimelineEventKind::Appointment,
            format!("{} {}", appt.appointment_date, appt.start_time),
            format!("Appointment with {}", doctor),
            appt.generate_summary(), // Reportable
            Some(appt.status_color().to_string()), // dynamic: reflects live status
            Some(format!("/appointments/{}", appt.id)),
        ));
    }
    Ok(())
}

/// Timeline entry 2/4: this patient's medical records.
async fn push_record_events(
    pool: &SqlitePool,
    patient_id: i64,
    events: &mut Vec<TimelineEvent>,
) -> Result<(), AppError> {
    let records = sqlx::query_as::<_, MedicalRecord>(
        "SELECT * FROM medical_records WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    for record in records {
        events.push(TimelineEvent::new(
            TimelineEventKind::MedicalRecord,
            record.created_at.format("%Y-%m-%d %H:%M").to_string(),
            "Medical record created".into(),
            record.generate_summary(), // Reportable
            None,
            Some(format!("/records/{}", record.id)),
        ));
    }
    Ok(())
}

/// Timeline entry 3/4: this patient's prescriptions.
async fn push_prescription_events(
    pool: &SqlitePool,
    patient_id: i64,
    events: &mut Vec<TimelineEvent>,
) -> Result<(), AppError> {
    let prescriptions = sqlx::query_as::<_, Prescription>(
        "SELECT * FROM prescriptions WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    for rx in prescriptions {
        events.push(TimelineEvent::new(
            TimelineEventKind::Prescription,
            rx.prescribed_at.format("%Y-%m-%d %H:%M").to_string(),
            format!("Prescribed {}", rx.medication_name),
            rx.generate_summary(), // Reportable
            None,
            None,
        ));
    }
    Ok(())
}

/// Timeline entry 4/4: this patient's invoices.
async fn push_invoice_events(
    pool: &SqlitePool,
    patient_id: i64,
    events: &mut Vec<TimelineEvent>,
) -> Result<(), AppError> {
    let invoices = crate::billing::services::get_invoices_for_patient_id(pool, patient_id).await?;

    for invoice in invoices {
        events.push(TimelineEvent::new(
            TimelineEventKind::Invoice,
            invoice.created_at.format("%Y-%m-%d %H:%M").to_string(),
            format!("Invoice #{} issued", invoice.id),
            invoice.generate_summary(), // Reportable
            Some(invoice.status_color().to_string()), // dynamic: reflects live status
            Some(format!("/billing/{}", invoice.id)),
        ));
    }
    Ok(())
}

// ============================================================
// Medical Report Generation (advanced feature, project spec)
// ============================================================

/// Assemble everything the printable medical report needs: the record,
/// patient demographics, doctor details, and any prescriptions written
/// against the same appointment.
pub async fn build_record_report(
    pool: &SqlitePool,
    record: MedicalRecord,
) -> Result<RecordReportData, AppError> {
    let (patient_name, patient_dob, patient_blood_group) =
        sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT u.full_name, p.date_of_birth, p.blood_group
             FROM patients p JOIN users u ON p.user_id = u.id WHERE p.id = ?",
        )
        .bind(record.patient_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Patient not found".into()))?;

    let (doctor_name, doctor_specialization) = sqlx::query_as::<_, (String, String)>(
        "SELECT u.full_name, d.specialization
         FROM doctors d JOIN users u ON d.user_id = u.id WHERE d.id = ?",
    )
    .bind(record.doctor_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Doctor not found".into()))?;

    // Prescriptions tied to the same appointment as this record
    let prescriptions = match record.appointment_id {
        Some(appointment_id) => sqlx::query_as::<_, Prescription>(
            "SELECT * FROM prescriptions WHERE appointment_id = ? AND patient_id = ?
             ORDER BY prescribed_at",
        )
        .bind(appointment_id)
        .bind(record.patient_id)
        .fetch_all(pool)
        .await?,
        None => Vec::new(),
    };

    Ok(RecordReportData {
        summary: record.generate_summary(), // Reportable
        patient_name,
        patient_dob,
        patient_blood_group,
        doctor_name,
        doctor_specialization,
        prescriptions,
        generated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        record,
    })
}
