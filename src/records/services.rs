use crate::db;
use crate::errors::AppError;
use crate::records::models::{CreateRecordForm, MedicalRecord, Prescription};
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
