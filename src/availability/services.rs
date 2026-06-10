use crate::availability::models::{DoctorAvailability, SetAvailabilityForm};
use crate::db;
use crate::errors::AppError;
use sqlx::SqlitePool;

/// List all availability slots for a doctor.
pub async fn get_availability_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<DoctorAvailability>, AppError> {
    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "SELECT id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability WHERE doctor_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(doctor_id)
    .fetch_all(pool)
    .await?)
}

/// List all availability slots (admin view).
pub async fn get_all_availability(
    pool: &SqlitePool,
) -> Result<Vec<DoctorAvailability>, AppError> {
    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "SELECT id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability ORDER BY doctor_id, day_of_week, start_time",
    )
    .fetch_all(pool)
    .await?)
}

/// Add a new availability slot for a doctor.
/// Flow: validate the form, resolve the doctor, then persist.
pub async fn add_availability(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &SetAvailabilityForm,
) -> Result<DoctorAvailability, AppError> {
    // Validation — nothing touches the database before this passes
    form.validate()?;

    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "INSERT INTO doctor_availability
         (doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         RETURNING id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked",
    )
    .bind(doctor_id)
    .bind(form.day_of_week)
    .bind(&form.start_time)
    .bind(&form.end_time)
    .bind(form.recurring() as i32) // SQLite stores BOOLEAN as INTEGER 0/1
    .bind(form.specific_date_or_none())
    .bind(form.blocked() as i32)
    .fetch_one(pool)
    .await?)
}
