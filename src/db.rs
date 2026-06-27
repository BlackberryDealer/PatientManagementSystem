use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use log::info;

use crate::errors::AppError;

/// Create a connection pool for the SQLite database.
pub async fn create_pool(database_url: &str) -> SqlitePool {
    info!("Connecting to database: {}", database_url);
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Failed to create database pool. Is the DATABASE_URL correct?")
}

/// Run all pending SQLx migrations from the `migrations/` directory.
pub async fn run_migrations(pool: &SqlitePool) {
    info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("Failed to run database migrations. Check your migration SQL files.");
    info!("Database migrations completed successfully.");
}

// ============================================================
// Shared ID-lookup helpers (avoid duplication across modules)
// ============================================================

/// Resolve the internal patient row ID from a user ID.
pub async fn get_patient_id(pool: &SqlitePool, user_id: i64) -> Result<i64, AppError> {
    sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .map(|(id,)| id)
        .ok_or_else(|| AppError::NotFound("Patient profile not found".into()))
}

/// Resolve the internal doctor row ID from a user ID.
pub async fn get_doctor_id(pool: &SqlitePool, user_id: i64) -> Result<i64, AppError> {
    sqlx::query_as::<_, (i64,)>("SELECT id FROM doctors WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .map(|(id,)| id)
        .ok_or_else(|| AppError::NotFound("Doctor profile not found".into()))
}

/// Reject a `patient_id` that doesn't exist, as a clean 400 instead of letting
/// it surface later as a foreign-key failure. Shared by the record- and
/// prescription-creation paths, which both reference a patient by row ID.
pub async fn ensure_patient_exists(pool: &SqlitePool, patient_id: i64) -> Result<(), AppError> {
    let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM patients WHERE id = ?")
        .bind(patient_id)
        .fetch_one(pool)
        .await?;
    if exists.0 == 0 {
        return Err(AppError::BadRequest("Selected patient does not exist".into()));
    }
    Ok(())
}
