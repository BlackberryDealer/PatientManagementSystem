use crate::errors::AppError;
use crate::users::models::{Doctor, LoginForm, Patient, RegisterForm, User};
use sqlx::SqlitePool;

// ============================================================
// Registration & Authentication
// ============================================================

/// Register a new user. Automatically creates a Patient or Doctor
/// profile row depending on the selected role. Passwords are hashed
/// with bcrypt (cost factor 10).
pub async fn register_user(
    pool: &SqlitePool,
    form: &RegisterForm,
) -> Result<User, AppError> {
    // Validate role
    if !["patient", "doctor", "admin"].contains(&form.role.as_str()) {
        return Err(AppError::BadRequest("Invalid role specified".into()));
    }

    let password_hash = bcrypt::hash(&form.password, 10)?;

    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (username, email, password_hash, role, full_name)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id, username, email, password_hash, role, full_name, created_at",
    )
    .bind(&form.username)
    .bind(&form.email)
    .bind(&password_hash)
    .bind(&form.role)
    .bind(&form.full_name)
    .fetch_one(pool)
    .await?;

    // Create corresponding profile row
    match form.role.as_str() {
        "patient" => {
            sqlx::query("INSERT INTO patients (user_id) VALUES (?)")
                .bind(user.id)
                .execute(pool)
                .await?;
        }
        "doctor" => {
            sqlx::query(
                "INSERT INTO doctors (user_id, specialization, license_number) VALUES (?, ?, ?)",
            )
            .bind(user.id)
            .bind("General Practice") // default, can be updated later
            .bind("PENDING")
            .execute(pool)
            .await?;
        }
        _ => { /* admin has no extra profile table */ }
    }

    Ok(user)
}

/// Authenticate a user by username and password.
/// Returns the full User row on success, or `Unauthorized` on failure.
pub async fn authenticate_user(
    pool: &SqlitePool,
    form: &LoginForm,
) -> Result<User, AppError> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(&form.username)
        .fetch_optional(pool)
        .await?;

    match user {
        Some(u) if bcrypt::verify(&form.password, &u.password_hash)? => Ok(u),
        _ => Err(AppError::Unauthorized(
            "Invalid username or password".into(),
        )),
    }
}

// ============================================================
// Queries
// ============================================================

pub async fn get_user_by_id(pool: &SqlitePool, user_id: i64) -> Result<User, AppError> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))
}

pub async fn get_all_users(pool: &SqlitePool) -> Result<Vec<User>, AppError> {
    Ok(
        sqlx::query_as::<_, User>("SELECT * FROM users ORDER BY id")
            .fetch_all(pool)
            .await?,
    )
}

pub async fn get_patient_by_user_id(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<Option<Patient>, AppError> {
    Ok(
        sqlx::query_as::<_, Patient>("SELECT * FROM patients WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn get_doctor_by_user_id(
    pool: &SqlitePool,
    user_id: i64,
) -> Result<Option<Doctor>, AppError> {
    Ok(
        sqlx::query_as::<_, Doctor>("SELECT * FROM doctors WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await?,
    )
}
