use serde::{Deserialize, Serialize};

// ============================================================
// User — core authentication entity
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: String,
    pub full_name: String,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub role: String, // "patient", "doctor", or "admin"
}

impl RegisterForm {
    /// All registration input rules in one place, checked before any
    /// hashing or database work happens.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.username.trim().len() < 3 {
            return Err(AppError::BadRequest(
                "Username must be at least 3 characters".into(),
            ));
        }
        if !self.email.contains('@') {
            return Err(AppError::BadRequest(
                "A valid email address is required".into(),
            ));
        }
        if self.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }
        if self.full_name.trim().is_empty() {
            return Err(AppError::BadRequest("Full name is required".into()));
        }
        // Public self-registration is restricted to patients. Staff accounts
        // (doctor/admin) are provisioned by an administrator or the seed script
        // — never chosen by the registrant. Otherwise anyone could submit
        // `role=admin` and grant themselves full access to the whole system.
        if self.role != "patient" {
            return Err(AppError::BadRequest(
                "Public registration is for patients only. Staff accounts are created by an administrator."
                    .into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub login: String,    // username or email
    pub password: String,
}

/// Form an administrator uses to create a staff (doctor/admin) account — the
/// privileged counterpart to `RegisterForm`, which is restricted to patients.
#[derive(Debug, Deserialize)]
pub struct CreateStaffForm {
    pub username: String,
    pub email: String,
    pub password: String,
    pub full_name: String,
    pub role: String, // "doctor" or "admin"
    // Doctor-only profile fields (ignored for admin).
    pub specialization: Option<String>,
    pub license_number: Option<String>,
}

impl CreateStaffForm {
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.username.trim().len() < 3 {
            return Err(AppError::BadRequest(
                "Username must be at least 3 characters".into(),
            ));
        }
        if !self.email.contains('@') {
            return Err(AppError::BadRequest(
                "A valid email address is required".into(),
            ));
        }
        if self.password.len() < 8 {
            return Err(AppError::BadRequest(
                "Password must be at least 8 characters".into(),
            ));
        }
        if self.full_name.trim().is_empty() {
            return Err(AppError::BadRequest("Full name is required".into()));
        }
        // Only staff roles may be created here — patients self-register.
        if !["doctor", "admin"].contains(&self.role.as_str()) {
            return Err(AppError::BadRequest(
                "Role must be either doctor or admin".into(),
            ));
        }
        Ok(())
    }
}

// ============================================================
// Edit profile form
// ============================================================

#[derive(Debug, Deserialize)]
pub struct EditProfileForm {
    pub full_name: String,
    pub email: String,
    // Patient fields
    pub date_of_birth: Option<String>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub blood_group: Option<String>,
    pub emergency_contact: Option<String>,
    // Doctor fields
    pub specialization: Option<String>,
    pub license_number: Option<String>,
}

impl EditProfileForm {
    /// Profile edits must satisfy the same core rules as registration:
    /// a non-empty full name and a plausible email address. Checked
    /// before anything touches the database, mirroring `RegisterForm`.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        use crate::errors::AppError;
        if self.full_name.trim().is_empty() {
            return Err(AppError::BadRequest("Full name is required".into()));
        }
        if !self.email.contains('@') {
            return Err(AppError::BadRequest(
                "A valid email address is required".into(),
            ));
        }
        Ok(())
    }
}

// ============================================================
// Patient — extended profile for patient-role users
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Patient {
    pub id: i64,
    pub user_id: i64,
    pub date_of_birth: Option<chrono::NaiveDate>,
    pub phone: Option<String>,
    pub address: Option<String>,
    pub blood_group: Option<String>,
    pub emergency_contact: Option<String>,
}

// ============================================================
// Doctor — extended profile for doctor-role users
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Doctor {
    pub id: i64,
    pub user_id: i64,
    pub specialization: String,
    pub license_number: String,
    pub phone: Option<String>,
}
