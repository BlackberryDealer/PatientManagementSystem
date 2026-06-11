use crate::db;
use crate::errors::AppError;
use crate::records::models::{
    CreateRecordForm, MedicalRecord, Prescription, PrescriptionForm, RecordReportData,
    TimelineEvent,
};
use crate::traits::{Reportable, StatusManaged};
use sqlx::SqlitePool;

/// Create a new medical record (doctor only).
pub async fn create_record(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &CreateRecordForm,
) -> Result<MedicalRecord, AppError> {
    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

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

/// List medical records for a patient.
pub async fn get_records_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<MedicalRecord>, AppError> {
    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

    Ok(sqlx::query_as::<_, MedicalRecord>(
        "SELECT * FROM medical_records WHERE patient_id = ? ORDER BY created_at DESC",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?)
}

/// List all medical records (admin view).
pub async fn get_all_records(pool: &SqlitePool) -> Result<Vec<MedicalRecord>, AppError> {
    Ok(
        sqlx::query_as::<_, MedicalRecord>(
            "SELECT * FROM medical_records ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?,
    )
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
    role: &str,
) -> Result<MedicalRecord, AppError> {
    let record = get_record_by_id(pool, record_id).await?;
    if role == "patient" {
        let patient_id = db::get_patient_id(pool, user_id).await?;
        if record.patient_id != patient_id {
            return Err(AppError::Forbidden(
                "You do not have permission to view this record.".into(),
            ));
        }
    }
    Ok(record)
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

    // The target patient must exist (clean 400 instead of an FK error)
    let patient_exists: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM patients WHERE id = ?")
            .bind(form.patient_id)
            .fetch_one(pool)
            .await?;
    if patient_exists.0 == 0 {
        return Err(AppError::BadRequest("Selected patient does not exist".into()));
    }

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
pub async fn get_all_patients(pool: &SqlitePool) -> Result<Vec<(i64, String)>, AppError> {
    Ok(sqlx::query_as::<_, (i64, String)>(
        "SELECT p.id, u.full_name FROM patients p JOIN users u ON p.user_id = u.id
         ORDER BY u.full_name",
    )
    .fetch_all(pool)
    .await?)
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

/// Build a patient's full chronological history: appointments, medical
/// records, prescriptions, and invoices merged into one timeline, newest
/// first. Each entity contributes its own summary via the Reportable
/// trait — the polymorphism the timeline is built on.
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

    // 1. Appointments
    let appointments = sqlx::query_as::<_, crate::appointments::models::Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    for appt in appointments {
        let doctor = doctor_names
            .get(&appt.doctor_id)
            .cloned()
            .unwrap_or_else(|| format!("Doctor #{}", appt.doctor_id));
        events.push(TimelineEvent {
            event_type: "appointment".into(),
            icon: "fa-calendar-check".into(),
            color: appt.status_badge_class().trim_start_matches("is-").to_string(),
            when: format!("{} {}", appt.appointment_date, appt.start_time),
            title: format!("Appointment with {}", doctor),
            summary: appt.generate_summary(), // Reportable
            link: Some(format!("/appointments/{}", appt.id)),
        });
    }

    // 2. Medical records
    let records = sqlx::query_as::<_, MedicalRecord>(
        "SELECT * FROM medical_records WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    for record in records {
        events.push(TimelineEvent {
            event_type: "record".into(),
            icon: "fa-notes-medical".into(),
            color: "success".into(),
            when: record.created_at.format("%Y-%m-%d %H:%M").to_string(),
            title: "Medical record created".into(),
            summary: record.generate_summary(), // Reportable
            link: Some(format!("/records/{}", record.id)),
        });
    }

    // 3. Prescriptions
    let prescriptions = sqlx::query_as::<_, Prescription>(
        "SELECT * FROM prescriptions WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    for rx in prescriptions {
        events.push(TimelineEvent {
            event_type: "prescription".into(),
            icon: "fa-prescription".into(),
            color: "link".into(),
            when: rx.prescribed_at.format("%Y-%m-%d %H:%M").to_string(),
            title: format!("Prescribed {}", rx.medication_name),
            summary: rx.generate_summary(), // Reportable
            link: None,
        });
    }

    // 4. Invoices
    let invoices = sqlx::query_as::<_, crate::billing::models::Invoice>(
        "SELECT * FROM invoices WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    for invoice in invoices {
        events.push(TimelineEvent {
            event_type: "invoice".into(),
            icon: "fa-file-invoice-dollar".into(),
            color: invoice.status_badge_class().trim_start_matches("is-").to_string(),
            when: invoice.created_at.format("%Y-%m-%d %H:%M").to_string(),
            title: format!("Invoice #{} issued", invoice.id),
            summary: invoice.generate_summary(), // Reportable
            link: Some(format!("/billing/{}", invoice.id)),
        });
    }

    // Newest first. ISO "YYYY-MM-DD HH:MM" strings sort chronologically.
    events.sort_by(|a, b| b.when.cmp(&a.when));
    Ok(events)
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
