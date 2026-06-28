use crate::auth::Role;
use crate::errors::AppError;
use crate::users::models::{CreateStaffForm, Doctor, LoginForm, Patient, RegisterForm, User};
use sqlx::SqlitePool;

/// bcrypt work factor for all password hashing and verification. Single source
/// of truth so registration, staff creation, and the login dummy-verify all
/// spend the same amount of CPU (see `authenticate_user`).
const BCRYPT_COST: u32 = 10;

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

    let password_hash = bcrypt::hash(&form.password, BCRYPT_COST)?;

    // The user row and its role profile row must be created together — a user
    // with no matching patient/doctor row is an orphan that breaks later
    // id-lookups. Both inserts commit atomically (like invoice header + items).
    let mut tx = pool.begin().await?;

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
    .fetch_one(&mut *tx)
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
                .execute(&mut *tx)
                .await?;
        }
        "doctor" => {
            sqlx::query(
                "INSERT INTO doctors (user_id, specialization, license_number) VALUES (?, ?, ?)",
            )
            .bind(user.id)
            .bind(Doctor::DEFAULT_SPECIALIZATION) // can be updated later
            .bind(Doctor::DEFAULT_LICENSE)
            .execute(&mut *tx)
            .await?;
        }
        _ => { /* admin has no extra profile table */ }
    }

    tx.commit().await?;
    Ok(user)
}

/// Create a staff (doctor/admin) account. This is the privileged path that
/// bypasses the patient-only rule of public registration, so the caller MUST
/// already be an authenticated admin (enforced in the handler). Doctors get a
/// matching `doctors` profile row.
pub async fn create_staff_user(
    pool: &SqlitePool,
    form: &CreateStaffForm,
) -> Result<User, AppError> {
    form.validate()?;

    let password_hash = bcrypt::hash(&form.password, BCRYPT_COST)?;

    // User row + doctor profile commit atomically, so a failed profile insert
    // can never leave an orphaned staff account behind.
    let mut tx = pool.begin().await?;

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
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::bad_request_on_unique(e, "That username or email is already taken."))?;

    if form.role == "doctor" {
        let specialization = form
            .specialization
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(Doctor::DEFAULT_SPECIALIZATION);
        let license = form
            .license_number
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(Doctor::DEFAULT_LICENSE);
        sqlx::query("INSERT INTO doctors (user_id, specialization, license_number) VALUES (?, ?, ?)")
            .bind(user.id)
            .bind(specialization)
            .bind(license)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(user)
}

/// Authenticate a user by username OR email (case-insensitive).
/// Returns the full User row on success, or `Unauthorized` on failure.
pub async fn authenticate_user(
    pool: &SqlitePool,
    form: &LoginForm,
) -> Result<User, AppError> {
    // Validation first — empty credentials never reach the database
    form.validate()?;

    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE LOWER(username) = LOWER(?) OR LOWER(email) = LOWER(?)",
    )
    .bind(&form.login)
    .bind(&form.login)
    .fetch_optional(pool)
    .await?;

    match user {
        Some(u) if u.verify_password(&form.password)? => Ok(u),
        Some(_) => Err(AppError::Unauthorized(
            "Invalid username/email or password".into(),
        )),
        None => {
            // Burn a comparable bcrypt workload even when no account matched, so
            // the response time can't reveal whether a username/email exists
            // (defeats account enumeration via timing). Same cost as a real
            // verify of a stored hash.
            let _ = bcrypt::hash(&form.password, BCRYPT_COST);
            Err(AppError::Unauthorized(
                "Invalid username/email or password".into(),
            ))
        }
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

/// Resolve a user profile for display, enforcing patient anti-enumeration.
///
/// Patients may only view their own profile. Rather than returning a 403
/// (which would reveal that the target ID exists), `None` is returned so
/// the handler can silently redirect the patient to their own profile page.
/// Doctors and admins receive the full profile without restriction.
pub async fn get_user_profile_checked(
    pool: &SqlitePool,
    viewer_id: i64,
    viewer_role: Role,
    profile_id: i64,
) -> Result<Option<User>, AppError> {
    if viewer_role == Role::Patient && viewer_id != profile_id {
        return Ok(None); // caller redirects to viewer's own profile
    }
    Ok(Some(get_user_by_id(pool, profile_id).await?))
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
    // Validation gate: no part of the payload reaches the database until
    // the form's own rules pass (Route -> Validation -> DB).
    form.validate()?;
    let user = update_user(pool, user_id, form).await?;
    // Typed role match (exhaustive, no stringly-typed arms): which extension
    // row a role implies is a domain decision and belongs here.
    match user.role() {
        Role::Patient => {
            update_patient(pool, user_id, form).await?;
        }
        Role::Doctor => {
            update_doctor(pool, user_id, form).await?;
        }
        Role::Admin => {} // admin has no extension row
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

/// Trim-to-None: an absent, empty, or whitespace-only optional field becomes
/// `None`, so the database stores a proper NULL instead of "". One helper for
/// every nullable profile column, replacing the per-field `as_deref().filter()`
/// boilerplate.
fn non_empty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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

    let date_of_birth = non_empty(&form.date_of_birth);
    let phone = non_empty(&form.phone);
    let address = non_empty(&form.address);
    let blood_group = non_empty(&form.blood_group);
    let emergency_contact = non_empty(&form.emergency_contact);

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

    let specialization = form.specialization.as_deref().filter(|s| !s.is_empty()).unwrap_or(Doctor::DEFAULT_SPECIALIZATION);
    let license_number = form.license_number.as_deref().filter(|s| !s.is_empty()).unwrap_or(Doctor::DEFAULT_LICENSE);
    let phone = non_empty(&form.phone);

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
