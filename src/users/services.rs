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
    // Validation first — nothing is hashed or persisted until this passes
    form.validate()?;

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
    .await
    .map_err(|e| {
        // A duplicate username/email is a user mistake (400), not a server fault
        AppError::bad_request_on_unique(e, "That username or email is already taken.")
    })?;

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

/// Authenticate a user by username OR email (case-insensitive).
/// Returns the full User row on success, or `Unauthorized` on failure.
pub async fn authenticate_user(
    pool: &SqlitePool,
    form: &LoginForm,
) -> Result<User, AppError> {
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE LOWER(username) = LOWER(?) OR LOWER(email) = LOWER(?)",
    )
    .bind(&form.login)
    .bind(&form.login)
    .fetch_optional(pool)
    .await?;

    match user {
        Some(u) if bcrypt::verify(&form.password, &u.password_hash)? => Ok(u),
        _ => Err(AppError::Unauthorized(
            "Invalid username/email or password".into(),
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

// ============================================================
// Profile editing
// ============================================================

use crate::users::models::EditProfileForm;

/// Update a user's profile: core details plus the role-specific
/// (patient/doctor) extension row. Which extension a role implies is a
/// domain decision and belongs here, not in the route handler.
pub async fn update_profile(
    pool: &SqlitePool,
    user_id: i64,
    form: &EditProfileForm,
) -> Result<(), AppError> {
    let user = update_user(pool, user_id, form).await?;
    match user.role.as_str() {
        "patient" => {
            update_patient(pool, user_id, form).await?;
        }
        "doctor" => {
            update_doctor(pool, user_id, form).await?;
        }
        _ => {} // admin has no extension row
    }
    Ok(())
}

/// Update a user's core details (full_name, email).
pub async fn update_user(
    pool: &SqlitePool,
    user_id: i64,
    form: &EditProfileForm,
) -> Result<User, AppError> {
    sqlx::query_as::<_, User>(
        "UPDATE users SET full_name = ?, email = ? WHERE id = ?
         RETURNING id, username, email, password_hash, role, full_name, created_at",
    )
    .bind(&form.full_name)
    .bind(&form.email)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".into()))
}

/// Update or insert patient-specific fields.
pub async fn update_patient(
    pool: &SqlitePool,
    user_id: i64,
    form: &EditProfileForm,
) -> Result<Patient, AppError> {
    let patient = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    let date_of_birth = form.date_of_birth.as_deref().filter(|s| !s.is_empty()).map(|s| s.to_string());
    let phone = form.phone.as_deref().filter(|s| !s.is_empty()).map(|s| s.to_string());
    let address = form.address.as_deref().filter(|s| !s.is_empty()).map(|s| s.to_string());
    let blood_group = form.blood_group.as_deref().filter(|s| !s.is_empty()).map(|s| s.to_string());
    let emergency_contact = form.emergency_contact.as_deref().filter(|s| !s.is_empty()).map(|s| s.to_string());

    if let Some((pid,)) = patient {
        let _ = pid;
        sqlx::query_as::<_, Patient>(
            "UPDATE patients SET date_of_birth = ?, phone = ?, address = ?, blood_group = ?, emergency_contact = ?
             WHERE user_id = ?
             RETURNING id, user_id, date_of_birth, phone, address, blood_group, emergency_contact",
        )
        .bind(&date_of_birth).bind(&phone).bind(&address).bind(&blood_group).bind(&emergency_contact)
        .bind(user_id)
        .fetch_one(pool).await
        .map_err(|e| e.into())
    } else {
        sqlx::query_as::<_, Patient>(
            "INSERT INTO patients (user_id, date_of_birth, phone, address, blood_group, emergency_contact)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING id, user_id, date_of_birth, phone, address, blood_group, emergency_contact",
        )
        .bind(user_id).bind(&date_of_birth).bind(&phone).bind(&address).bind(&blood_group).bind(&emergency_contact)
        .fetch_one(pool).await
        .map_err(|e| e.into())
    }
}

/// Update or insert doctor-specific fields.
pub async fn update_doctor(
    pool: &SqlitePool,
    user_id: i64,
    form: &EditProfileForm,
) -> Result<Doctor, AppError> {
    let doctor = sqlx::query_as::<_, (i64,)>("SELECT id FROM doctors WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    let specialization = form.specialization.as_deref().filter(|s| !s.is_empty()).unwrap_or("General Practice");
    let license_number = form.license_number.as_deref().filter(|s| !s.is_empty()).unwrap_or("PENDING");
    let phone = form.phone.as_deref().filter(|s| !s.is_empty()).map(|s| s.to_string());

    if let Some((did,)) = doctor {
        let _ = did;
        sqlx::query_as::<_, Doctor>(
            "UPDATE doctors SET specialization = ?, license_number = ?, phone = ? WHERE user_id = ?
             RETURNING id, user_id, specialization, license_number, phone",
        )
        .bind(specialization).bind(license_number).bind(&phone)
        .bind(user_id)
        .fetch_one(pool).await
        .map_err(|e| e.into())
    } else {
        sqlx::query_as::<_, Doctor>(
            "INSERT INTO doctors (user_id, specialization, license_number, phone)
             VALUES (?, ?, ?, ?)
             RETURNING id, user_id, specialization, license_number, phone",
        )
        .bind(user_id).bind(specialization).bind(license_number).bind(&phone)
        .fetch_one(pool).await
        .map_err(|e| e.into())
    }
}
