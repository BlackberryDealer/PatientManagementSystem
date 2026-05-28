use crate::errors::AppError;
use crate::records::models::{CreateRecordForm, MedicalRecord, Prescription};
use sqlx::SqlitePool;

/// Get the doctor's internal ID from user ID.
async fn get_doctor_id(pool: &SqlitePool, user_id: i64) -> Result<i64, AppError> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM doctors WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Doctor profile not found".into()))?;
    Ok(row.0)
}

/// Get the patient's internal ID from user ID.
async fn get_patient_id(pool: &SqlitePool, user_id: i64) -> Result<i64, AppError> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Patient profile not found".into()))?;
    Ok(row.0)
}

/// Create a new medical record (doctor only).
pub async fn create_record(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &CreateRecordForm,
) -> Result<MedicalRecord, AppError> {
    let doctor_id = get_doctor_id(pool, doctor_user_id).await?;

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
    let patient_id = get_patient_id(pool, patient_user_id).await?;

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

/// Get prescriptions for a patient.
pub async fn get_prescriptions_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<Prescription>, AppError> {
    let patient_id = get_patient_id(pool, patient_user_id).await?;

    Ok(sqlx::query_as::<_, Prescription>(
        "SELECT * FROM prescriptions WHERE patient_id = ? ORDER BY prescribed_at DESC",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?)
}
