use crate::availability::models::{DoctorAvailability, SetAvailabilityForm};
use crate::errors::AppError;
use sqlx::SqlitePool;

/// Get the doctor's internal ID from their user ID.
async fn get_doctor_id(pool: &SqlitePool, user_id: i64) -> Result<i64, AppError> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM doctors WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Doctor profile not found".into()))?;
    Ok(row.0)
}

/// List all availability slots for a doctor.
pub async fn get_availability_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<DoctorAvailability>, AppError> {
    let doctor_id = get_doctor_id(pool, doctor_user_id).await?;

    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "SELECT * FROM doctor_availability WHERE doctor_id = ? ORDER BY day_of_week, start_time",
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
        "SELECT * FROM doctor_availability ORDER BY doctor_id, day_of_week, start_time",
    )
    .fetch_all(pool)
    .await?)
}

/// Add a new availability slot for a doctor.
pub async fn add_availability(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &SetAvailabilityForm,
) -> Result<DoctorAvailability, AppError> {
    let doctor_id = get_doctor_id(pool, doctor_user_id).await?;

    let is_recurring = form.is_recurring.as_deref() == Some("on");
    let is_blocked = form.is_blocked.as_deref() == Some("on");

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
    .bind(is_recurring)
    .bind(&form.specific_date)
    .bind(is_blocked)
    .fetch_one(pool)
    .await?)
}
